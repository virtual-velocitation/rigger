//! Periphery test for spec 122 criterion 5, PROOFS SETTLE ONCE PER TRANSACTION, at the frames the
//! crate's own oracle does not reach. That oracle folds through the projector's private fold
//! inside its `transact` frame; the settlement runs in every transaction frame of the file, so
//! this test drives the same hand-built log, from OUTSIDE the crate, through the public live fold
//! as one batch and one event to a transaction, and through `Projector::rebuild` folding the log
//! as one batch and one event to a batch. Each frame must leave every definition with the same
//! exact proof: the forward references a frame folds before their definitions land, and a
//! generation dropping a reference and one dropping a definition take their proofs away. It
//! runs on both lanes.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::fixtures::{
    attrs_of, definition, definition_ids, f_rs_batch_def, folds, folds_each, graph_file_in,
    proven_definition, rebuild_from_the_log, t_rs_batch_reference,
};
use rigger::contextgraph::sqlite::Projector;
use rigger::eventstore::Event;

/// `tests/t.rs`'s generation `k1` references `kept`, `dropped` and `lead` before `src/f.rs`'s
/// generation `h1` defines them; `k2` drops the reference to `lead`; `h2` keeps `lead`, its head,
/// and `kept` and drops the definition of `dropped`.
fn the_log() -> Vec<Event> {
    vec![
        t_rs_batch_reference(1, "k1", 0, "kept", 3, 1),
        t_rs_batch_reference(2, "k1", 1, "dropped", 4, 2),
        t_rs_batch_reference(3, "k1", 2, "lead", 5, 3),
        f_rs_batch_def(4, "h1", 0, ("lead", "function", 1), None, 4),
        f_rs_batch_def(5, "h1", 1, ("kept", "function", 2), None, 5),
        f_rs_batch_def(6, "h1", 2, ("dropped", "function", 3), None, 6),
        t_rs_batch_reference(7, "k2", 0, "kept", 3, 7),
        t_rs_batch_reference(8, "k2", 1, "dropped", 4, 8),
        f_rs_batch_def(9, "h2", 0, ("lead", "function", 1), None, 9),
        f_rs_batch_def(10, "h2", 1, ("kept", "function", 2), None, 10),
    ]
}

/// Every definition `p` holds, sorted by id, with the attrs it carries.
fn definitions_of(p: &Projector) -> Vec<(String, BTreeMap<String, String>)> {
    definition_ids(p)
        .into_iter()
        .map(|id| (id.clone(), attrs_of(p, &id)))
        .collect()
}

/// `log` folded into a fresh graph file under `dir` in each frame - live as one batch, live one
/// event to a transaction, and rebuilt as one batch and one event to a batch - each named.
fn folded_in_every_frame(dir: &Path, log: &[Event]) -> [(Projector, &'static str); 4] {
    let one_batch = graph_file_in(dir, "one_batch.db");
    folds(&one_batch, log);
    let per_event = graph_file_in(dir, "per_event.db");
    folds_each(&per_event, log);
    rebuild_from_the_log(&dir.join("rebuilt_whole.db"), log, log.len(), &mut |_| {
        Ok(None)
    });
    rebuild_from_the_log(&dir.join("rebuilt_each.db"), log, 1, &mut |_| Ok(None));
    [
        (one_batch, "the live fold of the log as one batch"),
        (per_event, "the live fold, one event to a transaction"),
        (
            graph_file_in(dir, "rebuilt_whole.db"),
            "a rebuild folding the log as one batch",
        ),
        (
            graph_file_in(dir, "rebuilt_each.db"),
            "a rebuild folding one event to a batch",
        ),
    ]
}

/// GIVEN a log whose test references fold before the definitions they name, then a test
/// generation dropping the reference to `lead` and a source generation dropping the definition of
/// `dropped`, WHEN it folds in every frame ([`folded_in_every_frame`]), THEN once the definitions
/// fold each carries the proof its forward reference recorded, and at the log's end every frame
/// leaves `lead` unproven, `kept` proven by `tests/t.rs:3` alone and no definition of `dropped`.
#[test]
fn a_generation_dropping_a_reference_and_a_definition_settles_alike_in_every_frame() {
    let log = the_log();
    let forward = tempfile::tempdir().unwrap();
    let whole = tempfile::tempdir().unwrap();

    let referenced_first = vec![
        (
            "src/f.rs::dropped".to_string(),
            proven_definition("dropped", "function", 3, &["tests/t.rs:4"]),
        ),
        (
            "src/f.rs::kept".to_string(),
            proven_definition("kept", "function", 2, &["tests/t.rs:3"]),
        ),
        (
            "src/f.rs::lead".to_string(),
            proven_definition("lead", "function", 1, &["tests/t.rs:5"]),
        ),
    ];
    for (p, frame) in folded_in_every_frame(forward.path(), &log[..6]) {
        assert_eq!(definitions_of(&p), referenced_first, "{frame}, to h1");
    }

    let settled = vec![
        (
            "src/f.rs::kept".to_string(),
            proven_definition("kept", "function", 2, &["tests/t.rs:3"]),
        ),
        (
            "src/f.rs::lead".to_string(),
            definition("lead", "function", 1),
        ),
    ];
    for (p, frame) in folded_in_every_frame(whole.path(), &log) {
        assert_eq!(definitions_of(&p), settled, "{frame}, to h2");
    }
}
