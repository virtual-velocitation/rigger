//! Periphery for spec 107, criterion 4 - THE EXTRACTION READS BYTES.
//!
//! The workflow-definition extraction reads a file's bytes, not the file: it parses the text it is
//! handed through `rigger::config_store::parse_workflow`. From outside the crates that own the
//! parse, this test holds that the parse of the definition the real `rigger init` writes answers
//! the workflow `load_workflow` reads from that file. NOT OWNED: the facts the code store's
//! extractor reads out of a definition's bytes, which the extractor's own tests pin beside it.

use crate::common;

use common::cli::{rigger_file, run_rigger_ok, temp_project};
use rigger::config_store::{load_workflow, parse_workflow};

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
