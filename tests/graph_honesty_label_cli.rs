//! The honesty label (6.1) heads every graph answer the CLI prints: the commit the answer
//! describes, or the caller's worktree over it when the two differ, and what it withheld when the
//! worktree cannot be compared. Under it the lookup names each entity's referrers by kind, its test
//! referrers only under `--tests include` or `only`, and marks `?` a referrer whose mention gave the entity
//! no single edge. `--show` prints a node's doc and its referrers by kind as entries of its site,
//! each referrer as the lookup prints it. A reader an uncommitted edit adds is listed by a lookup
//! from the worktree holding the edit, and by none from another worktree at the same head.

use crate::common;
use common::cli::{run_rigger, run_rigger_ok, temp_project};
use common::fixtures::{git_commit_all, git_out, write_files, write_text, READERS};
use common::git::linked_worktree;
use rigger::contextgraph::REFERRER_LABELS;

/// GIVEN a project whose source is committed and built, WHEN `rigger graph --around` reads it
/// from a clean worktree, from one with an unread file edited, and from one with the read file
/// edited, THEN the first line is the label naming the head commit and, when the worktree differs,
/// the worktree the answer describes and how many of its files it took over the commit, the
/// subgraph header follows it, and the answer holds the edited file's code as the worktree has it.
#[test]
fn around_prints_the_label_first() {
    let overlaid = "describes {w} ({head} + 1 file); groupings describe {head}";
    // (case, the file edited after the build, the label, the code node the answer holds, the one
    // it lacks)
    let cases: [(&str, Option<&str>, &str, &str, &str); 3] = [
        (
            "a clean worktree",
            None,
            "describes {head}",
            "src/lib.rs::alpha",
            "src/lib.rs::gamma",
        ),
        (
            "an edited file the answer does not reach",
            Some("src/other.rs"),
            overlaid,
            "src/lib.rs::alpha",
            "src/lib.rs::gamma",
        ),
        (
            "the edited file the answer reads",
            Some("src/lib.rs"),
            overlaid,
            "src/lib.rs::gamma",
            "src/lib.rs::alpha",
        ),
    ];
    for (case, edited, label, holds, lacks) in cases {
        let dir = temp_project();
        let root = dir.path();
        write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
        write_text(root, "src/other.rs", "pub fn beta() {}\n");
        git_commit_all(root, "source");
        run_rigger_ok(root, &["graph", "build"]);
        if let Some(file) = edited {
            write_text(root, file, "pub fn gamma() {}\n");
        }
        let head = git_out(root, &["rev-parse", "HEAD"]);
        let worktree = root.canonicalize().unwrap().display().to_string();
        let label = label.replace("{w}", &worktree).replace("{head}", &head);

        let (out, err, ok) = run_rigger(root, &["graph", "--around", "src/lib.rs", "--depth", "1"]);

        assert!(ok, "{case}: the around answers; stderr:\n{err}");
        let lines: Vec<&str> = out.lines().take(2).collect();
        assert_eq!(
            lines,
            [label.as_str(), "subgraph around \"src/lib.rs\" (depth 1):"],
            "{case}: the label heads the answer; stdout:\n{out}"
        );
        assert!(out.contains(holds), "{case}: holds {holds}; stdout:\n{out}");
        assert!(
            !out.contains(lacks),
            "{case}: lacks {lacks}; stdout:\n{out}"
        );
    }
}

/// GIVEN a committed, built project whose worktree index cannot be read, WHEN `rigger graph`
/// answers `--around` the file, `--show` its function and `--lookup` the function's name, THEN
/// each answer carries the label saying the worktree cannot be compared and counting the code
/// nodes it withheld, no code node is answered, and the lookup's text half still answers.
#[test]
fn an_uncomparable_worktree_withholds_every_code_hit() {
    // (read, the arguments, the withheld count, what the answer says in place of the code)
    let cases: [(&str, &[&str], usize, &str); 3] = [
        (
            "around the file",
            &["graph", "--around", "src/lib.rs", "--depth", "1"],
            2,
            "subgraph around \"src/lib.rs\" (depth 1):",
        ),
        (
            "show the function",
            &["graph", "--show", "alpha"],
            1,
            "show \"alpha\": no such entity",
        ),
        (
            "lookup the function's name",
            &["graph", "--lookup", "alpha"],
            1,
            "src/lib.rs:1: pub fn alpha() {}",
        ),
    ];
    for (case, args, withheld, says) in cases {
        let dir = temp_project();
        let root = dir.path();
        write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
        git_commit_all(root, "source");
        run_rigger_ok(root, &["graph", "build"]);
        std::fs::write(root.join(".git").join("index"), "unreadable").unwrap();
        let head = git_out(root, &["rev-parse", "HEAD"]);
        let worktree = root.canonicalize().unwrap().display().to_string();
        let label = format!("describes {head}; {worktree} cannot be compared; {withheld} withheld");

        let (out, err, ok) = run_rigger(root, args);

        assert!(ok, "{case}: the read answers; stderr:\n{err}");
        assert!(
            out.lines().any(|l| l == label),
            "{case}: the answer carries {label:?}; stdout:\n{out}"
        );
        assert!(out.contains(says), "{case}: says {says:?}; stdout:\n{out}");
        assert!(
            !out.contains("src/lib.rs::alpha"),
            "{case}: no code node is answered; stdout:\n{out}"
        );
    }
}

/// GIVEN a built project whose product function `alpha` has a same-named test beside it, WHEN
/// `rigger graph --lookup alpha` is read under each `--tests` setting, THEN `exclude` (the
/// default) answers the product hit and counts the test hit apart, `include` answers both and
/// marks the test, `only` answers the test alone, and a value that is none of the three is
/// refused naming the three.
#[test]
fn graph_takes_a_tests_option() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
    write_text(
        root,
        "tests/t.rs",
        "#[test]\nfn alpha() { crate_alpha(); }\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let product = "src/lib.rs:1 src/lib.rs::alpha (function)";
    let test = "tests/t.rs:2 tests/t.rs::alpha (function) [test]";
    let not_shown = "1 test hit not shown";
    // (the arguments, what the graph half holds, what it does not)
    let cases: [(&[&str], &[&str], &[&str]); 4] = [
        (
            &["graph", "--lookup", "alpha"],
            &[product, not_shown],
            &[test],
        ),
        (
            &["graph", "--lookup", "alpha", "--tests", "include"],
            &[product, test],
            &[not_shown],
        ),
        (
            &["graph", "--lookup", "alpha", "--tests", "only"],
            &[test],
            &[product, not_shown],
        ),
        (
            &["graph", "--lookup", "alpha", "--tests", "exclude"],
            &[product, not_shown],
            &[test],
        ),
    ];
    for (args, holds, lacks) in cases {
        let (out, err, ok) = run_rigger(root, args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        for line in holds {
            assert!(out.contains(line), "{args:?} holds {line:?}:\n{out}");
        }
        for line in lacks {
            assert!(!out.contains(line), "{args:?} lacks {line:?}:\n{out}");
        }
    }
    let (_, err, ok) = run_rigger(root, &["graph", "--lookup", "alpha", "--tests", "some"]);
    assert!(!ok);
    assert!(
        err.contains("--tests: `tests` is one of exclude, include or only, not \"some\""),
        "{err}"
    );
}

/// GIVEN a built project whose product function `alpha` two tests call (one twice) and whose
/// `beta` no test calls, WHEN `rigger graph --around` and `--show` read `alpha` under each
/// `--tests` setting, THEN `exclude` (the default) says under the label that the two tests
/// proving it are not shown and which filter shows them, `include` and `only` say nothing of
/// hidden tests, and `beta`, which no test proves, says nothing either.
#[test]
fn around_and_show_count_the_hidden_tests_proving_what_they_show() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\npub fn beta() {}\n");
    write_text(
        root,
        "tests/t.rs",
        "#[test]\nfn one() { alpha(); }\n#[test]\nfn two() { alpha(); alpha(); }\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let counted = "2 test hits not shown; the tests filter (include or only) shows them";
    let around = ["graph", "--around", "src/lib.rs::alpha", "--depth", "1"];
    let show = ["graph", "--show", "src/lib.rs::alpha"];
    // (the arguments, the line under the label when one counts hidden tests)
    let cases: [(Vec<&str>, Option<&str>); 7] = [
        (around.to_vec(), Some(counted)),
        (
            [&around[..], &["--tests", "exclude"]].concat(),
            Some(counted),
        ),
        ([&around[..], &["--tests", "include"]].concat(), None),
        ([&around[..], &["--tests", "only"]].concat(), None),
        (show.to_vec(), Some(counted)),
        ([&show[..], &["--tests", "include"]].concat(), None),
        (vec!["graph", "--show", "src/lib.rs::beta"], None),
    ];
    for (args, line) in cases {
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        match line {
            Some(line) => assert_eq!(out.lines().nth(1), Some(line), "{args:?}:\n{out}"),
            None => assert!(!out.contains("not shown"), "{args:?}:\n{out}"),
        }
    }
}

/// GIVEN a built project whose tests hide under the default filter (`one` proving the product
/// `alpha`, a test named `alpha`, one named `beta` beside two products so named, and an inline
/// `gamma` proving the product it shares a name with), WHEN `rigger graph --show` names a hidden
/// test by id or by leaf, a product beside same-named tests, or nothing, THEN the line under the
/// label counts each test hit the filter hid once (the tests the query names and those proving
/// the node it locates) and names the filter that shows them, the answer says there is no such
/// entity only when no test hid, and `include` locates the test and counts nothing.
#[test]
fn show_counts_the_hidden_tests_its_query_names() {
    let dir = temp_project();
    let root = dir.path();
    write_text(
        root,
        "src/lib.rs",
        "pub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn gamma() { super::gamma(); }\n}\n",
    );
    write_text(root, "src/other.rs", "pub fn beta() {}\n");
    write_text(root, "tests/t.rs", "#[test]\nfn one() { alpha(); }\n");
    write_text(
        root,
        "tests/u.rs",
        "#[test]\nfn alpha() {}\n#[test]\nfn beta() {}\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let one = "1 test hit not shown; the tests filter (include or only) shows them";
    let two = "2 test hits not shown; the tests filter (include or only) shows them";
    let none = "show \"nothing\": no such entity in the graph";
    // (the arguments, the line under the label when one counts hidden tests, a line it holds)
    let cases: [(&[&str], Option<&str>, &str); 7] = [
        (&["--show", "tests/t.rs::one"], Some(one), one),
        (&["--show", "one"], Some(one), one),
        (
            &["--show", "tests/t.rs::one", "--tests", "include"],
            None,
            "show tests/t.rs::one",
        ),
        (&["--show", "alpha"], Some(two), "show src/lib.rs::alpha"),
        (&["--show", "beta"], Some(one), "src/other.rs::beta"),
        (&["--show", "gamma"], Some(one), "show src/lib.rs::gamma"),
        (&["--show", "nothing"], None, none),
    ];
    for (args, line, holds) in cases {
        let args = [&["graph"][..], args].concat();
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        assert!(out.contains(holds), "{args:?} holds {holds:?}:\n{out}");
        match line {
            Some(line) => {
                assert_eq!(out.lines().nth(1), Some(line), "{args:?}:\n{out}");
                assert!(!out.contains("no such entity"), "{args:?}:\n{out}");
            }
            None => assert!(!out.contains("not shown"), "{args:?}:\n{out}"),
        }
    }
}

/// GIVEN a built project where `p` and `w` are each defined in two files, so a call to either
/// from another file lands on that file's placeholder and proves neither, and the test `t` and
/// the test helper `h` call `p` from a test file while the test `u` calls `h` and the test `v`
/// calls `w`, WHEN `rigger graph --around` reads one `p` or one `w` under each `--tests` setting,
/// THEN `exclude` counts the tests the callers walk reaches through a placeholder from a node it
/// steps from, at any depth (never stepping from a test to reach `u`, nor counting `v` for the
/// `w` that `p`'s file shows), and `include` shows them and counts nothing.
#[test]
fn around_counts_the_hidden_tests_calling_through_a_placeholder() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub mod a;\npub mod b;\n");
    write_text(root, "src/a.rs", "pub fn p() {}\npub fn w() {}\n");
    write_text(root, "src/b.rs", "pub fn p() {}\npub fn w() {}\n");
    write_text(
        root,
        "tests/t.rs",
        "#[test]\nfn t() { p(); }\nfn h() { p(); }\n#[test]\nfn u() { h(); }\n#[test]\nfn v() { w(); }\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let one = "1 test hit not shown; the tests filter (include or only) shows them";
    let two = "2 test hits not shown; the tests filter (include or only) shows them";
    // (the queried id, the depth and filter arguments, the line under the label when one counts
    // hidden tests)
    let cases: [(&str, &[&str], Option<&str>); 4] = [
        ("src/a.rs::p", &["1"], Some(two)),
        ("src/a.rs::p", &["2"], Some(two)),
        ("src/a.rs::p", &["2", "--tests", "include"], None),
        ("src/a.rs::w", &["1"], Some(one)),
    ];
    for (id, extra, line) in cases {
        let args = [&["graph", "--around", id, "--depth"][..], extra].concat();
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        match line {
            Some(line) => assert_eq!(out.lines().nth(1), Some(line), "{args:?}:\n{out}"),
            None => {
                assert!(!out.contains("not shown"), "{args:?}:\n{out}");
                assert!(out.contains("node tests/t.rs::h"), "{args:?}:\n{out}");
            }
        }
    }
}

/// GIVEN a built project where `helper` has a `#[cfg(test)]` twin calling `target` beside its
/// product definition, so the one id `helper` is product code holding a test call, WHEN `rigger
/// graph` reads the community, `target` and the file under each `--tests` setting, THEN the
/// twin's call is shown as a test edge under `include` but names no test entity: the community
/// lists no test and draws no `TESTS` edge, and no answer counts a hidden test for it.
#[test]
fn a_test_twin_of_a_product_definition_is_no_test_entity() {
    let dir = temp_project();
    let root = dir.path();
    write_text(
        root,
        "src/lib.rs",
        "pub fn target() {}\n#[cfg(test)]\nfn helper() { target(); }\n#[cfg(not(test))]\nfn helper() {}\npub fn user() { helper(); }\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let file = ["--around", "src/lib.rs", "--depth", "1"];
    // (the arguments, a line the answer holds, what it never holds)
    let cases: [(&[&str], &str, &[&str]); 4] = [
        (
            &["communities", "--tests", "include"],
            "community/1/0",
            &["test src/lib.rs::helper", "1 test"],
        ),
        (
            &["--show", "src/lib.rs::target"],
            "show src/lib.rs::target",
            &["not shown"],
        ),
        (&file, "node src/lib.rs::helper", &["not shown"]),
        (
            &[&file[..], &["--tests", "include"]].concat(),
            "edge src/lib.rs::helper -CALLS-> src/lib.rs::target",
            &["-TESTS->"],
        ),
    ];
    for (args, holds, lacks) in cases {
        let args = [&["graph"][..], args].concat();
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        assert!(out.contains(holds), "{args:?} holds {holds:?}:\n{out}");
        for lack in lacks {
            assert!(!out.contains(lack), "{args:?} never holds {lack:?}:\n{out}");
        }
    }
}

/// GIVEN a built project whose design doc links a product function and the inline test proving
/// it, WHEN the graph reads the doc under each `--tests` setting, THEN the linked test is test code
/// like any other: `exclude` hides it and its link, standing in no artifact and clustering it into
/// no concept, while `include` shows it as the code entity it is and `only` shows the doc's link
/// to it.
#[test]
fn a_design_link_to_an_inline_test_is_test_code() {
    let dir = temp_project();
    let root = dir.path();
    write_text(
        root,
        "src/lib.rs",
        "pub fn run() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn checks() { super::run(); }\n}\n",
    );
    write_text(
        root,
        "docs/a.md",
        "# Running\n\nThe entry is `src/lib.rs::run`, proved by `src/lib.rs::tests::checks`.\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let doc = ["--around", "docs/a.md", "--depth", "1"];
    let link = "edge docs/a.md -SPECIFIES-> src/lib.rs::tests::checks";
    // (the arguments, a line the answer holds, what it never holds)
    let cases: [(&[&str], &str, &[&str]); 5] = [
        (
            &doc,
            "edge docs/a.md -SPECIFIES-> src/lib.rs::run",
            &["tests::checks", "artifact"],
        ),
        (
            &["--show", "src/lib.rs::tests::checks"],
            "not shown",
            &["artifact"],
        ),
        (&["concepts"], "1 concept over 2 node(s)", &[]),
        (
            &[&doc[..], &["--tests", "include"]].concat(),
            link,
            &["artifact"],
        ),
        (
            &[&doc[..], &["--tests", "only"]].concat(),
            link,
            &["artifact"],
        ),
    ];
    for (args, holds, lacks) in cases {
        let args = [&["graph"][..], args].concat();
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        assert!(out.contains(holds), "{args:?} holds {holds:?}:\n{out}");
        for lack in lacks {
            assert!(!out.contains(lack), "{args:?} never holds {lack:?}:\n{out}");
        }
    }
}

/// The source of a Python module that imports `f` from `y.py` and calls it at module level,
/// outside every definition.
const TOP_LEVEL_CALL: &str = "from y import f\n\nf()\n";

/// A Python function `f` in `y.py`, called at module level by the product module `z.py` and by the
/// test module `test_y.py`.
const FILE_CALLS: [(&str, &str); 3] = [
    ("y.py", "def f():\n    return 1\n"),
    ("z.py", TOP_LEVEL_CALL),
    ("test_y.py", TOP_LEVEL_CALL),
];

/// GIVEN a built project whose product module and test module each call a function at module
/// level, outside every definition, WHEN the graph reads the function's neighbourhood under each
/// `--tests` setting, THEN each call is its file's: the product module calls the function, and the
/// test module's call shows under `include` and is the hidden test hit under `exclude`.
#[test]
fn a_call_outside_every_definition_is_its_files_call() {
    let dir = temp_project();
    let root = dir.path();
    write_files(root, &FILE_CALLS);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let around = ["--around", "y.py::f", "--depth", "1"];
    // (the arguments, the lines the answer holds, what it never holds)
    let cases: [(&[&str], &[&str], &[&str]); 2] = [
        (
            &around,
            &["edge z.py -CALLS-> y.py::f", "1 test hit not shown"],
            &["test_y.py"],
        ),
        (
            &[&around[..], &["--tests", "include"]].concat(),
            &[
                "edge z.py -CALLS-> y.py::f",
                "edge test_y.py -CALLS-> y.py::f",
            ],
            &["not shown"],
        ),
    ];
    for (args, holds, lacks) in cases {
        let args = [&["graph"][..], args].concat();
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{args:?} answers; stderr:\n{err}");
        for hold in holds {
            assert!(out.contains(hold), "{args:?} holds {hold:?}:\n{out}");
        }
        for lack in lacks {
            assert!(!out.contains(lack), "{args:?} never holds {lack:?}:\n{out}");
        }
    }
}

/// GIVEN a built project whose one community has a test proving it, WHEN `rigger graph
/// communities` reads it under each `--tests` setting, THEN the community shows its attached test
/// count; `include` also lists the test beneath it, and `only` reports the tests alone.
#[test]
fn graph_communities_report_their_attached_tests() {
    let dir = temp_project();
    let root = dir.path();
    write_text(
        root,
        "src/lib.rs",
        "pub fn alpha() { beta() }\npub fn beta() {}\n",
    );
    write_text(
        root,
        "tests/t.rs",
        "#[test]\nfn proves_alpha() { alpha(); }\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let summary = "graph communities: 1 community over 3 node(s) at resolution 1\n";
    let community = "community/1/0\t3\tbeta\t1 test\n";
    let listed = "  test tests/t.rs::proves_alpha\n";
    // (the extra arguments, the whole report)
    let cases: [(&[&str], String); 3] = [
        (&[], format!("{summary}{community}")),
        (
            &["--tests", "include"],
            format!("{summary}{community}{listed}"),
        ),
        (
            &["--tests", "only"],
            format!("{summary}{community}{listed}"),
        ),
    ];
    for (extra, report) in cases {
        let mut args = vec!["graph", "communities"];
        args.extend(extra);
        assert_eq!(run_rigger_ok(root, &args), report, "{extra:?}");
    }
}

/// GIVEN a built project whose product function `alpha` has a test calling it, WHEN `rigger
/// ground alpha` runs under each `--tests` setting, THEN `exclude` (the default) prints the
/// product row and says on stderr how many test hits it left out, `include` prints the product
/// row (a test's call joins the entity it calls) and `only` the test rows, neither stating a count, and a bad value is refused.
#[test]
fn ground_takes_the_same_tests_option_and_counts_what_it_hides() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
    write_text(
        root,
        "tests/t.rs",
        "#[test]\nfn alpha_works() { alpha(); }\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let hidden = "test hit not shown";
    // (the extra arguments, whether the product row and the test row print, whether a count does)
    let cases: [(&[&str], bool, bool, bool); 4] = [
        (&[], true, false, true),
        (&["--tests", "exclude"], true, false, true),
        // The ranked page dedups by entity, so the test call joins the product row it calls.
        (&["--tests", "include"], true, false, false),
        (&["--tests", "only"], false, true, false),
    ];
    for (extra, product, test, counted) in cases {
        let mut args = vec!["ground", "alpha"];
        args.extend(extra);
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{extra:?} answers; stderr:\n{err}");
        assert_eq!(out.contains("src/lib.rs:"), product, "{extra:?}:\n{out}");
        assert_eq!(out.contains("tests/t.rs:"), test, "{extra:?}:\n{out}");
        assert_eq!(err.contains(hidden), counted, "{extra:?}:\n{err}");
    }
    let (_, err, ok) = run_rigger(root, &["ground", "alpha", "--tests", "some"]);
    assert!(!ok && err.contains("not \"some\""), "{err}");
}

/// GIVEN a committed, built project with an untracked file at its top level, WHEN `rigger ground`
/// runs from a subdirectory, THEN its label names the caller's worktree (the git top-level, as
/// every other read names it) and counts the untracked file outside the cwd among the files the
/// answer takes from it.
#[test]
fn ground_names_the_callers_worktree_from_a_subdirectory() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    write_text(root, "loose.rs", "pub fn beta() {}\n");
    let head = git_out(root, &["rev-parse", "HEAD"]);
    let worktree = root.canonicalize().unwrap().display().to_string();
    let label = format!("describes {worktree} ({head} + 1 file); groupings describe {head}");

    let (out, err, ok) = run_rigger(&root.join("src"), &["ground", "alpha"]);

    assert!(ok, "ground answers; stderr:\n{err}");
    assert!(
        out.contains("src/lib.rs:1:"),
        "the hit answers; stdout:\n{out}"
    );
    assert!(
        err.lines().any(|l| l == label),
        "the label is {label:?}; stderr:\n{err}"
    );
}

/// GIVEN a committed, built project whose workflow definition names a stage, WHEN the definition
/// is edited and `rigger graph --around` reads the stage, THEN the stage, a fact of the differing
/// file, is answered as the worktree's definition has it, never from the snapshot.
#[test]
fn a_workflow_node_of_a_differing_definition_describes_the_worktree() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
    let workflow = "stages:\n  implement:\n    agent: worker\n";
    write_text(root, ".rigger/workflow.yml", workflow);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    write_text(
        root,
        ".rigger/workflow.yml",
        &workflow.replace("worker", "other"),
    );
    let head = git_out(root, &["rev-parse", "HEAD"]);
    let worktree = root.canonicalize().unwrap().display().to_string();

    let (out, err, ok) = run_rigger(
        root,
        &["graph", "--around", "stage:implement", "--depth", "1"],
    );

    assert!(ok, "the around answers; stderr:\n{err}");
    let label = format!("describes {worktree} ({head} + 1 file); groupings describe {head}");
    assert_eq!(out.lines().next(), Some(label.as_str()), "stdout:\n{out}");
    let answer: Vec<&str> = out.lines().skip(2).collect();
    assert!(
        answer.iter().any(|l| l.contains("agent:other")),
        "the worktree's definition is answered; stdout:\n{out}"
    );
    assert!(
        !answer.iter().any(|l| l.contains("agent:worker")),
        "the snapshot's definition is not; stdout:\n{out}"
    );
}

/// GIVEN a built project holding [`READERS`], WHEN `rigger graph --lookup` reads the constant and
/// the type, THEN the lookup names each one's referrers by kind: the constant's doc's first line
/// under its site line, then a `reads:` line naming the two product functions at the lines that
/// read it, in mention order (line order here, since no reader sits in a macro's arguments), and
/// neither test reader, though the `#[cfg(test)]` reader's id is the product `helper`'s; the type's
/// `type use:` line names the function whose signature names it. Read the same way, the
/// function's `called by:` line names each product call at the call's own line, once per call, in
/// mention order, and not the `#[test]` function's call.
#[test]
fn the_lookup_names_referrers_by_kind() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", READERS);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    // (the term, its block's lines from its heading to the next section's)
    let cases: [(&str, &[&str]); 3] = [
        (
            "LIMIT",
            &[
                "### LIMIT",
                "exact match: 1 entity",
                "- src/lib.rs:3 src/lib.rs::LIMIT (constant)",
                "  The most widgets one bin holds.",
                "  reads: src/lib.rs:11 src/lib.rs::spill (function); src/lib.rs:16 src/lib.rs::fits (function)",
                "",
                "## text",
            ],
        ),
        (
            "Bin",
            &[
                "### Bin",
                "exact match: 1 entity",
                "- src/lib.rs:5 src/lib.rs::Bin (type)",
                "  type use: src/lib.rs:9 src/lib.rs::spill (function)",
                "",
                "## text",
            ],
        ),
        (
            "ready",
            &[
                "### ready",
                "exact match: 1 entity",
                "- src/lib.rs:38 src/lib.rs::ready (function)",
                "  called by: src/lib.rs:43 src/lib.rs::start (function); src/lib.rs:47 src/lib.rs::begin (function); src/lib.rs:48 src/lib.rs::begin (function)",
                "",
                "## text",
            ],
        ),
    ];
    lookup_prints_blocks(root, &[], &cases);
}

/// A constant `LIMIT` defined in `src/a.rs` and `src/b.rs` and read by `fits` in `src/c.rs`; a
/// function `shared` defined in `src/d.rs` and `src/e.rs` and `alone` defined in `src/g.rs`, each
/// called by `spill` in `src/f.rs`.
const AMBIGUOUS: [(&str, &str); 7] = [
    ("src/a.rs", "pub const LIMIT: usize = 3;\n"),
    ("src/b.rs", "pub const LIMIT: usize = 4;\n"),
    (
        "src/c.rs",
        "pub fn fits(n: usize) -> bool {\n    n <= LIMIT\n}\n",
    ),
    ("src/d.rs", "pub fn shared() {}\n"),
    ("src/e.rs", "pub fn shared() {}\n"),
    (
        "src/f.rs",
        "pub fn spill() {\n    shared();\n    alone();\n}\n",
    ),
    ("src/g.rs", "pub fn alone() {}\n"),
];

/// GIVEN a built project whose constant `LIMIT` is defined in two files and read by one function
/// in a third, WHEN `rigger graph --lookup` reads the constant, THEN each constant's `reads:` line
/// names the reading function marked `?`: its mention gave neither constant a single edge. Read the
/// same way, a function `shared` defined in two files and called by `spill` in another lists `spill`
/// marked `?` under each one's `called by:` line, its call kept by a placeholder holding both, and
/// `alone`, defined once in a file of its own and called by `spill` too, lists it unmarked: that
/// call reaches its one definition alone.
#[test]
fn an_ambiguous_referrer_is_marked() {
    let dir = temp_project();
    let root = dir.path();
    write_files(root, &AMBIGUOUS);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    // (the term, its block's lines from its heading to the next section's)
    let cases: [(&str, &[&str]); 3] = [
        (
            "LIMIT",
            &[
                "### LIMIT",
                "exact match: 2 entities",
                "- src/a.rs:1 src/a.rs::LIMIT (constant)",
                "  reads: src/c.rs:2 src/c.rs::fits (function) ?",
                "- src/b.rs:1 src/b.rs::LIMIT (constant)",
                "  reads: src/c.rs:2 src/c.rs::fits (function) ?",
                "",
                "## text",
            ],
        ),
        (
            "shared",
            &[
                "### shared",
                "exact match: 2 entities",
                "- src/d.rs:1 src/d.rs::shared (function)",
                "  called by: src/f.rs:2 src/f.rs::spill (function) ?",
                "- src/e.rs:1 src/e.rs::shared (function)",
                "  called by: src/f.rs:2 src/f.rs::spill (function) ?",
                "",
                "## text",
            ],
        ),
        (
            "alone",
            &[
                "### alone",
                "exact match: 1 entity",
                "- src/g.rs:1 src/g.rs::alone (function)",
                "  called by: src/f.rs:3 src/f.rs::spill (function)",
                "",
                "## text",
            ],
        ),
    ];
    lookup_prints_blocks(root, &[], &cases);
}

/// The lookup's block for [`READERS`]'s `LIMIT`, from its heading to the next section's, while
/// tests are hidden: its `reads:` line names the two product functions alone.
const LIMIT_READ_BY_PRODUCTS: &[&str] = &[
    "### LIMIT",
    "exact match: 1 entity",
    "- src/lib.rs:3 src/lib.rs::LIMIT (constant)",
    "  The most widgets one bin holds.",
    "  reads: src/lib.rs:11 src/lib.rs::spill (function); src/lib.rs:16 src/lib.rs::fits (function)",
    "",
    "## text",
];

/// GIVEN a built project holding [`READERS`], WHEN `rigger graph --lookup` reads the constant with
/// no `--tests` option, under `--tests exclude`, under `--tests include` and under `--tests only`,
/// THEN its `reads:` line names the two product functions alone unless tests are shown. Under
/// include and only it names the test readers after them too, in mention order, each marked
/// `[test]`: the `#[cfg(test)]` twin under the product `helper`'s id, then the `#[test]` function;
/// and an `imported by:` line names the test module whose `use` imports the constant, marked
/// `[test]` too.
#[test]
fn a_test_referrer_is_hidden_by_default() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", READERS);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let product = LIMIT_READ_BY_PRODUCTS;
    let every: &[&str] = &[
        "### LIMIT",
        "exact match: 1 entity",
        "- src/lib.rs:3 src/lib.rs::LIMIT (constant)",
        "  The most widgets one bin holds.",
        "  reads: src/lib.rs:11 src/lib.rs::spill (function); src/lib.rs:16 src/lib.rs::fits (function); src/lib.rs:21 src/lib.rs::helper (function) [test]; src/lib.rs:34 src/lib.rs::tests::the_bound_holds (method) [test]",
        "  imported by: src/lib.rs:30 src/lib.rs::tests (module) [test]",
        "",
        "## text",
    ];
    // (the options the lookup passes, the constant's block from its heading to the next section's)
    let rows: [(&[&str], &[&str]); 4] = [
        (&[], product),
        (&["--tests", "exclude"], product),
        (&["--tests", "include"], every),
        (&["--tests", "only"], every),
    ];
    for (options, block) in rows {
        lookup_prints_blocks(root, options, &[("LIMIT", block)]);
    }
}

/// GIVEN a built project holding [`READERS`] and a file opening with `//!` lines, WHEN `rigger
/// graph --show` shows the documented constant with no `--tests` option and under `--tests
/// include`, and shows the file, THEN the constant's entries after its attrs are its doc, stripped
/// of its markers and printed on one line, then a `referenced by reads` entry naming the two product
/// readers as the lookup's `reads:` line names them, joined by `, `. Under include, the test readers
/// follow them marked `[test]`, and a `referenced by imported by` entry names the test module. The
/// file's entries end with the file's doc.
#[test]
fn show_prints_the_doc_and_the_referrers() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", READERS);
    write_text(
        root,
        "src/notes.rs",
        "//! Notes on bins.\n//! Kept apart from the readers.\n\npub fn noted() {}\n",
    );
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let doc = "  doc_comment The most widgets one bin holds. It bounds every bin.";
    let product =
        "src/lib.rs:11 src/lib.rs::spill (function), src/lib.rs:16 src/lib.rs::fits (function)";
    let reads = format!("  referenced by reads {product}");
    let every = format!(
        "{reads}, src/lib.rs:21 src/lib.rs::helper (function) [test], \
         src/lib.rs:34 src/lib.rs::tests::the_bound_holds (method) [test]"
    );
    // (the show's arguments, its entry lines between the site line and the body or the note)
    let rows: [(&[&str], Vec<&str>); 3] = [
        (
            &["graph", "--show", "src/lib.rs::LIMIT"],
            vec!["  name LIMIT", "  lang rust", doc, reads.as_str()],
        ),
        (
            &["graph", "--show", "src/lib.rs::LIMIT", "--tests", "include"],
            vec![
                "  name LIMIT",
                "  lang rust",
                doc,
                every.as_str(),
                "  referenced by imported by src/lib.rs:30 src/lib.rs::tests (module) [test]",
            ],
        ),
        (
            &["graph", "--show", "src/notes.rs"],
            vec![
                "  lang rust",
                "  doc_comment Notes on bins. Kept apart from the readers.",
            ],
        ),
    ];
    for (args, entries) in rows {
        let out = common::cli::past_label(&run_rigger_ok(root, args));
        let printed: Vec<&str> = (out.lines().skip(2))
            .take_while(|l| !common::cli::is_body_line(l) && !l.starts_with("  ("))
            .collect();
        assert_eq!(printed, entries, "{args:?}:\n{out}");
    }
}

/// One row of [`show_lists_each_referrer_as_the_lookup_does`]: the project's files, the shown id,
/// the looked-up name, the options both reads pass, and the show's `referenced by` entries.
type ReferrerRow = (
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

/// GIVEN a built project holding [`AMBIGUOUS`] and one holding [`FILE_CALLS`], WHEN `rigger graph
/// --show` shows an entity and `rigger graph --lookup` looks up its name under the same `--tests`
/// option, THEN each `referenced by <label>` entry of the show lists the sites that the lookup's
/// `<label>:` line under that entity lists. They come in the same order and each is printed the
/// same way; the show joins them by `, ` where the lookup joins them by `; `. The rows are a reader
/// in another file marked `?`, and, under `--tests include`, the module-level calls of a product
/// and a test module, each site naming its file alone.
#[test]
fn show_lists_each_referrer_as_the_lookup_does() {
    let rows: [ReferrerRow; 2] = [
        (
            &AMBIGUOUS,
            "src/a.rs::LIMIT",
            "LIMIT",
            &[],
            &["  referenced by reads src/c.rs:2 src/c.rs::fits (function) ?"],
        ),
        (
            &FILE_CALLS,
            "y.py::f",
            "f",
            &["--tests", "include"],
            &["  referenced by called by test_y.py:3 test_y.py (file) [test], z.py:3 z.py (file)"],
        ),
    ];
    for (files, id, term, options, entries) in rows {
        let dir = temp_project();
        let root = dir.path();
        write_files(root, files);
        git_commit_all(root, "source");
        run_rigger_ok(root, &["graph", "build"]);
        let read = |verb: &str, query: &str| {
            run_rigger_ok(root, &[&["graph", verb, query][..], options].concat())
        };
        let show = read("--show", id);
        let shown: Vec<&str> = (show.lines())
            .filter(|l| l.starts_with("  referenced by "))
            .collect();
        assert_eq!(shown, entries, "{id} {options:?}:\n{show}");
        // The lookup's label lines under the entity's site line, keyed and joined as the show's.
        let lookup = read("--lookup", term);
        let site = format!(" {id} (");
        let listed: Vec<String> = (lookup.lines())
            .skip_while(|l| !(l.starts_with("- ") && l.contains(&site)))
            .skip(1)
            .take_while(|l| l.starts_with("  "))
            .filter_map(|l| {
                REFERRER_LABELS.iter().find_map(|(_, label)| {
                    let sites = l.strip_prefix(&format!("  {label}: "))?;
                    Some(format!(
                        "  referenced by {label} {}",
                        sites.replace("; ", ", ")
                    ))
                })
            })
            .collect();
        assert_eq!(listed, entries, "{term} {options:?}:\n{lookup}");
    }
}

/// GIVEN a built project holding [`READERS`] and two linked worktrees at its head, WHEN one
/// worktree's uncommitted edit appends a function reading the constant and `rigger graph --lookup`
/// reads the constant from that worktree and then from the other, THEN the editing worktree's
/// `reads:` line names the new reader after the two product functions, at the line that reads
/// it, and the other's names the two product functions alone.
#[test]
fn the_overlay_carries_new_references() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", READERS);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    let trees = tempfile::tempdir().unwrap();
    let [edited, other] =
        ["edited", "other"].map(|name| linked_worktree(root, trees.path().join(name)));
    let drain = "pub fn drain() -> usize {\n    LIMIT\n}\n";
    write_text(&edited, "src/lib.rs", &format!("{READERS}{drain}"));
    let carried: &[&str] = &[
        "### LIMIT",
        "exact match: 1 entity",
        "- src/lib.rs:3 src/lib.rs::LIMIT (constant)",
        "  The most widgets one bin holds.",
        "  reads: src/lib.rs:11 src/lib.rs::spill (function); src/lib.rs:16 src/lib.rs::fits (function); src/lib.rs:56 src/lib.rs::drain (function)",
        "",
        "## text",
    ];
    // (the worktree the lookup reads from, the constant's block from its heading to the next
    // section's)
    for (worktree, block) in [(&edited, carried), (&other, LIMIT_READ_BY_PRODUCTS)] {
        lookup_prints_blocks(worktree, &[], &[("LIMIT", block)]);
    }
}

/// Each case's term looked up through the binary from `root`, a worktree of a built project,
/// `options` passed after it: the printed lines from the term's heading on, as many as the case
/// holds, are exactly the case's block.
fn lookup_prints_blocks(root: &std::path::Path, options: &[&str], cases: &[(&str, &[&str])]) {
    for (term, block) in cases {
        let args: Vec<&str> = ["graph", "--lookup", term]
            .into_iter()
            .chain(options.iter().copied())
            .collect();
        let (out, err, ok) = run_rigger(root, &args);
        assert!(ok, "{term} {options:?} answers; stderr:\n{err}");
        let heading = format!("### {term}");
        let printed: Vec<&str> = (out.lines())
            .skip_while(|l| *l != heading)
            .take(block.len())
            .collect();
        assert_eq!(printed, *block, "{term} {options:?}:\n{out}");
    }
}
