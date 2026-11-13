//! A graph read spawns git only to compare the caller's worktree with the head it answers from,
//! in every build that serves the read: the CLI's reads and the MCP graph tool alike.

use crate::common;
use common::cli::{rigger_command, run_rigger_envs, run_rigger_ok, temp_project};
use common::fixtures::{git_commit_all, git_ok, git_ok_with_identity, git_out, write_text};
use common::mcp::McpSession;
use serde_json::json;
use std::path::{Path, PathBuf};

/// GIVEN a committed, built project with one file edited since, WHEN `rigger graph` answers
/// `--around` the file, `--show` its function and `--lookup` the function's name, from the
/// checkout and from a linked worktree of it with the same file edited, and the MCP graph tool
/// answers `around` and `show` of them, each read traced by git itself (`GIT_TRACE`), THEN each
/// read's one open runs exactly two git commands: the worktree diff against the head's tree and
/// the untracked listing. A build with no code store has no snapshot to take the head from, so it
/// also asks git for the head's branch, the object it names, and that object's commit and tree in
/// one query.
#[test]
fn one_open_runs_exactly_the_diff_and_the_untracked_listing() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
    write_text(root, "src/other.rs", "pub fn beta() {}\n");
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    write_text(root, "src/other.rs", "pub fn gamma() {}\n");
    let traces = tempfile::tempdir().unwrap();
    let expected = one_open(root);
    let linked = common::git::linked_worktree(root, traces.path().join("linked"));
    write_text(&linked, "src/other.rs", "pub fn gamma() {}\n");
    // (read, the worktree it reads from, the arguments)
    let cases: [(&str, &std::path::Path, &[&str]); 4] = [
        (
            "around",
            root,
            &["graph", "--around", "src/lib.rs", "--depth", "1"],
        ),
        ("show", root, &["graph", "--show", "src/lib.rs::alpha"]),
        ("lookup", root, &["graph", "--lookup", "alpha"]),
        (
            "around from a linked worktree",
            &linked,
            &["graph", "--around", "src/lib.rs", "--depth", "1"],
        ),
    ];
    for (case, cwd, args) in cases {
        let (_, traced) = traced_read(cwd, args, &[], &traces.path().join(case));
        assert_eq!(
            git_commands(&traced),
            expected,
            "{case}: one open runs the diff and the untracked listing alone; trace:\n{traced}"
        );
    }

    // The MCP graph tool: each call is a read of its own over the one session.
    let trace = traces.path().join("mcp");
    let mut mcp = McpSession::from_command(rigger_command(
        root,
        &["mcp"],
        &[("GIT_TRACE", trace.to_str().unwrap())],
        root,
    ));
    mcp.initialize();
    // (read, the tool's arguments)
    let calls = [
        ("mcp around", json!({"around": "src/lib.rs", "depth": 1})),
        ("mcp show", json!({"show": "src/lib.rs::alpha"})),
    ];
    for (case, arguments) in calls {
        let before = std::fs::read_to_string(&trace).unwrap_or_default().len();
        let answer = mcp.graph_tool(arguments);
        let traced = std::fs::read_to_string(&trace).unwrap_or_default()[before..].to_string();
        assert!(!answer.is_null(), "{case}: the tool answers");
        assert_eq!(
            git_commands(&traced),
            expected,
            "{case}: one open runs the diff and the untracked listing alone; trace:\n{traced}"
        );
    }
    mcp.finish_cleanly();
}

/// How a layout lays out a checkout under a scratch directory: the checkout a read runs from.
type Layout = fn(&Path) -> PathBuf;

/// GIVEN a committed project laid out in each shape git finds a checkout in, built, with a file
/// edited since, WHEN `rigger graph --around` reads the other file from the checkout, traced by
/// git itself, THEN it answers from that checkout's own content, its store lies at the root its
/// repository resolves to, and its one open runs exactly the diff and the untracked listing: each
/// layout's repository is read from git's files, never asked of git. The root is the main
/// checkout git records, which a layout builds first where it is not the checkout read from, and
/// the checkout's own top where git records none.
#[test]
fn every_layout_git_finds_a_checkout_in_is_read_from_its_files() {
    // (layout, how it is laid out, the root its store lies at below the scratch directory the
    // layout lies in, whether the read runs under a git ceiling at that scratch directory, which
    // a walk up from the checkout meets after its `.git`)
    let layouts: [(&str, Layout, &str, bool); 16] = [
        (
            "a clone",
            |at| cloned(&at.join("origin"), at),
            "clone",
            false,
        ),
        (
            "a clone with every ref packed",
            |at| {
                let clone = cloned(&at.join("origin"), at);
                git_ok(&clone, &["pack-refs", "--all"]);
                clone
            },
            "clone",
            false,
        ),
        (
            "a clone whose remote path names worktrees",
            |at| cloned(&at.join("worktrees/origin"), at),
            "clone",
            false,
        ),
        (
            "a linked worktree naming its administrative directory relatively",
            |at| {
                let main = built(source(&at.join("main")));
                let linked = common::git::linked_worktree(&main, at.join("linked"));
                write_text(&linked, ".git", "gitdir: ../main/.git/worktrees/linked\n");
                linked
            },
            "main",
            false,
        ),
        (
            "a sparse checkout",
            |at| {
                let clone = cloned(&at.join("origin"), at);
                git_ok(&clone, &["sparse-checkout", "set", "--no-cone", "/*"]);
                clone
            },
            "clone",
            false,
        ),
        (
            "a checkout whose git directory lies apart",
            apart,
            "apart",
            false,
        ),
        (
            "a linked worktree of a checkout whose git directory lies apart",
            |at| common::git::linked_worktree(&apart(at), at.join("linked")),
            "linked",
            false,
        ),
        ("a submodule", submodule, "outer/sub", false),
        (
            "a linked worktree of a submodule",
            |at| common::git::linked_worktree(&built(submodule(at)), at.join("linked")),
            "outer/sub",
            false,
        ),
        (
            "a checkout whose .git is a symlink",
            dot_symlink,
            "clone",
            false,
        ),
        (
            "a linked worktree of a checkout whose .git is a symlink",
            |at| common::git::linked_worktree(&dot_symlink(at), at.join("linked")),
            "linked",
            false,
        ),
        (
            "a linked worktree under per-worktree configuration",
            |at| {
                let main = built(source(&at.join("main")));
                git_ok(&main, &["config", "extensions.worktreeConfig", "true"]);
                common::git::linked_worktree(&main, at.join("linked"))
            },
            "main",
            false,
        ),
        (
            "a bare repository's linked worktree",
            |at| {
                let origin = source(&at.join("origin"));
                git_ok(
                    at,
                    &[
                        "clone",
                        "-q",
                        "--bare",
                        origin.to_str().unwrap(),
                        "bare.git",
                    ],
                );
                common::git::linked_worktree(&at.join("bare.git"), at.join("linked"))
            },
            "linked",
            false,
        ),
        (
            "a linked worktree of a checkout configured bare",
            |at| {
                let clone = cloned(&at.join("origin"), at);
                git_ok(&clone, &["config", "core.bare", "true"]);
                common::git::linked_worktree(&clone, at.join("linked"))
            },
            "linked",
            false,
        ),
        (
            "a checkout whose configuration includes a file",
            |at| {
                let clone = cloned(&at.join("origin"), at);
                write_text(&clone, ".git/extra", "[core]\n\tautocrlf = false\n");
                git_ok(&clone, &["config", "include.path", "extra"]);
                clone
            },
            "clone",
            false,
        ),
        (
            "a clone read under a ceiling above it",
            |at| cloned(&at.join("origin"), at),
            "clone",
            true,
        ),
    ];
    for (layout, lay_out, root, under_a_ceiling) in layouts {
        assert_one_open_in(layout, lay_out, root, under_a_ceiling, &[]);
    }
}

/// GIVEN a committed project laid out in each shape whose repository git's files leave to git
/// to set up, built, with a file edited since, WHEN `rigger graph --around` reads the other file
/// from the checkout, traced by git itself, THEN it answers from that checkout's own content and
/// its one open asks git where the repository lies once, in one `rev-parse`, besides the diff and
/// the untracked listing.
#[test]
fn every_layout_git_alone_sets_up_is_asked_of_git_once() {
    // (layout, how it is laid out, the root its store lies at below the scratch directory)
    let layouts: [(&str, Layout, &str); 3] = [
        (
            "a clone whose configuration names no format version",
            |at| {
                let clone = cloned(&at.join("origin"), at);
                git_ok(
                    &clone,
                    &["config", "--unset", "core.repositoryformatversion"],
                );
                clone
            },
            "clone",
        ),
        (
            "a clone of format 0 naming an extension git ignores there",
            |at| {
                let clone = cloned(&at.join("origin"), at);
                git_ok(&clone, &["config", "extensions.unheardof", "true"]);
                clone
            },
            "clone",
        ),
        (
            "a checkout whose .git directory names a common directory",
            |at| {
                let main = built(source(&at.join("main")));
                let named = common::git::linked_worktree(&main, at.join("named"));
                std::fs::remove_file(named.join(".git")).unwrap();
                std::fs::rename(main.join(".git/worktrees/named"), named.join(".git")).unwrap();
                let common = main.join(".git");
                write_text(&named, ".git/commondir", &format!("{}\n", common.display()));
                named
            },
            "main",
        ),
    ];
    for (layout, lay_out, root) in layouts {
        assert_one_open_in(layout, lay_out, root, false, &["rev-parse --show-toplevel"]);
    }
}

/// Lay a committed project out as `lay_out` does, build it, edit a file, and read around the
/// other file from the checkout, under a git ceiling at the scratch directory when
/// `under_a_ceiling`: the store lies at `root` below the scratch directory, the read names the
/// checkout and its commit, answers from the checkout's own content where the build keeps a code
/// store, and its one open runs the diff and the untracked listing and the `asked` discovery
/// commands alone.
fn assert_one_open_in(
    layout: &str,
    lay_out: Layout,
    root: &str,
    under_a_ceiling: bool,
    asked: &[&str],
) {
    let scratch = tempfile::tempdir().unwrap();
    let checkout = lay_out(scratch.path());
    run_rigger_ok(&checkout, &["graph", "build"]);
    let root = scratch.path().join(root);
    assert!(
        root.join(".rigger").is_dir(),
        "{layout}: the store lies at {}",
        root.display()
    );
    write_text(&checkout, "src/other.rs", "pub fn gamma() {}\n");
    let ceiling = [("GIT_CEILING_DIRECTORIES", scratch.path().to_str().unwrap())];
    let envs: &[(&str, &str)] = if under_a_ceiling { &ceiling } else { &[] };
    let args = ["graph", "--around", "src/lib.rs", "--depth", "1"];
    let (out, traced) = traced_read(&checkout, &args, envs, &scratch.path().join("trace"));
    // A build with no code store holds no fact to answer with, so its label alone names the
    // checkout and the commit it read.
    let label = format!(
        "describes {} ({} + 1 file)",
        checkout.canonicalize().unwrap().display(),
        git_out(&checkout, &["rev-parse", "HEAD"])
    );
    assert!(
        out.starts_with(&label)
            && (!cfg!(feature = "symbols") || out.contains("src/lib.rs::alpha"))
            && !out.contains("enclosing"),
        "{layout}: the read answers from the checkout's own content:\n{out}"
    );
    let mut expected = one_open(&checkout);
    expected.extend(asked.iter().map(ToString::to_string));
    expected.sort();
    assert_eq!(
        git_commands(&traced),
        expected,
        "{layout}: one open runs the diff and the untracked listing and {asked:?} alone; \
         trace:\n{traced}"
    );
}

/// `checkout`, built.
fn built(checkout: PathBuf) -> PathBuf {
    run_rigger_ok(&checkout, &["graph", "build"]);
    checkout
}

/// A clone, beside it under `at`, of a [`source`] repository at `at/origin`, its git directory
/// kept apart beside it.
fn apart(at: &Path) -> PathBuf {
    source(&at.join("origin"));
    git_ok(
        at,
        &[
            "clone",
            "-q",
            "--separate-git-dir",
            "apart.git",
            "origin",
            "apart",
        ],
    );
    at.join("apart")
}

/// A [`source`] repository at `at/origin`, added as the submodule `sub` of a committed
/// repository at `at/outer` whose own `src/lib.rs` defines `enclosing`: the submodule's checkout.
fn submodule(at: &Path) -> PathBuf {
    let origin = source(&at.join("origin"));
    let outer = at.join("outer");
    std::fs::create_dir_all(&outer).unwrap();
    git_ok(&outer, &["init", "-q"]);
    write_text(&outer, "src/lib.rs", "pub fn enclosing() {}\n");
    let add = ["submodule", "add", "-q", origin.to_str().unwrap(), "sub"];
    git_ok(
        &outer,
        &[&["-c", "protocol.file.allow=always"][..], &add].concat(),
    );
    git_ok(&outer, &["add", "-A"]);
    git_ok_with_identity(&outer, &["commit", "-q", "-m", "outer"]);
    outer.join("sub")
}

/// A [`cloned`] checkout under `at` whose `.git` is a symlink to its git directory, moved beside
/// it.
fn dot_symlink(at: &Path) -> PathBuf {
    let clone = cloned(&at.join("origin"), at);
    std::fs::rename(clone.join(".git"), at.join("moved.git")).unwrap();
    std::os::unix::fs::symlink(at.join("moved.git"), clone.join(".git")).unwrap();
    clone
}

/// A repository at `at` with `src/lib.rs` defining `alpha` and `src/other.rs` defining `beta`,
/// committed.
fn source(at: &Path) -> PathBuf {
    std::fs::create_dir_all(at).unwrap();
    git_ok(at, &["init", "-q"]);
    write_text(at, "src/lib.rs", "pub fn alpha() {}\n");
    write_text(at, "src/other.rs", "pub fn beta() {}\n");
    git_ok(at, &["add", "-A"]);
    git_ok_with_identity(at, &["commit", "-q", "-m", "source"]);
    at.to_path_buf()
}

/// A clone, beside it under `at`, of a [`source`] repository laid out at `origin`.
fn cloned(origin: &Path, at: &Path) -> PathBuf {
    let clone = at.join("clone");
    let origin = source(origin);
    git_ok(
        at,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    clone
}

/// The git commands one open of a read from `checkout` runs, sorted: the worktree diff and the
/// untracked listing, and in a build with no code store, the head queries.
fn one_open(checkout: &Path) -> Vec<String> {
    let head = git_out(checkout, &["rev-parse", "HEAD"]);
    let mut expected = vec!["diff --raw".to_string(), "ls-files --others".into()];
    if !cfg!(feature = "symbols") {
        expected.extend([
            format!("rev-parse '{head}^{{commit}}'"),
            "rev-parse --verify".into(),
            "symbolic-ref --short".into(),
        ]);
    }
    expected.sort();
    expected
}

/// `rigger <args>` run from `cwd` under `envs`, traced by git into `trace`, which must answer:
/// its stdout and the trace.
fn traced_read(cwd: &Path, args: &[&str], envs: &[(&str, &str)], trace: &Path) -> (String, String) {
    let mut envs = envs.to_vec();
    envs.push(("GIT_TRACE", trace.to_str().unwrap()));
    let (out, err, ok) = run_rigger_envs(cwd, args, &envs);
    assert!(
        ok,
        "rigger {} answers from {}; stderr:\n{err}",
        args.join(" "),
        cwd.display()
    );
    (out, std::fs::read_to_string(trace).unwrap_or_default())
}

/// The git commands a `GIT_TRACE` log names, sorted: each git process names its command once, as
/// a built-in or as the program it runs, by its first two words.
fn git_commands(traced: &str) -> Vec<String> {
    let mut commands: Vec<String> = (traced.lines())
        .filter_map(|l| {
            let (_, rest) = l
                .split_once("trace: built-in: git ")
                .or_else(|| l.split_once("trace: exec: "))?;
            Some(rest.split(' ').take(2).collect::<Vec<_>>().join(" "))
        })
        .collect();
    commands.sort();
    commands
}
