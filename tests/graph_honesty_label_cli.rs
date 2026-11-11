//! The honesty label (6.1) heads every graph answer the CLI prints: the commit the answer
//! describes, then how the caller's worktree differs from it and what that withheld.

use crate::common;
use common::cli::{run_rigger, run_rigger_ok, temp_project};
use common::fixtures::{git_commit_all, git_out, write_text};

/// GIVEN a project whose source is committed and built, WHEN `rigger graph --around` reads it
/// from a clean worktree, from one with an unread file edited, and from one with the read file
/// edited, THEN the first line is the label naming the head commit, the differing files and the
/// nodes withheld for them, and the subgraph header follows it.
#[test]
fn around_prints_the_label_first() {
    // (case, the file edited after the build, the label after `describes <head>`)
    let cases: [(&str, Option<&str>, &str); 3] = [
        ("a clean worktree", None, ""),
        (
            "an edited file the answer does not reach",
            Some("src/other.rs"),
            "; 1 file in {w} differs",
        ),
        // The file's node and its fn's.
        (
            "the edited file the answer reads",
            Some("src/lib.rs"),
            "; 1 file in {w} differs; 2 withheld",
        ),
    ];
    for (case, edited, tail) in cases {
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

        let (out, err, ok) = run_rigger(root, &["graph", "--around", "src/lib.rs", "--depth", "1"]);

        assert!(ok, "{case}: the around answers; stderr:\n{err}");
        let lines: Vec<&str> = out.lines().take(2).collect();
        assert_eq!(
            lines,
            [
                format!("describes {head}{}", tail.replace("{w}", &worktree)).as_str(),
                "subgraph around \"src/lib.rs\" (depth 1):",
            ],
            "{case}: the label heads the answer; stdout:\n{out}"
        );
    }
}
