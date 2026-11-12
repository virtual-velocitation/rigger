//! Periphery for the per-role advisor declaration. The host's own settings can hand every session
//! an `advisor` tool (a stronger model the session consults mid-turn); a role whose job is to judge
//! someone else's work consults none. A persona declares that with the frontmatter key
//! `advisor: false`, and both process hosts turn it into the one launch setting that removes the
//! tool from the session, `advisorModel` set to the empty string in the `--settings` they hand it.
//! A disallowed-tools entry does not remove it (the tool is server side and is not in the
//! session's tool list), so no host passes one. Undeclared, or `advisor: true`, the session
//! inherits the host's setting and the launch carries nothing about it.
//!
//! Every launch below goes through the real parser (`parse_agent`) into the real argv builder of
//! each host, so the key is proven READ, not merely tolerated. The last two tests pin which
//! personas carry the key: exactly the roles the workflow's review panels and stages name as
//! judges, in this repository and in the personas `rigger init` seeds, so a review persona added
//! later cannot silently inherit the advisor.

use crate::common;
use common::cli::{loaded_config, run_rigger_ok, temp_project};
use common::fixtures::implementer_opts;
use common::repo::repo_root;

use std::collections::BTreeSet;

use rigger::config::{parse_agent, push_reviewers, AgentDef, Config};
use rigger::driver::{claude_code, cli, LaunchSession, SystemPromptFile};
use serde_json::Value;

/// The `--settings` JSON `agent`'s launch carries on each process host, named by host, the spawn's
/// own settings being `spawn_settings`. Each launch is also held to carry no disallowed-tools flag.
fn launch_settings(agent: &AgentDef, spawn_settings: &str) -> Vec<(&'static str, Value)> {
    let mut opts = implementer_opts("u1/reviewer#0");
    opts.settings_json = spawn_settings.to_string();
    let persona = SystemPromptFile::default();
    let session = LaunchSession::fresh();
    let launches = [
        (
            "claude_code",
            claude_code::build_args(agent, &persona, &opts, &session, "rigger", &[]),
        ),
        (
            "cli",
            cli::build_args(agent, &persona, &opts, &session, "rigger"),
        ),
    ];
    launches
        .into_iter()
        .map(|(host, args)| {
            let args = args.unwrap_or_else(|e| panic!("{host}: build the launch: {e}"));
            assert!(
                !args
                    .iter()
                    .any(|a| a == "--disallowedTools" || a == "--disallowed-tools"),
                "{host}: the advisor is removed by a setting, never a disallowed tool: {args:?}"
            );
            let at = args
                .iter()
                .position(|a| a == "--settings")
                .unwrap_or_else(|| panic!("{host}: the launch carries --settings: {args:?}"));
            let settings = serde_json::from_str(&args[at + 1])
                .unwrap_or_else(|e| panic!("{host}: --settings is JSON: {e}"));
            (host, settings)
        })
        .collect()
}

fn agent_doc(extra_frontmatter: &str) -> AgentDef {
    let doc = format!(
        "---\nid: reviewer\nmodel: opus\n{extra_frontmatter}tools: [Read]\nisolation: none\n---\nJudge the diff.\n"
    );
    parse_agent(doc.as_bytes()).unwrap_or_else(|e| panic!("parse {extra_frontmatter:?}: {e}"))
}

/// What a launch's settings carry for `advisorModel`: the value, or `None` when the key is absent.
fn advisor_model(settings: &Value) -> Option<&str> {
    settings.get("advisorModel").map(|v| {
        v.as_str()
            .unwrap_or_else(|| panic!("advisorModel is a string: {settings}"))
    })
}

/// A role declaring `advisor: false` launches with the advisor removed, a role that does not keeps
/// the host's setting, whatever the spawn's own settings hold, and the rest of those settings
/// survive the merge.
#[test]
fn a_persona_declaring_no_advisor_launches_without_one_and_any_other_does_not_touch_it() {
    struct Case {
        name: &'static str,
        frontmatter: &'static str,
        spawn_settings: &'static str,
        advisor_model: Option<&'static str>,
        model: Option<&'static str>,
    }
    let cases = [
        Case {
            name: "undeclared inherits the host setting",
            frontmatter: "",
            spawn_settings: "",
            advisor_model: None,
            model: None,
        },
        Case {
            name: "advisor true inherits the host setting",
            frontmatter: "advisor: true\n",
            spawn_settings: "",
            advisor_model: None,
            model: None,
        },
        Case {
            name: "advisor false removes it",
            frontmatter: "advisor: false\n",
            spawn_settings: "",
            advisor_model: Some(""),
            model: None,
        },
        Case {
            name: "advisor false keeps the spawn's other settings",
            frontmatter: "advisor: false\n",
            spawn_settings: r#"{"model":"opus"}"#,
            advisor_model: Some(""),
            model: Some("opus"),
        },
        Case {
            name: "advisor false wins over an advisor the spawn's settings name",
            frontmatter: "advisor: false\n",
            spawn_settings: r#"{"advisorModel":"strong"}"#,
            advisor_model: Some(""),
            model: None,
        },
        Case {
            name: "undeclared leaves an advisor the spawn's settings name",
            frontmatter: "",
            spawn_settings: r#"{"advisorModel":"strong"}"#,
            advisor_model: Some("strong"),
            model: None,
        },
    ];
    for case in cases {
        let agent = agent_doc(case.frontmatter);
        for (host, settings) in launch_settings(&agent, case.spawn_settings) {
            assert_eq!(
                advisor_model(&settings),
                case.advisor_model,
                "{host}: {}: {settings}",
                case.name
            );
            assert_eq!(
                settings.get("model").and_then(Value::as_str),
                case.model,
                "{host}: {}: {settings}",
                case.name
            );
        }
    }
}

/// The key is a boolean: a value that is not one is refused at the parse, naming the key.
#[test]
fn a_non_boolean_advisor_value_is_refused_at_the_parse() {
    let doc = b"---\nid: reviewer\nadvisor: sometimes\n---\nJudge the diff.\n";
    let refusal = parse_agent(doc).expect_err("a non-boolean advisor must not parse");
    assert!(
        refusal.to_string().contains("advisor"),
        "the refusal names the key: {refusal}"
    );
}

/// Every agent id a workflow names as a judge: the agents of its default review panel, of each
/// stage's own review panel, and a stage's own `adversary` and `adjudicator` (the plan critique).
fn judging_roles(cfg: &Config) -> BTreeSet<String> {
    let mut ids = cfg.workflow.defaults.review.agent_ids();
    for stage in cfg.workflow.stages.values() {
        ids.extend(stage.review.agent_ids());
        push_reviewers(&mut ids, &stage.adversary, &stage.adjudicator);
    }
    ids.into_iter().collect()
}

/// The ids of `cfg`'s agents whose launch removes the advisor, on both hosts alike.
fn agents_without_the_advisor(cfg: &Config) -> BTreeSet<String> {
    cfg.agents
        .iter()
        .filter(|(id, agent)| {
            let hosts = launch_settings(agent, "");
            let off: Vec<bool> = hosts
                .iter()
                .map(|(_, s)| advisor_model(s) == Some(""))
                .collect();
            assert!(
                off.iter().all(|o| *o == off[0]),
                "{id}: the hosts disagree on the advisor: {hosts:?}"
            );
            off[0]
        })
        .map(|(id, _)| id.clone())
        .collect()
}

fn ids(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

/// This repository's personas carry `advisor: false` exactly on the roles its workflow names as
/// judges, and those are the four review personas.
#[test]
fn this_repositorys_judging_personas_run_without_the_advisor_and_no_other_persona_does() {
    let cfg = loaded_config(&repo_root());
    let judging = judging_roles(&cfg);
    assert_eq!(
        judging,
        ids(&["adjudicator", "adversary", "architecture-reviewer", "sdet"]),
        "the workflow's judging roles"
    );
    assert_eq!(
        agents_without_the_advisor(&cfg),
        judging,
        "a persona runs without the advisor exactly when the workflow names it a judge"
    );
}

/// The personas `rigger init` seeds follow the same rule: the review roles of the workflow it
/// writes run without the advisor, the planner and the implementer inherit it.
#[test]
fn the_personas_rigger_init_seeds_follow_the_same_rule() {
    let project = temp_project();
    run_rigger_ok(project.path(), &["init"]);
    let cfg = loaded_config(project.path());
    let judging = judging_roles(&cfg);
    assert!(
        judging.is_superset(&ids(&["adjudicator", "adversary"])),
        "the scaffolded workflow names its judges: {judging:?}"
    );
    assert_eq!(agents_without_the_advisor(&cfg), judging);
}
