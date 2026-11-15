//! Periphery for A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE (spec 125, criterion 8), driven
//! through the built binary over a git repository whose workflow declares the stub extractor:
//! `rigger graph build` asks the declared extractor for each committed file of its extensions -
//! the commit's text, never the worktree's - in one session started in the root that every thread
//! of the advance asks, and holds each answer as that file's code facts under the declared name, a
//! declaration named `rust` over `rs` in place of the Rust grammar's; a failed answer is said on
//! stderr as `rigger: extractor <name>: <path>: <why>`, leaves the file its identity facts and
//! closes the session, the next file starting a new one, and the file is asked again only when its
//! content or the declaration changes; a graph read in a linked worktree asks the root's
//! extractor for the worktree's text of a declared file it changed; and `rigger graph --show`
//! bounds a declared definition's body by the `end_line` the root's extractor answers for the
//! shown tree's text, in a session of its own, a failed answer leaving a note and never the
//! bundled grammar's bound.

use std::path::Path;

use crate::common;
use common::cli::{past_label, run_rigger, source_repository, write_scaffold, BuiltGraph, SOURCE};
use common::fixtures::{
    answer_defining, answer_defining_at, declared_identity, git_ok, git_out, request_for,
    root_dir_of, workflow_declaring_extractors, write_files, StubExtractor,
};
use common::git::linked_worktree;
use rigger_domain::code_facts::{Def, FileFacts};
use rigger_domain::code_view::{CodeView, Entry, Extract};
use rigger_grounder::facts::FactsExtractor;

/// The answer to `src/c.zig` whose two defs each name the other their parent.
const CYCLE: &str = concat!(
    r#"{"path":"src/c.zig","defs":["#,
    r#"{"kind":"Function","name":"f","line":1,"end_line":1,"path":"f","parent":"g"},"#,
    r#"{"kind":"Function","name":"g","line":1,"end_line":1,"path":"g","parent":"f"}"#,
    r#"],"refs":[]}"#
);

/// Writes the workflow at `root` declaring `stub` as the extractor `name` of the flow sequence
/// `extensions`, left uncommitted as an operator's working declaration is read.
fn declare(root: &Path, stub: &StubExtractor, name: &str, extensions: &str) {
    write_scaffold(
        root,
        &[],
        &workflow_declaring_extractors(&stub.declaring(name, extensions)),
    );
}

/// Writes `files` at `root` and commits them by their paths alone, so the store files a build
/// left beside them stay out of the commit.
fn commit(root: &Path, files: &[(&str, &str)]) {
    write_files(root, files);
    let mut add = vec!["add", "--"];
    add.extend(files.iter().map(|(path, _)| *path));
    git_ok(root, &add);
    git_ok(root, &["commit", "-q", "-m", "files"]);
}

/// The id of the blob the head commit holds at `path`.
fn committed_blob(root: &Path, path: &str) -> String {
    git_out(root, &["rev-parse", &format!("HEAD:{path}")])
}

/// The facts the head commit's `path` holds under the declared extractor `lang` when its answer
/// gave the file-level functions `leaves`, each on the first line, or failed and gave none.
fn declared_facts(root: &Path, path: &str, lang: &str, leaves: &[&str]) -> FileFacts {
    FileFacts {
        defs: leaves
            .iter()
            .map(|leaf| Def {
                local: leaf.to_string(),
                leaf: leaf.to_string(),
                kind: "function".into(),
                line: 1,
                end_line: 1,
                parent: None,
                test: false,
            })
            .collect(),
        ..declared_identity(path, &committed_blob(root, path), lang)
    }
}

/// Asserts the code store the last graph build at `root` advanced holds `expected`, each file's
/// facts under its path.
fn assert_store_holds(root: &Path, expected: &[FileFacts], what: &str) {
    let view = BuiltGraph::of(root).view;
    let held: Vec<Option<FileFacts>> = expected
        .iter()
        .map(|facts| view.file(&facts.path).map(|held| held.into_owned()))
        .collect();
    assert_eq!(
        held,
        expected.iter().cloned().map(Some).collect::<Vec<_>>(),
        "{what}: the facts the store holds"
    );
}

/// `lines` in order.
fn sorted(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines
}

/// `n` lines the stub writes when its input ends.
fn ended(n: usize) -> Vec<String> {
    vec!["ended".to_string(); n]
}

/// Given a committed repository whose workflow declares the stub for some extensions, and a
/// declared file changed in the worktree since, when the operator builds the graph, each committed
/// declared file holds the stub's answer as its code facts under the declared name and the commit's
/// blob - a declaration named `rust` over `rs` in place of the Rust grammar's - and every other
/// file the bundled grammars' facts; the stub started once, in the root, read the commit's text of
/// each declared file once, every thread of the advance asking that one session, and its input
/// ended when the build was done; nothing is said on stderr.
#[test]
fn graph_build_grounds_each_committed_declared_file_from_one_session() {
    let many: Vec<(String, String, String)> = (0..12)
        .map(|i| format!("m{i:02}"))
        .map(|leaf| (format!("src/{leaf}.zig"), format!("fn {leaf}\n"), leaf))
        .chain([("src/z.zag".into(), "fn z\n".into(), "z".into())])
        .collect();
    let many: Vec<(&str, &str, &str)> = many
        .iter()
        .map(|(path, text, leaf)| (path.as_str(), text.as_str(), leaf.as_str()))
        .collect();
    // (the declared name, its extensions, each declared file with its committed text and the leaf
    // its answer gives, each other file with its committed text)
    type Row<'a> = (
        &'a str,
        &'a str,
        Vec<(&'a str, &'a str, &'a str)>,
        Vec<(&'a str, &'a str)>,
    );
    let rows: [Row; 2] = [
        ("zig", "[zig, zag]", many, vec![SOURCE]),
        (
            "rust",
            "[rs]",
            vec![
                (SOURCE.0, SOURCE.1, "declared"),
                ("src/more.rs", "fn more() {}\n", "answered"),
            ],
            vec![("tool/run.py", "def held():\n    pass\n")],
        ),
    ];
    for (name, extensions, declared, undeclared) in rows {
        let (_dir, root) = source_repository();
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        declare(&root, &stub, name, extensions);
        let files: Vec<(&str, &str)> = declared
            .iter()
            .map(|(path, text, _)| (*path, *text))
            .chain(undeclared.iter().copied())
            .collect();
        commit(&root, &files);
        for (path, _, leaf) in &declared {
            stub.answers(path, &answer_defining(path, leaf));
        }
        std::fs::write(root.join(declared[0].0), "changed\n").expect("change a declared file");
        let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
        assert_eq!(
            (ok, err.as_str()),
            (true, ""),
            "{name}: graph build succeeds saying nothing on stderr; stdout: {out}"
        );
        let expected: Vec<FileFacts> = declared
            .iter()
            .map(|(path, _, leaf)| declared_facts(&root, path, name, &[leaf]))
            .chain(undeclared.iter().map(|(path, text)| {
                let entry = Entry {
                    path: path.to_string(),
                    blob: committed_blob(&root, path),
                };
                FactsExtractor.extract(&entry, text.as_bytes())
            }))
            .collect();
        assert_store_holds(&root, &expected, name);
        assert_eq!(
            (stub.starts(), sorted(stub.requests()), stub.ends()),
            (
                vec![root.display().to_string()],
                sorted(
                    declared
                        .iter()
                        .map(|(path, text, _)| request_for(path, text))
                        .collect()
                ),
                ended(1),
            ),
            "{name}: one session, in the root, asked each committed declared file's text once and \
             closed after the build"
        );
    }
}

/// Given a committed repository declaring the stub for `zig`, when the operator builds the graph,
/// each file the stub answers with an error, with another file's path, with two defs each naming
/// the other its parent, or not at all is said on stderr with why, holds its identity facts and no
/// definitions, and closes the session, so the next file starts a new one in the root, while the
/// file the stub answers well holds its definition. Built again, nothing changed asks nothing; a
/// committed change asks that file alone; and a changed declaration asks every file - each of them
/// in one session, saying nothing on stderr.
#[test]
fn a_failed_answer_is_said_and_asked_again_only_when_its_file_or_the_declaration_changes() {
    let (_dir, root) = source_repository();
    let kept = tempfile::tempdir().expect("tempdir");
    let stub = StubExtractor::new(kept.path());
    declare(&root, &stub, "zig", "[zig]");
    let root_dir = root.display().to_string();
    // (the file, its committed text, the stub's first answer, why that answer fails)
    let first: [(&str, &str, Option<String>, Option<&str>); 5] = [
        (
            "src/a.zig",
            "a\n",
            Some(r#"{"path":"src/a.zig","error":"no parser"}"#.into()),
            Some("the answer is an error: no parser"),
        ),
        (
            "src/b.zig",
            "b\n",
            Some(answer_defining("src/c.zig", "b")),
            Some(r#"the answer is for "src/c.zig""#),
        ),
        (
            "src/c.zig",
            "c\n",
            Some(CYCLE.into()),
            Some(r#"the defs' parents form a cycle through "f""#),
        ),
        ("src/d.zig", "d\n", None, Some("end of output")),
        (
            "src/e.zig",
            "e\n",
            Some(answer_defining("src/e.zig", "e")),
            None,
        ),
    ];
    commit(
        &root,
        &first.each_ref().map(|(path, text, ..)| (*path, *text)),
    );
    for (path, _, answer, _) in &first {
        if let Some(answer) = answer {
            stub.answers(path, answer);
        }
    }
    let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
    assert_eq!(
        (ok, sorted(err.lines().map(str::to_string).collect())),
        (
            true,
            sorted(
                first
                    .iter()
                    .filter_map(|(path, _, _, why)| {
                        why.map(|why| format!("rigger: extractor zig: {path}: {why}"))
                    })
                    .collect()
            )
        ),
        "graph build succeeds saying each failed answer once on stderr; stdout: {out}"
    );
    assert_store_holds(
        &root,
        &first.each_ref().map(|(path, _, _, why)| {
            let leaves: &[&str] = if why.is_some() { &[] } else { &["e"] };
            declared_facts(&root, path, "zig", leaves)
        }),
        "the first build",
    );
    let failed: Vec<String> = first
        .iter()
        .filter(|(.., why)| why.is_some())
        .map(|(path, text, ..)| request_for(path, text))
        .collect();
    let requests = stub.requests();
    let restarts = requests[..requests.len() - 1]
        .iter()
        .filter(|line| failed.contains(line))
        .count();
    assert_eq!(
        (stub.starts(), sorted(requests.clone()), stub.ends()),
        (
            vec![root_dir.clone(); 1 + restarts],
            sorted(
                first
                    .iter()
                    .map(|(path, text, ..)| request_for(path, text))
                    .collect()
            ),
            ended(restarts),
        ),
        "a session for the first file and one after each failed file but the last asked, each \
         closed but the one whose stub ended its own output; requests in order: {requests:?}"
    );

    let good = |path: &str, leaf: &str| (path.to_string(), answer_defining(path, leaf));
    // (what changed, the files committed, the answers the stub gives, the extensions declared, the
    // requests the build asks, the leaves src/a.zig to src/e.zig then hold)
    type Rebuild<'a> = (
        &'a str,
        &'a [(&'a str, &'a str)],
        Vec<(String, String)>,
        &'a str,
        Vec<String>,
        [&'a [&'a str]; 5],
    );
    let rebuilds: [Rebuild; 3] = [
        (
            "nothing",
            &[],
            vec![],
            "[zig]",
            vec![],
            [&[], &[], &[], &[], &["e"]],
        ),
        (
            "src/a.zig's committed text",
            &[("src/a.zig", "a2\n")],
            vec![good("src/a.zig", "a")],
            "[zig]",
            vec![request_for("src/a.zig", "a2\n")],
            [&["a"], &[], &[], &[], &["e"]],
        ),
        (
            "the declaration",
            &[],
            vec![
                good("src/b.zig", "b"),
                good("src/c.zig", "c"),
                good("src/d.zig", "d"),
            ],
            "[zig, zag]",
            [
                ("src/a.zig", "a2\n"),
                ("src/b.zig", "b\n"),
                ("src/c.zig", "c\n"),
                ("src/d.zig", "d\n"),
                ("src/e.zig", "e\n"),
            ]
            .map(|(path, text)| request_for(path, text))
            .to_vec(),
            [&["a"], &["b"], &["c"], &["d"], &["e"]],
        ),
    ];
    for (changed, commits, answers, extensions, asked, leaves) in rebuilds {
        if !commits.is_empty() {
            commit(&root, commits);
        }
        for (path, answer) in &answers {
            stub.answers(path, answer);
        }
        declare(&root, &stub, "zig", extensions);
        let (starts, requests, ends) = (stub.starts(), stub.requests(), stub.ends());
        let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
        assert_eq!(
            (ok, err.as_str()),
            (true, ""),
            "{changed} changed: graph build succeeds saying nothing on stderr; stdout: {out}"
        );
        let expected: Vec<FileFacts> = first
            .iter()
            .zip(leaves)
            .map(|((path, ..), leaves)| declared_facts(&root, path, "zig", leaves))
            .collect();
        assert_store_holds(&root, &expected, changed);
        let session = usize::from(!asked.is_empty());
        assert_eq!(
            (
                stub.starts(),
                sorted(stub.requests()[requests.len()..].to_vec()),
                stub.ends()
            ),
            (
                vec![root_dir.clone(); starts.len() + session],
                sorted(asked),
                ended(ends.len() + session),
            ),
            "{changed} changed: the build asks only the files it changed, in one session"
        );
    }
}

/// Given a repository built under a declaration and a linked worktree of it, when a declared file
/// is changed in the worktree and the operator looks a name up there, the read asks the root's
/// extractor - started in the root, not the worktree - for the worktree's text of that file alone
/// and answers the definition the stub gives it, saying nothing on stderr, while the root's store
/// keeps the commit's facts.
#[test]
fn a_read_in_a_linked_worktree_asks_the_roots_extractor_for_the_text_it_changed() {
    let (_dir, root) = source_repository();
    let kept = tempfile::tempdir().expect("tempdir");
    let stub = StubExtractor::new(kept.path());
    declare(&root, &stub, "zig", "[zig]");
    commit(&root, &[("src/a.zig", "a\n")]);
    stub.answers("src/a.zig", &answer_defining("src/a.zig", "kept"));
    let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
    assert_eq!(
        (ok, err.as_str()),
        (true, ""),
        "graph build succeeds saying nothing on stderr; stdout: {out}"
    );
    let trees = tempfile::tempdir().expect("tempdir");
    let worktree = linked_worktree(
        &root,
        trees.path().canonicalize().expect("canonical").join("wt"),
    );
    std::fs::write(worktree.join("src/a.zig"), "fresh\n").expect("change a declared file");
    stub.answers("src/a.zig", &answer_defining("src/a.zig", "fresh"));
    let mut requests = stub.requests();
    let (out, err, ok) = run_rigger(&worktree, &["graph", "--lookup", "fresh"]);
    let hits: Vec<&str> = out
        .split_once("### fresh\n")
        .map(|(_, rest)| rest.lines().take_while(|line| !line.is_empty()).collect())
        .unwrap_or_default();
    assert_eq!(
        (ok, err.as_str(), hits),
        (
            true,
            "",
            vec![
                "exact match: 1 entity",
                "- src/a.zig:1 src/a.zig::fresh (function)"
            ]
        ),
        "the lookup in the worktree answers the stub's definition of its text:\n{out}"
    );
    requests.push(request_for("src/a.zig", "fresh\n"));
    assert_eq!(
        (stub.starts(), stub.requests(), stub.ends()),
        (vec![root.display().to_string(); 2], requests, ended(2)),
        "the read started the root's extractor in the root and asked it the worktree's text alone"
    );
    assert_store_holds(
        &root,
        &[declared_facts(&root, "src/a.zig", "zig", &["kept"])],
        "the root's store after the read",
    );
}

/// A Zig file holding `main` over its lines 2 to 4, between two other lines.
const MAIN_ZIG: &str = "const a = 1;\npub fn main() void {\n    a;\n}\nconst b = 2;\n";

/// A Rust file holding `two`, whose function item the Rust grammar ends on line 3, then a comment.
const TWO_RS: &str = "pub fn two() {\n    1;\n}\n// after two\n";

/// A Rust file holding `three`, whose function item the Rust grammar ends on line 2, then a
/// comment.
const THREE_RS: &str = "pub fn three() {\n}\n// after three\n";

/// The text a linked worktree gives `src/a.zig`: `fresh` over its lines 3 to 5, between other
/// lines.
const FRESH_ZIG: &str = "const a = 1;\n\npub fn fresh() void {\n    a;\n}\nconst b = 2;\n";

/// One `rigger graph --show` case over a repository whose workflow declares the stub.
struct ShowCase<'a> {
    /// The declared extractor's name and its extensions' flow sequence.
    declared: (&'a str, &'a str),
    /// The files committed beside [`SOURCE`].
    committed: &'a [(&'a str, &'a str)],
    /// The answers the stub gives while `graph build` runs, by path.
    built: Vec<(&'a str, String)>,
    /// The file a linked worktree changes, with its text there, the show then run in that
    /// worktree; `None` runs it in the root.
    worktree: Option<(&'a str, &'a str)>,
    /// The answers the stub gives while the show runs, by path.
    shown: Vec<(&'a str, String)>,
    /// The id shown.
    query: &'a str,
    /// Every line the show prints past its honesty label.
    out: &'a [&'a str],
    /// What the show says on stderr.
    err: &'a str,
    /// The request each session the show starts reads, one per session.
    asked: Vec<String>,
}

/// Given a committed repository whose workflow declares the stub and a graph built under it, when
/// the operator shows a definition, a file of a declared extension prints the lines from the
/// definition's line through the `end_line` the stub answers for the shown tree's text - for an
/// extension no grammar covers, and for `rs` under a declaration named `rust`, which replaces the
/// bundled grammar's bound - the show starting the stub in the root, asking it that text and
/// closing it; a file no declaration lists is bounded by its bundled grammar, the stub never
/// started though it holds a wider answer for it; an answer that fails prints the
/// missing-definition note in place of the body, never the grammar's, and says why on stderr; and
/// in a linked worktree, which holds no declaration of its own, the body is the worktree's text,
/// the read and the show each asking the root's extractor in the root.
#[test]
fn graph_show_bounds_a_declared_definitions_body_by_its_extractors_end_line() {
    let two_answer = answer_defining_at("src/two.rs", "two", 1, 4);
    let cases = [
        ShowCase {
            declared: ("zig", "[zig]"),
            committed: &[("src/a.zig", MAIN_ZIG)],
            built: vec![("src/a.zig", answer_defining_at("src/a.zig", "main", 2, 4))],
            worktree: None,
            shown: vec![],
            query: "src/a.zig::main",
            out: &[
                "show src/a.zig::main",
                "  site: src/a.zig:2   kind function   degree 2",
                "  name main",
                "  lang zig",
                "       2 | pub fn main() void {",
                "       3 |     a;",
                "       4 | }",
            ],
            err: "",
            asked: vec![request_for("src/a.zig", MAIN_ZIG)],
        },
        ShowCase {
            declared: ("rust", "[rs]"),
            committed: &[("src/two.rs", TWO_RS)],
            built: vec![
                (SOURCE.0, answer_defining(SOURCE.0, "product")),
                ("src/two.rs", two_answer.clone()),
            ],
            worktree: None,
            shown: vec![],
            query: "src/two.rs::two",
            out: &[
                "show src/two.rs::two",
                "  site: src/two.rs:1   kind function   degree 2",
                "  name two",
                "  lang rust",
                "       1 | pub fn two() {",
                "       2 |     1;",
                "       3 | }",
                "       4 | // after two",
            ],
            err: "",
            asked: vec![request_for("src/two.rs", TWO_RS)],
        },
        ShowCase {
            declared: ("zig", "[zig]"),
            committed: &[("src/three.rs", THREE_RS)],
            built: vec![(
                "src/three.rs",
                answer_defining_at("src/three.rs", "three", 1, 3),
            )],
            worktree: None,
            shown: vec![],
            query: "src/three.rs::three",
            out: &[
                "show src/three.rs::three",
                "  site: src/three.rs:1   kind function   degree 2",
                "  name three",
                "  lang rust",
                "       1 | pub fn three() {",
                "       2 | }",
            ],
            err: "",
            asked: vec![],
        },
        ShowCase {
            declared: ("rust", "[rs]"),
            committed: &[("src/two.rs", TWO_RS)],
            built: vec![
                (SOURCE.0, answer_defining(SOURCE.0, "product")),
                ("src/two.rs", two_answer),
            ],
            worktree: None,
            shown: vec![(
                "src/two.rs",
                r#"{"path":"src/two.rs","error":"no parser"}"#.to_string(),
            )],
            query: "src/two.rs::two",
            out: &[
                "show src/two.rs::two",
                "  site: src/two.rs:1   kind function   degree 2",
                "  name two",
                "  lang rust",
                r#"  (no production definition named "two" at path "two" of kind function at line 1 of src/two.rs)"#,
            ],
            err: "rigger: extractor rust: src/two.rs: the answer is an error: no parser\n",
            asked: vec![request_for("src/two.rs", TWO_RS)],
        },
        ShowCase {
            declared: ("zig", "[zig]"),
            committed: &[("src/a.zig", "fn kept\n")],
            built: vec![("src/a.zig", answer_defining("src/a.zig", "kept"))],
            worktree: Some(("src/a.zig", FRESH_ZIG)),
            shown: vec![("src/a.zig", answer_defining_at("src/a.zig", "fresh", 3, 5))],
            query: "src/a.zig::fresh",
            // The groupings describe the commit, which holds no `fresh`, so its one edge is its
            // file's.
            out: &[
                "show src/a.zig::fresh",
                "  site: src/a.zig:3   kind function   degree 1",
                "  name fresh",
                "  lang zig",
                "       3 | pub fn fresh() void {",
                "       4 |     a;",
                "       5 | }",
            ],
            err: "",
            asked: vec![request_for("src/a.zig", FRESH_ZIG); 2],
        },
    ];
    for case in cases {
        let (_dir, root) = source_repository();
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let (name, extensions) = case.declared;
        declare(&root, &stub, name, extensions);
        commit(&root, case.committed);
        for (path, answer) in &case.built {
            stub.answers(path, answer);
        }
        let (out, err, ok) = run_rigger(&root, &["graph", "build"]);
        assert_eq!(
            (ok, err.as_str()),
            (true, ""),
            "{}: graph build succeeds saying nothing on stderr; stdout: {out}",
            case.query
        );
        let trees = tempfile::tempdir().expect("tempdir");
        let shown_in = match case.worktree {
            Some((path, text)) => {
                let worktree = linked_worktree(
                    &root,
                    trees.path().canonicalize().expect("canonical").join("wt"),
                );
                std::fs::write(worktree.join(path), text).expect("change a declared file");
                worktree
            }
            None => root.clone(),
        };
        for (path, answer) in &case.shown {
            stub.answers(path, answer);
        }
        let (starts, requests, ends) = (stub.starts(), stub.requests(), stub.ends());
        let (out, err, ok) = run_rigger(&shown_in, &["graph", "--show", case.query]);
        assert_eq!(
            (ok, past_label(&out), err.as_str()),
            (true, format!("{}\n", case.out.join("\n")), case.err),
            "{}: the show's stdout past its label and its stderr",
            case.query
        );
        let sessions = case.asked.len();
        assert_eq!(
            (
                stub.starts()[starts.len()..].to_vec(),
                stub.requests()[requests.len()..].to_vec(),
                stub.ends()[ends.len()..].to_vec()
            ),
            (
                vec![root_dir_of(&root); sessions],
                case.asked,
                ended(sessions)
            ),
            "{}: each session the show starts begins in the root, is asked once and is closed",
            case.query
        );
    }
}
