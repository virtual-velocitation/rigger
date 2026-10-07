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
use common::fixtures::{entry_event, events_of, folds, generation_ingested, write_text};
use rigger::contextgraph::sqlite::{Projector, RebuildSink, Rebuilt};
use rigger::contextgraph::{wired, EntryFold, Error, Fold, Projection, TYPE_CODE_ENTITY_EXTRACTED};
use rigger::eventstore::{Event, ExpectedRevision};
use rigger::retention::{GenerationIngested, TYPE_GENERATION_INGESTED};

#[cfg(feature = "symbols")]
use common::cli::{nanos, temp_repoless_project};
#[cfg(feature = "symbols")]
use common::fixtures::{
    git_ok, git_ok_with_identity, git_out, live_edges, loose_object, walked_batch, DOCUMENT_BODY,
    DOCUMENT_PATH, SOURCE_BODY, SOURCE_PATH, TEST_MODULE_BODY, TEST_MODULE_PATH, WORKFLOW_BODY,
    WORKFLOW_PATH,
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

/// The generation the extraction tree's fixture records for the source file's `gc` batch.
const SOURCE_GENERATION: &str = "f81a57a5c4f55f52";

/// An object id no repository of these tests holds.
#[cfg(feature = "symbols")]
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

/// The object id git computes for the file at `path` under `root`, writing no object.
#[cfg(feature = "symbols")]
fn blob_id(root: &Path, path: &str) -> String {
    git_out(root, &["hash-object", path])
}

/// The object id of the file at `path` under `root`, written to the repository's object database
/// as a loose object.
#[cfg(feature = "symbols")]
fn held_blob(root: &Path, path: &str) -> String {
    git_out(root, &["hash-object", "-w", path])
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
    let first_source = blob_id(root, SOURCE_PATH);
    let test_module = blob_id(root, TEST_MODULE_PATH);
    let first_document = blob_id(root, DOCUMENT_PATH);
    let gone = blob_id(root, GONE_PATH);
    let project = settled(root);

    // The tree moves on: the source file drops `helper` and is not written to the object
    // database, the document drops its citation, the third source file is deleted, and the
    // workflow definition is written, uncommitted.
    write_text(root, SOURCE_PATH, SOURCE_WITHOUT_HELPER);
    let second_source = blob_id(root, SOURCE_PATH);
    write_text(root, DOCUMENT_PATH, DOCUMENT_WITHOUT_CITATION);
    let second_document = held_blob(root, DOCUMENT_PATH);
    std::fs::remove_file(root.join(GONE_PATH)).unwrap();
    write_text(root, WORKFLOW_PATH, WORKFLOW_BODY);
    let workflow = blob_id(root, WORKFLOW_PATH);

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
            SOURCE_GENERATION,
            "878ec204b714de6b",
            "ea5177040caf5338",
            "88eadaf4024b4a86",
            "08eb9cb734e95dc1",
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
            Some("878ec204b714de6b".to_string()),
            Some(recordings[7].named.generation.clone()),
            Some("88eadaf4024b4a86".to_string()),
            Some("08eb9cb734e95dc1".to_string()),
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
            Some(SOURCE_GENERATION.to_string()),
            Some("878ec204b714de6b".to_string()),
            Some("ea5177040caf5338".to_string()),
            Some("08eb9cb734e95dc1".to_string()),
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
            Some(SOURCE_GENERATION.to_string()),
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
            Some("ea5177040caf5338".to_string()),
            Some(SOURCE_GENERATION.to_string()),
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
    let first_source = held_blob(root, SOURCE_PATH);
    let first_document = held_blob(root, DOCUMENT_PATH);
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
        named: generation_ingested("gc", "src/lib.rs", SOURCE_GENERATION, "", false),
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
    write_text(root, "src/lib.rs", common::fixtures::SOURCE_BODY);
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
    let held = Some(SOURCE_GENERATION.to_string());
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
    use common::fixtures::planted_extraction_tree;

    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    let _ = common::git::run_git(root, &["init", "-q"]);
    let project = settled(root);
    let recordings = [
        ("gc", "src/lib.rs", SOURCE_GENERATION, false),
        ("gc", "src/checks.rs", "878ec204b714de6b", true),
        ("gd", "docs/architecture.md", "ea5177040caf5338", false),
        ("gw", ".rigger/workflow.yml", "08eb9cb734e95dc1", false),
    ]
    .map(|(prefix, file, generation, excluded)| Recording {
        named: generation_ingested(prefix, file, generation, "", excluded),
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
        (Some(SOURCE_GENERATION.to_string()), true, true)
    );
}
