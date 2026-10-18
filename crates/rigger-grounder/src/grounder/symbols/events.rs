//! The code-to-events emit pass (spec 29a): lowers an extracted `SymbolIndex` / `FileSymbols`
//! into `CodeEntityExtracted` (one per path) and `EdgeInferred` (one per reference) events,
//! which the always-compiled context-graph fold ingests into `code-entity` / `file` nodes and
//! structural edges. Code structure thus becomes a rebuildable projection over the event log, not
//! a mutable side index. This is the emit half; the fold half lives in `contextgraph::sqlite` and
//! stays compiled in both lanes.

use crate::contextgraph::{
    definition_precedence, CodeEntityExtracted, EdgeInferred, CODE_KIND_CONSTANT,
    CODE_KIND_CONSTRUCTOR, CODE_KIND_DECLARATION, CODE_KIND_FIELD, CODE_KIND_FUNCTION,
    CODE_KIND_IMPL, CODE_KIND_MACRO, CODE_KIND_METHOD, CODE_KIND_MODULE, CODE_KIND_OTHER,
    CODE_KIND_PROPERTY, CODE_KIND_STATIC, CODE_KIND_TRAIT, CODE_KIND_TYPE, CODE_KIND_VARIABLE,
    CODE_KIND_VARIANT, REF_KIND_MODULE, TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
};
use crate::eventstore::Event;
use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef, SymbolIndex};
use crate::grounder::symbols::registry::for_path;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

/// Round 5 (adj-u86c3-r4-out-of-line-exclusion-still-unmigrated): the symbols [`extract_events`]
/// actually sees for one file - `fs` unchanged, UNLESS `excluded` (this file is the resolved
/// target of an out-of-line `#[cfg(test)] mod name;` declaration elsewhere,
/// [`out_of_line_test_module_files`]), in which case its defs/refs are hollowed to empty. An
/// out-of-line target's OWN items are never marked `is_test` themselves - that flag is set by a
/// LOCAL `#[cfg(test)]` attribute stack inside the SAME file, and being the resolved target of an
/// EXTERNAL declaration elsewhere is invisible to a per-file parse - so hollowing here, at the ONE
/// place every lowering of a file passes through ([`lower_file`]), is what lets
/// `extract_events`'s own existing "nothing survived" branch return the empty-boundary sentinel
/// for it, exactly as it
/// already does for a whole dev-only file or an in-file `#[cfg(test)]` re-wrap - never a
/// second, bespoke sentinel-construction path for a third shape. `Cow` so the overwhelmingly
/// common (non-excluded) path borrows `fs` unchanged rather than cloning it. [`lower_file`] is
/// its one caller.
fn for_extraction(fs: &FileSymbols, excluded: bool) -> Cow<'_, FileSymbols> {
    if excluded {
        Cow::Owned(FileSymbols {
            lang: fs.lang,
            defs: Vec::new(),
            refs: Vec::new(),
            partial: fs.partial,
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
/// single file's. Returns `(file, events, excluded)` per file in the index's sorted path order,
/// `excluded` the flag [`project_batches_paced`] answers; EVERY file -
/// an out-of-line test-module target ([`for_extraction`]-hollowed) included, round 5 - contributes
/// a batch, and that batch is NEVER empty ([`extract_events`] itself never returns empty, spec 86
/// criterion 3: it stamps its own boundary-only sentinel when nothing real survives, so a file
/// whose structural set is empty still contributes at least one boundary event). The caller keys
/// each batch on its content, so an unchanged file is not re-ingested and a changed one
/// re-extracts.
pub fn project_batches(root: &str) -> Vec<(String, Vec<Event>, bool)> {
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
/// criterion 3), so every file's own batch is never empty. Each file is lowered by [`lower_file`],
/// the one composition [`file_batches`] lowers a named file by, over the index and the exclusions
/// [`walk_exclusions`](crate::ingest::walk_exclusions) answers, and answered beside its batch
/// with its flag (spec 107): whether those exclusions name the file's `gc` identity, the one
/// answer its lowering takes too. Returns `(batches, workers_engaged)`, each batch
/// `(file, events, excluded)`.
pub fn project_batches_paced(
    root: &str,
    workers: usize,
) -> (Vec<(String, Vec<Event>, bool)>, usize) {
    let (idx, excluded) = crate::ingest::walk_exclusions(root);
    let files: Vec<(&String, &FileSymbols)> = idx.files().iter().collect();
    // Parse/lower per file in parallel; `map_ordered` returns the per-file results in the input
    // (sorted-path) order, so the emit sequence is independent of which worker finished first.
    crate::parallel::map_ordered(&files, workers, |&(path, fs)| {
        let excluded = names_code_identity(&excluded, path);
        (path.clone(), lower_file(path, fs, excluded), excluded)
    })
}

/// Scoped counterpart to [`project_batches`]/[`project_batches_paced`] (spec 92, FRESH ON EVERY
/// INTEGRATION): extract only the NAMED `files`' events, rather than walking the whole persisted
/// index - the property an integration's own reindex needs, bounded by the merge's own file list
/// (Design/Constraints Walk) rather than the project's total file count. Reuses the SAME index and
/// exclusions ([`walk_exclusions`](crate::ingest::walk_exclusions)) and the SAME [`lower_file`] as
/// `project_batches_paced` - never a second lowering path - so a named file's scoped batch is
/// byte-identical to what a full walk would produce for it. Returns `(file, events, excluded)`
/// triples in the SAME order `files` was given, one per named file (never fewer), `excluded` the
/// batch's flag (spec 107), whether the exclusions name the file's `gc` identity, answered for
/// every named file by that one question: a file the index holds no
/// entry for (deleted since the index was last built, or never source) still contributes exactly
/// [`empty_structural_boundary_event`]'s single-event batch - the SAME boundary sentinel a file that
/// "extracts to nothing" stamps (spec 86 criterion 3) - so its prior structural edges retire through
/// the existing supersession rather than dangling forever, mirroring `extract_events`'s own "never
/// returns empty" contract for the whole-project walk.
pub fn file_batches(root: &str, files: &[String]) -> Vec<(String, Vec<Event>, bool)> {
    let (idx, excluded) = crate::ingest::walk_exclusions(root);
    files
        .iter()
        .map(|file| {
            let excluded = names_code_identity(&excluded, file);
            let events = match idx.files().get(file) {
                Some(fs) => lower_file(file, fs, excluded),
                // Absent from the index: deleted or unreadable since the index was last freshened
                // (or never source at all).
                None => unparsed_batch(file),
            };
            (file.clone(), events, excluded)
        })
        .collect()
}

/// The `gc` batch of a path that holds nothing a grammar parsed: exactly the one structural
/// boundary event, of an unknown language. `lang` is immaterial there - the fold's supersede keys
/// on the file alone - so it is never guessed or re-derived.
fn unparsed_batch(path: &str) -> Vec<Event> {
    vec![empty_structural_boundary_event(path, "unknown", false)]
}

/// The `gc` batch of the file at `path` holding `bytes` (spec 107), total over its input: the
/// batch the walk lowers from that file. UTF-8 bytes that extract under the grammar the path
/// resolves, as every production caller of the index resolves it - the symbols
/// [`extracted`](crate::grounder::symbols::extracted) answers, the ones the index holds for that
/// text - yield [`lower_file`] over them under `excluded`; every other input - no bytes, bytes
/// that are not UTF-8, a path with no grammar, a failed extraction - yields [`unparsed_batch`], as
/// [`file_batches`] gives a path the index lacks. `excluded` changes only a parsed batch. Never empty.
pub fn bytes_batch(path: &str, bytes: Option<&[u8]>, excluded: bool) -> Vec<Event> {
    let text = || crate::grounder::text_of(bytes);
    let symbols = crate::grounder::symbols::extracted(path, text, None)
        .flatten()
        .map(|(_, symbols)| symbols);
    match symbols {
        Some(symbols) => lower_file(path, &symbols, excluded),
        None => unparsed_batch(path),
    }
}

/// Whether `excluded`, the identities [`walk_exclusions`](crate::ingest::walk_exclusions)
/// answers, names the `gc` identity of `path`.
fn names_code_identity(excluded: &BTreeSet<String>, path: &str) -> bool {
    excluded.contains(&crate::ingest::code_identity(path))
}

/// The batch the walk lowers from one file's `symbols` (spec 107): its structural events
/// ([`extract_events`]) over the symbols [`for_extraction`] leaves it, hollowed when `excluded`,
/// and after them, unless `excluded`, its test-origin evidence ([`proof_events`], spec 86
/// criterion 2, never empty when run). `excluded` says the file is the resolved target of an
/// out-of-line test module declaration elsewhere; such a file's batch is the one boundary event
/// and carries no evidence. Never empty, as neither part is.
fn lower_file(path: &str, symbols: &FileSymbols, excluded: bool) -> Vec<Event> {
    let mut events = extract_events(path, &for_extraction(symbols, excluded));
    if !excluded {
        events.extend(proof_events(path, symbols));
    }
    events
}

/// Emit one file's extracted symbols as events: one `CodeEntityExtracted` per path (spec 119, ONE
/// PATH, ONE EVENT), for the definition at that path first by `CODE_KIND_PRECEDENCE`
/// ([`path_precedence`]), then the lower line, then the order `extract` returns them, carrying
/// its id path when that differs from its name and its parent's path when it has one, then one
/// `EdgeInferred` per reference, then one module row per out-of-line `mod` declaration naming a
/// candidate file (spec 117, A MODULE ROW REACHES ITS FILE). Each set is emitted in a
/// deterministic order - the module rows in the order `extract` returns them - so identical
/// source yields byte-identical events - the determinism-by-construction spec 29a requires: refs
/// by name/line, and defs parent before child (spec 119, A PARENT IS EMITTED BEFORE ITS CHILD), by
/// the length of each one's parent chain ([`chain_length`]), then leaf, then path, a total order
/// over one-per-path definitions, so a parent's own event precedes every child's. Definitions are
/// emitted before references so the fold can land a same-file reference on its already-folded
/// definition entity.
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
/// 1. **Whole file at a dev-only path** ([`is_dev_only`]): no `CodeEntityExtracted`
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
    if is_dev_only(file) {
        return vec![empty_structural_boundary_event(file, lang, fs.partial)];
    }
    let mut events = Vec::with_capacity(fs.defs.len() + fs.refs.len());

    let survivors = fs.defs.iter().filter(|d| !d.is_test);
    // Spec 119 (ONE PATH, ONE EVENT): one definition per path, the first by
    // `CODE_KIND_PRECEDENCE`, then the lower line, then the order `extract` returns them.
    let mut emitted: BTreeMap<&str, &Def> = BTreeMap::new();
    for d in survivors.clone() {
        let held = emitted.entry(d.path.as_str()).or_insert(d);
        if path_precedence(d) < path_precedence(held) {
            *held = d;
        }
    }
    let mut defs: Vec<&Def> = survivors
        .clone()
        .filter(|d| std::ptr::eq(emitted[d.path.as_str()], *d))
        .collect();
    // Spec 119 (A PARENT IS EMITTED BEFORE ITS CHILD): two distinct paths never tie, so the
    // emission is a total order independent of `extract`'s.
    defs.sort_by_cached_key(|&d| {
        let chain = chain_length(d.parent.as_deref(), &emitted);
        (chain, d.name.as_str(), d.path.as_str())
    });
    for d in defs {
        let payload = CodeEntityExtracted {
            file: file.to_string(),
            name: d.name.clone(),
            // Spec 119 (IDS FOLLOW THE LANGUAGE'S PATH): the id path, carried only when it differs
            // from the name, so a file-level definition's event stays byte-identical to the form
            // recorded before the field existed.
            path: (d.path != d.name).then(|| d.path.clone()),
            // Spec 119 (CONTAINS IS STRUCTURE): the path its `CONTAINS` comes from, absent at
            // file level.
            parent: d.parent.clone(),
            kind: kind_str(d.kind).to_string(),
            line: d.line,
            lang: lang.to_string(),
            // The first event of the file's batch marks the re-extraction boundary; set below.
            fresh: false,
            // Spec 92 criterion 2 round 4 (adv-u2c2-partial-marker-unimplemented): carried on
            // EVERY event of this file's batch (never only the `fresh` one) so the fold's
            // merge-not-replace `ensure_node` stamp is independent of which event happens to be
            // first - see `FileSymbols::partial`'s own doc.
            partial: fs.partial,
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
            // 37): the caller, same-file, by its path (spec 119 criterion 8). Read straight off the
            // `SymRef` in the already-fixed sorted order, never a new sort key, so the
            // deterministic refs emission is unchanged; `None` (a top-level reference) is omitted
            // on the wire, keeping the event byte-identical to the pre-37 form.
            caller: r.enclosing.clone(),
            // A STRUCTURAL reference's edge carries no line (spec 86 criterion 2 introduces `line`
            // for evidence only); leaving it 0 keeps this event's wire form byte-identical to
            // before the field existed.
            line: 0,
            // This is a structural reference, never evidence - `proof_events` (below) is the one
            // emitter of `is_test: true` events.
            is_test: false,
            // See the definition arm's identical field above.
            partial: fs.partial,
            // Spec 117 (REFERENCES): a tagged reference's kind, absent for a call, so a call's
            // event stays byte-identical to the form recorded before the field existed.
            ref_kind: r.ref_kind.clone(),
        };
        events.push(Event::new(
            TYPE_EDGE_INFERRED,
            serde_json::to_vec(&payload).expect("edge payload serializes"),
        ));
    }

    // Spec 117 (A MODULE ROW REACHES ITS FILE): after the references, one module row per
    // surviving out-of-line `mod` declaration, in the order `extract` returns them, from the
    // `mod` node's path and named by the declaration's candidates joined by `\n`, so cfg-twin
    // declarations with different candidates stay two rows; one naming no candidate lowers none.
    for d in survivors.filter(|d| d.is_out_of_line_module) {
        let candidates = module_candidates(file, d);
        if candidates.is_empty() {
            continue;
        }
        let payload = EdgeInferred {
            file: file.to_string(),
            name: candidates.join("\n"),
            lang: lang.to_string(),
            fresh: false,
            caller: Some(d.path.clone()),
            line: 0,
            is_test: false,
            partial: fs.partial,
            ref_kind: Some(REF_KIND_MODULE.to_string()),
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
        events.push(empty_structural_boundary_event(file, lang, fs.partial));
    }

    events
}

/// The length of the parent chain above a definition whose parent is `parent` (spec 119, A PARENT
/// IS EMITTED BEFORE ITS CHILD): 0 at file level, else one link for the parent plus the chain
/// above the one definition `emitted` holds at its path - each emitted path mapped to its
/// definition - a parent naming no emitted definition adding nothing above it.
fn chain_length(parent: Option<&str>, emitted: &BTreeMap<&str, &Def>) -> usize {
    parent.map_or(0, |path| {
        1 + emitted
            .get(path)
            .map_or(0, |d| chain_length(d.parent.as_deref(), emitted))
    })
}

/// Where `d` stands in the order that picks a path's one emitted definition (spec 119, ONE PATH,
/// ONE EVENT): the [`definition_precedence`] of its kind and line.
fn path_precedence(d: &Def) -> (usize, u32) {
    definition_precedence(kind_str(d.kind), d.line)
}

/// Spec 86 criterion 3 (THE MIGRATION IS DELIBERATE): the STRUCTURAL batch-boundary sentinel for a
/// file whose surviving (non-`is_test`, non-dev-only-path) definition and reference set is EMPTY -
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
/// migration too). Two callers reach this: a WHOLE dev-only file ([`is_dev_only`], the
/// central migration case named in the Design - every entity it ever held was test code) and the
/// general "extracted to nothing" case at the end of [`extract_events`] (whatever survived
/// filtering, if anything, amounted to zero definitions and zero references). Both are ONE rule,
/// never two: [`extract_events`] never returns an empty `Vec`.
///
/// `partial` (spec 92 criterion 2 round 4): the file's own parse-degraded marker, carried through
/// for wire consistency with every other event of a batch even though this sentinel's own
/// empty-name fold guard returns before ever reaching `ensure_node` - criterion 1's "never a node"
/// promise for an excluded/empty file holds unchanged; there is no `KIND_FILE` node here for the
/// marker to land on.
fn empty_structural_boundary_event(file: &str, lang: &str, partial: bool) -> Event {
    let payload = EdgeInferred {
        file: file.to_string(),
        name: String::new(),
        lang: lang.to_string(),
        fresh: true,
        caller: None,
        line: 0,
        is_test: false,
        partial,
        ref_kind: None,
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
/// A reference counts as evidence only when it is a call ([`FileSymbols::calls`], spec 117: a
/// type reference, const read or `use` is never evidence, so `proven_by` stands) and EITHER: the
/// whole file is at a dev-only path
/// ([`is_dev_only`] - every reference in a test, bench or example file is test-origin, whatever
/// its own `is_test` marking, since the file itself exercises production code), OR the reference
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
/// distinct from `extract_events`'s structural boundary on the SAME event list
/// ([`lower_file`] concatenates both, so a file can carry two independent boundaries - one
/// per concern) - without it, editing a test file (adding an unrelated test, fixing a comment)
/// re-extracts the whole file and re-records every unchanged is_test reference as brand-new
/// evidence, permanently inflating `proven_by`.
///
/// Round 3 (adv-u86c2-r2-deleted-test-reference-strands-proof-forever): a file whose evidence set
/// is EMPTY - the overwhelming common case for an ordinary product file with no is_test content at
/// all, but ALSO the ordinary case of deleting or rewriting the one test that proved something -
/// used to return an empty `Vec` here, so this function stamped no boundary. That was fine for a
/// file that NEVER had evidence, but silently wrong for one TRANSITIONING from evidence to none: a
/// file's WHOLE batch (`extract_events` alongside this function, concatenated by
/// [`lower_file`]) can itself be empty - a dev-only file's structural side is ALWAYS
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
/// Disclosed, non-blocking scope limit: a
/// file pulled in only by an OUT-OF-LINE `#[cfg(test)] mod name;` declaration elsewhere
/// ([`out_of_line_test_module_files`]) still contributes a STRUCTURAL batch since round 5
/// (`extract_events` runs on it hollowed, via [`lower_file`], the composition that hollows it
/// through [`for_extraction`] - see either function's doc), but this function is deliberately
/// never called
/// for it (its own references carry no `is_test` marking of their own - the attribute lives on the
/// DECLARING file's side, invisible to this per-file view, per [`out_of_line_test_module_files`]'s
/// own doc), so a test-only file reached that way contributes no evidence, one [`is_dev_only`]
/// names too (a `src/foo/tests/mod.rs` that `#[cfg(test)] mod tests;` declares) included.
///
/// Like criterion 3's identical structural sentinel, [`empty_evidence_boundary_event`]'s payload
/// is a CONSTANT per `(file, lang)` with no generation-distinguishing field, so a LATER occurrence
/// of an all-empty batch extracts to the SAME generation (`crate::ingest::key_batch`,
/// content-hashing a file's WHOLE batch) as an EARLIER one. Neither ingest sink drops it for that:
/// each weighs a batch against its identity's LATEST recorded generation, the run's sink against
/// the graph's current one too, never against every generation ever recorded, so a later
/// occurrence is recorded again whenever another generation came between.
pub fn proof_events(file: &str, fs: &FileSymbols) -> Vec<Event> {
    let whole_file_test = is_dev_only(file);
    let lang = lang_str(fs.lang);
    let mut refs: Vec<&SymRef> = fs
        .calls()
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
                // Evidence events never reach the structural `ensure_node` stamp (`fold_test_evidence`
                // returns before it) - always `false`, a plain field-completeness requirement, not a
                // meaningful signal here.
                partial: false,
                // Evidence is a call alone: `fs.calls()` above never yields a tagged reference.
                ref_kind: None,
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
        partial: false,
        ref_kind: None,
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

/// DEV-ONLY PATHS (spec 116 criterion 1): whether `path` (a '/'-separated, project-relative
/// path, matching what [`extract_events`]'s `file` parameter and every batch key already carry)
/// names dev-only code by where it lives. The ONE path predicate the graph and ranking share:
/// [`extract_events`] lowers such a file to its boundary alone, [`proof_events`] emits its every
/// reference as evidence, [`crate::grounder::design::extract`]'s link pass drops a link to it,
/// and the grounder's ranking marks its every location test. Case-sensitive, read off the path
/// alone, in two parts:
///
/// - Ungated, in every language: a DIRECTORY component (every segment except the file's own
///   name, [`dir_of`]) `tests`, `benches`, `examples` or `__tests__`. So `tests/foo.rs` and
///   `crate/benches/b.rs` are dev-only while `src/testsuite.rs` and `src/tests.rs` are not.
/// - Gated on the language [`for_path`] gives the path's own extension (a dotfile such as
///   `.cs` has none): Go `*_test.go`; Python `test_*.py` and `*_test.py`; JS and TS `*.test.*`
///   and `*.spec.*`; C# a directory component ending `Test` or `Tests` (its last dot-separated
///   segment ending so, neither suffix holding a `.`). A file whose extension registers no
///   language, one indexed under a `--language` override included, matches the ungated part
///   only.
pub(crate) fn is_dev_only(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lang = for_path(path, None).map(|entry| entry.lang);
    let dev_dir = |dir: &str| {
        matches!(dir, "tests" | "benches" | "examples" | "__tests__")
            || (lang == Some(Lang::CSharp) && (dir.ends_with("Test") || dir.ends_with("Tests")))
    };
    dir_of(path).split('/').any(dev_dir)
        || match lang {
            Some(Lang::Go) => name.ends_with("_test.go"),
            Some(Lang::Python) => name.starts_with("test_") || name.ends_with("_test.py"),
            Some(Lang::Js | Lang::Ts) => name.contains(".test.") || name.contains(".spec."),
            Some(Lang::Rust | Lang::CSharp) | None => false,
        }
}

/// The directory component of a project-relative `path` (every segment except the file's own
/// name) - the ONE place this repo splits a path this way, shared by [`is_dev_only`],
/// [`module_dir`] and [`module_candidates`]' `#[path]`-override branch, so the
/// split-off-the-last-`/` idiom exists exactly once.
fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// `rest` under the directory `base`: `<base>/<rest>`, or `rest` alone when `base` is empty (the
/// project root) - the ONE place a module path joins a directory, shared by [`module_dir`] and
/// [`module_candidates`].
fn join_under(base: &str, rest: &str) -> String {
    if base.is_empty() {
        rest.to_string()
    } else {
        format!("{base}/{rest}")
    }
}

/// Resolves `.` and `..` components, at ANY position, in a `/`-separated LOGICAL path string - the
/// ONE place [`module_candidates`]' `#[path]`-override branch normalizes a raw attribute
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
/// matching a real, UNRELATED file's key, at which point the index lookup over the candidates finds
/// a match and silently drops that unrelated file's whole entity set from the graph. So this now
/// returns `None` the instant a `..` has nothing real left to pop, letting the caller name no
/// candidate rather than one that only LOOKS like a real path.
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
/// declaring file `path`'s own out-of-line children resolve under, read off the file's name
/// alone, never simply `path`'s own directory (the round-6 bug,
/// `sdet-u86c1-r6-out-of-line-mod-resolution-uses-declaring-files-directory-not-rusts-own-module-
/// nesting-path`). A file named `mod.rs`, `lib.rs` or `main.rs` owns its OWN directory: its
/// `mod name;` children are same-directory siblings. Every OTHER file is taken as a leaf submodule
/// whose children sit in a NEW subdirectory named after its own stem (`foo.rs` -> `foo/`).
/// The rule reads the file's name alone, while rustc decides by whether the file is a crate root,
/// a `mod.rs`, or loaded through `#[path]`, so the two part ways on some files. Among them: a
/// crate root of another name (`src/bin/tool.rs`, `build.rs`) and a `#[path]`-loaded file not
/// named `mod.rs`, `lib.rs` or `main.rs`, both of which this rule puts one directory deeper than
/// rustc; and a submodule file named `lib.rs` or `main.rs` (`src/sub/main.rs`), which this rule
/// gives its own directory where rustc uses its stem.
fn module_dir(path: &str) -> String {
    let dir = dir_of(path);
    let base = path.rsplit('/').next().unwrap_or(path);
    if base == "mod.rs" || base == "lib.rs" || base == "main.rs" {
        dir.to_string()
    } else {
        join_under(dir, base.strip_suffix(".rs").unwrap_or(base))
    }
}

/// The candidate files this resolver names for the `mod` declaration `d` in `declaring_path`, in
/// order, the first a caller holds winning (spec 117 criterion 8): a pure function of the
/// declaring path and the declaration, reading no index, so a caller matches them against
/// whatever files it holds. It applies Rust's file-per-module convention as the rules below
/// spell it, and it departs from rustc on some declarations. Among them: every file
/// [`module_dir`]'s name-only rule places differently from rustc; an enclosing inline mod carrying
/// its own `#[path]`, which contributes its name to the chain, not its path; a raw-identifier
/// declaration (`mod r#match;` names `r#match.rs` where rustc reads `match.rs`); a path given
/// through `#[cfg_attr(.., path = "..")]`, which is not taken as an override; and both
/// `<name>.rs` and `<name>/mod.rs` present, which rustc rejects (E0761) and where the first
/// candidate wins here. Round 9
/// (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`) sets out the
/// `#[path]`-override rules this code applies as one enumerated list:
/// 1. A `#[path = ".."]` override on the declaration ([`Def::path_override`], captured
///    structurally at extraction time) takes precedence over the convention entirely, naming one
///    file or none. BASE DIRECTORY: when the declaration sits at the declaring file's own top
///    level (the overwhelmingly common case), the base is the declaring file's own DIRECTORY
///    ([`dir_of`]) - never [`module_dir`], since `#[path]` is rustc's own escape hatch FROM the
///    file-per-module convention and is, at the file's top level, unconditionally
///    directory-of-file-relative regardless of whether the declaring file is itself a
///    directory-style module. When the declaration is instead nested inside one or more INLINE
///    `mod outer { .. }` blocks ([`Def::enclosing_inline_module_path`]), the base is the
///    declaring file's own MODULE directory ([`module_dir`]) with the enclosing inline chain
///    appended - one directory component per inline module, outermost first - verified against
///    real rustc for a plain inline parent such as `mod outer`
///    (`Def::enclosing_inline_module_path`'s doc gives examples where the chain departs).
///    ABSOLUTE / SCHEME REJECTION: an override that is itself an OS-absolute path, a URI scheme,
///    or a Windows drive letter
///    ([`path_override_names_an_unresolvable_shape`]) names no file, joined onto nothing - rustc
///    uses an absolute `#[path]` as-is, never relative to any Rust-side directory, so it can never
///    name a key a project-relative index holds; left unguarded, a root-level declaring file's
///    EMPTY base directory would let the join degenerate to the raw override unchanged, and its
///    leading `/` would then silently drop during normalization (the SAME rule that drops an
///    ordinary `.` segment), coincidentally colliding with an unrelated real file exactly like the
///    `..`-overflow defect below. NORMALIZATION: the joined `<base>/<override>` string is then
///    [`normalize_logical_path`]'d (round 8, `sdet-u86c1-r7-path-override-dotdot-unresolved` /
///    `adv-u86c1-r7-dot-slash-override-also-unresolved-not-just-dotdot`): a raw `#[path]` value
///    may itself contain `.` or `..` segments at any position (rustc resolves those purely
///    lexically too), and an index's keys are always already-clean, so an unnormalized join would
///    silently match no key for such a value. Round 9
///    (`adv-u86c1-r8-overcount-dotdot-silently-excludes-an-unrelated-real-file`): normalization can
///    itself fail - a `..` count exceeding the declaring directory's own depth - and that failure
///    names no file rather than a collapsed-but-wrong string that might coincidentally name a
///    real, unrelated file.
/// 2. Otherwise the convention's two, under the declaring file's [`module_dir`] with the enclosing
///    inline-mod chain appended, as the override nested in one takes it, so `mod outer { mod x; }`
///    in `src/lib.rs` names `src/outer/x.rs` first: the flat `<dir>/<name>.rs`, then the
///    directory-module form `<dir>/<name>/mod.rs`.
fn module_candidates(declaring_path: &str, d: &Def) -> Vec<String> {
    let module_dir = module_dir(declaring_path);
    let inline_dir = match &d.enclosing_inline_module_path {
        Some(chain) => join_under(&module_dir, chain),
        None => module_dir,
    };
    match &d.path_override {
        Some(p) if path_override_names_an_unresolvable_shape(p) => Vec::new(),
        Some(p) => {
            let base = match d.enclosing_inline_module_path {
                Some(_) => inline_dir.as_str(),
                None => dir_of(declaring_path),
            };
            normalize_logical_path(&join_under(base, p))
                .into_iter()
                .collect()
        }
        None => [format!("{}.rs", d.name), format!("{}/mod.rs", d.name)]
            .map(|leaf| join_under(&inline_dir, &leaf))
            .into(),
    }
}

/// Round 7: the one file `d.name` resolves to from `declaring_path`, matched against paths `idx`
/// ACTUALLY holds (never assumed): the first of its [`module_candidates`] `idx` holds, or `None`
/// when it holds none - a stale or unresolvable declaration excludes nothing and nothing collapses
/// onto another file. It is no closer to rustc than [`module_candidates`] is.
fn resolve_out_of_line_target(idx: &SymbolIndex, declaring_path: &str, d: &Def) -> Option<String> {
    module_candidates(declaring_path, d)
        .into_iter()
        .find(|candidate| idx.files().contains_key(candidate))
}

/// Round 6 (`op-u86c1-r5-close-every-remaining-test-shape` item 2), resolution fixed in round 7
/// (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): the set of file paths in `idx` that
/// are declared, OUT OF LINE, by a test-code `mod name;` item somewhere in another file (one under
/// `#[cfg(test)]`, or anywhere in a file opening `#![cfg(test)]`) - Rust's
/// out-of-tree module form, distinct from an INLINE `#[cfg(test)] mod name { .. }` (which already
/// excludes its own contents by containment, `Def::is_test`/`extract::test_regions`, needing no
/// cross-file lookup at all). A per-file extraction pass can structurally never see this on the
/// DECLARED file's own side - the attribute governing it lives in the DECLARING file's tree
/// entirely (`Def::is_out_of_line_module`'s doc) - so this resolves it here, at the events/index
/// layer, the ONE place every file's path in the project is already known together.
///
/// Two passes over [`resolve_out_of_line_target`]:
/// 1. SEED: every `Module`-kind definition that is itself out-of-line AND test code (`is_test`,
///    set by the test regions `extract::test_regions` reads - a `#[cfg(test)]` sibling of the
///    `mod name;` item, or a range holding it, such as a file opening `#![cfg(test)]`) contributes
///    its resolved target.
/// 2. CLOSURE: "Everything under a resolved test module file (its own nested out-of-line children,
///    resolved recursively by the same rule) is test code." A file pulled in by step 1 is now
///    wholly test code, so EVERY out-of-line module IT declares is excluded too, REGARDLESS of
///    whether that declaration line carries its own `#[cfg(test)]` - there is nothing left for it
///    to gate, since the whole file it lives in is already test-only. A worklist walks newly
///    excluded files to a fixed point; `excluded.insert` returning `false` for an already-seen
///    target both terminates the closure and guards against a cycle looping forever.
pub fn out_of_line_test_module_files(idx: &SymbolIndex) -> BTreeSet<String> {
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
pub(crate) fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Function => CODE_KIND_FUNCTION,
        Kind::Method => CODE_KIND_METHOD,
        Kind::Type => CODE_KIND_TYPE,
        Kind::Trait => CODE_KIND_TRAIT,
        Kind::Impl => CODE_KIND_IMPL,
        Kind::Module => CODE_KIND_MODULE,
        Kind::Constant => CODE_KIND_CONSTANT,
        Kind::Constructor => CODE_KIND_CONSTRUCTOR,
        Kind::Macro => CODE_KIND_MACRO,
        Kind::Declaration => CODE_KIND_DECLARATION,
        Kind::Static => CODE_KIND_STATIC,
        Kind::Variant => CODE_KIND_VARIANT,
        Kind::Field => CODE_KIND_FIELD,
        Kind::Variable => CODE_KIND_VARIABLE,
        Kind::Property => CODE_KIND_PROPERTY,
        Kind::Other => CODE_KIND_OTHER,
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

    /// Every event the production whole-project ingest ([`super::project_batches`]) emits for
    /// `root`, its per-file batches flattened in sorted file order.
    fn project_events(root: &str) -> Vec<crate::eventstore::Event> {
        super::project_batches(root)
            .into_iter()
            .flat_map(|(_, batch, _)| batch)
            .collect()
    }

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
        let events = project_events(dir.path().to_str().unwrap());

        // The extraction pass emitted at least one definition event and one reference event.
        assert!(
            events.iter().any(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED),
            "a definition emits a CodeEntityExtracted event"
        );
        assert!(
            events.iter().any(|e| e.type_ == TYPE_EDGE_INFERRED),
            "a reference emits an EdgeInferred event"
        );

        let p = crate::test_support::fold_events(events);

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
                    end_line: 9,
                    path: "beta".to_string(),
                    parent: None,
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                },
                Def {
                    kind: Kind::Function,
                    name: "alpha".to_string(),
                    line: 3,
                    end_line: 3,
                    path: "alpha".to_string(),
                    parent: None,
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
                ref_kind: None,
            }],
            partial: false,
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
                    ref_kind: None,
                },
                SymRef {
                    name: "apply".to_string(),
                    line: 4,
                    enclosing: None,
                    is_test: false,
                    ref_kind: None,
                },
            ],
            partial: false,
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
                end_line: 1,
                path: "F".to_string(),
                parent: None,
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
                    ref_kind: None,
                },
                // A top-level `use` outside every definition: no caller.
                SymRef {
                    name: "std_thing".to_string(),
                    line: 5,
                    enclosing: None,
                    is_test: false,
                    ref_kind: None,
                },
            ],
            partial: false,
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
            partial: false,
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

    /// The kinds spec 116 adds (the caller rule's at criterion 2, the Rust data definitions' at
    /// 6, Go's variable at 7, C#'s property at 8) each lower to their own kind string.
    #[test]
    fn each_kind_spec_116_adds_lowers_to_its_kind_string() {
        assert_eq!(
            [
                Kind::Constructor,
                Kind::Macro,
                Kind::Declaration,
                Kind::Static,
                Kind::Variant,
                Kind::Field,
                Kind::Variable,
                Kind::Property
            ]
            .map(super::kind_str),
            [
                "constructor",
                "macro",
                "declaration",
                "static",
                "variant",
                "field",
                "variable",
                "property"
            ]
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
                end_line: 1,
                path: "product_fn".into(),
                parent: None,
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
                ref_kind: None,
            }],
            partial: false,
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

    /// [`product_fs`] in the language `path`'s own extension registers, Rust when it registers
    /// none (the shape a `--language` override indexes a file in).
    fn product_fs_at(path: &str) -> FileSymbols {
        FileSymbols {
            lang: super::for_path(path, None).map_or(Lang::Rust, |entry| entry.lang),
            ..product_fs()
        }
    }

    /// The batch a file at `path` lowers [`product_fs_at`] to. Dev-only: the structural boundary
    /// alone, then its one reference as proof evidence at its line. Production: its definition,
    /// its reference as a structural edge, then the empty evidence boundary.
    fn expected_batch(path: &str, dev_only: bool) -> Vec<(String, String)> {
        let lang = super::lang_str(product_fs_at(path).lang);
        let event = |type_: &str, name: &str, rest: &str| {
            (
                type_.to_string(),
                format!(r#"{{"file":"{path}","name":"{name}",{rest}}}"#),
            )
        };
        let edge = |name: &str, tail: &str| {
            event(
                TYPE_EDGE_INFERRED,
                name,
                &format!(r#""lang":"{lang}"{tail}"#),
            )
        };
        if dev_only {
            return vec![
                edge("", r#","fresh":true"#),
                edge("product_fn", r#","fresh":true,"line":2,"is_test":true"#),
            ];
        }
        vec![
            event(
                TYPE_CODE_ENTITY_EXTRACTED,
                "product_fn",
                &format!(r#""kind":"function","line":1,"lang":"{lang}","fresh":true"#),
            ),
            edge("product_fn", ""),
            edge("", r#","fresh":true,"is_test":true"#),
        ]
    }

    /// What [`super::lower_file`] lowers [`product_fs_at`] to at `path`, outside the out-of-line
    /// set.
    fn lowered_at(path: &str) -> Vec<(String, String)> {
        crate::test_support::wire_owned(&super::lower_file(path, &product_fs_at(path), false))
    }

    /// DEV-ONLY CODE MAKES NO NODE (spec 116 criterion 1): a file each DEV-ONLY PATHS pattern
    /// names lowers to no definition and no structural edge, only the boundary, and its
    /// reference emits as proof evidence.
    #[test]
    fn a_file_each_dev_only_paths_pattern_names_lowers_to_the_boundary_and_its_proof() {
        for (path, pattern) in [
            ("tests/foo.rs", "a `tests` directory component"),
            ("crate/benches/b.rs", "a `benches` directory component"),
            ("examples/demo/main.rs", "an `examples` directory component"),
            ("web/__tests__/a.js", "a `__tests__` directory component"),
            ("pkg/x_test.go", "Go `*_test.go`"),
            ("app/test_views.py", "Python `test_*.py`"),
            ("app/views_test.py", "Python `*_test.py`"),
            ("shim/shim.test.mjs", "JS `*.test.*`"),
            ("web/a.spec.ts", "TS `*.spec.*`"),
            ("Acme.Test/A.cs", "a C# directory component ending `Test`"),
            ("Acme.Tests/A.cs", "a C# directory component ending `Tests`"),
        ] {
            assert_eq!(
                lowered_at(path),
                expected_batch(path, true),
                "{pattern}: {path}"
            );
        }
    }

    /// The gates and edges of DEV-ONLY PATHS, each still dev-only: every registered JS and TS
    /// extension, either marker in either language, a marker ahead of a further dot segment, the
    /// empty glob star, a pattern directory at any depth, and an ungated directory holding a
    /// file whose extension registers no language.
    #[test]
    fn a_dev_only_paths_pattern_holds_across_its_languages_and_at_its_edges() {
        for path in [
            "web/a.test.js",
            "web/a.test.cjs",
            "web/a.test.jsx",
            "web/a.test.ts",
            "web/a.test.tsx",
            "web/a.spec.js",
            "web/a.test.d.ts",
            "web/.spec.ts",
            "pkg/_test.go",
            "app/test_.py",
            "app/_test.py",
            "FooTest/A.cs",
            "FooTests/Sub/A.cs",
            "Test/A.cs",
            "a/b/tests/c/d.rs",
            "benches/b.go",
            "tests/notes.txt",
            "examples/README",
        ] {
            assert_eq!(lowered_at(path), expected_batch(path, true), "{path}");
        }
    }

    /// Every production control of DEV-ONLY PATHS, and the nearest path each pattern does not
    /// name, lowers its definition and its structural edge and emits no proof.
    #[test]
    fn a_production_control_of_dev_only_paths_lowers_its_definitions() {
        for (path, why) in [
            (
                "src/testsuite.rs",
                "a file name that reads close to `tests`",
            ),
            ("build.rs", "a root file with no directory"),
            ("Contest/a.cs", "the C# suffix is case-sensitive"),
            ("LoadTest/run.py", "the directory suffix is C#'s alone"),
            ("FooTests/lib.rs", "the directory suffix is C#'s alone"),
            ("src/a.test.rs", "`*.test.*` is JS and TS's alone"),
            ("src/a.spec.rs", "`*.spec.*` is JS and TS's alone"),
            (
                "src/tests.rs",
                "a pattern directory name as the file's own name",
            ),
            (
                "src/benches.rs",
                "a pattern directory name as the file's own name",
            ),
            (
                "src/examples.rs",
                "a pattern directory name as the file's own name",
            ),
            (
                "src/__tests__.js",
                "a pattern directory name as the file's own name",
            ),
            (
                "src/tests",
                "a pattern directory name as an extensionless file",
            ),
            (
                "testing/a.rs",
                "a directory that only starts with `tests`'s letters",
            ),
            ("mytests/a.rs", "a directory that only ends with `tests`"),
            ("pkg/xtest.go", "no `_` before `test`"),
            ("pkg/x_test.rs", "`*_test.go` is Go's alone"),
            ("pkg/test_x.go", "`test_*` is Python's alone"),
            ("pkg/x.test.go", "`*.test.*` is JS and TS's alone"),
            (
                "pkg/x_test.go.txt",
                "the gate reads the path's own extension",
            ),
            ("app/test.py", "no `_` after `test`"),
            ("app/tests_a.py", "`tests_` is not `test_`"),
            ("app/attest.py", "no `_` before `test`"),
            ("app/a_test_b.py", "`_test` is not the name's end"),
            ("app/a.spec.py", "`*.spec.*` is JS and TS's alone"),
            ("web/test.js", "no dot before `test`"),
            ("web/spec.ts", "no dot before `spec`"),
            ("web/a_test.js", "`*_test` is Go and Python's alone"),
            ("web/test_a.ts", "`test_*` is Python's alone"),
            (
                "web/atest.spec",
                "the marker is the extension, which registers no language",
            ),
            (
                "src/WidgetTests.cs",
                "the C# suffix is a directory's, never the file's",
            ),
            (
                "Acme.Tests.Core/A.cs",
                "the C# suffix is the last dot segment's",
            ),
            ("Testing/A.cs", "`Test` is not the directory's end"),
            ("Acme.tests/A.cs", "the C# suffix is case-sensitive"),
            (
                "Acme.Tests/.cs",
                "a dotfile has no extension, so no language",
            ),
        ] {
            assert_eq!(
                lowered_at(path),
                expected_batch(path, false),
                "{why}: {path}"
            );
        }
    }

    /// The whole walk over real sources: a `benches` file and a `*_test.go` file lower to the
    /// boundary and their references as proof, beside the production files that lower their
    /// definitions, and a `src/foo/tests/mod.rs` that `#[cfg(test)] mod tests;` declares, named
    /// by a DEV-ONLY PATHS pattern and by the out-of-line set, takes the set's rule: the
    /// boundary alone, no proof.
    #[test]
    fn the_walk_lowers_dev_only_files_to_proof_and_a_file_both_inputs_name_to_no_proof() {
        use crate::test_support::wire;

        let dir = tempfile::tempdir().unwrap();
        for (path, source) in [
            ("benches/b.rs", "fn bench() {\n    product();\n}\n"),
            ("pkg/x.go", "package pkg\n\nfunc Product() {}\n"),
            (
                "pkg/x_test.go",
                "package pkg\n\nfunc TestProduct() {\n\tProduct()\n}\n",
            ),
            (
                "src/foo/mod.rs",
                "#[cfg(test)]\nmod tests;\n\npub fn product() {}\n",
            ),
            (
                "src/foo/tests/mod.rs",
                "fn check() {\n    super::product();\n}\n",
            ),
        ] {
            crate::host_fixtures::write_file(&dir.path().join(path), source.as_bytes());
        }

        let batches = super::project_batches(dir.path().to_str().unwrap());
        let lowered: Vec<_> = batches
            .iter()
            .map(|(path, batch, excluded)| (path.as_str(), wire(batch), *excluded))
            .collect();
        assert_eq!(
            lowered,
            vec![
                (
                    "benches/b.rs",
                    vec![
                        (
                            TYPE_EDGE_INFERRED,
                            r#"{"file":"benches/b.rs","name":"","lang":"rust","fresh":true}"#
                        ),
                        (
                            TYPE_EDGE_INFERRED,
                            r#"{"file":"benches/b.rs","name":"product","lang":"rust","fresh":true,"line":2,"is_test":true}"#
                        ),
                    ],
                    false
                ),
                (
                    "pkg/x.go",
                    vec![
                        (
                            TYPE_CODE_ENTITY_EXTRACTED,
                            r#"{"file":"pkg/x.go","name":"Product","kind":"function","line":3,"lang":"go","fresh":true}"#
                        ),
                        (
                            TYPE_EDGE_INFERRED,
                            r#"{"file":"pkg/x.go","name":"","lang":"go","fresh":true,"is_test":true}"#
                        ),
                    ],
                    false
                ),
                (
                    "pkg/x_test.go",
                    vec![
                        (
                            TYPE_EDGE_INFERRED,
                            r#"{"file":"pkg/x_test.go","name":"","lang":"go","fresh":true}"#
                        ),
                        (
                            TYPE_EDGE_INFERRED,
                            r#"{"file":"pkg/x_test.go","name":"Product","lang":"go","fresh":true,"line":4,"is_test":true}"#
                        ),
                    ],
                    false
                ),
                (
                    "src/foo/mod.rs",
                    vec![
                        (
                            TYPE_CODE_ENTITY_EXTRACTED,
                            r#"{"file":"src/foo/mod.rs","name":"product","kind":"function","line":4,"lang":"rust","fresh":true}"#
                        ),
                        (
                            TYPE_EDGE_INFERRED,
                            r#"{"file":"src/foo/mod.rs","name":"","lang":"rust","fresh":true,"is_test":true}"#
                        ),
                    ],
                    false
                ),
                (
                    "src/foo/tests/mod.rs",
                    vec![(
                        TYPE_EDGE_INFERRED,
                        r#"{"file":"src/foo/tests/mod.rs","name":"","lang":"rust","fresh":true}"#
                    )],
                    true
                ),
            ]
        );
    }

    /// A TEST REGION IS EVERY TEST-ATTRIBUTED RANGE (spec 116 criterion 11): a `mod x;` past
    /// the first item of a file opening `#![cfg(test)]` is test code, so the file it declares
    /// joins the out-of-line test module set, while a `mod z;` in a file opening an outer
    /// `#[cfg(test)] use y;` leaves its file out.
    #[test]
    fn a_mod_declared_in_a_file_opening_an_inner_cfg_test_joins_the_out_of_line_test_module_files()
    {
        assert_out_of_line_test_module_files_hold_src_x(&[
            ("src/lib.rs", "#![cfg(test)]\nfn first() {}\nmod x;\n"),
            ("src/x.rs", crate::symbol_index::FN_Y),
            (
                "src/main.rs",
                "#[cfg(test)]\nuse y;\nfn first() {}\nmod z;\n",
            ),
            ("src/z.rs", "fn w() {}\n"),
        ]);
    }

    /// Asserts the out-of-line test module files of the Rust `files` are `src/x.rs` alone.
    fn assert_out_of_line_test_module_files_hold_src_x(files: &[(&str, &str)]) {
        assert_eq!(
            super::out_of_line_test_module_files(&crate::symbol_index::extracted_rust_index(files)),
            std::collections::BTreeSet::from(["src/x.rs".to_string()])
        );
    }

    /// Spec 117 criterion 8: the candidates [`super::module_candidates`] names, in order, for each
    /// declaration the table holds, read off the declaring path and the declaration alone - a
    /// `#[path]` override's one file under the declaring file's directory, or its module
    /// directory and inline chain when nested, none for an absolute or over-climbing override;
    /// else `x.rs` then `x/mod.rs` under the module directory and the enclosing inline-mod chain.
    #[test]
    fn module_candidates_name_the_candidates_of_each_tabled_declaration_in_order() {
        use crate::symbol_index::{ABSOLUTE_PATH_MOD_X, INLINE_MOD_X, MOD_X, PATH_MOD_X};
        let cases: [(&str, &str, &[&str]); 10] = [
            ("src/lib.rs", PATH_MOD_X, &["src/gen/real.rs"]),
            ("lib.rs", "#[path = \"real.rs\"]\nmod x;\n", &["real.rs"]),
            (
                "src/lib.rs",
                "mod outer {\n    #[path = \"real.rs\"]\n    mod x;\n}\n",
                &["src/outer/real.rs"],
            ),
            ("src/lib.rs", ABSOLUTE_PATH_MOD_X, &[]),
            ("src/lib.rs", "#[path = \"../../x.rs\"]\nmod x;\n", &[]),
            ("src/lib.rs", MOD_X, &["src/x.rs", "src/x/mod.rs"]),
            ("src/foo.rs", MOD_X, &["src/foo/x.rs", "src/foo/x/mod.rs"]),
            ("lib.rs", MOD_X, &["x.rs", "x/mod.rs"]),
            (
                "src/lib.rs",
                INLINE_MOD_X,
                &["src/outer/x.rs", "src/outer/x/mod.rs"],
            ),
            ("lib.rs", INLINE_MOD_X, &["outer/x.rs", "outer/x/mod.rs"]),
        ];
        for (declaring, src, candidates) in cases {
            let symbols = crate::symbol_index::extracted("rs", src);
            assert_eq!(
                super::module_candidates(declaring, out_of_line_declaration(&symbols)),
                candidates,
                "{declaring}: {src}"
            );
        }
    }

    /// Spec 117 criterion 8: a declaring file whose name lacks `.rs` (one indexed as Rust under a
    /// `--language` override) keeps its whole name as its module directory's last component, so
    /// `src/foo` names the candidates `src/foo.rs` does.
    #[test]
    fn module_candidates_of_a_declaring_file_without_the_rust_extension_keep_its_whole_name() {
        let symbols = crate::symbol_index::extracted("rs", crate::symbol_index::MOD_X);
        let declaration = &symbols.defs[0];
        assert_eq!(
            super::module_candidates("src/foo", declaration),
            super::module_candidates("src/foo.rs", declaration)
        );
    }

    /// Spec 117 criterion 8: a declaration resolves to the first of its [`super::module_candidates`]
    /// the index holds, so of `x.rs` and `x/mod.rs` both held, `#[cfg(test)] mod x;` excludes
    /// `x.rs` alone.
    #[test]
    fn a_test_mod_resolves_to_the_first_of_its_candidates_the_index_holds() {
        assert_out_of_line_test_module_files_hold_src_x(&[
            ("src/lib.rs", crate::symbol_index::CFG_TEST_MOD_X),
            ("src/x.rs", crate::symbol_index::FN_Y),
            ("src/x/mod.rs", crate::symbol_index::FN_Y),
        ]);
    }

    use crate::symbol_index::{
        INLINE_TEST_MOD_HELPER, INLINE_TEST_MOD_TARGET, INLINE_TEST_MOD_TREE,
    };

    /// Spec 117 criterion 8, A DEV-ONLY INLINE MOD'S TARGET IS DEV-ONLY: the file a
    /// `#[cfg(test)] mod x;` inside an inline `mod outer` names, `outer/x.rs` under the declaring
    /// file's module directory, joins the out-of-line test module files.
    #[test]
    fn an_inline_mods_cfg_test_mod_target_joins_the_out_of_line_test_module_files() {
        assert_eq!(
            super::out_of_line_test_module_files(&crate::symbol_index::extracted_rust_index(
                &INLINE_TEST_MOD_TREE
            )),
            std::collections::BTreeSet::from([INLINE_TEST_MOD_TARGET.to_string()])
        );
    }

    /// Spec 117 criterion 8: [`INLINE_TEST_MOD_TARGET`] folded as production, then re-recorded at
    /// its next generation under the flag the out-of-line test module files give it, retires its
    /// production nodes, its file node with them.
    #[test]
    fn an_inline_mods_cfg_test_mod_target_re_recorded_dev_only_retires_its_production_nodes() {
        use crate::test_support::{
            file_and_entity_ids, fold_events, fold_generation, live_file_and_entity_ids,
            GENERATION_AFTER, GENERATION_BEFORE,
        };

        let idx = crate::symbol_index::extracted_rust_index(&INLINE_TEST_MOD_TREE);
        let symbols = &idx.files()[INLINE_TEST_MOD_TARGET];
        let p = fold_events([]);
        let mut pos = 0;
        fold_generation(
            &p,
            &mut pos,
            INLINE_TEST_MOD_TARGET,
            GENERATION_BEFORE,
            super::lower_file(INLINE_TEST_MOD_TARGET, symbols, false),
        );
        assert_eq!(
            live_file_and_entity_ids(&p.whole().unwrap()),
            file_and_entity_ids(&[INLINE_TEST_MOD_TARGET], &[INLINE_TEST_MOD_HELPER]),
            "folded as production, the target holds its file and its definition"
        );
        let excluded = super::out_of_line_test_module_files(&idx).contains(INLINE_TEST_MOD_TARGET);
        fold_generation(
            &p,
            &mut pos,
            INLINE_TEST_MOD_TARGET,
            GENERATION_AFTER,
            super::lower_file(INLINE_TEST_MOD_TARGET, symbols, excluded),
        );
        assert_eq!(
            live_file_and_entity_ids(&p.whole().unwrap()),
            file_and_entity_ids(&[], &[]),
            "re-recorded under its dev-only flag, the target's production nodes retire"
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
                    end_line: 5,
                    path: "aaa_test_item".into(),
                    parent: None,
                    is_test: true,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                },
                Def {
                    kind: Kind::Function,
                    name: "product_fn".into(),
                    line: 1,
                    end_line: 1,
                    path: "product_fn".into(),
                    parent: None,
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
                    ref_kind: None,
                },
                SymRef {
                    name: "product_fn".into(),
                    line: 2,
                    enclosing: None,
                    is_test: false,
                    ref_kind: None,
                },
            ],
            partial: false,
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
        // REAL extraction + emit + fold pipeline (`project_batches` -> `Projector`).
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

        let events = project_events(dir.path().to_str().unwrap());

        let p = crate::test_support::fold_events(events);

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
    /// REAL pipeline (`project_batches` -> `Projector` -> `dash::card`), never a
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

        let events = project_events(dir.path().to_str().unwrap());

        let p = crate::test_support::fold_events(events);

        let g = p
            .subgraph(
                &["product.rs".to_string(), "tests/integration.rs".to_string()],
                3,
            )
            .unwrap();

        // PROVEN: product_fn is referenced by the in-file test (line 11) and the tests/-dir
        // integration test (line 3) - proven_by: 2, both file:lines, in fold order (product.rs
        // sorts before tests/integration.rs, so its own evidence lands first).
        let card = crate::contextgraph::query::card(&g, "product.rs::product_fn")
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
        let unused = crate::contextgraph::query::card(&g, "product.rs::unused_fn")
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
        // (`project_batches_paced`) could be empty and get DROPPED entirely, so
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
            partial: false,
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

    #[test]
    fn a_malformed_js_fixture_gets_the_partial_marker_end_to_end_and_a_well_formed_one_never_does()
    {
        // Spec 92 criterion 2 round 4 (review REJECT `adj-u2c2-r3-verdict-reject`, finding
        // `adv-u2c2-partial-marker-unimplemented`): the SAME REAL pipeline
        // (`project_batches` -> `Projector`) `a_source_file_extraction_emits_events_
        // the_fold_turns_into_a_code_graph` above proves for Rust, run here over a REAL malformed
        // `.js` fixture the grammar cannot fully parse alongside a well-formed one, folding both
        // into ONE graph. `broken.js`'s file node must carry the degraded `partial` marker;
        // `clean.js`'s must never carry it - proving detection (`extract::extract`'s
        // `has_error()` check), emission (`extract_events` threading `FileSymbols::partial`), and
        // the fold's `ensure_node` stamp all actually compose end to end, not merely in isolation.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("broken.js"),
            "function greet() { return 1; }\nfunction broken(\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("clean.js"),
            "function greet2() { return 2; }\n",
        )
        .unwrap();

        let events = project_events(dir.path().to_str().unwrap());

        let p = crate::test_support::fold_events(events);

        let g = p
            .subgraph(&["broken.js".to_string(), "clean.js".to_string()], 1)
            .unwrap();

        let broken = g
            .nodes
            .iter()
            .find(|n| n.kind == KIND_FILE && n.id == "broken.js")
            .expect("the malformed file still carries a KIND_FILE container node - it indexed as far as the parse reached");
        assert_eq!(
            broken.attrs.get("partial").map(String::as_str),
            Some("true"),
            "a real malformed-JS fixture's file node must carry the partial marker end to end; \
             got {:?}",
            broken.attrs
        );
        // The well-formed part of the SAME malformed file was still indexed, never dropped.
        assert!(
            g.nodes
                .iter()
                .any(|n| n.kind == KIND_CODE_ENTITY && n.id == "broken.js::greet"),
            "the well-formed function ahead of the parse error must still be indexed; got {:?}",
            g.nodes
        );

        let clean = g
            .nodes
            .iter()
            .find(|n| n.kind == KIND_FILE && n.id == "clean.js")
            .expect("the well-formed file's container node folded");
        assert_eq!(
            clean.attrs.get("partial"),
            None,
            "a real well-formed JS fixture must never carry the partial marker; got {:?}",
            clean.attrs
        );
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
            batches
                .iter()
                .map(|(f, _, _)| f.as_str())
                .collect::<Vec<_>>(),
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

    /// [`file_batches`] EXTENDS an ordinary (non-excluded) file's batch with its
    /// [`proof_events`] - the polarity of `if !excluded { events.extend(proof_events(..)) }`
    /// inside [`lower_file`]
    /// pinned directly: `product.rs` here is not an out-of-line test-module target
    /// (`out_of_line_test_module_files` returns it empty), so `excluded` is false and the
    /// extension MUST happen. `extract_events` filters every `is_test` def/ref OUT (see its own
    /// doc); `proof_events` is the ONLY emitter of `is_test: true` events, so their presence in
    /// the batch is observable only through this extension - a flipped polarity (`if
    /// excluded` instead) would extend the WRONG files and this batch would carry none.
    #[test]
    fn file_batches_extends_a_non_excluded_files_batch_with_its_proof_events() {
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
        let root = dir.path().to_str().unwrap();

        let batches = super::file_batches(root, &["product.rs".to_string()]);
        assert_eq!(batches.len(), 1, "one batch for the one named file");
        let proof_names: Vec<String> = batches[0]
            .1
            .iter()
            .filter(|e| e.type_ == TYPE_EDGE_INFERRED)
            .filter_map(|e| serde_json::from_slice::<serde_json::Value>(&e.data).ok())
            .filter(|v| v.get("is_test").and_then(|b| b.as_bool()) == Some(true))
            .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        assert_eq!(
            proof_names,
            vec!["product_fn".to_string()],
            "an ordinary file's batch must carry its proof_events test-evidence edge \
             (`is_test: true`, naming the referenced product entity); a polarity flip of \
             `if !excluded` inside `lower_file` drops it entirely. got batch: {:?}",
            batches[0].1
        );
    }

    /// THE EXTRACTION READS BYTES (spec 107): `lower_file` answers, for each `gc` file of the
    /// extraction tree, the batch the whole walk and the named walk lower from it - the
    /// out-of-line test module hollowed to its one boundary event, the source file carrying its
    /// definitions, its reference and the evidence of its in-file test module.
    #[test]
    fn lower_file_answers_the_batch_both_walks_lower_from_each_file_of_the_extraction_tree() {
        use crate::extraction_tree::{
            planted_extraction_tree, walked_batch, SOURCE_PATH, TEST_MODULE_PATH,
        };
        use crate::test_support::wire;

        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let root = dir.path().to_str().unwrap();
        let idx = crate::grounder::symbols::build_index(root, None);
        let files = [TEST_MODULE_PATH.to_string(), SOURCE_PATH.to_string()];

        let walked = super::project_batches_paced(root, 1).0;
        let named = super::file_batches(root, &files);
        let paths = |batches: &[(String, Vec<crate::eventstore::Event>, bool)]| -> Vec<String> {
            batches.iter().map(|(file, _, _)| file.clone()).collect()
        };
        assert_eq!(paths(&walked), files.to_vec());
        assert_eq!(paths(&named), files.to_vec());

        for (i, (path, excluded)) in [(TEST_MODULE_PATH, true), (SOURCE_PATH, false)]
            .into_iter()
            .enumerate()
        {
            let expected = walked_batch("gc", path).to_vec();
            let lowered = super::lower_file(path, &idx.files()[path], excluded);
            assert_eq!(wire(&lowered), expected, "lower_file over {path}");
            assert_eq!(wire(&walked[i].1), expected, "the whole walk over {path}");
            assert_eq!(wire(&named[i].1), expected, "the named walk over {path}");
        }
    }

    /// THE WALK HANDS EACH BATCH WITH ITS FLAG (spec 107), at the code half: the whole walk and
    /// the named walk each answer, beside a file's batch, whether `walk_exclusions` names its
    /// `gc` identity - set for the out-of-line test module alone, clear for the source file and
    /// for a named path the index lacks.
    #[test]
    fn both_walks_of_the_code_half_answer_each_batchs_flag_beside_it() {
        use crate::extraction_tree::{planted_extraction_tree, SOURCE_PATH, TEST_MODULE_PATH};

        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let root = dir.path().to_str().unwrap();
        let flags =
            |batches: &[(String, Vec<crate::eventstore::Event>, bool)]| -> Vec<(String, bool)> {
                batches
                    .iter()
                    .map(|(file, _, excluded)| (file.clone(), *excluded))
                    .collect()
            };
        let walked = [
            (TEST_MODULE_PATH.to_string(), true),
            (SOURCE_PATH.to_string(), false),
        ];

        assert_eq!(flags(&super::project_batches_paced(root, 1).0), walked);
        assert_eq!(flags(&super::project_batches(root)), walked);
        let named = [
            SOURCE_PATH.to_string(),
            "src/absent.rs".to_string(),
            TEST_MODULE_PATH.to_string(),
        ];
        assert_eq!(
            flags(&super::file_batches(root, &named)),
            [
                (SOURCE_PATH.to_string(), false),
                ("src/absent.rs".to_string(), false),
                (TEST_MODULE_PATH.to_string(), true),
            ]
        );
    }

    /// `lower_file`'s `excluded` decides the batch on the SAME symbols: the source file excluded
    /// is hollowed to one boundary event with no evidence, and the test module not excluded
    /// carries its own definition and reference with the evidence boundary after them.
    #[test]
    fn lower_file_hollows_an_excluded_file_and_adds_evidence_to_one_that_is_not() {
        use crate::extraction_tree::{planted_extraction_tree, SOURCE_PATH, TEST_MODULE_PATH};
        use crate::test_support::wire;

        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let idx = crate::grounder::symbols::build_index(dir.path().to_str().unwrap(), None);

        let hollowed = super::lower_file(SOURCE_PATH, &idx.files()[SOURCE_PATH], true);
        assert_eq!(
            wire(&hollowed),
            vec![(
                TYPE_EDGE_INFERRED,
                r#"{"file":"src/lib.rs","name":"","lang":"rust","fresh":true}"#
            )]
        );

        let kept = super::lower_file(TEST_MODULE_PATH, &idx.files()[TEST_MODULE_PATH], false);
        assert_eq!(
            wire(&kept),
            vec![
                (
                    TYPE_CODE_ENTITY_EXTRACTED,
                    r#"{"file":"src/checks.rs","name":"checks_product","kind":"function","line":2,"lang":"rust","fresh":true}"#
                ),
                (
                    TYPE_EDGE_INFERRED,
                    r#"{"file":"src/checks.rs","name":"product","lang":"rust","caller":"checks_product"}"#
                ),
                (
                    TYPE_EDGE_INFERRED,
                    r#"{"file":"src/checks.rs","name":"","lang":"rust","fresh":true,"is_test":true}"#
                ),
            ]
        );
    }

    /// THE REBUILD RE-EXTRACTS THE LEDGER (spec 107): `bytes_batch` over a `gc` file's UTF-8
    /// bytes answers the batch the walk lowers from that file - the source file's whole batch,
    /// and the out-of-line test module hollowed to its one boundary event under `excluded`.
    #[test]
    fn bytes_batch_answers_the_batch_the_walk_lowers_from_a_files_bytes() {
        use crate::extraction_tree::{
            walked_batch, SOURCE_BODY, SOURCE_PATH, TEST_MODULE_BODY, TEST_MODULE_PATH,
        };
        use crate::test_support::wire;

        for (path, body, excluded) in [
            (SOURCE_PATH, SOURCE_BODY, false),
            (TEST_MODULE_PATH, TEST_MODULE_BODY, true),
        ] {
            assert_eq!(
                wire(&super::bytes_batch(path, Some(body.as_bytes()), excluded)),
                walked_batch("gc", path).to_vec(),
                "the bytes of {path}"
            );
        }
    }

    /// `excluded` changes a parsed batch: the source file's bytes under the flag are hollowed to
    /// one boundary event of the file's own language.
    #[test]
    fn bytes_batch_hollows_a_parsed_file_under_the_excluded_flag() {
        use crate::extraction_tree::{SOURCE_BODY, SOURCE_PATH};
        use crate::test_support::wire;

        assert_eq!(
            wire(&super::bytes_batch(
                SOURCE_PATH,
                Some(SOURCE_BODY.as_bytes()),
                true
            )),
            vec![(
                TYPE_EDGE_INFERRED,
                r#"{"file":"src/lib.rs","name":"","lang":"rust","fresh":true}"#
            )]
        );
    }

    /// Every input `bytes_batch` cannot parse - no bytes, bytes that are not UTF-8, a path no
    /// grammar resolves - answers exactly the one boundary event of an unknown language, whatever
    /// the flag says, as the named walk gives a path the index lacks.
    #[test]
    fn bytes_batch_answers_the_unknown_boundary_for_every_input_it_cannot_parse() {
        use crate::extraction_tree::{SOURCE_BODY, SOURCE_PATH};
        use crate::test_support::wire_owned;

        let boundary = |path: &str| {
            vec![(
                TYPE_EDGE_INFERRED.to_string(),
                format!(r#"{{"file":"{path}","name":"","lang":"unknown","fresh":true}}"#),
            )]
        };
        let not_utf8: &[u8] = &[b'f', b'n', 0xff, 0xfe];
        for (path, bytes, excluded) in [
            (SOURCE_PATH, None, false),
            (SOURCE_PATH, None, true),
            (SOURCE_PATH, Some(not_utf8), false),
            (SOURCE_PATH, Some(not_utf8), true),
            ("notes.txt", Some(SOURCE_BODY.as_bytes()), false),
            ("notes.txt", Some(SOURCE_BODY.as_bytes()), true),
        ] {
            assert_eq!(
                wire_owned(&super::bytes_batch(path, bytes, excluded)),
                boundary(path),
                "{path} with bytes {bytes:?} under excluded {excluded}"
            );
        }
        // The named walk's batch for a path the index lacks is that same event.
        let dir = tempfile::tempdir().unwrap();
        let named =
            super::file_batches(dir.path().to_str().unwrap(), &["src/absent.rs".to_string()]);
        assert_eq!(wire_owned(&named[0].1), boundary("src/absent.rs"));
    }

    /// `file`'s real extraction of `source`, the registered query its extension names, lowered by
    /// `extract_events`.
    fn extracted_events(file: &str, source: &str) -> Vec<Event> {
        let (_, ext) = file
            .rsplit_once('.')
            .expect("the fixture file has an extension");
        extract_events(file, &crate::symbol_index::extracted(ext, source))
    }

    /// A fresh graph holding [`extracted_events`] of `file` and `source`, folded.
    fn folded_extraction(file: &str, source: &str) -> Projector {
        let p = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds(
            &p,
            &crate::event_fixtures::positioned(extracted_events(file, source)),
        );
        p
    }

    /// Spec 119, IDS FOLLOW THE LANGUAGE'S PATH: each file's real extraction, folded, names every
    /// definition `<file>::<path>` - its enclosing definitions' path, `::`, its leaf, `.` before a
    /// field or property; an impl contributing its self type or `<T as Trait>`, a Go method its
    /// receiver's base type, a C# file-scoped namespace every declaration after it - and equal-range
    /// siblings, a name-filtered `_` and a named value equal to its declarator add nothing.
    #[test]
    fn each_extracted_definition_folds_at_its_language_path_id() {
        use crate::csharp_definition_sources::CSHARP_FILE_SCOPED_NAMESPACE;
        use crate::go_definition_sources::GO_CLOSURE_VAR;
        use crate::test_support::{definition_ids, ENUM_E, VARIANT_E_A, VARIANT_E_B};

        let cases: [(&str, &str, &[&str]); 7] = [
            (
                "a.rs",
                "struct T;\nstruct U;\nimpl T {\n    fn new() -> T { T }\n    const NAME: u8 = 1;\n}\n\
                 impl U {\n    fn new() -> U { U }\n}\ntrait Show {\n    fn show(&self);\n}\n\
                 impl Show for T {\n    fn show(&self) {}\n}\nmod m {\n    struct V;\n    impl V {\n        \
                 fn make() {}\n    }\n}\nenum E {\n    A,\n    B { a: u8 },\n}\nstruct S {\n    \
                 field: u8,\n}\nstruct P(u8);\nfn outer() {\n    fn inner() {}\n}\nstruct Foo<X>(X);\n\
                 impl Foo<u8> {\n    fn new() {}\n}\nconst _: () = {\n    fn a() {}\n};\n\
                 const _: () = {\n    fn a() {}\n};\n",
                &[
                    "a.rs::<T as Show>::show",
                    ENUM_E,
                    VARIANT_E_A,
                    VARIANT_E_B,
                    "a.rs::E::B.a",
                    "a.rs::Foo",
                    "a.rs::Foo.0",
                    "a.rs::Foo<u8>::new",
                    "a.rs::P",
                    "a.rs::P.0",
                    "a.rs::S",
                    "a.rs::S.field",
                    "a.rs::Show",
                    "a.rs::Show::show",
                    "a.rs::T",
                    "a.rs::T::NAME",
                    "a.rs::T::new",
                    "a.rs::U",
                    "a.rs::U::new",
                    "a.rs::a",
                    "a.rs::impl Foo<u8>",
                    "a.rs::impl Show for T",
                    "a.rs::impl T",
                    "a.rs::impl U",
                    "a.rs::m",
                    "a.rs::m::V",
                    "a.rs::m::V::make",
                    "a.rs::m::impl V",
                    "a.rs::outer",
                    "a.rs::outer::inner",
                ],
            ),
            (
                "g.go",
                "package g\n\ntype Recv struct{}\n\nfunc (r *Recv) Name() {}\n\nconst a, b = f(), g()\n",
                &["g.go::Recv", "g.go::Recv::Name", "g.go::a", "g.go::b"],
            ),
            (
                GO_CLOSURE_VAR.0,
                GO_CLOSURE_VAR.1,
                &["h.go::T", "h.go::a", "h.go::b"],
            ),
            (
                "c.ts",
                "class C {\n    f = 1;\n}\nconst f = function f() {};\nconst Foo = class Foo {};\n\
                 function g() {\n    return function g() {};\n}\n",
                &["c.ts::C", "c.ts::C.f", "c.ts::Foo", "c.ts::f", "c.ts::g", "c.ts::g::g"],
            ),
            (
                "n.cs",
                "namespace N {\n    class Foo {\n        Foo() {}\n        int P { get; set; }\n    }\n}\n",
                &["n.cs::N", "n.cs::N::Foo", "n.cs::N::Foo.P", "n.cs::N::Foo::Foo"],
            ),
            (
                CSHARP_FILE_SCOPED_NAMESPACE.0,
                CSHARP_FILE_SCOPED_NAMESPACE.1,
                &["s.cs::N", "s.cs::N::Foo"],
            ),
            (
                "p.py",
                "def outer():\n    def inner():\n        pass\n",
                &["p.py::outer", "p.py::outer::inner"],
            ),
        ];
        for (file, source, ids) in cases {
            let p = folded_extraction(file, source);
            assert_eq!(definition_ids(&p), ids, "{file}: each definition's path id");
        }
    }

    /// Spec 119, IDS FOLLOW THE LANGUAGE'S PATH: the fold names a `CodeEntityExtracted` carrying a
    /// `path` `<file>::<path>`, its `name` staying the leaf, and one without `path`, or whose
    /// `path` is empty, `<file>::<name>`, as every event recorded before `path` existed folds.
    #[test]
    fn an_event_folds_at_its_path_and_at_its_name_without_one() {
        use crate::event_fixtures::positioned;
        use crate::test_support::{
            attrs_of, code_entity_json, definition_ids, event_of, folds, path_entity_json,
        };

        let p = Projector::open(":memory:", "test").unwrap();
        folds(
            &p,
            &positioned(vec![
                event_of(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    path_entity_json("x.rs", "new", "T::new", "method", 2, "rust"),
                ),
                event_of(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    code_entity_json("x.rs", "run", "function", 5, "rust"),
                ),
                event_of(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    path_entity_json("x.rs", "go", "", "function", 7, "rust"),
                ),
            ]),
        );
        assert_eq!(
            definition_ids(&p),
            ["x.rs::T::new", "x.rs::go", "x.rs::run"],
            "a path names its node; an absent or empty one the name"
        );
        assert_eq!(
            attrs_of(&p, "x.rs::T::new").get("name").map(String::as_str),
            Some("new"),
            "the node's name stays its leaf"
        );
    }

    /// Spec 119, CONTAINS IS STRUCTURE: each file's real extraction, folded, hangs every
    /// definition's `CONTAINS` off its parent - a variant off its enum, a field off its variant,
    /// an associated item off its impl, an item off its inline mod or enclosing fn, a C#
    /// file-scoped namespace's class off the namespace, a Go method off the receiver type its file
    /// defines - and the file's off the file alone: a top-level definition, an impl of a type the
    /// file does not define, a Go method whose receiver the file does not define, what a
    /// name-filtered `_` holds and a type inside a closure two equal-range siblings share.
    #[test]
    fn each_definitions_contains_comes_from_its_parent_and_the_files_only_from_the_file() {
        use crate::csharp_definition_sources::CSHARP_FILE_SCOPED_NAMESPACE;
        use crate::go_definition_sources::GO_CLOSURE_VAR;
        use crate::test_support::{assert_live, ENUM_E, VARIANT_E_A, VARIANT_E_B};

        let cases = [
            (
                "a.rs",
                "enum E {\n    A,\n    B { a: u8 },\n}\nstruct T;\nimpl T {\n    fn new() -> T { T }\n}\n\
                 impl Display for X {\n    fn fmt(&self) {}\n}\nmod m {\n    fn item() {}\n}\n\
                 fn outer() {\n    fn inner() {}\n}\nconst _: () = {\n    fn a() {}\n};\n",
                &[
                    ("a.rs", ENUM_E),
                    ("a.rs", "a.rs::T"),
                    ("a.rs", "a.rs::a"),
                    ("a.rs", "a.rs::impl Display for X"),
                    ("a.rs", "a.rs::impl T"),
                    ("a.rs", "a.rs::m"),
                    ("a.rs", "a.rs::outer"),
                    (ENUM_E, VARIANT_E_A),
                    (ENUM_E, VARIANT_E_B),
                    (VARIANT_E_B, "a.rs::E::B.a"),
                    ("a.rs::impl Display for X", "a.rs::<X as Display>::fmt"),
                    ("a.rs::impl T", "a.rs::T::new"),
                    ("a.rs::m", "a.rs::m::item"),
                    ("a.rs::outer", "a.rs::outer::inner"),
                ][..],
            ),
            (
                "g.go",
                "package g\n\ntype Recv struct{}\n\nfunc (r *Recv) Name() {}\n\nfunc (o *Other) Run() {}\n",
                &[
                    ("g.go", "g.go::Other::Run"),
                    ("g.go", "g.go::Recv"),
                    ("g.go::Recv", "g.go::Recv::Name"),
                ][..],
            ),
            (
                GO_CLOSURE_VAR.0,
                GO_CLOSURE_VAR.1,
                &[("h.go", "h.go::T"), ("h.go", "h.go::a"), ("h.go", "h.go::b")][..],
            ),
            (
                CSHARP_FILE_SCOPED_NAMESPACE.0,
                CSHARP_FILE_SCOPED_NAMESPACE.1,
                &[("s.cs", "s.cs::N"), ("s.cs::N", "s.cs::N::Foo")][..],
            ),
        ];
        for (file, source, contains) in cases {
            assert_live(
                &folded_extraction(file, source),
                REL_CONTAINS,
                contains,
                &format!("{file}: each definition's CONTAINS comes from its parent"),
            );
        }
    }

    /// Spec 119, CONTAINS IS STRUCTURE: a removed variant's `CONTAINS` from its enum retires with
    /// its file's next generation, folded from the real extraction of each.
    #[test]
    fn a_removed_variants_contains_retires_at_the_next_generation() {
        use crate::rust_definition_sources::{ENUM_E_HOLDING_A, ENUM_E_HOLDING_A_AND_B};
        use crate::test_support::{
            assert_live, fold_events, fold_generation, E_HOLDS_A, GENERATION_AFTER,
            GENERATION_BEFORE,
        };

        let p = fold_events([]);
        let mut pos = 0;
        for (generation, source) in [
            (GENERATION_BEFORE, ENUM_E_HOLDING_A_AND_B),
            (GENERATION_AFTER, ENUM_E_HOLDING_A),
        ] {
            fold_generation(
                &p,
                &mut pos,
                "a.rs",
                generation,
                extracted_events("a.rs", source),
            );
        }
        assert_live(
            &p,
            REL_CONTAINS,
            E_HOLDS_A,
            "the removed variant's CONTAINS retires with the generation",
        );
    }

    /// Spec 119, CONTAINS IS STRUCTURE: a named value one with the class field holding it takes
    /// the field's parent, so the one event of `class A { f = function f() {} }`'s `f` names `A`,
    /// and the file-level `A` names none.
    #[test]
    fn a_named_value_takes_the_parent_of_the_definition_it_is_one_with() {
        use crate::event_fixtures::payloads_of_type;

        let events = extracted_events("c.ts", "class A {\n    f = function f() {};\n}\n");
        let parents: Vec<(serde_json::Value, serde_json::Value)> =
            payloads_of_type(&events, TYPE_CODE_ENTITY_EXTRACTED)
                .into_iter()
                .map(|payload| (payload["name"].clone(), payload["parent"].clone()))
                .collect();
        assert_eq!(
            parents,
            [
                ("A".into(), serde_json::Value::Null),
                ("f".into(), "A".into())
            ],
            "each event's (name, parent)"
        );
    }

    /// A CALLER IS NAMED BY ITS PATH (spec 119 criterion 8): `extract_events` copies the caller's
    /// path into `EdgeInferred.caller`, so a call in `impl T { fn new() }` folds its `CALLS` from
    /// `<file>::T::new` and one in a Go receiver method from `<file>::Recv::name`, while a
    /// hand-built caller naming a leaf folds from `<file>::<leaf>`.
    #[test]
    fn a_call_folds_its_calls_edge_from_its_caller_s_path() {
        use crate::caller_sources::{
            GO_RECEIVER_CALLER_CALLS, GO_RECEIVER_CALLER_SOURCE, RUST_METHOD_CALLER_CALLS,
            RUST_METHOD_CALLER_SOURCE,
        };
        use crate::test_support::{apply_call, assert_live_calls, def};

        assert_live_calls(
            &folded_extraction("a.rs", RUST_METHOD_CALLER_SOURCE),
            RUST_METHOD_CALLER_CALLS,
            "the call in `impl T { fn new() }` folds from `T::new`'s path",
        );
        assert_live_calls(
            &folded_extraction("g.go", GO_RECEIVER_CALLER_SOURCE),
            GO_RECEIVER_CALLER_CALLS,
            "the calls in the receiver method `name` fold from `Recv::name`'s path",
        );

        let p = Projector::open(":memory:", "test").unwrap();
        def(&p, 1, "x.rs", "go");
        apply_call(&p, 2, "x.rs", "go", "run");
        assert_live_calls(
            &p,
            &[("x.rs::run", "x.rs::go")],
            "a hand-built leaf caller folds from <file>::<leaf>",
        );
    }

    /// Each `CodeEntityExtracted` payload of `events` beside its path, in emission order, the path
    /// its name where the payload carries none.
    fn emitted_payloads(events: &[Event]) -> Vec<(String, serde_json::Value)> {
        crate::event_fixtures::payloads_of_type(events, TYPE_CODE_ENTITY_EXTRACTED)
            .into_iter()
            .map(|payload| {
                let path = payload["path"].as_str().or(payload["name"].as_str());
                (path.unwrap().to_owned(), payload)
            })
            .collect()
    }

    /// Each `CodeEntityExtracted` of `events` as `<path>@<line>`, in emission order.
    fn emitted_definitions(events: &[Event]) -> Vec<String> {
        emitted_payloads(events)
            .into_iter()
            .map(|(path, payload)| format!("{path}@{}", payload["line"]))
            .collect()
    }

    /// Each `CodeEntityExtracted` of `events` as `<kind> <path>@<line> in <parent>`, in emission
    /// order, the parent `-` where the payload carries none.
    fn emitted_entities(events: &[Event]) -> Vec<String> {
        emitted_payloads(events)
            .into_iter()
            .map(|(path, payload)| {
                let kind = payload["kind"].as_str().unwrap();
                let parent = payload["parent"].as_str().unwrap_or("-");
                format!("{kind} {path}@{} in {parent}", payload["line"])
            })
            .collect()
    }

    /// A PARENT IS EMITTED BEFORE ITS CHILD (spec 119 criterion 16): the real extraction emits a
    /// variant whose leaf sorts before its enum's after the enum, and a method nested in an impl
    /// inside a mod after the impl, the type it is for and the mod - each definition after every
    /// definition containing it, a depth's definitions by leaf.
    #[test]
    fn extract_events_emits_each_definition_after_every_definition_containing_it() {
        let events = extracted_events(
            "a.rs",
            "enum Z {\n    A,\n}\nmod m {\n    struct B;\n    impl B {\n        fn a() {}\n    }\n}\n",
        );
        assert_eq!(
            emitted_definitions(&events),
            ["Z@1", "m@4", "Z::A@2", "m::B@5", "m::impl B@6", "m::B::a@7"],
            "each definition after every definition containing it"
        );
    }

    /// A definition of `name` at `line` on `path` under `parent`, as `extract` would set them.
    fn placed(name: &str, line: u32, path: &str, parent: Option<&str>) -> Def {
        Def {
            line,
            path: path.into(),
            parent: parent.map(Into::into),
            ..crate::symbol_index::def(Kind::Function, name)
        }
    }

    /// The batch `extract_events` lowers from a Rust file holding `defs` and no reference.
    fn lowered_rust(defs: Vec<Def>) -> Vec<Event> {
        extract_events("a.rs", &crate::symbol_index::rust_file(defs, Vec::new()))
    }

    /// [`emitted_definitions`] of [`lowered_rust`] over `defs`.
    fn emitted_from(defs: Vec<Def>) -> Vec<String> {
        emitted_definitions(&lowered_rust(defs))
    }

    /// A PARENT IS EMITTED BEFORE ITS CHILD (spec 119 criterion 16): the emission sorts by the
    /// length of each definition's parent chain - a parent counting one link, resolved to the one
    /// definition emitted at its path, and one naming no emitted definition still one link - then
    /// by leaf, then by path, the definitions emitted being one per path (spec 119 criterion 10).
    #[test]
    fn extract_events_orders_by_parent_chain_then_leaf_then_path() {
        let emitted = emitted_from(vec![
            placed("t", 9, "t", None),
            placed("t", 8, "t", None),
            placed("n", 1, "U::n", None),
            placed("n", 2, "T::n", None),
            placed("a", 3, "Z::a", None),
            placed("b", 4, "A::b", None),
            placed("P", 10, "P", None),
            placed("Q", 11, "Q", None),
            placed("P", 12, "P", Some("Q")),
            placed("c", 13, "P::c", Some("P")),
            placed("d", 14, "Q::d", Some("Q")),
            placed("o", 15, "gone::o", Some("gone")),
            placed("b", 16, "lost::b", Some("lost")),
        ]);
        assert_eq!(
            emitted,
            [
                "P@10",
                "Q@11",
                "Z::a@3",
                "A::b@4",
                "T::n@2",
                "U::n@1",
                "t@8",
                "lost::b@16",
                "P::c@13",
                "Q::d@14",
                "gone::o@15",
            ],
            "by parent chain, then leaf, then path, one event per path"
        );
    }

    /// ONE PATH IS ONE EVENT (spec 119 criterion 10): each file's real extraction emits one event
    /// per path, its winner the first by `CODE_KIND_PRECEDENCE`, then the lower line, with the
    /// winner's own kind, line and parent: `fn x` over `mod x;`, the first of two cfg twins and of
    /// two impls with one header, the production `fn x` over a `#[cfg(test)]` twin at a lower
    /// line, the function of `const f = function f() {}`, and the field `B` of `class A { B }`
    /// over a `namespace A.B` above it.
    #[test]
    fn extract_events_emits_one_event_per_path_the_first_by_kind_precedence_then_line() {
        let cases: [(&str, &str, &[&str]); 6] = [
            ("a.rs", "mod x;\nfn x() {}\n", &["function x@2 in -"]),
            (
                "a.rs",
                "#[cfg(unix)]\nfn x() {}\n#[cfg(not(unix))]\nfn x() {}\n",
                &["function x@2 in -"],
            ),
            (
                "a.rs",
                "struct T;\nimpl T {}\nimpl T {}\n",
                &["type T@1 in -", "impl impl T@2 in -"],
            ),
            (
                "a.rs",
                "#[cfg(test)]\nfn x() {}\nfn x() {}\n",
                &["function x@3 in -"],
            ),
            (
                "c.ts",
                "const f = function f() {};\n",
                &["function f@1 in -"],
            ),
            (
                "c.ts",
                crate::js_ts_definition_sources::NAMESPACE_A_B_BESIDE_CLASS_A_FIELD_B,
                &["type A@2 in -", "field A.B@3 in A"],
            ),
        ];
        for (file, source, winners) in cases {
            assert_eq!(
                emitted_entities(&extracted_events(file, source)),
                winners,
                "{source:?}: one event per path"
            );
        }
    }

    /// ONE PATH IS ONE EVENT (spec 119 criterion 10): of same-path twins of one precedence the
    /// lower line wins though `extract` returns it last, and of twins on one line the first
    /// `extract` returns, each with its own parent.
    #[test]
    fn extract_events_keeps_the_lower_line_then_the_first_extract_returns() {
        let events = lowered_rust(vec![
            placed("t", 9, "t", None),
            placed("t", 8, "t", Some("A")),
            placed("x", 1, "x", Some("A")),
            placed("x", 1, "x", Some("B")),
        ]);
        assert_eq!(
            emitted_entities(&events),
            ["function t@8 in A", "function x@1 in A"],
            "the lower line, then the first extract returns"
        );
    }

    /// ONE PATH IS ONE EVENT (spec 119 criterion 10): each generation re-derives the winner, so
    /// removing the twin that won leaves the other's line and parent.
    #[test]
    fn a_removed_twin_leaves_the_other_s_line_and_parent() {
        use crate::rust_definition_sources::{
            METHOD_C_IN_THE_FILE_LEVEL_IMPL, METHOD_C_IN_TWO_IMPLS,
        };
        let twin_events = |source: &str| -> Vec<String> {
            emitted_entities(&extracted_events("a.rs", source))
                .into_iter()
                .filter(|entity| entity.contains(" a::B::c@"))
                .collect()
        };
        assert_eq!(
            twin_events(METHOD_C_IN_TWO_IMPLS),
            ["method a::B::c@4 in a::impl B"],
            "both twins: the lower line wins"
        );
        assert_eq!(
            twin_events(METHOD_C_IN_THE_FILE_LEVEL_IMPL),
            ["method a::B::c@8 in impl a::B"],
            "the winner removed: the other's line and parent"
        );
    }

    /// Spec 117 criterion 6: a production file's const read, type reference and `use` each
    /// emit an `EdgeInferred` carrying its `ref_kind`, while the ones inside its `#[cfg(test)]`
    /// region and every one of a dev-only file emit no tagged event, and the evidence a file's
    /// batch carries ([`super::proof_events`]) names its call references alone.
    #[test]
    fn test_code_and_dev_only_files_emit_no_tagged_event_and_evidence_is_calls_alone() {
        let src = "\
use crate::store::Ledger;
pub fn product(k: Kestrel) -> u8 { QUOTA_CAP }
#[cfg(test)]
mod tests {
    use crate::model::Plover;
    const LOCAL_CAP: u8 = 1;
    fn t(p: Plover) -> u8 { super::product(make()); LOCAL_CAP }
}
";
        let production = [
            ("Kestrel", "type"),
            ("Ledger", "use"),
            ("QUOTA_CAP", "const"),
        ];
        for (path, tagged) in [("src/a.rs", &production[..]), ("tests/a.rs", &[][..])] {
            let edges = crate::event_fixtures::payloads_of_type(
                &super::bytes_batch(path, Some(src.as_bytes()), false),
                TYPE_EDGE_INFERRED,
            );
            let kinds = crate::event_fixtures::tagged_payloads(&edges);
            assert_eq!(kinds, tagged, "{path}: the tagged events");
            let evidence: Vec<&str> = edges
                .iter()
                .filter(|p| p["is_test"] == true)
                .filter_map(|p| p["name"].as_str())
                .collect();
            assert_eq!(evidence, ["make", "product"], "{path}: the evidence");
        }
    }

    /// Spec 117 criterion 6: an impl header's trait `type` row is emitted after the untagged
    /// reference of the same name and site, from the self type's path, carrying its `ref_kind`.
    #[test]
    fn an_impl_trait_row_follows_the_untagged_header_reference() {
        let fs = crate::symbol_index::extracted(
            "rs",
            "trait Draw {}\nstruct Kestrel;\nimpl Draw for Kestrel {}\n",
        );
        let edges = crate::event_fixtures::payloads_of_type(
            &super::extract_events("a.rs", &fs),
            TYPE_EDGE_INFERRED,
        );
        let refs = crate::event_fixtures::named_references(&edges);
        let header = Some("impl Draw for Kestrel");
        assert_eq!(
            refs,
            [
                (Some("Draw"), header, None),
                (Some("Draw"), Some("Kestrel"), Some("type")),
                (Some("Kestrel"), header, None),
            ]
        );
    }

    /// Spec 117 criterion 1, STAGED REFERENCES: over the real extraction folded as ledger
    /// entries, `src/a.rs`'s `use` of a fn, its field naming a type and its fn reading a const,
    /// each defined once in `src/b.rs`, are each one `inferred` `REFERENCES` edge onto that
    /// definition whichever file folds first, and none once `src/c.rs` defines each leaf again.
    #[test]
    fn a_use_a_field_type_and_a_const_read_reach_their_one_definition_in_either_fold_order() {
        use crate::contextgraph::TIER_INFERRED;
        use crate::test_support::landed;

        let a = (
            "src/a.rs",
            "use crate::b::helper;\npub struct Nest {\n    k: Kestrel,\n}\n\
             pub fn read() -> u8 {\n    QUOTA_CAP\n}\n",
        );
        let b = (
            "src/b.rs",
            "pub const QUOTA_CAP: u8 = 1;\npub struct Kestrel;\npub fn helper() {}\n",
        );
        let c = ("src/c.rs", b.1);
        let references = |p: &Projector| -> Vec<(String, String, String)> {
            landed(p)
                .into_iter()
                .filter(|(_, rel, ..)| rel == REL_REFERENCES)
                .map(|(from, _, to, tier)| (from, to, tier))
                .collect()
        };
        let resolved: Vec<(String, String, String)> = [
            ("src/a.rs", "src/b.rs::helper"),
            ("src/a.rs::Nest.k", "src/b.rs::Kestrel"),
            ("src/a.rs::read", "src/b.rs::QUOTA_CAP"),
        ]
        .map(|(from, to)| (from.into(), to.into(), TIER_INFERRED.into()))
        .into();
        for order in [[a, b], [b, a]] {
            let p = Projector::open(":memory:", "test").unwrap();
            fold_source(&p, 1, order[0]);
            fold_source(&p, 2, order[1]);
            assert_eq!(references(&p), resolved, "{:?} first", order[0].0);
            fold_source(&p, 3, c);
            assert_eq!(
                references(&p),
                [],
                "a second definition of each leaf leaves every row unresolved"
            );
        }
    }

    /// The one out-of-line `mod` declaration `symbols` holds.
    fn out_of_line_declaration(symbols: &FileSymbols) -> &Def {
        symbols
            .defs
            .iter()
            .find(|d| d.is_out_of_line_module)
            .expect("the source declares a module out of line")
    }

    /// Fold `file` holding `source` as generation `g1` of its ledger entry at log position `pos`,
    /// valid from `pos` seconds, its batch the walk's ([`super::bytes_batch`]) - the fold that
    /// stages a row.
    fn fold_source(p: &Projector, pos: u64, (file, source): (&str, &str)) {
        let batch = super::bytes_batch(file, Some(source.as_bytes()), false);
        crate::test_support::fold_asked(p, file, "g1", pos, pos, batch);
    }

    /// Spec 117 criterion 3: [`super::extract_events`] lowers an out-of-line `mod` declaration to
    /// one module row from its `mod` node's path, named by its [`super::module_candidates`]
    /// joined by `\n`, and a declaration naming no candidate to none.
    #[test]
    fn extract_events_lowers_a_mod_declaration_to_one_module_row_naming_its_candidates() {
        use crate::contextgraph::REF_KIND_MODULE;
        use crate::graph_fixtures::DECLARING_FILE;
        use crate::symbol_index::{ABSOLUTE_PATH_MOD_X, INLINE_MOD_X, MOD_X};
        for source in [MOD_X, INLINE_MOD_X, ABSOLUTE_PATH_MOD_X] {
            let symbols = crate::symbol_index::extracted("rs", source);
            let declaration = out_of_line_declaration(&symbols);
            let candidates = super::module_candidates(DECLARING_FILE, declaration).join("\n");
            let edges = crate::event_fixtures::payloads_of_type(
                &super::extract_events(DECLARING_FILE, &symbols),
                TYPE_EDGE_INFERRED,
            );
            let rows: Vec<_> = crate::event_fixtures::named_references(&edges)
                .into_iter()
                .filter(|(.., kind)| *kind == Some(REF_KIND_MODULE))
                .collect();
            let expected: Vec<_> = (!candidates.is_empty())
                .then_some((
                    Some(candidates.as_str()),
                    Some(declaration.path.as_str()),
                    Some(REF_KIND_MODULE),
                ))
                .into_iter()
                .collect();
            assert_eq!(rows, expected, "{source}");
        }
    }

    /// Spec 117 criterion 3, A MODULE ROW REACHES ITS FILE: over the real extraction folded as
    /// ledger entries, each tabled declaration in [`DECLARING_FILE`] - a `#[path]` override,
    /// `x.rs`, `x/mod.rs`, both, and `mod outer { mod x; }` - is one `extracted` `CONTAINS` from
    /// its `mod` node onto the first of its [`super::module_candidates`] holding a `file` node,
    /// whichever file folds first, and none while no candidate holds one.
    ///
    /// [`DECLARING_FILE`]: crate::graph_fixtures::DECLARING_FILE
    #[test]
    fn a_mod_declaration_reaches_the_first_candidate_holding_a_file_node_in_either_fold_order() {
        use crate::contextgraph::{query::entity_id, TIER_EXTRACTED};
        use crate::graph_fixtures::DECLARING_FILE;
        use crate::symbol_index::{FN_Y, INLINE_MOD_X, MOD_X, PATH_MOD_X};
        use crate::test_support::module_edges;
        // (the declaring source, the candidates folded as files by index, the one reached)
        let cases: [(&str, &[usize], usize); 5] = [
            (PATH_MOD_X, &[0], 0),
            (MOD_X, &[0], 0),
            (MOD_X, &[1], 1),
            (MOD_X, &[0, 1], 0),
            (INLINE_MOD_X, &[0], 0),
        ];
        for (source, held, reached) in cases {
            let symbols = crate::symbol_index::extracted("rs", source);
            let declaration = out_of_line_declaration(&symbols);
            let candidates = super::module_candidates(DECLARING_FILE, declaration);
            let edge = (
                entity_id(DECLARING_FILE, &declaration.path),
                candidates[reached].clone(),
                TIER_EXTRACTED.to_string(),
            );
            for declared_first in [true, false] {
                let p = Projector::open(":memory:", "test").unwrap();
                let mut pos = 0;
                let mut fold = |file: &str, source: &str| {
                    pos += 1;
                    fold_source(&p, pos, (file, source));
                };
                if declared_first {
                    fold(DECLARING_FILE, source);
                    assert_eq!(module_edges(&p), [], "{source}: no candidate holds a file");
                }
                for &i in held {
                    fold(&candidates[i], FN_Y);
                }
                if !declared_first {
                    fold(DECLARING_FILE, source);
                }
                assert_eq!(
                    module_edges(&p),
                    std::slice::from_ref(&edge),
                    "{source}: declared first {declared_first}"
                );
            }
        }
    }

    /// Spec 117 criterion 3: a `mod x;` in dev-only code stages no row - one under
    /// `#[cfg(test)]`, one in a file under `tests/`, and one in a file the walk lowers under its
    /// excluded flag - so its first candidate folded as a file gains no module edge.
    #[test]
    fn a_mod_declaration_in_dev_only_code_stages_no_row() {
        use crate::graph_fixtures::DECLARING_FILE;
        use crate::symbol_index::{CFG_TEST_MOD_X, FN_Y, MOD_X};
        use crate::test_support::module_edges;
        let under_tests = format!("tests/{DECLARING_FILE}");
        let cases = [
            (DECLARING_FILE, CFG_TEST_MOD_X, false),
            (under_tests.as_str(), MOD_X, false),
            (DECLARING_FILE, MOD_X, true),
        ];
        for (file, source, excluded) in cases {
            let symbols = crate::symbol_index::extracted("rs", source);
            let target = &super::module_candidates(file, out_of_line_declaration(&symbols))[0];
            let p = Projector::open(":memory:", "test").unwrap();
            let batch = super::bytes_batch(file, Some(source.as_bytes()), excluded);
            crate::test_support::fold_asked(&p, file, "g1", 1, 1, batch);
            fold_source(&p, 2, (target, FN_Y));
            assert_eq!(
                module_edges(&p),
                [],
                "{file}: {source}, excluded {excluded}"
            );
        }
    }
}
