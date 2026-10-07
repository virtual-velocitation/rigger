//! INIT SCAFFOLDS GATES FROM THE PROJECT'S TEMPLATE SET (spec 113), driven through the built
//! binary: what an operator sees `rigger init` and `rigger setup` print, and what each leaves on
//! disk, in a project matching the Rust set, a project matching no set, a project that already has
//! a workflow, and on a rerun - and that `rigger validate` accepts both fresh scaffolds.

mod common;
use common::cli::{
    cargo_project, loaded_config, run_rigger, run_rigger_envs, run_rigger_ok, temp_project,
};
use common::repo::repo_root;
use std::path::Path;

/// The line `rigger init` prints for a workflow written from the Rust set.
const RUST_WORKFLOW_LINE: &str = "scaffolded .rigger/workflow.yml (gate template set: rust)";

/// The line `rigger init` prints for a workflow written with no set.
const PLAIN_WORKFLOW_LINE: &str = "scaffolded .rigger/workflow.yml";

/// The no-set line, naming every embedded set's markers.
const NO_SET_LINE: &str = "no gate template set matches this project (rust: Cargo.toml at the \
                           project root), so .rigger/workflow.yml declares no gates - declare \
                           your own under gates:";

/// The line one written set file gets, for each path the Rust set lists, in listed order.
const RUST_SET_FILE_LINES: [&str; 3] = [
    "scaffolded .rigger/gates/red-before-green.sh",
    "scaffolded .rigger/gates/mutation.sh",
    "scaffolded .rigger/gates/container-env.sh",
];

/// What a rerun with nothing left to scaffold prints, in full.
const ALREADY_INITIALIZED: &str = "rigger init: already initialized; nothing to scaffold\n";

/// The lines of `out` that report the workflow, the gate template set, or a scaffolded path
/// under `.rigger/gates/`, in printed order - the slice of the init report this criterion owns.
fn set_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| {
            l.contains("workflow.yml")
                || l.contains("gate template set")
                || l.starts_with("scaffolded .rigger/gates/")
        })
        .collect()
}

/// The two unit stages' gate lists, `(implement, checkin)`.
fn stage_lists(cfg: &rigger::config::Config) -> (Vec<String>, Vec<String>) {
    (
        cfg.workflow.stages["implement"].gates.clone(),
        cfg.workflow.stages["checkin"].gates.clone(),
    )
}

/// Asserts `out` and `root` show a no-set scaffold: the plain workflow line directly followed by
/// the no-set line, no `.rigger/gates/`, and a workflow declaring no gate with `[]` on both unit
/// stages.
fn assert_no_set_scaffold(root: &Path, out: &str) {
    assert_eq!(
        set_lines(out),
        [PLAIN_WORKFLOW_LINE, NO_SET_LINE],
        "stdout:\n{out}"
    );
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == PLAIN_WORKFLOW_LINE)
        .unwrap();
    assert_eq!(
        lines[at + 1],
        NO_SET_LINE,
        "the no-set line follows the workflow line"
    );
    assert!(
        !root.join(".rigger/gates").exists(),
        "a project matching no set gets no gate script"
    );
    let cfg = loaded_config(root);
    assert_eq!(cfg.workflow.gates.len(), 0, "no gate declared");
    assert_eq!(stage_lists(&cfg), (vec![], vec![]));
}

/// Given a project with a root `Cargo.toml`, when the operator runs `rigger init`, then it names
/// the Rust set on the workflow line, reports every listed file in listed order, prints no no-set
/// line, and writes a workflow carrying the repository's `scaffold/rust/set.yml` gates text
/// verbatim, indented under `gates:`, whose every gate runs a real command.
#[test]
fn init_beside_a_root_cargo_toml_names_the_rust_set_and_reports_each_listed_file() {
    let dir = cargo_project();
    let out = run_rigger_ok(dir.path(), &["init"]);
    let mut expected = vec![RUST_WORKFLOW_LINE];
    expected.extend(RUST_SET_FILE_LINES);
    assert_eq!(set_lines(&out), expected, "stdout:\n{out}");

    let set: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(repo_root().join("scaffold/rust/set.yml")).unwrap(),
    )
    .unwrap();
    let gates_text = set["gates"].as_str().expect("the Rust set's gates text");
    let indented: String = gates_text
        .lines()
        .map(|line| format!("  {line}\n"))
        .collect();
    let workflow = std::fs::read_to_string(dir.path().join(".rigger/workflow.yml")).unwrap();
    assert!(
        workflow.contains(&format!("\ngates:\n{indented}")),
        "the workflow carries the embedded gates text verbatim under gates:\n{workflow}"
    );
    let cfg = loaded_config(dir.path());
    let runs: Vec<(&str, &str)> = cfg
        .workflow
        .gates
        .iter()
        .map(|(id, gate)| (id.as_str(), gate.run.as_str()))
        .collect();
    assert_eq!(
        runs,
        [
            ("build", "cargo build --workspace"),
            ("fmt", "cargo fmt --all --check"),
            (
                "lane-no-default",
                "cargo clippy --workspace --all-targets --no-default-features -- -D warnings && \
                 cargo test --workspace --no-default-features"
            ),
            (
                "lint",
                "cargo clippy --workspace --all-targets -- -D warnings"
            ),
            ("red-before-green", "sh .rigger/gates/red-before-green.sh"),
            ("test", "cargo test --workspace"),
        ],
        "every scaffolded gate runs a real command, none a placeholder"
    );
}

/// Given a project matching no set, when the operator runs `rigger init`, then it prints the
/// plain workflow line followed directly by the no-set line, writes no `.rigger/gates/`, and the
/// workflow loads declaring no gate with `[]` on both unit stages - and `rigger validate`
/// accepts it, warning that the fan-out template runs ungated.
#[test]
fn init_matching_no_set_prints_the_no_set_line_and_writes_a_gateless_workflow() {
    let dir = temp_project();
    let out = run_rigger_ok(dir.path(), &["init"]);
    assert_no_set_scaffold(dir.path(), &out);

    let (validated, warned, ok) = run_rigger(dir.path(), &["validate"]);
    assert!(
        ok,
        "rigger validate accepts the no-set scaffold; stderr:\n{warned}"
    );
    assert!(
        validated
            .lines()
            .any(|l| l == "config valid: 6 agents, 4 stages, 0 gates"),
        "stdout:\n{validated}"
    );
    assert!(
        warned.lines().any(|l| l
            == "warning: fan-out template 'implement' declares no gates - every unit it \
                decomposes into will run ungated; add a `gates:` list to the template if this is \
                unintended"),
        "stderr:\n{warned}"
    );
}

/// Given an initialized Rust project, a rerun prints only the already-initialized line and
/// rewrites no listed file; with one listed file deleted, a rerun writes it back with the
/// repository's bytes and reports exactly that one file; `rigger validate` accepts the scaffold.
#[test]
fn init_rerun_on_a_rust_project_writes_back_only_a_deleted_set_file() {
    let dir = cargo_project();
    run_rigger_ok(dir.path(), &["init"]);
    let mine = dir.path().join(".rigger/gates/red-before-green.sh");
    std::fs::write(&mine, "# my own gate\n").unwrap();

    assert_eq!(run_rigger_ok(dir.path(), &["init"]), ALREADY_INITIALIZED);
    assert_eq!(
        std::fs::read_to_string(&mine).unwrap(),
        "# my own gate\n",
        "a present set file is kept"
    );

    let mutation = ".rigger/gates/mutation.sh";
    std::fs::remove_file(dir.path().join(mutation)).unwrap();
    assert_eq!(
        run_rigger_ok(dir.path(), &["init"]),
        "scaffolded .rigger/gates/mutation.sh\n"
    );
    assert_eq!(
        std::fs::read(dir.path().join(mutation)).unwrap(),
        std::fs::read(repo_root().join(mutation)).unwrap(),
        "the rewritten file carries the repository's bytes"
    );

    let validated = run_rigger_ok(dir.path(), &["validate"]);
    assert!(
        validated
            .lines()
            .any(|l| l == "config valid: 6 agents, 4 stages, 6 gates"),
        "stdout:\n{validated}"
    );
}

/// Given a Rust project that already has its own workflow, when the operator runs `rigger init`,
/// then the workflow is kept byte for byte, no workflow, set or no-set line is printed, and the
/// Rust set's files are still written and reported.
#[test]
fn init_keeps_an_existing_workflow_and_still_writes_the_rust_sets_files() {
    let dir = cargo_project();
    let rigger_dir = dir.path().join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let own = "stages:\n  plan:\n    agent: planner\n";
    std::fs::write(rigger_dir.join("workflow.yml"), own).unwrap();

    let out = run_rigger_ok(dir.path(), &["init"]);
    assert_eq!(set_lines(&out), RUST_SET_FILE_LINES, "stdout:\n{out}");
    assert_eq!(
        std::fs::read_to_string(rigger_dir.join("workflow.yml")).unwrap(),
        own
    );
    for line in RUST_SET_FILE_LINES {
        let path = line.strip_prefix("scaffolded ").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(path)).unwrap(),
            std::fs::read(repo_root().join(path)).unwrap(),
            "{path} carries the repository's bytes"
        );
    }
}

/// Given a project with a root `Cargo.toml`, `rigger setup` scaffolds from the same template set
/// as `rigger init`: it names the Rust set, reports every listed file, writes the Rust set's
/// stage lists, and prints no no-set line.
#[test]
fn setup_beside_a_root_cargo_toml_scaffolds_from_the_rust_set() {
    let dir = cargo_project();
    let (out, err, ok) = run_rigger_envs(dir.path(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup failed:\nstdout:\n{out}\nstderr:\n{err}");
    let mut expected = vec![RUST_WORKFLOW_LINE];
    expected.extend(RUST_SET_FILE_LINES);
    assert_eq!(set_lines(&out), expected, "stdout:\n{out}");
    let cfg = loaded_config(dir.path());
    assert_eq!(
        stage_lists(&cfg),
        (
            [
                "fmt",
                "build",
                "test",
                "lane-no-default",
                "lint",
                "red-before-green"
            ]
            .map(String::from)
            .to_vec(),
            ["fmt", "build", "test", "lane-no-default", "lint"]
                .map(String::from)
                .to_vec(),
        )
    );
}

/// Given a project whose only `Cargo.toml` sits below the root (a member directory), when the
/// operator runs `rigger init`, then no set matches - the marker must be a file AT the project
/// root - so it prints the plain workflow line and the no-set line, writes no `.rigger/gates/`,
/// and the workflow declares no gate.
#[test]
fn init_with_a_cargo_toml_only_below_the_root_matches_no_set() {
    let dir = temp_project();
    let member = dir.path().join("member");
    std::fs::create_dir_all(&member).unwrap();
    std::fs::write(
        member.join("Cargo.toml"),
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let out = run_rigger_ok(dir.path(), &["init"]);
    assert_no_set_scaffold(dir.path(), &out);
}

/// Given a project matching no set, `rigger setup` scaffolds exactly as `rigger init` does: the
/// plain workflow line directly followed by the no-set line, no `.rigger/gates/`, and a workflow
/// declaring no gate with `[]` on both unit stages.
#[test]
fn setup_matching_no_set_prints_the_no_set_line_and_writes_a_gateless_workflow() {
    let dir = temp_project();
    let (out, err, ok) = run_rigger_envs(dir.path(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup failed:\nstdout:\n{out}\nstderr:\n{err}");
    assert_no_set_scaffold(dir.path(), &out);
}

/// Given a project already initialized with no set matching, a rerun of `rigger init` prints only
/// the already-initialized line: the no-set line belongs to the run that wrote the workflow, so a
/// rerun that writes nothing never repeats it, and the workflow is left byte for byte.
#[test]
fn init_rerun_matching_no_set_does_not_repeat_the_no_set_line() {
    let dir = temp_project();
    run_rigger_ok(dir.path(), &["init"]);
    let workflow = dir.path().join(".rigger/workflow.yml");
    let before = std::fs::read(&workflow).unwrap();

    assert_eq!(run_rigger_ok(dir.path(), &["init"]), ALREADY_INITIALIZED);
    assert_eq!(
        std::fs::read(&workflow).unwrap(),
        before,
        "a rerun keeps the workflow"
    );
}
