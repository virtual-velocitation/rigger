//! Periphery tests for spec 117's settled code half, over the built binary. `rigger graph build`
//! settles the code half whole into the snapshot, and a graph read on a clean worktree answers its
//! neighbourhood from those rows under the default filter; a worktree that differs from its head is
//! read through the overlay, which offers no settled half, so the same read resolves near the ids
//! it reads, as it does under any other tests filter. The crates' own tests pin each side alone (the
//! snapshot's rows against the resolution they settle, a neighbourhood read over a hand-built half);
//! these drive the CLI across the store-to-reader seam, so a half the store settles wrong, or a
//! read that takes the half where it must not, prints an answer the resolving path does not: under
//! another tests filter the half lacks the test code, and through the overlay it lacks the
//! worktree's edit.

use crate::common;

use common::cli::{run_rigger_ok, temp_project};
use common::fixtures::{git_commit_all, git_out, write_text, READERS};
use std::path::Path;

/// A file no read below reaches: editing it makes the worktree differ from its head and leaves
/// every answer's facts as the head holds them.
const UNREAD: &str = "pub fn beta() -> u8 {\n    7\n}\n";

/// GIVEN a built project holding [`READERS`] and an [`UNREAD`] `src/other.rs`,
/// WHEN the operator runs `rigger graph --around` and `--show` on its entities from the clean
/// worktree, where each read under the default filter is served from the settled code half, and
/// again once a comment is appended to `src/other.rs`, where each read resolves near the ids
/// through the overlay,
/// THEN each read prints one exact answer under each worktree's own label: the same nodes, edges,
/// degrees, referrers and hidden test count from both paths, the callers `ready`'s depth-1
/// neighbourhood reaches through its incoming edges alone, the constant a bare name finds by its
/// leaf, and under `--tests include` the test code the half never holds.
#[test]
fn a_graph_read_answers_alike_from_the_settled_half_and_near_its_ids() {
    let dir = temp_project();
    let root = dir.path();
    let head = built(root);
    let hidden = rigger::code_view::tests_hidden_line(1).unwrap();
    let hidden = hidden.as_str();
    // (the read's arguments, its answer under the label)
    let reads: [(&[&str], &[&str]); 6] = [
        (
            &["graph", "--around", "src/lib.rs::ready"],
            &[
                hidden,
                "subgraph around \"src/lib.rs::ready\" (depth 2):",
                "  node community/1/0            community",
                "  node src/lib.rs               file",
                "  node src/lib.rs::Bin          code-entity",
                "  node src/lib.rs::Bin.count    code-entity",
                "  node src/lib.rs::LIMIT        code-entity",
                "  node src/lib.rs::begin        code-entity",
                "  node src/lib.rs::fits         code-entity",
                "  node src/lib.rs::helper       code-entity",
                "  node src/lib.rs::ready        code-entity",
                "  node src/lib.rs::spill        code-entity",
                "  node src/lib.rs::start        code-entity",
                "  edge src/lib.rs -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::Bin",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::LIMIT",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::begin",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::fits",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::helper",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::ready",
                "  edge src/lib.rs -REFERENCES-> src/lib.rs::ready",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::spill",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::start",
                "  edge src/lib.rs::Bin -CONTAINS-> src/lib.rs::Bin.count",
                "  edge src/lib.rs::begin -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::begin -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::fits -REFERENCES-> src/lib.rs::LIMIT",
                "  edge src/lib.rs::helper -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::ready -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::spill -REFERENCES-> src/lib.rs::Bin",
                "  edge src/lib.rs::spill -REFERENCES-> src/lib.rs::Bin.count",
                "  edge src/lib.rs::spill -REFERENCES-> src/lib.rs::LIMIT",
                "  edge src/lib.rs::start -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::start -CALLS-> src/lib.rs::ready",
            ],
        ),
        (
            &["graph", "--around", "src/lib.rs::ready", "--depth", "1"],
            &[
                hidden,
                "subgraph around \"src/lib.rs::ready\" (depth 1):",
                "  node community/1/0            community",
                "  node src/lib.rs               file",
                "  node src/lib.rs::begin        code-entity",
                "  node src/lib.rs::ready        code-entity",
                "  node src/lib.rs::start        code-entity",
                "  edge src/lib.rs -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::begin",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::ready",
                "  edge src/lib.rs -REFERENCES-> src/lib.rs::ready",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::start",
                "  edge src/lib.rs::begin -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::begin -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::ready -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::start -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::start -CALLS-> src/lib.rs::ready",
            ],
        ),
        (
            &["graph", "--show", "src/lib.rs::ready"],
            &[
                hidden,
                "show src/lib.rs::ready",
                "  site: src/lib.rs:38   kind function   degree 5",
                "  name ready",
                "  lang rust",
                "  referenced by called by src/lib.rs:43 src/lib.rs::start (function), src/lib.rs:47 src/lib.rs::begin (function), src/lib.rs:48 src/lib.rs::begin (function)",
                "      38 | pub fn ready() -> bool {",
                "      39 |     true",
                "      40 | }",
            ],
        ),
        (
            &["graph", "--show", "LIMIT"],
            &[
                "show src/lib.rs::LIMIT",
                "  site: src/lib.rs:3   kind constant   degree 4",
                "  name LIMIT",
                "  lang rust",
                "  doc_comment The most widgets one bin holds. It bounds every bin.",
                "  referenced by reads src/lib.rs:11 src/lib.rs::spill (function), src/lib.rs:16 src/lib.rs::fits (function)",
                "       3 | pub const LIMIT: usize = 3;",
            ],
        ),
        (
            &["graph", "--around", "src/lib.rs::ready", "--tests", "include"],
            &[
                "subgraph around \"src/lib.rs::ready\" (depth 2):",
                "  node community/1/0            community",
                "  node src/lib.rs               file",
                "  node src/lib.rs::Bin          code-entity",
                "  node src/lib.rs::Bin.count    code-entity",
                "  node src/lib.rs::LIMIT        code-entity",
                "  node src/lib.rs::begin        code-entity",
                "  node src/lib.rs::fits         code-entity",
                "  node src/lib.rs::helper       code-entity",
                "  node src/lib.rs::it_starts    code-entity",
                "  node src/lib.rs::ready        code-entity",
                "  node src/lib.rs::spill        code-entity",
                "  node src/lib.rs::start        code-entity",
                "  node src/lib.rs::tests        code-entity",
                "  node src/lib.rs::tests::the_bound_holds code-entity",
                "  edge src/lib.rs -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::Bin",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::LIMIT",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::begin",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::fits",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::helper",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::it_starts",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::ready",
                "  edge src/lib.rs -REFERENCES-> src/lib.rs::ready",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::spill",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::start",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::tests",
                "  edge src/lib.rs::Bin -CONTAINS-> src/lib.rs::Bin.count",
                "  edge src/lib.rs::begin -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::begin -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::fits -REFERENCES-> src/lib.rs::LIMIT",
                "  edge src/lib.rs::helper -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::helper -REFERENCES-> src/lib.rs::LIMIT",
                "  edge src/lib.rs::it_starts -TESTS-> community/1/0",
                "  edge src/lib.rs::it_starts -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::ready -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::spill -REFERENCES-> src/lib.rs::Bin",
                "  edge src/lib.rs::spill -REFERENCES-> src/lib.rs::Bin.count",
                "  edge src/lib.rs::spill -REFERENCES-> src/lib.rs::LIMIT",
                "  edge src/lib.rs::start -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::start -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::tests -REFERENCES-> src/lib.rs::LIMIT",
                "  edge src/lib.rs::tests -CONTAINS-> src/lib.rs::tests::the_bound_holds",
                "  edge src/lib.rs::tests::the_bound_holds -REFERENCES-> src/lib.rs::LIMIT",
            ],
        ),
        (
            &["graph", "--show", "src/lib.rs::ready", "--tests", "include"],
            &[
                "show src/lib.rs::ready",
                "  site: src/lib.rs:38   kind function   degree 6",
                "  name ready",
                "  lang rust",
                "  referenced by called by src/lib.rs:43 src/lib.rs::start (function), src/lib.rs:47 src/lib.rs::begin (function), src/lib.rs:48 src/lib.rs::begin (function), src/lib.rs:53 src/lib.rs::it_starts (function) [test]",
                "      38 | pub fn ready() -> bool {",
                "      39 |     true",
                "      40 | }",
            ],
        ),
    ];
    // (the worktree, the comment appended to `src/other.rs` first, its label)
    let worktrees = [
        ("the clean worktree", None, format!("describes {head}")),
        (
            "a worktree differing in an unread file",
            Some("// a note\n"),
            overlaid(root, &head),
        ),
    ];
    for (case, note, label) in worktrees {
        if let Some(note) = note {
            write_text(root, "src/other.rs", &format!("{UNREAD}{note}"));
        }
        assert_reads(root, case, &label, &reads);
    }
}

/// A function appended to `src/lib.rs` past the end of [`READERS`], calling `ready`.
const AGAIN: &str = "\npub fn again() -> bool {\n    ready()\n}\n";

/// GIVEN a built project holding [`READERS`],
/// WHEN an uncommitted edit appends [`AGAIN`] to `src/lib.rs` and the operator runs
/// `rigger graph --around` on `ready` and `--show` on `again` from that worktree,
/// THEN each read prints one exact answer under the worktree's label, the edit's node, its CALLS
/// edge onto `ready` and its site included: the overlay offers no settled half, so the head's
/// settled rows, which hold neither, answer none of it.
#[test]
fn a_worktree_edit_a_read_reaches_is_answered_from_the_worktree_not_the_settled_half() {
    let dir = temp_project();
    let root = dir.path();
    let head = built(root);
    write_text(root, "src/lib.rs", &format!("{READERS}{AGAIN}"));
    let hidden = rigger::code_view::tests_hidden_line(1).unwrap();
    let hidden = hidden.as_str();
    // (the read's arguments, its answer under the label)
    let reads: [(&[&str], &[&str]); 2] = [
        (
            &["graph", "--around", "src/lib.rs::ready", "--depth", "1"],
            &[
                hidden,
                "subgraph around \"src/lib.rs::ready\" (depth 1):",
                "  node community/1/0            community",
                "  node src/lib.rs               file",
                "  node src/lib.rs::again        code-entity",
                "  node src/lib.rs::begin        code-entity",
                "  node src/lib.rs::ready        code-entity",
                "  node src/lib.rs::start        code-entity",
                "  edge src/lib.rs -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::again",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::begin",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::ready",
                "  edge src/lib.rs -REFERENCES-> src/lib.rs::ready",
                "  edge src/lib.rs -CONTAINS-> src/lib.rs::start",
                "  edge src/lib.rs::again -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::begin -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::begin -CALLS-> src/lib.rs::ready",
                "  edge src/lib.rs::ready -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::start -IN_COMMUNITY-> community/1/0",
                "  edge src/lib.rs::start -CALLS-> src/lib.rs::ready",
            ],
        ),
        (
            &["graph", "--show", "again"],
            &[
                "show src/lib.rs::again",
                "  site: src/lib.rs:56   kind function   degree 2",
                "  name again",
                "  lang rust",
                "      56 | pub fn again() -> bool {",
                "      57 |     ready()",
                "      58 | }",
            ],
        ),
    ];
    let label = overlaid(root, &head);
    assert_reads(root, "a worktree editing a read file", &label, &reads);
}

/// Builds the project at `root` holding [`READERS`] and an [`UNREAD`] `src/other.rs`, committed,
/// and returns its head.
fn built(root: &Path) -> String {
    write_text(root, "src/lib.rs", READERS);
    write_text(root, "src/other.rs", UNREAD);
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    git_out(root, &["rev-parse", "HEAD"])
}

/// The label of a read from the worktree at `root` differing from `head` in one file.
fn overlaid(root: &Path, head: &str) -> String {
    let worktree = root.canonicalize().unwrap().display().to_string();
    format!("describes {worktree} ({head} + 1 file); groupings describe {head}")
}

/// Runs each `(arguments, answer)` of `reads` in `root`, asserting it prints `label` and then the
/// answer's lines, exactly; `case` names the worktree in a failure.
fn assert_reads(root: &Path, case: &str, label: &str, reads: &[(&[&str], &[&str])]) {
    for (args, answer) in reads {
        let want = [&[label], *answer].concat().join("\n") + "\n";
        assert_eq!(
            run_rigger_ok(root, args),
            want,
            "{case}: rigger {}",
            args.join(" ")
        );
    }
}
