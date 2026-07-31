//! Spec 57, criterion 4 - the HANDBOOK tells the new grounding truth (and keeps telling it).
//!
//! `docs/handbook/authoring-loops.md` ships an operator-copyable `.rigger/workflow.yml`
//! example, and it claims to reproduce the Rigger repo's own committed workflow. Spec 57
//! retired the vector-embedding grounder (`turbovec`) and its composite mode (`hybrid`): the
//! structural `symbols` grounder is the default, and `grep` / `nop` are the only explicit
//! opt-outs. A handbook config line that still reads `grounder: turbovec` is a copy-paste trap:
//! an operator pasting it hits the LOUD retired-grounder migration error
//! (`grounder::retired_grounder_error`) instead of a working run, and it makes the "reproduces
//! `.rigger/workflow.yml`" claim a lie once the repo config moved to `symbols`.
//!
//! This pin closes the recurrence gap the front-door `README.md` / `architecture.md` pins do
//! not cover: the handbook surface. Two properties are held:
//!
//!   1. The copyable config example REPRODUCES the repo's own grounder default - the
//!      `grounder:` value in `docs/handbook/authoring-loops.md` equals the one in the committed
//!      `.rigger/workflow.yml`, and both are the current default (`symbols`). Drift in either
//!      file - the handbook going stale, or the repo config flipping - fails RED.
//!   2. NO handbook document names a retired grounder as a live config choice. The retired
//!      engine names appear in neither a `grounder:` directive nor a grounder-enumeration
//!      comment. The guard is anchored to grounder-context forms (a directive value or a
//!      pipe-enumeration) so it cannot false-positive on unrelated prose (e.g. a "hybrid"
//!      push/pull design elsewhere in the docs).
//!
//! Like the sibling doc pins, this test reads the committed files (resolved from
//! `CARGO_MANIFEST_DIR`, so it does not depend on the process CWD), parses text, and touches
//! no backend symbol. It is deliberately NOT feature-gated: it runs identically in both lanes.

use std::path::{Path, PathBuf};

/// The repo root, resolved from the manifest dir so the test does not depend on the process
/// CWD (integration tests may run from anywhere).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Extract the value of the first `grounder:` key in a YAML-ish document: the token after the
/// colon, with any inline `#` comment and surrounding whitespace stripped. Returns `None` if no
/// `grounder:` key is present.
fn grounder_value(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("grounder:"))
        .map(|rest| {
            let no_comment = rest.split('#').next().unwrap_or(rest);
            no_comment.trim().to_string()
        })
}

/// Every markdown file under `docs/handbook/`, so the negative guard scans the whole surface,
/// not just the one file this unit edited.
fn handbook_docs() -> Vec<PathBuf> {
    let dir = repo_root().join("docs").join("handbook");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    out.sort();
    assert!(
        !out.is_empty(),
        "expected markdown files under {}",
        dir.display()
    );
    out
}

/// Grounder-context forms that name a RETIRED engine as a live config choice (spec 57),
/// checked case-insensitively. Each is anchored to a `grounder:` directive value or a
/// pipe-enumeration comment, so the guard catches a reintroduced retired-grounder config line
/// (the exact recurrence this unit fixes) without false-positiving on unrelated prose.
const RETIRED_GROUNDER_FORMS: &[&str] = &[
    // a config directive selecting the retired engine or its composite mode.
    "grounder: turbovec",
    "grounder: hybrid",
    // a grounder-enumeration comment offering the retired engine as a valid option,
    // e.g. `# turbovec | grep | nop` or `# symbols | turbovec | grep`.
    "turbovec |",
    "| turbovec",
];

#[test]
fn handbook_config_example_reproduces_the_repo_grounder_default() {
    let root = repo_root();
    let handbook = root
        .join("docs")
        .join("handbook")
        .join("authoring-loops.md");
    let workflow = root.join(".rigger").join("workflow.yml");

    let handbook_value = grounder_value(&read(&handbook)).unwrap_or_else(|| {
        panic!(
            "docs/handbook/authoring-loops.md must carry a `grounder:` line in its \
             workflow example (spec 57, criterion 4)"
        )
    });
    let workflow_value = grounder_value(&read(&workflow)).unwrap_or_else(|| {
        panic!(".rigger/workflow.yml must carry a `grounder:` default (spec 57, criterion 4)")
    });

    assert_eq!(
        workflow_value, "symbols",
        "the repo's own .rigger/workflow.yml must default to the structural `symbols` grounder \
         (spec 57): the vector engine `turbovec` and its `hybrid` composite were retired. Found: \
         grounder: {workflow_value:?}"
    );
    assert_eq!(
        handbook_value, workflow_value,
        "docs/handbook/authoring-loops.md claims its example reproduces the repo's own \
         .rigger/workflow.yml, so its `grounder:` value ({handbook_value:?}) must equal the \
         committed workflow's ({workflow_value:?}). An operator copies this block verbatim - a \
         stale `grounder: turbovec` here pastes the retired engine and hits the loud \
         retired-grounder migration error instead of a working run."
    );
}

#[test]
fn no_handbook_document_names_a_retired_grounder_as_a_live_choice() {
    let offenders: Vec<String> = handbook_docs()
        .into_iter()
        .flat_map(|path| {
            let text = read(&path).to_lowercase();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<?>")
                .to_string();
            RETIRED_GROUNDER_FORMS
                .iter()
                .filter(move |form| text.contains(**form))
                .map(move |form| format!("docs/handbook/{name}: {form:?}"))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "no handbook document may name a RETIRED grounder as a live config choice (spec 57, \
         criterion 4): the vector engine `turbovec` and its `hybrid` composite are gone, so a \
         `grounder: turbovec` directive or a `# turbovec | ...` enumeration is a copy-paste trap \
         that resolves to the loud retired-grounder migration error. `symbols` is the default; \
         `grep` and `nop` are the only opt-outs. Retired-grounder config forms still present: \
         {offenders:#?}"
    );
}
