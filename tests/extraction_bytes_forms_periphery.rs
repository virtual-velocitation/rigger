//! Periphery for spec 107, criterion 4 - THE EXTRACTION READS BYTES.
//!
//! The criterion splits each extraction half's bytes form out of its disk read. From outside the
//! crates that own the splits, these tests hold the public forms through the paths an outside
//! caller reaches them by: the named walk beside the whole walk, `rigger::ingest::walk_exclusions`
//! in both lanes, and `rigger::config_store::parse_workflow` over the definition the real
//! `rigger init` writes. NOT OWNED: each split against its own caller and against literal
//! payloads, which the tests of the module owning the split pin beside the code.

use crate::common;

use common::cli::{rigger_file, run_rigger_ok, temp_project};
use common::fixtures::{planted_extraction_tree, write_file};
use rigger::config_store::{load_workflow, parse_workflow};

#[cfg(feature = "symbols")]
use common::fixtures::{
    events_of, minted_events, wire_owned, WalkedBatch, SOURCE_PATH, TEST_MODULE_PATH, WALKED,
};
#[cfg(feature = "symbols")]
use rigger::eventstore::Event;

/// One batch of the derived index: its identity, its generation and its events as
/// `(type, payload text)`, in order.
#[cfg(feature = "symbols")]
type Batch = (String, String, Vec<(String, String)>);

/// `keyed` - events beside their `<identity>@<generation>#<i>` keys, in emit order - gathered
/// into the batches the keys name: each run of one identity at one generation, with its events.
#[cfg(feature = "symbols")]
fn grouped(keyed: &[(String, Event)]) -> Vec<(String, String, Vec<Event>)> {
    let mut out: Vec<(String, String, Vec<Event>)> = Vec::new();
    for (key, event) in keyed {
        let (identity, generation) =
            rigger::ingest::derived_key_parts(key).expect("a key names its generation");
        match out.last_mut() {
            Some((i, g, events)) if i == identity && g == generation => events.push(event.clone()),
            _ => out.push((
                identity.to_string(),
                generation.to_string(),
                vec![event.clone()],
            )),
        }
    }
    out
}

/// [`grouped`] with each event as `(type, payload text)`.
#[cfg(feature = "symbols")]
fn batches(keyed: &[(String, Event)]) -> Vec<Batch> {
    grouped(keyed)
        .into_iter()
        .map(|(identity, generation, events)| (identity, generation, wire_owned(&events)))
        .collect()
}

/// The batches the extraction tree's fixture records that `admit` admits, in the walk's order.
#[cfg(feature = "symbols")]
fn walked(admit: impl Fn(&WalkedBatch) -> bool) -> Vec<Batch> {
    WALKED
        .iter()
        .filter(|batch| admit(batch))
        .map(|batch| {
            (
                format!("{}/{}", batch.prefix, batch.path),
                batch.generation.to_string(),
                wire_owned(&events_of(batch.events)),
            )
        })
        .collect()
}

/// `keyed` as `(key, type, payload bytes)`, in order: the form two keyed sequences compare by.
#[cfg(feature = "symbols")]
fn triples(keyed: &[(String, Event)]) -> Vec<(&str, &str, &[u8])> {
    keyed
        .iter()
        .map(|(key, event)| (key.as_str(), event.type_.as_str(), event.data.as_slice()))
        .collect()
}

/// The paths of the index `rigger::ingest::walk_exclusions` answers over the extraction tree, and
/// the identities it excludes, each in its own sorted order.
fn walk_exclusions_over_the_extraction_tree() -> (Vec<String>, Vec<String>) {
    let dir = planted_extraction_tree(write_file);
    let (index, excluded) = rigger::ingest::walk_exclusions(dir.path().to_str().unwrap());
    (
        index.files().keys().cloned().collect(),
        excluded.into_iter().collect(),
    )
}

/// The named walk hands, for the two source files it is named, the keyed events the whole walk
/// hands for those files - the out-of-line test module hollowed under `gc` in both - and they are
/// the four batches the tree's fixture records for the two paths.
#[cfg(feature = "symbols")]
#[test]
fn the_named_walk_hands_the_batches_the_whole_walk_hands_for_the_files_it_is_named() {
    let dir = planted_extraction_tree(write_file);
    let root = dir.path();
    let files = [TEST_MODULE_PATH.to_string(), SOURCE_PATH.to_string()];
    let names = |path: &str| files.iter().any(|file| file == path);

    let mut named: Vec<(String, Event)> = Vec::new();
    rigger::ingest::ingest_files_batched(root.to_str().unwrap(), &files, |batch, _| {
        for (key, event) in batch {
            named.push((key.clone(), (*event).clone()));
        }
    });
    let whole: Vec<(String, Event)> = minted_events(root)
        .into_iter()
        .filter(|(key, _)| {
            let (identity, _) =
                rigger::ingest::derived_key_parts(key).expect("a key names its generation");
            let (_, path) = identity
                .split_once('/')
                .expect("an identity names its prefix");
            names(path)
        })
        .collect();

    assert_eq!(triples(&named), triples(&whole));
    assert_eq!(batches(&named), walked(|batch| names(batch.path)));
}

/// With the extraction pass compiled, `walk_exclusions` reads the tree into an index of its two
/// source files and names the out-of-line test module's `gc` identity alone: the same path's `gd`
/// identity, and every other identity, is left out.
#[cfg(feature = "symbols")]
#[test]
fn walk_exclusions_names_the_out_of_line_test_modules_gc_identity_and_no_other_identity() {
    assert_eq!(
        walk_exclusions_over_the_extraction_tree(),
        (
            vec![TEST_MODULE_PATH.to_string(), SOURCE_PATH.to_string()],
            vec!["gc/src/checks.rs".to_string()],
        )
    );
}

/// Without the extraction pass, `walk_exclusions` answers the empty index and excludes nothing,
/// over a tree that holds an out-of-line test module.
#[cfg(not(feature = "symbols"))]
#[test]
fn walk_exclusions_without_the_extraction_pass_answers_the_empty_index_and_excludes_nothing() {
    assert_eq!(
        walk_exclusions_over_the_extraction_tree(),
        (Vec::<String>::new(), Vec::<String>::new())
    );

    let dir = planted_extraction_tree(write_file);
    let (index, _) = rigger::ingest::walk_exclusions(dir.path().to_str().unwrap());
    assert_eq!(
        index,
        rigger::grounder::symbols::model::SymbolIndex::default()
    );
}

/// Given a project `rigger init` scaffolded, `parse_workflow` over the text of the workflow
/// definition it wrote answers the workflow `load_workflow` reads from that file, with every
/// stage the definition's own YAML holds named after its key.
#[test]
fn parse_workflow_answers_load_workflow_for_the_definition_init_wrote() {
    let dir = temp_project();
    let root = dir.path();
    run_rigger_ok(root, &["init"]);
    let path = rigger_file(root, "workflow.yml");
    let text = std::fs::read_to_string(&path).expect("init wrote the workflow definition");

    let parsed = parse_workflow(&text).expect("the definition init wrote parses");
    let loaded = load_workflow(&path).expect("the definition init wrote loads");
    assert_eq!(format!("{parsed:?}"), format!("{loaded:?}"));

    let yaml: serde_yaml::Value = serde_yaml::from_str(&text).expect("the definition is YAML");
    let mut keys: Vec<&str> = yaml["stages"]
        .as_mapping()
        .expect("the definition holds a stage map")
        .keys()
        .map(|key| key.as_str().expect("a stage key is a string"))
        .collect();
    keys.sort_unstable();
    assert_ne!(keys, Vec::<&str>::new(), "init writes stages");
    let named: Vec<(&str, &str)> = parsed
        .stages
        .iter()
        .map(|(key, stage)| (key.as_str(), stage.name.as_str()))
        .collect();
    let expected: Vec<(&str, &str)> = keys.iter().map(|key| (*key, *key)).collect();
    assert_eq!(named, expected);
}
