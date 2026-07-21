//! The code-to-events emit pass (spec 29a): lowers an extracted `SymbolIndex` / `FileSymbols`
//! into `CodeEntityExtracted` (one per definition) and `EdgeInferred` (one per reference) events,
//! which the always-compiled context-graph fold ingests into `code-entity` / `file` nodes and
//! structural edges. Code structure thus becomes a rebuildable projection over the event log, not
//! a mutable side index. This is the emit half; the fold half lives in `contextgraph::sqlite` and
//! stays compiled in both lanes.

use crate::contextgraph::{
    CodeEntityExtracted, EdgeInferred, TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
};
use crate::eventstore::Event;
use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef, SymbolIndex};

/// Emit the whole index as events: for each file (in the index's sorted path order) its
/// definitions and references, lowered through [`extract_events`]. Deterministic by construction -
/// the index iterates a `BTreeMap`, so identical source yields byte-identical events.
pub fn index_events(idx: &SymbolIndex) -> Vec<Event> {
    idx.files()
        .iter()
        .flat_map(|(path, fs)| extract_events(path, fs))
        .collect()
}

/// Extract the WHOLE project tree at `root` into per-file event batches (spec 29c criterion 5):
/// the production entry point that lowers the ACTUAL project source into the code half of the
/// unified graph, so a live run populates the graph 29a built the machinery for but left with no
/// caller. Reuses the `symbols` grounder's PERSISTED index when one is on disk (a live run's
/// grounder built and persisted it, so this is a cheap read, not a second whole-tree parse) and
/// falls back to a fresh [`build_index`](crate::grounder::symbols::build_index) otherwise. Each
/// file is lowered through the shared [`extract_events`] authority - the SAME per-file emit the
/// fold tests and the incremental path use, so the whole-project ingest can never drift from a
/// single file's. Returns `(file, events)` per file in the index's sorted path order, skipping a
/// file that extracts to nothing (no events, so no batch and no boundary to supersede). The caller
/// keys each batch on its content, so an unchanged file is not re-ingested and a changed one
/// re-extracts.
pub fn project_batches(root: &str) -> Vec<(String, Vec<Event>)> {
    let idx = crate::grounder::symbols::store::load(root)
        .unwrap_or_else(|| crate::grounder::symbols::build_index(root, None));
    idx.files()
        .iter()
        .filter_map(|(path, fs)| {
            let events = extract_events(path, fs);
            (!events.is_empty()).then(|| (path.clone(), events))
        })
        .collect()
}

/// Emit one file's extracted symbols as events: one `CodeEntityExtracted` per definition, then
/// one `EdgeInferred` per reference. Each set is emitted in a sorted, deterministic order (defs by
/// name/line/kind, refs by name/line) so identical source yields byte-identical events - the
/// determinism-by-construction spec 29a requires. Definitions are emitted before references so the
/// fold can land a same-file reference on its already-folded definition entity.
///
/// The FIRST event of the file's batch carries `fresh` (spec 29a criterion 3): it marks the
/// extraction-batch boundary, so the fold supersedes the file's PRIOR structural edges before
/// folding this batch. Re-extracting a changed file therefore REPLACES its structural edges rather
/// than accreting duplicates, while the old edges stay in the graph with `valid_to` stamped (a
/// historical query still reaches them). Which event is first is deterministic (the first
/// definition when the file defines anything, else the first reference), so a refs-only file still
/// supersedes; a file that extracts to nothing emits no events and thus no boundary.
pub fn extract_events(file: &str, fs: &FileSymbols) -> Vec<Event> {
    let lang = lang_str(fs.lang);
    let mut events = Vec::with_capacity(fs.defs.len() + fs.refs.len());

    let mut defs: Vec<&Def> = fs.defs.iter().collect();
    defs.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.line.cmp(&b.line))
            .then_with(|| kind_str(a.kind).cmp(kind_str(b.kind)))
    });
    for d in defs {
        let payload = CodeEntityExtracted {
            file: file.to_string(),
            name: d.name.clone(),
            kind: kind_str(d.kind).to_string(),
            line: d.line,
            lang: lang.to_string(),
            // The first event of the file's batch marks the re-extraction boundary; set below.
            fresh: false,
        };
        events.push(Event::new(
            TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&payload).expect("code-entity payload serializes"),
        ));
    }

    let mut refs: Vec<&SymRef> = fs.refs.iter().collect();
    refs.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));
    for r in refs {
        let payload = EdgeInferred {
            file: file.to_string(),
            name: r.name.clone(),
            lang: lang.to_string(),
            fresh: false,
        };
        events.push(Event::new(
            TYPE_EDGE_INFERRED,
            serde_json::to_vec(&payload).expect("edge payload serializes"),
        ));
    }

    // Stamp the batch boundary onto the FIRST event (a definition if the file defines anything,
    // else the first reference), by re-serializing that one payload with `fresh = true`. Doing it
    // here - after the sorted defs-then-refs order is fixed - keeps the "which event is first"
    // rule in one place and independent of whether the file has definitions. A file that emits no
    // events (extracts to nothing) has no boundary, which is correct: it has no edges to supersede.
    if let Some(first) = events.first_mut() {
        set_fresh(first);
    }

    events
}

/// Re-serialize a code event's payload with `fresh = true`, marking it the extraction-batch
/// boundary. The event is one this module just serialized, so its type and bytes are known-good;
/// this deserializes the payload, flips `fresh`, and re-serializes - keeping the ONE serialization
/// contract (the payload structs) as the single source of the wire form, never hand-patching JSON.
fn set_fresh(e: &mut Event) {
    match e.type_.as_str() {
        TYPE_CODE_ENTITY_EXTRACTED => {
            let mut p: CodeEntityExtracted =
                serde_json::from_slice(&e.data).expect("own code-entity payload round-trips");
            p.fresh = true;
            e.data = serde_json::to_vec(&p).expect("code-entity payload serializes");
        }
        TYPE_EDGE_INFERRED => {
            let mut p: EdgeInferred =
                serde_json::from_slice(&e.data).expect("own edge payload round-trips");
            p.fresh = true;
            e.data = serde_json::to_vec(&p).expect("edge payload serializes");
        }
        other => unreachable!("extract_events emits only code events, got {other}"),
    }
}

/// The lowercase, stable string for a rigger definition `Kind`, carried on the emitted event and
/// folded onto the code-entity node's `kind` attr. A rigger-owned rendering, never a grammar tag.
fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Function => "function",
        Kind::Method => "method",
        Kind::Type => "type",
        Kind::Trait => "trait",
        Kind::Impl => "impl",
        Kind::Module => "module",
        Kind::Constant => "constant",
        Kind::Other => "other",
    }
}

/// The lowercase, stable string for a rigger `Lang`, carried on the emitted event and folded onto
/// the file / entity `lang` attr.
fn lang_str(l: Lang) -> &'static str {
    match l {
        Lang::Rust => "rust",
        Lang::CSharp => "csharp",
        Lang::Js => "js",
        Lang::Ts => "ts",
        Lang::Go => "go",
        Lang::Python => "python",
    }
}

#[cfg(test)]
mod tests {
    use crate::contextgraph::sqlite::Projector;
    use crate::contextgraph::{
        Projection, KIND_CODE_ENTITY, KIND_FILE, REL_CONTAINS, REL_REFERENCES,
        TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
    };
    use crate::grounder::symbols::build_index;
    use crate::grounder::symbols::events::index_events;

    #[test]
    fn a_source_file_extraction_emits_events_the_fold_turns_into_a_code_graph() {
        // Criterion 1, end to end: run the real extraction pass over a source file, emit its
        // definitions and references AS events, fold them, and confirm code structure lives in the
        // projection - a file container node, a code-entity node for the definition, and structural
        // CONTAINS / REFERENCES edges - with no mutable side index in the middle.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage() {}\nfn caller() { apply_damage(); }\n",
        )
        .unwrap();
        let idx = build_index(dir.path().to_str().unwrap(), None);
        let events = index_events(&idx);

        // The extraction pass emitted at least one definition event and one reference event.
        assert!(
            events.iter().any(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED),
            "a definition emits a CodeEntityExtracted event"
        );
        assert!(
            events.iter().any(|e| e.type_ == TYPE_EDGE_INFERRED),
            "a reference emits an EdgeInferred event"
        );

        let p = Projector::open(":memory:", "test").unwrap();
        for (i, mut e) in events.into_iter().enumerate() {
            e.position = (i + 1) as u64;
            p.apply(&e).unwrap();
        }

        let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
        assert!(
            g.nodes
                .iter()
                .any(|n| n.kind == KIND_FILE && n.id == "combat.rs"),
            "the file container node folded from the emitted events; got {:?}",
            g.nodes
        );
        assert!(
            g.nodes
                .iter()
                .any(|n| n.kind == KIND_CODE_ENTITY && n.id == "combat.rs::apply_damage"),
            "the definition folded into a code-entity node; got {:?}",
            g.nodes
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_CONTAINS && e.to == "combat.rs::apply_damage"),
            "a CONTAINS edge ties the file to its definition; got {:?}",
            g.edges
        );
        // The `apply_damage()` call site references the same-file definition, so the REFERENCES
        // edge lands on that definition's entity id.
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_REFERENCES && e.to == "combat.rs::apply_damage"),
            "a REFERENCES edge ties the file to the referenced symbol; got {:?}",
            g.edges
        );
    }

    use crate::eventstore::Event;
    use crate::grounder::symbols::events::extract_events;
    use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef};

    /// Read a code event's `fresh` batch-boundary marker from its raw on-log JSON. The flag is
    /// `skip_serializing_if` when false, so an ABSENT key reads as `false` - the exact wire form a
    /// rebuild replays.
    fn fresh_of(e: &Event) -> bool {
        let v: serde_json::Value = serde_json::from_slice(&e.data).unwrap();
        v.get("fresh").and_then(|f| f.as_bool()).unwrap_or(false)
    }

    #[test]
    fn extract_events_marks_exactly_the_first_event_of_a_files_batch_fresh() {
        // Criterion 3, emit side: the fold supersedes a file's prior structural edges ONLY at the
        // event carrying `fresh`, so the emit pass MUST stamp `fresh` on exactly one event per file
        // - the first - and on nothing else. If it stamped every event, a re-extraction would retire
        // its own freshly-folded edges mid-batch; if it stamped none, a re-extraction would accrete
        // duplicates and never supersede. This pins the boundary to the first event and proves it is
        // the FIRST definition when the file defines anything (definitions emit before references).
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![
                Def {
                    kind: Kind::Function,
                    name: "beta".to_string(),
                    line: 9,
                },
                Def {
                    kind: Kind::Function,
                    name: "alpha".to_string(),
                    line: 3,
                },
            ],
            refs: vec![SymRef {
                name: "helper".to_string(),
                line: 5,
            }],
        };
        let events = extract_events("src/a.rs", &fs);

        // Exactly one event is fresh, and it is the first.
        let fresh_positions: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| fresh_of(e))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            fresh_positions,
            vec![0],
            "exactly the first event of the batch is fresh; got fresh at {fresh_positions:?} over \
             {} events",
            events.len()
        );
        // The first event is a definition (defs emit before refs), so a re-extraction supersedes at
        // the batch boundary before any of the batch's own edges fold.
        assert_eq!(
            events[0].type_, TYPE_CODE_ENTITY_EXTRACTED,
            "the fresh boundary event is the first definition"
        );
        // No other event carries the marker.
        assert!(
            events.iter().skip(1).all(|e| !fresh_of(e)),
            "only the first event of the batch is fresh"
        );
    }

    #[test]
    fn a_refs_only_file_carries_the_batch_boundary_on_its_first_reference() {
        // A file that references symbols but defines NONE still re-extracts, so it still needs the
        // batch boundary - otherwise its stale references would never be superseded. With no
        // definitions the first emitted event is a reference, so `fresh` rides that instead. This
        // proves the boundary is independent of whether the file defines anything.
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![],
            refs: vec![
                SymRef {
                    name: "clamp".to_string(),
                    line: 2,
                },
                SymRef {
                    name: "apply".to_string(),
                    line: 4,
                },
            ],
        };
        let events = extract_events("src/only_refs.rs", &fs);
        assert!(
            !events.is_empty(),
            "a refs-only file emits reference events"
        );
        assert_eq!(
            events[0].type_, TYPE_EDGE_INFERRED,
            "with no definitions the first event is a reference"
        );
        assert!(
            fresh_of(&events[0]),
            "the first reference of a refs-only file carries the batch boundary"
        );
        assert!(
            events.iter().skip(1).all(|e| !fresh_of(e)),
            "only the first event of the batch is fresh"
        );
    }

    #[test]
    fn a_file_that_extracts_to_nothing_emits_no_boundary() {
        // A file with no definitions and no references emits no events, so there is no boundary -
        // correct, because it has no structural edges to supersede. This pins that the boundary is
        // absent (not a spurious empty-payload event) in the degenerate case.
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![],
            refs: vec![],
        };
        let events = extract_events("src/empty.rs", &fs);
        assert!(
            events.is_empty(),
            "a file that extracts to nothing emits no events; got {events:?}"
        );
    }
}
