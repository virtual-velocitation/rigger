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
use std::borrow::Cow;
use std::collections::BTreeSet;

/// Emit the whole index as events: for each file (in the index's sorted path order) its
/// definitions and references, lowered through [`extract_events`]. Deterministic by construction -
/// the index iterates a `BTreeMap`, so identical source yields byte-identical events. A file
/// [`out_of_line_test_module_files`] resolves as the target of a `#[cfg(test)] mod name;`
/// declaration elsewhere in `idx` still contributes a batch (round 5,
/// adj-u86c3-r4-out-of-line-exclusion-still-unmigrated): its symbols are hollowed via
/// [`for_extraction`] before the SAME [`extract_events`] every other file goes through runs on it,
/// so that function's own already-existing empty-survivor boundary (spec 86 criterion 3) retires
/// its prior structural edges too - never a second, caller-level sentinel path that drops the file
/// before `extract_events` ever sees it, which is exactly the defect that let a legacy test-entity
/// node reached only through an out-of-line declaration survive every re-ingest forever. Evidence
/// (`proof_events`) is a separate concern with its own disclosed, non-blocking gap for this same
/// shape - see that function's own doc - so it is skipped for an excluded file here, unchanged.
pub fn index_events(idx: &SymbolIndex) -> Vec<Event> {
    let excluded = out_of_line_test_module_files(idx);
    idx.files()
        .iter()
        .flat_map(|(path, fs)| {
            let is_excluded = excluded.contains(path.as_str());
            let mut events = extract_events(path, &for_extraction(fs, is_excluded));
            // Spec 86 criterion 2: alongside (never inside) the structural pass, emit this file's
            // TEST-ORIGIN evidence too - except for an out-of-line-excluded file, which never
            // reaches this call (see this function's own doc and [`proof_events`]'s disclosed
            // gap). See [`proof_events`]'s own doc for why this rides a separate function rather
            // than folding into `extract_events` itself.
            if !is_excluded {
                events.extend(proof_events(path, fs));
            }
            events
        })
        .collect()
}

/// Round 5 (adj-u86c3-r4-out-of-line-exclusion-still-unmigrated): the symbols [`extract_events`]
/// actually sees for one file - `fs` unchanged, UNLESS `excluded` (this file is the resolved
/// target of an out-of-line `#[cfg(test)] mod name;` declaration elsewhere,
/// [`out_of_line_test_module_files`]), in which case its defs/refs are hollowed to empty. An
/// out-of-line target's OWN items are never marked `is_test` themselves - that flag is set by a
/// LOCAL `#[cfg(test)]` attribute stack inside the SAME file, and being the resolved target of an
/// EXTERNAL declaration elsewhere is invisible to a per-file parse - so hollowing here, at the ONE
/// place both callers already compute the excluded set, is what lets `extract_events`'s own
/// existing "nothing survived" branch return the empty-boundary sentinel for it, exactly as it
/// already does for a whole `tests/`-dir file or an in-file `#[cfg(test)]` re-wrap - never a
/// second, bespoke sentinel-construction path for a third shape. `Cow` so the overwhelmingly
/// common (non-excluded) path borrows `fs` unchanged rather than cloning it.
fn for_extraction(fs: &FileSymbols, excluded: bool) -> Cow<'_, FileSymbols> {
    if excluded {
        Cow::Owned(FileSymbols {
            lang: fs.lang,
            defs: Vec::new(),
            refs: Vec::new(),
        })
    } else {
        Cow::Borrowed(fs)
    }
}

/// Extract the WHOLE project tree at `root` into per-file event batches (spec 29c criterion 5):
/// the production entry point that lowers the ACTUAL project source into the code half of the
/// unified graph, so a live run populates the graph 29a built the machinery for but left with no
/// caller. Reuses the `symbols` grounder's PERSISTED index when one is on disk (a live run's
/// grounder built and persisted it, so this is a cheap read, not a second whole-tree parse) and
/// falls back to a fresh [`build_index`](crate::grounder::symbols::build_index) otherwise. Each
/// file is lowered through the shared [`extract_events`] authority - the SAME per-file emit the
/// fold tests and the incremental path use, so the whole-project ingest can never drift from a
/// single file's. Returns `(file, events)` per file in the index's sorted path order; EVERY file -
/// an out-of-line test-module target ([`for_extraction`]-hollowed) included, round 5 - contributes
/// a batch, and that batch is NEVER empty ([`extract_events`] itself never returns empty, spec 86
/// criterion 3: it stamps its own boundary-only sentinel when nothing real survives, so a file
/// whose structural set is empty still contributes at least one boundary event). The caller keys
/// each batch on its content, so an unchanged file is not re-ingested and a changed one
/// re-extracts.
pub fn project_batches(root: &str) -> Vec<(String, Vec<Event>)> {
    project_batches_paced(root, crate::parallel::default_workers()).0
}

/// [`project_batches`] at a chosen parse width, also reporting how many worker threads engaged. The
/// per-file lowering fans across up to `workers` threads via [`crate::parallel::map_ordered`], but
/// the batches come back in the index's SORTED path order (index-preserving), so the emit is
/// byte-identical to a serial walk's however the pool interleaved. `workers <= 1` runs the lowering
/// inline - the serial walk a wider walk is compared against. EVERY file - an out-of-line
/// test-module target ([`for_extraction`]-hollowed, round 5,
/// adj-u86c3-r4-out-of-line-exclusion-still-unmigrated) included - is lowered through the ONE
/// [`extract_events`] authority (never a second parallel copy), which never returns empty (spec 86
/// criterion 3), so every file's own batch is never empty. [`proof_events`] runs alongside it for a
/// non-excluded file only, mirroring [`index_events`]'s identical composition - see that function's
/// own doc and [`proof_events`]'s disclosed gap. Returns `(batches, workers_engaged)`.
pub fn project_batches_paced(root: &str, workers: usize) -> (Vec<(String, Vec<Event>)>, usize) {
    let idx = crate::grounder::symbols::store::load(root)
        .unwrap_or_else(|| crate::grounder::symbols::build_index(root, None));
    let excluded = out_of_line_test_module_files(&idx);
    let files: Vec<(&String, &FileSymbols)> = idx.files().iter().collect();
    // Parse/lower per file in parallel; `map_ordered` returns the per-file results in the input
    // (sorted-path) order, so the emit sequence is independent of which worker finished first.
    crate::parallel::map_ordered(&files, workers, |&(path, fs)| {
        let is_excluded = excluded.contains(path.as_str());
        let mut events = extract_events(path, &for_extraction(fs, is_excluded));
        // Spec 86 criterion 2: this file's test-origin evidence, alongside its structural
        // events - see [`index_events`]'s identical composition and [`proof_events`]'s doc. Never
        // empty (round 3) when run, so a non-excluded file's batch is never dropped; skipped for
        // an out-of-line-excluded file (see this module's [`for_extraction`] doc).
        if !is_excluded {
            events.extend(proof_events(path, fs));
        }
        (path.clone(), events)
    })
}

/// Scoped counterpart to [`project_batches`]/[`project_batches_paced`] (spec 92, FRESH ON EVERY
/// INTEGRATION): extract only the NAMED `files`' events, rather than walking the whole persisted
/// index - the property an integration's own reindex needs, bounded by the merge's own file list
/// (Design/Constraints Walk) rather than the project's total file count. Reuses the SAME persisted-
/// index load, exclusion computation, and `extract_events`/`proof_events` authority as
/// `project_batches_paced` - never a second lowering path - so a named file's scoped batch is
/// byte-identical to what a full walk would produce for it. Returns `(file, events)` pairs in the
/// SAME order `files` was given, one pair per named file (never fewer): a file the index holds no
/// entry for (deleted since the index was last built, or never source) still contributes exactly
/// [`empty_structural_boundary_event`]'s single-event batch - the SAME boundary sentinel a file that
/// "extracts to nothing" stamps (spec 86 criterion 3) - so its prior structural edges retire through
/// the existing supersession rather than dangling forever, mirroring `extract_events`'s own "never
/// returns empty" contract for the whole-project walk.
pub fn file_batches(root: &str, files: &[String]) -> Vec<(String, Vec<Event>)> {
    let idx = crate::grounder::symbols::store::load(root)
        .unwrap_or_else(|| crate::grounder::symbols::build_index(root, None));
    let excluded = out_of_line_test_module_files(&idx);
    files
        .iter()
        .map(|file| {
            let events = match idx.files().get(file) {
                Some(fs) => {
                    let is_excluded = excluded.contains(file.as_str());
                    let mut events = extract_events(file, &for_extraction(fs, is_excluded));
                    if !is_excluded {
                        events.extend(proof_events(file, fs));
                    }
                    events
                }
                // Absent from the index: deleted or unreadable since the index was last freshened
                // (or never source at all). `lang` is immaterial here - the fold's supersede keys
                // on `file` alone - so this never has to guess or re-derive it.
                None => vec![empty_structural_boundary_event(file, "unknown")],
            };
            (file.clone(), events)
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
/// definition when the file defines anything, else the first reference) - EXCEPT when the file's
/// surviving definition/reference set is itself empty (spec 86 criterion 3, THE MIGRATION IS
/// DELIBERATE): rather than emit no events and thus no boundary, this now returns exactly
/// [`empty_structural_boundary_event`] as the file's WHOLE batch, so re-extracting a file down to
/// nothing STILL supersedes its prior structural edges - see that function's own doc for why this
/// changed and what it fixes. `extract_events` therefore never returns an empty `Vec`, mirroring
/// [`proof_events`]'s own identical round-3 rule for evidence.
///
/// Spec 86 criterion 1 (TESTS ARE NOT NODES): this is "the code-entity pass" the criterion names,
/// so the exclusion rule lives here, at the ONE place both what a file's definitions/references
/// ARE (`fs`) and where the file LIVES (`file`) are in hand together. Two exclusions, applied in
/// this order:
///
/// 1. **Whole file under a `tests/` directory** ([`is_under_tests_dir`]): no `CodeEntityExtracted`
///    and no `EdgeInferred` for any item the file defines or references, product-shaped or not.
///    `fs` is left untouched (still PARSED, for grounding); this file contributes no REAL
///    entity or edge, exactly like a file that extracts to nothing - it still stamps
///    [`empty_structural_boundary_event`] as its whole batch (spec 86 criterion 3), never a
///    genuine `CodeEntityExtracted`/named `EdgeInferred`.
/// 2. **A `#[test]`/`#[cfg(test)]` region inside an otherwise-included file**
///    ([`crate::grounder::symbols::model::Def::is_test`] /
///    [`crate::grounder::symbols::model::SymRef::is_test`], computed once at extraction time):
///    that ONE definition or reference is skipped, so a product file's own entities and edges
///    still emit normally around it.
///
/// Either way, excluded code creates no NODE and no structural edge "on the canvas" - the graph
/// holds the product's own structure, never the tests that prove it. (The EVIDENCE those excluded
/// references carry - `proven_by` counts and their `file:line`s - is a separate concern this
/// criterion does not own.)
pub fn extract_events(file: &str, fs: &FileSymbols) -> Vec<Event> {
    let lang = lang_str(fs.lang);
    if is_under_tests_dir(file) {
        return vec![empty_structural_boundary_event(file, lang)];
    }
    let mut events = Vec::with_capacity(fs.defs.len() + fs.refs.len());

    let mut defs: Vec<&Def> = fs.defs.iter().filter(|d| !d.is_test).collect();
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

    let mut refs: Vec<&SymRef> = fs.refs.iter().filter(|r| !r.is_test).collect();
    refs.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));
    for r in refs {
        let payload = EdgeInferred {
            file: file.to_string(),
            name: r.name.clone(),
            lang: lang.to_string(),
            // The first event of the file's batch marks the re-extraction boundary; set below.
            fresh: false,
            // The enclosing definition this reference was attributed to during extraction (spec
            // 37): the caller, same-file. Read straight off the `SymRef` in the already-fixed
            // sorted order, never a new sort key, so the deterministic refs emission is unchanged;
            // `None` (a top-level reference) is omitted on the wire, keeping the event
            // byte-identical to the pre-37 form.
            caller: r.enclosing.clone(),
            // A STRUCTURAL reference's edge carries no line (spec 86 criterion 2 introduces `line`
            // for evidence only); leaving it 0 keeps this event's wire form byte-identical to
            // before the field existed.
            line: 0,
            // This is a structural reference, never evidence - `proof_events` (below) is the one
            // emitter of `is_test: true` events.
            is_test: false,
        };
        events.push(Event::new(
            TYPE_EDGE_INFERRED,
            serde_json::to_vec(&payload).expect("edge payload serializes"),
        ));
    }

    // Stamp the batch boundary onto the FIRST event (a definition if the file defines anything,
    // else the first reference), by re-serializing that one payload with `fresh = true`. Doing it
    // here - after the sorted defs-then-refs order is fixed - keeps the "which event is first"
    // rule in one place and independent of whether the file has definitions.
    //
    // Spec 86 criterion 3 (THE MIGRATION IS DELIBERATE): a file whose SURVIVING set is empty (an
    // edit removed its last definition and reference, or every item it held was `is_test` and got
    // filtered above) used to emit NOTHING here - correct on a file's very first extraction (there
    // is genuinely nothing to supersede yet), but silently WRONG for a file re-extracting DOWN to
    // empty: its whole batch (this function's own former `Vec::new()`) was dropped before ever
    // reaching a sink, so the fold's `supersede_file_edges` never ran and the file's PRIOR live
    // structural edges - a legacy test-entity node's own `CONTAINS`/`REFERENCES`/`CALLS` edge
    // included, the exact shape a store that predates criterion 1's exclusion rule holds - stayed
    // live forever. Mirroring [`proof_events`]'s own identical round-3 fix for evidence, this now
    // stamps [`empty_structural_boundary_event`] as the file's WHOLE batch instead of nothing: an
    // ordinary `is_test: false` `EdgeInferred` with an EMPTY `name` (never a real reference's
    // name), always `fresh`. The fold's `TYPE_EDGE_INFERRED` arm still runs `supersede_file_edges`
    // on ANY `fresh` event before looking at what else it carries, so this sentinel's supersede
    // call retires the file's prior structural edges exactly as a genuine re-extraction would -
    // while its own empty-name guard recognizes the sentinel and creates no node, no `KIND_FILE`
    // container, and no edge of its own (see the fold arm's own doc). A no-op on the file's very
    // first extraction (nothing yet to supersede), so the degenerate "genuinely empty product
    // file" case costs one harmless no-op event, never a behavior change an operator would notice.
    if let Some(first) = events.first_mut() {
        set_fresh(first);
    } else {
        events.push(empty_structural_boundary_event(file, lang));
    }

    events
}

/// Spec 86 criterion 3 (THE MIGRATION IS DELIBERATE): the STRUCTURAL batch-boundary sentinel for a
/// file whose surviving (non-`is_test`, non-`tests/`-dir) definition and reference set is EMPTY -
/// riding the existing `EdgeInferred` shape with an EMPTY `name` (never a real reference's name)
/// and `is_test: false` (never [`proof_events`]'s own EVIDENCE sentinel,
/// [`empty_evidence_boundary_event`], which sets `is_test: true` and is this function's evidence-
/// side twin) marking it boundary-only. Always `fresh`: this is ALWAYS a file's WHOLE structural
/// batch (never returned alongside a real definition or reference - [`extract_events`]'s own
/// end-of-function stamping marks a genuine survivor `fresh` instead), so it is unconditionally
/// the batch's first and only event.
///
/// This is what makes "a file that now extracts to NOTHING... still stamps one [boundary]" (spec
/// 86 Design, MIGRATION) true: the fold's `TYPE_EDGE_INFERRED` arm runs `supersede_file_edges` on
/// ANY `fresh` event, so this sentinel's supersede call retires every LIVE structural edge this
/// file's PRIOR extraction left - while its own empty-name guard recognizes the sentinel and
/// creates no node, no `KIND_FILE` container, and no edge (the fold's "never a node and never an
/// edge on the canvas" promise for excluded content, spec 86 criterion 1, holds through the
/// migration too). Two callers reach this: a WHOLE `tests/`-dir file ([`is_under_tests_dir`], the
/// central migration case named in the Design - every entity it ever held was test code) and the
/// general "extracted to nothing" case at the end of [`extract_events`] (whatever survived
/// filtering, if anything, amounted to zero definitions and zero references). Both are ONE rule,
/// never two: [`extract_events`] never returns an empty `Vec`.
fn empty_structural_boundary_event(file: &str, lang: &str) -> Event {
    let payload = EdgeInferred {
        file: file.to_string(),
        name: String::new(),
        lang: lang.to_string(),
        fresh: true,
        caller: None,
        line: 0,
        is_test: false,
    };
    Event::new(
        TYPE_EDGE_INFERRED,
        serde_json::to_vec(&payload).expect("edge payload serializes"),
    )
}

/// Spec 86 criterion 2 (PROOF LANDS ON THE CARD): the TEST-EVIDENCE emission pass, run ALONGSIDE
/// (never inside) [`extract_events`] at every one of its callers. It is a SEPARATE function,
/// deliberately, rather than a change to `extract_events` itself: criterion 1's own frozen tests
/// pin an EXACT emitted-event count/order for a fixture mixing product and test content (e.g.
/// [`tests::extract_events_skips_is_test_items_and_the_fresh_boundary_lands_on_the_first_survivor`]
/// asserts the two `is_test` items "emit nothing" - a literal count over `extract_events`'s own
/// return value), so criterion 1's exclusion contract is that a dropped test item never grows that
/// function's event list. This function reads the SAME already-parsed `fs.refs` (never a second
/// parse) and independently emits the evidence `extract_events` deliberately does not, so the two
/// contracts - "extract_events emits nothing for excluded content" and "no evidence is ever lost" -
/// both hold, in two disjoint functions rather than one straining to prove both at once.
///
/// A reference counts as evidence when EITHER: the whole file is under a `tests/` directory
/// ([`is_under_tests_dir`] - every reference in an out-of-tree test file is test-origin, whatever
/// its own `is_test` marking, since the file itself is nothing but test code), OR the reference
/// itself is `is_test` (a `#[cfg(test)]`/`#[test]` region inside an otherwise-included product
/// file - the common in-file `mod tests` idiom). A DEFINITION is never evidence (only a reference,
/// a USE of a product entity, proves it); a product (non-test) reference in an included file is
/// skipped here (it already emits normally through `extract_events`).
///
/// Each emitted event is an ordinary [`EdgeInferred`] with [`EdgeInferred::is_test`] set and
/// [`EdgeInferred::line`] carrying the reference's own source line - the fold
/// (`contextgraph::sqlite`'s `TYPE_EDGE_INFERRED` arm) reads that marker and folds the evidence
/// onto the referenced entity's `proven_by` attrs directly, never a `file` node, never a
/// `REFERENCES`/`CALLS` edge - so criterion 1's "never a node and never an edge on the canvas"
/// promise holds for evidence exactly as it holds for the exclusion itself. `caller` is left at
/// its default: evidence carries no caller-attribution semantics of its own.
///
/// `fresh` (round 2,
/// adv-u86c2-r-test-file-re-extraction-double-counts-its-own-unchanged-references): the FIRST
/// event THIS function returns is stamped via the SAME [`set_fresh`] `extract_events` uses,
/// marking the boundary of THIS file's own evidence batch. Re-extraction supersession is
/// criterion 2's OWN mechanism for evidence (`contextgraph::sqlite::supersede_file_proof`),
/// distinct from `extract_events`'s structural boundary on the SAME event list (`index_events`/
/// `project_batches_paced` concatenate both, so a file can carry two independent boundaries - one
/// per concern) - without it, editing a test file (adding an unrelated test, fixing a comment)
/// re-extracts the whole file and re-records every unchanged is_test reference as brand-new
/// evidence, permanently inflating `proven_by`.
///
/// Round 3 (adv-u86c2-r2-deleted-test-reference-strands-proof-forever): a file whose evidence set
/// is EMPTY - the overwhelming common case for an ordinary product file with no is_test content at
/// all, but ALSO the ordinary case of deleting or rewriting the one test that proved something -
/// used to return an empty `Vec` here, so this function stamped no boundary. That was fine for a
/// file that NEVER had evidence, but silently wrong for one TRANSITIONING from evidence to none: a
/// file's WHOLE batch (`extract_events` alongside this function, concatenated by `index_events`/
/// `project_batches_paced`) can itself be empty - a `tests/`-dir file's structural side is ALWAYS
/// empty - so the file's batch was dropped entirely and `fold_test_evidence`/`supersede_file_proof`
/// never even ran, stranding this file's own prior `proof_evidence` contribution on whichever
/// entities it named, forever. Mirroring criterion 3's own already-established empty-after-
/// exclusion pattern for `extract_events`'s structural boundary (a file that extracts to nothing
/// still stamps ONE boundary event rather than being skipped), this function now NEVER returns an
/// empty `Vec`: an empty evidence set returns [`empty_evidence_boundary_event`] as the file's WHOLE
/// batch instead - an ordinary `is_test` `EdgeInferred`, always `fresh`, but with an EMPTY `name`
/// (never a real reference's name) marking it boundary-only. The fold's `is_test` branch
/// (`fold_test_evidence`) still runs on it - so the supersede-on-re-extract retraction above still
/// fires - but its own empty-name guard resolves/records nothing for it.
///
/// Sorted by name then line (mirroring `extract_events`'s own ref ordering), so identical source
/// yields byte-identical evidence events regardless of parse order.
///
/// Disclosed, non-blocking scope limit (mirrors criterion 1's own disclosed Go-language gap): a
/// file pulled in only by an OUT-OF-LINE `#[cfg(test)] mod name;` declaration elsewhere
/// ([`out_of_line_test_module_files`]) still contributes a STRUCTURAL batch since round 5
/// (`extract_events` runs on it hollowed, via `index_events`/`project_batches_paced`'s own
/// [`for_extraction`] - see either function's doc), but this function is deliberately never called
/// for it (its own references carry no `is_test` marking of their own - the attribute lives on the
/// DECLARING file's side, invisible to this per-file view, per [`out_of_line_test_module_files`]'s
/// own doc), so a test-only file reached only that way contributes no evidence. Not named by spec
/// 86's Design/Done-when text, which is written in terms of a `tests/` directory and
/// `#[cfg(test)]`/`#[test]` regions.
///
/// Disclosed, non-blocking, SHARED limitation (not this criterion's alone to close): like
/// criterion 3's identical structural sentinel, [`empty_evidence_boundary_event`]'s payload is a
/// CONSTANT per `(file, lang)` with no generation-distinguishing field, so within one long-lived
/// process the ingest replay-key dedup (`crate::ingest::key_batch`, content-hashing a file's WHOLE
/// batch) can treat a LATER occurrence of an all-empty batch as a replay of an EARLIER one and
/// silently drop it. This is the same collision class already tracked against criterion 3's own
/// sentinel (a peer finding on that unit); fixing it belongs to `key_batch`/`emit_keyed_batch`
/// (shared ingest infrastructure both sentinels ride), not to a bespoke, duplicated workaround in
/// either criterion's own emit function.
pub fn proof_events(file: &str, fs: &FileSymbols) -> Vec<Event> {
    let whole_file_test = is_under_tests_dir(file);
    let lang = lang_str(fs.lang);
    let mut refs: Vec<&SymRef> = fs
        .refs
        .iter()
        .filter(|r| whole_file_test || r.is_test)
        .collect();
    refs.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));
    if refs.is_empty() {
        return vec![empty_evidence_boundary_event(file, lang)];
    }
    let mut events: Vec<Event> = refs
        .into_iter()
        .map(|r| {
            let payload = EdgeInferred {
                file: file.to_string(),
                name: r.name.clone(),
                lang: lang.to_string(),
                // The first event of THIS function's own return value marks ITS OWN batch
                // boundary; set below via the SAME `set_fresh` helper `extract_events` uses.
                fresh: false,
                caller: None,
                line: r.line,
                is_test: true,
            };
            Event::new(
                TYPE_EDGE_INFERRED,
                serde_json::to_vec(&payload).expect("evidence payload serializes"),
            )
        })
        .collect();
    if let Some(first) = events.first_mut() {
        set_fresh(first);
    }
    events
}

/// Spec 86 criterion 2, round 3 (adv-u86c2-r2-deleted-test-reference-strands-proof-forever): the
/// batch-boundary sentinel for a file whose TEST-EVIDENCE set is empty - riding the existing
/// `EdgeInferred` shape with `is_test: true` (so the fold's existing `is_test` branch routes it to
/// `fold_test_evidence`, never a new event type or a new fold arm) and an EMPTY `name` (never a
/// real reference's name) marking it boundary-only. Always `fresh`: used only as a file's WHOLE
/// evidence batch (never alongside a real reference the ordinary stamping in [`proof_events`]
/// would mark instead), so it is always that batch's first and only event. The fold
/// (`contextgraph::sqlite::fold_test_evidence`) recognizes the empty name and runs ONLY the
/// supersede-on-re-extract retraction (`supersede_file_proof`), resolving/recording nothing for
/// it - mirroring the identical "recognize the empty discriminator, do nothing but supersede"
/// contract criterion 3's own structural boundary sentinel uses.
fn empty_evidence_boundary_event(file: &str, lang: &str) -> Event {
    let payload = EdgeInferred {
        file: file.to_string(),
        name: String::new(),
        lang: lang.to_string(),
        fresh: true,
        caller: None,
        line: 0,
        is_test: true,
    };
    Event::new(
        TYPE_EDGE_INFERRED,
        serde_json::to_vec(&payload).expect("evidence payload serializes"),
    )
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

/// Spec 86 criterion 1: whether `file` (a '/'-separated, project-relative path, matching what
/// [`extract_events`]'s `file` parameter and every batch key already carry) is UNDER a `tests/`
/// directory - the Rust (and this repo's own) convention for out-of-tree integration test
/// sources, distinct from an in-file `#[cfg(test)]` module. Matched on DIRECTORY components only
/// (every segment except the file's own name), so `tests/foo.rs` and `crate/tests/bar.rs` are
/// excluded while `src/testsuite.rs` (a product file that merely reads close to "tests") is not -
/// the file's own component is never compared against the exact segment `"tests"`.
///
/// `pub(crate)`, not `fn`: the CONSTRAINTS WALK amendment to spec 86 assigns a second consumer to
/// this SAME rule - a design doc's inline-code mention of a `tests/`-rooted path is excluded from
/// the design-intent link pass too, "by the SAME exclusion rule... it needs no criterion of its
/// own because criterion 1's fixture-proven exclusion is the one rule both extraction passes
/// obey." [`crate::grounder::design::extract::doc_links`] reuses this exact function rather than
/// re-deriving the directory-component check a second time.
pub(crate) fn is_under_tests_dir(file: &str) -> bool {
    let mut segments: Vec<&str> = file.split('/').collect();
    segments.pop(); // the file's own name is never a directory component
    segments.contains(&"tests")
}

/// The directory component of a project-relative `path` (every segment except the file's own
/// name) - the ONE place this repo splits a path this way, shared by [`module_dir`] and
/// [`resolve_out_of_line_target`]'s `#[path]`-override branch, so the split-off-the-last-`/`
/// idiom exists exactly once rather than twice with the identical shape.
fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// Resolves `.` and `..` components, at ANY position, in a `/`-separated LOGICAL path string - the
/// ONE place [`resolve_out_of_line_target`]'s `#[path]`-override branch normalizes a raw attribute
/// value joined onto a directory before matching it against [`SymbolIndex::files`]'s keys. Those
/// keys are always the project's own already-clean relative paths (never containing a literal `.`
/// or `..` segment), so `std::fs::canonicalize` does not apply here - there is no real filesystem
/// to resolve against at every intermediate step, only a string key to compute, matching rustc's
/// own purely lexical handling of a `#[path]` value. A `.` segment is dropped; a `..` segment pops
/// the most recently pushed real segment off the stack.
///
/// Round 9 (ADJUDICATION `adj-u86c1-r8-verdict-reject`, upheld finding
/// `adv-u86c1-r8-overcount-dotdot-silently-excludes-an-unrelated-real-file`): a `..` reached once
/// the stack is already EMPTY - an override whose `..` count exceeds the declaring directory's own
/// depth - is an IRRECOVERABLE overflow, not a harmless one to keep walking past. `Vec::pop` on an
/// empty stack is Rust's own silent no-op with no signal to the caller; continuing the walk after
/// one and joining whatever segments remain computes a string that is syntactically a valid
/// relative path but semantically WRONG (it claims to walk back further than the declaring file's
/// own tree allows) - and nothing stops that wrong-but-plausible string from coincidentally
/// matching a real, UNRELATED file's key, at which point the caller's `contains_key` check finds a
/// match and silently drops that unrelated file's whole entity set from the graph. So this now
/// returns `None` the instant a `..` has nothing real left to pop, letting the caller short-circuit
/// to "unresolvable" immediately rather than running a lookup against a value that only LOOKS like
/// a real path.
fn normalize_logical_path(path: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            real => stack.push(real),
        }
    }
    Some(stack.join("/"))
}

/// Round 9 (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`,
/// NORMALIZATION rule): whether a raw `#[path]` override value `p` is a shape
/// [`normalize_logical_path`] must never be asked to join onto any base directory at all - an
/// OS-absolute path, a URI scheme, or a Windows drive letter, none of which a real project's
/// `idx.files()` (always `/`-separated, repo-relative, drive-and-scheme-free keys) could ever
/// hold. Verified against real rustc before this was written: `#[path = "/etc/hostname"]` reads
/// `/etc/hostname` directly, an OS-absolute path used AS-IS rather than joined onto anything.
/// Checked on the RAW override, before joining onto the base directory: joining first and only
/// then normalizing would let a leading `/` on `p` silently vanish via the SAME
/// `"" | "." => {}` rule that drops an ordinary `.` segment (an empty split segment either way),
/// reproducing the identical wrong-but-plausible-collapsed-string failure
/// [`normalize_logical_path`]'s own `..`-overflow guard closes, through a different front door -
/// most sharply for a ROOT-level declaring file, whose empty base directory lets the join
/// degenerate to `p` completely unchanged.
fn path_override_names_an_unresolvable_shape(p: &str) -> bool {
    if p.starts_with('/') {
        return true;
    }
    if p.contains("://") {
        return true;
    }
    let bytes = p.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Round 7 (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): the MODULE DIRECTORY a
/// declaring file `path`'s own out-of-line children resolve under, per Rust's real file-per-module
/// convention - never simply `path`'s own directory (the round-6 bug,
/// `sdet-u86c1-r6-out-of-line-mod-resolution-uses-declaring-files-directory-not-rusts-own-module-
/// nesting-path`). A directory-style file (`mod.rs`, `lib.rs`, `main.rs` - a crate root or a
/// directory module's own body-carrying file) owns its OWN directory: its `mod name;` children are
/// same-directory siblings. Every OTHER file is itself a leaf submodule and, per Rust's convention,
/// puts ITS OWN children in a NEW subdirectory named after its own stem (`foo.rs` -> `foo/`) -
/// never a same-directory sibling.
fn module_dir(path: &str) -> String {
    let dir = dir_of(path);
    let base = path.rsplit('/').next().unwrap_or(path);
    if base == "mod.rs" || base == "lib.rs" || base == "main.rs" {
        dir.to_string()
    } else {
        let stem = base.strip_suffix(".rs").unwrap_or(base);
        if dir.is_empty() {
            stem.to_string()
        } else {
            format!("{dir}/{stem}")
        }
    }
}

/// Round 7: the single file `d.name` names from `declaring_path`, resolved EXACTLY as rustc
/// resolves it, matched against paths `idx` ACTUALLY holds (never assumed) at every step. Round 9
/// (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`) closes the
/// whole `#[path]`-override contract as one enumerated list:
/// 1. A `#[path = ".."]` override on the declaration ([`Def::path_override`], captured
///    structurally at extraction time) takes precedence over the convention entirely. BASE
///    DIRECTORY: when the declaration sits at the declaring file's own top level (the
///    overwhelmingly common case), the base is the declaring file's own DIRECTORY
///    ([`dir_of`]) - never [`module_dir`], since `#[path]` is rustc's own escape hatch FROM the
///    file-per-module convention and is, at the file's top level, unconditionally
///    directory-of-file-relative regardless of whether the declaring file is itself a
///    directory-style module. When the declaration is instead nested inside one or more INLINE
///    `mod outer { .. }` blocks ([`Def::enclosing_inline_module_path`]), the base is the
///    declaring file's own MODULE directory ([`module_dir`]) with the enclosing inline chain
///    appended - one directory component per inline module, outermost first - verified against
///    real rustc (`Def::enclosing_inline_module_path`'s doc). ABSOLUTE / SCHEME REJECTION: an
///    override that is itself an OS-absolute path, a URI scheme, or a Windows drive letter
///    ([`path_override_names_an_unresolvable_shape`]) is UNRESOLVABLE outright, joined onto
///    nothing - rustc uses an absolute `#[path]` as-is, never relative to any Rust-side
///    directory, so it can never name a key this project-relative index holds; left unguarded, a
///    root-level declaring file's EMPTY base directory would let the join degenerate to the raw
///    override unchanged, and its leading `/` would then silently drop during normalization
///    (the SAME rule that drops an ordinary `.` segment), coincidentally colliding with an
///    unrelated real file exactly like the `..`-overflow defect below. NORMALIZATION: the joined
///    `<base>/<override>` string is then [`normalize_logical_path`]'d (round 8,
///    `sdet-u86c1-r7-path-override-dotdot-unresolved` /
///    `adv-u86c1-r7-dot-slash-override-also-unresolved-not-just-dotdot`): a raw `#[path]` value
///    may itself contain `.` or `..` segments at any position (rustc resolves those purely
///    lexically too), and `idx.files()`'s keys are always already-clean, so an unnormalized join
///    would silently fail every `contains_key` lookup for such a value and leave its target
///    unexcluded. Round 9 (`adv-u86c1-r8-overcount-dotdot-silently-excludes-an-unrelated-real-file`):
///    normalization can itself fail - a `..` count exceeding the declaring directory's own depth -
///    and that failure short-circuits this whole branch to `None` via `?` rather than running a
///    `contains_key` lookup on a collapsed-but-wrong string that might coincidentally name a real,
///    unrelated file. LOOKUP: the normalized key must equal a file `idx` actually holds; no match
///    is UNRESOLVABLE. Every UNRESOLVABLE outcome above returns `None` - the declaration is
///    ignored for exclusion purposes and nothing collapses onto another file; never a silent
///    match on a wrong-but-plausible string.
/// 2. Otherwise, the flat sibling `<module_dir>/<name>.rs`.
/// 3. Otherwise, the nested directory-module form `<module_dir>/<name>/mod.rs`.
/// 4. Otherwise `None` - a stale or unresolvable declaration excludes nothing.
fn resolve_out_of_line_target(idx: &SymbolIndex, declaring_path: &str, d: &Def) -> Option<String> {
    if let Some(p) = &d.path_override {
        if path_override_names_an_unresolvable_shape(p) {
            return None;
        }
        let base = match &d.enclosing_inline_module_path {
            Some(chain) => {
                let module_dir = module_dir(declaring_path);
                if module_dir.is_empty() {
                    chain.clone()
                } else {
                    format!("{module_dir}/{chain}")
                }
            }
            None => dir_of(declaring_path).to_string(),
        };
        let joined = if base.is_empty() {
            p.clone()
        } else {
            format!("{base}/{p}")
        };
        let resolved = normalize_logical_path(&joined)?;
        return idx.files().contains_key(&resolved).then_some(resolved);
    }
    let dir = module_dir(declaring_path);
    let flat = if dir.is_empty() {
        format!("{}.rs", d.name)
    } else {
        format!("{dir}/{}.rs", d.name)
    };
    if idx.files().contains_key(&flat) {
        return Some(flat);
    }
    let nested = if dir.is_empty() {
        format!("{}/mod.rs", d.name)
    } else {
        format!("{dir}/{}/mod.rs", d.name)
    };
    idx.files().contains_key(&nested).then_some(nested)
}

/// Round 6 (`op-u86c1-r5-close-every-remaining-test-shape` item 2), resolution fixed in round 7
/// (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): the set of file paths in `idx` that
/// are declared, OUT OF LINE, by a `#[cfg(test)] mod name;` item somewhere in another file - Rust's
/// out-of-tree module form, distinct from an INLINE `#[cfg(test)] mod name { .. }` (which already
/// excludes its own contents by containment, `Def::is_test`/`extract::test_regions`, needing no
/// cross-file lookup at all). A per-file extraction pass can structurally never see this on the
/// DECLARED file's own side - the attribute governing it lives in the DECLARING file's tree
/// entirely (`Def::is_out_of_line_module`'s doc) - so this resolves it here, at the events/index
/// layer, the ONE place every file's path in the project is already known together.
///
/// Two passes over [`resolve_out_of_line_target`]:
/// 1. SEED: every `Module`-kind definition that is itself out-of-line AND directly test-attributed
///    (`is_test`, set by the SAME attribute stack `extract::test_regions` reads - a `#[cfg(test)]`
///    sibling of the `mod name;` item) contributes its resolved target.
/// 2. CLOSURE: "Everything under a resolved test module file (its own nested out-of-line children,
///    resolved recursively by the same rule) is test code." A file pulled in by step 1 is now
///    wholly test code, so EVERY out-of-line module IT declares is excluded too, REGARDLESS of
///    whether that declaration line carries its own `#[cfg(test)]` - there is nothing left for it
///    to gate, since the whole file it lives in is already test-only. A worklist walks newly
///    excluded files to a fixed point; `excluded.insert` returning `false` for an already-seen
///    target both terminates the closure and guards against a cycle looping forever.
fn out_of_line_test_module_files(idx: &SymbolIndex) -> BTreeSet<String> {
    let mut excluded = BTreeSet::new();
    let mut worklist: Vec<String> = Vec::new();

    for (path, fs) in idx.files() {
        for d in &fs.defs {
            if d.kind != Kind::Module || !d.is_out_of_line_module || !d.is_test {
                continue;
            }
            if let Some(target) = resolve_out_of_line_target(idx, path, d) {
                if excluded.insert(target.clone()) {
                    worklist.push(target);
                }
            }
        }
    }

    while let Some(path) = worklist.pop() {
        let Some(fs) = idx.files().get(&path) else {
            continue;
        };
        for d in &fs.defs {
            if d.kind != Kind::Module || !d.is_out_of_line_module {
                continue;
            }
            if let Some(target) = resolve_out_of_line_target(idx, &path, d) {
                if excluded.insert(target.clone()) {
                    worklist.push(target);
                }
            }
        }
    }

    excluded
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

    /// Read a reference event's `caller` (the enclosing definition it was attributed to) from its
    /// raw on-log JSON. The field is `skip_serializing_if` `Option::is_none`, so an ABSENT key reads
    /// as `None` - the exact wire form a caller-less (top-level) reference replays.
    fn caller_of(e: &Event) -> Option<String> {
        let v: serde_json::Value = serde_json::from_slice(&e.data).unwrap();
        v.get("caller")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
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
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                },
                Def {
                    kind: Kind::Function,
                    name: "alpha".to_string(),
                    line: 3,
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                },
            ],
            refs: vec![SymRef {
                name: "helper".to_string(),
                line: 5,
                enclosing: None,
                is_test: false,
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
                    enclosing: None,
                    is_test: false,
                },
                SymRef {
                    name: "apply".to_string(),
                    line: 4,
                    enclosing: None,
                    is_test: false,
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
    fn extract_events_lowers_each_reference_to_an_edge_carrying_its_enclosing_caller() {
        // Criterion 2 (the emit pass carries the caller): `extract_events` reads the enclosing
        // definition c1 attributed onto each `SymRef` and lowers it onto the emitted `EdgeInferred`
        // as `caller`. A reference inside `F` (attributed `enclosing = Some("F")`) emits an edge
        // whose `caller` is `F`; a top-level reference (attributed `enclosing = None`, an import
        // outside every definition) emits one with no caller. This unit does NOT own the extraction
        // attribution (criterion 1) - it owns only that the emit faithfully carries what extraction
        // attributed - so the fixture presets `enclosing` directly rather than driving the parser.
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Function,
                name: "F".to_string(),
                line: 1,
                is_test: false,
                is_out_of_line_module: false,
                path_override: None,
                enclosing_inline_module_path: None,
            }],
            refs: vec![
                // A call to `G` from inside the body of `F`: attributed to its enclosing caller.
                SymRef {
                    name: "G".to_string(),
                    line: 2,
                    enclosing: Some("F".to_string()),
                    is_test: false,
                },
                // A top-level `use` outside every definition: no caller.
                SymRef {
                    name: "std_thing".to_string(),
                    line: 5,
                    enclosing: None,
                    is_test: false,
                },
            ],
        };
        let events = extract_events("src/combat.rs", &fs);

        // Locate each reference's emitted edge by its referenced name (`caller_of` reads the raw
        // wire form, so this fails RED until the emit actually writes the `caller` field).
        let edge_for = |name: &str| -> &Event {
            events
                .iter()
                .find(|e| {
                    e.type_ == TYPE_EDGE_INFERRED
                        && serde_json::from_slice::<serde_json::Value>(&e.data)
                            .unwrap()
                            .get("name")
                            .and_then(|n| n.as_str())
                            == Some(name)
                })
                .unwrap_or_else(|| {
                    panic!("an EdgeInferred event was emitted for the reference {name}")
                })
        };
        assert_eq!(
            caller_of(edge_for("G")),
            Some("F".to_string()),
            "the reference inside F lowers to an edge whose caller is F"
        );
        assert_eq!(
            caller_of(edge_for("std_thing")),
            None,
            "the top-level reference lowers to an edge with no caller (absent key)"
        );

        // Determinism by construction: identical source yields byte-identical events, so deriving
        // the caller must not reorder or otherwise perturb the emitted bytes.
        let again = extract_events("src/combat.rs", &fs);
        let bytes = |es: &[Event]| -> Vec<Vec<u8>> { es.iter().map(|e| e.data.clone()).collect() };
        assert_eq!(
            bytes(&events),
            bytes(&again),
            "identical source yields byte-identical events"
        );
    }

    #[test]
    fn a_file_that_extracts_to_nothing_still_stamps_an_empty_structural_boundary() {
        // Spec 86 criterion 3 (THE MIGRATION IS DELIBERATE): a file with no surviving definitions
        // and no surviving references no longer emits NOTHING - it stamps exactly ONE boundary
        // sentinel event instead, so a re-extraction still supersedes whatever structural edges
        // this file held before (a legacy entity a prior extraction created, now gone). Mirrors
        // `proof_events`'s own identical round-3 rule for evidence.
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![],
            refs: vec![],
        };
        let events = extract_events("src/empty.rs", &fs);
        assert_eq!(
            events.len(),
            1,
            "a file that extracts to nothing still stamps exactly one boundary event, never an \
             empty Vec; got {events:?}"
        );
        assert_eq!(
            events[0].type_, TYPE_EDGE_INFERRED,
            "the structural boundary rides the existing EdgeInferred shape - no new event type"
        );
        let v: serde_json::Value = serde_json::from_slice(&events[0].data).unwrap();
        assert_eq!(
            v.get("name").and_then(|n| n.as_str()),
            Some(""),
            "an empty name marks this record as boundary-only - no real definition or reference"
        );
        assert!(
            !v.get("is_test").and_then(|b| b.as_bool()).unwrap_or(false),
            "the STRUCTURAL sentinel is never an is_test event - that shape is proof_events's own \
             evidence sentinel"
        );
        assert!(
            fresh_of(&events[0]),
            "the lone boundary event IS the file's whole structural batch, so it carries fresh"
        );
    }

    /// A `FileSymbols` carrying a product-shaped (non-test) definition and reference alongside a
    /// non-empty index - reused across the spec-86 tests below so each fixture is built the same
    /// way and only ITS `file` path or `is_test` marking varies.
    fn product_fs() -> FileSymbols {
        FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Function,
                name: "product_fn".into(),
                line: 1,
                is_test: false,
                is_out_of_line_module: false,
                path_override: None,
                enclosing_inline_module_path: None,
            }],
            refs: vec![SymRef {
                name: "product_fn".into(),
                line: 2,
                enclosing: None,
                is_test: false,
            }],
        }
    }

    /// True when `events` carries only [`empty_structural_boundary_event`]'s own shape - one
    /// `EdgeInferred` with an empty `name` - i.e. no REAL `CodeEntityExtracted` or named
    /// `EdgeInferred` at all. The helper both spec 86 criteria 1 and 3 share for asserting "this
    /// file contributed nothing real, only the boundary sentinel".
    fn is_boundary_only(events: &[Event]) -> bool {
        events.len() == 1
            && events[0].type_ == TYPE_EDGE_INFERRED
            && serde_json::from_slice::<serde_json::Value>(&events[0].data)
                .unwrap()
                .get("name")
                .and_then(|n| n.as_str())
                == Some("")
    }

    #[test]
    fn extract_events_excludes_a_file_under_a_tests_directory_entirely() {
        // Spec 86 criterion 1: EVERY file under a `tests/` directory is excluded from the
        // code-entity pass, wholesale - no CodeEntityExtracted, no named EdgeInferred - regardless
        // of what it defines or references (product-shaped content included, so the rule is a path
        // rule, not a content sniff); criterion 3 then requires it to still stamp the boundary-only
        // sentinel rather than emit nothing (see `is_boundary_only`). A SIBLING file at the same
        // content but a `src/` path still emits normally, proving the exclusion keys on the
        // DIRECTORY, not the content.
        let fs = product_fs();
        assert!(
            is_boundary_only(&extract_events("tests/foo.rs", &fs)),
            "a top-level tests/ file emits nothing real, only the boundary sentinel; got {:?}",
            extract_events("tests/foo.rs", &fs)
        );
        assert!(
            is_boundary_only(&extract_events("crate/tests/bar.rs", &fs)),
            "a NESTED tests/ directory (not just a top-level one) is excluded too; got {:?}",
            extract_events("crate/tests/bar.rs", &fs)
        );
        assert!(
            !is_boundary_only(&extract_events("src/foo.rs", &fs)),
            "the exclusion is a DIRECTORY rule: a product path with the identical content still \
             emits real events"
        );
        assert!(
            !is_boundary_only(&extract_events("src/testsuite.rs", &fs)),
            "a file that merely reads close to \"tests\" in its OWN name (not a directory \
             component) is never excluded"
        );
    }

    #[test]
    fn extract_events_skips_is_test_items_and_the_fresh_boundary_lands_on_the_first_survivor() {
        // Spec 86 criterion 1, the in-file case: a definition/reference the extraction pass
        // marked `is_test` (a `#[cfg(test)]`/`#[test]` region) emits no event, while a product
        // sibling in the SAME file emits normally. `aaa_test_item` sorts BEFORE `product_fn`, so
        // this also proves the `fresh` batch-boundary marker lands on the first SURVIVING event,
        // not on a test item that would have sorted first had it not been filtered out.
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![
                Def {
                    kind: Kind::Function,
                    name: "aaa_test_item".into(),
                    line: 5,
                    is_test: true,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                },
                Def {
                    kind: Kind::Function,
                    name: "product_fn".into(),
                    line: 1,
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                },
            ],
            refs: vec![
                SymRef {
                    name: "test_only_callee".into(),
                    line: 6,
                    enclosing: Some("aaa_test_item".into()),
                    is_test: true,
                },
                SymRef {
                    name: "product_fn".into(),
                    line: 2,
                    enclosing: None,
                    is_test: false,
                },
            ],
        };
        let events = extract_events("src/mixed.rs", &fs);

        let names: Vec<String> = events
            .iter()
            .map(|e| {
                serde_json::from_slice::<serde_json::Value>(&e.data)
                    .unwrap()
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            names,
            vec!["product_fn".to_string(), "product_fn".to_string()],
            "only the product definition and the product reference emit; the two is_test items \
             emit nothing; got {names:?}"
        );
        assert_eq!(
            events[0].type_, TYPE_CODE_ENTITY_EXTRACTED,
            "the surviving definition emits before the surviving reference"
        );
        let fresh_of = |e: &Event| -> bool {
            serde_json::from_slice::<serde_json::Value>(&e.data)
                .unwrap()
                .get("fresh")
                .and_then(|f| f.as_bool())
                .unwrap_or(false)
        };
        assert!(
            fresh_of(&events[0]),
            "the batch boundary lands on the first SURVIVING event (product_fn), not on the \
             filtered-out aaa_test_item that would have sorted first"
        );
        assert!(
            !fresh_of(&events[1]),
            "only the first surviving event carries the boundary"
        );
    }

    #[test]
    fn ingesting_a_fixture_with_product_and_test_code_graphs_only_the_product_and_lists_no_test_file(
    ) {
        // Spec 86 criterion 1's own Done-when, end to end: a fixture with product code, a
        // `tests/` file, a `#[cfg(test)]` module, and `#[test]` functions - ingested through the
        // REAL extraction + emit + fold pipeline (`build_index` -> `index_events` -> `Projector`).
        // The graph must hold a code-entity node for the product item and NONE for any test item;
        // the test file must carry no KIND_FILE container node at all (the concrete mechanism
        // behind "the files lens lists no test file as a subject" - the files lens folds each
        // code entity by its own file, so a file with no code-entity node can never be one of its
        // subjects).
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("product.rs"),
            "\
fn product_fn() {
    helper();
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        product_fn();
    }
}
",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests").join("integration.rs"),
            "\
#[test]
fn an_integration_test() {
    also_calls_product();
}
",
        )
        .unwrap();

        let idx = build_index(dir.path().to_str().unwrap(), None);
        let events = index_events(&idx);

        let p = Projector::open(":memory:", "test").unwrap();
        for (i, mut e) in events.into_iter().enumerate() {
            e.position = (i + 1) as u64;
            p.apply(&e).unwrap();
        }

        let g = p
            .subgraph(
                &["product.rs".to_string(), "tests/integration.rs".to_string()],
                3,
            )
            .unwrap();

        // The product item has a code-entity node...
        assert!(
            g.nodes
                .iter()
                .any(|n| n.kind == KIND_CODE_ENTITY && n.id == "product.rs::product_fn"),
            "the product definition folds into a code-entity node; got {:?}",
            g.nodes
        );
        // ...and its own file has a container node (it has product content to hold).
        assert!(
            g.nodes
                .iter()
                .any(|n| n.kind == KIND_FILE && n.id == "product.rs"),
            "the product file carries its own KIND_FILE container node; got {:?}",
            g.nodes
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_CONTAINS && e.to == "product.rs::product_fn"),
            "a CONTAINS edge ties the product file to its product definition; got {:?}",
            g.edges
        );

        // NONE of the test items - the #[cfg(test)] module, its #[test] fn, or the whole
        // tests/integration.rs file's #[test] fn - ever became a code-entity node.
        for excluded in [
            "product.rs::tests",
            "product.rs::it_works",
            "tests/integration.rs::an_integration_test",
        ] {
            assert!(
                !g.nodes.iter().any(|n| n.id == excluded),
                "{excluded:?} is test code and must NEVER become a graph node; got {:?}",
                g.nodes
            );
        }
        // The tests/ file carries NO KIND_FILE node at all - it contributed no batch (whole-file
        // exclusion), so there is nothing for the files lens to ever list as a subject.
        assert!(
            !g.nodes
                .iter()
                .any(|n| n.kind == KIND_FILE && n.id == "tests/integration.rs"),
            "an all-test file must not even carry a file container node; got {:?}",
            g.nodes
        );
        // No REFERENCES edge from the test file's call into `also_calls_product` ever landed
        // "on the canvas" either - the whole file emitted nothing.
        assert!(
            !g.edges.iter().any(|e| e.from == "tests/integration.rs"),
            "an excluded file's references never become structural edges; got {:?}",
            g.edges
        );
    }

    /// Spec 86 criterion 2's own Done-when, end to end: a product entity referenced by TWO test
    /// functions - one in-file (`#[cfg(test)] mod tests`), one in a whole `tests/`-dir file -
    /// carries `proven_by: 2` with both `file:line`s in the graph payload and renders on its card,
    /// while an unreferenced entity in the SAME file renders the explicit no-test state. Runs the
    /// REAL pipeline (`build_index` -> `index_events` -> `Projector` -> `dash::card`), never a
    /// hand-built fixture, so it proves the whole chain - extraction, the `proof_events` emission
    /// pass, the fold, and the card - agree.
    #[test]
    fn a_product_entity_referenced_by_two_tests_carries_proven_by_2_and_renders_on_its_card() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("product.rs"),
            "\
fn product_fn() {
    helper();
}

fn unused_fn() {}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        product_fn();
    }
}
",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests").join("integration.rs"),
            "\
#[test]
fn an_integration_test() {
    product_fn();
}
",
        )
        .unwrap();

        let idx = build_index(dir.path().to_str().unwrap(), None);
        let events = index_events(&idx);

        let p = Projector::open(":memory:", "test").unwrap();
        for (i, mut e) in events.into_iter().enumerate() {
            e.position = (i + 1) as u64;
            p.apply(&e).unwrap();
        }

        let g = p
            .subgraph(
                &["product.rs".to_string(), "tests/integration.rs".to_string()],
                3,
            )
            .unwrap();

        // PROVEN: product_fn is referenced by the in-file test (line 11) and the tests/-dir
        // integration test (line 3) - proven_by: 2, both file:lines, in fold order (product.rs
        // sorts before tests/integration.rs, so its own evidence lands first).
        let card = crate::dash::card(&g, "product.rs::product_fn")
            .expect("product.rs::product_fn is a graph node");
        assert_eq!(
            card.proven_by, 2,
            "two test-origin references prove product_fn twice; card: {card:?}"
        );
        assert_eq!(
            card.proof_evidence,
            vec![
                "product.rs:11".to_string(),
                "tests/integration.rs:3".to_string()
            ],
            "both evidence file:lines land on the card, in fold order; card: {card:?}"
        );

        // UNREFERENCED: unused_fn, defined in the SAME file, is never called by any test - the
        // explicit no-test state (proven_by: 0, no evidence), never a made-up value.
        let unused = crate::dash::card(&g, "product.rs::unused_fn")
            .expect("product.rs::unused_fn is a graph node");
        assert_eq!(
            unused.proven_by, 0,
            "an unreferenced entity renders the explicit no-test state; card: {unused:?}"
        );
        assert!(unused.proof_evidence.is_empty());

        // Criterion 1's own promise still holds alongside the new evidence: neither test item
        // ever became a node, and the tests/-dir file still carries no KIND_FILE container.
        for excluded in ["product.rs::tests", "product.rs::it_works"] {
            assert!(
                !g.nodes.iter().any(|n| n.id == excluded),
                "{excluded:?} is test code and must never become a graph node; got {:?}",
                g.nodes
            );
        }
        assert!(
            !g.nodes
                .iter()
                .any(|n| n.kind == KIND_FILE && n.id == "tests/integration.rs"),
            "an all-test file still carries no file container node even though it contributes \
             evidence; got {:?}",
            g.nodes
        );
    }

    use crate::grounder::symbols::events::proof_events;

    #[test]
    fn proof_events_on_a_file_with_no_test_evidence_returns_a_boundary_only_sentinel_never_an_empty_vec(
    ) {
        // Spec 86 criterion 2, round 3 (adv-u86c2-r2-deleted-test-reference-strands-proof-
        // forever): `proof_events` used to return an EMPTY `Vec` whenever a file's evidence set
        // was empty - the overwhelming common case (an ordinary product file with no
        // `#[cfg(test)]`/`#[test]` content at all, product_fs() below). Combined with
        // `extract_events` ALSO returning empty for a whole-file-test path, a file's WHOLE batch
        // (`index_events`/`project_batches_paced`) could be empty and get DROPPED entirely, so
        // `fold_test_evidence`/`supersede_file_proof` never even ran on a LATER re-extraction that
        // emptied a file's evidence - stranding stale proof forever. Mirroring criterion 3's own
        // already-established empty-after-exclusion pattern (a file that extracts to nothing still
        // stamps ONE boundary event rather than being skipped), `proof_events` now returns exactly
        // one sentinel event instead of nothing: an ordinary `is_test` EdgeInferred, `fresh: true`
        // (it is the file's WHOLE evidence batch), but an EMPTY `name` - never a real reference's
        // name - marking it boundary-only. The fold's own guard (`fold_test_evidence`) recognizes
        // the empty name and runs ONLY the supersede-on-re-extract retraction, resolving/recording
        // nothing for it.
        let events = proof_events("src/product.rs", &product_fs());
        assert_eq!(
            events.len(),
            1,
            "a file with no test evidence still stamps exactly one boundary event, never an \
             empty Vec; got {events:?}"
        );
        assert_eq!(
            events[0].type_, TYPE_EDGE_INFERRED,
            "the boundary rides the existing EdgeInferred shape - no new event type"
        );
        let v: serde_json::Value = serde_json::from_slice(&events[0].data).unwrap();
        assert_eq!(
            v.get("name").and_then(|n| n.as_str()),
            Some(""),
            "an empty name marks this record as boundary-only - no real reference, never a \
             genuine one"
        );
        assert_eq!(
            v.get("file").and_then(|n| n.as_str()),
            Some("src/product.rs"),
            "the boundary still carries the file so the fold supersedes the right file's \
             evidence"
        );
        assert!(
            v.get("is_test").and_then(|b| b.as_bool()).unwrap_or(false),
            "the sentinel is an is_test event, so the fold's existing is_test branch routes it \
             to fold_test_evidence - never a new fold arm"
        );
        assert!(
            fresh_of(&events[0]),
            "the lone boundary event IS the file's whole evidence batch, so it carries fresh"
        );
    }

    #[test]
    fn proof_events_on_a_whole_file_test_directory_with_zero_references_also_returns_the_sentinel()
    {
        // The degenerate whole-file-test case: a `tests/`-dir file that defines/references
        // NOTHING at all (every reference would count as evidence here, but there are none to
        // begin with) must ALSO stamp the boundary sentinel, not return empty - the same
        // "empty-after-exclusion is a first-class boundary" rule applies regardless of WHY the
        // evidence set is empty.
        let fs = FileSymbols {
            lang: Lang::Rust,
            defs: vec![],
            refs: vec![],
        };
        let events = proof_events("tests/empty_integration.rs", &fs);
        assert_eq!(
            events.len(),
            1,
            "a tests/-dir file with zero references still stamps the boundary sentinel; got \
             {events:?}"
        );
        let v: serde_json::Value = serde_json::from_slice(&events[0].data).unwrap();
        assert_eq!(v.get("name").and_then(|n| n.as_str()), Some(""));
    }

    /// [`file_batches`] (spec 92, FRESH ON EVERY INTEGRATION): scoped to exactly the NAMED files,
    /// never the whole project - the property `project_batches`/`project_batches_paced` do not
    /// have, and the one an integration's own bounded reindex needs (Design/Constraints Walk: "the
    /// reindex is bounded by the merge's file list"). `b.rs` is present and indexable but never
    /// named, so its ABSENCE from the result (not merely `a.rs`'s presence) is what proves the
    /// scope - a whole-project walk would return both.
    #[test]
    fn file_batches_is_scoped_to_the_named_files_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn bar() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();

        let batches = super::file_batches(root, &["a.rs".to_string()]);
        assert_eq!(
            batches.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>(),
            vec!["a.rs"],
            "only the named file gets a batch, never an untouched sibling; got {batches:?}"
        );
        let names: Vec<String> = batches[0]
            .1
            .iter()
            .filter(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED)
            .filter_map(|e| serde_json::from_slice::<serde_json::Value>(&e.data).ok())
            .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        assert_eq!(
            names,
            vec!["foo".to_string()],
            "the named file's real, current definition is what the batch carries"
        );
    }

    /// [`file_batches`] reflects LIVE content (spec 92): re-extracting a file after it CHANGED on
    /// disk returns its NEW definition, never a stale one - the property that lets a scoped
    /// integration-time reindex heal a moved/renamed function.
    #[test]
    fn file_batches_reflects_the_files_current_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("churn.rs");
        std::fs::write(&path, "fn original() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();
        let file = "churn.rs".to_string();

        let first = super::file_batches(root, std::slice::from_ref(&file));
        let first_names: Vec<String> = first[0]
            .1
            .iter()
            .filter(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED)
            .filter_map(|e| serde_json::from_slice::<serde_json::Value>(&e.data).ok())
            .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        assert_eq!(first_names, vec!["original".to_string()]);

        std::fs::write(&path, "fn renamed() {}\n").unwrap();
        let second = super::file_batches(root, std::slice::from_ref(&file));
        let second_names: Vec<String> = second[0]
            .1
            .iter()
            .filter(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED)
            .filter_map(|e| serde_json::from_slice::<serde_json::Value>(&e.data).ok())
            .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        assert_eq!(
            second_names,
            vec!["renamed".to_string()],
            "a re-extraction after the file changed on disk reflects the NEW content, not the \
             first call's stale snapshot"
        );
    }

    /// [`file_batches`] over a file the index holds no entry for (deleted since the index was last
    /// built, or never source) still contributes a batch - exactly the boundary sentinel
    /// `extract_events` stamps for a file that "extracts to nothing" (spec 86 criterion 3) - so its
    /// prior structural edges retire through the SAME existing supersession rather than dangling
    /// forever (Design/Constraints Walk: "a file deleted by the integration - its entities are
    /// retired through the existing supersession, not left dangling").
    #[test]
    fn file_batches_retires_a_file_absent_from_the_index_via_the_boundary_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // "deleted.rs" is named but was never written to disk, so a fresh build of `root` (there is
        // no persisted index here) never indexes it - the same shape as a file the persisted index
        // dropped because `Grounder::reindex` already purged it as deleted.
        let batches = super::file_batches(root, &["deleted.rs".to_string()]);
        assert_eq!(
            batches.len(),
            1,
            "an index-absent file still contributes a batch (never silently skipped); got {batches:?}"
        );
        assert_eq!(batches[0].0, "deleted.rs");
        assert_eq!(
            batches[0].1.len(),
            1,
            "exactly one event - the boundary sentinel, nothing else; got {:?}",
            batches[0].1
        );
        assert_eq!(batches[0].1[0].type_, TYPE_EDGE_INFERRED);
        let v: serde_json::Value = serde_json::from_slice(&batches[0].1[0].data).unwrap();
        assert_eq!(
            v.get("name").and_then(|n| n.as_str()),
            Some(""),
            "the boundary sentinel carries no name (never a real edge)"
        );
        assert_eq!(
            v.get("fresh").and_then(|n| n.as_bool()),
            Some(true),
            "the sentinel is stamped fresh, so the fold supersedes this file's PRIOR structural \
             edges - the retirement mechanism itself"
        );
    }
}
