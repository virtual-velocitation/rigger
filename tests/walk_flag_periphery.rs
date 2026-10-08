//! Periphery for spec 107, criterion 9 - THE WALK HANDS EACH BATCH WITH ITS FLAG.
//!
//! Every walk hands its sink each batch beside one flag: set when the batch is the `gc` batch of
//! an out-of-line test module, clear for every other batch. The tests beside the code pin that at
//! one worker, inside the crates that own the walk. What they cannot see, and what this file
//! holds, is the flag as an outside caller meets it through the `rigger` facade: a closure written
//! against the public sink shape, the whole-tree walk at its default width and at several, the
//! named walk over absent, repeated and no paths, the flag against `walk_exclusions` as the one
//! answer it repeats, the flag moving with the declaration that makes a file a test module, and
//! the fallible-sink driver handing on what the real walk handed it.
//!
//! What this file OWNS: the flag each public walk hands and each public code-half function
//! answers, from outside. NOT OWNED: what a sink does with the flag - both sinks and the lag
//! advisory take it and ignore it here, and what they record is held by the tests that drive the
//! binary over the same tree.

mod common;

use rigger::eventstore::Event;
use rigger::ingest::{sink_walked_batches, BatchSink};

#[cfg(feature = "symbols")]
use common::fixtures::{
    events_of, planted_extraction_tree, walked_handoffs, wire_owned, write_file, SOURCE_BODY,
    SOURCE_PATH, TEST_MODULE_PATH, WALKED, WORKFLOW_PATH,
};
#[cfg(feature = "symbols")]
use rigger::grounder::symbols::events::{file_batches, project_batches, project_batches_paced};
#[cfg(feature = "symbols")]
use std::collections::BTreeSet;
#[cfg(feature = "symbols")]
use std::path::Path;

/// What `walk` handed the sink it was given, one `(identity, flag)` per batch in the order the
/// sink saw them. A batch is one identity's: every key of it names that identity.
#[cfg(feature = "symbols")]
fn handoffs(walk: impl FnOnce(&mut dyn BatchSink)) -> Vec<(String, bool)> {
    let mut seen: Vec<(String, bool)> = Vec::new();
    walk(&mut |batch: &[(String, &Event)], excluded: bool| {
        let identities: BTreeSet<&str> = batch
            .iter()
            .map(|(key, _)| {
                rigger::ingest::derived_key_parts(key)
                    .expect("a key names its identity")
                    .0
            })
            .collect();
        let identities: Vec<&str> = identities.into_iter().collect();
        assert_eq!(
            identities.len(),
            1,
            "one batch, one identity: {identities:?}"
        );
        seen.push((identities[0].to_string(), excluded));
    });
    seen
}

/// `pairs` as owned `(identity, flag)`.
#[cfg(feature = "symbols")]
fn owned(pairs: &[(&str, bool)]) -> Vec<(String, bool)> {
    pairs
        .iter()
        .map(|(identity, flag)| (identity.to_string(), *flag))
        .collect()
}

/// What the whole-tree walk hands over `root` through each public entry: the default-width one,
/// then the paced one at one worker and at four.
#[cfg(feature = "symbols")]
fn whole_walks(root: &Path) -> [Vec<(String, bool)>; 3] {
    let root = root.to_str().unwrap();
    [
        handoffs(|sink| {
            rigger::ingest::ingest_project_batched(root, sink);
        }),
        handoffs(|sink| {
            rigger::ingest::ingest_project_batched_paced(root, 1, sink);
        }),
        handoffs(|sink| {
            rigger::ingest::ingest_project_batched_paced(root, 4, sink);
        }),
    ]
}

/// What the named walk hands over `root` for `files`.
#[cfg(feature = "symbols")]
fn named_walk(root: &Path, files: &[&str]) -> Vec<(String, bool)> {
    let files: Vec<String> = files.iter().map(|file| file.to_string()).collect();
    handoffs(|sink| {
        rigger::ingest::ingest_files_batched(root.to_str().unwrap(), &files, sink);
    })
}

/// The extraction tree with its source file no longer declaring `checks` under `cfg(test)`: the
/// same files, the module now an ordinary one.
#[cfg(feature = "symbols")]
fn tree_declaring_the_module_plainly() -> tempfile::TempDir {
    let dir = planted_extraction_tree(write_file);
    let plain = SOURCE_BODY.replace("#[cfg(test)]\nmod checks;", "mod checks;");
    assert_eq!(
        plain.len() + "#[cfg(test)]\n".len(),
        SOURCE_BODY.len(),
        "exactly the one attribute is dropped"
    );
    write_file(&dir.path().join(SOURCE_PATH), plain.as_bytes());
    dir
}

/// Given the extraction tree, when an outside caller walks it through each public whole-tree
/// entry, then each hands the six batches in the walk's order with the flag the tree's fixture
/// records: set for the out-of-line test module's `gc` batch alone, clear for the other `gc`
/// batch, for the same path's `gd` batch and every other `gd` batch, and for the `gw` batch.
#[cfg(feature = "symbols")]
#[test]
fn every_public_whole_tree_walk_hands_only_the_out_of_line_test_modules_gc_batch_flagged() {
    let dir = planted_extraction_tree(write_file);
    let workflow = format!("gw/{WORKFLOW_PATH}");
    let expected = owned(&[
        ("gc/src/checks.rs", true),
        ("gc/src/lib.rs", false),
        ("gd/docs/architecture.md", false),
        ("gd/src/checks.rs", false),
        ("gd/src/lib.rs", false),
        (workflow.as_str(), false),
    ]);
    assert_eq!(walked_handoffs(), expected);

    let [default_width, one_worker, four_workers] = whole_walks(dir.path());
    assert_eq!(default_width, expected);
    assert_eq!(one_worker, expected);
    assert_eq!(four_workers, expected);
}

/// Given a tree whose out-of-line test modules sort between its ordinary modules, when it is
/// walked at any width, then each batch keeps its own flag: a worker finishing early never hands
/// one file's flag beside another file's batch.
#[cfg(feature = "symbols")]
#[test]
fn a_walk_at_any_width_hands_each_interleaved_test_module_its_own_flag() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (path, body) in [
        (
            "src/lib.rs",
            "#[cfg(test)]\nmod a_checks;\nmod b_plain;\n#[cfg(test)]\nmod c_checks;\n\
             mod d_plain;\n#[cfg(test)]\nmod e_checks;\n\nfn product() {}\n",
        ),
        (
            "src/a_checks.rs",
            "// WHY: the first checks sit out of line\nfn a() {}\n",
        ),
        ("src/b_plain.rs", "fn b() {}\n"),
        ("src/c_checks.rs", "fn c() {}\n"),
        ("src/d_plain.rs", "fn d() {}\n"),
        ("src/e_checks.rs", "fn e() {}\n"),
    ] {
        write_file(&root.join(path), body.as_bytes());
    }
    let expected = owned(&[
        ("gc/src/a_checks.rs", true),
        ("gc/src/b_plain.rs", false),
        ("gc/src/c_checks.rs", true),
        ("gc/src/d_plain.rs", false),
        ("gc/src/e_checks.rs", true),
        ("gc/src/lib.rs", false),
        ("gd/src/a_checks.rs", false),
    ]);

    let [default_width, one_worker, four_workers] = whole_walks(root);
    assert_eq!(default_width, expected);
    assert_eq!(one_worker, expected);
    assert_eq!(four_workers, expected);
}

/// Given the extraction tree, when an outside caller names files to the integration reindex, then
/// only the out-of-line test module's `gc` batch is handed flagged: the `gc` batch of the source
/// file, of a path the tree does not hold and of a document, and every `gd` batch, are handed
/// clear, the `gc` batches in the order the files were named.
#[cfg(feature = "symbols")]
#[test]
fn the_named_walk_hands_only_the_out_of_line_test_modules_gc_batch_flagged() {
    let dir = planted_extraction_tree(write_file);

    assert_eq!(
        named_walk(
            dir.path(),
            &[
                SOURCE_PATH,
                "src/absent.rs",
                TEST_MODULE_PATH,
                "docs/architecture.md"
            ]
        ),
        owned(&[
            ("gc/src/lib.rs", false),
            ("gc/src/absent.rs", false),
            ("gc/src/checks.rs", true),
            ("gc/docs/architecture.md", false),
            ("gd/docs/architecture.md", false),
            ("gd/src/checks.rs", false),
            ("gd/src/lib.rs", false),
        ])
    );
}

/// The named walk answers the flag per named file: a test module named twice is handed flagged
/// twice, a source file named twice clear twice, and no file named hands no batch at all.
#[cfg(feature = "symbols")]
#[test]
fn the_named_walk_flags_a_repeated_file_each_time_and_hands_nothing_for_no_file() {
    let dir = planted_extraction_tree(write_file);
    let code = |files: &[&str]| -> Vec<(String, bool)> {
        named_walk(dir.path(), files)
            .into_iter()
            .filter(|(identity, _)| identity.starts_with("gc/"))
            .collect()
    };

    assert_eq!(
        code(&[TEST_MODULE_PATH, SOURCE_PATH, TEST_MODULE_PATH, SOURCE_PATH]),
        owned(&[
            ("gc/src/checks.rs", true),
            ("gc/src/lib.rs", false),
            ("gc/src/checks.rs", true),
            ("gc/src/lib.rs", false),
        ])
    );
    assert_eq!(named_walk(dir.path(), &[]), owned(&[]));
}

/// The flag is `walk_exclusions`' answer and no other: over the extraction tree, the identities
/// the whole walk hands flagged, and the ones the named walk hands flagged, are exactly the
/// identities `walk_exclusions` names for the same tree.
#[cfg(feature = "symbols")]
#[test]
fn the_flagged_batches_are_exactly_the_identities_walk_exclusions_names() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    let (_, excluded) = rigger::ingest::walk_exclusions(root.to_str().unwrap());
    let named_by_the_exclusions: Vec<String> = excluded.into_iter().collect();
    let flagged = |handed: Vec<(String, bool)>| -> Vec<String> {
        handed
            .into_iter()
            .filter(|(_, excluded)| *excluded)
            .map(|(identity, _)| identity)
            .collect()
    };

    let [whole, _, _] = whole_walks(root);
    assert_eq!(named_by_the_exclusions, ["gc/src/checks.rs"]);
    assert_eq!(flagged(whole), named_by_the_exclusions);
    assert_eq!(
        flagged(named_walk(root, &[SOURCE_PATH, TEST_MODULE_PATH])),
        named_by_the_exclusions
    );
}

/// The flag follows the declaration, never the path: once the source file declares `checks`
/// without `cfg(test)`, the same six identities are walked and every one of them, the module's
/// `gc` batch among them, is handed clear - by the whole walk at each width and by the named one.
#[cfg(feature = "symbols")]
#[test]
fn a_module_no_longer_declared_under_cfg_test_is_handed_clear() {
    let dir = tree_declaring_the_module_plainly();
    let root = dir.path();
    let all_clear: Vec<(String, bool)> = walked_handoffs()
        .into_iter()
        .map(|(identity, _)| (identity, false))
        .collect();
    assert_eq!(all_clear.len(), 6);

    let [default_width, one_worker, four_workers] = whole_walks(root);
    assert_eq!(default_width, all_clear);
    assert_eq!(one_worker, all_clear);
    assert_eq!(four_workers, all_clear);
    assert_eq!(
        named_walk(root, &[TEST_MODULE_PATH, SOURCE_PATH]),
        owned(&[
            ("gc/src/checks.rs", false),
            ("gc/src/lib.rs", false),
            ("gd/src/checks.rs", false),
            ("gd/src/lib.rs", false),
        ])
    );
}

/// One code batch as an outside caller compares it: its file, its flag and its events as
/// `(type, payload text)`.
#[cfg(feature = "symbols")]
type CodeBatch = (String, bool, Vec<(String, String)>);

/// `batches`, the code half's answer, as [`CodeBatch`]es in the order it was answered.
#[cfg(feature = "symbols")]
fn code_batches(batches: &[(String, Vec<Event>, bool)]) -> Vec<CodeBatch> {
    batches
        .iter()
        .map(|(file, events, excluded)| (file.clone(), *excluded, wire_owned(events)))
        .collect()
}

/// The code half's three public functions answer each file's flag beside the batch it belongs
/// to: over the extraction tree the flagged batch is the test module's one boundary event and the
/// clear one the source file's whole extraction, as the tree's fixture records both, from the
/// whole walk at either width and from the named walk in the order it was named.
#[cfg(feature = "symbols")]
#[test]
fn the_code_half_answers_each_files_flag_beside_the_batch_it_lowered_for_it() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path().to_str().unwrap();
    let recorded: Vec<CodeBatch> = WALKED
        .iter()
        .filter(|batch| batch.prefix == "gc")
        .map(|batch| {
            (
                batch.path.to_string(),
                batch.excluded,
                wire_owned(&events_of(batch.events)),
            )
        })
        .collect();
    let flags: Vec<(&str, bool, usize)> = recorded
        .iter()
        .map(|(file, excluded, events)| (file.as_str(), *excluded, events.len()))
        .collect();
    assert_eq!(flags[0], ("src/checks.rs", true, 1));
    assert_eq!((flags[1].0, flags[1].1), ("src/lib.rs", false));
    assert_eq!(flags.len(), 2);

    assert_eq!(code_batches(&project_batches(root)), recorded);
    assert_eq!(code_batches(&project_batches_paced(root, 1).0), recorded);
    assert_eq!(code_batches(&project_batches_paced(root, 4).0), recorded);

    let named = [SOURCE_PATH.to_string(), TEST_MODULE_PATH.to_string()];
    let reversed: Vec<CodeBatch> = recorded.iter().rev().cloned().collect();
    assert_eq!(code_batches(&file_batches(root, &named)), reversed);
}

/// Given the extraction tree, when the fallible-sink driver drives the real walk into a sink that
/// fails the flagged batch, then the sink still saw every batch with the flag the walk handed it
/// and the driver answers that batch's error; a sink failing every clear batch instead is
/// answered the first of them.
#[cfg(feature = "symbols")]
#[test]
fn the_fallible_sink_driver_hands_on_the_flag_the_real_walk_handed_each_batch() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path().to_str().unwrap();
    let drive = |fails: bool| -> (Result<(), String>, Vec<(String, bool)>) {
        let mut seen: Vec<(String, bool)> = Vec::new();
        let answer = sink_walked_batches(
            |sink| {
                rigger::ingest::ingest_project_batched(root, sink);
            },
            |batch, excluded| {
                let identity = rigger::ingest::derived_key_parts(&batch[0].0)
                    .expect("a key names its identity")
                    .0
                    .to_string();
                seen.push((identity.clone(), excluded));
                if excluded == fails {
                    Err(identity)
                } else {
                    Ok(())
                }
            },
        );
        (answer, seen)
    };

    assert_eq!(
        drive(true),
        (Err("gc/src/checks.rs".to_string()), walked_handoffs())
    );
    assert_eq!(
        drive(false),
        (Err("gc/src/lib.rs".to_string()), walked_handoffs())
    );
}

/// In either lane a closure written against the public sink shape is a `BatchSink`, and the
/// fallible-sink driver hands it each batch beside the flag the walk gave: set and clear both
/// arrive as given, in order, with the batch they were handed beside.
#[test]
fn the_fallible_sink_driver_hands_a_hand_made_walks_flags_to_an_outside_sink() {
    let event = Event::new("CodeEntityExtracted", b"{}".to_vec());
    let walk = |sink: &mut dyn BatchSink| {
        sink(&[("gc/a.rs@h#0".to_string(), &event)], true);
        sink(&[("gd/a.rs@h#0".to_string(), &event)], false);
        sink(&[("gc/b.rs@h#0".to_string(), &event)], true);
    };

    let mut seen: Vec<(String, bool)> = Vec::new();
    let answer = sink_walked_batches(walk, |batch, excluded| {
        seen.push((batch[0].0.clone(), excluded));
        Ok::<(), String>(())
    });

    assert_eq!(answer, Ok(()));
    assert_eq!(
        seen,
        [
            ("gc/a.rs@h#0".to_string(), true),
            ("gd/a.rs@h#0".to_string(), false),
            ("gc/b.rs@h#0".to_string(), true),
        ]
    );
}

/// Without the extraction pass there is no walk: both public entries take a sink of the flagged
/// shape and hand it no batch, over a tree that holds an out-of-line test module.
#[cfg(not(feature = "symbols"))]
#[test]
fn without_the_extraction_pass_neither_walk_hands_a_flagged_sink_any_batch() {
    use common::fixtures::{planted_extraction_tree, write_file, SOURCE_PATH, TEST_MODULE_PATH};

    let dir = planted_extraction_tree(write_file);
    let root = dir.path().to_str().unwrap();
    let mut handed: Vec<(usize, bool)> = Vec::new();

    rigger::ingest::ingest_project_batched(root, |batch: &[(String, &Event)], excluded: bool| {
        handed.push((batch.len(), excluded))
    });
    rigger::ingest::ingest_files_batched(
        root,
        &[TEST_MODULE_PATH.to_string(), SOURCE_PATH.to_string()],
        |batch: &[(String, &Event)], excluded: bool| handed.push((batch.len(), excluded)),
    );

    assert_eq!(handed, Vec::<(usize, bool)>::new());
}

/// Given a project holding the extraction tree's source file and the out-of-line test module it
/// declares, when the operator runs `rigger step` over a log that records nothing, then the run's
/// sink - handed each batch beside its flag - records every derived event the walk mints under
/// the key the walk minted, in the walk's order, the flagged batch's one boundary event among
/// them under the generation the tree's fixture records: the flag it is handed changes nothing
/// it records.
#[cfg(feature = "symbols")]
#[test]
fn a_step_records_the_flagged_batch_as_the_walk_minted_it() {
    use common::cli::{
        identified_git_project, init_event_log, read_run_events, step_line, write_workflow,
    };
    use common::fixtures::{minted_events, TEST_MODULE_BODY};

    /// One keyed event as the log and the walk compare: its key, its type and its payload read
    /// as JSON, so the order a store writes a payload's fields in is no part of the comparison.
    type Keyed = (String, String, serde_json::Value);
    let keyed = |key: String, event: &Event| -> Keyed {
        let payload = serde_json::from_slice(&event.data).expect("a payload is JSON");
        (key, event.type_.clone(), payload)
    };

    let dir = identified_git_project();
    let root = dir.path();
    write_file(
        &root.join(".gitignore"),
        format!("{}/\n", rigger::config::RIGGER_DIR).as_bytes(),
    );
    write_workflow(root, "");
    init_event_log(root);
    write_file(&root.join(SOURCE_PATH), SOURCE_BODY.as_bytes());
    write_file(&root.join(TEST_MODULE_PATH), TEST_MODULE_BODY.as_bytes());
    let _ = common::git::run_git(root, &["add", "-A"]);
    let _ = common::git::run_git(root, &["commit", "-q", "-m", "tree"]);
    let minted: Vec<Keyed> = minted_events(root)
        .into_iter()
        .map(|(key, event)| keyed(key, &event))
        .collect();

    step_line(root, "the step that ingests the tree");

    let recorded: Vec<Keyed> = read_run_events(root)
        .iter()
        .filter(|event| rigger::ingest::is_derived_index_type(&event.type_))
        .map(|event| {
            let key = event.meta.get(rigger::ingest::META_REPLAY_KEY);
            keyed(key.expect("a derived event is keyed").clone(), event)
        })
        .collect();
    assert_eq!(recorded, minted);

    let flagged = &WALKED[0];
    assert_eq!(
        (flagged.prefix, flagged.path, flagged.excluded),
        ("gc", TEST_MODULE_PATH, true)
    );
    let identity = &walked_handoffs()[0].0;
    let of_the_flagged_batch: Vec<Keyed> = recorded
        .into_iter()
        .filter(|(key, _, _)| key.starts_with(&format!("{identity}@")))
        .collect();
    let as_the_fixture_records_it: Vec<Keyed> = events_of(flagged.events)
        .iter()
        .enumerate()
        .map(|(i, event)| keyed(format!("{identity}@{}#{i}", flagged.generation), event))
        .collect();
    assert_eq!(as_the_fixture_records_it.len(), 1);
    assert_eq!(of_the_flagged_batch, as_the_fixture_records_it);
}
