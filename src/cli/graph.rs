use super::*;

pub(crate) fn cmd_graph(args: &[String]) -> Res {
    // `rigger graph build` is a distinct verb (populate) from the default `--around` inspector
    // read: fold the project's source into the graph from a cold checkout, no run required.
    if args.first().map(String::as_str) == Some("build") {
        return cmd_graph_build(&args[1..]);
    }
    // `rigger graph communities` is the OFFLINE detection pass (spec 53): derive the code lens's
    // coupling communities over the already-folded structure layer and record them as events.
    if args.first().map(String::as_str) == Some("communities") {
        return cmd_graph_communities(&args[1..]);
    }
    // `rigger graph concepts` is the OFFLINE intent-derivation pass (spec 54): derive the concepts
    // lens's grouping over the already-folded intent layer and record them as events.
    if args.first().map(String::as_str) == Some("concepts") {
        return cmd_graph_concepts(&args[1..]);
    }
    let mut around = String::new();
    let mut show = String::new();
    let mut depth: i64 = 2;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--around" => {
                i += 1;
                around = args.get(i).cloned().unwrap_or_default();
            }
            "--show" => {
                i += 1;
                show = args.get(i).cloned().unwrap_or_default();
            }
            "--depth" => {
                i += 1;
                depth = args.get(i).and_then(|d| d.parse().ok()).unwrap_or(2);
            }
            _ => {}
        }
        i += 1;
    }
    // `rigger graph --show <entity>` is the TEXT half of lookup (spec 58): the definition site and
    // body, beside `--around`'s structural neighborhood. Dispatched before the `--around` guard so
    // a `--show` query needs no `--around`.
    if !show.is_empty() {
        return cmd_graph_show(&show);
    }
    if around.is_empty() {
        return Err("graph: --around <id> or --show <entity> is required".into());
    }
    let gp = Projector::open(&db_path("graph.db"), &project_identity())?;
    let g = gp.subgraph(&[around.clone()], depth)?;
    println!("subgraph around {around:?} (depth {depth}):");
    print_around_subgraph(&g, &around);
    Ok(())
}

/// The number of newest governing decision/finding nodes `rigger graph --around` prints in full
/// before collapsing the rest into a trailing count (spec 92, u92c6 - "a file's neighborhood is
/// code first"). Ten is a page, not a cliff: enough to read at a glance, small enough that
/// decision spam never crowds the code entities off the screen the way it did before this fix -
/// the u88c1 evidence recorded a loop agent grepping `conductor.rs` because `--around` returned
/// "only generic decision-node spam, not code structure".
const AROUND_GOVERNANCE_CAP: usize = 10;

/// Print a `rigger graph --around` subgraph CODE FIRST (spec 92, u92c6). Before this fix
/// [`cmd_graph`] printed every node and edge [`Projector::subgraph`] returned in one
/// undifferentiated, unbounded list - a decision or finding node is indistinguishable in shape
/// from a code entity, and a file governed by dozens of rounds' worth of decisions buried its
/// own structure under them (the u88c1 evidence this criterion fixes).
///
/// Two sections, never interleaved:
/// - CODE FIRST: every node that is NOT a [`contextgraph::KIND_DECISION`] /
///   [`contextgraph::KIND_FINDING`] - a file, a code entity, a design doc, a community, anything
///   structural - sorted by id for a deterministic read, followed by every edge whose BOTH
///   endpoints are in that same set (a decision's `GOVERNS` / a finding's `ABOUT` edge, which
///   always terminates on a narrative node, is never one of these - the narrative section speaks
///   for itself as a node list).
/// - GOVERNING DECISIONS/FINDINGS, separately and capped: every [`contextgraph::KIND_DECISION`] /
///   [`contextgraph::KIND_FINDING`] node, ranked NEWEST first and capped to
///   [`AROUND_GOVERNANCE_CAP`], with a trailing count of however many more this subgraph held.
///   "Newest" is the event log POSITION of the node's OWN `GOVERNS` (decision) / `ABOUT`
///   (finding) edge - via [`conductor::recency_by_own_edge`], the SAME from-side-only core
///   `conductor::write_capped_section` dates the prompt's decisions/lessons/findings sections
///   with - never an edge that merely touches the node as `to` (a superseded decision's
///   inbound `SUPERSEDES` edge carries its superseder's fresh position, which would let the
///   stale decision crowd a live one out of the cap if it counted).
fn print_around_subgraph(g: &contextgraph::Graph, around: &str) {
    use std::collections::{BTreeMap, BTreeSet};

    let is_narrative =
        |kind: &str| kind == contextgraph::KIND_DECISION || kind == contextgraph::KIND_FINDING;

    let mut code_nodes: Vec<&contextgraph::Node> =
        g.nodes.iter().filter(|n| !is_narrative(&n.kind)).collect();
    code_nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let code_ids: BTreeSet<&str> = code_nodes.iter().map(|n| n.id.as_str()).collect();

    for n in &code_nodes {
        println!("  node {:<24} {}", n.id, n.kind);
    }
    for e in &g.edges {
        if code_ids.contains(e.from.as_str()) && code_ids.contains(e.to.as_str()) {
            println!("  edge {} -{}-> {}", e.from, e.rel, e.to);
        }
    }

    // Recency per node: the SAME from-side-only core `write_capped_section` uses for the
    // prompt's decisions/lessons/findings sections (`conductor::recency_by_own_edge`), never a
    // scan of every edge touching a node as either endpoint. A decision is dated off its own
    // `GOVERNS` edge, a finding off its own `ABOUT` edge - both point node -> file, so `from`
    // is always the narrative node itself. Keying on either endpoint (as an earlier version of
    // this function did) let a superseded decision inherit its superseder's fresh position
    // through the inbound `SUPERSEDES` edge (`from` = the new decision, `to` = the superseded
    // one) and crowd a genuinely live decision out of the newest-`AROUND_GOVERNANCE_CAP` slice
    // while printing the stale one as if current - the exact bug class `write_capped_section`'s
    // own doc comment already fixed once; this reuses that fix rather than re-deriving it.
    let mut recency: BTreeMap<&str, Position> =
        conductor::recency_by_own_edge(g, contextgraph::REL_GOVERNS);
    for (id, pos) in conductor::recency_by_own_edge(g, contextgraph::REL_ABOUT) {
        let slot = recency.entry(id).or_insert(0);
        *slot = (*slot).max(pos);
    }

    let mut narrative_nodes: Vec<&contextgraph::Node> =
        g.nodes.iter().filter(|n| is_narrative(&n.kind)).collect();
    narrative_nodes.sort_by(|a, b| {
        let ra = recency.get(a.id.as_str()).copied().unwrap_or(0);
        let rb = recency.get(b.id.as_str()).copied().unwrap_or(0);
        // Newest (highest position) first; the id breaks a tie so the page is deterministic.
        rb.cmp(&ra).then_with(|| a.id.cmp(&b.id))
    });

    if !narrative_nodes.is_empty() {
        println!();
        let shown = narrative_nodes.len().min(AROUND_GOVERNANCE_CAP);
        println!(
            "  {} governing decision(s)/finding(s) (newest {shown} shown):",
            narrative_nodes.len()
        );
        for n in narrative_nodes.iter().take(AROUND_GOVERNANCE_CAP) {
            println!("  node {:<24} {}", n.id, n.kind);
        }
        let rest = narrative_nodes.len().saturating_sub(AROUND_GOVERNANCE_CAP);
        if rest > 0 {
            println!(
                "  (+{rest} more decision(s)/finding(s) not shown - see `rigger peers {around}` for the full history)"
            );
        }
    }

    if g.nodes.is_empty() {
        println!("  (nothing found; has `rigger run` been run yet?)");
    }
}

/// The upper bound on how many body lines `rigger graph --show` prints (spec 58): the definition's
/// extent comes from the grammar's own node boundary, but a very long body is clamped to this window
/// so the surface never dumps an unbounded body. A clamp is announced with an explicit note (the
/// omitted-line count), so a bounded body is never mistaken for the whole definition.
const SHOW_MAX_BODY_LINES: u32 = 60;

/// `rigger graph --show <entity>` (spec 58, criterion 1) - the TEXT half of graph lookup, beside
/// `--around`'s structural neighborhood. It resolves the entity through [`Projector::locate`] (a
/// full `<file>::<name>` id, or a bare name via the pinned name-suffix match), then:
///
/// - ONE match: prints the definition SITE (`file:line`), the entity's KIND and one-hop DEGREE, and
///   the definition BODY read from the WORKING TREE at that location - line-numbered and bounded by
///   the extent the shared multi-grammar symbols authority derives (the grammar's own node
///   boundary), clamped to a max window. A missing file, a recorded line past end-of-file, a drifted
///   location the current tree no longer matches, or a build without the extraction grammar degrades
///   to the site plus an explicit note, never an error (the recorded graph facts are still shown;
///   only the body is unavailable, and the surface says so).
/// - MANY matches (an ambiguous bare name): LISTS the sorted candidates, each with its file, and
///   prints NO body - the graph's honesty rule is never to guess among candidates.
/// - NONE: a one-line not-found note (never an error), mirroring `--around`'s empty result.
///
/// Read-only over the projection and the working tree; deterministic for a given tree and graph.
fn cmd_graph_show(entity: &str) -> Res {
    let gp = Projector::open(&db_path("graph.db"), &project_identity())?;
    match gp.locate(entity)? {
        Located::None => {
            println!(
                "show {entity:?}: no such entity in the graph (has it been built? try `rigger graph build`)"
            );
        }
        Located::Many(cands) => {
            println!(
                "show {entity:?}: {} candidates - the name is ambiguous, re-run --show on one id:",
                cands.len()
            );
            for c in &cands {
                println!("  {}   ({})", c.id, c.file);
            }
        }
        Located::One(site) => print_entity_site(&site),
    }
    Ok(())
}

/// Print one located entity for `rigger graph --show` (spec 58; spec 92 criterion 1, FRESH ON EVERY
/// INTEGRATION, for the header): the site/kind/degree header, then the line-numbered body bounded
/// through the shared multi-grammar symbols authority - or an explicit note (a drifted location this
/// tree cannot resolve even by name, or a build without the extraction grammar) in place of the body,
/// so the surface is never silently wrong (a graceful degrade, never an error). When
/// [`definition_body`] HEALS a drifted recorded line to the entity's live one, the header prints the
/// LIVE site with the recorded one noted alongside it (`"recorded line N, now M"`) rather than
/// silently swapping one location for the other with no trace of the drift.
fn print_entity_site(site: &contextgraph::EntitySite) {
    let kind = if site.kind.is_empty() {
        "?"
    } else {
        site.kind.as_str()
    };
    // The definition name is the id's suffix after the first `::` (a file path never contains one),
    // the twin of the `<file>::<name>` id `locate` resolved - used to match the working-tree extent.
    let name = site
        .id
        .split_once("::")
        .map(|(_, n)| n)
        .unwrap_or(site.id.as_str());
    println!("show {}", site.id);
    match definition_body(&site.file, site.line, name) {
        ShowBody::Lines {
            start,
            lines,
            omitted,
            extent_end,
        } => {
            print_site_header(site, kind, start);
            for (n, text) in lines {
                println!("  {n:>6} | {text}");
            }
            if omitted > 0 {
                // The extent ran past the max window: print an explicit clamp note (the omitted
                // count and the extent's true last line) so a bounded body is never read as whole.
                println!(
                    "  (body clamped to {SHOW_MAX_BODY_LINES} lines; {omitted} more line(s) omitted, through line {extent_end})"
                );
            }
        }
        ShowBody::Note(reason) => {
            print_site_header(site, kind, site.line);
            println!("  ({reason})");
        }
    }
}

/// The site/kind/degree header line (spec 92 criterion 1): `live_line` is where the body actually
/// came from (or, for a [`ShowBody::Note`], simply the recorded line - nothing was located). When it
/// agrees with the entity's RECORDED line (`site.line`) - the overwhelmingly common case, and every
/// case before spec 92 - the header is exactly the spec-58 shape. When a name-only fallback healed a
/// drifted recorded line to a different live one, the header shows the LIVE line as the site (it is
/// what the body below is FROM) and notes the recorded line beside it, so the drift is visible rather
/// than silently resolved.
fn print_site_header(site: &contextgraph::EntitySite, kind: &str, live_line: u32) {
    if live_line != site.line {
        println!(
            "  site: {}:{}   kind {}   degree {}   (recorded line {}, now {})",
            site.file, live_line, kind, site.degree, site.line, live_line
        );
    } else {
        println!(
            "  site: {}:{}   kind {}   degree {}",
            site.file, site.line, kind, site.degree
        );
    }
}

/// The outcome of bounding a located definition's body for `rigger graph --show` (spec 58; spec 92
/// criterion 1).
enum ShowBody {
    /// The line-numbered body window `[start, end]`: `start` is where the body actually begins - the
    /// entity's RECORDED line when it still holds the definition, or the LIVE line a name-only
    /// fallback healed a drift to (spec 92) - so the caller always knows which line the printed body
    /// is from. `omitted` is how many lines were dropped past the [`SHOW_MAX_BODY_LINES`] clamp (`0`
    /// when the whole extent fit), and `extent_end` is the extent's true last line, so the caller can
    /// print an honest clamp note when `omitted > 0`.
    Lines {
        start: u32,
        lines: Vec<(u32, String)>,
        omitted: u32,
        extent_end: u32,
    },
    /// No body could be shown; the string is the human reason (a drifted working-tree location this
    /// tree cannot resolve even by name, or a build compiled without the extraction grammar).
    /// Printed in place of the body so the show surface degrades honestly, never guessing or
    /// silently truncating.
    Note(String),
}

/// Bound and read a located definition's body from the WORKING TREE for `rigger graph --show`
/// (spec 58; spec 92 criterion 1, FRESH ON EVERY INTEGRATION). The file is read relative to the git
/// top-level (so a `--show` launched from a subdirectory still finds it), falling back to the cwd
/// outside a git context.
///
/// The extent is derived through the SHARED multi-grammar symbols authority, not a hand-rolled
/// per-language lexer: [`locate_definition_extent`] resolves the file's grammar via the symbols
/// registry and reads the definition's line range from the grammar's OWN tree-sitter node boundary.
/// So a braced language's closing brace, a Python block's dedent, a Go backtick raw string, and a JS
/// single-quote string carrying a lone `{` are all bounded correctly by the parser - including a
/// signature that itself carries a brace (a struct-destructuring parameter, an `= {}` default) and
/// a definition that CONTAINS a nested `fn`/item (its extent spans the child, never truncates at
/// it). The window is `[start, extent]` (the RESOLVED `start` - see below), clamped by
/// [`SHOW_MAX_BODY_LINES`]; a clamp reports its omitted-line count so a bounded body is never read
/// as whole.
///
/// The recorded `start` no longer holding the definition (the graph has not been reindexed since the
/// code moved) is not, by itself, a reason to refuse: [`locate_definition_extent`] falls back to
/// locating `name` by a name-only search of the SAME file, healing to the live line when that name is
/// UNAMBIGUOUS there. [`ShowBody::Lines::start`] then carries that LIVE line rather than the recorded
/// one, so the caller's header can show the drift instead of hiding it.
///
/// Returns [`ShowBody::Note`] - the caller prints it in place of the body, never an error - when the
/// body cannot be shown honestly: the recorded `start` line is `0` or past end-of-file (a location
/// that never named a real source line, or one that has drifted past what a within-file name search
/// can safely resolve - see [`locate_definition_extent`]'s own doc for why these stay hard refusals),
/// the file cannot be read (a drifted or unknown location), the current tree holds no definition of
/// that name ANYWHERE in the file (deleted, not merely moved), the name is ambiguous in the file (more
/// than one live candidate - never guessed), or this build has no extraction grammar (the light,
/// `--no-default-features` lane). It never GUESSES a body from a structural next-definition bound or
/// from an ambiguous candidate.
fn definition_body(file: &str, start: u32, name: &str) -> ShowBody {
    // A recorded line of 0 never named a real source line: degrade before any read. (Also covers a
    // reference-only graph entity with no definition site of its own to search from - see
    // `locate_definition_extent`'s doc for why this stays a hard, un-healed refusal.)
    if start == 0 {
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    }
    let root = git_repo();
    let path = if root.is_empty() {
        std::path::PathBuf::from(file)
    } else {
        std::path::Path::new(&root).join(file)
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    };
    let all: Vec<&str> = text.lines().collect();
    let total = all.len() as u32;
    if start > total {
        // The recorded line is past end-of-file: the location drifted further than a within-file
        // name search is asked to reach (see locate_definition_extent's doc) - a hard refusal.
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    }
    // Resolve WHERE the body starts (the recorded line, or - spec 92 - a healed live line) and its
    // extent's end, through the ONE multi-grammar authority. A miss (deleted, ambiguous, or a
    // light-lane build with no grammar) is an explicit note, never a guessed body.
    let (live_start, extent_end) = match locate_definition_extent(file, &text, start, name) {
        Ok((s, e)) => (s, e.min(total)),
        Err(why) => return ShowBody::Note(why),
    };
    // The max window: never dump an unbounded body. A clamp keeps the extent's true end so the
    // caller can announce the omitted lines. The window is anchored at the RESOLVED start, so a
    // healed drift is bounded exactly like an unmoved definition would be.
    let window_cap = live_start
        .saturating_add(SHOW_MAX_BODY_LINES)
        .saturating_sub(1);
    let printed_end = extent_end.max(live_start).min(window_cap);
    let omitted = extent_end.saturating_sub(printed_end);
    let lines = (live_start..=printed_end)
        .map(|n| (n, all[(n - 1) as usize].to_string()))
        .collect();
    ShowBody::Lines {
        start: live_start,
        lines,
        omitted,
        extent_end,
    }
}

/// The 1-based, inclusive `(start, end)` line range of the definition named `name`, derived through
/// the shared multi-grammar symbols authority (spec 58; spec 92 criterion 1, FRESH ON EVERY
/// INTEGRATION). It resolves the file's grammar via the symbols registry and reads every candidate
/// extent from [`definition_extents`], the SAME tree-sitter tag mechanism the code graph is extracted
/// with - so ONE extent authority generalizes across every ingested grammar rather than a Rust-only
/// brace lexer in this composition root.
///
/// Two tiers, in order:
///
/// 1. **Exact match** at the RECORDED `start` line: the graph's location is still current, so the
///    resolved start is `start` itself (no drift) - when several definitions share the name and
///    line, the widest extent (the outermost construct) wins, exactly as before spec 92.
/// 2. **Name-only fallback** (spec 92: the recorded line no longer holds the definition - the code
///    moved since the graph was last indexed): every candidate named `name`, AT ANY LINE in this
///    file, is collected. Healing is safe ONLY when that leaves exactly ONE live line - with more
///    than one candidate the surface cannot tell which the caller meant, so it degrades exactly as
///    every drift did before spec 92 (never guess a body under the wrong name or line - the spec-58
///    invariant this filter exists to hold). The fallback NEVER crosses files and NEVER matches a
///    different name: only the identity `graph --show` was asked to resolve is ever searched for.
///
/// Returns `Err` with a human reason - the caller degrades to a note - when no grammar is registered
/// for the file's extension, the grammar cannot tag the source, no definition named `name` exists
/// anywhere in the file (deleted, not moved), or the name is ambiguous (tier 2 found more than one
/// live candidate).
#[cfg(feature = "symbols")]
fn locate_definition_extent(
    file: &str,
    source: &str,
    start: u32,
    name: &str,
) -> Result<(u32, u32), String> {
    use rigger::grounder::symbols::{extract, registry};
    let Some(entry) = registry::for_path(file, None) else {
        return Err(format!(
            "no code-extraction grammar is registered for {file}; the body extent is unavailable"
        ));
    };
    let extents = extract::definition_extents(source, &entry.language, entry.tags_query)?;
    // Tier 1: exact (name, recorded line) match - the graph's location is current. Widest extent
    // wins when several definitions share the name and line (unchanged spec-58 rule).
    if let Some(end) = extents
        .iter()
        .filter(|d| d.name == name && d.start_line == start)
        .map(|d| d.end_line)
        .max()
    {
        return Ok((start, end));
    }
    // Tier 2 (spec 92): the recorded line drifted - re-locate `name` by a name-only search of this
    // SAME file. `by_line` groups candidate lines (never two rows for one line: the widest extent
    // per line wins, matching tier 1's own rule), so its length is the count of DISTINCT live lines
    // this name occupies - the ambiguity measure healing must stay safe against.
    let mut by_line: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    for d in extents.iter().filter(|d| d.name == name) {
        by_line
            .entry(d.start_line)
            .and_modify(|end| *end = (*end).max(d.end_line))
            .or_insert(d.end_line);
    }
    match by_line.len() {
        0 => Err(format!(
            "no definition named {name:?} at line {start} in the current working tree; the recorded location may be stale"
        )),
        1 => Ok(by_line.into_iter().next().expect("len == 1")),
        n => Err(format!(
            "{n} definitions named {name:?} exist in {file} and none starts at the recorded line \
             {start}; the recorded location may be stale (cannot resolve which one moved)"
        )),
    }
}

/// Light-lane [`locate_definition_extent`]: a build WITHOUT the `symbols` feature links no grammar,
/// so no extent - recorded or healed - can be derived. It returns an explicit reason the caller
/// prints as a note - the show surface stays honest ("the body needs the extraction grammar this
/// build omits") rather than falling back to a hand-rolled lexer that would mis-read the very
/// grammars the graph ingests.
#[cfg(not(feature = "symbols"))]
fn locate_definition_extent(
    _file: &str,
    _source: &str,
    _start: u32,
    _name: &str,
) -> Result<(u32, u32), String> {
    Err(
        "the body extent needs the code-extraction grammar; this build was compiled without the `symbols` feature"
            .to_string(),
    )
}

/// `rigger graph build` - fold the project's source into `.rigger/graph.db` from a COLD checkout
/// (spec 45): no run, no `RunStarted`, no event beyond the code-ingest events the fold already
/// emits, so the graph exists on any repo the tool has merely cloned - not only ones a run has
/// driven. It reuses the SAME walk-and-content-key ingest authority ([`rigger::ingest::ingest_project_batched`])
/// the live run uses; only this standalone entry is new, so a build and a run can never fork the
/// key an event is deduped under.
///
/// Store lifecycle mirrors the RUN DRIVER, not the couriers: it CREATES the store under the cwd's
/// `.rigger/` when absent (a cold checkout legitimately has none yet - this command's whole point
/// is to populate it) rather than the courier walk-up that refuses a missing store. On an EXISTING
/// store it refreshes incrementally through the ONE first-sight helper
/// ([`rigger::ingest::batch_is_latest_recorded`]) the live run's keyed sink also calls: each batch
/// is weighed against its identity's LATEST recorded generation, answered by the store's group
/// lookup, never by reading the log - one walk hands this command each batch identity (`gc`/`gd`
/// per file) exactly once and this command walks once. So an unchanged file's batch
/// is already wholly recorded and re-ingests nothing, while a file whose content AS THE WALK LOWERED
/// IT differs from its latest recorded batch re-emits every event the walk extracted for it. That
/// includes a file REVERTED to content it held at an earlier generation - its keys are byte-identical
/// to records the log still carries, and it re-emits precisely because those records are no longer
/// that file's latest generation. The qualifier is load-bearing and the two halves differ on it: the
/// design half reads the LIVE tree, while the code half lowers from the PERSISTED symbols index when
/// the project has one, so on such a project the decision is taken against what that index holds.
/// Both halves of that are claims about what this command APPENDS, over the files the walk emits a
/// batch for: a file the walk hands over NO batch for - one the walk no longer sees, or one whose
/// extraction the walk lowered to nothing - reaches no suppression decision here at all and retires
/// nothing, whereas a path the tree has DELETED that the persisted index still lists IS handed over
/// and does reach one. And a batch whose append lands but whose fold does not leaves the log right
/// and the graph behind (`append_and_fold_batch` folds best-effort by contract). What a re-emitted batch RETIRES is the FOLD's doing and reaches the code half only:
/// a code batch carries a `fresh` head whose 29a mechanism supersedes that file's prior structural
/// edges, while a design batch sets no `fresh` head, so re-emitting one adds edges without retiring
/// the ones its earlier generation left live. The light lane compiles no extraction pass, so
/// `graph build` there degrades to an empty graph (it still creates the store) and exits 0, never
/// an error.
fn cmd_graph_build(_args: &[String]) -> Res {
    // Bootstrap the store like `run`/`step` do (create-or-open under the cwd's `.rigger/`), NOT the
    // courier `require_store_dir` walk-up that refuses when none exists.
    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(None, None)?;
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;

    // The tree to fold: the git top-level, so a build launched from a subdirectory still ingests
    // the WHOLE project (the same root a run's `deps.repo` carries), falling back to the cwd
    // outside any git context.
    let root = {
        let top = git_repo();
        if top.is_empty() {
            ".".to_string()
        } else {
            top
        }
    };

    // A re-build refreshes incrementally (spec 45) without reading the log (spec 101): the walk
    // hands this command each batch identity (`gc`/`gd` per file) exactly once, so each batch asks
    // the store, through the group lookup, whether it is already its identity's latest recorded
    // generation ([`rigger::ingest::batch_is_latest_recorded`], the one first-sight helper the
    // run's keyed sink also calls). An unchanged file's batch is, and appends nothing; a changed,
    // reverted or never-recorded file's batch is not, and appends whole - a revert re-emits
    // because the records its keys match are no longer the file's latest generation.
    //
    // Each appended event is built by the one keyed derived-event builder
    // ([`rigger::ingest::keyed_derived_event`]), so it carries its replay key and its group, and
    // the batch is appended and folded in ONE store append and ONE graph transaction through the
    // shared batched append-and-fold authority (spec 49), exactly as the run's keyed sink does.
    // There is no run to stamp, so the events carry no run id.
    let mut appended = 0usize;
    rigger::ingest::ingest_project_batched(&root, |keyed| {
        match rigger::ingest::batch_is_latest_recorded(&store, conductor::STREAM, keyed) {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                eprintln!(
                    "graph build: skipping a batch whose recorded generation is unreadable: {e}"
                );
                return;
            }
        }
        let batch: Vec<Event> = keyed
            .iter()
            .map(|(key, ev)| rigger::ingest::keyed_derived_event((*ev).clone(), key))
            .collect();
        // Fold best-effort, exactly as the run's batched append-and-fold does: a fold failure must
        // not fail the ingest, which already landed durably in the log.
        match rigger::ingest::append_and_fold_batch(
            &store,
            Some(&graph as &dyn Projection),
            conductor::STREAM,
            &batch,
        ) {
            Ok(_) => appended += batch.len(),
            Err(e) => eprintln!("graph build: skipping a batch that failed to append: {e}"),
        }
    });

    println!(
        "graph build: ingested {appended} code-ingest event(s) into {}",
        db_path("graph.db")
    );
    Ok(())
}

/// `rigger graph communities [--resolution <r>]` - the OFFLINE, DETERMINISTIC community-detection
/// pass (spec 53, the CODE lens). It reads the project's already-folded coupling layer (the live
/// `CALLS` / `REFERENCES` / `CONTAINS` edges among code-entity / file nodes), runs modularity-based
/// detection over it at the given `--resolution` grain (default [`community::DEFAULT_RESOLUTION`]),
/// and RECORDS the result as `CommunityAssigned` events - so the derived grouping is event-sourced
/// (the `IN_COMMUNITY` membership edges are a rebuildable fold of the log), never computed at request
/// time. Re-running at a resolution with a NON-empty result supersedes only that grain's prior
/// assignments (the fold's `fresh` boundary), so distinct grains coexist and the lens reads one live
/// set per grain.
///
/// Store lifecycle mirrors `graph build` (the composition root, not the courier walk-up): it
/// CREATES the store under the cwd's `.rigger/` when absent, then reads the WHOLE projection and
/// appends-and-folds the pass's events in ONE batch. Detection is always-compiled and reads only
/// folded edges, so this runs identically in both feature lanes; a graph with no coupling edges
/// detects nothing and records nothing (exit 0, never an error). Because an empty result records NO
/// events, an empty re-run is KEEP-LAST-GOOD: it does NOT clear a grain's prior assignment - the last
/// NON-empty pass at that resolution stays live (see `community::events`, decision
/// d-u53c2-empty-rerun-keep-last-good). A real subsystem removal is a SHRINK - a smaller NON-empty
/// result - which DOES supersede via the `fresh` boundary and drops the emptied community's node.
fn cmd_graph_communities(args: &[String]) -> Res {
    run_graph_pass(
        "communities",
        args,
        community::DEFAULT_RESOLUTION,
        |whole, resolution| {
            // Detect communities over the live projection's coupling layer. The `fresh` head
            // supersedes this grain's prior memberships; the rest re-add, so a re-run REPLACES this
            // resolution's assignment set.
            let coupling = community::Coupling::from_graph(whole);
            let assignment = community::detect(&coupling, resolution);
            let events = community::events(&assignment);
            let summary = format!(
            "detected {} communit{} over {} coupled node(s) at resolution {} ({} membership event(s) recorded into {})",
            assignment.num_communities,
            if assignment.num_communities == 1 { "y" } else { "ies" },
            coupling.len(),
            resolution,
            events.len(),
            db_path("graph.db")
        );
            (events, summary)
        },
    )
}

/// One offline, deterministic `rigger graph <verb> [--resolution <r>]` pass: parse the
/// positive, finite resolution (defaulting to `default`), bootstrap the store like `graph build`/
/// `run` do (create-or-open under the cwd's `.rigger/`), and read the WHOLE live projection - the
/// same direct, project-scoped, sorted read the dash's `/api/graph` provider consults. `derive`
/// turns it into the pass's events plus its summary line; the events are appended in ONE store
/// append and folded in ONE transaction (the shared batched append-and-fold authority), then the
/// summary is printed.
fn run_graph_pass(
    verb: &str,
    args: &[String],
    default: f64,
    derive: impl FnOnce(&contextgraph::Graph, f64) -> (Vec<Event>, String),
) -> Res {
    let mut resolution = default;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--resolution" => {
                i += 1;
                let raw = args.get(i).cloned().unwrap_or_default();
                resolution = raw.parse::<f64>().map_err(|_| {
                    format!("graph {verb}: --resolution expects a number, got {raw:?}")
                })?;
                if !(resolution.is_finite() && resolution > 0.0) {
                    return Err(format!(
                        "graph {verb}: --resolution must be a positive finite number, got {resolution}"
                    )
                    .into());
                }
            }
            other => {
                return Err(format!("graph {verb}: unknown argument {other:?}").into());
            }
        }
        i += 1;
    }

    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(None, None)?;
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;

    let (events, summary) = derive(&graph.whole()?, resolution);
    rigger::ingest::append_and_fold_batch(
        &store,
        Some(&graph as &dyn Projection),
        conductor::STREAM,
        &events,
    )?;
    println!("graph {verb}: {summary}");
    Ok(())
}

/// `rigger graph concepts [--resolution <r>]` - the OFFLINE, DETERMINISTIC intent-derivation pass
/// (spec 54, the CONCEPTS lens). It reads the project's already-folded INTENT layer (the live
/// `SPECIFIES` / `CONSTRAINS` / `GOVERNS` / `explains` / `references` edges among design docs,
/// handbook rules, specs, rationale, and the code they attach to), runs the SAME deterministic
/// community detection the code lens ships over it at the given `--resolution` grain (default
/// [`concepts::DEFAULT_RESOLUTION`]), and RECORDS the result as `ConceptDerived` / `ConceptRealized`
/// events - so the derived grouping is event-sourced (the `REALIZES` membership edges are a
/// rebuildable fold of the log), never computed at request time. Re-running at a resolution with a
/// NON-empty result supersedes only that grain's prior grouping (the fold's `fresh` boundary), so
/// distinct grains coexist and the lens reads one live set per grain.
///
/// Store lifecycle mirrors `graph communities` (the composition root, not the courier walk-up): it
/// CREATES the store under the cwd's `.rigger/` when absent, then reads the WHOLE projection and
/// appends-and-folds the pass's events in ONE batch. The derivation is always-compiled and reads only
/// folded edges, so this runs identically in both feature lanes; a graph with no intent edges derives
/// nothing and records nothing (exit 0, never an error). Because an empty result records NO events, an
/// empty re-run is KEEP-LAST-GOOD: it does NOT clear a grain's prior grouping - the last NON-empty
/// pass at that resolution stays live (see `concepts::events`).
fn cmd_graph_concepts(args: &[String]) -> Res {
    run_graph_pass(
        "concepts",
        args,
        concepts::DEFAULT_RESOLUTION,
        |whole, resolution| {
            // Derive concepts over the live projection's intent layer. The `fresh` head supersedes
            // this grain's prior grouping; the rest re-add, so a re-run REPLACES this resolution's
            // concept set.
            let layer = concepts::intent_layer(whole);
            let derivation = concepts::derive(whole, &layer, resolution);
            let events = concepts::events(&derivation);
            let summary = format!(
            "derived {} concept{} over {} intent-linked node(s) at resolution {} ({} event(s) recorded into {})",
            derivation.num_concepts,
            if derivation.num_concepts == 1 { "" } else { "s" },
            layer.len(),
            resolution,
            events.len(),
            db_path("graph.db")
        );
            (events, summary)
        },
    )
}

/// `rigger ground "<query>" [<k>]` - run the project's configured grounder (the
/// same one the `run`/`serve` paths build from `defaults.grounder` via
/// [`select_grounder`]) over the repo and print up to `k` (default 8) relevant
/// entities, one per line as `file:line: <text> (degree N)`. This is the CLI surface a
/// native-workflow agent (which has Bash, not the MCP grounding tool) uses to ground.
///
/// The page (spec 92 criterion 3, RANKED BY INTENT) is `Grounder::ground_ranked` - ranked
/// exact-name-match first, then by the matched token's tree-wide commonness, then the
/// existing definition-over-reference tier, DEDUPLICATED so every call site of one function
/// occupies a single line carrying its degree. Before printing it, `Grounder::has_strong_match`
/// gates a WEAK query (`k > 0` and every match is tree-wide-common, or there is no match at
/// all) to the honest "no entity matches strongly" line instead of noise; `k == 0` is the
/// caller explicitly asking for nothing, so it stays silent rather than printing that line. A
/// non-structural grounder (grep / nop) has no commonness concept, so it is always "strong" and
/// prints its usual (undeduplicated, degree-0) rows - byte-for-byte the prior behavior.
pub(crate) fn cmd_ground(args: &[String]) -> Res {
    let query = args
        .first()
        .ok_or("ground: expected a query: rigger ground \"<query>\" [<k>]")?;
    let k: usize = match args.get(1) {
        Some(s) => s
            .parse()
            .map_err(|_| format!("ground: <k> must be a non-negative integer, got {s:?}"))?,
        None => 8,
    };
    if args.len() > 2 {
        return Err(format!(
            "ground: expected at most a query and k, got {} arguments",
            args.len()
        )
        .into());
    }
    // Honor the project's configured `defaults.grounder` when a config is present;
    // a project with no `.rigger/workflow.yml` yet falls back to the default grounder
    // (the empty name -> symbols, the scaffold default), so an agent can ground before
    // a workflow is authored rather than hitting a config error.
    let name = config_store::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    let grounder = select_grounder(&name)?;
    if k > 0 && !grounder.has_strong_match(query, k) {
        println!("no entity matches strongly for {query:?}");
        return Ok(());
    }
    for r in grounder.ground_ranked(query, k) {
        println!(
            "{}:{}: {} (degree {})",
            r.loc.file, r.loc.line, r.loc.text, r.degree
        );
    }
    Ok(())
}

/// `rigger reindex <file>...` - incrementally re-index the named files in the
/// project's persisted grounding index. It resolves the grounder from
/// `defaults.grounder` via [`select_reindex_grounder`] (rooted at `.`) - which, after
/// turbovec's retirement, resolves IDENTICALLY to [`select_grounder`]: the `symbols`
/// grounder's `open` only LOADS the persisted index (it does not freshen the whole
/// tree), so the named files are re-parsed exactly ONCE here rather than once by a
/// load-time freshen and again by the reindex. It then calls [`Grounder::reindex`] on
/// the changed files, so the `symbols` grounder drops each file's old symbols, re-parses
/// its current content, and persists the delta to `.rigger/symbols/` - a later `rigger
/// ground` (and the review tier the workflow runs after a unit lands) then reflects the
/// just-integrated code WITHOUT re-indexing the whole repo. For the grep / nop
/// grounders `reindex` is a no-op (they re-read the tree each call), so this command is
/// harmless there. Files are repo-relative, matching how the grounder records and
/// grounds them. At least one file is required.
pub(crate) fn cmd_reindex(args: &[String]) -> Res {
    if args.is_empty() {
        return Err("reindex: expected at least one file: rigger reindex <file>...".into());
    }
    // Same selection path as `cmd_ground`: honor `defaults.grounder` when a config
    // is present, else the unset default (symbols). The grounder is rooted at `.`,
    // so the persisted index it loads/updates is this project's `.rigger/symbols/`.
    let name = config_store::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    // Use the reindex-specific constructor: it loads the persisted index WITHOUT a
    // whole-tree freshen, so `reindex` re-parses ONLY the named files - never those
    // files twice (once by a load-time freshen, once by the reindex below).
    let grounder = select_reindex_grounder(&name)?;
    grounder.reindex(".", args);
    println!(
        "reindexed {} file(s) in the grounding index: {}",
        args.len(),
        args.join(", ")
    );
    Ok(())
}

/// `rigger symbols-index [<dir>]` - the criterion-3 fresh-process determinism harness for the
/// `symbols` structural index (spec 15, unit 3). It builds the whole-project symbol index over
/// `<dir>` (default `.`) via [`rigger::grounder::symbols::build_index`] and persists it with
/// [`rigger::grounder::symbols::store::save`], then prints the persisted path and file count.
///
/// It is DELIBERATELY independent of [`select_grounder`] / `defaults.grounder`: it drives unit
/// 3's own build+persist path directly, so a determinism test can re-index the SAME tree in two
/// SEPARATE `rigger` processes and diff the persisted `index.json` byte-for-byte - the
/// cross-process check the in-process lib test structurally cannot make, since one process
/// shares a single hash seed. Keeping this off the grounder-selection wiring also keeps the
/// spec-15 unit DAG acyclic (this harness needs only unit 3's code, never unit 4's selection).
///
/// Feature-gated on `symbols`: a build without it has no structural index, so the command
/// errors loudly rather than pretending to build one (the same no-silent-degrade rule the
/// grounder selection follows).
pub(crate) fn cmd_symbols_index(args: &[String]) -> Res {
    #[cfg(feature = "symbols")]
    {
        if args.len() > 1 {
            return Err(format!(
                "symbols-index: expected at most a directory, got {} arguments",
                args.len()
            )
            .into());
        }
        let dir = args.first().map(String::as_str).unwrap_or(".");
        let idx = rigger::grounder::symbols::build_index(dir, None);
        rigger::grounder::symbols::store::save(&idx, dir)?;
        println!(
            "symbols index: {} file(s) -> {}",
            idx.files().len(),
            rigger::grounder::symbols::store::index_path(dir).display()
        );
        Ok(())
    }
    #[cfg(not(feature = "symbols"))]
    {
        let _ = args;
        Err(
            "symbols-index requires the `symbols` feature; rebuild with the default features"
                .into(),
        )
    }
}

/// `rigger emit <type> '<json-object>'` - append an event `{type: <type>, data:
/// <parsed json>}` to the project's event store AND fold it into the context graph,
/// EXACTLY as the MCP `rigger_emit` tool does (both call [`mcpserver::emit_event`]).
/// The store and graph are opened the way `serve` opens them - the namespaced
/// per-project event store and the `graph.db` projector on the `conductor::STREAM`.
/// A bad / non-object JSON payload, or one whose shape the context-graph fold cannot apply
/// (checked by the shared core before the append), is a clear error to stderr with a non-zero
/// exit.
pub(crate) fn cmd_emit(args: &[String]) -> Res {
    // Optional `--spawn <id>`: stamp the emit with the EMITTING spawn's id
    // ([`META_SPAWN`](conductor::META_SPAWN)) at RECORD time. A native courier's `rigger emit`
    // is otherwise unattributable once the conductor replays it (the conductor never touched
    // it), so the verdict-channel-mismatch backstop (spec 18, unit 3) could not tell a GATING
    // adjudicator's OWN approve from a concurrent sibling's by position alone. The workflow
    // threads the worker's own spawn id here, exactly as the cli emit callback and the workflow
    // MCP server stamp their emits, so the recording the ReplayDriver later folds already
    // names its emitting spawn and is correlated by identity, never a shared-stream position.
    let mut spawn: Option<&str> = None;
    let mut positional: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--spawn" {
            spawn = Some(
                it.next()
                    .ok_or("emit: --spawn expects a spawn id: rigger emit --spawn <id> <type> '<json-object>'")?
                    .as_str(),
            );
        } else {
            positional.push(a);
        }
    }
    let typ = positional
        .first()
        .ok_or("emit: expected a type: rigger emit [--spawn <id>] <type> '<json-object>'")?;
    let json_arg = positional
        .get(1)
        .ok_or("emit: expected a JSON object: rigger emit [--spawn <id>] <type> '<json-object>'")?;
    if positional.len() > 2 {
        return Err(format!(
            "emit: expected a type and a single JSON object, got {} arguments",
            positional.len()
        )
        .into());
    }
    let data: serde_json::Value = serde_json::from_str(json_arg)
        .map_err(|e| format!("emit: <json-object> is not valid JSON: {e}"))?;
    if !data.is_object() {
        return Err(format!(
            "emit: <json-object> must be a JSON object, got {}",
            json_type_name(&data)
        )
        .into());
    }

    // Resolve the EXISTING store (walk up; refuse if none) rather than fabricating one
    // in the wrong cwd, and scope it by the RESOLVED root's identity (not the cwd's), so
    // a walked-up write lands in the stream the conductor reads - see [`require_store_dir`].
    let (loc, selection) = require_store_dir()?;
    // Spec 62 (couriers count as activity): re-stamp this project's registry heartbeat before
    // the real work below, so the instance stays discoverable even if this emit is the only
    // traffic in the run for a while. Best-effort/warn-only; never fails the emit.
    refresh_registry_entry(&loc, &selection);
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let graph = Projector::open(&loc.file("graph.db"), &loc.identity())?;

    // Same args shape the MCP tool receives, so emit_event - the shared core both
    // surfaces call - behaves identically here and over MCP. A non-empty `--spawn <id>`
    // rides in `meta.spawn`, the same key the MCP server's `stamp_current_spawn` writes.
    let mut tool_args = serde_json::json!({ "type": typ, "data": data });
    if let Some(spawn) = spawn.filter(|s| !s.is_empty()) {
        let mut meta = serde_json::Map::new();
        meta.insert(
            conductor::META_SPAWN.to_string(),
            serde_json::Value::String(spawn.to_string()),
        );
        tool_args
            .as_object_mut()
            .expect("json! built an object")
            .insert("meta".to_string(), serde_json::Value::Object(meta));
    }
    let pos = mcpserver::emit_event(&store, conductor::STREAM, Some(&graph), &tool_args)?;
    println!("emitted {typ} (position {pos}) and folded it into the context graph");
    Ok(())
}

/// The grounder for `rigger reindex`. After turbovec's retirement it resolves IDENTICALLY to
/// [`select_grounder`]: the only case that ever differed was turbovec (whose freshening `new`
/// had to be swapped for `new_for_reindex` to avoid a double-embed). The surviving grounders
/// have no such distinction - `Symbols::open` only LOADS the persisted index (it does not
/// freshen the whole tree), and grep / nop have no index at all - so there is one selection
/// authority, not two to keep in sync by hand.
fn select_reindex_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    select_grounder(name)
}

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::*;
    use crate::test_support::MinimalProjection;
    use rigger::eventstore::{
        Appended, Error, GroupHead, Revision, Subscription, TypeSelection,
    };

    /// A store whose group lookup cannot be read. Nothing else is reachable: a build that weighed
    /// a batch any other way, or appended one whose recorded generation it could not read, panics.
    struct UnreadableGroups;

    impl EventStore for UnreadableGroups {
        fn append(&self, _: &str, _: ExpectedRevision, _: &[Event]) -> Result<Appended, Error> {
            panic!("a batch whose recorded generation is unreadable is never appended")
        }
        fn read_stream(&self, _: &str, _: Revision, _: Direction) -> Result<Vec<Event>, Error> {
            unreachable!("a build never reads the stream")
        }
        fn read_all(&self, _: Position, _: Direction, _: &Filter) -> Result<Vec<Event>, Error> {
            unreachable!("a build never reads the log")
        }
        fn subscribe_all(&self, _: Position, _: &Filter) -> Result<Subscription, Error> {
            unreachable!("a build never subscribes")
        }
        fn subscribe_stream(&self, _: &str, _: Revision) -> Result<Subscription, Error> {
            unreachable!("a build never subscribes")
        }
        fn last_position(&self, _: &str, _: &str) -> Result<Option<Revision>, Error> {
            unreachable!("a build never looks up a boundary")
        }
        fn read_stream_typed(
            &self,
            _: &str,
            _: Revision,
            _: TypeSelection,
        ) -> Result<Vec<Event>, Error> {
            unreachable!("a build never reads by type")
        }
        fn latest_in_group(&self, _: &str, _: &str) -> Result<Option<GroupHead>, Error> {
            Err(Error::Backend("group index unreadable".into()))
        }
    }

    /// Spec 101: a `graph build` whose store cannot answer a batch's recorded generation FAILS with
    /// that error rather than skipping the batch and reporting success - an unanswered lookup is
    /// never read as "already recorded", the fail-unsafe direction.
    #[test]
    fn a_build_whose_recorded_generation_is_unreadable_fails_with_that_error() {
        let tree = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tree.path().join("src")).unwrap();
        std::fs::write(tree.path().join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n").unwrap();
        match ingest_tree(&UnreadableGroups, &MinimalProjection, tree.path().to_str().unwrap()) {
            Err(Error::Backend(msg)) => assert_eq!(msg, "group index unreadable"),
            other => panic!("the lookup's failure is the build's, got {other:?}"),
        }
    }
}
