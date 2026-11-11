use super::*;

pub(crate) fn cmd_graph(args: &[String]) -> Res {
    // `rigger graph build` is a distinct verb (populate) from the default `--around` inspector
    // read: advance the project's code store to the head, no run required.
    if args.first().map(String::as_str) == Some("build") {
        return cmd_graph_build(&args[1..]);
    }
    // `rigger graph communities` and `rigger graph concepts` report the code store's groupings.
    if args.first().map(String::as_str) == Some("communities") {
        return cmd_graph_communities(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("concepts") {
        return cmd_graph_concepts(&args[1..]);
    }
    let mut around = String::new();
    let mut show = String::new();
    let mut lookup = Vec::new();
    let mut depth: i64 = 2;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--lookup" => {
                while args.get(i + 1).is_some_and(|a| !a.starts_with("--")) {
                    i += 1;
                    lookup.push(args[i].clone());
                }
            }
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
    if !lookup.is_empty() {
        return cmd_graph_lookup(&lookup);
    }
    if around.is_empty() {
        return Err(
            "graph: --lookup <term>..., --around <id> or --show <entity> is required".into(),
        );
    }
    let records = owning_graph_to_read()?;
    let read = ReadGraph::new(&records, caller_tree());
    let graph = read.open(Default::default())?;
    // The queried id (spec 117, UNRESOLVED REFERENCES ARE A READ): the query resolved once through
    // the interface, an alias to its canonical id, else the query as typed. Its one site read
    // answers the queried node's unresolved leaves.
    let queried = graph.resolve(&around)?.unwrap_or(around.clone());
    let g = contextgraph::around(graph.as_ref(), std::slice::from_ref(&queried), depth)?;
    let unresolved = match graph.locate(&queried)? {
        Located::One(site) => site.unresolved,
        _ => Vec::new(),
    };
    // The label heads the answer (6.1), counting what the reads above withheld.
    println!("{}", graph.stamp_label());
    println!("subgraph around {around:?} (depth {depth}):");
    print_around_subgraph(&g, &queried, &unresolved);
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
///
/// The queried node `around`'s line is followed by `  unresolved <count>`, the count of its
/// `unresolved` leaves (spec 117), when it lists any; no other node prints one.
///
/// An empty subgraph prints one note naming the seed form (spec 119, A BARE SHOW FINDS A LEAF OR
/// PATH): a seed is an exact id, `<file>::<path>`, or an alias [`cmd_graph`] resolves to one (spec
/// 117), never resolved from a bare name, and `rigger graph --show <name>` is the verb that
/// resolves one.
fn print_around_subgraph(g: &contextgraph::Graph, around: &str, unresolved: &[String]) {
    use std::collections::{BTreeMap, BTreeSet};

    let is_narrative =
        |kind: &str| kind == contextgraph::KIND_DECISION || kind == contextgraph::KIND_FINDING;

    let mut code_nodes: Vec<&contextgraph::Node> =
        g.nodes.iter().filter(|n| !is_narrative(&n.kind)).collect();
    code_nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let code_ids: BTreeSet<&str> = code_nodes.iter().map(|n| n.id.as_str()).collect();

    for n in &code_nodes {
        println!("  node {:<24} {}", n.id, n.kind);
        if n.id == around && !unresolved.is_empty() {
            println!("  unresolved {}", unresolved.len());
        }
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
                "  (+{rest} more decision(s)/finding(s) not shown - {})",
                conductor::RECORD_RECOVERY
            );
        }
    }

    if g.nodes.is_empty() {
        println!(
            "  (nothing found; a seed is an exact id, `<file>::<path>`, or an alias of one, and \
             `rigger graph --show <name>` resolves a bare name; has `rigger run` been run yet?)"
        );
    }
}

/// The upper bound on how many body lines `rigger graph --show` prints (spec 58): the definition's
/// extent comes from the grammar's own node boundary, but a very long body is clamped to this window
/// so the surface never dumps an unbounded body. A clamp is announced with an explicit note (the
/// omitted-line count), so a bounded body is never mistaken for the whole definition.
const SHOW_MAX_BODY_LINES: u32 = 60;

/// `rigger graph --show <entity>` (spec 58, criterion 1) - the TEXT half of graph lookup, beside
/// `--around`'s structural neighborhood. It resolves the entity through [`Projector::locate`] (a
/// full `<file>::<path>` id, a bare name matched as a definition's leaf, a path such as
/// `Struct.field` matched by its id suffix, or a pre-spec flat `<file>::<name>` matched as a leaf
/// in that file), then:
///
/// - ONE match: prints the definition SITE (`file:line`), the entity's KIND and one-hop DEGREE, its
///   remaining `site_fields` entries one line each ([`print_entity_site`]; a node naming no file
///   prints no site, no body and no note), and
///   the definition BODY read from the WORKING TREE at that location - line-numbered and bounded by
///   the extent the shared multi-grammar symbols authority derives (the grammar's own node
///   boundary), clamped to a max window. A missing file, a line past end-of-file, no definition of
///   the node's name and kind at its line, a nameless node, or a build without the extraction
///   grammar degrades to the site plus an explicit note, never an error (the recorded
///   graph facts are still shown; only the body is unavailable, and the surface says so).
/// - MANY matches (an ambiguous bare name, path or flat `<file>::<name>`): LISTS the sorted
///   candidates, each with its file, and prints NO body - the graph's honesty rule is never to
///   guess among candidates.
/// - NONE: a one-line not-found note (never an error), mirroring `--around`'s empty result.
///
/// Read-only over the projection and the working tree; deterministic for a given tree and graph.
/// The decision, lesson or finding `id` names, read from the run store's log the way `rigger
/// peers` reads it ([`Sidecar::site_of`]), for an id the graph no longer holds: a `rigger reset
/// --runs` prune drops a superseded run's records from the graph and deletes no event. A project
/// with no store yet holds no such record.
fn logged_site(id: &str) -> Result<Option<contextgraph::EntitySite>, Box<dyn std::error::Error>> {
    let Some((loc, selection)) = store_if_any()? else {
        return Ok(None);
    };
    over_the_run_store_at(&loc, &selection, |store| {
        Ok(Sidecar::read(store, conductor::STREAM)?.site_of(id))
    })
}

/// `rigger graph --lookup <term>...`: the combined lookup, the same reply `rigger_graph`'s
/// `lookup` serves, answered from the owning repository's graph and the caller's own tree.
fn cmd_graph_lookup(terms: &[String]) -> Res {
    use rigger::lookup::Lookup as _;
    let lookup = rigger::lookup::TreeLookup::new(caller_tree());
    let reply = match owning_graph_to_read() {
        Ok(records) => {
            let read = ReadGraph::new(&records, caller_tree());
            let graph = contextgraph::open_unowed(&records, &read, Default::default());
            lookup.lookup(terms, graph.as_deref().map_err(|e| e.0.clone()))?
        }
        Err(e) => lookup.lookup(
            terms,
            Err(format!("the context graph could not be opened: {e}")),
        )?,
    };
    print!("{reply}");
    Ok(())
}

fn cmd_graph_show(entity: &str) -> Res {
    let records = owning_graph_to_read()?;
    let read = ReadGraph::new(&records, caller_tree());
    let graph = read.open(Default::default())?;
    let located = graph.locate(entity)?;
    // The label heads the answer (6.1), counting what the locate withheld.
    println!("{}", graph.stamp_label());
    match located {
        Located::None => match logged_site(entity)? {
            Some(site) => {
                print_entity_site(&site);
                println!("  ({})", rigger::sidecar::LOGGED_SITE_NOTE);
            }
            None => println!(
                "show {entity:?}: no such entity in the graph (has it been built? try `rigger graph build`)"
            ),
        },
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

/// Print one located node for `rigger graph --show` (spec 58; spec 92 criterion 1, FRESH ON EVERY
/// INTEGRATION, for the header; spec 119, SHOW IS ONE RENDERER): `show <id>`, the site/kind/degree
/// header, one `  <entry> <value>` line per remaining [`contextgraph::site_fields`] entry, then the
/// line-numbered body bounded through the shared multi-grammar symbols authority - or an explicit
/// note (no definition of the node's name and kind at its line, a nameless node, or a build
/// without the extraction grammar) in place of the body, so the surface is never silently wrong
/// (a graceful degrade, never an error). A node naming no file has no line: its header reads
/// `  kind K degree D` with no `site:` part, and it prints no body and no note, [`definition_body`]
/// never called.
///
/// The body is bounded by the node's `name` attr and its kind (spec 119, ONE EXTENT), never a cut
/// of the id, since a file id holds a `.` and an impl header id a `::`, and by its id path, the
/// rest half of the id's one cut ([`contextgraph::query::split_id`]; spec 119, IDS FOLLOW THE
/// LANGUAGE'S PATH), so `T::new` bounds its own body, never `U::new`'s; an id with no `::` gives
/// none, bounded by name, kind and line alone. A node with no `name` at a non-zero line gets the
/// "no production definition named" note with its id in place of the name, here, ahead of the
/// feature-gated [`locate_definition_extent`], so both lanes print the same note; one at line 0
/// keeps [`definition_body`]'s `source unavailable`.
fn print_entity_site(site: &contextgraph::EntitySite) {
    println!("show {}", site.id);
    let Some(file) = site.file.as_deref() else {
        println!("  kind {}   degree {}", site.kind, site.degree);
        print_site_entries(site);
        return;
    };
    let name = site
        .attrs
        .get("name")
        .map(String::as_str)
        .filter(|name| !name.is_empty());
    let body = match name {
        None if site.line != 0 => {
            ShowBody::Note(Unbounded::Missing.note(&site.id, None, &site.kind, file, site.line))
        }
        // A named node, or a nameless one at line 0, which `definition_body` refuses before it
        // reads a name.
        name => definition_body(
            file,
            site.line,
            name.unwrap_or_default(),
            contextgraph::query::split_id(&site.id).1,
            &site.kind,
        ),
    };
    match body {
        ShowBody::Lines {
            lines,
            omitted,
            extent_end,
        } => {
            print_site_header(site, file);
            print_site_entries(site);
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
            print_site_header(site, file);
            print_site_entries(site);
            println!("  ({reason})");
        }
    }
}

/// Why `rigger graph --show` bounds no body for a located definition (spec 119, ONE EXTENT),
/// printed as one note in place of the body ([`Unbounded::note`]).
enum Unbounded {
    /// No production definition (one outside test code) of the label's name and kind, at its id
    /// path when it has one, sits at its line, or the node is nameless and no definition can be
    /// sought for it.
    Missing,
    /// Any other reason, already worded: a source the grammar cannot tag, or a build without the
    /// extraction grammar.
    Note(String),
}

impl Unbounded {
    /// The note for a definition labelled `label` - its name, or a nameless node's id - at the id
    /// path `path` when one was sought ([`sought`]), of kind `kind` recorded at `line` of `file`.
    fn note(self, label: &str, path: Option<&str>, kind: &str, file: &str, line: u32) -> String {
        match self {
            Unbounded::Missing => format!(
                "no production definition {} of kind {kind} at line {line} of {file}",
                sought(label, path)
            ),
            Unbounded::Note(why) => why,
        }
    }
}

/// How a show note names the definition it sought (spec 119, IDS FOLLOW THE LANGUAGE'S PATH):
/// `named "<label>"`, then ` at path "<path>"` when the id carried a path, the one
/// [`extract::locate_extent`](rigger::grounder::symbols::extract::locate_extent) filtered by.
fn sought(label: &str, path: Option<&str>) -> String {
    match path {
        Some(path) => format!("named {label:?} at path {path:?}"),
        None => format!("named {label:?}"),
    }
}

/// The site/kind/degree header line of a node naming `file` (spec 92 criterion 1), at the line
/// the node holds.
fn print_site_header(site: &contextgraph::EntitySite, file: &str) {
    println!(
        "  site: {}:{}   kind {}   degree {}",
        file, site.line, site.kind, site.degree
    );
}

/// One `  <entry> <value>` line per [`contextgraph::site_fields`] entry the header does not print
/// (spec 119, SHOW IS ONE RENDERER), in the list's order, a value's newlines collapsed to spaces
/// so each entry stays one line (the MCP key carrying it verbatim).
fn print_site_entries(site: &contextgraph::EntitySite) {
    for (entry, value) in contextgraph::site_fields(site) {
        // The header's entries; every entry after them is a string value.
        if matches!(entry, "file" | "line" | "kind" | "degree") {
            continue;
        }
        if let serde_json::Value::String(text) = value {
            println!("  {entry} {}", text.replace('\n', " "));
        }
    }
}

/// The outcome of bounding a located definition's body for `rigger graph --show` (spec 58; spec 92
/// criterion 1).
enum ShowBody {
    /// The line-numbered body window from the node's line. `omitted` is how many lines were
    /// dropped past the [`SHOW_MAX_BODY_LINES`] clamp (`0` when the whole extent fit), and
    /// `extent_end` is the extent's true last line, so the caller can print an honest clamp note
    /// when `omitted > 0`.
    Lines {
        lines: Vec<(u32, String)>,
        omitted: u32,
        extent_end: u32,
    },
    /// No body could be shown; the string is the human reason (no definition of the node's name
    /// and kind at its line, a nameless node, or a build compiled without the extraction grammar).
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
/// it). The window is `[start, extent]`, clamped by [`SHOW_MAX_BODY_LINES`]; a clamp reports its
/// omitted-line count so a bounded body is never read as whole.
///
/// Returns [`ShowBody::Note`] - the caller prints it in place of the body, never an error - when the
/// body cannot be shown honestly: the `start` line is `0` or past end-of-file, the file cannot be
/// read, the tree holds no production definition (one outside test code) of that name and kind
/// at `id_path`, when one is given, at `start`, or this build has no extraction grammar (the
/// light, `--no-default-features` lane). It never GUESSES a body from a structural
/// next-definition bound.
fn definition_body(
    file: &str,
    start: u32,
    name: &str,
    id_path: Option<&str>,
    kind: &str,
) -> ShowBody {
    // A line of 0 never named a real source line: degrade before any read. (Also covers a
    // reference-only graph entity with no definition site of its own.)
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
        // The line is past end-of-file.
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    }
    // The extent's end through the ONE multi-grammar authority. A miss (no such definition at
    // the line, or a light-lane build with no grammar) is an explicit note, never a guessed body.
    let extent_end = match locate_definition_extent(file, &text, start, name, id_path, kind) {
        Ok(end) => end.min(total),
        Err(why) => return ShowBody::Note(why.note(name, id_path, kind, file, start)),
    };
    // The max window: never dump an unbounded body. A clamp keeps the extent's true end so the
    // caller can announce the omitted lines.
    let window_cap = start.saturating_add(SHOW_MAX_BODY_LINES).saturating_sub(1);
    let printed_end = extent_end.max(start).min(window_cap);
    let omitted = extent_end.saturating_sub(printed_end);
    let lines = (start..=printed_end)
        .map(|n| (n, all[(n - 1) as usize].to_string()))
        .collect();
    ShowBody::Lines {
        lines,
        omitted,
        extent_end,
    }
}

/// The 1-based, inclusive last line of the definition named `name` of kind `kind` at `start`, at
/// the id path `id_path` when one is given (spec 58; spec 119, ONE EXTENT and IDS FOLLOW THE
/// LANGUAGE'S PATH). It resolves the file's grammar by its path through the symbols registry, so
/// a `.tsx` file keeps its TSX grammar, and only calls [`extract::locate_extent`], the ONE extent
/// authority, which reads production definitions only, those outside test code.
///
/// Returns the [`Unbounded`] reason - the caller degrades to a note - when no such production
/// definition sits at `start`, and a worded one when the grammar cannot tag the source. A file no
/// grammar is registered for holds no definition: the code view extracts by the same registry
/// lookup, so it holds no entity there, and the miss is no production definition.
#[cfg(feature = "symbols")]
fn locate_definition_extent(
    file: &str,
    source: &str,
    start: u32,
    name: &str,
    id_path: Option<&str>,
    kind: &str,
) -> Result<u32, Unbounded> {
    use rigger::grounder::symbols::extract::{self, Recorded};
    use rigger::grounder::symbols::registry;
    let entry = registry::for_path(file, None).ok_or(Unbounded::Missing)?;
    extract::locate_extent(
        source,
        entry.lang,
        &entry.language,
        entry.tags_query,
        Recorded {
            name,
            path: id_path,
            kind,
            line: start,
        },
    )
    .map_err(Unbounded::Note)?
    .ok_or(Unbounded::Missing)
}

/// Light-lane [`locate_definition_extent`]: a build WITHOUT the `symbols` feature links no grammar,
/// so no extent can be derived. It returns an explicit reason the caller
/// prints as a note - the show surface stays honest ("the body needs the extraction grammar this
/// build omits") rather than falling back to a hand-rolled lexer that would mis-read the very
/// grammars the graph ingests.
#[cfg(not(feature = "symbols"))]
fn locate_definition_extent(
    _file: &str,
    _source: &str,
    _start: u32,
    _name: &str,
    _id_path: Option<&str>,
    _kind: &str,
) -> Result<u32, Unbounded> {
    Err(Unbounded::Note(
        "the body extent needs the code-extraction grammar; this build was compiled without the `symbols` feature"
            .to_string(),
    ))
}

/// `rigger graph build` - advance the project's code store to the head, from a cold checkout or
/// a live one. It creates the store under the cwd's `.rigger/` when absent, as a run does, rather
/// than the courier walk-up that refuses a missing store, and records nothing in the log: the
/// code half is the snapshot the advance writes, never an event. The light lane compiles no
/// extractor and keeps no code store, so there `graph build` creates the store, says so and exits
/// 0.
fn cmd_graph_build(_args: &[String]) -> Res {
    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(None, None)?;
    resolve_store(&selection, &db_path("events.db"))?;
    open_graph(
        &db_path("graph.db"),
        &project_identity(),
        &selection,
        "graph build",
    )?;
    match open_project_code_store(rigger_store::code::Mode::Write)? {
        Some(opened) => {
            println!("graph build: {}", opened.line());
            opened.warn_if_behind();
        }
        None => println!("graph build: this build keeps no code store"),
    }
    Ok(())
}

/// `rigger graph communities [--resolution <r>]` - a read-only report of the code store's
/// communities: the coupling layer of the production view, detected at advance at the default
/// grain and computed here on demand at any other. It records nothing: groupings have one writer,
/// the advance that stores them in the snapshot.
fn cmd_graph_communities(args: &[String]) -> Res {
    report_groupings(
        "communities",
        args,
        |g| &g.communities,
        |n| format!("{n} communit{}", if n == 1 { "y" } else { "ies" }),
    )
}

/// `rigger graph concepts [--resolution <r>]` - a read-only report of the code store's concepts:
/// the tree's intent layer, derived at advance at the default grain and computed here on demand
/// at any other. It records nothing.
fn cmd_graph_concepts(args: &[String]) -> Res {
    report_groupings(
        "concepts",
        args,
        |g| &g.concepts,
        |n| format!("{n} concept{}", if n == 1 { "" } else { "s" }),
    )
}

/// One `rigger graph <verb> [--resolution <r>]` report: parse the positive, finite resolution
/// (defaulting to the stored grain), open the code store as a reader, and print the clusters
/// `pick` selects - a summary line, then each cluster's id, member count and label.
fn report_groupings(
    verb: &str,
    args: &[String],
    pick: impl Fn(&rigger_domain::code_view::Groupings) -> &Vec<rigger_domain::code_view::Cluster>,
    counted: impl Fn(usize) -> String,
) -> Res {
    let resolution = grain(verb, args)?;
    let lines = head_view(&cwd(), |view, _, _| {
        let computed;
        let groupings = if resolution == community::DEFAULT_RESOLUTION {
            view.groupings()
        } else {
            computed = rigger_domain::groupings::group_at(view, resolution);
            &computed
        };
        let clusters = pick(groupings);
        let members: usize = clusters.iter().map(|c| c.members.len()).sum();
        let mut lines = vec![format!(
            "graph {verb}: {} over {members} node(s) at resolution {resolution}",
            counted(clusters.len())
        )];
        lines.extend(
            clusters
                .iter()
                .map(|c| format!("{}\t{}\t{}", c.id, c.members.len(), c.label)),
        );
        Ok(lines)
    })?;
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

/// The `--resolution` grain `args` ask `rigger graph <verb>` for: a positive, finite number,
/// [`community::DEFAULT_RESOLUTION`] when none is given.
fn grain(verb: &str, args: &[String]) -> Result<f64, Box<dyn std::error::Error>> {
    let mut resolution = community::DEFAULT_RESOLUTION;
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
    Ok(resolution)
}

/// `rigger ground "<query>" [<k>]` - run the project's configured grounder (the
/// same one the `run` path builds from `defaults.grounder` via
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
    // The rows and their honesty label (6.1) come from one read of the code store.
    let (rows, label) = grounder.ground_ranked_labelled(query, k);
    match rows {
        None => println!("no entity matches strongly for {query:?}"),
        Some(rows) => {
            for r in rows {
                println!(
                    "{}:{}: {} (degree {})",
                    r.loc.file, r.loc.line, r.loc.text, r.degree
                );
            }
        }
    }
    // The label travels with every answer; stderr keeps the rows themselves clean.
    if let Some(label) = label {
        eprintln!("{label}");
    }
    Ok(())
}

/// `rigger emit <type> '<json-object>'` - append an event `{type: <type>, data:
/// <parsed json>}` to the project's event store AND fold it into the context graph,
/// EXACTLY as the MCP `rigger_emit` tool does (both call [`mcpserver::emit_event`]).
/// The store and graph are opened the way `rigger run` opens them - the namespaced
/// per-project event store and the `graph.db` projector on the `conductor::STREAM`.
/// A bad / non-object JSON payload, one whose shape the context-graph fold cannot apply, or a
/// record whose id another author's kept record holds (both checked by the shared core before
/// the append), is a clear error to stderr with a non-zero exit.
pub(crate) fn cmd_emit(args: &[String]) -> Res {
    // Optional `--spawn <id>`: stamp the emit with the EMITTING spawn's id
    // ([`META_SPAWN`](conductor::META_SPAWN)) at RECORD time. An agent's `rigger emit` is
    // otherwise unattributable once the conductor reads it back (the conductor never touched
    // it), so the verdict-channel-mismatch backstop (spec 18, unit 3) could not tell a GATING
    // adjudicator's OWN approve from a concurrent sibling's by position alone. A caller threads
    // the worker's own spawn id here, exactly as the cli emit callback and the spawn MCP server
    // stamp their emits, so the recording the conductor later reads already names its emitting
    // spawn and is correlated by identity, never a shared-stream position.
    // Without the flag, the spawn id the host exported to its worker
    // ([`SPAWN_ENV`](rigger::driver::SPAWN_ENV)) stamps the emit, so an agent session's emit is
    // attributed even when it never passes the flag; the operator's own shell exports none.
    let mut spawn: Option<String> = None;
    let mut positional: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--spawn" {
            spawn = Some(
                it.next()
                    .ok_or("emit: --spawn expects a spawn id: rigger emit --spawn <id> <type> '<json-object>'")?
                    .clone(),
            );
        } else {
            positional.push(a);
        }
    }
    let spawn = spawn.or_else(|| std::env::var(rigger::driver::SPAWN_ENV).ok());
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
    over_the_run_store_at(&loc, &selection, |store| {
        // Same args shape the MCP tool receives, so the append - the shared core both surfaces
        // call - behaves identically here and over MCP. A non-empty spawn id (the flag's, else the
        // exported one) rides in `meta.spawn`, the same key the spawn-bound MCP server
        // writes.
        let mut tool_args = serde_json::json!({ "type": typ, "data": data });
        if let Some(spawn) = spawn.filter(|s| !s.is_empty()) {
            let mut meta = serde_json::Map::new();
            meta.insert(
                conductor::META_SPAWN.to_string(),
                serde_json::Value::String(spawn),
            );
            tool_args
                .as_object_mut()
                .expect("json! built an object")
                .insert("meta".to_string(), serde_json::Value::Object(meta));
        }
        // The event goes on the log FIRST; `graph.db` is opened only after (spec 101), and a fold
        // that cannot happen is reported as not folded, with the reason, instead of claimed.
        let emitted = mcpserver::emit_event(
            store,
            conductor::STREAM,
            || loc.graph(&selection),
            &tool_args,
        )?;
        println!(
            "emitted {typ} (position {}){}",
            emitted.position,
            fold_clause(&emitted.fold)
        );
        Ok(())
    })
}
