//! Periphery (contract / API / integration) tests for spec 60 criterion 3: what a re-ingest
//! APPENDS when the tree moved, and what a reader then GETS from the graph, proved through the
//! SHIPPED BINARY over an on-disk store instead of in-process over `:memory:`.
//!
//! The inside-out proof of this criterion lives in `conductor::tests` and drives `conductor::run`
//! with in-crate `Deps`, a stub driver, two `:memory:` stores and a hand-composed content-identity
//! guard. That proof is the right one for the rule. Three things about the SHIPPED product are
//! structurally invisible to it, and this file is only about those three:
//!
//! 1. THE SECOND SINK. `rigger graph build` is a wholly separate ingest sink with its own seeding
//!    call site, and it is the one a person actually runs. It seeds through the same predicate the
//!    run seeds through, but "the same predicate" is a claim about two call sites, and only a test
//!    that drives the binary can hold both of them to one answer. A regression that reached only
//!    this sink would leave the run's own proof green.
//! 2. THE PROCESS AND THE DISK. A run's suppression is decided against a seed read from the log at
//!    start, which is why the in-crate proof spends a fresh `RunStarted` per campaign to get an
//!    honest re-read. A separate PROCESS re-reads by construction, from a real
//!    `.rigger/events.db`, and folds into a real `.rigger/graph.db` that outlives it. Nothing
//!    proved the sequence over persisted state.
//! 3. THE READER'S VIEW. The criterion's consequence is not a key set, it is what a traversal
//!    SEES. In-crate that is a `Projection` handle; the shipped read surface is
//!    `rigger graph --around`, and the stranding this criterion forbids is only a defect because
//!    it is what a reader gets. These tests assert on that surface.
//!
//! Scope is strictly criterion 3, and strictly the boundary. The predicate's own rule (criterion
//! 1), the run-scoping of non-ingest keys (criterion 2) and the store-layer append guard
//! (criterion 4) are owned by sibling units and by their own periphery files. In particular
//! `dedup_seeding_periphery.rs` already drives the CHANGE path through this binary at the LOG
//! level and deliberately scopes criterion 3 out; what it never drives is a REVERT, or the graph
//! any of it leaves behind. Those are what is here.
//!
//! Why a revert is the case worth a process of its own: the reverted content's keys are
//! byte-identical to keys the log has ALREADY recorded. A sink that asked "has this key ever been
//! recorded" answers yes and appends nothing, the fold never runs, and the graph stays on the
//! superseded generation forever. Re-folding the log replays the same suppression, so there is no
//! recovery short of deleting the store. That failure is silent at every layer that reports only
//! counts, which is why the assertions below end at the reader.

use std::collections::BTreeSet;

/// Generation A of the file that churns, and generation B. Each carries a definition AND a
/// reference to it, so the code half of the index has both an entity and a structural edge for a
/// later generation to supersede, and each file references only its OWN definition so one file's
/// generation can never move another file's edges.
const GEN_A: &str = "pub fn alpha_symbol() {}\npub fn churn_caller() { alpha_symbol(); }\n";
const GEN_B: &str = "pub fn beta_symbol() {}\npub fn churn_caller() { beta_symbol(); }\n";
const STABLE: &str = "pub fn stable_symbol() {}\npub fn stable_caller() { stable_symbol(); }\n";

/// Run `rigger <args...>` in `cwd` through the COMPILED binary, returning (stdout, stderr,
/// success). Each invocation gets its own throwaway state home so a short-lived integration run
/// never registers a phantom instance in the operator's machine-global registry, and never spawns
/// a real dashboard.
fn run_rigger(cwd: &std::path::Path, args: &[&str]) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME for the rigger invocation");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rigger"))
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// How many events `rigger graph build` reported ingesting, parsed from the line it prints. This
/// is the SHIPPED observable for "this build appended nothing", and it is the number that goes
/// wrong first when a revert is suppressed.
fn ingested_count(stdout: &str) -> usize {
    stdout
        .split_once("ingested ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("graph build must report its ingested count; got:\n{stdout}"))
}

/// Drive `graph build` in `root` and return what it reported ingesting, failing loudly rather than
/// letting a non-zero exit read as a zero-ingest.
fn build(root: &std::path::Path, stage: &str) -> usize {
    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok,
        "the {stage} graph build must succeed; stderr: {err}; stdout: {out}"
    );
    ingested_count(&out)
}

/// Lay the fixture tree down under `root` with `churn` as the churning file's content: one file
/// that will move generation, one that never will, and a real design doc so the untouched-file
/// claim covers BOTH ingest prefixes rather than only the code half. It is its own git repo, so
/// the binary resolves a stable project identity and folds from the git top-level.
///
/// The live project and the cold reference build both come through here, so the two trees are
/// byte-identical by construction rather than by a comment claiming they are.
fn write_tree(root: &std::path::Path, churn: &str) {
    let _ = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(root.join("src/churn.rs"), churn).unwrap();
    std::fs::write(root.join("src/stable.rs"), STABLE).unwrap();
    std::fs::write(
        root.join("specs/sample.md"),
        include_str!("../specs/29c-unified-traversal-tiers.md"),
    )
    .unwrap();
}

/// A throwaway project holding generation A of the churning file.
fn temp_churn_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_tree(dir.path(), GEN_A);
    dir
}

/// The project identity the binary resolves for `root`, mirrored here so this test reads back the
/// exact namespaced run stream the compiled binary wrote: the tracked `.rigger/project.id` at the
/// git top-level when present, else the git top-level basename, else `root`'s own basename.
fn run_stream_identity(root: &std::path::Path) -> String {
    let toplevel = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel
        .as_deref()
        .map(std::path::Path::new)
        .unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// The whole namespaced run stream the binary wrote under `root`, read back the way the binary
/// reads it: through the crate's PUBLIC store surface, from outside the crate.
fn read_run_stream(root: &std::path::Path) -> Vec<rigger::eventstore::Event> {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let backend = Store::open(
        root.join(".rigger")
            .join("events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap()
}

/// The LIVE suppression set the shipped predicate returns over the log the binary wrote: each
/// file's latest recorded generation only, which is exactly what the next build will skip.
fn live_suppression_set(root: &std::path::Path) -> BTreeSet<String> {
    rigger::ingest::project_scoped_replay_keys(&read_run_stream(root))
        .into_iter()
        .collect()
}

/// The derived-index replay keys the recorded stream carries FOR ONE FILE, in append order.
///
/// The marker is `/<file>@`, the writer's own trailing separator, so a key is attributed to the
/// file whose whole `<prefix>/<file>` span it names and never to a file that merely shares a
/// prefix with it. Order is preserved because "re-emits its whole batch" is a claim about the
/// walk's own sequence, not about a set.
#[cfg(feature = "symbols")]
fn keys_for(events: &[rigger::eventstore::Event], file: &str) -> Vec<String> {
    let marker = format!("/{file}@");
    events
        .iter()
        .filter(|e| rigger::ingest::is_derived_index_type(&e.type_))
        .filter_map(|e| e.meta.get(rigger::conductor::META_REPLAY_KEY).cloned())
        .filter(|k| k.contains(&marker))
        .collect()
}

/// Every content key the SHIPPED walk mints for the tree at `root`, as a set. This is the WRITER
/// half: the same public entry both sinks drive, so these are the keys the log will carry and
/// never a spelling this test invented.
#[cfg(feature = "symbols")]
fn minted_keys(root: &std::path::Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |batch| {
        for (key, _) in batch {
            out.insert(key.clone());
        }
    });
    out
}

/// What a READER gets from the shipped inspector around `seed`: the `node <id> <kind>` pairs and
/// the `edge <from> -REL-> <to>` lines, parsed exactly rather than substring-matched, so an id
/// that only appears inside an edge can never be mistaken for a reachable node.
///
/// Both halves are needed. A superseded generation's node ROW survives a supersede (only its edges
/// are invalidated), so "the old entity is gone" is a statement about REACHABILITY, and
/// reachability is what this traversal reports.
#[cfg(feature = "symbols")]
#[allow(clippy::type_complexity)]
fn inspector_view(
    root: &std::path::Path,
    seed: &str,
) -> (BTreeSet<(String, String)>, BTreeSet<String>) {
    let (out, err, ok) = run_rigger(root, &["graph", "--around", seed, "--depth", "2"]);
    assert!(
        ok,
        "graph --around {seed} must succeed; stderr: {err}; stdout: {out}"
    );
    let mut nodes: BTreeSet<(String, String)> = BTreeSet::new();
    let mut edges: BTreeSet<String> = BTreeSet::new();
    for line in out.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("node ") {
            let mut parts = rest.split_whitespace();
            let id = parts.next().unwrap_or_default().to_string();
            let kind = parts.next().unwrap_or_default().to_string();
            if !id.is_empty() {
                nodes.insert((id, kind));
            }
        } else if let Some(rest) = line.strip_prefix("edge ") {
            edges.insert(rest.to_string());
        }
    }
    // A comparison over an EMPTY neighborhood proves nothing, and an empty answer here means the
    // fixture broke (a seed the fold never built a node for, a build that silently ingested
    // nothing) rather than that the contract holds.
    assert!(
        !nodes.is_empty() && !edges.is_empty(),
        "the inspector must reach a real neighborhood around {seed}; it reported {} node(s) and \
         {} edge(s) from:\n{out}",
        nodes.len(),
        edges.len()
    );
    (nodes, edges)
}

/// The code entities a traversal from `file` REACHES, which is the observable form of "this file's
/// structural edges": an entity whose containing edge was superseded is unreachable from its file
/// however long its node row survives.
#[cfg(feature = "symbols")]
fn reached_entities(root: &std::path::Path, file: &str) -> BTreeSet<String> {
    inspector_view(root, file)
        .0
        .into_iter()
        .filter(|(_, kind)| kind == "code-entity")
        .map(|(id, _)| id)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The tests below need the extraction pass to have a tree to walk, so they are `symbols`-gated
// exactly as the walk is. The light lane's own boundary claim is at the bottom of this file.
// ---------------------------------------------------------------------------------------------

/// INTEGRATION, through the SHIPPED BINARY and an on-disk store: a file driven BACK to content it
/// held at an earlier RECORDED generation re-ingests, appending that generation's whole batch
/// under its own already-recorded keys, while every untouched file still appends nothing.
///
/// This is the discrimination the in-crate proof makes against a hand-composed stack and this one
/// makes against the stack that ships. Every key the revert appends was ALREADY in the log before
/// the build ran (asserted, not assumed), so a sink that suppressed on "ever recorded" would
/// report zero here and the assertions on count, on keys and on the second occurrence would each
/// fail on their own.
///
/// The expected key set is never hand-listed: it is what the shipped walk mints for the tree as it
/// stands, so the assertion cannot drift from what the extraction pass actually extracts.
#[cfg(feature = "symbols")]
#[test]
fn a_revert_through_the_shipped_build_re_emits_the_earlier_generations_recorded_keys() {
    let dir = temp_churn_project();
    let root = dir.path();

    // Generation A, recorded by the shipped build.
    assert!(
        build(root, "first") > 0,
        "sanity: the first build over a fresh tree must ingest the derived index"
    );
    let after_one = read_run_stream(root);
    let gen_a_keys = keys_for(&after_one, "src/churn.rs");
    let stable_gen_a = keys_for(&after_one, "src/stable.rs");
    let doc_gen_a = keys_for(&after_one, "specs/sample.md");
    assert!(
        !gen_a_keys.is_empty() && !stable_gen_a.is_empty() && !doc_gen_a.is_empty(),
        "sanity: the first build must record a batch for both code files and the design doc, else \
         there is nothing for a later build to leave alone"
    );

    // Generation B: the churning file moves forward in a SECOND process, and generation A stops
    // being its latest recorded generation.
    std::fs::write(root.join("src/churn.rs"), GEN_B).unwrap();
    assert!(
        build(root, "generation-B") > 0,
        "sanity: a changed file's new generation must reach the log"
    );
    let after_two = read_run_stream(root);
    let churn_after_two = keys_for(&after_two, "src/churn.rs");
    let gen_b_keys = &churn_after_two[gen_a_keys.len()..];
    assert!(
        !gen_b_keys.is_empty() && gen_b_keys != gen_a_keys,
        "sanity: the edit must record a DIFFERENT generation of the file; it recorded \
         {gen_b_keys:?} against generation A's {gen_a_keys:?}"
    );
    let retired: BTreeSet<String> = gen_a_keys.iter().cloned().collect();
    assert!(
        live_suppression_set(root).is_disjoint(&retired),
        "sanity: generation A must have been RETIRED from the live suppression set by generation \
         B, because that retirement is the only reason the revert below is allowed to append"
    );

    // THE REVERT, in a THIRD process: byte-identical to generation A, every one of whose keys the
    // log still carries.
    std::fs::write(root.join("src/churn.rs"), GEN_A).unwrap();
    assert_eq!(
        minted_keys(root).intersection(&retired).count(),
        gen_a_keys.len(),
        "sanity: the reverted tree must mint generation A's keys again, else this is not a revert"
    );
    assert!(
        build(root, "revert") > 0,
        "a file reverted to an earlier recorded generation must re-ingest; the shipped build \
         reported ingesting nothing, so the graph is stranded on the superseded generation and no \
         later build will ever recover it"
    );

    let after_three = read_run_stream(root);
    let churn_after_three = keys_for(&after_three, "src/churn.rs");
    let re_emitted = &churn_after_three[churn_after_two.len()..];
    assert_eq!(
        re_emitted, gen_a_keys,
        "the revert must re-emit generation A's WHOLE batch under its own keys, in the walk's own \
         order; it emitted {re_emitted:?} against generation A's {gen_a_keys:?}"
    );

    // The keys really were already recorded before this build ran, and the log now carries each of
    // them TWICE. Without both of these the test could pass over a fixture that never posed the
    // revert question at all.
    for key in &gen_a_keys {
        assert!(
            after_two
                .iter()
                .any(|e| e.meta.get(rigger::conductor::META_REPLAY_KEY) == Some(key)),
            "the fixture is vacuous unless {key} was already recorded before the revert"
        );
        assert_eq!(
            after_three
                .iter()
                .filter(|e| e.meta.get(rigger::conductor::META_REPLAY_KEY) == Some(key))
                .count(),
            2,
            "the log must carry {key} once from generation A and once from the revert"
        );
    }

    // The untouched files appended NOTHING across the change or the revert, in EITHER ingest half.
    assert_eq!(
        keys_for(&after_three, "src/stable.rs"),
        stable_gen_a,
        "an untouched code file must append nothing on the builds that re-ingest another file"
    );
    assert_eq!(
        keys_for(&after_three, "specs/sample.md"),
        doc_gen_a,
        "an untouched design doc must append nothing on the builds that re-ingest another file"
    );

    // And the sequence SETTLES: the live suppression set is exactly the tree's current generation,
    // so a further build appends nothing at all.
    assert_eq!(
        live_suppression_set(root),
        minted_keys(root),
        "after a change and a revert the live suppression set must be exactly the tree's current \
         generation - no retired generation left live, no untouched file's keys dropped"
    );
    assert_eq!(
        build(root, "settling"),
        0,
        "once the reverted generation is recorded again, a further build must append NOTHING"
    );
}

/// INTEGRATION, at the READER: after a change and after a revert, the shipped inspector shows the
/// generation the tree is AT and no longer reaches the one it left.
///
/// This is the criterion's consequence stated where it is felt. The log-level test above proves
/// the right events were appended; it cannot prove they were FOLDED, and an append that never
/// reaches the projection leaves exactly the same stranded reader the suppression would have. The
/// untouched file is checked too, because a fold that rebuilt the world would also satisfy a claim
/// made only about the file that moved.
#[cfg(feature = "symbols")]
#[test]
fn after_a_revert_the_shipped_inspector_shows_the_reverted_generation_and_not_the_superseded_one() {
    let dir = temp_churn_project();
    let root = dir.path();

    assert!(build(root, "first") > 0, "sanity: the first build ingests");
    let stable_view = inspector_view(root, "src/stable.rs");
    let reached = reached_entities(root, "src/churn.rs");
    assert!(
        reached.contains("src/churn.rs::alpha_symbol"),
        "sanity: the first build's graph must reach generation A's definition; reached {reached:?}"
    );

    // THE CHANGE: the reader moves to generation B and generation A becomes unreachable.
    std::fs::write(root.join("src/churn.rs"), GEN_B).unwrap();
    assert!(
        build(root, "generation-B") > 0,
        "sanity: the change ingests"
    );
    let reached = reached_entities(root, "src/churn.rs");
    assert!(
        reached.contains("src/churn.rs::beta_symbol"),
        "after the change the new definition must be reachable from its file; reached {reached:?}"
    );
    assert!(
        !reached.contains("src/churn.rs::alpha_symbol"),
        "after the change the file's PRIOR structural edges must be superseded, so the old \
         definition is no longer reachable from it; reached {reached:?}"
    );

    // THE REVERT: the reader comes BACK to generation A, and generation B does not linger beside
    // it. A revert that appended but did not supersede would leave both live at once, which is a
    // graph no tree ever had.
    std::fs::write(root.join("src/churn.rs"), GEN_A).unwrap();
    assert!(build(root, "revert") > 0, "sanity: the revert ingests");
    let reached = reached_entities(root, "src/churn.rs");
    assert!(
        reached.contains("src/churn.rs::alpha_symbol"),
        "after the revert the reverted generation's definition must be reachable again; reached \
         {reached:?}"
    );
    assert!(
        !reached.contains("src/churn.rs::beta_symbol"),
        "after the revert the superseded generation's structural edges must be gone, not left \
         live beside the reverted one; reached {reached:?}"
    );

    // The file that never moved reads identically to how it read before any of it.
    assert_eq!(
        inspector_view(root, "src/stable.rs"),
        stable_view,
        "an untouched file's neighborhood must be identical after another file changed and was \
         reverted"
    );
}

/// A project built ONCE, from scratch, over the tree `churn` describes: the graph a person would
/// get by deleting `.rigger` and starting over, and the reference the global constraint names. It
/// comes through the same fixture writer as the live project, so the two trees are byte-identical
/// by construction rather than by a comment claiming they are.
#[cfg(feature = "symbols")]
fn cold_reference(churn: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_tree(dir.path(), churn);
    assert!(
        build(dir.path(), "cold reference") > 0,
        "sanity: the cold reference build must ingest the tree"
    );
    dir
}

/// INTEGRATION, the global constraint stated as it reads: whatever mix of skipping and re-ingest a
/// project went through, the live graph a reader gets equals what a COLD build over the tree AS IT
/// STANDS produces.
///
/// Checked at BOTH points of the sequence, not only at the end. After the revert the correct graph
/// and a graph stranded on the FIRST generation are the same graph, so an end-state comparison
/// alone is satisfied by a re-ingest that appended and never folded: it would be reading generation
/// A because it never left it. The check after the CHANGE is where those two stories separate, and
/// together they pin the constraint across the whole sequence rather than at a coincidence.
///
/// Equality is taken over the reader's whole view (reachable nodes with their kinds, and the valid
/// edges among them), for the file that churned and for the file that did not. Position and
/// validity stamps are deliberately not compared: a store three builds appended to and a store
/// built once legitimately date the same fact differently, and the constraint is about the graph a
/// reader GETS, which is exactly what the inspector reports.
#[cfg(feature = "symbols")]
#[test]
fn after_a_change_and_after_a_revert_the_shipped_graph_equals_a_cold_build_of_the_current_tree() {
    let live_dir = temp_churn_project();
    let live = live_dir.path();
    assert!(build(live, "first") > 0, "sanity: the first build ingests");

    // After the CHANGE.
    std::fs::write(live.join("src/churn.rs"), GEN_B).unwrap();
    assert!(
        build(live, "generation-B") > 0,
        "sanity: the change ingests"
    );
    let cold_b = cold_reference(GEN_B);
    for file in ["src/churn.rs", "src/stable.rs"] {
        assert_eq!(
            inspector_view(live, file),
            inspector_view(cold_b.path(), file),
            "after the change path, the live graph around {file} must equal what a cold build over \
             the current tree produces"
        );
    }

    // After the REVERT.
    std::fs::write(live.join("src/churn.rs"), GEN_A).unwrap();
    assert!(build(live, "revert") > 0, "sanity: the revert ingests");
    let cold_a = cold_reference(GEN_A);
    for file in ["src/churn.rs", "src/stable.rs"] {
        assert_eq!(
            inspector_view(live, file),
            inspector_view(cold_a.path(), file),
            "after the revert, the live graph around {file} must equal what a cold build over the \
             current tree produces"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The light lane.
// ---------------------------------------------------------------------------------------------

/// The LIGHT LANE's own boundary claim, so this file is not silently absent from that lane.
///
/// Without the extraction pass there is no walk, so `graph build` mints no content key and appends
/// no derived-index event. That makes criterion 3 vacuous here - but vacuous BY CONSTRUCTION is a
/// claim, not an assumption. If this lane ever began appending keyed derived events, the change
/// and revert rules would start to govern it while nothing proved they held, and the first symptom
/// would be a reader stranded on a superseded generation with no test in either lane watching.
///
/// So this pins the premise through the same shipped path the gated tests drive: the build
/// succeeds (it degrades to an empty graph, it does not error), it reports ingesting nothing over
/// a fresh tree, a change appends nothing, a revert appends nothing, and the shared predicate over
/// the log all three left returns an EMPTY suppression set - there is no recorded generation for
/// any later build to be suppressed against.
#[cfg(not(feature = "symbols"))]
#[test]
fn the_light_lane_mints_no_content_key_so_a_change_and_a_revert_have_nothing_to_strand() {
    let dir = temp_churn_project();
    let root = dir.path();

    assert_eq!(
        build(root, "first"),
        0,
        "with no extraction pass compiled in there is nothing to walk, so a cold build over a \
         fresh tree must append no derived-index event"
    );

    std::fs::write(root.join("src/churn.rs"), GEN_B).unwrap();
    assert_eq!(
        build(root, "generation-B"),
        0,
        "a changed file cannot re-emit a batch in a lane that mints no key"
    );

    std::fs::write(root.join("src/churn.rs"), GEN_A).unwrap();
    assert_eq!(
        build(root, "revert"),
        0,
        "a reverted file cannot re-emit a batch in a lane that mints no key"
    );

    assert!(
        live_suppression_set(root).is_empty(),
        "the shared predicate must return no suppression key at all over a log this lane wrote; \
         it returned {:?}",
        live_suppression_set(root)
    );
}
