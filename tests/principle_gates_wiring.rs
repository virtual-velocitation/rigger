//! THE PRINCIPLE GATES ARE DEFINITION: the gates that hold every unit to the engineering
//! principles mechanically are declared in the workflow `rigger init` scaffolds for a consumer
//! project, each listed on the stages it guards. That scaffolded workflow is the one workflow this
//! file reads, loaded through the production parser (`rigger::config_store::load`), so a gate that
//! is merely mentioned in a comment, or declared but never listed on a stage, fails.

mod common;
use common::cli::{run_rigger, temp_project};
use common::fixtures::{container_runtime, with_kurrentdb, TEST_CONTAINER_LABEL};
use common::git::{commit_files, git_ok, git_out, init_repo};
use common::repo::{repo_root, stub_path};
use common::shell_outcome;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// One principle gate: its id, the stages that must list it, and the gates it must run before
/// wherever both are listed.
struct PrincipleGate {
    id: &'static str,
    stages: &'static [&'static str],
    precedes: &'static [&'static str],
}

const PRINCIPLE_GATES: &[PrincipleGate] = &[
    PrincipleGate {
        id: "boundary",
        stages: &["implement", "checkin"],
        precedes: &[],
    },
    // The audit gate regenerates `docs/audit/*` before it asserts, so it must run before the
    // `test` gate, whose plain `cargo test` includes the audit's drift guards: a unit whose
    // only audit difference is a stale generated catalog is then never red.
    PrincipleGate {
        id: "audit",
        stages: &["implement", "checkin"],
        precedes: &["test"],
    },
    PrincipleGate {
        id: "red-before-green",
        stages: &["implement"],
        precedes: &[],
    },
];

/// Every principle gate missing from the workflow at `root`: undeclared, or not listed on a stage
/// it guards.
fn missing_principle_gates(root: &Path) -> Vec<String> {
    let cfg = rigger::config_store::load(root.to_str().unwrap())
        .unwrap_or_else(|e| panic!("the workflow at {} must load: {e}", root.display()));
    let wf = &cfg.workflow;
    let mut missing = Vec::new();
    for gate in PRINCIPLE_GATES {
        if !wf.gates.contains_key(gate.id) {
            missing.push(format!("gate `{}` is not declared", gate.id));
        }
        for stage in gate.stages {
            let gates = wf
                .stages
                .get(*stage)
                .map(|s| s.gates.clone())
                .unwrap_or_default();
            let Some(at) = gates.iter().position(|g| g == gate.id) else {
                missing.push(format!("stage `{stage}` does not list gate `{}`", gate.id));
                continue;
            };
            for later in gate.precedes {
                if gates
                    .iter()
                    .position(|g| g == later)
                    .is_some_and(|l| l < at)
                {
                    missing.push(format!(
                        "stage `{stage}` runs gate `{later}` before gate `{}`",
                        gate.id
                    ));
                }
            }
        }
    }
    missing
}

/// A consumer project scaffolded by `rigger init` carries every principle gate on the stages it
/// guards and every persona checklist line ([`PERSONA_CHECKLIST`]).
#[test]
fn a_scaffolded_consumer_project_carries_every_principle_gate_and_checklist_line() {
    let dir = temp_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let mut missing = missing_principle_gates(dir.path());
    missing.extend(missing_checklist_lines(dir.path()));
    assert!(missing.is_empty(), "the scaffolded .rigger/: {missing:#?}");
}

/// The fixture's committed source file: one function and a trailing test module.
const LIB: &str = "pub fn f() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn f_is_one() {\n        assert_eq!(super::f(), 1);\n    }\n}\n";

/// A fixture repository whose `rigger-run` branch holds `lib` as its one committed source file,
/// checked out on a unit branch that `commits` (each `(path, content, message)`, in order) build
/// on it. Returns the repository and each commit's short sha.
fn unit_branch_repo(lib: &str, commits: &[(&str, &str, &str)]) -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);
    commit_files(repo, &[("src/lib.rs", lib)], "base");
    git_ok(repo, &["branch", "rigger-run"]);
    git_ok(repo, &["checkout", "-q", "-b", "unit"]);
    let shas = commits
        .iter()
        .map(|(rel, content, msg)| {
            commit_files(repo, &[(*rel, *content)], msg);
            git_out(repo, &["rev-parse", "--short", "HEAD"])
        })
        .collect();
    (dir, shas)
}

/// The shipped red-before-green gate script run on `repo`'s unit branch against `rigger-run`.
/// Returns (passed, output).
fn run_red_before_green(repo: &Path) -> (bool, String) {
    let script = repo_root().join(".rigger/gates/red-before-green.sh");
    let out = Command::new("sh")
        .arg(script)
        .current_dir(repo)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
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
    let (passed, out) = run_red_before_green(dir.path());
    assert!(!passed, "the gate passed a test-less source commit: {out}");
    assert!(
        out.contains("error[red-before-green]")
            && out.contains(&shas[offender])
            && out.contains(commits[offender].2),
        "the failure must name the offending commit: {out}"
    );
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
}

/// The review checklist line each persona carries for the principle gates, as `(agent file,
/// line)`: the lens names the principle, the adjudicator never trades a boundary red away, the
/// test lens checks red before green.
const PERSONA_CHECKLIST: &[(&str, &str)] = &[
    (
        "architecture-reviewer.md",
        "Name the SOLID principle for each finding",
    ),
    (
        "adjudicator.md",
        "A red `boundary` gate is non-negotiable: reject, never balance it against other evidence.",
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

/// The check-in mutation gate's logic lives in ONE shipped script, and `rigger init` writes the
/// identical file into a consumer project, so the sweep's bounds and scope reach every consumer
/// rather than living in this repository alone.
/// The script sources the container runtime snippet from beside itself, so `rigger init`
/// writes that file too, identical as well.
#[test]
fn the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script() {
    let dir = temp_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    for file in ["mutation.sh", "container-env.sh"] {
        let path = format!(".rigger/gates/{file}");
        let shipped = std::fs::read_to_string(repo_root().join(&path))
            .unwrap_or_else(|e| panic!("the shipped {path}: {e}"));
        let scaffolded = std::fs::read_to_string(dir.path().join(&path))
            .unwrap_or_else(|e| panic!("rigger init must write {path}: {e}"));
        assert_eq!(
            scaffolded, shipped,
            "the consumer gets the same {path}, byte for byte"
        );
    }
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
