//! Spec 107, criterion 5 - THE GRAPH REBUILDS FROM LEDGER AND TREE.
//!
//! A ledger entry names a generation and where its bytes were; the batch itself is nowhere on the
//! log. These tests hold that `rigger setup`'s rebuild re-extracts each entry's batch from the
//! repository's object database, the tree's file or no bytes, in that order, and folds it through
//! the ledger fold, so the rebuilt `graph.db` is the one the incremental folds built.
//!
//! Every incremental side is folded from hand-built entries through
//! `Projection::apply_generation` with batches written out here as literal payloads, so the
//! rebuilt side equals it only when the re-extraction reproduces those payloads byte for byte. The
//! comparison is spec 101's surface: the whole live projection and the fold state.
//!
//! The resolved folds, the resolution sources, the equalities and the dating run in the default
//! lane. The light lane compiles no extraction, so there every entry folds nothing and the hole a
//! refused fold left is still paid.

mod common;

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use common::cli::{
    applied, graph_identity, init_event_log, no_progress, read_run_events, rigger_file,
    run_rigger_envs, run_stream_identity, temp_project, with_run_store,
};
use common::fixtures::{
    entry_event, events_of, folds, generation_ingested, walked_generation, write_text,
    DOCUMENT_PATH, SOURCE_PATH, TEST_MODULE_PATH, WORKFLOW_PATH,
};
use rigger::contextgraph::sqlite::{Projector, RebuildSink, Rebuilt};
use rigger::contextgraph::{wired, EntryFold, Error, Fold, Projection, TYPE_CODE_ENTITY_EXTRACTED};
use rigger::eventstore::{Event, ExpectedRevision};
use rigger::retention::{GenerationIngested, TYPE_GENERATION_INGESTED};

#[cfg(feature = "symbols")]
use common::cli::{nanos, temp_repoless_project};
#[cfg(feature = "symbols")]
use common::fixtures::{
    git_hash_object, git_ok, git_ok_with_identity, git_out, live_edges, loose_object, walked_batch,
    DOCUMENT_BODY, SOURCE_BODY, TEST_MODULE_BODY, WORKFLOW_BODY,
};
#[cfg(feature = "symbols")]
use rigger::contextgraph::{REL_CONTAINS, REL_DOC_REFERENCES, REL_SPECIFIES};

/// What `rigger setup` prints before it rebuilds a `graph.db` that misses an event the log holds.
const LOST_FOLD_REBUILD_LINE: &str = "rebuilding graph.db from the event log: it misses an event \
     the log holds, so the log's live selection is refolded once";

/// What `rigger setup` prints once its rebuild is in place.
const REBUILT_LINE: &str = "rebuilt graph.db from the event log";

/// The tail every lost fold's error ends with.
const SETUP_PAYS: &str =
    " - the next `rigger setup` finds the event missing from graph.db and rebuilds it";

/// What the public fold reports for a ledger entry, before [`SETUP_PAYS`].
const ENTRY_REFUSED: &str = "graph: a GenerationIngested entry folds only with its batch, through \
                             `Projection::apply_generation`";

/// The generation the extraction tree's fixture records for the batch under `prefix` for `path`,
/// as a graph answers the generation it holds.
fn held_generation(prefix: &str, path: &str) -> Option<String> {
    Some(walked_generation(prefix, path).to_string())
}

/// An object id no repository of these tests holds.
const NOT_HELD: &str = "1111111111111111111111111111111111111111";

/// The source file once `helper` and the call to it are gone: one product function and the
/// in-file test that references it, on line 11.
#[cfg(feature = "symbols")]
const SOURCE_WITHOUT_HELPER: &str = "\
// WHY: the entry stays small so the walk has one product file
fn product() {}

#[cfg(test)]
mod checks;

#[cfg(test)]
mod inline {
    #[test]
    fn it_works() {
        product();
    }
}
";

/// The `gc` batch [`SOURCE_WITHOUT_HELPER`] extracts to at `path`.
#[cfg(feature = "symbols")]
fn source_without_helper_batch(path: &str) -> Vec<Event> {
    events_of(&[
        (
            "CodeEntityExtracted",
            &format!(
                r#"{{"file":"{path}","name":"product","kind":"function","line":2,"lang":"rust","fresh":true}}"#
            ),
        ),
        (
            "EdgeInferred",
            &format!(
                r#"{{"file":"{path}","name":"product","lang":"rust","fresh":true,"line":11,"is_test":true}}"#
            ),
        ),
    ])
}

/// The design document once its citation of the handbook is gone.
#[cfg(feature = "symbols")]
const DOCUMENT_WITHOUT_CITATION: &str = "\
# Reference architecture

## The walk

The entry is `src/lib.rs`.
";

/// The `gd` batch [`DOCUMENT_WITHOUT_CITATION`] extracts to: the fixture's batch for the document
/// without its last event, the `references` link.
#[cfg(feature = "symbols")]
fn document_without_citation_batch() -> Vec<Event> {
    let whole = walked_batch("gd", DOCUMENT_PATH);
    events_of(&whole[..whole.len() - 1])
}

/// A source file the tree later loses, and the `gc` batch it extracts to.
#[cfg(feature = "symbols")]
const GONE_PATH: &str = "src/gone.rs";
#[cfg(feature = "symbols")]
const GONE_BODY: &str = "fn gone() {}\n";
#[cfg(feature = "symbols")]
const GONE_BATCH: [(&str, &str); 2] = [
    (
        "CodeEntityExtracted",
        r#"{"file":"src/gone.rs","name":"gone","kind":"function","line":1,"lang":"rust","fresh":true}"#,
    ),
    (
        "EdgeInferred",
        r#"{"file":"src/gone.rs","name":"","lang":"rust","fresh":true,"is_test":true}"#,
    ),
];

/// The `gc` batch of [`GONE_PATH`] for no bytes: the one boundary event of a path that holds no
/// file.
#[cfg(feature = "symbols")]
const GONE_NO_BYTES_BATCH: [(&str, &str); 1] = [(
    "EdgeInferred",
    r#"{"file":"src/gone.rs","name":"","lang":"unknown","fresh":true}"#,
)];

/// One recording of a generation: what its entry names, when it was recorded, and the batch the
/// recording process extracted - `None` for a recording whose process resolved nothing.
#[derive(Clone)]
struct Recording {
    named: GenerationIngested,
    secs: u64,
    batch: Option<Vec<Event>>,
}

/// The recording of `batch` under `prefix` for `file`, extracted from `blob` under the walk's
/// flag `excluded` at `secs`, its generation the batch's own.
#[cfg(feature = "symbols")]
fn recording(
    prefix: &str,
    file: &str,
    blob: &str,
    excluded: bool,
    secs: u64,
    batch: Vec<Event>,
) -> Recording {
    Recording {
        named: generation_ingested(
            prefix,
            file,
            &rigger::ingest::batch_generation(&batch),
            blob,
            excluded,
        ),
        secs,
        batch: Some(batch),
    }
}

/// `rigger setup` in `cwd`: (stdout, stderr, success).
fn setup(cwd: &Path) -> (String, String, bool) {
    run_rigger_envs(cwd, &["setup"], &[("RIGGER_NPM", "true")])
}

/// `rigger setup` in `cwd`, which must succeed: the lines it prints about its rebuild.
fn setup_rebuild_lines(cwd: &Path) -> Vec<String> {
    let (out, err, ok) = setup(cwd);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    out.lines()
        .filter(|l| {
            l.starts_with("rebuilding graph.db")
                || l.starts_with("rebuilt graph.db")
                || l.starts_with("passed over ")
        })
        .map(str::to_string)
        .collect()
}

/// Give the directory `cwd` an event log, and let `rigger setup` settle its scaffold and its
/// identity before anything is recorded: the project identity the binary now resolves there.
fn settled(cwd: &Path) -> String {
    init_event_log(cwd);
    assert_eq!(
        setup_rebuild_lines(cwd),
        Vec::<String>::new(),
        "a project with no graph.db owes no rebuild"
    );
    run_stream_identity(cwd)
}

/// Append `event` to `cwd`'s run stream by a plain append, behind every fold, and answer its log
/// position.
fn append_unfolded(cwd: &Path, event: Event) -> u64 {
    with_run_store(cwd, |store| {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &[event])
            .unwrap()
            .one("the unfolded event")
            .unwrap()
    })
}

/// The ledger entry of `recording`, as its recording process appends it.
fn entry_of(recording: &Recording) -> Event {
    entry_event(
        &recording.named,
        recording.batch.as_ref().map_or(1, Vec::len),
    )
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(recording.secs))
}

/// Append the entry of each of `recordings` to `cwd`'s run stream, in order, and answer the
/// position of each.
fn record(cwd: &Path, recordings: &[Recording]) -> Vec<u64> {
    recordings
        .iter()
        .map(|recording| append_unfolded(cwd, entry_of(recording)))
        .collect()
}

/// Fold `cwd`'s whole run stream into the graph file `db` under `project` as the recording
/// processes folded it, the projector reopened for every event: each ledger entry through
/// `Projection::apply_generation` with the batch of its recording (`recordings` holds one per
/// entry, in log order), every other event through the public fold. Answers each entry's outcome.
fn fold_incrementally(
    db: &Path,
    project: &str,
    cwd: &Path,
    recordings: &[Recording],
) -> Vec<EntryFold> {
    let open = || Projector::open(db.to_str().unwrap(), project).unwrap();
    let mut batches = recordings.iter().map(|recording| recording.batch.clone());
    let mut outcomes = Vec::new();
    for event in read_run_events(cwd) {
        if event.type_ == TYPE_GENERATION_INGESTED {
            let batch = batches.next().expect("one recording per entry of the log");
            let port: &dyn Projection = &open();
            outcomes.push(
                port.apply_generation(&event, Box::new(move || Ok(batch)))
                    .unwrap(),
            );
        } else {
            folds(&open(), std::slice::from_ref(&event));
        }
    }
    assert!(batches.next().is_none(), "one entry per recording");
    outcomes
}

/// Make sure `cwd`'s `graph.db` stands, as a command that opened it left it: a file whose ledger
/// misses whatever the log gained by a plain append.
fn stand_graph(cwd: &Path, project: &str) {
    drop(Projector::open(rigger_file(cwd, "graph.db").to_str().unwrap(), project).unwrap());
}

/// The generation `cwd`'s `graph.db` holds for each of `identities`, in order.
fn generations(cwd: &Path, project: &str, identities: &[&str]) -> Vec<Option<String>> {
    let graph = Projector::open(rigger_file(cwd, "graph.db").to_str().unwrap(), project).unwrap();
    identities
        .iter()
        .map(|identity| graph.current_generation(identity).unwrap())
        .collect()
}

/// The graph file `db`, opened under `project`.
#[cfg(feature = "symbols")]
fn open(db: &Path, project: &str) -> Projector {
    Projector::open(db.to_str().unwrap(), project).unwrap()
}

/// The live edges of [`live_edges`] that touch `node`, in order.
#[cfg(feature = "symbols")]
fn edges_touching(
    edges: &[(String, String, String, i64, u64)],
    node: &str,
) -> Vec<(String, String, String, i64, u64)> {
    edges
        .iter()
        .filter(|(from, _, to, _, _)| from == node || to == node)
        .cloned()
        .collect()
}

/// Commit the `src` and `docs` directories of the repository at `root`, and nothing else.
#[cfg(feature = "symbols")]
fn commit_sources(root: &Path) {
    git_ok(root, &["add", "src", "docs"]);
    git_ok_with_identity(root, &["commit", "-q", "-m", "sources"]);
}

/// Given a repository whose history holds the first generation of a source file and a design
/// document and a source file since deleted, whose tree holds their second generations, an
/// out-of-line test module and the workflow definition, and a log of entries one process recorded
/// that all resolve, when the operator runs `rigger setup`, then the rebuilt `graph.db` equals the
/// one the incremental folds built: the superseded generations resolved from their blobs, the
/// entry whose blob git does not hold from the tree's file, the workflow definition's from the
/// tree's `.rigger/workflow.yml`, and the deleted file's entry with no blob from no bytes.
#[cfg(feature = "symbols")]
#[test]
fn setup_rebuilds_from_blobs_tree_files_and_no_bytes_the_graph_the_incremental_folds_built() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, TEST_MODULE_PATH, TEST_MODULE_BODY);
    write_text(root, DOCUMENT_PATH, DOCUMENT_BODY);
    write_text(root, GONE_PATH, GONE_BODY);
    commit_sources(root);
    let first_source = git_hash_object(root, SOURCE_PATH, false);
    let test_module = git_hash_object(root, TEST_MODULE_PATH, false);
    let first_document = git_hash_object(root, DOCUMENT_PATH, false);
    let gone = git_hash_object(root, GONE_PATH, false);
    let project = settled(root);

    // The tree moves on: the source file drops `helper` and is not written to the object
    // database, the document drops its citation, the third source file is deleted, and the
    // workflow definition is written, uncommitted.
    write_text(root, SOURCE_PATH, SOURCE_WITHOUT_HELPER);
    let second_source = git_hash_object(root, SOURCE_PATH, false);
    write_text(root, DOCUMENT_PATH, DOCUMENT_WITHOUT_CITATION);
    let second_document = git_hash_object(root, DOCUMENT_PATH, true);
    std::fs::remove_file(root.join(GONE_PATH)).unwrap();
    write_text(root, WORKFLOW_PATH, WORKFLOW_BODY);
    let workflow = git_hash_object(root, WORKFLOW_PATH, false);

    let walked = |prefix, path| events_of(walked_batch(prefix, path));
    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            &first_source,
            false,
            10,
            walked("gc", SOURCE_PATH),
        ),
        recording(
            "gc",
            TEST_MODULE_PATH,
            &test_module,
            true,
            11,
            walked("gc", TEST_MODULE_PATH),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            &first_document,
            false,
            12,
            walked("gd", DOCUMENT_PATH),
        ),
        recording(
            "gd",
            SOURCE_PATH,
            &first_source,
            false,
            13,
            walked("gd", SOURCE_PATH),
        ),
        recording(
            "gw",
            WORKFLOW_PATH,
            &workflow,
            false,
            14,
            walked("gw", WORKFLOW_PATH),
        ),
        recording("gc", GONE_PATH, &gone, false, 15, events_of(&GONE_BATCH)),
        recording(
            "gc",
            SOURCE_PATH,
            &second_source,
            false,
            20,
            source_without_helper_batch(SOURCE_PATH),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            &second_document,
            false,
            21,
            document_without_citation_batch(),
        ),
        recording(
            "gc",
            GONE_PATH,
            "",
            false,
            22,
            events_of(&GONE_NO_BYTES_BATCH),
        ),
    ];
    // The fixture's recorded generations are the ones these hand-built batches hash to.
    assert_eq!(
        recordings[..5]
            .iter()
            .map(|r| r.named.generation.as_str())
            .collect::<Vec<_>>(),
        vec![
            walked_generation("gc", SOURCE_PATH),
            walked_generation("gc", TEST_MODULE_PATH),
            walked_generation("gd", DOCUMENT_PATH),
            walked_generation("gd", SOURCE_PATH),
            walked_generation("gw", WORKFLOW_PATH),
        ]
    );
    let positions = record(root, &recordings);
    let scratch = tempfile::tempdir().unwrap();
    let incremental = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&incremental, &project, root, &recordings),
        vec![EntryFold::BatchAsked; 9]
    );
    stand_graph(root, &project);

    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE],
        "the rebuild folds every entry and passes none over"
    );

    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        graph_identity(&graph_db, &project),
        graph_identity(&incremental, &project),
        "the rebuilt graph.db is the incremental one, fold state included"
    );
    assert_eq!(
        generations(
            root,
            &project,
            &[
                "gc/src/lib.rs",
                "gc/src/checks.rs",
                "gd/docs/architecture.md",
                "gd/src/lib.rs",
                "gw/.rigger/workflow.yml",
                "gc/src/gone.rs",
            ]
        ),
        vec![
            Some(recordings[6].named.generation.clone()),
            held_generation("gc", TEST_MODULE_PATH),
            Some(recordings[7].named.generation.clone()),
            held_generation("gd", SOURCE_PATH),
            held_generation("gw", WORKFLOW_PATH),
            Some(recordings[8].named.generation.clone()),
        ],
        "every identity holds the generation of its latest entry"
    );
    let edges = live_edges(&open(&graph_db, &project));
    assert_eq!(
        edges
            .iter()
            .filter(|(from, rel, ..)| from == SOURCE_PATH && rel == REL_CONTAINS)
            .cloned()
            .collect::<Vec<_>>(),
        vec![(
            SOURCE_PATH.to_string(),
            REL_CONTAINS.to_string(),
            "src/lib.rs::product".to_string(),
            nanos(20),
            positions[6],
        )],
        "a code structural edge is dated at, and sourced from, the latest entry that folded its \
         file's batch, and the dropped entity's edge is gone"
    );
    assert_eq!(
        edges
            .iter()
            .filter(|(from, rel, ..)| {
                from == DOCUMENT_PATH && (rel == REL_SPECIFIES || rel == REL_DOC_REFERENCES)
            })
            .cloned()
            .collect::<Vec<_>>(),
        vec![(
            DOCUMENT_PATH.to_string(),
            REL_SPECIFIES.to_string(),
            SOURCE_PATH.to_string(),
            nanos(12),
            positions[7],
        )],
        "the design link both generations assert keeps its first valid-time and its newest \
         recording, and the dropped link is gone"
    );
    assert_eq!(
        edges_touching(&edges, "src/gone.rs::gone"),
        Vec::new(),
        "the deleted file's entry for no bytes retired its facts"
    );
    assert_eq!(
        edges_touching(&edges, "stage:implement")
            .iter()
            .map(|(from, rel, to, at, source)| (
                from.as_str(),
                rel.as_str(),
                to.as_str(),
                *at,
                *source
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "stage:implement",
                "RUNS",
                "agent:rust-engineer",
                nanos(14),
                positions[4]
            ),
            (
                "stage:implement",
                "RUNS",
                "gate:fmt",
                nanos(14),
                positions[4]
            ),
        ],
        "the workflow definition's entry resolved from the tree's file"
    );

    assert_eq!(
        setup_rebuild_lines(root),
        Vec::<String>::new(),
        "a paid rebuild is not owed again"
    );
}

/// Given a tree that is not a git repository, whose entries name blobs no object database holds,
/// when the operator runs `rigger setup`, then every entry resolves from the tree's files and the
/// rebuilt `graph.db` equals the incremental one.
#[cfg(feature = "symbols")]
#[test]
fn setup_outside_a_repository_resolves_every_entry_from_the_trees_files() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, TEST_MODULE_PATH, TEST_MODULE_BODY);
    write_text(root, DOCUMENT_PATH, DOCUMENT_BODY);
    let project = settled(root);
    write_text(root, WORKFLOW_PATH, WORKFLOW_BODY);

    let walked = |prefix, path| events_of(walked_batch(prefix, path));
    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            NOT_HELD,
            false,
            10,
            walked("gc", SOURCE_PATH),
        ),
        recording(
            "gc",
            TEST_MODULE_PATH,
            NOT_HELD,
            true,
            11,
            walked("gc", TEST_MODULE_PATH),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            NOT_HELD,
            false,
            12,
            walked("gd", DOCUMENT_PATH),
        ),
        recording(
            "gw",
            WORKFLOW_PATH,
            NOT_HELD,
            false,
            13,
            walked("gw", WORKFLOW_PATH),
        ),
        recording(
            "gc",
            GONE_PATH,
            "",
            false,
            14,
            events_of(&GONE_NO_BYTES_BATCH),
        ),
    ];
    record(root, &recordings);
    let scratch = tempfile::tempdir().unwrap();
    let incremental = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&incremental, &project, root, &recordings),
        vec![EntryFold::BatchAsked; 5]
    );
    stand_graph(root, &project);

    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE]
    );
    assert_eq!(
        graph_identity(&rigger_file(root, "graph.db"), &project),
        graph_identity(&incremental, &project)
    );
    assert_eq!(
        generations(
            root,
            &project,
            &[
                "gc/src/lib.rs",
                "gc/src/checks.rs",
                "gd/docs/architecture.md",
                "gw/.rigger/workflow.yml",
                "gc/src/gone.rs",
            ]
        ),
        vec![
            held_generation("gc", SOURCE_PATH),
            held_generation("gc", TEST_MODULE_PATH),
            held_generation("gd", DOCUMENT_PATH),
            held_generation("gw", WORKFLOW_PATH),
            Some(recordings[4].named.generation.clone()),
        ]
    );
}

/// Given a store whose `.rigger/` sits in a subdirectory of a repository, the subdirectory
/// holding a different file at the path an entry names, when the operator runs `rigger setup` in
/// that subdirectory, then each entry's path is read from the repository's top level: the entry
/// naming the top level's file resolves, the entry naming the subdirectory's file by its path
/// from the top level resolves, and the entry naming it by its path from the working directory
/// resolves nothing.
#[cfg(feature = "symbols")]
#[test]
fn setup_in_a_repository_subdirectory_resolves_from_the_top_level_not_the_working_directory() {
    let dir = temp_project();
    let top = dir.path();
    let sub = top.join("sub");
    write_text(top, SOURCE_PATH, SOURCE_BODY);
    write_text(&sub, SOURCE_PATH, SOURCE_WITHOUT_HELPER);
    write_text(&sub, "src/only.rs", GONE_BODY);
    let project = settled(&sub);

    let from_top = "sub/src/lib.rs";
    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            NOT_HELD,
            false,
            10,
            events_of(walked_batch("gc", SOURCE_PATH)),
        ),
        recording(
            "gc",
            from_top,
            NOT_HELD,
            false,
            11,
            source_without_helper_batch(from_top),
        ),
        // The subdirectory's own file, named as the working directory would name it: the batch
        // its recording process would have extracted had its root been the working directory.
        recording(
            "gc",
            "src/only.rs",
            NOT_HELD,
            false,
            12,
            events_of(&[
                (
                    "CodeEntityExtracted",
                    r#"{"file":"src/only.rs","name":"gone","kind":"function","line":1,"lang":"rust","fresh":true}"#,
                ),
                (
                    "EdgeInferred",
                    r#"{"file":"src/only.rs","name":"","lang":"rust","fresh":true,"is_test":true}"#,
                ),
            ]),
        ),
    ];
    record(&sub, &recordings);
    let resolving = [
        Recording {
            batch: recordings[0].batch.clone(),
            named: recordings[0].named.clone(),
            secs: 10,
        },
        Recording {
            batch: recordings[1].batch.clone(),
            named: recordings[1].named.clone(),
            secs: 11,
        },
        Recording {
            batch: None,
            named: recordings[2].named.clone(),
            secs: 12,
        },
    ];
    let scratch = tempfile::tempdir().unwrap();
    let incremental = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&incremental, &project, &sub, &resolving),
        vec![EntryFold::BatchAsked; 3]
    );
    stand_graph(&sub, &project);

    assert_eq!(
        setup_rebuild_lines(&sub),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE]
    );
    assert_eq!(
        generations(
            &sub,
            &project,
            &["gc/src/lib.rs", "gc/sub/src/lib.rs", "gc/src/only.rs"]
        ),
        vec![
            held_generation("gc", SOURCE_PATH),
            Some(recordings[1].named.generation.clone()),
            None,
        ],
        "the path `src/lib.rs` is the top level's file, and `src/only.rs` holds no file there"
    );
    assert_eq!(
        graph_identity(&rigger_file(&sub, "graph.db"), &project),
        graph_identity(&incremental, &project)
    );
}

/// The live facts of the graph file `db` that name nothing under `without`: every node id, and
/// every live edge as `(from, rel, to)`, sorted.
#[cfg(feature = "symbols")]
fn live_facts_without(
    db: &Path,
    project: &str,
    without: &str,
) -> (Vec<String>, Vec<(String, String, String)>) {
    let whole = Projector::open(db.to_str().unwrap(), project)
        .unwrap()
        .whole()
        .unwrap();
    let mut nodes: Vec<String> = whole
        .nodes
        .into_iter()
        .map(|n| n.id)
        .filter(|id| !id.contains(without))
        .collect();
    nodes.sort();
    let mut edges: Vec<(String, String, String)> = whole
        .edges
        .into_iter()
        .filter(|e| !e.from.contains(without) && !e.to.contains(without))
        .map(|e| (e.from, e.rel, e.to))
        .collect();
    edges.sort();
    (nodes, edges)
}

/// Given a log holding entries no source resolves - a superseded generation of a design document
/// between two recordings of the generation the repository holds, and the latest generation of a
/// source file - when the operator runs `rigger setup`, then the rebuild completes; every
/// identity whose latest entry resolves holds the live facts and the current generation the
/// incremental folds reached; the superseded unresolved entry folded nothing, so the link it
/// dropped keeps the valid-time of the first recording where the incremental graph dated it at
/// the return; and the identity whose latest entry does not resolve holds what its latest
/// resolvable entry gave.
#[cfg(feature = "symbols")]
#[test]
fn a_rebuild_over_an_entry_no_source_resolves_reaches_the_facts_of_every_latest_entry_that_does() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, DOCUMENT_PATH, DOCUMENT_BODY);
    write_text(root, GONE_PATH, GONE_BODY);
    let project = settled(root);

    let walked = |prefix, path| events_of(walked_batch(prefix, path));
    let recordings = [
        recording(
            "gd",
            DOCUMENT_PATH,
            NOT_HELD,
            false,
            10,
            walked("gd", DOCUMENT_PATH),
        ),
        recording(
            "gc",
            SOURCE_PATH,
            NOT_HELD,
            false,
            11,
            walked("gc", SOURCE_PATH),
        ),
        recording("gc", GONE_PATH, NOT_HELD, false, 12, events_of(&GONE_BATCH)),
        // No source holds the document without its citation: the tree's file still cites.
        recording(
            "gd",
            DOCUMENT_PATH,
            NOT_HELD,
            false,
            20,
            document_without_citation_batch(),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            NOT_HELD,
            false,
            30,
            walked("gd", DOCUMENT_PATH),
        ),
        // No source holds this generation of the source file either: the tree's file defines
        // `gone`, not `other`.
        recording(
            "gc",
            GONE_PATH,
            NOT_HELD,
            false,
            31,
            events_of(&[(
                "CodeEntityExtracted",
                r#"{"file":"src/gone.rs","name":"other","kind":"function","line":1,"lang":"rust","fresh":true}"#,
            )]),
        ),
    ];
    let positions = record(root, &recordings);
    let scratch = tempfile::tempdir().unwrap();
    let incremental = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&incremental, &project, root, &recordings),
        vec![EntryFold::BatchAsked; 6]
    );
    stand_graph(root, &project);

    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE],
        "an unresolved entry is folded, not passed over"
    );

    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        live_facts_without(&graph_db, &project, GONE_PATH),
        live_facts_without(&incremental, &project, GONE_PATH),
        "the identities whose latest entry resolves hold the incremental graph's live facts"
    );
    assert_eq!(
        generations(
            root,
            &project,
            &["gd/docs/architecture.md", "gc/src/lib.rs", "gc/src/gone.rs"]
        ),
        vec![
            held_generation("gd", DOCUMENT_PATH),
            held_generation("gc", SOURCE_PATH),
            Some(recordings[2].named.generation.clone()),
        ],
        "the last identity holds the generation of its latest resolvable entry"
    );
    let cited = |db: &Path| {
        live_edges(&open(db, &project))
            .into_iter()
            .filter(|(from, rel, ..)| from == DOCUMENT_PATH && rel == REL_DOC_REFERENCES)
            .collect::<Vec<_>>()
    };
    let citation = |secs: u64, position: u64| {
        vec![(
            DOCUMENT_PATH.to_string(),
            REL_DOC_REFERENCES.to_string(),
            "docs/handbook.md".to_string(),
            nanos(secs),
            position,
        )]
    };
    assert_eq!(
        (cited(&graph_db), cited(&incremental)),
        (citation(10, positions[0]), citation(30, positions[4])),
        "the superseded unresolved entry folded nothing: the rebuild never retired the link it \
         dropped, and the later entry of the held generation is a re-recording there"
    );
    assert_eq!(
        edges_touching(&live_edges(&open(&graph_db, &project)), "src/gone.rs::gone"),
        vec![(
            GONE_PATH.to_string(),
            REL_CONTAINS.to_string(),
            "src/gone.rs::gone".to_string(),
            nanos(12),
            positions[2],
        )],
        "the identity whose latest entry does not resolve keeps its latest resolvable entry's \
         facts"
    );
    assert_eq!(
        edges_touching(
            &live_edges(&open(&graph_db, &project)),
            "src/gone.rs::other"
        ),
        Vec::new()
    );
    assert!(
        positions
            .iter()
            .all(|position| applied(&graph_db, *position)),
        "every entry's position is recorded, resolved or not"
    );
}

/// A rebuild source over `log`, in batches of one event.
fn one_by_one(log: &[Event]) -> impl FnMut(u64, &mut RebuildSink) -> Result<(), Error> + '_ {
    move |after, sink| rigger::contextgraph::sqlite::stream_past(log, after, 1, sink)
}

/// Rebuild the graph file `db` under `project` from `log`, one event to a committed batch,
/// re-extracting each entry through `ingest::resolve_entry` over `root` and a batch process
/// started for this pass, as `rigger setup` binds them.
#[cfg(feature = "symbols")]
fn rebuild_resolving(
    db: &Path,
    project: &str,
    log: &[Event],
    root: &Path,
) -> Result<Option<Rebuilt>, String> {
    let mut blobs = rigger::worktree::BlobBatch::start(root);
    assert!(blobs.is_some(), "the tree is a repository");
    Projector::rebuild(
        &Projector::lock_rebuild(db.to_str().unwrap()).unwrap(),
        project,
        true,
        &mut one_by_one(log),
        &mut |entry| {
            let mut source = |id: &str| blobs.as_mut().unwrap().blob(id).map_err(|e| Error(e.0));
            rigger::ingest::resolve_entry(root, entry, Some(&mut source))
        },
        &mut no_progress,
    )
    .map_err(|e| e.0)
}

/// Given a log whose second entry names a loose object whose body is truncated, when a rebuild
/// folds the log a batch at a time, then the batch process dies answering that object and the
/// rebuild fails naming the object and its remedy, with the first entry's batch committed to the
/// shadow and `graph.db` untouched; and once the object is restored, the resumed pass reaches the
/// graph a single pass over an untouched repository reaches. Through the binary, `rigger setup`
/// fails the same way, changes nothing, and succeeds once the object is restored.
#[cfg(feature = "symbols")]
#[test]
fn a_batch_process_that_dies_mid_pass_fails_the_rebuild_and_the_resumed_pass_equals_a_single_one() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, DOCUMENT_PATH, DOCUMENT_BODY);
    let first_source = git_hash_object(root, SOURCE_PATH, true);
    let first_document = git_hash_object(root, DOCUMENT_PATH, true);
    let project = settled(root);
    // The tree moves on, so both first generations resolve from the object database alone.
    write_text(root, SOURCE_PATH, SOURCE_WITHOUT_HELPER);
    write_text(root, DOCUMENT_PATH, DOCUMENT_WITHOUT_CITATION);

    let walked = |prefix, path| events_of(walked_batch(prefix, path));
    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            &first_source,
            false,
            10,
            walked("gc", SOURCE_PATH),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            &first_document,
            false,
            11,
            walked("gd", DOCUMENT_PATH),
        ),
        recording(
            "gc",
            SOURCE_PATH,
            NOT_HELD,
            false,
            12,
            source_without_helper_batch(SOURCE_PATH),
        ),
    ];
    let positions = record(root, &recordings);
    let log = read_run_events(root);
    let scratch = tempfile::tempdir().unwrap();
    let (single, resumed) = (
        scratch.path().join("single.db"),
        scratch.path().join("resumed.db"),
    );
    for db in [&single, &resumed] {
        drop(Projector::open(db.to_str().unwrap(), &project).unwrap());
    }
    let untouched = Rebuilt::default();
    assert_eq!(
        rebuild_resolving(&single, &project, &log, root),
        Ok(Some(untouched)),
        "a single pass over the untouched repository"
    );

    let object = loose_object(root, &first_document);
    let whole = std::fs::read(&object).unwrap();
    std::fs::write(&object, &whole[..whole.len() - 6]).unwrap();
    let died = format!(
        "git cat-file --batch stopped while answering object {first_document}: restore the \
         object or remove it, after which git answers it missing and its entry resolves from \
         the tree"
    );
    let before = graph_identity(&resumed, &project);
    assert_eq!(
        rebuild_resolving(&resumed, &project, &log, root),
        Err(died.clone())
    );
    let shadow = scratch.path().join("resumed.db.rebuild");
    assert_eq!(
        (
            graph_identity(&resumed, &project),
            applied(&shadow, positions[0]),
            applied(&shadow, positions[1]),
        ),
        (before, true, false),
        "the failed pass left graph.db untouched and its shadow holding the batch before the \
         one that failed"
    );

    // Through the binary: setup fails naming the object, and leaves graph.db as it stood.
    stand_graph(root, &project);
    let graph_db = rigger_file(root, "graph.db");
    let stood = graph_identity(&graph_db, &project);
    let (out, err, ok) = setup(root);
    let refused = format!("rigger: graph: event store: {died}");
    assert_eq!(
        (
            ok,
            err.lines().last(),
            out.lines().any(|l| l == REBUILT_LINE),
            graph_identity(&graph_db, &project),
        ),
        (false, Some(refused.as_str()), false, stood),
        "stdout: {out} stderr: {err}"
    );

    std::fs::write(&object, &whole).unwrap();
    assert_eq!(
        rebuild_resolving(&resumed, &project, &log, root),
        Ok(Some(untouched))
    );
    assert_eq!(
        graph_identity(&resumed, &project),
        graph_identity(&single, &project),
        "the pass resumed once the object is restored equals a single pass"
    );
    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE]
    );
    assert_eq!(
        graph_identity(&graph_db, &project),
        graph_identity(&single, &project),
        "and so does the rebuild `rigger setup` resumed"
    );
}

/// The entry of `gc/src/lib.rs` at the fixture's generation, as its recording process appends it.
fn source_entry() -> Event {
    entry_of(&Recording {
        named: generation_ingested(
            "gc",
            SOURCE_PATH,
            walked_generation("gc", SOURCE_PATH),
            "",
            false,
        ),
        secs: 10,
        batch: None,
    })
}

/// Given a `graph.db` that owes nothing, and an entry appended through a plain append and handed
/// to the generic fold, which refuses it and marks the graph as owing its rebuild, when the
/// operator runs `rigger setup`, then the rebuild pays the hole through the ledger fold: the
/// entry's position is recorded, and a second setup owes nothing. Where an extraction is
/// compiled, the entry's batch is folded from the tree's file; where none is, it folds nothing.
#[test]
fn an_entry_the_generic_fold_refused_is_paid_by_setup_through_the_ledger_fold() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, common::fixtures::SOURCE_BODY);
    let project = settled(root);
    stand_graph(root, &project);
    let graph_db = rigger_file(root, "graph.db");

    let mut entry = source_entry();
    entry.position = append_unfolded(root, entry.clone());
    {
        let graph = Projector::open(graph_db.to_str().unwrap(), &project).unwrap();
        assert_eq!(
            Fold::of(wired(Some(&graph as &dyn Projection)), &entry),
            Fold::NotFolded(format!("{ENTRY_REFUSED}{SETUP_PAYS}"))
        );
    }
    assert_eq!(
        (
            rigger_file(root, "graph.db.owed").exists(),
            applied(&graph_db, entry.position)
        ),
        (true, false),
        "the refusal marks the graph owed and records no position"
    );

    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE]
    );
    assert_eq!(
        (
            rigger_file(root, "graph.db.owed").exists(),
            applied(&graph_db, entry.position)
        ),
        (false, true),
        "the rebuild folded the entry through the ledger fold and dropped the mark"
    );
    #[cfg(feature = "symbols")]
    let held = held_generation("gc", SOURCE_PATH);
    #[cfg(not(feature = "symbols"))]
    let held = None;
    assert_eq!(generations(root, &project, &["gc/src/lib.rs"]), vec![held]);
    assert_eq!(
        setup_rebuild_lines(root),
        Vec::<String>::new(),
        "a paid hole is not owed again"
    );
}

/// Given entries on the log that no fold reached, in the lane that compiles no extraction, when
/// the operator runs `rigger setup`, then each entry folds nothing - no generation, no fact - its
/// position is recorded all the same, and a second setup owes nothing.
#[cfg(not(feature = "symbols"))]
#[test]
fn without_an_extraction_every_entry_folds_nothing_and_its_position_is_recorded() {
    use common::fixtures::{planted_extraction_tree, write_file};

    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    let _ = common::git::run_git(root, &["init", "-q"]);
    let project = settled(root);
    let recordings = [
        ("gc", SOURCE_PATH, false),
        ("gc", TEST_MODULE_PATH, true),
        ("gd", DOCUMENT_PATH, false),
        ("gw", WORKFLOW_PATH, false),
    ]
    .map(|(prefix, file, excluded)| Recording {
        named: generation_ingested(prefix, file, walked_generation(prefix, file), "", excluded),
        secs: 10,
        batch: None,
    });
    let positions = record(root, &recordings);
    let scratch = tempfile::tempdir().unwrap();
    let nothing_folded = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&nothing_folded, &project, root, &recordings),
        vec![EntryFold::BatchAsked; 4]
    );
    stand_graph(root, &project);

    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE]
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        (
            generations(
                root,
                &project,
                &[
                    "gc/src/lib.rs",
                    "gc/src/checks.rs",
                    "gd/docs/architecture.md",
                    "gw/.rigger/workflow.yml",
                ]
            ),
            positions
                .iter()
                .map(|position| applied(&graph_db, *position))
                .collect::<Vec<_>>(),
        ),
        (vec![None, None, None, None], vec![true; 4])
    );
    assert_eq!(
        graph_identity(&graph_db, &project),
        graph_identity(&nothing_folded, &project),
        "the rebuilt graph holds what a graph every entry folded nothing into holds"
    );
    assert_eq!(setup_rebuild_lines(root), Vec::<String>::new());
}

/// Given an entry on the log whose payload does not parse, beside one whose payload does, when
/// the operator runs `rigger setup`, then the rebuild completes, says it passed the one event
/// over, and records both positions, so the next setup owes nothing.
#[test]
fn an_entry_whose_payload_does_not_parse_is_passed_over_by_a_rebuild_that_completes() {
    let dir = temp_project();
    let root = dir.path();
    let project = settled(root);
    stand_graph(root, &project);
    let unparsable = append_unfolded(root, Event::new(TYPE_GENERATION_INGESTED, b"{}".to_vec()));
    let parsable = append_unfolded(root, source_entry());

    assert_eq!(
        setup_rebuild_lines(root),
        vec![
            LOST_FOLD_REBUILD_LINE,
            REBUILT_LINE,
            "passed over 1 event(s) whose payload the fold rejects, recorded as folded",
        ]
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        [unparsable, parsable].map(|position| applied(&graph_db, position)),
        [true, true]
    );
    assert_eq!(
        generations(root, &project, &["gc/src/lib.rs"]),
        vec![None],
        "the tree holds no file at the parsable entry's path, so it folded nothing"
    );
    assert_eq!(setup_rebuild_lines(root), Vec::<String>::new());
}

/// `Projector::rebuild` handed a re-extracting function that answers a batch event whose payload
/// the fold rejects fails with the fold's error, rather than passing the entry over: it records no
/// position and leaves `graph.db` as it stood. An entry whose own payload does not parse is passed
/// over without the function being asked, and the same log rebuilt with a function whose batch
/// folds completes.
#[test]
fn a_rebuild_whose_re_extracted_batch_the_fold_rejects_fails_rather_than_passing_it_over() {
    const PROJECT: &str = "proj-ledger";
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    drop(Projector::open(db.to_str().unwrap(), PROJECT).unwrap());
    let mut unparsable = Event::new(TYPE_GENERATION_INGESTED, b"{}".to_vec());
    unparsable.position = 1;
    let mut entry = source_entry();
    entry.position = 2;
    let log = [unparsable, entry];

    let poison: &[u8] = b"{ not valid json";
    let rejected = serde_json::from_slice::<serde_json::Value>(poison)
        .unwrap_err()
        .to_string();
    let asked = std::cell::RefCell::new(Vec::new());
    let rebuild = |batch: Vec<Event>| {
        Projector::rebuild(
            &Projector::lock_rebuild(db.to_str().unwrap()).unwrap(),
            PROJECT,
            true,
            &mut one_by_one(&log),
            &mut |entry| {
                asked.borrow_mut().push(entry.position);
                Ok(Some(batch.clone()))
            },
            &mut no_progress,
        )
        .map_err(|e| e.0)
    };
    let before = graph_identity(&db, PROJECT);

    assert_eq!(
        rebuild(vec![Event::new(
            TYPE_CODE_ENTITY_EXTRACTED,
            poison.to_vec()
        )]),
        Err(rejected)
    );
    let shadow = dir.path().join("graph.db.rebuild");
    assert_eq!(
        (
            asked.borrow().clone(),
            graph_identity(&db, PROJECT),
            applied(&shadow, 1),
            applied(&shadow, 2),
        ),
        (vec![2], before, true, false),
        "the unparsable entry was passed over unasked, the rejected batch failed the rebuild, \
         and nothing recorded the entry folded"
    );

    assert_eq!(
        rebuild(events_of(&[(
            "CodeEntityExtracted",
            r#"{"file":"src/lib.rs","name":"alpha","kind":"function","line":1,"lang":"rust","fresh":true}"#,
        )])),
        Ok(Some(Rebuilt::default())),
        "the resumed rebuild folds the entry, and passes over nothing more"
    );
    assert_eq!(asked.borrow().clone(), vec![2, 2]);
    let graph = Projector::open(db.to_str().unwrap(), PROJECT).unwrap();
    assert_eq!(
        (
            graph.current_generation("gc/src/lib.rs").unwrap(),
            applied(&db, 1),
            applied(&db, 2),
        ),
        (held_generation("gc", SOURCE_PATH), true, true)
    );
}

/// `rigger setup` in `cwd`, which must succeed, with a recording `git` first on PATH: every
/// `git` invocation it made, as the argument line of each without its leading `-C <directory>`.
fn git_invocations_of_setup(cwd: &Path) -> Vec<String> {
    let work = tempfile::tempdir().unwrap();
    let path = common::repo::stub_path(work.path(), "git", Some("git-recording.sh"));
    let (out, err, ok) = run_rigger_envs(
        cwd,
        &["setup"],
        &[("RIGGER_NPM", "true"), ("PATH", path.as_str())],
    );
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    std::fs::read_to_string(work.path().join("bin").join("git-invocations"))
        .expect("the recording git was reached")
        .lines()
        .map(|line| match line.strip_prefix("-C ") {
            Some(anchored) => anchored.split_once(' ').map_or("", |(_, args)| args),
            None => line,
        })
        .map(str::to_string)
        .collect()
}

/// Every `git` invocation of a `rigger setup` whose rebuild folds no entry, in order: the ones its
/// scaffold and its identity make, and none for a root or an object database.
#[cfg(feature = "symbols")]
const SETUP_OWN_GIT: [&str; 5] = [
    "rev-parse --show-toplevel",
    "rev-parse --show-toplevel",
    "rev-parse --show-toplevel",
    "rev-parse --git-common-dir",
    "rev-parse --git-path hooks",
];

/// How often `rigger setup` in `cwd` asked git whether an object database stands there, and how
/// many `git cat-file --batch` processes it started.
fn blob_source_starts_of_setup(cwd: &Path) -> (usize, usize) {
    let invocations = git_invocations_of_setup(cwd);
    let asked = |args: &str| invocations.iter().filter(|line| *line == args).count();
    (asked("rev-parse --git-dir"), asked("cat-file --batch"))
}

/// Given a repository whose log holds three entries, two of them resolving only from the blobs
/// they name, when the operator runs `rigger setup`, then the rebuild decides once whether an
/// object database can be asked and reads every blob from one `git cat-file --batch` process;
/// and a setup that owes no rebuild, a rebuild over a log holding no entry, and a rebuild
/// outside a repository each start none - the last after asking, once, and finding no
/// repository.
#[cfg(feature = "symbols")]
#[test]
fn a_rebuild_reads_every_entrys_blob_from_one_batch_process_started_at_its_first_entry() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, DOCUMENT_PATH, DOCUMENT_BODY);
    let first_source = git_hash_object(root, SOURCE_PATH, true);
    let first_document = git_hash_object(root, DOCUMENT_PATH, true);
    let project = settled(root);
    // The tree moves on, so both first generations resolve from the object database alone.
    write_text(root, SOURCE_PATH, SOURCE_WITHOUT_HELPER);
    std::fs::remove_file(root.join(DOCUMENT_PATH)).unwrap();
    let walked = |prefix, path| events_of(walked_batch(prefix, path));
    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            &first_source,
            false,
            10,
            walked("gc", SOURCE_PATH),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            &first_document,
            false,
            11,
            walked("gd", DOCUMENT_PATH),
        ),
        recording(
            "gd",
            SOURCE_PATH,
            &first_source,
            false,
            12,
            walked("gd", SOURCE_PATH),
        ),
    ];
    record(root, &recordings);
    stand_graph(root, &project);

    assert_eq!(
        blob_source_starts_of_setup(root),
        (1, 1),
        "one question and one batch process for three entries"
    );
    assert_eq!(
        generations(
            root,
            &project,
            &["gc/src/lib.rs", "gd/docs/architecture.md", "gd/src/lib.rs"]
        ),
        vec![
            held_generation("gc", SOURCE_PATH),
            held_generation("gd", DOCUMENT_PATH),
            held_generation("gd", SOURCE_PATH),
        ],
        "each entry resolved from the blob the one process answered"
    );
    assert_eq!(
        git_invocations_of_setup(root),
        SETUP_OWN_GIT,
        "a setup that owes no rebuild starts no git process beyond its scaffold's own"
    );

    // A rebuild that meets no entry never starts the process.
    let bare = temp_project();
    let project = settled(bare.path());
    stand_graph(bare.path(), &project);
    let position = append_unfolded(
        bare.path(),
        Event::new(TYPE_CODE_ENTITY_EXTRACTED, common::cli::code_entity()),
    );
    assert_eq!(
        git_invocations_of_setup(bare.path()),
        SETUP_OWN_GIT,
        "a rebuild that folds no entry asks git for neither a root nor an object database"
    );
    assert!(
        applied(&rigger_file(bare.path(), "graph.db"), position),
        "the rebuild that started no process did fold the log"
    );

    // Outside a repository the one question is asked, and no process follows it.
    let repoless = temp_repoless_project();
    write_text(repoless.path(), SOURCE_PATH, SOURCE_BODY);
    let project = settled(repoless.path());
    stand_graph(repoless.path(), &project);
    record(
        repoless.path(),
        &[
            recording(
                "gc",
                SOURCE_PATH,
                NOT_HELD,
                false,
                10,
                walked("gc", SOURCE_PATH),
            ),
            recording(
                "gd",
                SOURCE_PATH,
                NOT_HELD,
                false,
                11,
                walked("gd", SOURCE_PATH),
            ),
        ],
    );
    assert_eq!(blob_source_starts_of_setup(repoless.path()), (1, 0));
    assert_eq!(
        generations(
            repoless.path(),
            &project,
            &["gc/src/lib.rs", "gd/src/lib.rs"]
        ),
        vec![
            held_generation("gc", SOURCE_PATH),
            held_generation("gd", SOURCE_PATH),
        ]
    );
}

/// Given a repository whose log holds entries naming blobs, in the lane that compiles no
/// extraction, when the operator runs `rigger setup`, then the rebuild folds the log without
/// asking git for an object database and without starting a `git cat-file --batch` process.
#[cfg(not(feature = "symbols"))]
#[test]
fn without_an_extraction_a_rebuild_over_entries_starts_no_batch_process() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, common::fixtures::SOURCE_BODY);
    let project = settled(root);
    stand_graph(root, &project);
    let recordings = ["gc", "gd"].map(|prefix| Recording {
        named: generation_ingested(
            prefix,
            SOURCE_PATH,
            walked_generation("gc", SOURCE_PATH),
            NOT_HELD,
            false,
        ),
        secs: 10,
        batch: None,
    });
    let positions = record(root, &recordings);

    assert_eq!(blob_source_starts_of_setup(root), (0, 0));
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        positions
            .iter()
            .map(|position| applied(&graph_db, *position))
            .collect::<Vec<_>>(),
        vec![true, true],
        "the rebuild that started no process did fold both entries"
    );
}

/// Given a repository whose tree still holds what three entries were recorded from, the first
/// naming a loose object whose body is truncated, the second a loose object whose header is
/// corrupt and the third, for a file since deleted, a name that is not an object id though git
/// would resolve it to the file's committed bytes, when the operator removes the truncated object
/// and runs `rigger setup`, then git answers both objects missing, the third name is never asked,
/// the rebuild completes, and the rebuilt `graph.db` equals the incremental one: the first two
/// entries resolved from the tree's files and the third resolved nothing.
#[cfg(feature = "symbols")]
#[test]
fn an_object_git_does_not_answer_falls_to_the_tree_and_a_name_that_is_no_object_id_is_not_asked() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, DOCUMENT_PATH, DOCUMENT_BODY);
    write_text(root, GONE_PATH, GONE_BODY);
    commit_sources(root);
    let source = git_hash_object(root, SOURCE_PATH, false);
    let document = git_hash_object(root, DOCUMENT_PATH, false);
    let committed_gone = format!("HEAD:{GONE_PATH}");
    assert_eq!(
        git_out(root, &["rev-parse", &committed_gone]),
        git_hash_object(root, GONE_PATH, false),
        "git resolves the third entry's name to the deleted file's committed blob"
    );
    let project = settled(root);
    std::fs::remove_file(root.join(GONE_PATH)).unwrap();

    let walked = |prefix, path| events_of(walked_batch(prefix, path));
    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            &source,
            false,
            10,
            walked("gc", SOURCE_PATH),
        ),
        recording(
            "gd",
            DOCUMENT_PATH,
            &document,
            false,
            11,
            walked("gd", DOCUMENT_PATH),
        ),
        Recording {
            batch: None,
            ..recording(
                "gc",
                GONE_PATH,
                &committed_gone,
                false,
                12,
                events_of(&GONE_BATCH),
            )
        },
    ];
    let positions = record(root, &recordings);
    let scratch = tempfile::tempdir().unwrap();
    let incremental = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&incremental, &project, root, &recordings),
        vec![EntryFold::BatchAsked; 3]
    );
    stand_graph(root, &project);

    let truncated = loose_object(root, &source);
    let whole = std::fs::read(&truncated).unwrap();
    std::fs::write(&truncated, &whole[..whole.len() - 6]).unwrap();
    std::fs::write(loose_object(root, &document), b"garbage").unwrap();
    let (out, err, ok) = setup(root);
    let refused = format!(
        "rigger: graph: event store: git cat-file --batch stopped while answering object \
         {source}: restore the object or remove it, after which git answers it missing and its \
         entry resolves from the tree"
    );
    assert_eq!(
        (ok, err.lines().last()),
        (false, Some(refused.as_str())),
        "stdout: {out} stderr: {err}"
    );

    // The remedy the failure names second: the object is removed.
    std::fs::remove_file(&truncated).unwrap();
    assert_eq!(
        setup_rebuild_lines(root),
        vec![LOST_FOLD_REBUILD_LINE, REBUILT_LINE],
        "the rebuild completes and passes no entry over"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        graph_identity(&graph_db, &project),
        graph_identity(&incremental, &project),
        "the rebuilt graph.db is the incremental one, fold state included"
    );
    assert_eq!(
        (
            generations(
                root,
                &project,
                &["gc/src/lib.rs", "gd/docs/architecture.md", "gc/src/gone.rs"]
            ),
            positions
                .iter()
                .map(|position| applied(&graph_db, *position))
                .collect::<Vec<_>>(),
        ),
        (
            vec![
                held_generation("gc", SOURCE_PATH),
                held_generation("gd", DOCUMENT_PATH),
                None,
            ],
            vec![true, true, true],
        ),
        "the two entries git answered missing for resolved from the tree, and the entry whose \
         name is no object id resolved nothing and is recorded all the same"
    );
    assert_eq!(setup_rebuild_lines(root), Vec::<String>::new());
}

/// Given a repository whose tree holds, beside a source file the walk visits, four files the walk
/// never reads - one under a hidden directory, one the tree's `.gitignore` excludes, one reached
/// only through a symbolic link, and a workflow definition at a path that is not the workflow
/// definition's - each holding bytes that extract to the generation an entry names for its path,
/// when the operator runs `rigger setup`, then the rebuild reads the tree by the tree's one read
/// rule: the visited file's entry resolves, the four others resolve nothing and are recorded all
/// the same, and the rebuilt `graph.db` equals the one whose incremental folds resolved nothing
/// for them. For its five entries the rebuild asks git once where the tree is rooted and once
/// whether an object database stands there, and starts one batch process.
#[cfg(feature = "symbols")]
#[test]
fn setup_reads_only_the_files_the_walk_visits_and_asks_git_for_its_root_once() {
    let dir = temp_project();
    let root = dir.path();
    let hidden = ".hidden/lib.rs";
    let ignored = "ignored/lib.rs";
    let linked = "src/link.rs";
    let misplaced = "elsewhere.yml";
    write_text(root, SOURCE_PATH, SOURCE_WITHOUT_HELPER);
    write_text(root, hidden, SOURCE_WITHOUT_HELPER);
    write_text(root, ignored, SOURCE_WITHOUT_HELPER);
    write_text(root, misplaced, WORKFLOW_BODY);
    write_text(root, ".gitignore", "ignored/\n");
    std::os::unix::fs::symlink("lib.rs", root.join(linked)).unwrap();
    let project = settled(root);
    assert_eq!(
        [hidden, ignored, linked].map(|path| std::fs::read_to_string(root.join(path)).unwrap()),
        [SOURCE_WITHOUT_HELPER; 3],
        "each unvisited path holds the bytes its entry was recorded from"
    );

    let recordings = [
        recording(
            "gc",
            SOURCE_PATH,
            NOT_HELD,
            false,
            10,
            source_without_helper_batch(SOURCE_PATH),
        ),
        recording(
            "gc",
            hidden,
            NOT_HELD,
            false,
            11,
            source_without_helper_batch(hidden),
        ),
        recording(
            "gc",
            ignored,
            NOT_HELD,
            false,
            12,
            source_without_helper_batch(ignored),
        ),
        recording(
            "gc",
            linked,
            NOT_HELD,
            false,
            13,
            source_without_helper_batch(linked),
        ),
        recording(
            "gw",
            misplaced,
            NOT_HELD,
            false,
            14,
            events_of(walked_batch("gw", WORKFLOW_PATH)),
        ),
    ];
    let positions = record(root, &recordings);
    let mut as_folded = recordings.clone();
    for unvisited in &mut as_folded[1..] {
        unvisited.batch = None;
    }
    let scratch = tempfile::tempdir().unwrap();
    let incremental = scratch.path().join("incremental.db");
    assert_eq!(
        fold_incrementally(&incremental, &project, root, &as_folded),
        vec![EntryFold::BatchAsked; 5]
    );
    stand_graph(root, &project);

    // The rebuild runs before the scaffold's last question, the hooks directory.
    let (before_rebuild, after_rebuild) = SETUP_OWN_GIT.split_at(4);
    assert_eq!(
        git_invocations_of_setup(root),
        [
            before_rebuild,
            &[
                "rev-parse --show-toplevel",
                "rev-parse --git-dir",
                "cat-file --batch",
            ],
            after_rebuild,
        ]
        .concat(),
        "the root and the object database are each asked of git once, at the first entry"
    );

    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        (
            generations(
                root,
                &project,
                &[
                    "gc/src/lib.rs",
                    "gc/.hidden/lib.rs",
                    "gc/ignored/lib.rs",
                    "gc/src/link.rs",
                    "gw/elsewhere.yml",
                ]
            ),
            positions
                .iter()
                .map(|position| applied(&graph_db, *position))
                .collect::<Vec<_>>(),
        ),
        (
            vec![
                Some(recordings[0].named.generation.clone()),
                None,
                None,
                None,
                None,
            ],
            vec![true; 5],
        ),
        "only the file the walk visits is read; every entry's position is recorded"
    );
    assert_eq!(
        graph_identity(&graph_db, &project),
        graph_identity(&incremental, &project),
        "the rebuilt graph.db is the incremental one, fold state included"
    );
    assert_eq!(setup_rebuild_lines(root), Vec::<String>::new());
}

// Spec 107, criterion 6 - THE REBUILD REPORTS WHAT THE NEXT INGEST RECORDS.
//
// Once a rebuild ends, `rigger setup` prints one number: the identities whose generation in
// `graph.db` is not their latest recording's and whose file the tree holds. In the default lane a
// number above zero carries a note; a zero and the light lane's line carry none.

/// The report line a rebuild prints, before its number.
const REPORT_LEAD: &str = "identities the tree holds a file for whose generation in graph.db is \
                           not their latest recording's: ";

/// The note the default lane's report line ends with when its number is above zero.
#[cfg(feature = "symbols")]
const REPORT_NOTE: &str = " (each records an entry at its next ingest of the file)";

/// A generation no source extracts to.
const UNREPRODUCED: &str = "0badc0de";

/// A source file under a hidden directory, which the walk never enters.
const HIDDEN_PATH: &str = ".hidden/lib.rs";

/// A design document whose bytes are not UTF-8, which the design half maps to the empty batch.
const UNDECODED_PATH: &str = "docs/undecoded.md";

/// A source file the tree does not hold.
const ABSENT_PATH: &str = "src/absent.rs";

/// The report lines among what `rigger setup` printed.
fn report_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|line| line.starts_with("identities "))
        .collect()
}

/// The recording of `generation` under `prefix` for `file` at `secs`, naming no blob.
fn named_recording(
    prefix: &str,
    file: &str,
    generation: &str,
    excluded: bool,
    secs: u64,
) -> Recording {
    Recording {
        named: generation_ingested(prefix, file, generation, "", excluded),
        secs,
        batch: None,
    }
}

/// A repository whose tree and log hold every kind of identity the report tells apart, with a
/// `graph.db` standing behind the log: the project identity and each entry's position.
///
/// - `gc/src/lib.rs`: an entry of the generation its file extracts to, then a later entry no
///   source reproduces, so the rebuilt graph holds the EARLIER generation of a file the tree holds.
/// - `gc/src/checks.rs`: one entry of the generation its file extracts to, so the rebuilt graph
///   holds the latest recording's.
/// - `gd/docs/architecture.md`: one entry no source reproduces, so the rebuilt graph holds NO
///   generation of a file the tree holds.
/// - `gc/src/absent.rs`: one entry no source reproduces, of a file the tree does not hold.
/// - `gc/.hidden/lib.rs`: one entry no source reproduces, of a regular readable file outside the
///   walk's scope.
/// - `gd/docs/undecoded.md`: one entry no source reproduces, of a file the tree holds whose bytes
///   the design half maps to the empty batch.
fn report_fixture(root: &Path) -> (String, Vec<u64>) {
    write_text(root, SOURCE_PATH, common::fixtures::SOURCE_BODY);
    write_text(root, TEST_MODULE_PATH, common::fixtures::TEST_MODULE_BODY);
    write_text(root, DOCUMENT_PATH, common::fixtures::DOCUMENT_BODY);
    write_text(root, HIDDEN_PATH, common::fixtures::SOURCE_BODY);
    std::fs::write(root.join(UNDECODED_PATH), [0xff, 0xfe, b'\n']).unwrap();
    let project = settled(root);
    assert_eq!(
        (
            std::fs::read_to_string(root.join(HIDDEN_PATH)).unwrap(),
            root.join(ABSENT_PATH).exists(),
        ),
        (common::fixtures::SOURCE_BODY.to_string(), false),
        "the out-of-scope path holds a regular readable file, and the absent one holds none"
    );
    let positions = record(
        root,
        &[
            named_recording(
                "gc",
                SOURCE_PATH,
                walked_generation("gc", SOURCE_PATH),
                false,
                10,
            ),
            named_recording(
                "gc",
                TEST_MODULE_PATH,
                walked_generation("gc", TEST_MODULE_PATH),
                true,
                11,
            ),
            named_recording("gc", SOURCE_PATH, UNREPRODUCED, false, 12),
            named_recording("gd", DOCUMENT_PATH, UNREPRODUCED, false, 13),
            named_recording("gc", ABSENT_PATH, UNREPRODUCED, false, 14),
            named_recording("gc", HIDDEN_PATH, UNREPRODUCED, false, 15),
            named_recording("gd", UNDECODED_PATH, UNREPRODUCED, false, 16),
        ],
    );
    stand_graph(root, &project);
    (project, positions)
}

/// The identities of [`report_fixture`], in the order its doc lists them.
const REPORT_IDENTITIES: [&str; 6] = [
    "gc/src/lib.rs",
    "gc/src/checks.rs",
    "gd/docs/architecture.md",
    "gc/src/absent.rs",
    "gc/.hidden/lib.rs",
    "gd/docs/undecoded.md",
];

/// The generation the rebuilt graph holds for each of [`REPORT_IDENTITIES`]: where an extraction
/// is compiled, the two entries the tree's files reproduce; where none is, nothing.
fn report_fixture_generations() -> Vec<Option<String>> {
    #[cfg(feature = "symbols")]
    let reproduced = [
        held_generation("gc", SOURCE_PATH),
        held_generation("gc", TEST_MODULE_PATH),
    ];
    #[cfg(not(feature = "symbols"))]
    let reproduced = [None, None];
    reproduced.into_iter().chain(vec![None; 4]).collect()
}

/// The report line of a rebuild over [`report_fixture`]. Where an extraction is compiled it
/// counts the two identities whose file the tree holds and whose graph generation is not their
/// latest recording's - one held at an earlier generation, one at none - and neither the gone
/// file, the out-of-scope one nor the one extracting to the empty batch, and carries the note.
/// Where none is compiled no entry folds and no half is asked, so it counts the four identities
/// whose file the tree holds, and carries no note.
fn report_fixture_line() -> String {
    #[cfg(feature = "symbols")]
    let counted = format!("2{REPORT_NOTE}");
    #[cfg(not(feature = "symbols"))]
    let counted = "4";
    format!("{REPORT_LEAD}{counted}")
}

/// Given a log whose latest entry of one identity resolves and of five others does not - a file
/// the tree holds at another generation, one it holds that the graph holds no generation of, one
/// it does not hold, one outside the walk's scope and one extracting to the empty batch - when the
/// operator runs `rigger setup`, then the rebuild prints the number of identities whose graph
/// generation is not their latest recording's and whose file the tree holds, with the note in the
/// default lane and none in the light lane; and a setup that rebuilds nothing prints no report.
#[test]
fn the_rebuild_reports_the_identities_behind_their_latest_recording_whose_file_the_tree_holds() {
    let dir = temp_project();
    let root = dir.path();
    let (project, positions) = report_fixture(root);

    let (out, err, ok) = setup(root);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(report_lines(&out), vec![report_fixture_line()]);
    assert_eq!(
        out.lines().skip_while(|line| *line != REBUILT_LINE).nth(2),
        Some(report_fixture_line().as_str()),
        "the report follows the rebuild's own lines; stdout: {out}"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        (
            generations(root, &project, &REPORT_IDENTITIES),
            positions
                .iter()
                .map(|position| applied(&graph_db, *position))
                .collect::<Vec<_>>(),
        ),
        (report_fixture_generations(), vec![true; 7]),
        "the number is read from the rebuilt graph these generations stand in"
    );

    let (out, err, ok) = setup(root);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        (report_lines(&out), out.lines().any(|l| l == REBUILT_LINE)),
        (Vec::<&str>::new(), false),
        "a setup that rebuilds nothing reports nothing"
    );
}

/// Given the same log and tree, and a rebuild that stopped after its third committed batch,
/// leaving its shadow beside an untouched `graph.db`, when the operator runs `rigger setup`, then
/// the resumed rebuild prints the number a single pass prints.
#[test]
fn a_rebuild_interrupted_and_resumed_reports_the_number_a_single_pass_does() {
    let dir = temp_project();
    let root = dir.path();
    let (project, positions) = report_fixture(root);
    let graph_db = rigger_file(root, "graph.db");
    let log = read_run_events(root);
    let through = log
        .iter()
        .position(|event| event.position == positions[2])
        .unwrap();

    let interrupted = Projector::rebuild(
        &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
        &project,
        true,
        &mut |after, sink: &mut RebuildSink| {
            rigger::contextgraph::sqlite::stream_past(&log[..=through], after, 1, sink)?;
            Err(Error("interrupted".to_string()))
        },
        &mut |entry| rigger::ingest::resolve_entry(root, entry, None),
        &mut no_progress,
    );
    assert_eq!(interrupted.map_err(|e| e.0), Err("interrupted".to_string()));
    let shadow = rigger_file(root, "graph.db.rebuild");
    assert_eq!(
        (
            applied(&shadow, positions[2]),
            applied(&shadow, positions[3]),
            applied(&graph_db, positions[0]),
        ),
        (true, false, false),
        "the stopped rebuild committed three entries to its shadow and left graph.db untouched"
    );

    let (out, err, ok) = setup(root);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        (
            report_lines(&out),
            out.lines().any(|l| l == REBUILT_LINE),
            generations(root, &project, &REPORT_IDENTITIES),
            positions
                .iter()
                .map(|position| applied(&graph_db, *position))
                .collect::<Vec<_>>(),
        ),
        (
            vec![report_fixture_line().as_str()],
            true,
            report_fixture_generations(),
            vec![true; 7],
        ),
        "the resumed rebuild reports what a single pass reports; stdout: {out}"
    );
}

/// Given a log whose only identities behind their latest recording are a file the tree does not
/// hold and a regular readable file outside the walk's scope, when the operator runs `rigger
/// setup`, then the rebuild prints a zero, and no note in either lane.
#[test]
fn a_rebuild_with_no_identity_to_record_again_reports_a_zero_without_the_note() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, HIDDEN_PATH, common::fixtures::SOURCE_BODY);
    let project = settled(root);
    record(
        root,
        &[
            named_recording("gc", ABSENT_PATH, UNREPRODUCED, false, 10),
            named_recording("gc", HIDDEN_PATH, UNREPRODUCED, false, 11),
        ],
    );
    stand_graph(root, &project);

    let (out, err, ok) = setup(root);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(report_lines(&out), vec![format!("{REPORT_LEAD}0")]);
    assert_eq!(
        generations(root, &project, &["gc/src/absent.rs", "gc/.hidden/lib.rs"]),
        vec![None, None],
        "both identities are behind their latest recording, and neither is counted"
    );
}

/// Given a run stream holding keyed derived rows that carry no group, an unkeyed derived row, a
/// knowledge event keyed like a content key, and ledger entries, when the log side of the report
/// is read, then `ingest::perceived_generations` answers each identity's latest recording, entry
/// or derived row alike, and makes ONE typed read of the perception types from the stream's
/// start: no whole-stream read and no lookup.
#[test]
fn the_log_side_of_the_report_is_one_typed_read_of_the_perception_types() {
    use common::fixtures::{CountedRead, ReadCountingStore};
    use rigger::ingest::META_REPLAY_KEY;
    use std::collections::HashMap;

    let dir = temp_project();
    let root = dir.path();
    settled(root);
    let keyed =
        |type_: &str, key: &str| Event::new(type_, b"{}".to_vec()).with_meta(META_REPLAY_KEY, key);
    for event in [
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/old.rs@h1#0"),
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/lib.rs@h1#0"),
        Event::new(TYPE_CODE_ENTITY_EXTRACTED, b"{}".to_vec()),
        keyed("ReviewFinding", "gc/src/finding.rs@h1#0"),
        entry_of(&named_recording("gc", SOURCE_PATH, "h2", false, 10)),
        entry_of(&named_recording("gd", DOCUMENT_PATH, "h3", false, 11)),
        entry_of(&named_recording("gd", DOCUMENT_PATH, "h4", false, 12)),
    ] {
        append_unfolded(root, event);
    }

    let (answered, reads) = with_run_store(root, |store| {
        let counting = ReadCountingStore::new(store);
        let answered =
            rigger::ingest::perceived_generations(&counting, rigger::conductor::STREAM).unwrap();
        (answered, counting.reads())
    });

    let answer = |identity: &str, generation: &str, key: &str| {
        (
            identity.to_string(),
            (generation.to_string(), vec![key.to_string()]),
        )
    };
    assert_eq!(
        answered,
        HashMap::from([
            answer("gc/src/old.rs", "h1", "gc/src/old.rs@h1#0"),
            answer("gc/src/lib.rs", "h2", "gc/src/lib.rs@h2#1"),
            answer(
                "gd/docs/architecture.md",
                "h4",
                "gd/docs/architecture.md@h4#1"
            ),
        ])
    );
    assert_eq!(
        reads,
        vec![CountedRead::Typed {
            stream: rigger::conductor::STREAM.to_string(),
            from: 0,
            only: true,
            types: rigger::retention::PERCEPTION_TYPES
                .map(str::to_string)
                .to_vec(),
            materialized: 6,
        }]
    );
}

/// Given a store whose `.rigger/` sits in a subdirectory of a repository, and three identities
/// behind their latest recording - a file of the top level, a file of the subdirectory named by
/// its path from the top level, and that file named by its path from the working directory - when
/// the operator runs `rigger setup` in the subdirectory, then the report reads the tree from the
/// repository's top level: it counts the first two and not the third.
#[test]
fn the_report_reads_the_tree_from_the_top_level_of_a_store_in_a_repository_subdirectory() {
    let dir = temp_project();
    let top = dir.path();
    let sub = top.join("sub");
    write_text(top, SOURCE_PATH, common::fixtures::SOURCE_BODY);
    write_text(&sub, "src/only.rs", "fn only() {}\n");
    let project = settled(&sub);
    record(
        &sub,
        &[
            named_recording("gc", SOURCE_PATH, UNREPRODUCED, false, 10),
            named_recording("gc", "sub/src/only.rs", UNREPRODUCED, false, 11),
            named_recording("gc", "src/only.rs", UNREPRODUCED, false, 12),
        ],
    );
    stand_graph(&sub, &project);
    assert_eq!(
        (
            sub.join(SOURCE_PATH).exists(),
            sub.join("src/only.rs").is_file(),
            top.join("src/only.rs").exists(),
        ),
        (false, true, false),
        "the working directory holds the third identity's file and not the first's"
    );

    let (out, err, ok) = setup(&sub);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    #[cfg(feature = "symbols")]
    let counted = format!("2{REPORT_NOTE}");
    #[cfg(not(feature = "symbols"))]
    let counted = "2";
    assert_eq!(report_lines(&out), vec![format!("{REPORT_LEAD}{counted}")]);
}

// Spec 107, criterion 6 - the report's boundaries, from outside the crates that own them: the
// count of exactly one, the file question at the edges of its public form, the root the report
// asks git for, the stream the log side reads, and the type list every other reader still hands.

/// A source file the tree's `.gitignore` excludes.
const IGNORED_PATH: &str = "ignored/lib.rs";

/// A symbolic link to the source file.
const LINKED_PATH: &str = "src/link.rs";

/// A workflow definition at a path that is not the workflow definition's.
const MISPLACED_PATH: &str = "elsewhere.yml";

/// Plant, under `root`, one file of each kind the file question tells apart: the source file, the
/// design document and the workflow definition an ingest reads, and beside them a document whose
/// bytes are not UTF-8, an empty source file, a source file under a hidden directory, one the
/// tree's `.gitignore` excludes, a symbolic link to the source file, a workflow definition at
/// another path and a file at the top level named like a path with no prefix.
fn plant_read_rule_tree(root: &Path) {
    write_text(root, SOURCE_PATH, common::fixtures::SOURCE_BODY);
    write_text(root, DOCUMENT_PATH, common::fixtures::DOCUMENT_BODY);
    write_text(root, WORKFLOW_PATH, common::fixtures::WORKFLOW_BODY);
    std::fs::write(root.join(UNDECODED_PATH), [0xff, 0xfe, b'\n']).unwrap();
    write_text(root, "src/empty.rs", "");
    write_text(root, HIDDEN_PATH, common::fixtures::SOURCE_BODY);
    write_text(root, IGNORED_PATH, common::fixtures::SOURCE_BODY);
    write_text(root, MISPLACED_PATH, common::fixtures::WORKFLOW_BODY);
    write_text(root, "lib.rs", common::fixtures::SOURCE_BODY);
    write_text(root, ".gitignore", "ignored/\n");
    std::os::unix::fs::symlink("lib.rs", root.join(LINKED_PATH)).unwrap();
    assert_eq!(
        [HIDDEN_PATH, IGNORED_PATH, LINKED_PATH, "lib.rs"]
            .map(|path| std::fs::read_to_string(root.join(path)).unwrap()),
        [common::fixtures::SOURCE_BODY; 4],
        "each path the read rule refuses holds a readable source file all the same"
    );
}

/// Given a tree holding one file of each kind, when `ingest::next_ingest_records` is asked of an
/// identity, then it answers by the tree's one read rule and the half its prefix names: an
/// in-scope regular file whose half extracts a batch from its bytes, and nothing else. Where no
/// extraction is compiled no half is asked, so every file the read rule hands bytes for answers
/// yes whatever its prefix.
#[test]
fn the_file_question_answers_by_the_trees_read_rule_and_the_half_its_prefix_names() {
    let dir = temp_project();
    let root = dir.path();
    plant_read_rule_tree(root);
    // What only an extraction decides: a prefix that names no half, and bytes a half maps to the
    // empty batch.
    let extraction_decides = cfg!(not(feature = "symbols"));

    let answers: Vec<(&str, bool)> = [
        "",
        "gc",
        "gc/",
        "gc/.hidden/lib.rs",
        "gc/ignored/lib.rs",
        "gc/src",
        "gc/src/absent.rs",
        "gc/src/empty.rs",
        "gc/src/lib.rs",
        "gc/src/link.rs",
        "gd/docs/architecture.md",
        "gd/docs/undecoded.md",
        "gw/.rigger/workflow.yml",
        "gw/elsewhere.yml",
        "gx/src/lib.rs",
        "lib.rs",
    ]
    .into_iter()
    .map(|identity| {
        (
            identity,
            rigger::ingest::next_ingest_records(root, identity),
        )
    })
    .collect();

    assert_eq!(
        answers,
        vec![
            ("", false),
            ("gc", false),
            ("gc/", false),
            ("gc/.hidden/lib.rs", false),
            ("gc/ignored/lib.rs", false),
            ("gc/src", false),
            ("gc/src/absent.rs", false),
            ("gc/src/empty.rs", true),
            ("gc/src/lib.rs", true),
            ("gc/src/link.rs", false),
            ("gd/docs/architecture.md", true),
            ("gd/docs/undecoded.md", extraction_decides),
            ("gw/.rigger/workflow.yml", true),
            ("gw/elsewhere.yml", false),
            ("gx/src/lib.rs", extraction_decides),
            ("lib.rs", false),
        ]
    );
}

/// Given an in-scope regular file this process cannot read, when `ingest::next_ingest_records` is
/// asked of its identity, then it answers no: a file that hands no bytes is named by no walk.
#[test]
fn the_file_question_answers_no_for_a_file_this_process_cannot_read() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, common::fixtures::SOURCE_BODY);
    assert!(
        rigger::ingest::next_ingest_records(root, "gc/src/lib.rs"),
        "the readable file is named"
    );
    if !common::fixtures::arm_read_fault(&root.join(SOURCE_PATH)) {
        return;
    }
    assert!(!rigger::ingest::next_ingest_records(root, "gc/src/lib.rs"));
}

/// Given a log whose identities behind their latest recording are the workflow definition the
/// tree holds and four files the tree's read rule refuses - one the `.gitignore` excludes, one
/// reached through a symbolic link, one under a hidden directory and a workflow definition at
/// another path - when the operator runs `rigger setup`, then the rebuild reports exactly one,
/// with the note in the default lane and none in the light lane.
#[test]
fn a_rebuild_with_one_identity_to_record_again_reports_a_one_with_the_note() {
    let dir = temp_project();
    let root = dir.path();
    plant_read_rule_tree(root);
    let project = settled(root);
    write_text(root, WORKFLOW_PATH, common::fixtures::WORKFLOW_BODY);
    let recorded = [
        ("gw", WORKFLOW_PATH),
        ("gc", IGNORED_PATH),
        ("gc", LINKED_PATH),
        ("gc", HIDDEN_PATH),
        ("gw", MISPLACED_PATH),
    ];
    let recordings: Vec<Recording> = recorded
        .iter()
        .zip(10..)
        .map(|((prefix, path), secs)| named_recording(prefix, path, UNREPRODUCED, false, secs))
        .collect();
    record(root, &recordings);
    stand_graph(root, &project);

    let (out, err, ok) = setup(root);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    #[cfg(feature = "symbols")]
    let counted = format!("1{REPORT_NOTE}");
    #[cfg(not(feature = "symbols"))]
    let counted = "1";
    assert_eq!(report_lines(&out), vec![format!("{REPORT_LEAD}{counted}")]);
    let identities: Vec<String> = recorded
        .iter()
        .map(|(prefix, path)| format!("{prefix}/{path}"))
        .collect();
    assert_eq!(
        generations(
            root,
            &project,
            &identities.iter().map(String::as_str).collect::<Vec<_>>()
        ),
        vec![None; 5],
        "all five identities are behind their latest recording, and one is counted"
    );
}

/// The keyed derived rows of the `gc` batch the source file extracts to, as a store that has not
/// shed them holds them.
#[cfg(feature = "symbols")]
fn keyed_source_rows() -> Vec<Event> {
    let generation = walked_generation("gc", SOURCE_PATH);
    events_of(walked_batch("gc", SOURCE_PATH))
        .into_iter()
        .enumerate()
        .map(|(n, event)| {
            event.with_meta(
                rigger::ingest::META_REPLAY_KEY,
                format!("gc/{SOURCE_PATH}@{generation}#{n}"),
            )
        })
        .collect()
}

/// A repository whose tree holds the source file and whose log holds `rows`, appended behind a
/// standing `graph.db`: the directory and the project identity.
#[cfg(feature = "symbols")]
fn owing_rows(rows: Vec<Event>) -> (tempfile::TempDir, String) {
    let dir = temp_project();
    write_text(dir.path(), SOURCE_PATH, SOURCE_BODY);
    let project = settled(dir.path());
    stand_graph(dir.path(), &project);
    for row in rows {
        append_unfolded(dir.path(), row);
    }
    (dir, project)
}

/// A ledger entry of the source file's identity at a generation of its own whose payload does not
/// parse: a recording the log side reads by its key, which a rebuild passes over without asking
/// for its batch.
#[cfg(feature = "symbols")]
fn unparsable_source_entry() -> Event {
    Event::new(TYPE_GENERATION_INGESTED, b"{}".to_vec()).with_meta(
        rigger::ingest::META_REPLAY_KEY,
        format!("gc/{SOURCE_PATH}@{UNREPRODUCED}#1"),
    )
}

/// Given a rebuild that folds no ledger entry, when the operator runs `rigger setup`, then the
/// report asks git where the tree is rooted only once an identity is behind its latest
/// recording: over keyed derived rows the rebuilt graph holds the generation of it prints a zero
/// and asks git nothing beyond the scaffold's own questions, and over a recording the rebuild
/// passed over it asks for the root once, with no object database asked, and counts the file.
#[cfg(feature = "symbols")]
#[test]
fn the_report_asks_git_for_the_root_only_once_an_identity_is_behind_its_recording() {
    let zero = format!("{REPORT_LEAD}0");
    let one = format!("{REPORT_LEAD}1{REPORT_NOTE}");
    let (before_rebuild, after_rebuild) = SETUP_OWN_GIT.split_at(4);
    let behind = || {
        let mut rows = keyed_source_rows();
        rows.push(unparsable_source_entry());
        rows
    };

    // What each rebuild prints, and the generation its graph ends holding.
    let printed: Vec<(Vec<String>, Vec<Option<String>>)> = [keyed_source_rows(), behind()]
        .into_iter()
        .map(|rows| {
            let (dir, project) = owing_rows(rows);
            let (out, err, ok) = setup(dir.path());
            assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
            (
                report_lines(&out).into_iter().map(str::to_string).collect(),
                generations(dir.path(), &project, &["gc/src/lib.rs"]),
            )
        })
        .collect();
    assert_eq!(
        printed,
        vec![
            (vec![zero], vec![held_generation("gc", SOURCE_PATH)]),
            (vec![one], vec![held_generation("gc", SOURCE_PATH)]),
        ],
        "the graph holds the derived rows' generation either way; the passed-over recording \
         leaves the identity behind"
    );

    // What each rebuild asks git.
    let (held, _) = owing_rows(keyed_source_rows());
    assert_eq!(
        git_invocations_of_setup(held.path()),
        SETUP_OWN_GIT,
        "no identity is behind, so the report asks git nothing"
    );
    let (owing, _) = owing_rows(behind());
    assert_eq!(
        git_invocations_of_setup(owing.path()),
        [
            before_rebuild,
            &["rev-parse --show-toplevel"],
            after_rebuild
        ]
        .concat(),
        "the report is the first to need the root, and asks for it once"
    );
}

/// Given a run stream holding a ledger entry of an identity and then a keyed derived row of a
/// later generation of it, and another stream of the same project holding an entry of a second
/// identity, when the log side of the report is read for the run stream, then
/// `ingest::perceived_generations` answers the derived row as the first identity's latest
/// recording and nothing for the second: it reads the one stream it is handed.
#[test]
fn the_log_side_of_the_report_reads_the_one_stream_it_is_handed() {
    use rigger::ingest::META_REPLAY_KEY;
    use std::collections::HashMap;

    let dir = temp_project();
    let root = dir.path();
    settled(root);
    let elsewhere = "elsewhere";
    append_unfolded(
        root,
        entry_of(&named_recording("gc", SOURCE_PATH, "h2", false, 10)),
    );
    append_unfolded(
        root,
        Event::new(TYPE_CODE_ENTITY_EXTRACTED, b"{}".to_vec())
            .with_meta(META_REPLAY_KEY, "gc/src/lib.rs@h3#0"),
    );
    let (run, other) = with_run_store(root, |store| {
        store
            .append(
                elsewhere,
                ExpectedRevision::Any,
                &[entry_of(&named_recording(
                    "gd",
                    DOCUMENT_PATH,
                    "h4",
                    false,
                    11,
                ))],
            )
            .unwrap();
        (
            rigger::ingest::perceived_generations(store, rigger::conductor::STREAM).unwrap(),
            rigger::ingest::perceived_generations(store, elsewhere).unwrap(),
        )
    });

    let answer = |identity: &str, generation: &str, key: &str| {
        HashMap::from([(
            identity.to_string(),
            (generation.to_string(), vec![key.to_string()]),
        )])
    };
    assert_eq!(
        (run, other),
        (
            answer("gc/src/lib.rs", "h3", "gc/src/lib.rs@h3#0"),
            answer(
                "gd/docs/architecture.md",
                "h4",
                "gd/docs/architecture.md@h4#1"
            ),
        )
    );
}

/// Given a tree whose source file the log's keyed derived rows record at the generation it
/// extracts to, and a later ledger entry of the same identity at another generation, when the
/// index-lag readers are asked, then neither lists the file: they read the derived types alone,
/// so a ledger entry is no recording to them, while the perception types read it as the latest.
#[cfg(feature = "symbols")]
#[test]
fn the_index_lag_readers_pass_a_ledger_entry_over() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, SOURCE_PATH, SOURCE_BODY);
    write_text(root, TEST_MODULE_PATH, TEST_MODULE_BODY);
    let tree = root.to_str().unwrap();
    let file = vec![SOURCE_PATH.to_string()];
    let lag = |prior: &[Event]| {
        (
            rigger::ingest::graph_index_lag(tree, prior, &file),
            rigger::ingest::graph_index_lag_sample(tree, prior),
        )
    };
    let fresh = (Vec::<String>::new(), Vec::<String>::new());

    let mut prior = keyed_source_rows();
    assert_eq!(
        (lag(&prior), lag(&[])),
        (fresh.clone(), (file.clone(), Vec::new())),
        "the derived rows record the generation the file extracts to; without them it lags"
    );

    prior.push(entry_of(&named_recording(
        "gc",
        SOURCE_PATH,
        UNREPRODUCED,
        false,
        10,
    )));
    assert_eq!(
        rigger::ingest::project_scoped_latest_generations(
            &prior,
            &rigger::retention::PERCEPTION_TYPES
        )
        .get("gc/src/lib.rs")
        .map(|(generation, _)| generation.as_str()),
        Some(UNREPRODUCED),
        "the entry is the identity's latest recording under the perception types"
    );
    assert_eq!(lag(&prior), fresh);
}
