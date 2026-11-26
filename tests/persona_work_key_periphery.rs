//! Periphery for spec 135's THE WORK KEY PARSES, from outside the parser. `parse_agent`'s own
//! table (`crates/rigger-domain/src/config.rs`) covers every shape a `work:` value can take. This
//! file covers the layers above it, using persona files on disk. A file whose `work:` is none of
//! the three kinds stops `rigger validate`, and the refusal names the file, the persona and the
//! three kinds. The loader's read of `.rigger/agents/` carries each persona's declared work and
//! effort.

use crate::common;
use common::cli::{both_fail, refusal_line, Surface};
use common::fixtures::work_refusal;
use rigger::config::Work;

/// Given an initialized project with the persona file `file` (whose `id:` is `critic`) holding
/// `frontmatter`, when the operator runs `rigger validate`, then the load fails. Its error names
/// the file, the persona and the three kinds, and the binary ends its stderr with that refusal.
fn assert_validate_refuses_the_persona_naming_its_file_and_id(file: &str, frontmatter: &str) {
    let (lib_err, cli_err) = both_fail(
        Surface::Validate,
        &format!("agents/{file}"),
        &format!("---\n{frontmatter}---\nJudge the diff.\n"),
    );
    let refusal = format!("config: {file}: config: {}", work_refusal("critic"));
    assert_eq!(lib_err, refusal, "config_store::load on {frontmatter:?}");
    assert_eq!(
        cli_err.lines().last(),
        Some(refusal_line(&refusal).as_str()),
        "rigger validate's last stderr line on {frontmatter:?}; stderr:\n{cli_err}"
    );
}

rigger::test_cases! {
    /// A `work:` value that matches none of the three kinds, written above the persona's `id:`.
    rigger_validate_refuses_a_persona_whose_work_names_no_kind:
        assert_validate_refuses_the_persona_naming_its_file_and_id(
            "critic-persona.md",
            "work: deciding\nid: critic\n",
        );

    /// A `work:` key the file declares twice, each time with one of the three kinds.
    rigger_validate_refuses_a_persona_that_declares_work_twice:
        assert_validate_refuses_the_persona_naming_its_file_and_id(
            "critic-persona.md",
            "work: judging\nid: critic\nwork: writing\n",
        );
}

/// Given a directory of persona files, each declaring its work and effort its own way, when the
/// loader reads the directory, then each persona carries exactly the work and effort its file
/// declares, in whatever key order the file writes them. An absent key reads as none.
#[test]
fn the_agents_dir_read_carries_each_personas_declared_work_and_effort() {
    let dir = tempfile::tempdir().expect("create the agents dir");
    for (file, frontmatter) in [
        ("judge.md", "id: judge\nwork: judging\neffort: high\n"),
        ("plain.md", "id: plain\n"),
        ("scout.md", "id: scout\nwork: reading\n"),
        ("scribe.md", "effort: xhigh\nwork: writing\nid: scribe\n"),
    ] {
        std::fs::write(
            dir.path().join(file),
            format!("---\n{frontmatter}---\nDo the work.\n"),
        )
        .unwrap_or_else(|e| panic!("write {file}: {e}"));
    }

    let mut read: Vec<_> = rigger::config_store::read_agents_dir(dir.path())
        .expect("every persona file declares a valid work key")
        .into_iter()
        .map(|(file, agent)| (file, agent.id, agent.work, agent.effort))
        .collect();
    read.sort_by(|a, b| a.0.cmp(&b.0));

    let row = |file: &str, id: &str, work, effort: &str| {
        (file.to_string(), id.to_string(), work, effort.to_string())
    };
    assert_eq!(
        read,
        [
            row("judge.md", "judge", Some(Work::Judging), "high"),
            row("plain.md", "plain", None, ""),
            row("scout.md", "scout", Some(Work::Reading), ""),
            row("scribe.md", "scribe", Some(Work::Writing), "xhigh"),
        ]
    );
}
