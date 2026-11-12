//! INIT SCAFFOLDS GATES AND THE IMPLEMENTER FROM THE PROJECT'S TEMPLATE SET (specs 113 and 121),
//! driven through the built binary: what an operator sees `rigger init` and `rigger setup` print,
//! and what each leaves on disk, in a project matching the Rust set, a project matching no set, a
//! project that already has a workflow, and on a rerun - the gates and the implementer persona the
//! implement and checkin stages run among it - and that `rigger validate` accepts both fresh
//! scaffolds.

use crate::common;
use common::cli::{
    cargo_project, loaded_config, run_rigger, run_rigger_ok, setup_ok, temp_project,
};
use common::fixtures::{sorted_dir_names, LANGUAGE_NEUTRAL_IMPLEMENTER};
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

/// The repository's `scaffold/rust/set.yml`, the Rust set the binary embeds.
fn rust_set() -> serde_yaml::Value {
    serde_yaml::from_str(
        &std::fs::read_to_string(repo_root().join("scaffold/rust/set.yml")).unwrap(),
    )
    .unwrap()
}

/// The persona the repository's Rust set carries under `implementer:`.
fn rust_set_implementer() -> String {
    rust_set()["implementer"]
        .as_str()
        .expect("the Rust set's implementer")
        .to_string()
}

/// `rigger init` in `root`, returning its stdout.
fn init(root: &Path) -> String {
    run_rigger_ok(root, &["init"])
}

/// `rigger setup` in `root`, with the npm step stubbed, returning its stdout.
fn setup(root: &Path) -> String {
    setup_ok(root, &[])
}

/// The two unit stages' agents, `[implement, checkin]`.
fn stage_agents(cfg: &rigger::config::Config) -> [String; 2] {
    [
        cfg.workflow.stages["implement"].agent.clone(),
        cfg.workflow.stages["checkin"].agent.clone(),
    ]
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

    let set = rust_set();
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
            .any(|l| l == "config valid: 6 agents, 4 stages, 5 gates"),
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
    let out = setup_ok(dir.path(), &[]);
    let mut expected = vec![RUST_WORKFLOW_LINE];
    expected.extend(RUST_SET_FILE_LINES);
    assert_eq!(set_lines(&out), expected, "stdout:\n{out}");
    let cfg = loaded_config(dir.path());
    assert_eq!(
        stage_lists(&cfg),
        (
            ["fmt", "build", "test", "lint", "red-before-green"]
                .map(String::from)
                .to_vec(),
            ["fmt", "build", "test", "lint"].map(String::from).to_vec(),
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
    let out = setup_ok(dir.path(), &[]);
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

/// The line a fresh scaffold matching no set prints for its agents: the seeds in seed order, the
/// language-neutral `implementer` among them.
const NO_SET_AGENTS_LINE: &str = "scaffolded .rigger/agents/{planner.md, implementer.md, \
                                  architecture-reviewer.md, sdet.md, adversary.md, adjudicator.md}";

/// The line a fresh scaffold from the Rust set prints for its agents: the role seeds in seed
/// order, then the set's `rust-engineer`.
const RUST_AGENTS_LINE: &str = "scaffolded .rigger/agents/{planner.md, architecture-reviewer.md, \
                                sdet.md, adversary.md, adjudicator.md, rust-engineer.md}";

/// The sorted files `.rigger/agents` holds when its implementer persona is `implementer`: the
/// role seeds and that file.
fn agents_beside(implementer: &str) -> Vec<String> {
    let mut files: Vec<String> = [
        "planner.md",
        "architecture-reviewer.md",
        "sdet.md",
        "adversary.md",
        "adjudicator.md",
        implementer,
    ]
    .map(String::from)
    .to_vec();
    files.sort();
    files
}

/// Given a project matching no set or the Rust set, when the operator runs `rigger init` or
/// `rigger setup`, then the implement and checkin stages both run the implementer the set brings -
/// the language-neutral `implementer` with no set, the Rust set's `rust-engineer` beside a root
/// `Cargo.toml` - whose persona is written as `<id>.md` byte for byte (the neutral seed, or the
/// one the repository's `scaffold/rust/set.yml` carries), the agents line names exactly the files
/// written, in seed order, and `.rigger/agents` holds those and no other implementer.
#[test]
fn a_fresh_scaffold_writes_and_names_the_implementer_its_gate_template_set_brings() {
    type Row = (
        &'static str,
        fn() -> tempfile::TempDir,
        fn(&Path) -> String,
        &'static str,
        fn() -> String,
        &'static str,
    );
    let rows: [Row; 4] = [
        (
            "init, no set",
            temp_project,
            init,
            "implementer",
            || LANGUAGE_NEUTRAL_IMPLEMENTER.to_string(),
            NO_SET_AGENTS_LINE,
        ),
        (
            "setup, no set",
            temp_project,
            setup,
            "implementer",
            || LANGUAGE_NEUTRAL_IMPLEMENTER.to_string(),
            NO_SET_AGENTS_LINE,
        ),
        (
            "init, Rust set",
            cargo_project,
            init,
            "rust-engineer",
            rust_set_implementer,
            RUST_AGENTS_LINE,
        ),
        (
            "setup, Rust set",
            cargo_project,
            setup,
            "rust-engineer",
            rust_set_implementer,
            RUST_AGENTS_LINE,
        ),
    ];
    for (label, project, verb, id, persona, agents_line) in rows {
        let dir = project();
        let out = verb(dir.path());
        assert_eq!(
            out.lines()
                .filter(|l| l.starts_with("scaffolded .rigger/agents/"))
                .collect::<Vec<_>>(),
            [agents_line],
            "{label}: stdout:\n{out}"
        );
        let agents = dir.path().join(".rigger/agents");
        assert_eq!(
            sorted_dir_names(&agents),
            agents_beside(&format!("{id}.md")),
            "{label}"
        );
        assert_eq!(
            std::fs::read_to_string(agents.join(format!("{id}.md"))).unwrap(),
            persona(),
            "{label}: the implementer's persona"
        );
        assert_eq!(
            stage_agents(&loaded_config(dir.path())),
            [id, id],
            "{label}: the implement and checkin stages run the implementer"
        );
    }
}

/// Given a project an earlier `rigger init` scaffolded matching no set, whose implement and checkin
/// stages run `rust-engineer` with a persona the operator owns and which holds no `implementer.md`,
/// when the operator reruns `rigger init` - as it is, or with a root `Cargo.toml` added since -
/// then the workflow and that persona stay byte for byte, no implementer is written, and the rerun
/// prints only what it wrote: nothing to scaffold, or the Rust set's files once its marker is there.
#[test]
fn a_rerun_keeps_an_earlier_scaffold_whose_unit_stages_run_rust_engineer() {
    let set_files: String = RUST_SET_FILE_LINES.map(|l| format!("{l}\n")).concat();
    for (label, marker, printed) in [
        ("as it is", None, ALREADY_INITIALIZED),
        ("a Cargo.toml added", Some("Cargo.toml"), set_files.as_str()),
    ] {
        let dir = temp_project();
        let root = dir.path();
        init(root);
        let workflow = root.join(".rigger/workflow.yml");
        let agents = root.join(".rigger/agents");
        let earlier = std::fs::read_to_string(&workflow)
            .unwrap()
            .replace("agent: implementer", "agent: rust-engineer");
        std::fs::write(&workflow, &earlier).unwrap();
        std::fs::remove_file(agents.join("implementer.md")).unwrap();
        let mine = "---\nid: rust-engineer\n---\nMine.\n";
        std::fs::write(agents.join("rust-engineer.md"), mine).unwrap();
        if let Some(marker) = marker {
            std::fs::write(root.join(marker), "").unwrap();
        }

        assert_eq!(init(root), printed, "{label}");
        assert_eq!(
            std::fs::read_to_string(&workflow).unwrap(),
            earlier,
            "{label}"
        );
        assert_eq!(
            std::fs::read_to_string(agents.join("rust-engineer.md")).unwrap(),
            mine,
            "{label}"
        );
        assert_eq!(
            sorted_dir_names(&agents),
            agents_beside("rust-engineer.md"),
            "{label}"
        );
        assert_eq!(
            stage_agents(&loaded_config(root)),
            ["rust-engineer", "rust-engineer"],
            "{label}"
        );
    }
}
