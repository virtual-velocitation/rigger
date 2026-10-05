//! THE PRINCIPLE GATES ARE DEFINITION: the gates that hold every unit to the engineering
//! principles mechanically are declared in the workflow `rigger init` scaffolds for a consumer
//! project from the gate template set matching its language, each listed on the stages it
//! guards. That scaffolded workflow, written beside a root `Cargo.toml` from the Rust set, is the
//! one workflow this file reads, loaded through the production parser
//! (`rigger::config_store::load`), so a gate that is merely mentioned in a comment, or declared
//! but never listed on a stage, fails.

mod common;
use common::cli::{cargo_project, loaded_config, run_rigger};
use common::fixtures::{container_runtime, with_kurrentdb, TEST_CONTAINER_LABEL};
use common::git::{commit_files, git_ok, git_out, init_repo};
use common::repo::{repo_root, stub_path};
use common::shell_outcome;
use rigger::config::RIGGER_DIR;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// A fixture project holding a root `Cargo.toml`, after `rigger init` ran in it.
fn rust_project_after_init() -> tempfile::TempDir {
    let dir = cargo_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    dir
}

/// `rigger init` beside a root `Cargo.toml` writes the Rust gate template set's workflow: it loads
/// through `config_store::load`, declares exactly the Rust set's gates and lists them on the
/// implement and check-in stages in order - the one pin of the Rust set's gate ids and stage
/// lists - and the scaffolded personas carry every checklist line ([`PERSONA_CHECKLIST`]).
#[test]
fn a_scaffolded_rust_project_carries_the_rust_sets_gates_and_every_checklist_line() {
    let dir = rust_project_after_init();
    let cfg = loaded_config(dir.path());
    let wf = &cfg.workflow;
    assert_eq!(
        wf.gates.keys().map(String::as_str).collect::<Vec<_>>(),
        ["build", "fmt", "lint", "red-before-green", "test"],
        "the Rust set declares exactly its five gates"
    );
    assert_eq!(
        wf.stages["implement"].gates,
        ["fmt", "build", "test", "lint", "red-before-green"]
    );
    assert_eq!(wf.stages["checkin"].gates, ["fmt", "build", "test", "lint"]);
    let missing = missing_checklist_lines(dir.path());
    assert!(
        missing.is_empty(),
        "the scaffolded .rigger/agents: {missing:#?}"
    );
}

/// `rigger init` beside a root `Cargo.toml` writes every path the Rust set's `files` lists, each
/// with the bytes of this repository's copy.
#[test]
fn init_writes_each_file_the_rust_set_lists() {
    let dir = rust_project_after_init();
    let listed = std::fs::read_to_string(repo_root().join("scaffold/rust/files")).unwrap();
    let paths: Vec<&str> = listed.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        paths,
        [
            ".rigger/gates/red-before-green.sh",
            ".rigger/gates/mutation.sh",
            ".rigger/gates/container-env.sh",
        ],
        "the Rust set lists its three scripts"
    );
    for path in paths {
        let shipped = std::fs::read(repo_root().join(path))
            .unwrap_or_else(|e| panic!("the shipped {path}: {e}"));
        let scaffolded = std::fs::read(dir.path().join(path))
            .unwrap_or_else(|e| panic!("rigger init must write {path}: {e}"));
        assert_eq!(
            scaffolded, shipped,
            "the consumer gets the same {path}, byte for byte"
        );
    }
}

/// The fixture's committed source file: one function and a trailing test module.
const LIB: &str = "pub fn f() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn f_is_one() {\n        assert_eq!(super::f(), 1);\n    }\n}\n";

/// Makes `repo` a git repository whose `rigger-run` branch commits everything in it with `lib` as
/// its one source file, checked out on a unit branch that `commits` (each `(path, content,
/// message)`, in order) build on it. Returns each commit's short sha.
fn build_unit_branch(repo: &Path, lib: &str, commits: &[(&str, &str, &str)]) -> Vec<String> {
    init_repo(repo);
    commit_files(repo, &[("src/lib.rs", lib)], "base");
    git_ok(repo, &["branch", "rigger-run"]);
    git_ok(repo, &["checkout", "-q", "-b", "unit"]);
    commits
        .iter()
        .map(|(rel, content, msg)| {
            commit_files(repo, &[(*rel, *content)], msg);
            git_out(repo, &["rev-parse", "--short", "HEAD"])
        })
        .collect()
}

/// A fixture repository built by [`build_unit_branch`]. Returns it and each commit's short sha.
fn unit_branch_repo(lib: &str, commits: &[(&str, &str, &str)]) -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let shas = build_unit_branch(dir.path(), lib, commits);
    (dir, shas)
}

/// `sh` run with `args` from `repo`. Returns (passed, output).
fn run_sh(repo: &Path, args: &[&OsStr]) -> (bool, String) {
    shell_outcome(
        &Command::new("sh")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap(),
    )
}

/// The shipped red-before-green gate script run on `repo`'s unit branch against `rigger-run`.
/// Returns (passed, output).
fn run_red_before_green(repo: &Path) -> (bool, String) {
    let script = repo_root().join(".rigger/gates/red-before-green.sh");
    run_sh(repo, &[script.as_os_str()])
}

/// The `red-before-green` command the workflow `rigger init` scaffolds in a Rust fixture project,
/// run under `sh -c` from the project root as the gate runner runs it, on the unit branch that
/// `commits` build on [`LIB`]. Returns (passed, output) and each commit's short sha.
fn run_scaffolded_red_before_green(commits: &[(&str, &str, &str)]) -> (bool, String, Vec<String>) {
    let dir = rust_project_after_init();
    let cfg = loaded_config(dir.path());
    let command = &cfg.workflow.gates["red-before-green"].run;
    let shas = build_unit_branch(dir.path(), LIB, commits);
    let (passed, out) = run_sh(dir.path(), &["-c".as_ref(), command.as_ref()]);
    (passed, out, shas)
}

/// The gate's verdict failed the commit at index `offender` of `commits` (short shas `shas`),
/// naming it as the first source commit no test commit precedes.
fn assert_fails_naming(
    (passed, out): (bool, String),
    shas: &[String],
    commits: &[(&str, &str, &str)],
    offender: usize,
) {
    assert!(!passed, "the gate passed a test-less source commit: {out}");
    assert!(
        out.contains("error[red-before-green]")
            && out.contains(&shas[offender])
            && out.contains(commits[offender].2),
        "the failure must name the offending commit: {out}"
    );
}

const LIB_WITH_G: &str = "pub fn f() -> u8 {\n    1\n}\n\npub fn g() -> u8 {\n    2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn f_is_one() {\n        assert_eq!(super::f(), 1);\n    }\n}\n";

/// A fixture test file, committed on its own as the red half.
const TEST_FILE: &str = "#[test]\nfn g_is_two() {}\n";

/// A column-0 `#[cfg(test)]` on an import, heading a file: it opens no test module.
const TEST_IMPORT: &str = "#[cfg(test)]\nuse some::thing;\n\n";

/// A column-0 `#[cfg(test)]` on a module whose body lives in another file, heading a file: it
/// opens no test module in this one.
const TEST_MODULE_DECLARATION: &str = "#[cfg(test)]\nmod fixtures;\n\n";

/// `lib` with its test's assertion reworded: an edit inside the trailing test module.
fn with_an_edited_test(lib: &str) -> String {
    lib.replace(
        "assert_eq!(super::f(), 1);",
        "assert_eq!(super::f(), 1, \"f\");",
    )
}

/// The gate passes a unit branch that `commits` build on the fixture base `lib`.
fn red_before_green_passes(lib: &str, commits: &[(&str, &str, &str)]) {
    let (dir, _) = unit_branch_repo(lib, commits);
    let (passed, out) = run_red_before_green(dir.path());
    assert!(passed, "{out}");
}

/// The gate fails a unit branch that `commits` build on the fixture base `lib`, naming the
/// commit at index `offender` as the first source commit no test commit precedes.
fn red_before_green_fails_naming(lib: &str, commits: &[(&str, &str, &str)], offender: usize) {
    let (dir, shas) = unit_branch_repo(lib, commits);
    assert_fails_naming(run_red_before_green(dir.path()), &shas, commits, offender);
}

/// A source file holding one function and no test.
const MEMBER_LIB: &str = "pub fn g() -> u8 {\n    2\n}\n";

/// The scaffolded `red-before-green` command fails a test-less `src/` commit, naming it.
#[test]
fn the_scaffolded_red_before_green_command_fails_a_test_less_source_commit_naming_it() {
    let commits = [("src/lib.rs", LIB_WITH_G, "green without red")];
    let (passed, out, shas) = run_scaffolded_red_before_green(&commits);
    assert_fails_naming((passed, out), &shas, &commits, 0);
}

/// The scaffolded `red-before-green` command passes a branch whose test commit comes first.
#[test]
fn the_scaffolded_red_before_green_command_passes_a_test_commit_before_the_source_commit() {
    let (passed, out, _) = run_scaffolded_red_before_green(&[
        ("tests/g.rs", TEST_FILE, "red"),
        ("src/lib.rs", LIB_WITH_G, "green"),
    ]);
    assert!(passed, "{out}");
}

rigger::test_cases! {
    /// A test commit, then the source commit it drives.
    red_before_green_passes_a_test_commit_before_the_source_commit: red_before_green_passes(LIB, &[
        ("tests/g.rs", TEST_FILE, "red"),
        ("src/lib.rs", LIB_WITH_G, "green"),
    ]);
    /// One commit whose source change adds its own `#[test]`.
    red_before_green_passes_one_commit_that_carries_its_own_test: red_before_green_passes(LIB, &[(
        "src/lib.rs",
        &LIB_WITH_G.replace(
            "        assert_eq!(super::f(), 1);\n    }\n",
            "        assert_eq!(super::f(), 1);\n    }\n\n    #[test]\n    fn g_is_two() {\n        \
             assert_eq!(super::g(), 2);\n    }\n",
        ),
        "test and code together",
    )]);
    /// One commit whose source change also edits a test inside the trailing test module.
    red_before_green_counts_an_edit_inside_the_trailing_test_module_as_a_test:
        red_before_green_passes(LIB, &[(
            "src/lib.rs",
            &with_an_edited_test(LIB_WITH_G),
            "code with a changed test",
        )]);
    /// The same edit in a file whose first column-0 `#[cfg(test)]` gates an import: the test
    /// module still starts at the `#[cfg(test)]` that opens `mod tests`.
    red_before_green_counts_an_edit_inside_the_test_module_below_a_cfg_test_import:
        red_before_green_passes(&format!("{TEST_IMPORT}{LIB}"), &[(
            "src/lib.rs",
            &with_an_edited_test(&format!("{TEST_IMPORT}{LIB_WITH_G}")),
            "code with a changed test",
        )]);
    /// One commit whose code change adds a `#[test]` function outside any test module.
    red_before_green_counts_a_test_added_outside_any_test_module:
        red_before_green_passes(&format!("{TEST_IMPORT}{LIB}"), &[(
            "src/lib.rs",
            &format!(
                "{TEST_IMPORT}{}",
                LIB_WITH_G.replace(
                    "    2\n}\n",
                    "    2\n}\n\n#[test]\nfn g_is_two() {\n    assert_eq!(g(), 2);\n}\n",
                )
            ),
            "code with a test beside it",
        )]);
    /// One commit whose code change adds the file's first test module. Its async test carries
    /// no `#[test]`, so only the added module marks the commit as a test.
    red_before_green_counts_a_test_module_the_commit_adds:
        red_before_green_passes(&format!("{TEST_IMPORT}pub fn f() -> u8 {{\n    1\n}}\n"), &[(
            "src/lib.rs",
            &format!(
                "{TEST_IMPORT}{}",
                LIB_WITH_G.replace("#[test]\n    fn", "#[tokio::test]\n    async fn")
            ),
            "code with a new test module",
        )]);
    /// A branch that never touches source.
    red_before_green_passes_a_branch_with_no_source_commit: red_before_green_passes(LIB, &[(
        "docs/note.md",
        "a note\n",
        "docs only",
    )]);
    /// A source commit, then the test commit that should have preceded it.
    red_before_green_fails_a_source_commit_no_test_commit_precedes_naming_it:
        red_before_green_fails_naming(LIB, &[
            ("src/lib.rs", LIB_WITH_G, "green first"),
            ("tests/g.rs", TEST_FILE, "red after"),
        ], 0);
    /// A test-less code edit in a file whose first column-0 `#[cfg(test)]` gates an import: the
    /// edit sits below that line, yet outside the test module.
    red_before_green_fails_a_code_edit_below_a_cfg_test_import:
        red_before_green_fails_naming(&format!("{TEST_IMPORT}{LIB}"), &[(
            "src/lib.rs",
            &format!("{TEST_IMPORT}{LIB_WITH_G}"),
            "code below a test import",
        )], 0);
    /// The same edit below a column-0 `#[cfg(test)]` on a bodiless `mod fixtures;` declaration.
    red_before_green_fails_a_code_edit_below_a_cfg_test_module_declaration:
        red_before_green_fails_naming(&format!("{TEST_MODULE_DECLARATION}{LIB}"), &[(
            "src/lib.rs",
            &format!("{TEST_MODULE_DECLARATION}{LIB_WITH_G}"),
            "code below a test module declaration",
        )], 0);
    /// A test-less code edit that also adds a `#[cfg(test)]` import: an added `#[cfg(test)]`
    /// that opens no test module is not a test.
    red_before_green_fails_a_code_edit_that_adds_a_cfg_test_import:
        red_before_green_fails_naming(LIB, &[(
            "src/lib.rs",
            &format!("{TEST_IMPORT}{LIB_WITH_G}"),
            "code with a test import",
        )], 0);
    /// A test-less commit to a workspace member kept outside `crates/`: any file with a `src/`
    /// path component is source, whatever the layout.
    red_before_green_fails_a_test_less_commit_to_a_member_outside_crates:
        red_before_green_fails_naming(LIB, &[(
            "tools/x/src/lib.rs",
            MEMBER_LIB,
            "member code without a test",
        )], 0);
    /// A member's own test commit, then the member source it drives: a test under any `tests`
    /// directory counts, whatever the layout.
    red_before_green_passes_a_member_test_commit_before_the_member_source_commit:
        red_before_green_passes(LIB, &[
            ("tools/x/tests/g.rs", TEST_FILE, "member red"),
            ("tools/x/src/lib.rs", MEMBER_LIB, "member green"),
        ]);
    /// A test-less commit to a directory whose name only ends in `src`: source is a whole `src/`
    /// path component, so the branch touches no source.
    red_before_green_passes_a_commit_to_a_directory_whose_name_only_ends_in_src:
        red_before_green_passes(LIB, &[(
            "tools/xsrc/lib.rs",
            MEMBER_LIB,
            "not a source component",
        )]);
}

/// The review checklist line each persona carries for the principle gates, as `(agent file,
/// line)`: the lens names the principle, the adjudicator never trades a red gate away, the test
/// lens checks red before green.
const PERSONA_CHECKLIST: &[(&str, &str)] = &[
    (
        "architecture-reviewer.md",
        "Name the SOLID principle for each finding",
    ),
    (
        "adjudicator.md",
        "A red gate is non-negotiable: never weaken, skip or re-wire a gate to get green.",
    ),
    (
        "sdet.md",
        "Confirm the unit's first source commit follows a test commit (red before green).",
    ),
];

/// Every checklist line missing from the persona files under `root/.rigger/agents`.
fn missing_checklist_lines(root: &Path) -> Vec<String> {
    PERSONA_CHECKLIST
        .iter()
        .filter(|(file, line)| {
            let text =
                std::fs::read_to_string(root.join(".rigger/agents").join(file)).unwrap_or_default();
            !text.contains(line)
        })
        .map(|(file, line)| format!("{file}: {line:?}"))
        .collect()
}

#[test]
fn every_persona_carries_its_principle_gate_checklist_line() {
    let missing = missing_checklist_lines(&repo_root());
    assert!(missing.is_empty(), ".rigger/agents: {missing:#?}");
}

/// The container runtime snippet a gate that runs tests sources, relative to the worktree.
const CONTAINER_SNIPPET: &str = ".rigger/gates/container-env.sh";

/// The one listing the container runtime snippet asks for: every container carrying the label
/// key the test fixtures put on each container they start, running or not, so a container
/// without the label is never listed.
fn labelled_listing() -> String {
    [
        "ps -a --filter label=",
        TEST_CONTAINER_LABEL.0,
        " --format {{.ID}} {{.CreatedAt}}",
    ]
    .concat()
}

/// Sources the shipped container runtime snippet as a gate does, with a podman socket where
/// the snippet looks for one when `runtime`, and, when `cli`, a stand-in `podman` on PATH whose
/// labelled test containers are `fresh` (5 s old) and `old` (an hour old); without `cli` the
/// PATH holds no container CLI at all. `max_age` sets RIGGER_TEST_CONTAINER_MAX_AGE_S. Neither
/// DOCKER_HOST nor the test runner's cap RIGGER_TEST_AS_BYTES reaches the snippet. The snippet
/// must source cleanly; returns its output, one line per `podman` call, and the environment it
/// leaves exported to the tests the gate runs next.
fn source_container_snippet(
    runtime: bool,
    cli: bool,
    max_age: Option<&str>,
) -> (String, Vec<String>, BTreeMap<String, String>) {
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join("podman")).unwrap();
    let _socket = runtime.then(|| {
        std::os::unix::net::UnixListener::bind(work.path().join("podman/podman.sock")).unwrap()
    });
    let log = work.path().join("podman.log");
    let exported = work.path().join("exported.env");
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(". \"$0\" && /usr/bin/env -0 > \"$1\"")
        .arg(repo_root().join(CONTAINER_SNIPPET))
        .arg(&exported)
        .env(
            "PATH",
            stub_path(work.path(), "podman", cli.then_some("recording-podman.sh")),
        )
        .env("XDG_RUNTIME_DIR", work.path())
        .env("RECORDING_PODMAN_LOG", &log)
        .env("RECORDING_PODMAN_CONTAINERS", "fresh:5 old:3600")
        .env_remove("DOCKER_HOST")
        .env_remove("RIGGER_TEST_AS_BYTES");
    match max_age {
        Some(age) => cmd.env("RIGGER_TEST_CONTAINER_MAX_AGE_S", age),
        None => cmd.env_remove("RIGGER_TEST_CONTAINER_MAX_AGE_S"),
    };
    let (sourced, out) = shell_outcome(&cmd.output().unwrap());
    assert!(sourced, "the snippet must source cleanly: {out}");
    let calls = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let exported = std::fs::read_to_string(&exported)
        .unwrap()
        .split('\0')
        .filter_map(|var| var.split_once('='))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    (out, calls, exported)
}

/// Having found the podman socket, the snippet exports DOCKER_HOST at it and leaves the test
/// runner's per-process address-space cap as the caller left it (unset here, so the runner's own
/// default): the gate judges the container-backed tests under the same cap as a plain
/// `cargo test` run with DOCKER_HOST already set, never under a raised one.
#[test]
fn the_container_snippet_exports_the_found_socket_and_leaves_the_test_runner_cap_alone() {
    let (out, _, exported) = source_container_snippet(true, true, None);
    let socket = format!("unix://{}/podman/podman.sock", exported["XDG_RUNTIME_DIR"]);
    assert_eq!(exported.get("DOCKER_HOST"), Some(&socket), "{out}");
    assert_eq!(exported.get("RIGGER_TEST_AS_BYTES"), None, "{out}");
}

/// Once it has found the runtime, the snippet removes the labelled test containers an earlier
/// run left behind past the default max age of 600 s, and only those: a fresh one stays.
#[test]
fn the_container_snippet_removes_only_the_labelled_test_containers_past_their_age() {
    let (out, calls, _) = source_container_snippet(true, true, None);
    assert_eq!(calls, [labelled_listing().as_str(), "rm -f old"], "{out}");
}

/// With RIGGER_TEST_CONTAINER_MAX_AGE_S=0 every labelled test container goes, the fresh one too.
#[test]
fn the_container_snippet_removes_every_labelled_test_container_at_a_zero_max_age() {
    let (out, calls, _) = source_container_snippet(true, true, Some("0"));
    assert_eq!(
        calls,
        [labelled_listing().as_str(), "rm -f fresh", "rm -f old"],
        "{out}"
    );
}

/// The KurrentDB fixture puts the test label on the container it starts, so the snippet's
/// listing names that server and one a test ended by a signal left running is removed by the next
/// gate. The container is the one publishing the host port its connection string carries - a
/// port no other container on the host holds - read through the runtime the fixture reached.
/// Skips, as the fixture does, where no container runtime is reachable.
#[test]
fn the_kurrentdb_fixture_labels_its_container_for_the_snippet() {
    use testcontainers::bollard::query_parameters::ListContainersOptionsBuilder;
    use testcontainers::bollard::Docker;
    with_kurrentdb(|conn| {
        let port: u16 = conn
            .rsplit_once(':')
            .and_then(|(_, rest)| rest.split('?').next())
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("no host port in the connection string {conn}"));
        let containers = container_runtime()
            .block_on(async {
                let all = ListContainersOptionsBuilder::default().all(true).build();
                Docker::connect_with_defaults()?
                    .list_containers(Some(all))
                    .await
            })
            .expect("the container runtime lists its containers");
        let server = containers
            .iter()
            .find(|c| {
                c.ports
                    .iter()
                    .flatten()
                    .any(|p| p.public_port == Some(port))
            })
            .unwrap_or_else(|| panic!("no container publishes the server's port {port}"));
        let (key, value) = TEST_CONTAINER_LABEL;
        let labels = server.labels.clone().unwrap_or_default();
        assert_eq!(
            labels.get(key).map(String::as_str),
            Some(value),
            "the KurrentDB container {:?} must carry the label {key}={value}; its labels: {labels:?}",
            server.id
        );
    });
}

/// With no runtime found the snippet lists and removes nothing.
#[test]
fn the_container_snippet_removes_nothing_when_it_finds_no_runtime() {
    let (out, calls, _) = source_container_snippet(false, true, Some("0"));
    assert!(calls.is_empty(), "{out}{calls:?}");
}

/// A runtime with neither a podman nor a docker CLI on PATH costs one line of output, never
/// a failed gate.
#[test]
fn the_container_snippet_goes_on_without_a_container_cli() {
    let (out, _, _) = source_container_snippet(true, false, Some("0"));
    assert_eq!(out.lines().count(), 1, "{out}");
}

/// The committed implementer persona (`.rigger/agents/rust-engineer.md`), whitespace-
/// normalized (newlines and indentation collapsed to single spaces) so a pure reflow of a
/// wrapped paragraph never false-fails or false-passes a contiguous-phrase check.
fn implementer_persona_normalized() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(RIGGER_DIR)
        .join("agents")
        .join("rust-engineer.md");
    let persona = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed {}: {e}", path.display()));
    persona.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The committed implementer persona carries (`true`) or never carries (`false`) each
/// `(carried, clause, why)` contiguous clause.
fn assert_implementer_persona_pins(clauses: &[(bool, &str, &str)]) {
    let normalized = implementer_persona_normalized();
    for (carried, clause, why) in clauses {
        assert_eq!(
            normalized.contains(clause),
            *carried,
            "{why}; got:\n{normalized}"
        );
    }
}

rigger::test_cases! {
    /// Spec 91, criterion 3 (NO SWEEP IN THE LOOP). Supersedes
    /// `implementer_persona_pins_the_seeded_mutation_step_contract` (spec 73's persona pin) and
    /// `implementer_persona_pins_the_seeded_mutation_scratch_root_registration_contract` (spec
    /// 77's TMPDIR-registration pin) - both retired here: spec 91 Design decides "the
    /// implementer persona's mutation block is removed together with its unit.diff/TMPDIR
    /// choreography", so there is no more seeded per-round step, gating clause, or TMPDIR
    /// template to pin. The mutant accounting contract those tests protected now lives in
    /// this persona's prose for when it is spawned as the `checkin` stage, after every
    /// `implement` unit has already integrated and the `mutation` gate (spec 91 criterion 2)
    /// has already swept the whole spec diff once. This is a DRIFT GUARD, not a feature test: the
    /// implementer persona (`.rigger/agents/rust-engineer.md`) is OPERATOR CONFIGURATION
    /// seeded by the operator, not authored by any unit (spec 73 Design: "the grounder cannot
    /// ground non-code files, so no unit can own a Markdown blast radius").
    implementer_persona_pins_the_checkin_stage_survivor_closing_contract:
        assert_implementer_persona_pins(&[
        // One contiguous-phrase check, not two independently-satisfiable fragments: a
        // decomposed persona that keeps "checkin" and "stage" as bare substrings in unrelated
        // sentences (destroying the "this runs only when you are the checkin stage" gating
        // relation) must fail this test, not pass it.
        (
            true,
            "When you are spawned for the `checkin` stage",
            "the survivor-closing step must be gated on being spawned for the checkin stage, \
             as one contiguous clause, not two independently-satisfiable fragments",
        ),
        (
            true,
            "read `mutants.out/outcomes.json`",
            "the checkin stage must read the mutation gate's own outcomes file, never \
             stdout",
        ),
        // A survivor is always a failure (Byran 2026-09-26): one contiguous clause naming
        // the two ways it closes and the instrument narrowing that never closes it, so a
        // persona that re-admits a justification (an equivalence argument recorded as an
        // exclusion) fails this test.
        (
            true,
            "(surviving) mutant is always a failure: it is closed by a test that fails on it \
             or by rewriting the site so the mutable token disappears, never by an \
             `exclude_re` or `mutants::skip`",
            "a missed mutant closes only by a failing test or a rewrite, never by an \
             exclusion, as one contiguous clause",
        ),
        (
            false,
            "is either KILLED by a strengthened test or JUSTIFIED with a concrete \
             equivalence reason",
            "no justification closes a survivor: the kill-or-justify disjunction is gone",
        ),
        (
            true,
            "a miss still standing means the checkin stage is not done",
            "a missed mutant still standing must leave the checkin stage not done - the \
             consequence clause itself",
        ),
        // The ACCOUNTING shape (spec 73's deterministic per-mutant DecisionMade format): one
        // contiguous clause each for the id convention, the no-new-event-type + deterministic
        // ordering, the exhaustive status vocabulary (in order), and the empty-diff case - a
        // decomposed persona that keeps these as scattered bare words could satisfy
        // independent substring checks while dropping the actual shape a downstream consumer
        // parses against.
        (
            true,
            "record the accounting as one `<unit>-mutation-accounting`",
            "the accounting must be recorded under the deterministic <unit>-mutation- \
             accounting id (spec 73's shape)",
        ),
        (
            true,
            "DecisionMade (no new event type), deterministically ordered",
            "the accounting must be one DecisionMade, no new event type, deterministically \
             ordered",
        ),
        (
            true,
            "caught | missed-caught (naming the catching test) | unviable | timeout",
            "the accounting's per-mutant status vocabulary must be exhaustive and in this \
             order",
        ),
        (
            true,
            "A diff touching no Rust file records a provably-empty accounting",
            "an empty-diff checkin must still record a provably-empty accounting, never skip \
             the step",
        ),
        // The scope boundary itself (spec 91 Design: "Nothing mutation-specific enters the
        // conductor... no cargo-mutants path"): the agent must be told the `mutation` gate
        // owns running cargo-mutants, so it never re-invokes the sweep by hand.
        (
            true,
            "the `mutation` gate itself owns running cargo-mutants",
            "the persona must name the mutation gate as the sole cargo-mutants invoker, so \
             the agent never re-runs it by hand",
        ),
    ]);
    /// Spec 89, criterion 1 (A HALT NEVER DISCARDS A TREE): CHECKPOINT BEFORE LONG WORK.
    /// The persona must carry the checkpoint rule literally, using the design's own
    /// commit-message vocabulary ("mutation sweep", never the banned two-word invocation
    /// phrase "cargo mutants" - see `no_persona_under_rigger_agents_invokes_cargo_mutants`
    /// below, which spec 91 landed first and which this persona edit must not regress).
    implementer_persona_pins_the_checkpoint_before_long_work_contract:
        assert_implementer_persona_pins(&[
        // The trigger and the action as ONE contiguous clause - a decomposed persona
        // that keeps "mutation sweep" and "commit" as unrelated bare words (dropping
        // the "before long work, commit first" relation) must fail this test.
        (
            true,
            "Before a mutation sweep or any full lane suite, commit your current \
             tree",
            "the checkpoint rule must fire on EITHER a mutation sweep or a full lane \
             suite, as one contiguous clause",
        ),
        // The exact commit-message template spec 89 Design specifies, verbatim.
        (
            true,
            "`wip(<unit>): checkpoint before <mutation sweep | lane suite>`",
            "the checkpoint commit message template must be pinned verbatim",
        ),
        (
            true,
            "squash that checkpoint into your round's own commit \
             when you report",
            "the checkpoint must be squashed into the round commit on report, never \
             left standing as a separate commit",
        ),
        // Never the banned invocation phrase (spec 91): this persona edit must not
        // regress the already-landed no-cargo-mutants-invocation drift guard.
        (
            false,
            "cargo mutants",
            "the checkpoint rule must use the design's own vocabulary (\"mutation \
             sweep\"), never the literal invocation phrase \"cargo mutants\"",
        ),
    ]);
}

/// Spec 91, criterion 3 (NO SWEEP IN THE LOOP). The structural counterpart of
/// `implementer_persona_pins_the_checkin_stage_survivor_closing_contract` above: no persona
/// under `.rigger/agents/` - implementer, reviewer, or the SDET author - may INVOKE
/// `cargo mutants` itself any more. Only the `checkin` stage's `mutation` GATE (spec 91
/// criterion 2, `.rigger/workflow.yml`) runs that command now; a persona merely reading or
/// discussing its output (`mutants.out/outcomes.json`, or the noun "cargo-mutants") is
/// fine, so this checks for the two-word INVOCATION phrase specifically, never the bare
/// words "cargo" and "mutants" appearing anywhere in unrelated sentences.
#[test]
fn no_persona_under_rigger_agents_invokes_cargo_mutants() {
    let agents_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(RIGGER_DIR)
        .join("agents");
    let mut checked = 0;
    for entry in std::fs::read_dir(&agents_dir)
        .unwrap_or_else(|e| panic!("read committed {}: {e}", agents_dir.display()))
    {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read committed {}: {e}", path.display()));
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            !normalized.contains("cargo mutants"),
            "{} must never invoke `cargo mutants` itself - only the checkin stage's \
             `mutation` gate does now (spec 91); got:\n{normalized}",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 7,
        "expected to check every seeded persona file under {} (adjudicator, adversary, \
         architecture-reviewer, planner, rust-engineer, sdet, sdet-author, plus any \
         others); checked {checked}",
        agents_dir.display()
    );
}
