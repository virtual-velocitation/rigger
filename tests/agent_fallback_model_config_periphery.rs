//! Periphery for spec 104 criterion 1 (THE LAUNCH IS TYPED): `AgentDef::fallback_model`'s
//! CONFIG-PARSE seam, closed at the check-in seam (spec 91) re-enumeration pass over the
//! whole spec-104 diff.
//!
//! `src/config.rs`'s own doc comment on the field and `src/driver/claude_code.rs`'s
//! `build_args_types_the_full_launch` both assume the field reaches [`build_args`] already
//! populated, but neither proves it actually GETS there from a real
//! `.rigger/agents/<id>.md` document: the implementer's own inline test builds the
//! `AgentDef` as a Rust struct literal, never through [`parse_agent`]. Its sibling field,
//! `model_ladder`, has exactly this config-source-to-behavior seam proven end-to-end
//! (`step_resolves_the_model_ladders_first_rung_for_the_initial_attempt`, `tests/cli.rs`);
//! before this file, `fallback_model` had zero references anywhere under `tests/`.
//!
//! `tests/claude_code_launch_wire_periphery.rs` explicitly defers the argv shape
//! (including `--fallback-model`) to `src/driver/claude_code.rs`'s own inline tests - this
//! file does not re-litigate that (it asserts the SAME flag, but only ever downstream of a
//! REAL parse, which is the one link in the chain nothing else exercises).

use rigger::conductor::SpawnOpts;
use rigger::config::parse_agent;
use rigger::driver::claude_code::build_args;

fn opts(id: &str) -> SpawnOpts {
    SpawnOpts {
        id: id.to_string(),
        attempt: 0,
        system_prompt: "You implement findings.".to_string(),
        ..Default::default()
    }
}

/// The real parser (`parse_agent`, YAML frontmatter into `AgentDef`'s `#[serde(default)]`
/// field - `AgentDef` carries no `deny_unknown_fields`, so this proves the key is actually
/// READ, not merely tolerated) populates `fallback_model` from a
/// `.rigger/agents/<id>.md`-shaped document, and the resolved field survives into the real
/// launch args as `--fallback-model` - the config-source-to-behavior seam `model_ladder`
/// already proves for its own field.
#[test]
fn fallback_model_parses_from_real_frontmatter_and_reaches_build_args() {
    let doc = b"---\nid: worker\nmodel: sonnet\nfallback_model: haiku-fallback\ntools: [Read]\nisolation: none\n---\nDo the unit.\n";
    let agent =
        parse_agent(doc).expect("a well-formed agent doc with a fallback_model key must parse");
    assert_eq!(
        agent.fallback_model, "haiku-fallback",
        "parse_agent must read the fallback_model key from real frontmatter text, not just tolerate it"
    );

    let args = build_args(&agent, &opts("u1/implementer#0"), "sess-1", "rigger");
    let i = args.iter().position(|x| x == "--fallback-model").expect(
        "build_args must emit --fallback-model for a config-parsed AgentDef, not only a hand-built one",
    );
    assert_eq!(args[i + 1], "haiku-fallback");
}

/// The absence half of the same seam: an agent doc that never mentions `fallback_model`
/// (the overwhelmingly common case - every `.rigger/agents/*.md` in this repo predates the
/// field) parses to an empty string via `#[serde(default)]`, and `build_args` omits the
/// flag entirely rather than passing an explicitly-empty value through to Claude Code.
#[test]
fn fallback_model_absent_from_frontmatter_parses_empty_and_build_args_omits_the_flag() {
    let doc =
        b"---\nid: worker\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDo the unit.\n";
    let agent = parse_agent(doc)
        .expect("a well-formed agent doc with no fallback_model key must still parse");
    assert_eq!(
        agent.fallback_model, "",
        "an absent key must default to empty, not error"
    );

    let args = build_args(&agent, &opts("u1/implementer#0"), "sess-1", "rigger");
    assert!(
        !args.iter().any(|x| x == "--fallback-model"),
        "build_args must omit --fallback-model entirely when unconfigured; got {args:?}"
    );
}
