//! Periphery for spec 124, criterion 2 - THE BLOB IS WRITTEN.
//!
//! The one hash function writes the object it hashes when the root it hashes under is inside a
//! repository, and hashes without writing outside one. The test beside the function pins that for
//! the bytes it is handed. What this file holds is the rule as an operator meets it through the
//! built binary's `rigger graph build`, which records each file's ledger entry under the blob it
//! hashed:
//!
//!   1. **Inside a repository.** The object database, which held no object before the build,
//!      holds exactly the blob of each entry the build recorded, each the bytes its file holds.
//!   2. **Outside a repository.** The build records the same entries under the ids git gives the
//!      files' bytes, and leaves the tree as it found it at its top level, with no repository
//!      standing under it.
//!   3. **Whatever repository the environment names.** Run with `GIT_DIR` naming a second
//!      repository, as a git hook's environment names one, the build still writes each blob into
//!      the root's object database and none into the second's.
//!
//! The migration's own write, `rigger reset --derived` writing the blob of each entry it migrates,
//! is held beside the migration's other properties in `tests/reset_derived_compaction_periphery.rs`.

#![cfg(feature = "symbols")]

mod common;

use common::cli::{batch_events, graph_build_line, recorded_and_walked, run_rigger_envs};
use common::fixtures::{
    assert_outside_any_repository, dir_snapshot, git_dir, git_init_quiet, planted_extraction_tree,
    write_file, EntryRecord, WALKED,
};
use common::git::{assert_holds_no_object, held_blob, object_ids};
use std::collections::BTreeSet;
use std::path::Path;

/// Run `rigger graph build` at `root`, where the extraction tree is planted, with the extra
/// environment `envs`, and assert it
/// succeeds recording exactly what the walk extracts - one ledger entry per batch, each under the
/// id `git hash-object` gives its file's bytes - its line counting the entries' events, the walk
/// extracting each batch the tree holds ([`WALKED`]), so what a caller asserts over the entries
/// never holds of a walk that extracts nothing: the entries it recorded.
fn build_records_the_walk(root: &Path, envs: &[(&str, &str)]) -> Vec<EntryRecord> {
    let (out, err, ok) = run_rigger_envs(root, &["graph", "build"], envs);
    let [recorded, walked] = recorded_and_walked(root);
    assert_eq!(
        walked.len(),
        WALKED.len(),
        "the walk extracts each batch the extraction tree holds"
    );
    assert_eq!(
        (ok, out, recorded.clone()),
        (true, graph_build_line(batch_events(&walked), ""), walked),
        "the build records one entry per batch the walk extracts, under the id git gives its \
         file's bytes; stderr: {err}"
    );
    recorded
}

/// GIVEN the extraction tree planted in a repository whose object database holds no object,
/// nothing committed,
/// WHEN the operator runs `rigger graph build`,
/// THEN the build records the walk's entries, and the object database holds exactly the blob of
/// each, each the bytes its file holds.
#[test]
fn a_graph_build_inside_a_repository_writes_the_blob_of_each_entry_it_records() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    git_init_quiet(root);
    assert_holds_no_object(root);

    let recorded = build_records_the_walk(root, &[]);

    assert_eq!(
        object_ids(root),
        recorded
            .iter()
            .map(|(entry, _, _)| entry.blob.clone())
            .collect::<BTreeSet<_>>(),
        "the object database holds the blob of each entry the build recorded, and no other object"
    );
    assert_eq!(
        recorded
            .iter()
            .map(|(entry, _, _)| held_blob(root, &entry.blob))
            .collect::<Vec<_>>(),
        recorded
            .iter()
            .map(|(entry, _, _)| Some(std::fs::read(root.join(&entry.file)).unwrap()))
            .collect::<Vec<_>>(),
        "each entry's blob holds the bytes its file holds"
    );
}

/// GIVEN the extraction tree planted outside any repository, where no object database stands,
/// WHEN the operator runs `rigger graph build`,
/// THEN the build records the walk's entries under the ids git gives the files' bytes, hashing
/// without a write, and the tree's top level holds the entries it held before, no repository
/// standing under it.
#[test]
fn a_graph_build_outside_a_repository_records_each_blob_id_and_writes_no_repository() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    assert_outside_any_repository(root);
    let found = dir_snapshot(root, "");

    build_records_the_walk(root, &[]);

    assert_eq!(
        (dir_snapshot(root, ""), git_dir(root)),
        (found, None),
        "a build outside a repository writes no repository into the tree"
    );
}

/// GIVEN the extraction tree planted in a repository whose object database holds no object, and a
/// second repository, empty too, that `GIT_DIR` names in the environment the operator runs rigger
/// with, as a git hook's environment names one,
/// WHEN the operator runs `rigger graph build`,
/// THEN the build writes the blob of each entry it records into the root's object database, and
/// none into the second repository's.
#[test]
fn a_graph_build_writes_each_blob_into_the_root_s_repository_whatever_git_dir_names() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    git_init_quiet(root);
    let second = tempfile::tempdir().unwrap();
    git_init_quiet(second.path());
    assert_holds_no_object(root);
    assert_holds_no_object(second.path());
    let git_dir = second.path().join(".git");

    let recorded = build_records_the_walk(root, &[("GIT_DIR", git_dir.to_str().unwrap())]);

    assert_eq!(
        (object_ids(root), object_ids(second.path())),
        (
            recorded
                .iter()
                .map(|(entry, _, _)| entry.blob.clone())
                .collect::<BTreeSet<_>>(),
            BTreeSet::new()
        ),
        "the build writes each entry's blob into the root's repository, never into the one \
         GIT_DIR names"
    );
}
