//! Periphery tests of a change and a revert through the shipped binary, at the reader: once a
//! file moves to a new generation and back, `rigger graph --around` over the code store the builds
//! advanced shows the generation the tree is at, never the one it left, and equals what a cold
//! build over the current tree shows. A reverted file's content is byte-identical to content an
//! earlier build already described, so a store that skipped content it had seen before would
//! leave the reader on the superseded generation; the assertions end at the reader because that
//! failure is silent at every layer that reports only counts.

use std::collections::BTreeSet;

/// Generation A of the file that churns, and generation B. Each carries a definition AND a
/// reference to it, so the code half of the index has both an entity and a structural edge for a
/// later generation to supersede, and each file references only its OWN definition so one file's
/// generation can never move another file's edges.
const GEN_A: &str = "pub fn alpha_symbol() {}\npub fn churn_caller() { alpha_symbol(); }\n";
const GEN_B: &str = "pub fn beta_symbol() {}\npub fn churn_caller() { beta_symbol(); }\n";
const STABLE: &str = "pub fn stable_symbol() {}\npub fn stable_caller() { stable_symbol(); }\n";

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
use crate::common;
use common::git::run_git;

use common::cli::run_around;
use common::cli::run_rigger;

/// Commit the tree at `root` and drive `graph build` there, failing loudly on a non-zero exit or a
/// build that left the code store behind the head.
fn build(root: &std::path::Path, stage: &str) {
    common::fixtures::commit_tree(root);
    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok && out.starts_with("graph build: code store: "),
        "the {stage} graph build must advance the code store; stderr: {err}; stdout: {out}"
    );
}

/// Lay the fixture tree down under `root` with `churn` as the churning file's content: one file
/// that will move generation, one that never will, and a real design doc so the untouched-file
/// claim covers BOTH ingest prefixes rather than only the code half. It is its own git repo, so
/// the binary resolves a stable project identity and folds from the git top-level.
///
/// The live project and the cold reference build both come through here, so the two trees are
/// byte-identical by construction rather than by a comment claiming they are.
fn write_tree(root: &std::path::Path, churn: &str) {
    let _ = run_git(root, &["init", "-q"]);
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

/// What a READER gets from the shipped inspector around `seed`: the `node <id> <kind>` pairs and
/// the `edge <from> -REL-> <to>` lines, parsed exactly rather than substring-matched, so an id
/// that only appears inside an edge can never be mistaken for a reachable node.
///
/// Both halves are needed. A superseded generation's node ROW survives a supersede (only its edges
/// are invalidated), so "the old entity is gone" is a statement about REACHABILITY, and
/// reachability is what this traversal reports.
#[allow(clippy::type_complexity)]
fn inspector_view(
    root: &std::path::Path,
    seed: &str,
) -> (BTreeSet<(String, String)>, BTreeSet<String>) {
    let (out, err, ok) = run_around(root, seed, 2);
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
fn reached_entities(root: &std::path::Path, file: &str) -> BTreeSet<String> {
    inspector_view(root, file)
        .0
        .into_iter()
        .filter(|(_, kind)| kind == "code-entity")
        .map(|(id, _)| id)
        .collect()
}

/// INTEGRATION, at the READER: after a change and after a revert, the shipped inspector shows the
/// generation the tree is AT and no longer reaches the one it left.
///
/// The untouched file is checked too, because a store that re-derived the world would also satisfy
/// a claim made only about the file that moved.
#[test]
fn after_a_revert_the_shipped_inspector_shows_the_reverted_generation_and_not_the_superseded_one() {
    let dir = temp_churn_project();
    let root = dir.path();

    build(root, "first");
    let stable_view = inspector_view(root, "src/stable.rs");
    let reached = reached_entities(root, "src/churn.rs");
    assert!(
        reached.contains("src/churn.rs::alpha_symbol"),
        "sanity: the first build's graph must reach generation A's definition; reached {reached:?}"
    );

    // THE CHANGE: the reader moves to generation B and generation A becomes unreachable.
    std::fs::write(root.join("src/churn.rs"), GEN_B).unwrap();
    build(root, "generation-B");
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
    build(root, "revert");
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
fn cold_reference(churn: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_tree(dir.path(), churn);
    build(dir.path(), "cold reference");
    dir
}

/// INTEGRATION, the global constraint stated as it reads: whatever mix of skipping and re-ingest a
/// project went through, the live graph a reader gets equals what a COLD build over the tree AS IT
/// STANDS produces.
///
/// Checked at BOTH points of the sequence, not only at the end. After the revert the correct graph
/// and a graph stranded on the FIRST generation are the same graph, so an end-state comparison
/// alone is satisfied by a store that never advanced past the first build: it would be reading
/// generation A because it never left it. The check after the CHANGE is where those two stories
/// separate, and together they pin the constraint across the whole sequence rather than at a
/// coincidence.
///
/// Equality is taken over the reader's whole view (reachable nodes with their kinds, and the valid
/// edges among them), for the file that churned and for the file that did not. Position and
/// validity stamps are deliberately not compared: a store three builds advanced and a store
/// built once legitimately date the same fact differently, and the constraint is about the graph a
/// reader GETS, which is exactly what the inspector reports.
#[test]
fn after_a_change_and_after_a_revert_the_shipped_graph_equals_a_cold_build_of_the_current_tree() {
    let live_dir = temp_churn_project();
    let live = live_dir.path();
    build(live, "first");

    // After the CHANGE.
    std::fs::write(live.join("src/churn.rs"), GEN_B).unwrap();
    build(live, "generation-B");
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
    build(live, "revert");
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
