//! Periphery for THE WORKFLOW DECLARES EXTRACTORS (spec 125, criterion 7), driven through the
//! built binary: `rigger validate` accepts each well-formed `extractors:` declaration and refuses
//! each malformed one, naming `extractors.<name>`; `rigger graph build` builds the code store
//! under the fingerprint of the declaration in force - the bundled grammars' with none or with one
//! that fails the check, said on stderr, and one of its own for each declaration - so a store
//! built under one declaration is not in force under another.

use crate::common;
use common::cli::{run_rigger, temp_project, write_scaffold};
use common::fixtures::{workflow_declaring_extractors, ACCEPTED_EXTRACTORS, EXTRACTOR_REFUSALS};

/// A workflow declaring no extractor: its only key is another one.
const NO_EXTRACTORS_KEY: &str = "gates: {}\n";

/// The summary `rigger validate` prints second for a workflow declaring no agent, stage or gate.
const VALID_EMPTY_WORKFLOW: &str = "config valid: 0 agents, 0 stages, 0 gates";

/// Given a project whose workflow declares `extractors:`, when the operator validates it, each
/// well-formed declaration passes, saying on stderr what a workflow declaring none says, and each
/// malformed one fails, printing only its refusal.
#[test]
fn rigger_validate_accepts_each_well_formed_extractors_declaration_and_refuses_each_malformed_one()
{
    let project = temp_project();
    let root = project.path();
    write_scaffold(root, &[], NO_EXTRACTORS_KEY);
    let (out, undeclared, ok) = run_rigger(root, &["validate"]);
    assert_eq!(
        (ok, out.lines().nth(1)),
        (true, Some(VALID_EMPTY_WORKFLOW)),
        "a workflow declaring no extractor validates; stderr: {undeclared}"
    );
    for mapping in ACCEPTED_EXTRACTORS {
        write_scaffold(root, &[], &workflow_declaring_extractors(mapping));
        let (out, err, ok) = run_rigger(root, &["validate"]);
        assert_eq!(
            (ok, out.lines().nth(1), err.as_str()),
            (true, Some(VALID_EMPTY_WORKFLOW), undeclared.as_str()),
            "{mapping}"
        );
    }
    for (mapping, refusal) in EXTRACTOR_REFUSALS {
        write_scaffold(root, &[], &workflow_declaring_extractors(mapping));
        let (out, err, ok) = run_rigger(root, &["validate"]);
        assert_eq!(
            (ok, out.as_str(), err.as_str()),
            (false, "", format!("rigger: config: {refusal}\n").as_str()),
            "{mapping}"
        );
    }
}

#[cfg(feature = "symbols")]
use common::cli::{repository_code_store, run_rigger_ok, source_repository};
#[cfg(feature = "symbols")]
use common::fixtures::{git_ok, git_out};
#[cfg(feature = "symbols")]
use rigger_domain::code_view::CodeView;
#[cfg(feature = "symbols")]
use rigger_domain::groupings::ProductionGroupings;
#[cfg(feature = "symbols")]
use rigger_grounder::facts::{FactsExtractor, ProjectExtractor};
#[cfg(feature = "symbols")]
use rigger_store::code::{fingerprint, Store};
#[cfg(feature = "symbols")]
use std::collections::BTreeSet;

/// The workflow's path in a repository, as git names it.
#[cfg(feature = "symbols")]
const WORKFLOW: &str = ".rigger/workflow.yml";

/// Two well-formed declarations differing only in the command one extractor runs.
#[cfg(feature = "symbols")]
const DECLARATIONS: [&str; 2] = [
    ACCEPTED_EXTRACTORS[1],
    "{ zig: { extensions: [zig], run: zig-facts } }",
];

/// Given a committed repository, when the operator commits a workflow declaring no extractor, or
/// one that fails the check, and builds the graph, the build advances the store under the bundled
/// grammars' fingerprint to the new head - the store a build before any declaration made - and
/// says on stderr why a declaration that fails is not in force.
#[cfg(feature = "symbols")]
#[test]
fn with_no_declaration_or_a_failing_one_graph_build_advances_the_bundled_grammars_store() {
    let (_dir, root) = source_repository();
    let bundled = fingerprint(&FactsExtractor, &ProductionGroupings);
    let undeclared = [
        (None, None),
        (Some(NO_EXTRACTORS_KEY.to_string()), None),
        (Some(workflow_declaring_extractors("{}")), None),
        (
            Some("nonsense: 1\n".to_string()),
            Some("parse workflow: nonsense: unknown key"),
        ),
    ];
    let refused = EXTRACTOR_REFUSALS
        .map(|(mapping, refusal)| (Some(workflow_declaring_extractors(mapping)), Some(refusal)));
    for (workflow, why) in undeclared.into_iter().chain(refused) {
        if let Some(workflow) = &workflow {
            write_scaffold(&root, &[], workflow);
            git_ok(&root, &["add", WORKFLOW]);
            git_ok(&root, &["commit", "-q", "-m", workflow.as_str()]);
        }
        let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
        assert!(
            ok,
            "{workflow:?}: graph build succeeds; stdout: {out}; stderr: {err}"
        );
        assert_eq!(
            err.lines().collect::<BTreeSet<_>>(),
            why.map(|why| format!("rigger: extractors: {why}"))
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            "{workflow:?}: every line graph build says on stderr"
        );
        let head = git_out(&root, &["rev-parse", "HEAD"]);
        let advanced = Store::new(&root)
            .current(bundled)
            .expect("the store reads")
            .map(|view| view.stamp().clone());
        assert_eq!(
            advanced.as_ref().map(|stamp| stamp.tip.commit.as_deref()),
            Some(Some(head.as_str())),
            "{workflow:?}: the bundled grammars' store describes the head"
        );
        assert_eq!(
            repository_code_store(&root)
                .in_force()
                .expect("the store reads"),
            advanced,
            "{workflow:?}: the store in force is the bundled grammars'"
        );
    }
}

/// Given a repository whose store was built with no declaration, when the operator declares one
/// set of extractors, then another, then the first again, each new declaration finds no store in
/// force until a graph build makes one under its own fingerprint - the project extractor's,
/// different from the bundled grammars' and from the other declaration's - and the first
/// declaration, read again, finds its own store in force.
#[cfg(feature = "symbols")]
#[test]
fn a_store_built_under_one_declaration_is_not_in_force_under_another() {
    let (_dir, root) = source_repository();
    let store = repository_code_store(&root);
    run_rigger_ok(&root, &["graph", "build"]);
    let undeclared = store
        .in_force()
        .expect("the store reads")
        .expect("graph build made a store");
    assert_eq!(
        undeclared.fingerprint,
        fingerprint(&FactsExtractor, &ProductionGroupings),
        "with no workflow the store is the bundled grammars'"
    );
    let mut built = vec![undeclared];
    for mapping in DECLARATIONS {
        write_scaffold(&root, &[], &workflow_declaring_extractors(mapping));
        assert_eq!(
            store.in_force().expect("the store reads"),
            None,
            "{mapping}: no store is in force before a build under it"
        );
        let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
        assert_eq!(
            (ok, err.as_str()),
            (true, ""),
            "{mapping}: graph build succeeds saying nothing on stderr; stdout: {out}"
        );
        let declared = store
            .in_force()
            .expect("the store reads")
            .unwrap_or_else(|| panic!("{mapping}: graph build made a store"));
        assert_eq!(
            declared.fingerprint,
            fingerprint(&ProjectExtractor::of(&root), &ProductionGroupings),
            "{mapping}: the store is the project extractor's"
        );
        built.push(declared);
    }
    assert_eq!(
        built
            .iter()
            .map(|stamp| stamp.fingerprint)
            .collect::<BTreeSet<_>>()
            .len(),
        3,
        "no declaration, and each of two, give three fingerprints: {built:?}"
    );
    write_scaffold(&root, &[], &workflow_declaring_extractors(DECLARATIONS[0]));
    assert_eq!(
        store.in_force().expect("the store reads"),
        Some(built[1].clone()),
        "{}: read again, the declaration finds its own store in force",
        DECLARATIONS[0]
    );
}
