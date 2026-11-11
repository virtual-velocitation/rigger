//! The design-intent-to-events emit pass (spec 29b): lowers extracted [`DesignConcept`]s into
//! `DocConceptExtracted` events (one per concept, criterion 1) and extracted [`DesignLink`]s into
//! `DocLinkExtracted` events (one per link, criterion 2), which the always-compiled context-graph
//! fold ingests into `design-doc` / `arch-decision` / `handbook-rule` / `rationale` nodes and the
//! five typed design-intent edges. Design intent thus becomes a rebuildable projection over the
//! event log - the design half of the one graph (spec 29a is the code half). This is the emit half;
//! the fold half lives in `contextgraph::sqlite` and stays compiled in both lanes.

use crate::contextgraph::{
    DocConceptExtracted, DocLinkExtracted, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
};
use crate::eventstore::Event;
use crate::grounder::design::model::{DesignConcept, DesignLink};

/// Emit design-intent concepts as events: one `DocConceptExtracted` per concept. Deterministic by
/// construction - the concepts are emitted in a sorted order (by folded node kind, then id) so
/// identical docs yield byte-identical events, and the always-compiled fold turns each into its
/// design-intent node. The concept's kind is lowered through the single [`DesignConcept`] ->
/// `KIND_*` mapping, so the emitted `kind` string is exactly what the fold matches.
pub fn concept_events(concepts: &[DesignConcept]) -> Vec<Event> {
    sorted_events(
        concepts,
        |a, b| {
            a.kind
                .node_kind()
                .cmp(b.kind.node_kind())
                .then_with(|| a.id.cmp(&b.id))
        },
        TYPE_DOC_CONCEPT_EXTRACTED,
        |c| DocConceptExtracted {
            kind: c.kind.node_kind().to_string(),
            id: c.id.clone(),
            title: c.title.clone(),
            doc: c.doc.clone(),
        },
    )
}

/// Emit design-intent links as events: one `DocLinkExtracted` per link (spec 29b criterion 2).
/// Deterministic by construction - the links are emitted in a sorted order (by folded edge relation,
/// then from, then to) so identical docs yield byte-identical events, and the always-compiled fold
/// turns each into its typed design-intent edge. The link's relation is lowered through the single
/// [`LinkRel::rel`](crate::grounder::design::model::LinkRel::rel) mapping, so the emitted `rel`
/// string is exactly what the fold matches.
pub fn link_events(links: &[DesignLink]) -> Vec<Event> {
    sorted_events(
        links,
        |a, b| {
            a.rel
                .rel()
                .cmp(b.rel.rel())
                .then_with(|| a.from.cmp(&b.from))
                .then_with(|| a.to.cmp(&b.to))
        },
        TYPE_DOC_LINK_EXTRACTED,
        |l| DocLinkExtracted {
            from: l.from.clone(),
            to: l.to.clone(),
            rel: l.rel.rel().to_string(),
        },
    )
}

/// `items` in `order`, each lowered to one `type_` event carrying its `payload` - sorting first
/// makes the emit deterministic whatever order the extractor discovered the items in.
fn sorted_events<T, P: serde::Serialize>(
    items: &[T],
    order: impl Fn(&T, &T) -> std::cmp::Ordering,
    type_: &str,
    payload: impl Fn(&T) -> P,
) -> Vec<Event> {
    let mut sorted: Vec<&T> = items.iter().collect();
    sorted.sort_by(|a, b| order(a, b));
    sorted
        .iter()
        .map(|item| {
            Event::new(
                type_,
                serde_json::to_vec(&payload(item)).expect("a design-intent payload serializes"),
            )
        })
        .collect()
}

/// Walk the WHOLE project tree at `root` and extract every file's design intent into per-file event
/// batches (spec 29c criterion 5): the production entry point that lowers the ACTUAL design docs
/// (and inline source rationale) into the design half of the unified graph, so a live run populates
/// the graph 29b built the machinery for but left with no caller. Walks with the SHARED
/// [`walk_guarded`](crate::grounder::walk_guarded) skeleton (the same project scope - the repo's own
/// ignore rules, the always-excluded dotdirs, and root confinement - the grounders and the code
/// ingest use, so the design ingest never diverges on which files count), and lowers each file
/// through [`file_batch`].
/// Returns `(file, events)` per file in SORTED path order (`walk_guarded` visits in sorted file-name
/// order, and the collected batches are sorted for a deterministic emit order), skipping a file that
/// carries no design intent. The caller keys each batch on its content, so an unchanged file is not
/// re-ingested.
pub fn project_batches(root: &str) -> Vec<(String, Vec<Event>)> {
    batches_within(root, None)
}

/// [`project_batches`] for the `named` files alone (an integration's scoped ingest): the SAME walk
/// scope and the SAME per-file [`file_batch`], so a named file yields exactly the batch the whole
/// walk gives it, and a named file the scope excludes (rigger's own `.rigger` dir, an ignored
/// path) or that carries no design intent yields none.
pub fn named_batches(root: &str, named: &[String]) -> Vec<(String, Vec<Event>)> {
    batches_within(root, Some(named))
}

/// The walk behind [`project_batches`] and [`named_batches`]: every file the scoped walk visits
/// (all of them, or only the `named` ones) lowered through [`file_batch`], skipping a file with
/// no design intent, in sorted path order.
fn batches_within(root: &str, named: Option<&[String]>) -> Vec<(String, Vec<Event>)> {
    use std::ops::ControlFlow;
    use std::path::Path;

    let mut batches: Vec<(String, Vec<Event>)> = Vec::new();
    let _ = crate::grounder::walk_guarded_within(Path::new(root), named, &mut |path| {
        let rel = crate::grounder::relative_path(Path::new(root), path);
        let events = file_batch(root, &rel);
        if !events.is_empty() {
            batches.push((rel, events));
        }
        ControlFlow::Continue(())
    });
    batches.sort_by(|a, b| a.0.cmp(&b.0));
    batches
}

/// One file's design-intent events: [`text_batch`] over the file's text, and nothing for an
/// unreadable / binary file (which `read_to_string` rejects).
fn file_batch(root: &str, rel: &str) -> Vec<Event> {
    let Ok(contents) = std::fs::read_to_string(std::path::Path::new(root).join(rel)) else {
        return Vec::new();
    };
    text_batch(rel, &contents)
}

/// The `gd` batch of the file at `path` holding `bytes` (spec 107), total over its input: the
/// batch the walk lowers from that file - [`text_batch`] over UTF-8 bytes, and the empty batch
/// for no bytes and for bytes that are not UTF-8, as [`file_batch`] answers a file it cannot
/// read. The walk's flag changes no `gd` batch.
pub fn bytes_batch(path: &str, bytes: Option<&[u8]>, _excluded: bool) -> Vec<Event> {
    crate::grounder::text_of(bytes)
        .map(|text| text_batch(path, text))
        .unwrap_or_default()
}

/// The design-intent events of the file at `rel` holding `text` (spec 107), through the shared
/// `extract_concepts` / `extract_links` scope-gated authority: a design doc yields concept then
/// link events, a source file its `# WHY:` / `# NOTE:` rationale, and a usage doc or text with no
/// design intent nothing. It reads no file, so the same text yields the same batch wherever the
/// text came from.
fn text_batch(rel: &str, text: &str) -> Vec<Event> {
    use crate::grounder::design::extract::{extract_concepts, extract_links};

    let mut events = concept_events(&extract_concepts(rel, text));
    events.extend(link_events(&extract_links(rel, text)));
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::design::extract::{extract_concepts, extract_links};

    /// Emitting `items` and their reverse yields byte-identical events - the emit order is
    /// independent of the order the extractor discovered them in.
    fn assert_emit_ignores_input_order<T: Clone>(
        items: Vec<T>,
        emit: fn(&[T]) -> Vec<Event>,
        why: &str,
    ) {
        let a = emit(&items);
        let mut shuffled = items;
        shuffled.reverse();
        let b = emit(&shuffled);
        let bytes = |es: &[Event]| es.iter().map(|e| e.data.clone()).collect::<Vec<_>>();
        assert_eq!(bytes(&a), bytes(&b), "{why}");
    }

    crate::test_cases! {
        // Determinism by construction (spec 29b): identical concepts yield byte-identical events,
        // in a sorted order independent of the order the extractor discovered them.
        the_emit_is_deterministic_and_sorts_by_kind_then_id: assert_emit_ignores_input_order(
            extract_concepts("docs/architecture.md", "# Title\n\n## Zeta\n\n## Alpha\n"),
            concept_events,
            "emit order is independent of input order",
        );
        // Determinism by construction (spec 29b): identical links yield byte-identical events, in
        // a sorted order independent of the order the extractor discovered them.
        the_link_emit_is_deterministic_and_sorts_by_rel_then_from_then_to: assert_emit_ignores_input_order(
            extract_links(
                "docs/architecture.md",
                "# Title\n\nUses `src/z.rs`, `src/a.rs`; see [x](docs/x.md).\n",
            ),
            link_events,
            "link emit order is independent of input order",
        );
    }

    /// THE EXTRACTION READS BYTES (spec 107): `text_batch` answers, for the text of each file of
    /// the extraction tree that carries design intent, the batch `file_batch` lowers from that
    /// file on disk - the design document's two concepts then its two links, and each source
    /// file's rationale with the link that explains its file.
    #[test]
    fn text_batch_answers_the_batch_file_batch_lowers_from_each_file_of_the_extraction_tree() {
        use crate::extraction_tree::{
            planted_extraction_tree, walked_batch, DOCUMENT_BODY, DOCUMENT_PATH, SOURCE_BODY,
            SOURCE_PATH, TEST_MODULE_BODY, TEST_MODULE_PATH,
        };
        use crate::test_support::wire;

        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let root = dir.path().to_str().unwrap();

        for (path, body) in [
            (DOCUMENT_PATH, DOCUMENT_BODY),
            (TEST_MODULE_PATH, TEST_MODULE_BODY),
            (SOURCE_PATH, SOURCE_BODY),
        ] {
            let expected = walked_batch("gd", path).to_vec();
            assert_eq!(
                wire(&text_batch(path, body)),
                expected,
                "the text of {path}"
            );
            assert_eq!(wire(&file_batch(root, path)), expected, "the file {path}");
        }
    }

    /// Text with no design intent lowers to the empty batch, as does a file `file_batch` cannot
    /// read: neither is a batch the walk keeps.
    #[test]
    fn text_batch_is_empty_for_text_without_design_intent_as_file_batch_is_for_no_file() {
        use crate::test_support::wire;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plain.rs"), "fn plain() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();

        assert_eq!(wire(&text_batch("plain.rs", "fn plain() {}\n")), vec![]);
        assert_eq!(wire(&file_batch(root, "plain.rs")), vec![]);
        assert_eq!(wire(&file_batch(root, "absent.md")), vec![]);
    }

    /// THE REBUILD RE-EXTRACTS THE LEDGER (spec 107): `bytes_batch` over a `gd` file's UTF-8
    /// bytes answers the batch the walk lowers from that file, whatever the flag says, and the
    /// empty batch for no bytes and for bytes that are not UTF-8.
    #[test]
    fn bytes_batch_answers_the_walks_batch_for_utf8_bytes_and_the_empty_batch_otherwise() {
        use crate::extraction_tree::{walked_batch, DOCUMENT_BODY, DOCUMENT_PATH};
        use crate::test_support::wire;

        let expected = walked_batch("gd", DOCUMENT_PATH).to_vec();
        for excluded in [false, true] {
            assert_eq!(
                wire(&bytes_batch(
                    DOCUMENT_PATH,
                    Some(DOCUMENT_BODY.as_bytes()),
                    excluded
                )),
                expected,
                "excluded {excluded}"
            );
        }
        // A design document's title, cut inside a UTF-8 sequence.
        let not_utf8: &[u8] = &[b'#', b' ', b'A', 0xe2, 0x82];
        assert_eq!(wire(&bytes_batch(DOCUMENT_PATH, None, false)), vec![]);
        assert_eq!(
            wire(&bytes_batch(DOCUMENT_PATH, Some(not_utf8), false)),
            vec![]
        );
    }
}
