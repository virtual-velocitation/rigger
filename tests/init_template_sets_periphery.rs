//! INIT SCAFFOLDS GATES AND THE IMPLEMENTER FROM THE PROJECT'S TEMPLATE SET (specs 113, 121 and
//! 125), driven through the built binary: what an operator sees `rigger init --template <dir>` and
//! `rigger setup --template <dir>` print with this repository's root as the template root, and
//! what each leaves on disk, in a project matching the Rust set, a project matching no set, a
//! project that already has a workflow, and on a rerun - the gates and the implementer persona the
//! implement and checkin stages run among it - that `rigger validate` accepts both fresh
//! scaffolds, that a command naming no template root scaffolds no gate, and that a template root
//! holding no `scaffold/` is refused before any write.

use crate::common;
use common::cli::{
    cargo_project, failure_line, loaded_config, refusal_line, run_rigger, run_rigger_ok,
    temp_project,
};
use common::fixtures::{
    agents_beside, sorted_dir_names, stage_agents, LANGUAGE_NEUTRAL_IMPLEMENTER,
};
use common::repo::{collect_files, repo_root};
use std::path::Path;

/// The line `rigger init` prints for a workflow written from the Rust set.
const RUST_WORKFLOW_LINE: &str = "scaffolded .rigger/workflow.yml (gate template set: rust)";

/// The line `rigger init` prints for a workflow written with no set.
const PLAIN_WORKFLOW_LINE: &str = "scaffolded .rigger/workflow.yml";

/// The no-set line, naming the markers of every set this repository's template root holds.
const NO_SET_LINE: &str = "no gate template set matches this project (rust: Cargo.toml at the \
                           project root), so .rigger/workflow.yml declares no gates - declare \
                           your own under gates:";

/// The no-root line: what a command naming no template root prints after the workflow line.
const NO_ROOT_LINE: &str = "no template root was given (--template <dir>), so \
                            .rigger/workflow.yml declares no gates - declare your own under gates:";

/// The line one written set file gets, for each path the Rust set lists, in listed order.
const RUST_SET_FILE_LINES: [&str; 1] = ["scaffolded .rigger/gates/red-before-green.sh"];

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

/// The repository's `scaffold/rust/set.yml`, the Rust set `--template <repository root>` loads.
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

/// This repository's root, as the `--template` argument naming it.
fn template_root() -> String {
    repo_root().to_str().unwrap().to_string()
}

/// `rigger init --template <repository root>` in `root`, returning its stdout.
fn init(root: &Path) -> String {
    run_rigger_ok(root, &["init", "--template", &template_root()])
}

/// `rigger setup --template <repository root>` in `root`, returning its stdout.
fn setup(root: &Path) -> String {
    run_rigger_ok(root, &["setup", "--template", &template_root()])
}

/// `rigger init` naming no template root, in `root`, returning its stdout.
fn init_with_no_root(root: &Path) -> String {
    run_rigger_ok(root, &["init"])
}

/// `rigger setup` naming no template root, in `root`, returning its stdout.
fn setup_with_no_root(root: &Path) -> String {
    run_rigger_ok(root, &["setup"])
}

/// A command run in a project root, returning its stdout.
type Command = fn(&Path) -> String;

/// The two unit stages' gate lists, `(implement, checkin)`.
fn stage_lists(cfg: &rigger::config::Config) -> (Vec<String>, Vec<String>) {
    (
        cfg.workflow.stages["implement"].gates.clone(),
        cfg.workflow.stages["checkin"].gates.clone(),
    )
}

/// Every file under `root` but its `.git/`, as root-relative paths in sorted order.
fn project_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    files
        .iter()
        .map(|path| path.strip_prefix(root).unwrap())
        .filter(|rel| !rel.starts_with(".git"))
        .map(|rel| rel.to_str().unwrap().to_string())
        .collect()
}

/// Asserts `out` and `root` show a gateless scaffold: the plain workflow line directly followed by
/// `why` - the no-set line, or the no-root line - no `.rigger/gates/`, and a workflow declaring no
/// gate with `[]` on both unit stages.
fn assert_gateless_scaffold(root: &Path, out: &str, why: &str) {
    assert_eq!(set_lines(out), [PLAIN_WORKFLOW_LINE, why], "stdout:\n{out}");
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == PLAIN_WORKFLOW_LINE)
        .unwrap();
    assert_eq!(
        lines.get(at + 1),
        Some(&why),
        "the line saying why no gate is declared follows the workflow line; stdout:\n{out}"
    );
    assert!(
        !root.join(".rigger/gates").exists(),
        "a gateless scaffold gets no gate script"
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
    let out = init(dir.path());
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
        "the workflow carries the set's gates text verbatim under gates:\n{workflow}"
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

/// Given a project holding `Cargo.toml`, when the operator runs `rigger init --template <this
/// repository's root>`, then it writes the Rust set's gates - `.rigger/gates/` holds exactly
/// `red-before-green.sh`, with the repository's bytes - and the Rust set's implementer, it writes
/// exactly the scaffold's own files, and no file it wrote holds `crates/<name>` for any directory
/// `<name>` under this repository's `crates/`: the template root lends its sets, never its own
/// layout.
#[test]
fn init_from_this_repositorys_root_writes_the_rust_set_and_nothing_of_its_crates() {
    let dir = cargo_project();
    let root = dir.path();
    let out = init(root);
    assert_eq!(
        set_lines(&out),
        [RUST_WORKFLOW_LINE, RUST_SET_FILE_LINES[0]],
        "stdout:\n{out}"
    );
    let script = ".rigger/gates/red-before-green.sh";
    assert_eq!(
        sorted_dir_names(&root.join(".rigger/gates")),
        ["red-before-green.sh"]
    );
    assert_eq!(
        std::fs::read(root.join(script)).unwrap(),
        std::fs::read(repo_root().join(script)).unwrap(),
        "{script} carries the repository's bytes"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".rigger/agents/rust-engineer.md")).unwrap(),
        rust_set_implementer(),
        "the Rust set's implementer"
    );

    let mut expected: Vec<String> = [
        ".claude/agents/lookup.md",
        ".claude/agents/verify.md",
        ".claude/settings.json",
        ".gitignore",
    ]
    .map(String::from)
    .to_vec();
    expected.extend(
        agents_beside("rust-engineer.md")
            .iter()
            .map(|file| format!(".rigger/agents/{file}")),
    );
    expected.extend(
        [
            script,
            ".rigger/instructions/README.md",
            ".rigger/project.id",
            ".rigger/workflow.yml",
            "Cargo.toml",
        ]
        .map(String::from),
    );
    let written = project_files(root);
    assert_eq!(written, expected, "the files the project holds after init");

    let crates: Vec<String> = std::fs::read_dir(repo_root().join("crates"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| format!("crates/{}", entry.file_name().to_str().unwrap()))
        .collect();
    assert_ne!(
        crates,
        Vec::<String>::new(),
        "this repository's crates/ holds the directories the check reads"
    );
    let hits: Vec<(&String, &String)> = written
        .iter()
        .flat_map(|rel| {
            let text =
                String::from_utf8_lossy(&std::fs::read(root.join(rel)).unwrap()).into_owned();
            crates
                .iter()
                .filter(move |needle| text.contains(needle.as_str()))
                .map(move |needle| (rel, needle))
        })
        .collect();
    assert_eq!(
        hits,
        Vec::<(&String, &String)>::new(),
        "no written file names a crate of the template root"
    );
}

/// Given a project matching no set, when the operator runs `rigger init`, then it prints the
/// plain workflow line followed directly by the no-set line, writes no `.rigger/gates/`, and the
/// workflow loads declaring no gate with `[]` on both unit stages - and `rigger validate`
/// accepts it, warning that the fan-out template runs ungated.
#[test]
fn init_matching_no_set_prints_the_no_set_line_and_writes_a_gateless_workflow() {
    let dir = temp_project();
    let out = init(dir.path());
    assert_gateless_scaffold(dir.path(), &out, NO_SET_LINE);

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

/// Given a project holding `Cargo.toml`, when the operator runs `rigger init` or `rigger setup`
/// naming no template root, then no set is loaded, so even the Rust marker matches nothing: it
/// prints the plain workflow line followed directly by the no-root line, writes no
/// `.rigger/gates/`, the workflow loads declaring no gate with `[]` on both unit stages, and the
/// implementer it seeds is the language-neutral one.
#[test]
fn a_command_naming_no_template_root_prints_the_no_root_line_and_writes_a_gateless_workflow() {
    let rows: [(&str, Command); 2] = [("init", init_with_no_root), ("setup", setup_with_no_root)];
    for (label, verb) in rows {
        let dir = cargo_project();
        let out = verb(dir.path());
        assert_gateless_scaffold(dir.path(), &out, NO_ROOT_LINE);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".rigger/agents/implementer.md")).unwrap(),
            LANGUAGE_NEUTRAL_IMPLEMENTER,
            "{label}: the language-neutral implementer, as with no set matched"
        );
    }
}

/// Given a template root holding no `scaffold/`, when the operator runs `rigger init --template
/// <root>` or `rigger setup --template <root>`, then the command fails on the refusal naming the
/// root's `scaffold/` and why it could not be read, before any write: the project holds exactly
/// the files it held before.
#[test]
fn a_template_root_holding_no_scaffold_dir_is_refused_before_any_write() {
    for command in ["init", "setup"] {
        let dir = cargo_project();
        let template = tempfile::tempdir().unwrap();
        let before = project_files(dir.path());
        let (out, err, ok) = run_rigger(
            dir.path(),
            &[command, "--template", template.path().to_str().unwrap()],
        );
        let unreadable = std::fs::read_dir(template.path().join("scaffold")).unwrap_err();
        assert_eq!(
            failure_line(ok, &err),
            Some(refusal_line(&format!(
                "{}/scaffold/: the gate template sets directory is unreadable: {unreadable}",
                template.path().display()
            ))),
            "{command}: stdout:\n{out}"
        );
        assert_eq!(out, "", "{command}: nothing is reported scaffolded");
        assert_eq!(
            project_files(dir.path()),
            before,
            "{command}: the project is left as it was"
        );
        assert_eq!(before, ["Cargo.toml"], "{command}");
    }
}

/// Given an initialized Rust project, a rerun prints only the already-initialized line and keeps
/// a listed file the operator rewrote; with that file deleted, a rerun writes it back with the
/// repository's bytes and reports exactly that one file; `rigger validate` accepts the scaffold.
#[test]
fn init_rerun_on_a_rust_project_writes_back_only_a_deleted_set_file() {
    let dir = cargo_project();
    init(dir.path());
    let script = ".rigger/gates/red-before-green.sh";
    let mine = dir.path().join(script);
    std::fs::write(&mine, "# my own gate\n").unwrap();

    assert_eq!(init(dir.path()), ALREADY_INITIALIZED);
    assert_eq!(
        std::fs::read_to_string(&mine).unwrap(),
        "# my own gate\n",
        "a present set file is kept"
    );

    std::fs::remove_file(&mine).unwrap();
    assert_eq!(
        init(dir.path()),
        "scaffolded .rigger/gates/red-before-green.sh\n"
    );
    assert_eq!(
        std::fs::read(&mine).unwrap(),
        std::fs::read(repo_root().join(script)).unwrap(),
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

    let out = init(dir.path());
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
    let out = setup(dir.path());
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

    let out = init(dir.path());
    assert_gateless_scaffold(dir.path(), &out, NO_SET_LINE);
}

/// Given a project matching no set, `rigger setup` scaffolds exactly as `rigger init` does: the
/// plain workflow line directly followed by the no-set line, no `.rigger/gates/`, and a workflow
/// declaring no gate with `[]` on both unit stages.
#[test]
fn setup_matching_no_set_prints_the_no_set_line_and_writes_a_gateless_workflow() {
    let dir = temp_project();
    let out = setup(dir.path());
    assert_gateless_scaffold(dir.path(), &out, NO_SET_LINE);
}

/// Given a project already initialized with no set matching, a rerun of `rigger init` prints only
/// the already-initialized line: the no-set line belongs to the run that wrote the workflow, so a
/// rerun that writes nothing never repeats it, and the workflow is left byte for byte.
#[test]
fn init_rerun_matching_no_set_does_not_repeat_the_no_set_line() {
    let dir = temp_project();
    init(dir.path());
    let workflow = dir.path().join(".rigger/workflow.yml");
    let before = std::fs::read(&workflow).unwrap();

    assert_eq!(init(dir.path()), ALREADY_INITIALIZED);
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
