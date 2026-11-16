//! INIT SCAFFOLDS GATES AND THE IMPLEMENTER FROM THE PROJECT'S TEMPLATE SET (specs 113, 121 and
//! 125), driven through the built binary: what an operator sees `rigger init --template <dir>` and
//! `rigger setup --template <dir>` print, and what each leaves on disk, with this repository's
//! root or a template root the operator owns - named by an absolute path or a relative one - as
//! the template root, in a project matching a set, a project matching none, a project that already
//! has a workflow, and on a rerun - the gates, the build environment the workflow declares under
//! `build.env` and the implementer persona the implement and checkin stages run among it - that
//! `rigger validate` accepts both fresh scaffolds and reports that environment, that a fresh
//! scaffold bounds every spawn's wall clock so `rigger validate` prints no wall-clock warning over
//! it, that a fresh scaffold seeds the `sdet-author` test author its workflow declares and writes
//! no seed whose id an agent of the project already defines, whatever its file name, that a rerun
//! writes that seed back only for the test author the workflow declares and no agent defines, that
//! an agent file that does not parse stops every seed until it is fixed, that `rigger setup
//! --agents <dir>` imports a collection's agent in the place of the seed of its id and refuses a
//! collection that would define an id twice, that a command naming no template root scaffolds no
//! gate and keeps what an earlier root scaffolded, and that every argument and template root a
//! command refuses is refused before any write.

use crate::common;
use common::cli::{build_report_lines, UNDECLARED_BUILD_REPORT};
use common::cli::{
    cargo_project, failure_line, loaded_config, refusal_line, run_rigger, run_rigger_ok,
    temp_project, wall_clock_advisory_lines,
};
use common::cli::{config_valid_lines, fresh_scaffold_valid_line};
use common::fixtures::REFUSED_DECLARATIONS;
use common::fixtures::{
    agents_beside, dir_snapshot, sdet_author_seed, sorted_dir_names, stage_agents, write_files,
    LANGUAGE_NEUTRAL_IMPLEMENTER,
};
use common::fixtures::{build_declaring, RUST_SET_ENV, RUST_SET_ENV_WRITTEN};
use common::fixtures::{
    env_written_named, overlong_name_refusal, set_env_line_named, LONGEST_PLAIN_KEY,
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

/// The lines of `out` that report the agents written under `.rigger/agents/`, in printed order.
fn agents_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.starts_with("scaffolded .rigger/agents/"))
        .collect()
}

/// Each gate `cfg`'s workflow declares, as `(id, the command it runs)` in id order.
fn gate_runs(cfg: &rigger::config::Config) -> Vec<(&str, &str)> {
    cfg.workflow
        .gates
        .iter()
        .map(|(id, gate)| (id.as_str(), gate.run.as_str()))
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
    assert_eq!(
        gate_runs(&cfg),
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
        validated.lines().any(|l| l == fresh_scaffold_valid_line(0)),
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

/// A command writing a fresh scaffold: its label, the project it runs in, the command, the agents
/// line it prints and the number of gates the workflow it writes declares.
type FreshScaffold = (
    &'static str,
    fn() -> tempfile::TempDir,
    Command,
    &'static str,
    usize,
);

/// `rigger setup` naming no template root in a fresh project, `rigger init` naming this
/// repository's root in one matching no set and in one matching the Rust set beside a root
/// `Cargo.toml`, and `rigger setup` naming that root beside a root `Cargo.toml`.
fn fresh_scaffolds() -> [FreshScaffold; 4] {
    [
        (
            "setup, no template root",
            temp_project,
            setup_with_no_root,
            NO_SET_AGENTS_LINE,
            0,
        ),
        (
            "init, this repository's root, matching no set",
            temp_project,
            init,
            NO_SET_AGENTS_LINE,
            0,
        ),
        (
            "init, this repository's root, matching the Rust set",
            cargo_project,
            init,
            RUST_AGENTS_LINE,
            5,
        ),
        (
            "setup, this repository's root, matching the Rust set",
            cargo_project,
            setup,
            RUST_AGENTS_LINE,
            5,
        ),
    ]
}

/// Given a fresh project, when the operator runs `rigger setup` naming no template root, `rigger
/// init` naming this repository's root - matching no set, or the Rust set beside a root
/// `Cargo.toml` - or `rigger setup` naming that root beside a root `Cargo.toml`, then the written
/// workflow loads with `defaults.max_wall_clock` 3600 and `rigger validate` accepts it printing no
/// wall-clock warning: a new scaffold bounds every spawn, whatever its template root. `rigger init`
/// naming no template root is driven through `rigger validate` by `tests/cli.rs`'
/// `validate_warns_when_a_gating_roles_bound_resolves_to_zero`.
#[test]
fn a_fresh_scaffold_bounds_every_spawn_so_validate_prints_no_wall_clock_warning() {
    for (label, project, verb, ..) in fresh_scaffolds() {
        let dir = project();
        verb(dir.path());
        let (_out, err, ok) = run_rigger(dir.path(), &["validate"]);
        assert_eq!(
            (
                ok,
                wall_clock_advisory_lines(&err),
                loaded_config(dir.path()).workflow.defaults.max_wall_clock,
            ),
            (true, vec![], 3600),
            "{label}: validate accepts the scaffold, warns of no unbounded wall clock, and the \
             workflow bounds every spawn; stderr:\n{err}"
        );
    }
}

/// Given a fresh project - empty, or holding an agent of its own: its own `sdet-author.md`, a
/// persona defining `sdet-author` under another file name, or one defining `planner` under another
/// file name - when the operator runs `rigger init` naming no template root or any command of
/// [`fresh_scaffolds`], then the workflow declares `sdet-author` its test author and `rigger
/// validate` accepts the seven agents: the agents line names every seed whose id no agent of the
/// project defines, the `sdet-author` seed among them is written byte for byte and loaded, and the
/// project's own agent is kept byte for byte and loaded in the place of the seed of its id, which
/// is neither written nor named.
#[test]
fn a_fresh_scaffold_seeds_the_test_author_it_declares_and_keeps_a_projects_own() {
    let own_agent =
        |id: &str| format!("---\nid: {id}\nmodel: sonnet\n---\nThe project's own agent.\n");
    let no_root: FreshScaffold = (
        "init, no template root",
        temp_project,
        init_with_no_root,
        NO_SET_AGENTS_LINE,
        0,
    );
    for (label, project, verb, agents_line, gates) in fresh_scaffolds().into_iter().chain([no_root])
    {
        // (the agent file the project holds before the command and the id it defines, the seed
        // that agent keeps from being written, the sdet-author.md the project holds after the
        // command, the loaded models of `sdet-author` and `planner`).
        let arms = [
            (None, None, Some(sdet_author_seed()), ["opus", "opus"]),
            (
                Some(("sdet-author.md", "sdet-author")),
                Some("sdet-author.md"),
                Some(own_agent("sdet-author")),
                ["sonnet", "opus"],
            ),
            (
                Some(("tester.md", "sdet-author")),
                Some("sdet-author.md"),
                None,
                ["sonnet", "opus"],
            ),
            (
                Some(("my-planner.md", "planner")),
                Some("planner.md"),
                Some(sdet_author_seed()),
                ["opus", "sonnet"],
            ),
        ];
        for (own, displaced, author, models) in arms {
            let dir = project();
            let agents = dir.path().join(".rigger/agents");
            if let Some((file, id)) = own {
                write_files(&agents, &[(file, own_agent(id).as_str())]);
            }
            let out = verb(dir.path());
            let (validated, err, ok) = run_rigger(dir.path(), &["validate"]);
            let cfg = loaded_config(dir.path());
            let printed = displaced.map_or(agents_line.to_string(), |seed| {
                agents_line.replacen(&format!("{seed}, "), "", 1)
            });
            assert_eq!(
                (
                    agents_lines(&out),
                    std::fs::read_to_string(agents.join("sdet-author.md")).ok(),
                    own.map(|(file, _)| std::fs::read_to_string(agents.join(file)).unwrap()),
                    cfg.workflow.defaults.declared_test_author(),
                    ["sdet-author", "planner"]
                        .map(|id| cfg.agents.get(id).map(|a| a.model.as_str())),
                    ok,
                    config_valid_lines(&validated),
                ),
                (
                    vec![printed.as_str()],
                    author,
                    own.map(|(_, id)| own_agent(id)),
                    Some("sdet-author"),
                    models.map(Some),
                    true,
                    vec![fresh_scaffold_valid_line(gates).as_str()],
                ),
                "{label}, the project's own agent {own:?}: stdout:\n{out}\nstderr:\n{err}"
            );
        }
    }
}

/// Given a project `rigger init` scaffolded with no template root, which since holds a persona of
/// its own in `my-author.md` and no longer holds `sdet-author.md`, when the operator reruns
/// `rigger init`, then the seed is written back only for the test author the workflow declares
/// and no agent already defines: still declaring `sdet-author` beside a persona whose id is
/// `my-author`, the rerun writes the seed byte for byte and reports it, and `rigger validate`
/// accepts eight agents; declaring `my-author`, or declaring `sdet-author` beside a persona whose
/// id is `sdet-author` under its other file name, it writes nothing, prints only the
/// already-initialized line, and `rigger validate` accepts seven. `my-author.md` is kept every
/// way.
#[test]
fn a_rerun_writes_back_the_seed_only_for_the_test_author_the_workflow_declares() {
    // (the test author the workflow declares, the id my-author.md defines, the rerun's stdout but
    // its starter-fleet pointer, the sdet-author.md the project holds after it, the agents `rigger
    // validate` counts).
    let rows = [
        (
            "sdet-author",
            "my-author",
            vec!["scaffolded .rigger/agents/{sdet-author.md}"],
            Some(sdet_author_seed()),
            8,
        ),
        (
            "my-author",
            "my-author",
            vec![ALREADY_INITIALIZED.trim_end()],
            None,
            7,
        ),
        (
            "sdet-author",
            "sdet-author",
            vec![ALREADY_INITIALIZED.trim_end()],
            None,
            7,
        ),
    ];
    for (declared, id, printed, persona, agents) in rows {
        let mine = format!("---\nid: {id}\n---\nThe project's own test author.\n");
        let dir = temp_project();
        let root = dir.path();
        init_with_no_root(root);
        let workflow = root.join(".rigger/workflow.yml");
        let declaring = std::fs::read_to_string(&workflow).unwrap().replace(
            "test_author: sdet-author",
            &format!("test_author: {declared}"),
        );
        std::fs::write(&workflow, declaring).unwrap();
        let agents_dir = root.join(".rigger/agents");
        std::fs::remove_file(agents_dir.join("sdet-author.md")).unwrap();
        write_files(&agents_dir, &[("my-author.md", mine.as_str())]);

        let out = init_with_no_root(root);
        let (validated, err, ok) = run_rigger(root, &["validate"]);
        assert_eq!(
            (
                out.lines()
                    .filter(|l| !l.starts_with("next: "))
                    .collect::<Vec<_>>(),
                std::fs::read_to_string(agents_dir.join("sdet-author.md")).ok(),
                std::fs::read_to_string(agents_dir.join("my-author.md")).unwrap(),
                ok,
                config_valid_lines(&validated),
            ),
            (
                printed,
                persona,
                mine,
                true,
                vec![format!("config valid: {agents} agents, 4 stages, 0 gates").as_str()],
            ),
            "the workflow declaring {declared}, my-author.md defining {id}: stdout:\n{out}\n\
             stderr:\n{err}"
        );
    }
}

/// Given a fresh project holding an agent file that does not parse, when the operator runs `rigger
/// init` or `rigger setup` naming no template root, then the command fails before its first write,
/// printing only the line `rigger validate` prints for that file and leaving the project holding
/// that file alone; once the operator fixes the file, the same command writes every seed and
/// `rigger validate` accepts the eight agents.
#[test]
fn an_agent_file_that_does_not_parse_stops_every_seed_until_it_is_fixed() {
    const BROKEN: &str = "---\nid: broken\nrecurse: not-a-bool\n---\nThe project's own agent.\n";
    const FIXED: &str = "---\nid: broken\nrecurse: true\n---\nThe project's own agent.\n";
    const REFUSAL: &str = "rigger: config: broken.md: config: frontmatter: recurse: invalid \
                           type: string \"not-a-bool\", expected a boolean at line 2 column 10\n";
    for verb in ["init", "setup"] {
        let dir = temp_project();
        let root = dir.path();
        let agents = root.join(".rigger/agents");
        write_files(&agents, &[("broken.md", BROKEN)]);
        // The project's top level and its `.rigger/`, so a directory made and left empty in
        // either shows.
        let snapshot = || {
            (
                dir_snapshot(root, ""),
                dir_snapshot(&root.join(".rigger"), ""),
            )
        };
        let entries = snapshot();
        let (out, err, ok) = run_rigger(root, &[verb]);
        let left = (project_files(root), snapshot());
        let (_, refused, _) = run_rigger(root, &["validate"]);
        assert_eq!(
            (ok, out.as_str(), err.as_str(), refused.as_str(), left),
            (
                false,
                "",
                REFUSAL,
                REFUSAL,
                (vec![".rigger/agents/broken.md".to_string()], entries)
            ),
            "{verb} over an agent file that does not parse"
        );

        write_files(&agents, &[("broken.md", FIXED)]);
        let (out, err, ok) = run_rigger(root, &[verb]);
        let (validated, _, valid) = run_rigger(root, &["validate"]);
        assert_eq!(
            (
                ok,
                agents_lines(&out),
                valid,
                config_valid_lines(&validated)
            ),
            (
                true,
                vec![NO_SET_AGENTS_LINE],
                true,
                vec!["config valid: 8 agents, 4 stages, 0 gates"]
            ),
            "{verb} once the file is fixed: stdout:\n{out}\nstderr:\n{err}"
        );
    }
}

/// The collection directory, under a project's root, that `rigger setup --agents` imports.
const COLLECTION: &str = "collection";

/// Writes under `root`'s [`COLLECTION`] each `(file name, id)` of `agents` as an agent defining
/// that id on the `sonnet` model, so a loaded `sonnet` tells the collection's agent from a seed.
fn write_collection(root: &Path, agents: &[(&str, &str)]) {
    for (file, id) in agents {
        let agent = format!("---\nid: {id}\nmodel: sonnet\n---\nThe collection's agent.\n");
        write_files(&root.join(COLLECTION), &[(file, agent.as_str())]);
    }
}

/// The lines of `out` that report an agent `rigger setup --agents` imported or kept, and the
/// import's closing count, in printed order.
fn import_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.starts_with("imported ") || l.starts_with("kept existing "))
        .collect()
}

/// A row of [`setup_agents_imports_a_collections_agent_in_the_place_of_the_seed_of_its_id`]'s
/// table, whose fields that test names.
type ImportRow<'a> = (
    bool,
    &'a [(&'a str, &'a str)],
    Option<&'a str>,
    Option<&'a str>,
    [&'a str; 2],
);

/// Given a project - fresh, or one `rigger setup` scaffolded with no template root whose
/// `sdet-author.md` the operator has since deleted - and a collection holding an agent whose id a
/// seed carries, when the operator runs `rigger setup --agents <collection>`, then the collection's
/// agent is imported in the place of the seed of its id, under another file name or the seed's
/// own: that seed is neither written nor named, the import reports each file it wrote, and the
/// project loads with the collection's agent defining that id. A collection file under a seed's
/// name that defines another id is kept out, reported kept, and that seed is written.
#[test]
fn setup_agents_imports_a_collections_agent_in_the_place_of_the_seed_of_its_id() {
    // (the project before the import: true for one `rigger setup` scaffolded with its
    // `sdet-author.md` deleted since, false for a fresh one; the collection's agents as (file
    // name, id); the seed the import keeps from being written; the collection file it keeps out;
    // the loaded models of `sdet-author` and `planner`).
    let rows: [ImportRow<'_>; 5] = [
        (
            false,
            &[("test-author.md", "sdet-author")],
            Some("sdet-author.md"),
            None,
            ["sonnet", "opus"],
        ),
        (
            false,
            &[("my-planner.md", "planner")],
            Some("planner.md"),
            None,
            ["opus", "sonnet"],
        ),
        (
            false,
            &[("sdet-author.md", "sdet-author")],
            Some("sdet-author.md"),
            None,
            ["sonnet", "opus"],
        ),
        (
            false,
            &[("planner.md", "sdet-author")],
            None,
            Some("planner.md"),
            ["opus", "opus"],
        ),
        (
            true,
            &[("test-author.md", "sdet-author")],
            Some("sdet-author.md"),
            None,
            ["sonnet", "opus"],
        ),
    ];
    for (rerun, collection, displaced, kept, models) in rows {
        let dir = temp_project();
        let root = dir.path();
        let agents = root.join(".rigger/agents");
        if rerun {
            setup_with_no_root(root);
            std::fs::remove_file(agents.join("sdet-author.md")).unwrap();
        }
        write_collection(root, collection);

        let (out, err, ok) = run_rigger(root, &["setup", "--agents", COLLECTION]);
        let imported: Vec<(&str, &str)> = collection
            .iter()
            .copied()
            .filter(|(file, _)| Some(*file) != kept)
            .collect();
        let scaffolded = match (rerun, displaced) {
            (true, _) => Vec::new(),
            (false, None) => vec![NO_SET_AGENTS_LINE.to_string()],
            (false, Some(seed)) => vec![NO_SET_AGENTS_LINE.replacen(&format!("{seed}, "), "", 1)],
        };
        let mut reported: Vec<String> = kept
            .map(|file| format!("kept existing .rigger/agents/{file} (import never overwrites)"))
            .into_iter()
            .chain(
                imported
                    .iter()
                    .map(|(file, id)| format!("imported .rigger/agents/{file} (id: {id})")),
            )
            .collect();
        reported.push(format!(
            "imported {} agent {} from {COLLECTION} into .rigger/agents/ ({} kept - already \
             present)",
            imported.len(),
            if imported.len() == 1 { "file" } else { "files" },
            usize::from(kept.is_some()),
        ));
        let mut listing: Vec<String> = agents_beside("implementer.md")
            .into_iter()
            .filter(|file| Some(file.as_str()) != displaced)
            .chain(imported.iter().map(|(file, _)| file.to_string()))
            .collect();
        listing.sort();
        let cfg = loaded_config(root);
        assert_eq!(
            (
                failure_line(ok, &err),
                agents_lines(&out),
                import_lines(&out),
                sorted_dir_names(&agents),
                ["sdet-author", "planner"].map(|id| cfg.agents.get(id).map(|a| a.model.as_str())),
            ),
            (
                None,
                scaffolded.iter().map(String::as_str).collect(),
                reported.iter().map(String::as_str).collect(),
                listing,
                models.map(Some),
            ),
            "the collection {collection:?}, after a setup: {rerun}: stdout:\n{out}\nstderr:\n{err}"
        );
    }
}

/// Given a project - fresh, or one `rigger setup` scaffolded with no template root - and a
/// collection that would leave two agents defining `sdet-author` - two of its own files, or one
/// beside the project's seed of that id - when the operator runs `rigger setup --agents
/// <collection>`, then the command fails on the duplicate id and imports nothing: in the fresh
/// project before any write, leaving no `.rigger/`, and in the scaffolded one keeping exactly the
/// agents it held.
#[test]
fn setup_agents_refuses_a_collection_that_would_define_an_id_twice() {
    let refusal = refusal_line("config: duplicate agent id \"sdet-author\"");
    // (the project before the import: true for one `rigger setup` scaffolded, false for a fresh
    // one; the collection's agents as (file name, id)).
    let rows: [(bool, &[(&str, &str)]); 2] = [
        (false, &[("a.md", "sdet-author"), ("b.md", "sdet-author")]),
        (true, &[("test-author.md", "sdet-author")]),
    ];
    for (rerun, collection) in rows {
        let dir = temp_project();
        let root = dir.path();
        if rerun {
            setup_with_no_root(root);
        }
        write_collection(root, collection);

        let (out, err, ok) = run_rigger(root, &["setup", "--agents", COLLECTION]);
        let agents = root.join(".rigger/agents");
        assert_eq!(
            (
                failure_line(ok, &err),
                import_lines(&out),
                root.join(".rigger").exists(),
                agents.exists().then(|| sorted_dir_names(&agents)),
            ),
            (
                Some(refusal.clone()),
                Vec::<&str>::new(),
                rerun,
                rerun.then(|| agents_beside("implementer.md")),
            ),
            "the collection {collection:?}, after a setup: {rerun}: stdout:\n{out}\nstderr:\n{err}"
        );
    }
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

/// Runs `rigger <args>` in a fresh project holding only `Cargo.toml` and asserts it fails on
/// `refusal` before any write: nothing is reported scaffolded, the project holds exactly the
/// files it held before, and its top-level entries - a directory made and left empty among
/// them - are exactly those it held before.
fn assert_refused_before_any_write(args: &[&str], refusal: &str) {
    let dir = cargo_project();
    let before = project_files(dir.path());
    let entries = dir_snapshot(dir.path(), "");
    let (out, err, ok) = run_rigger(dir.path(), args);
    assert_eq!(
        failure_line(ok, &err),
        Some(refusal_line(refusal)),
        "{args:?}: stdout:\n{out}"
    );
    assert_eq!(out, "", "{args:?}: nothing is reported scaffolded");
    assert_eq!(
        project_files(dir.path()),
        before,
        "{args:?}: the project is left as it was"
    );
    assert_eq!(before, ["Cargo.toml"], "{args:?}");
    assert_eq!(
        dir_snapshot(dir.path(), ""),
        entries,
        "{args:?}: no directory is made"
    );
}

/// Given a template root holding no `scaffold/`, when the operator runs `rigger init --template
/// <root>` or `rigger setup --template <root>`, then the command fails on the refusal naming the
/// root's `scaffold/` as typed - an absolute path, or a relative one as the operator spelled it -
/// and why it could not be read, before any write.
#[test]
fn a_template_root_holding_no_scaffold_dir_is_refused_before_any_write() {
    let template = tempfile::tempdir().unwrap();
    let absolute = template.path().to_str().unwrap();
    let unreadable = std::fs::read_dir(template.path().join("scaffold")).unwrap_err();
    for (args, shown) in [
        (["init", "--template", absolute], absolute),
        (["setup", "--template", absolute], absolute),
        (["init", "--template", "nowhere"], "nowhere"),
    ] {
        assert_refused_before_any_write(
            &args,
            &format!(
                "{shown}/scaffold/: the gate template sets directory is unreadable: {unreadable}"
            ),
        );
    }
}

/// Given a command line `rigger init` or `rigger setup` refuses - `--template` with no directory,
/// `--agents` given to `init`, or a template root whose one set brings an implementer reusing a
/// scaffold seed's id, declares under `env` an entry the build environment's check refuses, or
/// declares under `env` a name one character past the longest key YAML reads plain, so the
/// workflow it scaffolds fails to load - when the operator runs it, then the command fails on the
/// refusal naming what is wrong - for a set, the set's key, for a refused `env` entry, the check's
/// own refusal naming the variable after it, and for the overlong name, the load error at the
/// line a scaffold writes its set's `env` on - before any write: nothing is reported scaffolded
/// and the project is left as it was, with no directory made.
#[test]
fn an_argument_or_a_set_the_command_refuses_writes_nothing() {
    let seeded = operator_template_root("implementer");
    let seeded = seeded.path().to_str().unwrap();
    let seed_id = "gate template set demo: implementer id implementer is a scaffold seed's id";
    // Each refused `env`'s template root, kept alive for the rows below, and its refusal.
    let declaring: Vec<(tempfile::TempDir, String)> = REFUSED_DECLARATIONS
        .iter()
        .map(|refused| {
            (
                operator_template_root_declaring(&refused.set_env_line()),
                format!("gate template set demo: env: {}", refused.message),
            )
        })
        .chain([(
            operator_template_root_declaring(&set_env_line_named(LONGEST_PLAIN_KEY + 1)),
            format!(
                "gate template set demo: {}",
                overlong_name_refusal(scaffolded_env_line())
            ),
        )])
        .collect();
    let env_rows: Vec<([&str; 3], &str)> = declaring
        .iter()
        .flat_map(|(template, refusal)| {
            let template = template.path().to_str().unwrap();
            ["init", "setup"].map(|command| ([command, "--template", template], refusal.as_str()))
        })
        .collect();
    let rows: [(&[&str], &str); 5] = [
        (
            &["init", "--template"],
            "init: --template needs a directory argument",
        ),
        (
            &["setup", "--template"],
            "setup: --template needs a directory argument",
        ),
        (
            &["init", "--agents", "agents"],
            "init: unknown argument \"--agents\"",
        ),
        (&["init", "--template", seeded], seed_id),
        (&["setup", "--template", seeded], seed_id),
    ];
    let env_rows = env_rows
        .iter()
        .map(|(args, refusal)| (args.as_slice(), *refusal));
    for (args, refusal) in rows.into_iter().chain(env_rows) {
        assert_refused_before_any_write(args, refusal);
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
        validated.lines().any(|l| l == fresh_scaffold_valid_line(5)),
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
                                  architecture-reviewer.md, sdet.md, sdet-author.md, adversary.md, \
                                  adjudicator.md}";

/// The line a fresh scaffold from the Rust set prints for its agents: the role seeds in seed
/// order, then the set's `rust-engineer`.
const RUST_AGENTS_LINE: &str = "scaffolded .rigger/agents/{planner.md, architecture-reviewer.md, \
                                sdet.md, sdet-author.md, adversary.md, adjudicator.md, \
                                rust-engineer.md}";

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
        assert_eq!(agents_lines(&out), [agents_line], "{label}: stdout:\n{out}");
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

/// The id of the implementer the operator's own set brings.
const DEMO_IMPLEMENTER_ID: &str = "demo-engineer";

/// The script the operator's own set lists, at the path it is read from and written to.
const DEMO_CHECK: &str = ".rigger/gates/check.sh";

/// The bytes of [`DEMO_CHECK`] in the operator's template root.
const DEMO_CHECK_BYTES: &str = "#!/bin/sh\n# the operator's own check\nexit 0\n";

/// The workflow line a scaffold from the operator's own set prints.
const DEMO_WORKFLOW_LINE: &str = "scaffolded .rigger/workflow.yml (gate template set: demo)";

/// The line the operator's one listed script gets when it is written.
const DEMO_CHECK_LINE: &str = "scaffolded .rigger/gates/check.sh";

/// The agents line a fresh scaffold from the operator's own set prints: the role seeds in seed
/// order, then the set's implementer.
const DEMO_AGENTS_LINE: &str = "scaffolded .rigger/agents/{planner.md, architecture-reviewer.md, \
                                sdet.md, sdet-author.md, adversary.md, adjudicator.md, \
                                demo-engineer.md}";

/// The no-set line over the operator's template root, naming its one set's two markers and no
/// set of this repository's.
const DEMO_NO_SET_LINE: &str = "no gate template set matches this project (demo: demo.marker or \
                                demo.lock at the project root), so .rigger/workflow.yml declares \
                                no gates - declare your own under gates:";

/// The persona an implementer with `id` carries in the operator's own set.
fn demo_implementer(id: &str) -> String {
    format!("---\nid: {id}\n---\nYou build the demo project.\n")
}

/// The operator's own `scaffold/demo/set.yml`, its implementer carrying `id`: two markers, one
/// gate running [`DEMO_CHECK`] on both unit stages.
fn demo_set(id: &str) -> String {
    let persona: String = demo_implementer(id)
        .lines()
        .map(|line| format!("  {line}\n"))
        .collect();
    format!(
        "detect: [demo.marker, demo.lock]\ngates: |\n  check: {{ run: \"sh {DEMO_CHECK}\", kind: \
         core }}\nimplement: [check]\ncheckin: [check]\nimplementer: |\n{persona}"
    )
}

/// Write under `root` a template root of the operator's own: one set, `demo`, its implementer
/// carrying `id`, listing [`DEMO_CHECK`], which the root holds.
fn write_operator_root(root: &Path, id: &str) {
    let set = demo_set(id);
    let listed = format!("{DEMO_CHECK}\n");
    write_files(
        root,
        &[
            ("scaffold/demo/set.yml", set.as_str()),
            ("scaffold/demo/files", listed.as_str()),
            (DEMO_CHECK, DEMO_CHECK_BYTES),
        ],
    );
}

/// A fresh template root of the operator's own ([`write_operator_root`]).
fn operator_template_root(id: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_operator_root(dir.path(), id);
    dir
}

/// A fresh template root of the operator's own ([`operator_template_root`], its implementer
/// [`DEMO_IMPLEMENTER_ID`]) whose one set's `set.yml` ends with `env_line`, its `env:` line.
fn operator_template_root_declaring(env_line: &str) -> tempfile::TempDir {
    let dir = operator_template_root(DEMO_IMPLEMENTER_ID);
    let set = format!("{}{env_line}", demo_set(DEMO_IMPLEMENTER_ID));
    write_files(dir.path(), &[("scaffold/demo/set.yml", set.as_str())]);
    dir
}

/// The line, counted from 1, that a scaffold writes its set's `env` on: where the workflow a fresh
/// `rigger init` writes, applying an operator's set whose `env` name is the longest key YAML reads
/// plain, holds that `env` under `build:`.
fn scaffolded_env_line() -> usize {
    let template = operator_template_root_declaring(&set_env_line_named(LONGEST_PLAIN_KEY));
    let dir = demo_project();
    run_rigger_ok(
        dir.path(),
        &["init", "--template", template.path().to_str().unwrap()],
    );
    let workflow = std::fs::read_to_string(dir.path().join(".rigger/workflow.yml")).unwrap();
    let env = format!("  env: {}", env_written_named(LONGEST_PLAIN_KEY));
    1 + workflow
        .lines()
        .position(|line| line == env)
        .unwrap_or_else(|| panic!("no line {env:?} in the workflow:\n{workflow}"))
}

/// Given a project holding the operator's own set's marker and no `Cargo.toml`, when the operator
/// runs `rigger init` or `rigger setup` naming a template root of their own - by an absolute path,
/// or by a path relative to the directory the command runs in, that directory below the git root
/// for `init` - then it names that root's set on the workflow line and reports its one listed
/// script, `.rigger/gates/` holds exactly that script with the root's bytes, the set's implementer
/// is written byte for byte beside the role seeds, and the workflow declares the set's one gate on
/// both unit stages, run by that implementer.
#[test]
fn init_and_setup_apply_a_set_from_a_template_root_the_operator_owns() {
    // (command, the directory below the git root it runs in, whether the root is relative).
    let rows: [(&str, Option<&str>, bool); 5] = [
        ("init", None, false),
        ("init", None, true),
        ("init", Some("member"), true),
        ("setup", None, false),
        ("setup", None, true),
    ];
    for (command, below, relative) in rows {
        let label = format!("{command} in {below:?}, relative root: {relative}");
        let dir = temp_project();
        let root = &below.map_or(dir.path().to_path_buf(), |sub| dir.path().join(sub));
        write_files(root, &[("demo.marker", "")]);
        let elsewhere = tempfile::tempdir().unwrap();
        let template = if relative {
            write_operator_root(&root.join("templates"), DEMO_IMPLEMENTER_ID);
            "templates".to_string()
        } else {
            write_operator_root(elsewhere.path(), DEMO_IMPLEMENTER_ID);
            elsewhere.path().to_str().unwrap().to_string()
        };

        let out = run_rigger_ok(root, &[command, "--template", &template]);
        assert_eq!(
            set_lines(&out),
            [DEMO_WORKFLOW_LINE, DEMO_CHECK_LINE],
            "{label}: stdout:\n{out}"
        );
        assert_eq!(
            agents_lines(&out),
            [DEMO_AGENTS_LINE],
            "{label}: stdout:\n{out}"
        );
        assert_eq!(
            sorted_dir_names(&root.join(".rigger/gates")),
            ["check.sh"],
            "{label}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(DEMO_CHECK)).unwrap(),
            DEMO_CHECK_BYTES,
            "{label}: the script carries the template root's bytes"
        );
        let agents = root.join(".rigger/agents");
        let persona = format!("{DEMO_IMPLEMENTER_ID}.md");
        assert_eq!(
            std::fs::read_to_string(agents.join(&persona)).unwrap(),
            demo_implementer(DEMO_IMPLEMENTER_ID),
            "{label}: the set's implementer"
        );
        assert_eq!(
            sorted_dir_names(&agents),
            agents_beside(&persona),
            "{label}"
        );

        let cfg = loaded_config(root);
        assert_eq!(
            gate_runs(&cfg),
            [("check", "sh .rigger/gates/check.sh")],
            "{label}"
        );
        assert_eq!(
            stage_lists(&cfg),
            (vec!["check".to_string()], vec!["check".to_string()]),
            "{label}"
        );
        assert_eq!(
            stage_agents(&cfg),
            [DEMO_IMPLEMENTER_ID, DEMO_IMPLEMENTER_ID],
            "{label}"
        );
    }
}

/// Given a project holding `Cargo.toml` and none of the operator's set's markers, when the
/// operator runs `rigger init` naming a template root of their own, then the no-set line names
/// that root's set and its markers alone - never this repository's Rust set - and the workflow
/// declares no gate.
#[test]
fn a_project_matching_none_of_the_operators_sets_gets_the_no_set_line_naming_only_them() {
    let dir = cargo_project();
    let template = operator_template_root(DEMO_IMPLEMENTER_ID);
    let out = run_rigger_ok(
        dir.path(),
        &["init", "--template", template.path().to_str().unwrap()],
    );
    assert_gateless_scaffold(dir.path(), &out, DEMO_NO_SET_LINE);
}

/// Given a project an earlier `rigger init` scaffolded from this repository's Rust set, which
/// also holds the operator's own set's marker - its Rust set script kept, or deleted since - when
/// the operator runs a later command - `rigger init` or `rigger setup` naming no template root, or
/// `rigger init` naming a template root of their own - then the workflow and the agents stay byte
/// for byte, none added: a command naming no root prints no set line and writes no set file, so
/// `.rigger/gates/` holds exactly the scripts it held, a deleted one staying deleted, and one
/// naming another root writes and reports only the script that root lists and the project lacks.
#[test]
fn a_later_command_keeps_what_an_earlier_root_scaffolded() {
    let template = operator_template_root(DEMO_IMPLEMENTER_ID);
    let operator_root = template.path().to_str().unwrap();
    let check_only = format!("{DEMO_CHECK_LINE}\n");
    let rust_script = ".rigger/gates/red-before-green.sh";
    let rust_bytes = std::fs::read(repo_root().join(rust_script)).unwrap();
    let kept: &[(&str, &[u8])] = &[("red-before-green.sh", rust_bytes.as_slice())];
    // (command line, whether the Rust set's script is deleted first, its set lines, its whole
    // stdout where it prints nothing else, each script `.rigger/gates/` holds after it with its
    // bytes).
    type Row<'a> = (
        &'a [&'a str],
        bool,
        &'a [&'a str],
        Option<&'a str>,
        &'a [(&'a str, &'a [u8])],
    );
    let rows: [Row; 5] = [
        (&["init"], false, &[], Some(ALREADY_INITIALIZED), kept),
        (&["init"], true, &[], Some(ALREADY_INITIALIZED), &[]),
        (&["setup"], false, &[], None, kept),
        (&["setup"], true, &[], None, &[]),
        (
            &["init", "--template", operator_root],
            false,
            &[DEMO_CHECK_LINE],
            Some(check_only.as_str()),
            &[
                ("check.sh", DEMO_CHECK_BYTES.as_bytes()),
                ("red-before-green.sh", rust_bytes.as_slice()),
            ],
        ),
    ];
    for (args, deleted, printed, whole, scripts) in rows {
        let label = format!("{args:?}, the Rust set's script deleted: {deleted}");
        let dir = cargo_project();
        let root = dir.path();
        write_files(root, &[("demo.marker", "")]);
        init(root);
        if deleted {
            std::fs::remove_file(root.join(rust_script)).unwrap();
        }
        let workflow = std::fs::read(root.join(".rigger/workflow.yml")).unwrap();
        let agents = dir_snapshot(&root.join(".rigger/agents"), "");

        let out = run_rigger_ok(root, args);
        assert_eq!(set_lines(&out), printed, "{label}: stdout:\n{out}");
        if let Some(whole) = whole {
            assert_eq!(out, whole, "{label}: it prints only what it wrote");
        }
        assert_eq!(
            std::fs::read(root.join(".rigger/workflow.yml")).unwrap(),
            workflow,
            "{label}: the workflow is kept"
        );
        assert_eq!(
            dir_snapshot(&root.join(".rigger/agents"), ""),
            agents,
            "{label}: the agents are kept, none added"
        );
        let expected: Vec<(String, Option<Vec<u8>>)> = scripts
            .iter()
            .map(|(name, bytes)| (name.to_string(), Some(bytes.to_vec())))
            .collect();
        assert_eq!(
            dir_snapshot(&root.join(".rigger/gates"), ""),
            expected,
            "{label}: the scripts .rigger/gates/ holds"
        );
    }
}

/// What `rigger validate` reports of a workflow declaring the Rust set's `env`, in name order,
/// then the default budget.
const RUST_SET_BUILD_REPORT: [&str; 3] = [
    "build env: CARGO_BUILD_JOBS={jobs}",
    "build env: CARGO_TARGET_DIR={build_cache}",
    "build budget: 4",
];

/// The `env:` line of an operator's set declaring three variables out of name order: one over
/// `{jobs}`, one below the build cache, and one whose name YAML reads as a boolean and whose value
/// holds a double quote.
const DEMO_ENV_LINE: &str =
    "env: {MY_JOBS: \"{jobs}\", A_CACHE: \"{build_cache}/demo\", \"TRUE\": \"say \\\"hi\\\"\"}\n";

/// [`DEMO_ENV_LINE`]'s variables, in name order.
const DEMO_ENV: [(&str, &str); 3] = [
    ("A_CACHE", "{build_cache}/demo"),
    ("MY_JOBS", "{jobs}"),
    ("TRUE", "say \"hi\""),
];

/// The `build.env` value a scaffold from the set declaring [`DEMO_ENV_LINE`] writes, byte for
/// byte: its name the boolean spelling double-quoted, its double quote escaped.
const DEMO_ENV_WRITTEN: &str =
    "{A_CACHE: \"{build_cache}/demo\", MY_JOBS: \"{jobs}\", \"TRUE\": \"say \\u0022hi\\u0022\"}";

/// What `rigger validate` reports of a workflow declaring [`DEMO_ENV`], then the default budget.
const DEMO_BUILD_REPORT: [&str; 4] = [
    "build env: A_CACHE={build_cache}/demo",
    "build env: MY_JOBS={jobs}",
    "build env: TRUE=say \"hi\"",
    "build budget: 4",
];

/// Where a scaffold's gate template sets come from.
enum Sets<'a> {
    /// This repository's root, its Rust set among them.
    Repository,
    /// No template root.
    NoRoot,
    /// A template root of the operator's own whose one set ends with the `env:` line given, or
    /// declares no `env` at all.
    Operator(Option<&'a str>),
}

/// A fresh project holding the operator's own set's marker and no `Cargo.toml`.
fn demo_project() -> tempfile::TempDir {
    let dir = temp_project();
    write_files(dir.path(), &[("demo.marker", "")]);
    dir
}

/// Given a fresh project, when the operator runs `rigger init` or `rigger setup` naming this
/// repository's root beside a root `Cargo.toml`, then the workflow it writes declares the Rust
/// set's `env` under `build:` - `CARGO_BUILD_JOBS` over `{jobs}` and `CARGO_TARGET_DIR` over
/// `{build_cache}` - and loads declaring exactly those two variables; matching no set, naming no
/// template root beside a root `Cargo.toml`, or applying an operator's set that declares no `env`,
/// it writes `env: {}` and declares none; applying an operator's set that declares its own `env`,
/// it writes that `env` in name order and loads declaring exactly it, a name of the longest key
/// YAML reads plain among them. `rigger validate` accepts each scaffold and reports one `build
/// env:` line per declared variable in name order, or none declared, then the build budget.
#[test]
fn a_fresh_scaffold_declares_its_template_sets_build_environment() {
    // (label, the project, the command, where its sets come from, the `build.env` value written,
    // the variables the workflow loads declaring, validate's build lines).
    type Row<'a> = (
        &'a str,
        fn() -> tempfile::TempDir,
        &'a str,
        Sets<'a>,
        &'a str,
        &'a [(&'a str, &'a str)],
        &'a [&'a str],
    );
    let longest = "A".repeat(LONGEST_PLAIN_KEY);
    let longest_line = set_env_line_named(LONGEST_PLAIN_KEY);
    let longest_written = env_written_named(LONGEST_PLAIN_KEY);
    let longest_report = format!("build env: {longest}=x");
    let rows: [Row<'_>; 8] = [
        (
            "init, this repository's root, the Rust set",
            cargo_project,
            "init",
            Sets::Repository,
            RUST_SET_ENV_WRITTEN,
            &RUST_SET_ENV,
            &RUST_SET_BUILD_REPORT,
        ),
        (
            "setup, this repository's root, the Rust set",
            cargo_project,
            "setup",
            Sets::Repository,
            RUST_SET_ENV_WRITTEN,
            &RUST_SET_ENV,
            &RUST_SET_BUILD_REPORT,
        ),
        (
            "init, this repository's root, matching no set",
            temp_project,
            "init",
            Sets::Repository,
            "{}",
            &[],
            UNDECLARED_BUILD_REPORT,
        ),
        (
            "init, no template root, beside a root Cargo.toml",
            cargo_project,
            "init",
            Sets::NoRoot,
            "{}",
            &[],
            UNDECLARED_BUILD_REPORT,
        ),
        (
            "setup, no template root, beside a root Cargo.toml",
            cargo_project,
            "setup",
            Sets::NoRoot,
            "{}",
            &[],
            UNDECLARED_BUILD_REPORT,
        ),
        (
            "init, an operator's set declaring no env",
            demo_project,
            "init",
            Sets::Operator(None),
            "{}",
            &[],
            UNDECLARED_BUILD_REPORT,
        ),
        (
            "setup, an operator's set declaring its own env",
            demo_project,
            "setup",
            Sets::Operator(Some(DEMO_ENV_LINE)),
            DEMO_ENV_WRITTEN,
            &DEMO_ENV,
            &DEMO_BUILD_REPORT,
        ),
        (
            "init, an operator's set declaring a name of the longest key YAML reads plain",
            demo_project,
            "init",
            Sets::Operator(Some(longest_line.as_str())),
            longest_written.as_str(),
            &[(longest.as_str(), "x")],
            &[longest_report.as_str(), UNDECLARED_BUILD_REPORT[1]],
        ),
    ];
    for (label, project, command, sets, written, declared, report) in rows {
        let dir = project();
        let root = dir.path();
        // The operator's template root is kept alive until the row ends.
        let (_operator, template) = match sets {
            Sets::Repository => (None, Some(template_root())),
            Sets::NoRoot => (None, None),
            Sets::Operator(env_line) => {
                let dir = env_line.map_or_else(
                    || operator_template_root(DEMO_IMPLEMENTER_ID),
                    operator_template_root_declaring,
                );
                let path = dir.path().to_str().unwrap().to_string();
                (Some(dir), Some(path))
            }
        };
        let mut args = vec![command];
        if let Some(template) = &template {
            args.extend(["--template", template.as_str()]);
        }
        run_rigger_ok(root, &args);

        let workflow = std::fs::read_to_string(root.join(".rigger/workflow.yml")).unwrap();
        let (out, err, ok) = run_rigger(root, &["validate"]);
        assert_eq!(
            (
                workflow
                    .matches(&format!("\nbuild:\n  env: {written}\n"))
                    .count(),
                loaded_config(root).workflow.build.env,
                ok,
                build_report_lines(&out),
            ),
            (1, build_declaring(declared, 0).env, true, report.to_vec()),
            "{label}: the written `build.env`, the variables it loads declaring, and validate's \
             build lines; workflow:\n{workflow}\nstdout:\n{out}\nstderr:\n{err}"
        );
    }
}
