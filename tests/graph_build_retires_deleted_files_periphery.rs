//! Periphery for the whole-project walk's retirement of deleted files: a file the tree no longer
//! holds stops answering in the graph after one `rigger graph build`, and a second build appends
//! nothing to the log.
//!
//! Each row plants the extraction tree (one file of each prefix the walk keys: `gc` code, `gd`
//! design, `gw` workflow definition) in a repository, builds, deletes one file, and builds again through the shipped binary. The
//! reader's view is `rigger graph --show`: every id the deleted file held answers before the
//! deletion and none answers after it, while the kept files still answer. A cold row deletes
//! `graph.db` after the repairing build, so `rigger setup` rebuilds the record half from the log
//! while the code store still answers for the tree it advanced to. No cold row deletes the workflow definition: `rigger setup` scaffolds a new
//! one where none exists, so the tree would hold the file again.

use crate::common;

use common::cli::{read_run_events, run_rigger};
use common::fixtures::{
    git_init_quiet, planted_extraction_tree, write_file, DOCUMENT_PATH, SOURCE_PATH, WORKFLOW_PATH,
};
use std::path::Path;

/// An id `rigger graph --show` answers and the kind it shows the id as.
type Shown = (&'static str, &'static str);

/// What a kept file still answers after any row's deletion, beside the file: the code half's
/// entity, the design doc and the workflow definition's stage, minus whatever the row deleted.
const KEPT: [(&str, Shown); 3] = [
    (SOURCE_PATH, ("src/lib.rs::product", "function")),
    (DOCUMENT_PATH, (DOCUMENT_PATH, "design-doc")),
    (WORKFLOW_PATH, ("stage:implement", "stage")),
];

/// What the code file answers while the tree holds it. Its path alone is no row of it: the kept
/// design doc cites the path, so the path still answers, as that citation's artifact.
const CODE_FILE: &[Shown] = &[
    (SOURCE_PATH, "file"),
    ("src/lib.rs::product", "function"),
    ("src/lib.rs::helper", "function"),
];

/// What the design doc answers while the tree holds it.
const DESIGN_DOC: &[Shown] = &[
    (DOCUMENT_PATH, "design-doc"),
    ("docs/architecture.md#the-walk", "design-doc"),
];

/// What the workflow definition answers while the tree holds it.
const WORKFLOW_DEFINITION: &[Shown] = &[("stage:implement", "stage"), ("gate:fmt", "gate")];

/// One row: the file deleted, whether the graph is rebuilt cold from the log after the repairing
/// build, and what the deleted file answered.
struct Deletion {
    deleted: &'static str,
    cold: bool,
    held: &'static [Shown],
}

/// Run `rigger <args>` at `root` and fail loudly on a non-zero exit.
fn rigger(root: &Path, args: &[&str]) -> String {
    let (out, err, ok) = run_rigger(root, args);
    assert!(
        ok,
        "rigger {} must succeed; stderr: {err}; stdout: {out}",
        args.join(" ")
    );
    out
}

/// Whether `rigger graph --show <id>` at `root` answers the id as `kind`, read from the kind its
/// header line shows.
fn answers(root: &Path, (id, kind): Shown) -> bool {
    let out = crate::common::cli::past_label(&rigger(root, &["graph", "--show", id]));
    let mut lines = out.lines();
    lines.next() == Some(&format!("show {id}"))
        && lines.next().is_some_and(|header| {
            header
                .split_whitespace()
                .skip_while(|word| *word != "kind")
                .nth(1)
                == Some(kind)
        })
}

/// GIVEN the extraction tree built into the graph, with the row's index state,
/// WHEN the row's file is deleted and `rigger graph build` runs (and, for a cold row, `rigger setup`
/// rebuilds the graph from the log over no `graph.db`),
/// THEN no id the file held answers `--show`, every kept id still does, and one more build
/// appends nothing to the log.
fn assert_a_build_retires(row: Deletion) {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    git_init_quiet(root);
    crate::common::cli::build_head(root);
    for &shown in row.held {
        assert!(
            answers(root, shown),
            "sanity: {shown:?} answers before {} is deleted",
            row.deleted
        );
    }

    std::fs::remove_file(root.join(row.deleted)).unwrap();
    crate::common::cli::build_head(root);
    if row.cold {
        for file in ["graph.db", "graph.db-wal", "graph.db-shm"] {
            let _ = std::fs::remove_file(root.join(".rigger").join(file));
        }
        rigger(root, &["setup"]);
    }

    let still: Vec<Shown> = row
        .held
        .iter()
        .copied()
        .filter(|&shown| answers(root, shown))
        .collect();
    assert!(
        still.is_empty(),
        "{} is deleted, yet --show still answers {still:?}",
        row.deleted
    );
    for (file, shown) in KEPT.iter().filter(|(file, _)| *file != row.deleted) {
        assert!(
            answers(root, *shown),
            "{file} is kept, so {shown:?} still answers"
        );
    }

    let recorded = read_run_events(root).len();
    rigger(root, &["graph", "build"]);
    assert_eq!(
        read_run_events(root).len(),
        recorded,
        "a repeat build over the same tree appends nothing to the log"
    );
}

rigger::test_cases! {
    /// A code file deleted.
    a_build_retires_a_deleted_code_file:
        assert_a_build_retires(Deletion {
            deleted: SOURCE_PATH,
            cold: false,
            held: CODE_FILE,
        });
    /// A design doc deleted.
    a_build_retires_a_deleted_design_doc:
        assert_a_build_retires(Deletion {
            deleted: DOCUMENT_PATH,
            cold: false,
            held: DESIGN_DOC,
        });
    /// The workflow definition deleted.
    a_build_retires_a_deleted_workflow_definition:
        assert_a_build_retires(Deletion {
            deleted: WORKFLOW_PATH,
            cold: false,
            held: WORKFLOW_DEFINITION,
        });
    /// A deleted code file's retirement, folded again by a cold rebuild from the log.
    a_cold_rebuild_keeps_a_deleted_code_file_retired:
        assert_a_build_retires(Deletion {
            deleted: SOURCE_PATH,
            cold: true,
            held: CODE_FILE,
        });
    /// A deleted design doc's retirement, folded again by a cold rebuild from the log.
    a_cold_rebuild_keeps_a_deleted_design_doc_retired:
        assert_a_build_retires(Deletion {
            deleted: DOCUMENT_PATH,
            cold: true,
            held: DESIGN_DOC,
        });
}
