//! Acceptance test for `rigger graph --show <entity>` FRESHNESS (spec 58, criterion 2: the degrade
//! path). It drives the COMPILED `rigger` binary against a throwaway project whose code store
//! describes a committed tree and whose working tree no longer holds the recorded file: a location
//! the tree no longer holds is never served as current. The answer describes the worktree, so the
//! deleted file's entities are gone from it and the honesty label heading the answer counts the
//! file the answer took from the worktree; the command EXITS SUCCESS (never an error) and prints
//! no body. The sibling file the working tree still matches keeps answering.

// The extractor that reads the committed source is linked by `symbols` alone.
#![cfg(feature = "symbols")]

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
use crate::common;

use common::cli::body_line_count;
use common::cli::run_rigger;
use common::cli::seed_rigger_dir;
use common::cli::temp_project;
use common::fixtures::commit_tree;

/// DRIFT SHAPE (a): the recorded FILE is missing from the working tree. The show surface describes
/// the worktree (the label counts the deleted file among the files it took from the worktree),
/// answers that the entity is not in the graph, prints no body, and EXITS SUCCESS - never an
/// error. A sibling file the tree still matches is shown.
#[test]
fn graph_show_answers_an_entity_whose_recorded_file_is_missing_as_gone() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // `ghost` is defined at gone.rs:7 in the committed tree, then gone.rs is deleted from the
    // working tree (the file was deleted since the code store was advanced). The sibling
    // `present.rs` is unchanged, so the store is genuinely populated.
    std::fs::write(root.join("present.rs"), "fn present() {}\n").unwrap();
    std::fs::write(
        root.join("gone.rs"),
        "// a\n// b\n// c\n// d\n// e\n// f\nfn ghost() {}\n",
    )
    .unwrap();
    commit_tree(root);
    std::fs::remove_file(root.join("gone.rs")).unwrap();

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "ghost"]);

    // NEVER AN ERROR: a missing working-tree file exits SUCCESS.
    assert!(
        ok,
        "a missing recorded file degrades gracefully (exit SUCCESS, never an error); stderr: {err}"
    );
    // THE LABEL SAYS THE ANSWER DESCRIBES THE WORKTREE, COUNTING THE DELETED FILE.
    let label = out.lines().next().unwrap_or_default();
    assert!(
        label.starts_with("describes ")
            && label.contains(" + 1 file); groupings describe ")
            && !label.contains("withheld"),
        "the honesty label counts the deleted file the answer took from the worktree; got:\n{out}"
    );
    // NEVER THE STALE LOCATION: the entity is not served as current, at gone.rs:7 or anywhere.
    assert!(
        out.contains("no such entity") && !out.contains("gone.rs:7"),
        "the deleted file's entity is gone, never served at its old location; got:\n{out}"
    );
    // NEVER WRONG TEXT: no line-numbered body is presented as the entity's current source.
    assert_eq!(
        body_line_count(&out),
        0,
        "no line-numbered body is printed for a missing file; got:\n{out}"
    );
    // THE UNCHANGED SIBLING STILL ANSWERS at its own site.
    let (present, err, ok) = run_rigger(root, &["graph", "--show", "present"]);
    assert!(ok, "the unchanged file's entity shows; stderr: {err}");
    assert!(
        present.contains("present.rs:1"),
        "the unchanged file's entity is served at its site; got:\n{present}"
    );
}
