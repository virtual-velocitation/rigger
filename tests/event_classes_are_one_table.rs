//! Spec 107 criterion 1, THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under
//! `src/` or `crates/` declares sits in exactly one of three class lists - the derived index
//! (`ingest::DERIVED_INDEX_TYPES`), the episodic types (`retention::EPISODIC_TYPES`) and the
//! knowledge list this file holds - so a new type is classified the day it is added. A `TYPE_`
//! constant neither the knowledge enumeration nor the episodic list names is added to the
//! knowledge list here.

mod common;

use common::repo::{collect_rs_files, repo_root};
use rigger::contextgraph::{
    TYPE_ALIAS_DEFINED, TYPE_ALIAS_UNRESOLVED, TYPE_COMMUNITY_ASSIGNED, TYPE_CONCEPT_DERIVED,
    TYPE_CONCEPT_REALIZED,
};
use rigger::ingest::DERIVED_INDEX_TYPES;
use rigger::retention::{
    GenerationIngested, EPISODIC_TYPES, PERCEPTION_TYPES, TYPE_GENERATION_INGESTED,
};
use rigger::run::read::{ADOPTION_TYPES, CARRY_OVER_TYPES};
use rigger::run::{MINT_DECISION_TYPES, RUN_CLOSURE_TYPES};
use rigger::spawn::TYPE_SPAWN_RESULT;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// `line` past its indentation and its visibility, whatever the visibility restricts to: a
/// leading `pub ` or `pub(...) ` is dropped, and any other line is returned past its indentation.
fn past_visibility(line: &str) -> &str {
    let item = line.trim_start();
    let Some(rest) = item.strip_prefix("pub") else {
        return item;
    };
    let rest = rest
        .strip_prefix('(')
        .and_then(|restricted| restricted.split_once(')'))
        .map_or(rest, |(_, after)| after);
    rest.strip_prefix(' ').unwrap_or(item)
}

/// Whether `line` opens a `TYPE_` constant, behind any visibility or none. A `static TYPE_` item
/// fails the scan, naming the line: a constant declares a type and a static is never classified.
fn opens_type_const(line: &str) -> bool {
    let item = past_visibility(line);
    assert!(
        !item.starts_with("static TYPE_") && !item.starts_with("static mut TYPE_"),
        "`{}`: a TYPE_ item is a constant, never a static",
        line.trim()
    );
    item.starts_with("const TYPE_")
}

/// Every `const TYPE_` statement in `text`, each from its opening line to the line holding its
/// `;`, the lines trimmed and joined by one space.
fn type_const_statements(text: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut open: Option<Vec<&str>> = None;
    for line in text.lines() {
        if open.is_none() && opens_type_const(line) {
            open = Some(Vec::new());
        }
        if let Some(lines) = open.as_mut() {
            lines.push(line.trim());
            if line.contains(';') {
                statements.push(lines.join(" "));
                open = None;
            }
        }
    }
    statements
}

/// Fails the scan on a `const TYPE_` statement whose right side it cannot classify.
fn unclassifiable(statement: &str) -> ! {
    panic!("`{statement}`: the right side is neither a string literal nor a TYPE_ alias")
}

/// What one `const TYPE_` statement declares: the event type its string literal spells, or none
/// for an alias whose right side is a path ending in a `TYPE_` identifier. Any other right side
/// is a shape the scan cannot classify and fails it, naming the statement.
fn declared_type(statement: &str) -> Option<String> {
    let Some((_, right)) = statement.split_once('=') else {
        unclassifiable(statement)
    };
    let right = right.trim();
    let right = right.strip_suffix(';').unwrap_or(right).trim();
    if right.len() >= 2 && right.starts_with('"') && right.ends_with('"') {
        return Some(right[1..right.len() - 1].to_string());
    }
    let last = right.rsplit("::").next().unwrap_or(right);
    let is_path = right
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':');
    if is_path && last.starts_with("TYPE_") {
        return None;
    }
    unclassifiable(statement)
}

/// Every event type a `TYPE_` constant under `<root>/src` or `<root>/crates` declares, each with
/// the files (relative to `root`, `/`-separated) whose constants spell it.
fn declared_types(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    collect_rs_files(&root.join("crates"), &mut files);
    let mut declared: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        for statement in type_const_statements(&text) {
            if let Some(type_) = declared_type(&statement) {
                declared.entry(type_).or_default().insert(rel.clone());
            }
        }
    }
    declared
}

/// The four cross-run lists: every type a fold reads across runs.
fn cross_run_types() -> BTreeSet<&'static str> {
    CARRY_OVER_TYPES
        .iter()
        .chain(ADOPTION_TYPES.iter())
        .chain(MINT_DECISION_TYPES.iter())
        .chain(RUN_CLOSURE_TYPES.iter())
        .copied()
        .collect()
}

/// KNOWLEDGE: the four cross-run lists, every type outside the derived index whose fold arm
/// changes the graph, and the ledger entry.
fn knowledge_types() -> BTreeSet<&'static str> {
    let mut knowledge = cross_run_types();
    knowledge.extend([
        TYPE_SPAWN_RESULT,
        TYPE_ALIAS_DEFINED,
        TYPE_ALIAS_UNRESOLVED,
        TYPE_COMMUNITY_ASSIGNED,
        TYPE_CONCEPT_DERIVED,
        TYPE_CONCEPT_REALIZED,
        TYPE_GENERATION_INGESTED,
    ]);
    knowledge
}

#[test]
fn every_declared_event_type_sits_in_exactly_one_class_list() {
    let scanned: BTreeSet<String> = declared_types(&repo_root()).into_keys().collect();
    let derived: BTreeSet<&str> = DERIVED_INDEX_TYPES.iter().copied().collect();
    let episodic: BTreeSet<&str> = EPISODIC_TYPES.iter().copied().collect();
    let knowledge = knowledge_types();
    assert_eq!(
        derived.len(),
        DERIVED_INDEX_TYPES.len(),
        "the derived index names a type twice"
    );
    assert_eq!(
        episodic.len(),
        EPISODIC_TYPES.len(),
        "the episodic list names a type twice"
    );
    assert_eq!(
        derived
            .intersection(&episodic)
            .copied()
            .collect::<Vec<&str>>(),
        Vec::<&str>::new(),
        "a type is classified both derived and episodic"
    );
    assert_eq!(
        derived
            .intersection(&knowledge)
            .copied()
            .collect::<Vec<&str>>(),
        Vec::<&str>::new(),
        "a type is classified both derived and knowledge"
    );
    assert_eq!(
        episodic
            .intersection(&knowledge)
            .copied()
            .collect::<Vec<&str>>(),
        Vec::<&str>::new(),
        "a type is classified both episodic and knowledge"
    );
    assert_eq!(
        cross_run_types()
            .intersection(&episodic)
            .copied()
            .collect::<Vec<&str>>(),
        Vec::<&str>::new(),
        "a type of the four cross-run lists is classified episodic"
    );
    let classified: BTreeSet<String> = derived
        .iter()
        .chain(episodic.iter())
        .chain(knowledge.iter())
        .map(|t| (*t).to_string())
        .collect();
    assert_eq!(
        scanned.difference(&classified).collect::<Vec<&String>>(),
        Vec::<&String>::new(),
        "declared by a TYPE_ constant but named by no class list: add it to knowledge_types() \
         here unless the episodic list names it"
    );
    assert_eq!(
        classified.difference(&scanned).collect::<Vec<&String>>(),
        Vec::<&String>::new(),
        "named by a class list but declared by no TYPE_ constant under src/ or crates/"
    );
}

#[test]
fn the_episodic_and_perception_lists_are_exactly_the_decided_ones() {
    assert_eq!(
        TYPE_GENERATION_INGESTED, "GenerationIngested",
        "the ledger entry's type string"
    );
    assert_eq!(
        EPISODIC_TYPES,
        [
            "SpawnRequested",
            "GateVerdict",
            "GatePromoted",
            "GateDemoted",
            "UnitProposed",
            "BlastRadiusComputed",
            "ScopeCreep",
            "FileTouched",
            "SpecDefect",
            "ManualReview",
            "DeferredGateFailed",
            "TaskAborted",
            "BudgetExhausted",
            "AgentProgress",
            "SpawnLaunched",
            "StopFailure",
        ],
        "the episodic list is the decided sixteen, in order"
    );
    assert_eq!(
        PERCEPTION_TYPES,
        [
            "CodeEntityExtracted",
            "EdgeInferred",
            "DocConceptExtracted",
            "DocLinkExtracted",
            "GenerationIngested",
        ],
        "the perception list is the decided five, in order"
    );
    assert_eq!(
        PERCEPTION_TYPES[..4].to_vec(),
        DERIVED_INDEX_TYPES.to_vec(),
        "the perception list is the derived index in order, then the ledger entry"
    );
}

#[test]
fn the_ledger_entry_payload_carries_its_five_fields_on_the_wire() {
    let entry = GenerationIngested {
        prefix: "gc".to_string(),
        file: "src/a.rs".to_string(),
        generation: "abc123".to_string(),
        blob: "9f2c".to_string(),
        excluded: true,
    };
    let wire = serde_json::json!({
        "prefix": "gc",
        "file": "src/a.rs",
        "generation": "abc123",
        "blob": "9f2c",
        "excluded": true,
    });
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        wire,
        "the entry serializes to its five named fields"
    );
    assert_eq!(
        serde_json::from_value::<GenerationIngested>(wire).unwrap(),
        entry,
        "the five named fields read back as the entry"
    );
}

#[test]
fn the_conductor_type_strings_are_spelled_once_in_the_ledger() {
    let declared = declared_types(&repo_root());
    for type_ in [
        "GatePromoted",
        "GateDemoted",
        "ScopeCreep",
        "TaskAborted",
        "SpecDefect",
    ] {
        assert_eq!(
            declared
                .get(type_)
                .map(|files| files.iter().map(String::as_str).collect::<Vec<_>>()),
            Some(vec!["crates/rigger-domain/src/ledger.rs"]),
            "{type_} is spelled once, in the ledger"
        );
    }
}

#[test]
fn a_string_literal_declares_its_type_and_an_alias_declares_none() {
    assert_eq!(
        declared_type("pub const TYPE_A: &str = \"Alpha\";"),
        Some("Alpha".to_string()),
        "a string literal declares the type it spells"
    );
    assert_eq!(
        declared_type("pub const TYPE_B: &str = ledger::TYPE_B;"),
        None,
        "a one-segment path alias declares no type"
    );
    assert_eq!(
        declared_type("const TYPE_C: &str = TYPE_B;"),
        None,
        "a bare identifier alias declares no type"
    );
    assert_eq!(
        declared_type("pub const TYPE_D: &str = crate::blocker::TYPE_D;"),
        None,
        "a multi-segment path alias declares no type"
    );
    assert_eq!(
        type_const_statements(concat!(
            "pub const TYPE_LONG: &str =\n",
            "    \"Long\";\n",
            "// const TYPE_COMMENTED: &str = \"No\";\n",
            "    pub(crate) const TYPE_B: &str = a::TYPE_A;\n",
            "let type_x = 1;\n",
            "const OTHER: &str = \"Other\";\n",
            "pub(super) const TYPE_S: &str = \"Super\";\n",
            "    pub(in crate::x) const TYPE_P: &str = \"Path\";\n",
            "const TYPE_BARE: &str = \"Bare\";\n",
            "pub(super) const OTHER_S: &str = \"No\";\n",
            "pub(in crate::x) fn type_const() {}\n",
            "static OTHER_STATIC: &str = \"No\";\n",
            "pub static mut COUNT: u8 = 0;\n",
        )),
        vec![
            "pub const TYPE_LONG: &str = \"Long\";".to_string(),
            "pub(crate) const TYPE_B: &str = a::TYPE_A;".to_string(),
            "pub(super) const TYPE_S: &str = \"Super\";".to_string(),
            "pub(in crate::x) const TYPE_P: &str = \"Path\";".to_string(),
            "const TYPE_BARE: &str = \"Bare\";".to_string(),
        ],
        "a statement opens behind any visibility and runs from its opening line to its semicolon"
    );
    for line in [
        "static TYPE_S: &str = \"Static\";",
        "pub static TYPE_S: &str = \"Static\";",
        "pub(super) static TYPE_S: &str = \"Static\";",
        "pub(in crate::x) static TYPE_S: &str = \"Static\";",
        "pub(crate) static mut TYPE_S: &str = \"Static\";",
    ] {
        let text = format!("const TYPE_A: &str = \"Alpha\";\n    {line}\n");
        assert_eq!(
            scan_failure(|| type_const_statements(&text)),
            format!("`{line}`: a TYPE_ item is a constant, never a static"),
            "a static TYPE_ item fails the scan naming its line, behind any visibility"
        );
    }
}

/// The message the scan fails with when `scan` runs.
fn scan_failure<T>(scan: impl FnOnce() -> T + std::panic::UnwindSafe) -> String {
    let failure = std::panic::catch_unwind(scan)
        .err()
        .expect("the scan reads the input instead of failing");
    failure
        .downcast_ref::<String>()
        .cloned()
        .expect("the scan fails with a formatted message")
}

#[test]
fn a_right_side_that_is_neither_a_literal_nor_a_type_alias_fails_the_scan_naming_it() {
    for statement in [
        "const TYPE_X: &str = concat!(\"a\", \"b\");",
        "const TYPE_X: &str = other::NAME;",
        "const TYPE_X: &str = \"Open;",
        "const TYPE_X: &str",
    ] {
        assert_eq!(
            scan_failure(|| declared_type(statement)),
            format!("`{statement}`: the right side is neither a string literal nor a TYPE_ alias"),
            "a macro call, an alias of a constant not named TYPE_, an unclosed literal and a \
             statement with no right side each fail the scan"
        );
    }
}
