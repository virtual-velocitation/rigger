//! The worktree overlay's equivalence (addendum section 13): the code store's view of a commit
//! with a worktree's uncommitted changes overlaid answers exactly what the store advanced to a
//! commit of that worktree's content answers, under every tests filter, whole and around every
//! node, but for the groupings, which stay the stamp's.

use crate::common;
use common::fixtures::{git_commit_all, init_repo, write_files, write_text, RESOLVER_TREE};
use rigger::code_facts::{FileFacts, ModuleDecl, Stamp};
use rigger::code_store::CodeStore;
use rigger::code_view::{
    CodeView, Compared, DefRef, FileRef, Groupings, MentionRef, OpenCodeView, Settled, Tests,
    ViewFilter,
};
use rigger::composed::{Composed, Honesty, Indexed};
use rigger::contextgraph::{ContextGraph, Graph};
use rigger_store::code::Mode;
use std::borrow::Cow;
use std::path::Path;

/// Each edit the worktree makes to the resolver's tree, `(path, the text replaced, its
/// replacement)`, with the rows of the resolver's table it moves.
const EDITS: &[(&str, &str, &str)] = &[
    // Test-ness flips another file: `util` made test-only takes `src/util.rs` into the test-module
    // closure (`compute` and `draw` lose their product candidates there, and `computes` no longer
    // proves `compute`), and `lib_tests` made product takes `src/lib_tests/more.rs` out of it
    // (`more_origin` turns product, so its call no longer proves `origin`).
    (
        "src/lib.rs",
        "mod util;\n#[cfg(test)]\nmod lib_tests;\n",
        "#[cfg(test)]\nmod util;\nmod lib_tests;\n",
    ),
    // A duplicate id loses a twin, and a second product `helper` makes the nested module's call a
    // frontier where it had one product candidate.
    (
        "src/shapes.rs",
        "#[cfg(not(unix))]\npub fn origin() -> u32 {\n    1\n}\n\n",
        "",
    ),
    (
        "src/shapes.rs",
        "pub fn draw() {}\n",
        "pub fn draw() {}\n\npub fn helper() -> u32 {\n    3\n}\n",
    ),
    // A Test-scope file gains a proof.
    (
        "tests/integration.rs",
        "    fixture::draw();\n",
        "    fixture::draw();\n    fixture::compute();\n",
    ),
    // A design link moves to another file, and a workflow relation to another agent.
    (
        "docs/adr/0001-shapes.md",
        "binds `src/util.rs`",
        "binds `src/shapes.rs`",
    ),
    (".rigger/workflow.yml", "agent: worker", "agent: builder"),
];

/// The file the worktree adds: a definition of `vanish`, the name no definition held, so its call
/// gains a candidate.
const ADDED: (&str, &str) = ("src/added.rs", "pub fn vanish() {}\n");

/// The file the worktree deletes: a module declaration's only target, so `right`'s `mod leaf;`
/// reaches no file.
const DELETED: &str = "src/twins/right/leaf.rs";

/// How many files the worktree takes over the commit: the five it edits, the one it adds and
/// the one it deletes.
const DIFFERING: usize = 7;

/// GIVEN a repository at the resolver's tree with its code store advanced there, and a worktree
/// that edits, adds and deletes files over it, flipping another file's test-ness and proofs and
/// moving a row of every rule of the resolver's table, WHEN the head is read for the worktree and
/// then for a commit of the worktree's content, THEN the first is overlaid with the seven files
/// and the second describes the commit, and the two answer the same whole graph and the same
/// neighbourhood one and two hops around every node, under each tests filter, groupings
/// excepted; and the worktree's changes moved the answer from the first commit's.
#[test]
fn the_overlaid_worktree_answers_what_a_commit_of_its_content_answers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    write_text(root, ".gitignore", "/.rigger/store/\n");
    write_files(root, RESOLVER_TREE);
    git_commit_all(root, "the resolver's tree");
    let store = CodeStore::in_repository(root);
    store.open(Mode::Write).expect("the store advances");
    let before = answers(&store, root, Compared::Overlaid(0));

    for (path, from, to) in EDITS {
        let text = std::fs::read_to_string(root.join(path)).unwrap();
        assert_eq!(text.matches(from).count(), 1, "{path}: {from:?}");
        write_text(root, path, &text.replacen(from, to, 1));
    }
    write_text(root, ADDED.0, ADDED.1);
    std::fs::remove_file(root.join(DELETED)).unwrap();
    let overlaid = answers(&store, root, Compared::Overlaid(DIFFERING));

    git_commit_all(root, "the worktree's content");
    store.open(Mode::Write).expect("the store advances");
    let committed = answers(&store, root, Compared::Overlaid(0));

    assert_eq!(overlaid.len(), committed.len());
    for ((case, got), (_, want)) in overlaid.iter().zip(&committed) {
        assert_eq!(got, want, "{case}");
    }
    // The premise: the changes moved the answer, a test-ness flip both ways among them.
    let whole = |answers: &[(String, String)], tests: &str| {
        let case = format!("whole, tests {tests}");
        let (_, answer) = answers.iter().find(|(c, _)| *c == case).unwrap();
        answer.clone()
    };
    for (id, was, is) in [
        ("\"src/util.rs::compute\"", true, false),
        ("\"src/lib_tests/more.rs::more_origin\"", false, true),
        ("\"src/added.rs::vanish\"", false, true),
        ("\"src/twins/right/leaf.rs::right_value\"", true, false),
    ] {
        let (b, c) = (whole(&before, "exclude"), whole(&committed, "exclude"));
        assert_eq!((b.contains(id), c.contains(id)), (was, is), "{id}");
    }
}

/// Every answer the head read for `worktree` gives, as `(case, answer)`, the head describing
/// `worktree` as `compared`: the whole graph and the neighbourhood one and two hops around each
/// node, under each tests filter, groupings excepted ([`Ungrouped`]).
fn answers(store: &CodeStore, worktree: &Path, compared: Compared) -> Vec<(String, String)> {
    let head = store.head(worktree).expect("the head reads");
    assert_eq!(head.compared, compared);
    let view = Ungrouped(&*head.view);
    let records = Indexed::default();
    let honesty = || Honesty {
        worktree: worktree.display().to_string(),
        compared: head.compared,
        behind: head.behind.clone(),
    };
    let mut answers = Vec::new();
    for (name, tests) in [
        ("exclude", Tests::Exclude),
        ("include", Tests::Include),
        ("only", Tests::Only),
    ] {
        let filter = ViewFilter { tests };
        let whole = Composed::new(&records, &view, filter, honesty()).unwrap();
        let graph = whole.whole().unwrap();
        answers.push((format!("whole, tests {name}"), rendered(&graph)));
        let near = Composed::near(&records, &view, filter, honesty()).unwrap();
        let mut ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        ids.sort();
        for id in ids {
            for depth in [1, 2] {
                let seed = [id.clone()];
                let around = near.subgraph(&seed, depth).unwrap();
                let case = format!("{depth} around {id}, tests {name}");
                answers.push((case, rendered(&around)));
            }
        }
    }
    answers
}

/// `graph` as sorted lines, one per node and one per edge.
fn rendered(graph: &Graph) -> String {
    let mut lines: Vec<String> = (graph.nodes.iter().map(|n| format!("{n:?}")))
        .chain(graph.edges.iter().map(|e| format!("{e:?}")))
        .collect();
    lines.sort();
    lines.join("\n")
}

/// No grouping: the groupings stay the stamp's under the overlay (section 13).
static NO_GROUPINGS: Groupings = Groupings {
    communities: Vec::new(),
    concepts: Vec::new(),
};

/// A view answered without its groupings, every other read its own.
struct Ungrouped<'v>(&'v dyn CodeView);

impl CodeView for Ungrouped<'_> {
    fn stamp(&self) -> &Stamp {
        self.0.stamp()
    }
    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
        self.0.files()
    }
    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.0.file(path)
    }
    fn intent_facts(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.0.intent_facts(path)
    }
    fn modules(&self, path: &str) -> Option<Cow<'_, [ModuleDecl]>> {
        self.0.modules(path)
    }
    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
        self.0.defs_named(leaf)
    }
    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
        self.0.mentions_named(leaf)
    }
    fn groupings(&self) -> &Groupings {
        &NO_GROUPINGS
    }
    fn def_leaves(&self) -> Vec<String> {
        self.0.def_leaves()
    }
    fn settled(&self) -> Option<&dyn Settled> {
        self.0.settled()
    }
}
