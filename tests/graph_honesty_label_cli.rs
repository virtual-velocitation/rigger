//! The honesty label (6.1) heads every graph answer the CLI prints: the commit the answer
//! describes, or the caller's worktree over it when the two differ, and what it withheld when the
//! worktree cannot be compared.

use crate::common;
use common::cli::{run_rigger, run_rigger_ok, temp_project};
use common::fixtures::{git_commit_all, git_out, write_text};

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
