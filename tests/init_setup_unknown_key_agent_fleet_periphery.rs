//! Periphery (CLI, real-binary) test for the checkin-round fix to spec 102 criterion 3 (AN
//! UNKNOWN KEY IS NAMED): `rigger init` and `rigger setup` must fail loudly on a
//! workflow.yml carrying an unrecognized key, never silently reinterpret the parse failure
//! as the empty-repo signal and re-scaffold the full default agent fleet.
//!
//! `get_referenced_agent_ids` (`src/main.rs`) used to parse `workflow.yml` with a bare
//! `serde_yaml::from_str::<Workflow>` instead of the canonical, `deny_unknown_fields`-honoring
//! `config_store::load_workflow`, and its only caller (`init_project`) folded that parse
//! error into `.unwrap_or_default()` - the exact same empty `HashSet` a genuinely ABSENT
//! workflow.yml produces. `init_project` treats an empty referenced-agent set as "no
//! workflow, scaffold every default" (the empty-repo case, spec 08 item 3), so a typo'd key
//! made `rigger init`/`rigger setup` silently recreate every default agent file - including
//! ones an operator had deliberately never referenced, or had deliberately removed - with no
//! error text anywhere and exit 0. The unit-level fix and its own tests
//! (`src/main.rs::get_referenced_agent_ids_errors_loudly_on_an_unknown_key_instead_of_returning_empty`,
//! `src/main.rs::init_project_errors_loudly_on_an_unknown_key_and_does_not_scaffold_agents`)
//! prove the library-level behavior; this file proves it end to end through the compiled
//! binary, at both entry points the finding named.

mod common;

use common::cli::temp_project;

use std::path::Path;

fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let out = common::rigger_courier()
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Every agent `rigger init`'s default fleet seeds when a workflow references none of them
/// (the genuine empty-repo case) - the exact set the bug re-scaffolds even when it should
/// not, because a broken-but-PRESENT workflow.yml was misread as an ABSENT one.
const DEFAULT_FLEET: &[&str] = &[
    "planner.md",
    "rust-engineer.md",
    "architecture-reviewer.md",
    "sdet.md",
    "adversary.md",
    "adjudicator.md",
];

fn agents_present(agents_dir: &Path) -> Vec<&'static str> {
    DEFAULT_FLEET
        .iter()
        .filter(|name| agents_dir.join(name).exists())
        .copied()
        .collect()
}

/// `rigger init` against a project whose `.rigger/workflow.yml` was hand-placed BEFORE init
/// ever ran (so `write_if_absent` keeps it, exactly as a real operator's committed file
/// would be kept on a rerun) and carries one unrecognized key: the command must exit
/// non-zero, name the dotted path in its stderr, and scaffold NO agent file at all - never
/// fall back to the empty-repo full-fleet seed.
#[test]
fn rigger_init_fails_loudly_on_unknown_key_workflow_and_scaffolds_no_agents() {
    let dir = temp_project();
    let root = dir.path();
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    std::fs::write(
        rigger_dir.join("workflow.yml"),
        "defaults:\n  max_parallel_unitz: 2\n",
    )
    .expect("write the unknown-key workflow.yml fixture");

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(
        !ok,
        "rigger init must fail on a workflow.yml carrying an unrecognized key"
    );
    assert!(
        err.contains("defaults.max_parallel_unitz: unknown key"),
        "rigger init's stderr must name the dotted path of the unrecognized key; stderr:\n{err}"
    );

    let agents_dir = rigger_dir.join("agents");
    let present = agents_present(&agents_dir);
    assert!(
        present.is_empty(),
        "a failed load must scaffold NO agent, never fall back to the empty-repo full \
         default fleet: found {present:?}"
    );
}

/// The destructive scenario the finding actually reproduced: an operator's ALREADY
/// initialized project deliberately references only two of the six default agents (a
/// curated fleet, mirroring `src/main.rs::init_scaffolds_only_the_workflow_referenced_agents`),
/// then `workflow.yml` later picks up a typo'd key (e.g. a hand-edit). A subsequent `rigger
/// setup` must fail loudly and must NOT reintroduce any of the four deliberately-unreferenced
/// default agents - the exact silent re-scaffold `adv-checkin-uphold-sharpen-arch-agent-ids-
/// silent-swallow` reproduced empirically against the pre-fix binary.
#[test]
fn rigger_setup_fails_loudly_on_unknown_key_workflow_and_never_reintroduces_the_curated_fleet() {
    let dir = temp_project();
    let root = dir.path();
    let rigger_dir = root.join(".rigger");
    let agents_dir = rigger_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create .rigger/agents");

    // A curated workflow, referencing only planner + adversary - written BEFORE the first
    // init so `write_if_absent` keeps it verbatim, exactly as a real committed file would be.
    std::fs::write(
        rigger_dir.join("workflow.yml"),
        "stages:\n  plan:\n    agent: planner\n  go:\n    agent: adversary\n",
    )
    .expect("write the curated workflow.yml fixture");

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(
        ok,
        "the first, well-formed init must succeed; stderr:\n{err}"
    );
    assert!(
        agents_dir.join("planner.md").exists() && agents_dir.join("adversary.md").exists(),
        "the two referenced agents must be scaffolded by the first init"
    );
    for unreferenced in [
        "rust-engineer.md",
        "architecture-reviewer.md",
        "sdet.md",
        "adjudicator.md",
    ] {
        assert!(
            !agents_dir.join(unreferenced).exists(),
            "an unreferenced default must not be scaffolded by the first init: {unreferenced}"
        );
    }

    // The operator's workflow.yml now picks up a typo'd key, keeping the same referenced
    // agents - the exact shape a hand-edit gone wrong produces.
    std::fs::write(
        rigger_dir.join("workflow.yml"),
        "stages:\n  plan:\n    agent: planner\n  go:\n    agent: adversary\n  \
         bad:\n    agent: planner\n    gatez: [build]\n",
    )
    .expect("overwrite workflow.yml with the unknown-key fixture");

    let (_out, err, ok) = run_rigger(root, &["setup"]);
    assert!(
        !ok,
        "rigger setup must fail on a workflow.yml carrying an unrecognized key"
    );
    assert!(
        err.contains("stages.bad.gatez: unknown key"),
        "rigger setup's stderr must name the dotted path of the unrecognized key; stderr:\n{err}"
    );

    for unreferenced in [
        "rust-engineer.md",
        "architecture-reviewer.md",
        "sdet.md",
        "adjudicator.md",
    ] {
        assert!(
            !agents_dir.join(unreferenced).exists(),
            "a failed setup must NEVER reintroduce a deliberately-unreferenced default agent \
             over the operator's curated fleet: {unreferenced}"
        );
    }
}
