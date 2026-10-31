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
//! Spec 120, criterion 1 - the HANDBOOK STATES THE RULE. `docs/handbook/authoring-loops.md` states
//! what rigger prescribes in two sentences, and `docs/handbook/agentic-sdlc.md` states review as a
//! sequence of steps of three kinds. Each stands under its own heading and is read by words, so a
//! rewrap never moves one. The page's five-step ceremony example is an operator-copyable fragment
//! too, so it is spliced under a minimal workflow and run through the production parser and
//! `Config::validate` to the five steps it declares; a page that drops the block fails that test.
//!
//! Like the sibling doc pins, this test reads the committed files (resolved from
//! `CARGO_MANIFEST_DIR`, so it does not depend on the process CWD), parses text, and touches
//! no backend symbol: the production code it runs is the workflow parser, its validation and two
//! text readers, `fenced_code_lines` and `normalize_ws`.
//! It is deliberately NOT feature-gated: it runs identically in both lanes.

mod common;

use common::fixtures::{
    agent_with_prompt, cfg_for, markdown_section as section, read_text as read, repeated_kind_steps,
};
use common::repo::repo_root;
use rigger::config_store::parse_workflow;
use rigger::spec::fenced_code_lines;
use rigger::wave::normalize_ws;
use std::path::PathBuf;

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

/// The handbook index's `Grounding` vocabulary bullet - the single line that both names
/// `**Grounding**` and points at the `rigger ground` command. This is the front-door definition
/// an operator reads (README.md links here as "read the handbook"), so it must describe what
/// `rigger ground` does *today*: the structural symbols pass, not the retired vector/semantic
/// engine. Returned lowercased. `None` if no such bullet is present.
fn readme_grounding_bullet() -> Option<String> {
    let path = repo_root().join("docs").join("handbook").join("README.md");
    read(&path)
        .lines()
        .map(str::to_lowercase)
        .find(|line| line.contains("**grounding**") && line.contains("`rigger ground`"))
}

#[test]
fn handbook_index_defines_grounding_as_structural_not_semantic() {
    let bullet = readme_grounding_bullet().unwrap_or_else(|| {
        panic!(
            "docs/handbook/README.md must carry a `**Grounding**` vocabulary bullet naming \
             `rigger ground` (spec 57, criterion 4)"
        )
    });

    // The retired vector engine did "semantic search over the code"; the shipping default is the
    // deterministic `symbols` structural pass. A round-4 recheck REJECTed this exact bullet for
    // still reading "semantic search over the code (`rigger ground`)" - a stale retired-grounder
    // claim the config-form guard above is structurally blind to (it matches directives, not
    // prose). This pin guards the prose form so the recurrence cannot slip a fifth time.
    assert!(
        !bullet.contains("semantic search"),
        "docs/handbook/README.md's Grounding bullet must not call `rigger ground` \"semantic \
         search\" (spec 57, criterion 4): the vector/semantic engine was retired; `rigger ground` \
         now runs the deterministic STRUCTURAL `symbols` grounder. Mirror the sibling \
         docs/handbook/tools-and-context.md wording. Found: {bullet:?}"
    );
    assert!(
        bullet.contains("structural"),
        "docs/handbook/README.md's Grounding bullet must name the STRUCTURAL pass `rigger ground` \
         actually runs post spec-57 (mirror docs/handbook/tools-and-context.md). Found: {bullet:?}"
    );
}

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

/// The text of the handbook page `page`.
fn handbook_page(page: &str) -> String {
    read(&repo_root().join("docs").join("handbook").join(page))
}

/// The text `heading` opens on `page`, up to the next `## ` or `### ` heading.
fn under_heading<'a>(page: &'a str, heading: &str) -> &'a str {
    let whole = section(page, heading);
    let next = whole[heading.len()..].find("\n### ");
    next.map_or(whole, |at| &whole[..heading.len() + at])
}

/// The first fenced block of `text`, held to be a `yaml` block, without its fences.
fn yaml_block(text: &str) -> String {
    let mut fenced = text
        .lines()
        .zip(fenced_code_lines(text))
        .skip_while(|(_, inside)| !inside)
        .take_while(|(_, inside)| *inside)
        .map(|(line, _)| line);
    assert_eq!(
        fenced.next(),
        Some("```yaml"),
        "expected a fenced yaml block in:\n{text}"
    );
    let mut body: Vec<&str> = fenced.collect();
    body.pop();
    body.join("\n")
}

/// The heading of the agnostic rule's section of `authoring-loops.md`.
const WHAT_RIGGER_PRESCRIBES: &str = "## What rigger prescribes";

/// Each page stating the rule, with the heading it stands under and its sentences.
const STATED_RULES: [(&str, &str, &[&str]); 2] = [
    (
        "authoring-loops.md",
        WHAT_RIGGER_PRESCRIBES,
        &[
            "Rigger is language and process agnostic: the binary provides tools to agents and \
             executes the workflow definition, and it prescribes nothing about how work is done \
             except by enforcing that definition.",
            "Prompts are guidance and logic is execution: the built-in instruction layer is best \
             practice a consumer may replace or switch off, never a procedure standing in for a \
             gate and never specific to a language.",
        ],
    ),
    (
        "agentic-sdlc.md",
        "### Code review: a sequence of steps, adversarial by construction",
        &[
            "A workflow declares review as a sequence of steps of three kinds: lenses, an \
             adversary and an adjudicator.",
            "The default sequence is the three steps below, one of each kind in that order, and \
             a consumer may extend it.",
        ],
    ),
];

/// The minimal workflow the ceremony example is spliced into: one stage, run by an agent the
/// example's panel also names.
const ONE_STAGE: &str = "stages:\n  s:\n    agent: a\n";

/// The adjudicator's persona: the verdict line a gating role's output ends with.
const ADJUDICATOR_BODY: &str = "End your output with the line {\"verdict\":\"approve\"}.";

#[test]
fn each_page_states_the_rule_under_its_heading_by_words() {
    for (page, heading, sentences) in STATED_RULES {
        let text = handbook_page(page);
        let held = normalize_ws(under_heading(&text, heading));
        for sentence in sentences {
            assert_eq!(
                held.matches(sentence).count(),
                1,
                "{page} must state {sentence:?} once under {heading:?}, whose text reads:\n{held}"
            );
        }
    }
}

#[test]
fn the_review_ceremony_example_validates_to_its_five_steps() {
    let text = handbook_page("authoring-loops.md");
    let prescribes = section(&text, WHAT_RIGGER_PRESCRIBES);
    let example = yaml_block(under_heading(prescribes, "### The review ceremony"));
    let workflow = format!("{example}\n{ONE_STAGE}");

    let mut cfg = cfg_for(&["a", "b", "c", "x"]);
    cfg.agents
        .insert("z".to_string(), agent_with_prompt("z", ADJUDICATOR_BODY));
    cfg.workflow = parse_workflow(&workflow).unwrap_or_else(|e| panic!("parse {workflow}: {e}"));
    if let Err(e) = cfg.validate() {
        panic!("validate {workflow}: {e}");
    }

    assert_eq!(
        cfg.workflow.defaults.review.steps(),
        repeated_kind_steps("c"),
        "the example declares lenses [a, b], adversary x, lenses [c], adversary x, adjudicator z, \
         in that order: {workflow}"
    );
}
