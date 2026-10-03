//! The disk-touching half of [`crate::config`] (spec 93, criterion 1): loading agent
//! definitions and the workflow from `.rigger/`, and [`Config::validate`] (which also
//! probes the build-wrapper/mutation-gate binary on `PATH` via [`crate::gate`]). See
//! `config.rs`'s own doc for the split rationale (mirrors [`crate::spawn`]/
//! [`crate::spawn_store`]) - the plain value types and the pure parsing/lint functions
//! stay there, always compiled, because [`Config`] and friends are part of the declared
//! `core` surface (spec 93 Design: "config (parsing)").
//!
//! `config::load` and friends are unaffected by this split; only the module a caller
//! names changed, from `config::X` to `config_store::X`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::config::{
    err, find_cycle, index_agents, parse_agent, parse_yaml_naming_unknown_keys,
    resolve_wall_clocks, AgentDef, Config, Defaults, Error, StoreConfig, Workflow, RIGGER_DIR,
};
use crate::instructions::Instruction;
// The rest of `config`'s pure surface this file's own PRODUCTION code never touches, but its
// moved test module (spec 93, criterion 1 - see this file's own doc) does: Duration for a
// backoff fixture, `failure` for classify() round-trips, and the lint/validate helpers'
// remaining fixture types. Gated so the plain (non-test) library build never carries an
// unused-import warning for a name only a test uses.
#[cfg(test)]
use crate::config::{
    lint_gating_verdict_lines, literal_is_emit_payload, puts_verdict_on_result_channel,
    unbounded_wall_clock_advisory, Gate, ReviewDepth, ReviewPanel, Stage,
};
#[cfg(test)]
use crate::failure;
#[cfg(test)]
use std::time::Duration;

/// Refuse a RETIRED workflow key that still carries a value: a key nothing reads any more
/// fails loudly, naming the key, why it was retired and the line to delete, rather than
/// silently no-op'ing a setting the operator believes is still wired. An absent key passes.
fn reject_retired_key(value: &str, key: &str, line: &str, why: &str) -> Result<(), Error> {
    if value.trim().is_empty() {
        return Ok(());
    }
    Err(err(format!(
        "the workflow key `{key}` is retired ({why}): delete {line} from your workflow.yml \
         (config key: {key})"
    )))
}

/// Load reads agent definitions from <dir>/.rigger/agents/*.md and the workflow
/// from <dir>/.rigger/workflow.yml, then validates referential and structural
/// integrity.
pub fn load(dir: &str) -> Result<Config, Error> {
    let base = Path::new(dir).join(RIGGER_DIR);
    let mut agents = load_agents(&base.join("agents"))?;
    let workflow = load_workflow(&base.join("workflow.yml"))?;
    resolve_wall_clocks(&mut agents, &workflow.defaults);
    let instructions = load_instructions(Path::new(dir))?;
    let cfg = Config {
        agents,
        workflow,
        instructions,
    };
    cfg.validate()?;
    Ok(cfg)
}

/// Read the operator instruction layer: every `<dir>/.rigger/instructions/*.md` in filename
/// order, each named by its file stem. The directory's own `README.md` documents the layer and
/// is not part of it. An absent directory is an empty layer; any other read failure is an
/// error naming the path, never a silently thinner layer.
pub fn load_instructions(dir: &Path) -> Result<Vec<Instruction>, Error> {
    let ins_dir = dir.join(RIGGER_DIR).join("instructions");
    let entries = match std::fs::read_dir(&ins_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(err(format!("read {}: {e}", ins_dir.display()))),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| err(format!("read {}: {e}", ins_dir.display())))?
            .path();
        let is_md = path.extension().and_then(|x| x.to_str()) == Some("md");
        if is_md && path.file_name().and_then(|x| x.to_str()) != Some(INSTRUCTIONS_README) {
            files.push(path);
        }
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let body = std::fs::read_to_string(&path)
                .map_err(|e| err(format!("read {}: {e}", path.display())))?;
            let name = path
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_string();
            Ok(Instruction { name, body })
        })
        .collect()
}

/// The file that documents `.rigger/instructions/` (scaffolded by `rigger init`); it
/// describes the operator layer and is never itself part of it.
pub const INSTRUCTIONS_README: &str = "README.md";

fn load_agents(dir: &Path) -> Result<BTreeMap<String, AgentDef>, Error> {
    index_agents(read_agents_dir(dir)?)
}

/// Read every `<dir>/*.md` agent definition into `(filename, AgentDef)` pairs, parsing
/// each through [`parse_agent`]. This is the ONE directory-read-and-parse loop: [`load`]
/// calls it then [`index_agents`]; a caller assembling a prospective fleet (e.g. `rigger
/// setup --agents`) calls it to enumerate the agents already on disk, then indexes that
/// list combined with its own additions - so both enumerate existing agents through the
/// same seam rather than re-implementing (and drifting from) the loop. Returns the pairs
/// UN-indexed so the caller controls when the fleet-wide id invariant is enforced.
pub fn read_agents_dir(dir: &Path) -> Result<Vec<(String, AgentDef)>, Error> {
    let entries = std::fs::read_dir(dir).map_err(|e| err(format!("read agents dir: {e}")))?;
    let mut parsed = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| err(e.to_string()))?.path();
        if path.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("?")
            .to_string();
        let b = std::fs::read(&path).map_err(|e| err(format!("read {name}: {e}")))?;
        let a = parse_agent(&b).map_err(|e| err(format!("{name}: {e}")))?;
        parsed.push((name, a));
    }
    Ok(parsed)
}

/// Parse the workflow definition at `path` (no agents-dir read, no referential validation) -
/// `pub` (widened from `pub(crate)` at spec 92 criterion 2, checkin round) so this is the ONE
/// canonical, `deny_unknown_fields`-honoring workflow parser every caller across BOTH crates
/// routes through, never a second ad hoc `serde_yaml::from_str::<Workflow>` reimplementing it.
/// Originally exposed to the workflow-DEFINITION graph indexer (`grounder::workflowdef`,
/// same crate) so it can read stages/gates/agents straight off `.rigger/workflow.yml` without
/// depending on the agents directory being valid, mirroring the reasoning [`read_store_config`]
/// below already established for the lightweight `store:`-only probe: indexing what the
/// definition NAMES must not fail because an unrelated agent frontmatter file is malformed.
/// Now also the binary crate's (`main.rs::get_referenced_agent_ids`) entry - a bare
/// `serde_yaml::from_str` there bypassed this function's unknown-key rejection entirely, and
/// its caller's `.unwrap_or_default()` folded THAT parse error into the same empty set an
/// absent workflow.yml legitimately produces, so a typo'd key made `rigger init`/`setup`
/// silently re-scaffold the full default agent fleet over an operator's deliberately curated
/// one. [`load`] above stays the FULL, validating entry every run-starting path uses.
pub fn load_workflow(path: &Path) -> Result<Workflow, Error> {
    let b = std::fs::read_to_string(path).map_err(|e| err(format!("read workflow: {e}")))?;
    let mut wf: Workflow =
        parse_yaml_naming_unknown_keys(&b).map_err(|msg| err(format!("parse workflow: {msg}")))?;
    let names: Vec<String> = wf.stages.keys().cloned().collect();
    for name in names {
        if let Some(st) = wf.stages.get_mut(&name) {
            st.name = name;
        }
    }
    Ok(wf)
}

/// Read ONLY the `store:` selection from `<rigger_dir>/workflow.yml` (§48 rung 4), tolerating an
/// absent file - a project that pins nothing - by returning the default ([`StoreConfig::default`],
/// "no opinion", so the store resolver falls through to its next rung). This is the LIGHTWEIGHT
/// probe the store resolver's project-config rung uses: it deliberately parses only the `store:`
/// key through a throwaway struct, ignoring the rest of the workflow (stages, gates, agents), so a
/// bare courier's store resolution never depends on - or fails on - a workflow concern the courier
/// has no need for. A syntactically malformed workflow.yml still surfaces as a clear parse error
/// (the same class `load_workflow` reports), and a PRESENT-but-unreadable file (a permission or IO
/// fault, distinct from a genuinely absent one) surfaces LOUDLY too - never a silent wrong-store
/// fallback that would misfile a bare courier's self-report off the store the conductor reads.
///
/// Anchored at the `.rigger` directory (not the project root) so the store resolver, which already
/// resolves that directory once at the owning repo root, passes it straight through.
pub fn read_store_config(rigger_dir: &Path) -> Result<StoreConfig, Error> {
    #[derive(Deserialize, Default)]
    struct Probe {
        #[serde(default)]
        store: StoreConfig,
    }
    let probe: Probe = read_workflow_probe(rigger_dir, "store config")?;
    Ok(probe.store)
}

/// THE LIGHTWEIGHT WORKFLOW PROBE: parse `<rigger_dir>/workflow.yml` into the throwaway probe
/// struct `P` (which names only the keys its caller needs), naming the probe `label` in any
/// error. An ABSENT file is "no opinion" - the project pins nothing, so it resolves to
/// `P::default()` and the caller falls through to its next rung. Any OTHER IO error (a
/// PRESENT-but-unreadable file: a permission or IO fault) surfaces LOUDLY, never collapsing into
/// the same default an absent file returns - a silent wrong-store fallback off an unreadable
/// config is the exact fracture the store rung guards against.
fn read_workflow_probe<P>(rigger_dir: &Path, label: &str) -> Result<P, Error>
where
    P: serde::de::DeserializeOwned + Default,
{
    let body = match std::fs::read_to_string(rigger_dir.join("workflow.yml")) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(P::default()),
        Err(e) => return Err(err(format!("read {label}: {e}"))),
    };
    parse_yaml_naming_unknown_keys(&body).map_err(|msg| err(format!("parse {label}: {msg}")))
}

/// Read ONLY `defaults.workdir` from `<rigger_dir>/workflow.yml` (spec 77 criterion 5,
/// BOUNDED SHARED CACHE), tolerating an absent file exactly like [`read_store_config`]'s own
/// NotFound-vs-other split - a project that pins nothing resolves to `""` (the scratch-root
/// resolver's own default rung, `<repo>/.rigger/tmp`). This is the LIGHTWEIGHT probe `rigger
/// reset --build-cache` resolves the scratch root through: it is a pure filesystem reclaim
/// with no store-mutation implication, and must not additionally require a fully loadable
/// agent fleet ([`load`]'s `load_agents`) or a passing [`Config::validate`] just to learn one
/// string field - a workflow.yml whose `stages:`/`agents:` reference something absent from
/// disk (or simply has no `agents/` dir at all) must not stop an operator from reclaiming
/// disk space.
///
/// Anchored at the `.rigger` directory, matching [`read_store_config`]'s own convention -
/// a caller that already resolved the store dir passes it straight through.
pub fn read_scratch_workdir(rigger_dir: &Path) -> Result<String, Error> {
    Ok(read_scratch_defaults(rigger_dir)?.workdir)
}

/// Read the FULL `defaults:` block from `<rigger_dir>/workflow.yml` (spec 83 criterion 2,
/// round 2) - the single lightweight parse [`read_scratch_workdir`] delegates to, so a
/// caller that also needs a second `defaults.*` field (e.g. `defaults.max_retries`) reads
/// it through the SAME probe rather than a second, independently-maintained copy. Tolerates
/// an absent file exactly like [`read_store_config`]'s own NotFound-vs-other split: an
/// absent file resolves to `Defaults::default()` ("no opinion"), never a silent swallow of a
/// genuinely unreadable one.
///
/// Requires neither a loadable `.rigger/agents/` fleet nor a passing [`Config::validate`] -
/// unlike [`load`], which fails outright whenever either is unsatisfied (e.g. this project's
/// own committed `gates.mutation` when `cargo-mutants` is off PATH, or simply no `agents/`
/// dir at all). A caller reading only a `defaults.*` field must never inherit that
/// unrelated failure by routing through the full loader and `.unwrap_or_default()`-ing past
/// it - doing so silently zeroes the field it actually wanted alongside the one that failed
/// (spec 83's own round-2 reject: `rigger status`/`rigger watch` silently lost a configured
/// `defaults.workdir` this way whenever `Config::validate` failed for an unrelated reason).
///
/// Anchored at the `.rigger` directory, matching [`read_store_config`]'s own convention.
pub fn read_scratch_defaults(rigger_dir: &Path) -> Result<Defaults, Error> {
    #[derive(Deserialize, Default)]
    struct Probe {
        #[serde(default)]
        defaults: Defaults,
    }
    let probe: Probe = read_workflow_probe(rigger_dir, "workflow")?;
    Ok(probe.defaults)
}

impl Config {
    /// Validate checks that every reference resolves and the stage graph is acyclic.
    pub fn validate(&self) -> Result<(), Error> {
        let wf = &self.workflow;
        // Build-environment resolution (spec 65 unit 2, NO SILENT DEGRADE): a CONFIGURED
        // (non-auto, non-off) `build.wrapper` absent from PATH, OR whose cache dir cannot
        // be created, is a run-start config error naming what failed and the relevant key -
        // the operator asked for it explicitly, so proceeding would fake a cache that never
        // actually runs. `auto` (either axis comes up empty -> injects nothing) and
        // `off`/empty never fail here - a discovered-implicit degrade, not a
        // configured-explicit one. This is the ONE resolution `crate::conductor`'s
        // build-environment authority and `rigger validate`'s reporting surface both read
        // too - never a second, independently re-derived check.
        if let Err(e) = crate::gate::resolve_build_layer(&wf.build.wrapper, &wf.build.cache_dir) {
            return Err(err(e.to_string()));
        }
        // The top-level workflow `name` key is retired: nothing ever read it, so a workflow that
        // still sets it fails loudly at the exact line to delete rather than carrying a setting
        // the operator believes does something.
        reject_retired_key(&wf.name, "name", "the `name:` line", "nothing reads it")?;
        // `build.mutation` schema retirement (spec 91): the per-round mutation-efficacy
        // switch spec 73 introduced is superseded by the `checkin` stage's own `mutation`
        // gate, which runs the sweep ONCE at check-in through the ordinary gate pipeline
        // rather than a build-config toggle re-probed every implementer round. ANY explicit
        // value - `on`, `off`, anything - is a run-start config error naming this spec.
        // Absent (the common case, and every workflow committed before either switch existed)
        // is unaffected.
        reject_retired_key(
            &wf.build.mutation,
            "build.mutation",
            "the `mutation:` line under `build:`",
            "spec 91: mutation testing now runs once, at a workflow's own `checkin` stage via a \
             `mutation` gate, never per implementer round - see \
             specs/91-mutation-runs-once-at-the-check-in-seam.md",
        )?;
        // The mutation gate's enabled-but-absent refusal (spec 91, MOVED from the retired
        // `build.mutation` switch above, GATES-LIST-DRIVEN): a workflow that DECLARES a gate
        // named `mutation` requires `cargo-mutants` resolvable on PATH at run start - the
        // operator wired the gate explicitly (whether or not any stage lists it yet), so
        // proceeding silently would let it fail loudly only mid-run, deep in a unit's
        // `checkin` stage. The SAME resolution `rigger validate`'s reporting surface would
        // read too - never a second, independently re-derived check.
        if wf.gates.contains_key(crate::gate::MUTATION_GATE_ID) {
            if let Err(e) = crate::gate::mutation_gate_binary_on_path() {
                return Err(err(e.to_string()));
            }
        }
        // The default review panel (applied to every unit) must reference real agents,
        // including its light-tier roster, and its depth policy must be structurally
        // sound (a configured light tier names an adjudicator).
        for aid in wf.defaults.review.agent_ids() {
            if !self.agents.contains_key(&aid) {
                return Err(err(format!(
                    "defaults.review references unknown agent {aid:?}"
                )));
            }
        }
        wf.defaults
            .review
            .validate_depth()
            .map_err(|m| err(format!("defaults.{m}")))?;
        for (name, st) in &wf.stages {
            for need in &st.needs {
                if !wf.stages.contains_key(need) {
                    return Err(err(format!("stage {name:?} needs unknown stage {need:?}")));
                }
            }
            for aid in st.agent_ids() {
                if !self.agents.contains_key(&aid) {
                    return Err(err(format!(
                        "stage {name:?} references unknown agent {aid:?}"
                    )));
                }
            }
            for g in &st.gates {
                if !wf.gates.contains_key(g) {
                    return Err(err(format!("stage {name:?} references unknown gate {g:?}")));
                }
            }
            // A per-stage `review` override that declares a depth policy must satisfy the
            // same light-tier-names-an-adjudicator invariant as the default panel.
            st.review
                .validate_depth()
                .map_err(|m| err(format!("stage {name:?} {m}")))?;
        }
        if let Some(cyc) = find_cycle(&wf.stages) {
            return Err(err(format!(
                "workflow has a dependency cycle involving stage {cyc:?}"
            )));
        }
        // Fail fast on a misauthored failure rule (an unknown class, an uncompilable
        // output_regex) at load rather than at the first classification, through the
        // SAME conversion the conductor uses.
        wf.failure_taxonomy()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::agent as agent_def;
    use crate::test_support::agent_with_prompt;

    // ---- instruction injection: the operator layer loader ----

    fn instructions_dir(root: &Path) -> std::path::PathBuf {
        let d = root.join(".rigger").join("instructions");
        std::fs::create_dir_all(&d).expect("create .rigger/instructions");
        d
    }

    #[test]
    fn load_instructions_is_empty_when_the_directory_is_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            load_instructions(tmp.path()).expect("an absent directory is not an error"),
            Vec::new()
        );
    }

    #[test]
    fn load_instructions_reads_md_files_in_filename_order_named_by_stem() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = instructions_dir(tmp.path());
        std::fs::write(d.join("20-team.md"), "Team rule.\n").unwrap();
        std::fs::write(d.join("10-house.md"), "House rule.\n").unwrap();
        let got = load_instructions(tmp.path()).expect("load");
        assert_eq!(
            got,
            vec![
                Instruction {
                    name: "10-house".into(),
                    body: "House rule.\n".into()
                },
                Instruction {
                    name: "20-team".into(),
                    body: "Team rule.\n".into()
                },
            ]
        );
    }

    #[test]
    fn load_instructions_ignores_non_md_files_and_the_directory_readme() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = instructions_dir(tmp.path());
        std::fs::write(d.join("notes.txt"), "not an instruction").unwrap();
        std::fs::write(d.join("README.md"), "what this directory is").unwrap();
        std::fs::write(d.join("10-house.md"), "House rule.").unwrap();
        let names: Vec<String> = load_instructions(tmp.path())
            .expect("load")
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert_eq!(names, vec!["10-house".to_string()]);
    }

    #[test]
    fn load_instructions_errors_naming_the_path_when_it_is_not_a_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join(".rigger")).unwrap();
        std::fs::write(tmp.path().join(".rigger").join("instructions"), "a file").unwrap();
        let msg = load_instructions(tmp.path()).unwrap_err().to_string();
        assert!(
            msg.contains("instructions"),
            "the error must name the path it could not read; got: {msg}"
        );
    }

    #[test]
    fn load_carries_the_operator_instructions_on_the_config() {
        fn copy_tree(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap() {
                let p = e.unwrap().path();
                let dest = to.join(p.file_name().unwrap());
                if p.is_dir() {
                    copy_tree(&p, &dest);
                } else {
                    std::fs::copy(&p, &dest).unwrap();
                }
            }
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        copy_tree(
            Path::new("../../examples/demo/.rigger"),
            &tmp.path().join(".rigger"),
        );
        std::fs::write(
            instructions_dir(tmp.path()).join("10-house.md"),
            "House rule.",
        )
        .unwrap();
        let cfg = load(tmp.path().to_str().unwrap()).expect("the demo definition loads");
        assert_eq!(
            cfg.instructions,
            vec![Instruction {
                name: "10-house".into(),
                body: "House rule.".into()
            }]
        );
    }

    // ---- spec 18, unit 1: gating-persona verdict-line static lint ----

    /// A config whose default review panel gates on an adjudicator carrying `prompt`.
    fn config_with_default_adjudicator(prompt: &str) -> Config {
        let mut cfg = Config::default();
        cfg.agents
            .insert("adj".into(), agent_with_prompt("adj", prompt));
        cfg.workflow.defaults.review.adjudicator = "adj".into();
        cfg
    }

    /// An adjudicator persona whose ONLY verdict path is `rigger_emit`: the guaranteed
    /// stall this lint targets (the gate reads the result channel and finds no verdict).
    const EMIT_ONLY: &str = "You are the Adjudicator. Weigh the lenses against the adversary \
        and decide. Record your verdict the moment you reach it via the rigger_emit tool with \
        type Verdict and data {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} \
        to reject. Do not add anything after you emit.";

    /// The compliant twin of [`EMIT_ONLY`]: it still records reasoning via `rigger_emit` but
    /// ENDS ITS OUTPUT with the verdict line the integration gate actually reads.
    const RESULT_LINE: &str = "You are the Adjudicator. Weigh the lenses against the adversary \
        and decide. Record your reasoning via the rigger_emit tool as you go. End your output \
        with a single line: {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to \
        reject.";

    #[test]
    fn verdict_line_matcher_flags_emit_only_and_passes_a_result_line_prompt() {
        assert!(
            !puts_verdict_on_result_channel(EMIT_ONLY),
            "a persona that records the verdict ONLY via rigger_emit puts no verdict on the \
             result channel"
        );
        assert!(
            puts_verdict_on_result_channel(RESULT_LINE),
            "a persona that ends its output with the verdict line is compliant (even though it \
             also emits reasoning)"
        );
    }

    #[test]
    fn verdict_line_matcher_never_flags_a_prompt_that_puts_the_verdict_on_the_result() {
        // A standalone JSON example with no emit instruction near it is on the result channel.
        assert!(puts_verdict_on_result_channel(
            "Decide, then write {\"verdict\":\"approve\"} and stop."
        ));
        // A prompt with no verdict literal at all is not flagged: there is no literal to judge,
        // and the lint trades that false negative away to never false-positive a compliant one.
        assert!(puts_verdict_on_result_channel(
            "Adjudicate the unit and record your decision."
        ));
    }

    /// spec 18, unit 1 hard promise (Design L32 / done-when L111): the lint may have false
    /// NEGATIVES but never a false POSITIVE - a persona that DOES present the verdict as its
    /// output must never be flagged. This pins that promise against the exact class the
    /// heuristic used to break on: a COMPLIANT verdict-line clause that happens to use words
    /// outside the output whitelist, while an UNRELATED `rigger_emit` instruction sits in a
    /// neighbouring sentence (rigger's own communication discipline puts one in every gating
    /// persona). Each of these presents `{"verdict"...}` AS OUTPUT and must pass.
    #[test]
    fn verdict_line_lint_never_false_positives_a_compliant_but_non_whitelisted_persona() {
        // Every string below is emit-adjacent (a `rigger_emit` / emit instruction is nearby,
        // as the discipline requires) yet presents the verdict literal as its OUTPUT, using
        // vocabulary the fixed output whitelist does not contain (finish/answer/closing/write).
        let compliant = [
            "Emit your reasoning as you go via rigger_emit. Then write your verdict as the \
             closing JSON: {\"verdict\":\"approve\"}",
            "Your verdict is the JSON {\"verdict\":\"approve\"}; also emit a DecisionMade as you \
             go.",
            "Weigh the lenses, record notes via rigger_emit. Finish with the JSON \
             {\"verdict\":\"approve\"}.",
            "Record your reasoning via rigger_emit. Your closing JSON must be \
             {\"verdict\":\"approve\"}.",
            "Emit each decision via rigger_emit. Write {\"verdict\":\"approve\"} as the closing \
             token.",
            "Deliberate and emit your reasoning via rigger_emit. Your answer is \
             {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "a persona that presents the verdict AS OUTPUT must never be flagged, even when \
                 an emit instruction sits in a neighbouring sentence and the verdict clause \
                 avoids the output whitelist:\n{prompt}"
            );
        }
        // The stall this lint targets is unchanged: a verdict literal that is genuinely the
        // payload of an emit call in its OWN clause, with no output cue, stays flagged.
        assert!(
            !puts_verdict_on_result_channel(EMIT_ONLY),
            "a persona whose verdict literal is the rigger_emit payload in its own clause is \
             still the stall this lint refuses"
        );
    }

    /// Every `compliant` prompt puts its verdict on the result channel (`compliant_why`), and
    /// every `stalls` prompt - the teeth - does not (`stall_why`).
    fn assert_verdict_channel(
        compliant: &[&str],
        compliant_why: &str,
        stalls: &[&str],
        stall_why: &str,
    ) {
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "{compliant_why}:\n{prompt}"
            );
        }
        for stall in stalls {
            assert!(
                !puts_verdict_on_result_channel(stall),
                "{stall_why}:\n{stall}"
            );
        }
    }

    crate::test_cases! {
        /// spec 18 unit 1's hard promise, pinned against the RESIDUAL false-positive class the
        /// prior fix left reachable (adj-u18-1 REJECT /
        /// adv-u18-1-residual-false-positive-same-clause-emit): an unrelated emit instruction
        /// sharing the SAME sentence as a verdict-output clause must NOT bind the literal. The
        /// earlier "any emit word in the clause" rule flagged these because an emit word
        /// (governing a DIFFERENT target - a DecisionMade, reasoning) sat in the clause with no
        /// whitelisted output cue; each nonetheless presents `{"verdict"...}` AS the verdict
        /// output and must PASS. None of these strings hit [`OUTPUT_CUES`], so they pin the
        /// emit-payload binding itself, not the whitelist. Compliance must NOT flip on clause
        /// order. Teeth preserved even IN-sentence: when the emit verb GENUINELY serializes the
        /// verdict as its `data` payload (not an unrelated target), it is still the stall -
        /// regardless of a determiner-`verdict` phrase, because the span to the literal carries
        /// the payload marker.
        verdict_line_lint_passes_an_unrelated_same_sentence_emit_before_a_verdict_output_clause:
            assert_verdict_channel(
                &[
                    // The exact string the adjudicator's directive named: an unrelated `emit a
                    // DecisionMade` instruction precedes a non-whitelisted verdict-output clause.
                    "You must emit a DecisionMade for each call and your verdict must be \
                     {\"verdict\":\"approve\"}.",
                    // Emit-clause first, verdict-output clause second, one sentence (no cue word).
                    "Emit each decision via rigger_emit, then your verdict is the JSON \
                     {\"verdict\":\"approve\"}.",
                    "Having emitted your reasoning, your verdict {\"verdict\":\"approve\"} governs.",
                    // The ORDER-FLIP of a previously-blessed string: verdict-first passed before;
                    // the emit-first reordering must pass too - compliance cannot flip on order.
                    "Also emit a DecisionMade as you go and your verdict is the JSON \
                     {\"verdict\":\"approve\"}.",
                    "Record each DecisionMade via rigger_emit as you go, and your verdict is \
                     {\"verdict\":\"approve\"}.",
                    "After you emit your DecisionMade events, your verdict is \
                     {\"verdict\":\"approve\"}.",
                    "Deliver two things: reasoning emitted via rigger_emit, and the verdict \
                     {\"verdict\":\"approve\"}.",
                ],
                "an unrelated emit instruction in the same sentence must not bind a verdict \
                 that is independently presented as output",
                &[
                    "Emit a note, then rigger_emit your verdict as data {\"verdict\":\"approve\"}.",
                    "Record notes and rigger_emit the verdict with data {\"verdict\":\"approve\"}.",
                ],
                "a verdict serialized as the emit `data` payload is still the stall",
            );
        /// spec 18 unit 1's hard promise, pinned against the PAYLOAD-SLOT residual
        /// false-positive class (adj-u18-1rr REJECT /
        /// adv-u18-1rr-residual-fp-defeats-your-verdict-escape): an explicit
        /// determiner-`verdict` presentation ("your verdict ... is {..}") is on the result
        /// channel even when a payload-slot noun sits in the span but does NOT abut the
        /// literal. The prior rule treated ANY payload-slot word in the span as defeating the
        /// presentation, so it flagged these clearly-compliant personas; only a payload word
        /// IMMEDIATELY abutting the literal ("as data {..}") marks it as the serialized emit
        /// data. None of these strings carries an output-cue word, and each shares its sentence
        /// with an unrelated emit instruction, so they pin the determiner-verdict escape against
        /// a payload noun in the span - not the output whitelist. Teeth: when the payload word
        /// IMMEDIATELY abuts the literal after an emit, the verdict IS the serialized emit data
        /// - still the stall, even behind a determiner-`verdict` phrase.
        verdict_line_lint_passes_a_determiner_verdict_when_a_payload_noun_sits_in_the_span:
            assert_verdict_channel(
                &[
                    // A dropped ambiguous common noun ("value"/"object") after the verdict subject.
                    "Emit each decision via rigger_emit and your verdict value is \
                     {\"verdict\":\"approve\"}.",
                    "Emit each decision via rigger_emit and your verdict object is \
                     {\"verdict\":\"approve\"}.",
                    // A KEPT emit-API token ("payload"/"data") that does NOT abut the literal -
                    // descriptive here, not the serialized argument, so the presentation stands.
                    "Emit each decision via rigger_emit and the verdict payload is \
                     {\"verdict\":\"approve\"}.",
                    "Record notes via rigger_emit as you go, and your verdict data is \
                     {\"verdict\":\"approve\"}.",
                ],
                "a determiner-verdict presentation is on the result channel even when a payload \
                 noun sits in its span but does not abut the literal",
                &[
                    "Emit a note, then rigger_emit your verdict as data {\"verdict\":\"approve\"}.",
                    "Record decisions, then rigger_emit the verdict payload \
                     {\"verdict\":\"approve\"}.",
                ],
                "a verdict literal a payload word directly introduces is the serialized emit \
                 data - still the stall",
            );
        /// spec 18 unit 1's hard promise, pinned against the UNRELATED-EMIT-EXAMPLE-BRACE
        /// false-positive class (adj-u18-1r3 REJECT, FP#1): an explicit determiner-`verdict`
        /// presentation ("your verdict is {..}") is on the result channel even when an
        /// UNRELATED emit-payload EXAMPLE brace ("... data {id} ...") shares its clause EARLIER,
        /// before the `verdict` word. The prior rule scanned EVERY brace in the clause, so a
        /// different literal's `data {id}` example short-circuited the determiner-verdict escape
        /// to a false flag - it fired on the EXACT wording rigger's own communication
        /// discipline mandates of every gating persona. The fix scopes the emit-payload test to
        /// the SPAN from the `verdict` word to the trailing literal, so an example brace before
        /// the presentation no longer defeats it. None of these strings carries an output cue,
        /// and each shares its sentence with a genuine `rigger_emit ... data {id}` example.
        /// Teeth preserved: when the payload word abuts the TRAILING verdict literal itself (in
        /// the presentation span), it IS the serialized emit data - still the stall, even behind
        /// a determiner-`verdict` phrase and even with an unrelated example brace elsewhere.
        verdict_line_lint_passes_a_determiner_verdict_when_an_unrelated_emit_example_brace_precedes_it:
            assert_verdict_channel(
                &[
                    // A/B delta of the reject: only difference from a passing twin is an inline
                    // `data {id}` example after the emit token; the escape must survive it.
                    "Record each decision via rigger_emit with data {id}, and your verdict is \
                     {\"verdict\":\"approve\"}.",
                    // The A-side twin (no example brace) - a control that must also pass.
                    "Record each decision via rigger_emit with a DecisionMade payload, and your \
                     verdict is {\"verdict\":\"approve\"}.",
                    // The EXACT rigger DecisionMade-discipline wording mandated of every gating
                    // persona: an emit-payload example `data {id,summary}` precedes the
                    // determiner-verdict presentation.
                    "Record every decision via rigger_emit with type DecisionMade and data \
                     {id,summary}, then your verdict is {\"verdict\":\"approve\"}.",
                    // The example brace can even be a `{id}` immediately after `rigger_emit`;
                    // still unrelated to the trailing determiner-verdict literal.
                    "Emit each decision as rigger_emit {id}, and your verdict is \
                     {\"verdict\":\"approve\"}.",
                ],
                "an unrelated emit-payload EXAMPLE brace earlier in the clause must not defeat a \
                 determiner-verdict presentation of the trailing literal",
                &[
                    "Record notes via rigger_emit {id}, then rigger_emit your verdict as data \
                     {\"verdict\":\"approve\"}.",
                    "Emit a note with data {id}, then rigger_emit the verdict with data \
                     {\"verdict\":\"approve\"}.",
                ],
                "a verdict literal a payload word directly introduces IN the presentation span \
                 is the serialized emit data - still the stall",
            );
    }

    /// spec 18 unit 1 residual class (adj-u18-1rr): a natural output verb OUTSIDE the fixed output
    /// whitelist ("report"/"deliver"/"send"), whose object is a dropped ambiguous common noun
    /// ("the value {..}"/"the object {..}"), presents the verdict as output and must PASS. Each
    /// string reaches the final emit-payload test (signal d: an emit instruction in the clause, no
    /// output cue, no determiner-verdict) and passes ONLY because the literal is NOT the serialized
    /// emit data. This pins the `literal_is_emit_payload == false` direction directly (mutation gap
    /// adv/sdet-u18-1r-emit-payload-narrowing-unpinned): forcing it TRUE turns every line RED.
    #[test]
    fn verdict_line_lint_passes_a_natural_output_verb_only_via_the_literal_not_being_emit_payload()
    {
        let compliant = [
            "Emit your reasoning via rigger_emit, then report the value {\"verdict\":\"approve\"}.",
            "Emit your reasoning via rigger_emit, then deliver the object {\"verdict\":\"approve\"}.",
            "Emit your reasoning via rigger_emit, then send the value {\"verdict\":\"approve\"}.",
            // A KEPT emit-API token ("data") in the clause but not abutting the literal.
            "Emit your reasoning as structured data via rigger_emit, then {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "a natural output verb outside the whitelist, whose object is not an emit \
                 payload-slot word abutting the literal, presents the verdict as output - it must \
                 PASS:\n{prompt}"
            );
        }
    }

    /// Direct pin on the emit-payload recognizer (mutation gap adv/sdet-u18-1r-emit-payload-
    /// narrowing-unpinned): the literal is the serialized emit data ONLY when a payload-slot word
    /// (or the emit verb) IMMEDIATELY abuts it, never when a payload noun sits earlier in the
    /// clause. This pins BOTH directions - mutating `literal_is_emit_payload` to a constant turns
    /// one arm RED. The argument is the lowercased introducing clause, which ends at the `{` of the
    /// literal (see [`introducing_clause`]).
    #[test]
    fn literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word() {
        // Abutting payload word after an emit -> genuinely the serialized emit data.
        assert!(literal_is_emit_payload(
            "record notes and rigger_emit the verdict with data {"
        ));
        // Bare emit abutting the literal -> the emit verb directly introduces it.
        assert!(literal_is_emit_payload("record notes, then rigger_emit {"));
        // Payload word present but NOT abutting the literal ("value is {") -> not the emit data.
        assert!(!literal_is_emit_payload(
            "emit each decision via rigger_emit and your verdict value is {"
        ));
        // A dropped common noun abutting the literal -> not a payload slot at all.
        assert!(!literal_is_emit_payload(
            "emit your reasoning via rigger_emit, then report the value {"
        ));
        // No emit instruction in the clause -> never the emit data, even with an abutting payload.
        assert!(!literal_is_emit_payload("here is the data {"));

        // Alternation continuation: the trailing literal abuts a connective (`or`) but an earlier
        // `{"verdict"` sibling IS payload-bound (`data {"verdict":"approve"}`), so it is the emit's
        // other serialized alternative - still the payload. This pins
        // `trailing_literal_is_payload_alternation`: neutralizing it turns the EMIT_ONLY alternation
        // GREEN (a false negative the reject demanded stay FLAGGED).
        assert!(literal_is_emit_payload(
            "rigger_emit with data {\"verdict\":\"approve\"} to approve or {"
        ));
        // But a connective-joined trailing literal whose earlier sibling is an UNRELATED example
        // brace (`data {id}`, not a `{"verdict"` literal) is NOT an alternation - biases to PASS.
        assert!(!literal_is_emit_payload("rigger_emit with data {id} or {"));
        // A connective with no payload-bound `{"verdict"` sibling at all is not an alternation.
        assert!(!literal_is_emit_payload(
            "emit each decision via rigger_emit, then finish with a or {"
        ));
    }

    #[test]
    fn lint_hard_errors_on_an_emit_only_gating_adjudicator_naming_the_fix() {
        let cfg = config_with_default_adjudicator(EMIT_ONLY);
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(msg.contains("\"adj\""), "names the offending agent: {msg}");
        assert!(
            msg.contains("gating role") && msg.contains("verdict line"),
            "names the defect: {msg}"
        );
        assert!(
            msg.contains("rigger_emit will never gate"),
            "names why an emit-only verdict never gates: {msg}"
        );
    }

    #[test]
    fn lint_passes_a_gating_adjudicator_that_ends_with_the_verdict_line() {
        let cfg = config_with_default_adjudicator(RESULT_LINE);
        assert!(lint_gating_verdict_lines(&cfg).is_ok());
    }

    #[test]
    fn the_verdict_line_lint_ignores_non_gating_roles() {
        // Only the adjudicator's result-channel verdict gates integration; an emit-only lens or
        // adversary is not a gating role, so the lint leaves it alone.
        let mut cfg = Config::default();
        cfg.agents
            .insert("adj".into(), agent_with_prompt("adj", RESULT_LINE));
        cfg.agents
            .insert("adv".into(), agent_with_prompt("adv", EMIT_ONLY));
        cfg.agents
            .insert("lens".into(), agent_with_prompt("lens", EMIT_ONLY));
        cfg.workflow.defaults.review.adjudicator = "adj".into();
        cfg.workflow.defaults.review.adversary = "adv".into();
        cfg.workflow.defaults.review.lenses = vec!["lens".into()];
        assert!(
            lint_gating_verdict_lines(&cfg).is_ok(),
            "a compliant adjudicator passes even when the adversary/lens personas are emit-only"
        );
    }

    #[test]
    fn the_verdict_line_lint_reaches_the_light_tier_adjudicator() {
        let mut cfg = Config::default();
        cfg.agents
            .insert("full".into(), agent_with_prompt("full", RESULT_LINE));
        cfg.agents
            .insert("light".into(), agent_with_prompt("light", EMIT_ONLY));
        cfg.workflow.defaults.review.adjudicator = "full".into();
        let mut depth = ReviewDepth::default();
        depth.light.adjudicator = "light".into();
        cfg.workflow.defaults.review.tiers = Some(Box::new(depth));
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(
            msg.contains("\"light\""),
            "the light-tier adjudicator is a gating role too: {msg}"
        );
    }

    #[test]
    fn the_verdict_line_lint_reaches_a_standalone_stage_adjudicator_that_is_not_a_critique_gate() {
        // A stage's own `adjudicator` renders a gating verdict, so its persona is linted like a
        // review-panel adjudicator. This stage is NOT the conductor-injected plan-critique gate
        // (there is no producer stage for it to `needs`, and `build_dag_critique_prompt` only
        // injects the verdict line for that gate), so its persona must carry the verdict line
        // itself and an emit-only one is flagged. The excluded critique-gate case is pinned
        // separately by `the_verdict_line_lint_excludes_the_conductor_injected_plan_critique_gate`.
        let mut cfg = Config::default();
        cfg.agents
            .insert("critic".into(), agent_with_prompt("critic", EMIT_ONLY));
        let st = Stage {
            adjudicator: "critic".into(),
            ..Default::default()
        };
        cfg.workflow.stages.insert("decision-gate".into(), st);
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(
            msg.contains("\"critic\""),
            "a standalone stage adjudicator that is not the injected critique gate is linted: {msg}"
        );
    }

    /// spec 18 unit 1's hard promise, pinned against the PLAN-CRITIQUE false-positive class
    /// (adj-u18-1r3 REJECT, FP#2): the conductor builds the plan-critique / DAG-critique gate
    /// adjudicator's prompt via `build_dag_critique_prompt`, which ALWAYS appends the result-channel
    /// verdict line ("Render your final verdict as a JSON line: {..}"). So that gate's persona need
    /// NOT carry the verdict line, and linting an emit-only DAG-critique adjudicator is a false
    /// positive that would REFUSE a legitimate run (unit 2 escalates the lint to a run-start
    /// refusal). `gating_agent_ids` excludes the critique-gate stage's standalone adjudicator; the
    /// per-unit review adjudicator (whose `build_prompt` injects nothing) stays linted.
    #[test]
    fn the_verdict_line_lint_excludes_the_conductor_injected_plan_critique_gate() {
        // A DEDICATED emit-only DAG-critique adjudicator, used ONLY at the plan-critique gate: the
        // conductor injects its verdict line, so it must NOT be flagged.
        let mut cfg = Config::default();
        cfg.agents
            .insert("planner".into(), agent_with_prompt("planner", RESULT_LINE));
        cfg.agents.insert(
            "dag-critic".into(),
            agent_with_prompt("dag-critic", EMIT_ONLY),
        );
        cfg.agents
            .insert("adj".into(), agent_with_prompt("adj", RESULT_LINE));
        // The producer stage (a planner that `produces` a DAG).
        let plan = Stage {
            agent: "planner".into(),
            produces: "dag".into(),
            ..Default::default()
        };
        cfg.workflow.stages.insert("plan".into(), plan);
        // The plan-critique gate: review-only (no `agent`), carries an adjudicator, needs the
        // producer - exactly `conductor::critique_gate_name`'s recognition.
        let gate = Stage {
            adjudicator: "dag-critic".into(),
            needs: vec!["plan".into()],
            ..Default::default()
        };
        cfg.workflow.stages.insert("plan-critique".into(), gate);
        // A compliant per-unit review adjudicator so the config is otherwise clean.
        cfg.workflow.defaults.review.adjudicator = "adj".into();
        assert!(
            lint_gating_verdict_lines(&cfg).is_ok(),
            "the conductor-injected plan-critique gate adjudicator must not be flagged for an \
             emit-only persona: {:?}",
            lint_gating_verdict_lines(&cfg)
        );

        // But if that SAME emit-only persona ALSO serves the per-unit review adjudicator (whose
        // build_prompt injects no verdict line), it is a real stall and stays flagged - the
        // exclusion is scoped to the critique-gate role, not the agent id everywhere.
        let mut cfg2 = cfg.clone();
        cfg2.workflow.defaults.review.adjudicator = "dag-critic".into();
        let msg = lint_gating_verdict_lines(&cfg2).unwrap_err().to_string();
        assert!(
            msg.contains("\"dag-critic\""),
            "an emit-only adjudicator that also gates per-unit review (build_prompt injects \
             nothing) is still flagged: {msg}"
        );
    }

    #[test]
    fn the_verdict_line_lint_reaches_a_stage_review_override_adjudicator() {
        // A per-stage `review:` override names its OWN adjudicator, a gating role spec 18
        // enumerates; the lint reaches it through `st.review.gating_agent_ids()`. This pins the
        // stage-review collection line directly (the standalone-adjudicator test above exercises a
        // different field), so dropping it can no longer ship green.
        let mut cfg = Config::default();
        cfg.agents
            .insert("sjudge".into(), agent_with_prompt("sjudge", EMIT_ONLY));
        let mut st = Stage::default();
        st.review.adjudicator = "sjudge".into();
        cfg.workflow.stages.insert("implement".into(), st);
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(
            msg.contains("\"sjudge\""),
            "a stage review-override adjudicator is a gating role too: {msg}"
        );
    }

    #[test]
    fn parses_agent_frontmatter_and_body() {
        let b = b"---\nid: builder\nmodel: sonnet\ntools: [Read, Edit]\n---\nYou are a builder.\n";
        let a = parse_agent(b).unwrap();
        assert_eq!(a.id, "builder");
        assert_eq!(a.model, "sonnet");
        assert_eq!(a.tools, ["Read", "Edit"]);
        assert_eq!(a.prompt, "You are a builder.");
    }

    #[test]
    fn rejects_missing_frontmatter() {
        assert!(parse_agent(b"no frontmatter here").is_err());
    }

    /// Spec 102, criterion 3 (AN UNKNOWN KEY IS NAMED) scopes "every config level" to the
    /// internal `workflow.yml` schema tree - `AgentDef` deliberately stays permissive of an
    /// unmodeled frontmatter field, since `rigger setup --agents` imports whole foreign
    /// agent collections (the Claude Code / agency-agents shape) carrying fields Rigger
    /// never models at all (`description`, ...). A regression guard: this must keep
    /// parsing, not start rejecting `description` the way a `workflow.yml` field would.
    #[test]
    fn parse_agent_tolerates_an_unmodeled_frontmatter_key() {
        let b = b"---\nid: researcher\ndescription: digs up prior art\n---\nBody.\n";
        let a = parse_agent(b).expect(
            "AgentDef must stay permissive of unmodeled frontmatter fields (foreign agent \
             collections import through this same parse)",
        );
        assert_eq!(a.id, "researcher");
    }

    /// Spec 102's amended Design (THE DOTTED PATH IS TRACKED STRUCTURALLY) names the config
    /// parse sites as "the workflow file and the agent frontmatter": `parse_agent` is wired
    /// through the SAME [`crate::config::parse_yaml_naming_unknown_keys`] as `load_workflow`,
    /// not a second, independently-maintained `serde_yaml::from_str` call. `AgentDef` stays
    /// permissive of unknown keys (the test above), so this cannot pin an unknown-key
    /// rejection here - it pins the OTHER half of the Design clause instead: a genuine parse
    /// error (a type mismatch) still surfaces `serde_yaml`'s own message unchanged, proving
    /// the shared function's passthrough branch, not a divergent one, is what runs here.
    #[test]
    fn parse_agent_routes_a_type_mismatch_through_the_shared_structural_parser() {
        let b = b"---\nid: builder\nrecurse: not-a-bool\n---\nBody.\n";
        let raw = serde_yaml::from_str::<AgentDef>("id: builder\nrecurse: not-a-bool\n")
            .expect_err("must fail to parse")
            .to_string();
        let msg = parse_agent(b).expect_err("a bool field given a string must fail to parse");
        assert!(
            msg.to_string().contains(&raw),
            "a genuine type mismatch in agent frontmatter must still surface serde_yaml's own \
             message unchanged, exactly as the shared parser's passthrough branch does for \
             workflow.yml: {msg}"
        );
    }

    /// The structural parser (spec 102, criterion 3): [`serde_path_to_error`] tracks the
    /// dotted path from the ACTUAL field/key accesses the deserializer performs, so
    /// [`parse_yaml_naming_unknown_keys`] never has to search the rendered error text for
    /// it. A nested violation names `parent.field`; a root-level violation (no parent
    /// path) names the bare field; anything that is not this error shape (a type mismatch
    /// here) passes through with `serde_yaml`'s own message unchanged.
    #[test]
    fn parse_yaml_naming_unknown_keys_recomposes_the_nested_path_and_passes_through_other_errors() {
        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Inner {
            #[serde(default)]
            #[allow(dead_code)]
            known: String,
        }
        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Outer {
            #[serde(default)]
            #[allow(dead_code)]
            inner: Inner,
        }

        let nested_err =
            crate::config::parse_yaml_naming_unknown_keys::<Outer>("inner:\n  knwon: x\n")
                .expect_err("must fail to parse");
        assert_eq!(
            nested_err, "inner.knwon: unknown key",
            "a nested unknown field recomposes the tracked path and the leaf field name into \
             one dotted token"
        );

        let root_err = crate::config::parse_yaml_naming_unknown_keys::<Outer>("bogus: 1\n")
            .expect_err("must fail to parse");
        assert_eq!(
            root_err, "bogus: unknown key",
            "a root-level unknown field (no parent path) names the bare field"
        );

        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct TypeMismatch {
            n: u32,
        }
        let raw_type_err = serde_yaml::from_str::<TypeMismatch>("n: not-a-number\n")
            .expect_err("must fail to parse")
            .to_string();
        let type_err =
            crate::config::parse_yaml_naming_unknown_keys::<TypeMismatch>("n: not-a-number\n")
                .expect_err("must fail to parse");
        assert_eq!(
            type_err, raw_type_err,
            "a non-unknown-field error (a type mismatch here) passes through unchanged"
        );

        // adv-u102c3-dotted-unknown-key-corrupts-type-mismatch-errors: a type mismatch whose
        // own invalid-value TEXT happens to echo the unknown-field marker wording must still
        // pass through with serde_yaml's raw message, never be corrupted into a fabricated
        // unknown-key report. The structural path for this input is the KNOWN field `n`, not
        // an unknown one, so the exact-prefix check below never even considers the marker text
        // the value happens to contain.
        let body = "n: unknown field `evil`, expected `n`\n";
        let raw_embedded_err = serde_yaml::from_str::<TypeMismatch>(body)
            .expect_err("must fail to parse")
            .to_string();
        let type_err_with_embedded_marker =
            crate::config::parse_yaml_naming_unknown_keys::<TypeMismatch>(body)
                .expect_err("must fail to parse");
        assert_eq!(
            type_err_with_embedded_marker, raw_embedded_err,
            "a type-mismatch error whose own invalid-value text echoes the unknown-field \
             marker wording must still pass through unchanged, never be mistaken for a real \
             unknown-key violation"
        );

        // adv-u102c3-dotted-unknown-key-truncates-backtick-in-key-name: an unknown field whose
        // own name contains a backtick must be named in full - the tracker captures the exact
        // key text it read, never a text-search substring of it.
        let backtick_field_err =
            crate::config::parse_yaml_naming_unknown_keys::<Outer>("weird`field: 1\n")
                .expect_err("must fail to parse");
        assert_eq!(
            backtick_field_err, "weird`field: unknown key",
            "a field name that itself contains a backtick is named in full"
        );

        // adv-u102c3-r2-terminator-embedded-in-field-name-still-truncates: an unknown field
        // whose own name contains the exact backtick-comma-space run `serde`'s own trailer
        // uses defeated a text search anchored on that run. The tracker never searches the
        // rendered trailer at all, so this is named in full regardless.
        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ManyFields {
            #[serde(default)]
            #[allow(dead_code)]
            a: String,
            #[serde(default)]
            #[allow(dead_code)]
            b: String,
            #[serde(default)]
            #[allow(dead_code)]
            c: String,
        }
        let terminator_embedded_err =
            crate::config::parse_yaml_naming_unknown_keys::<ManyFields>("\"z`, y\": 1\n")
                .expect_err("must fail to parse");
        assert_eq!(
            terminator_embedded_err, "z`, y: unknown key",
            "a field name that itself embeds the backtick-comma-space terminator is recovered \
             in full: {terminator_embedded_err}"
        );

        // A field name that embeds the marker text itself, at the document root, must not
        // confuse the path/marker recovery into inventing a bogus non-root path.
        let marker_embedded_at_root_err =
            crate::config::parse_yaml_naming_unknown_keys::<ManyFields>(
                "\"evil: unknown field `x\": 1\n",
            )
            .expect_err("must fail to parse");
        assert_eq!(
            marker_embedded_at_root_err, "evil: unknown field `x: unknown key",
            "a root-level field name that embeds marker-shaped text is still named in full, \
             not mistaken for a nested path: {marker_embedded_at_root_err}"
        );

        // op-102-c3-dotted-path-structural-tracker: the case that DEFEATED every text-search
        // round (an unknown key whose own name echoes the reformatter's own path/marker
        // boundary, genuinely ambiguous to any scan) is no longer ambiguous at all once the
        // path comes from the tracker: the final segment IS the literal key that was read,
        // regardless of what text it contains.
        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct BoundaryOuter {
            #[serde(default)]
            #[allow(dead_code)]
            inner: ManyFields,
        }
        let boundary_echoing_err = crate::config::parse_yaml_naming_unknown_keys::<BoundaryOuter>(
            "inner:\n  \"a: unknown field `b\": 1\n",
        )
        .expect_err("must fail to parse");
        assert_eq!(
            boundary_echoing_err, "inner.a: unknown field `b: unknown key",
            "a key that echoes the reformatter's own path/marker boundary text is still named \
             in full, structurally, never declined: {boundary_echoing_err}"
        );
    }

    /// A `deny_unknown_fields` stage probe with one known `agent` field, keyed by name under a
    /// `stages:` map ([`WithStages`]) - the shape where a stage name's own text lands in the
    /// reported path as a MAP KEY, not a struct field name.
    #[derive(Debug, Default, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ProbeStage {
        #[serde(default)]
        #[allow(dead_code)]
        agent: String,
    }

    #[derive(Debug, Default, serde::Deserialize)]
    struct WithStages {
        #[serde(default)]
        #[allow(dead_code)]
        stages: BTreeMap<String, ProbeStage>,
    }

    /// The rendered error of parsing `yaml` into `T`, which must fail.
    fn unknown_key_err<T: serde::de::DeserializeOwned + std::fmt::Debug>(yaml: &str) -> String {
        crate::config::parse_yaml_naming_unknown_keys::<T>(yaml).expect_err("must fail to parse")
    }

    /// Two cases where the offending key or a path segment is sourced from arbitrary
    /// operator-writable YAML text (a MAP KEY, unlike a plain struct field name) that echoes
    /// text a naive path/marker split would anchor on - the tracker composes the path from
    /// the actual map-key accesses instead, so both are recovered correctly with no split at
    /// all:
    /// - sdet-u102c3-colon-in-map-key-path-defeats-reformat: a `stages:` map key carrying its
    ///   own `": "`.
    /// - the round-3 case that made every text-search reformatter decline to guess (spec 102
    ///   Design amendment, THE DOTTED PATH IS TRACKED STRUCTURALLY): an unknown key literally
    ///   named `` `z: unknown field `y` `` under `defaults:`-shaped nesting, which makes the
    ///   marker text appear twice in the rendered message.
    #[test]
    fn parse_yaml_naming_unknown_keys_recomposes_paths_through_colon_and_marker_bearing_map_keys() {
        let map_key_err = unknown_key_err::<WithStages>("stages:\n  \"foo: bar\":\n    gatez: x\n");
        assert_eq!(
            map_key_err, "stages.foo: bar.gatez: unknown key",
            "a map-key path segment containing its own ': ' is recovered in full: {map_key_err}"
        );

        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Defaults {
            #[serde(default)]
            #[allow(dead_code)]
            autonomy: String,
        }
        #[derive(Debug, Default, serde::Deserialize)]
        struct Workflow {
            #[serde(default)]
            #[allow(dead_code)]
            defaults: Defaults,
        }
        let boundary_err = unknown_key_err::<Workflow>("defaults:\n  \"z: unknown field `y\": 1\n");
        assert_eq!(
            boundary_err, "defaults.z: unknown field `y: unknown key",
            "the dotted path is named even when the key itself echoes the path/marker \
             boundary text: {boundary_err}"
        );
    }

    /// adj-u102c3-r4-verdict-reject-path-composition-bugs /
    /// sdet-u102c3-r4-seq-index-in-parent-defeats-classification: round 4's `parent` was
    /// built with an unconditional `.join(".")` over pre-stringified segments, which puts a
    /// `.` before a `Segment::Seq` index - `serde_yaml`'s own rendering (and the tracker's
    /// own `Path::Display` for the FULL path, already relied on one line above) never does.
    /// An unknown field inside a `Vec<T>` element therefore made `expected_prefix` diverge
    /// from `inner_msg`, so the classification's `starts_with` check silently failed for a
    /// GENUINE unknown-key violation and the function fell through to the raw message - on
    /// production schema (`defaults.failure_rules: Vec<FailureRuleDef>`), with an ordinary
    /// typo, no adversarial input at all.
    #[test]
    fn parse_yaml_naming_unknown_keys_recomposes_the_path_past_a_seq_index() {
        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Elem {
            #[serde(default)]
            #[allow(dead_code)]
            known: u32,
        }
        #[derive(Debug, Default, serde::Deserialize)]
        struct WithSeq {
            #[serde(default)]
            #[allow(dead_code)]
            items: Vec<Elem>,
        }
        let seq_err = crate::config::parse_yaml_naming_unknown_keys::<WithSeq>(
            "items:\n  - known: 1\n  - bogus: 2\n",
        )
        .expect_err("must fail to parse");
        assert_eq!(
            seq_err, "items[1].bogus: unknown key",
            "an unknown field inside a Vec<T> element names the dotted path with NO \
             separator before the [N] index, matching serde_yaml's own rendering: {seq_err}"
        );
    }

    /// sdet-u102c3-r4-dot-in-map-key-recomposes-ambiguous-path: an unescaped join makes a
    /// map key that literally contains "." (a stage name, e.g. - already legal, no
    /// character restriction) recompose into a dotted path structurally indistinguishable
    /// from genuine nesting: a stage "foo.bar" with a plain unknown field, and a stage "foo"
    /// with a nested substructure "bar" holding the SAME unknown field, would both render
    /// the byte-identical "stages.foo.bar.<field>" text under an unconditional join.
    /// Backslash-escaping a literal "." (and the escape character itself) inside a
    /// Map/Enum key's own text resolves that: only an UN-escaped "." is ever a real
    /// separator, so the two cases below are now textually distinct.
    #[test]
    fn parse_yaml_naming_unknown_keys_escapes_a_literal_dot_inside_a_map_key() {
        let dotted_key_err =
            unknown_key_err::<WithStages>("stages:\n  \"foo.bar\":\n    bogus_field: 1\n");
        assert_eq!(
            dotted_key_err, "stages.foo\\.bar.bogus_field: unknown key",
            "a literal '.' inside a single map key is backslash-escaped so it cannot read as \
             a separator between two segments: {dotted_key_err}"
        );

        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Nested {
            #[serde(default)]
            #[allow(dead_code)]
            known: String,
        }
        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StageWithNested {
            #[serde(default)]
            #[allow(dead_code)]
            bar: Nested,
        }
        #[derive(Debug, Default, serde::Deserialize)]
        struct WithNestedStages {
            #[serde(default)]
            #[allow(dead_code)]
            stages: BTreeMap<String, StageWithNested>,
        }
        let genuinely_nested_err = unknown_key_err::<WithNestedStages>(
            "stages:\n  foo:\n    bar:\n      bogus_field: 1\n",
        );
        assert_eq!(
            genuinely_nested_err, "stages.foo.bar.bogus_field: unknown key",
            "true nesting (a stage \"foo\" containing substructure \"bar\") has no embedded \
             dot to escape, so it stays plain-dotted: {genuinely_nested_err}"
        );
        assert_ne!(
            dotted_key_err, genuinely_nested_err,
            "a stage literally named 'foo.bar' must render differently from a stage 'foo' \
             nested under 'bar' - an unconditional join made these byte-identical"
        );
    }

    #[test]
    fn model_ladder_parses_from_frontmatter() {
        // Agent frontmatter accepts a `model_ladder` list (spec 10 unit 4): the cheap-first
        // cascade the agent escalates through under remediation.
        let b = b"---\nid: worker\nmodel_ladder: [haiku, sonnet, opus]\n---\nImplement.\n";
        let a = parse_agent(b).unwrap();
        assert_eq!(a.id, "worker");
        assert_eq!(a.model_ladder, ["haiku", "sonnet", "opus"]);
        // Absent a `model:` line, the single-model field stays empty (the ladder is authority).
        assert_eq!(a.model, "");
    }

    #[test]
    fn model_for_attempt_advances_one_rung_per_attempt_and_clamps_at_the_last() {
        // A unit's first attempt resolves the first rung and each remediation attempt advances
        // one rung, CLAMPED at the last once exhausted (spec 10 unit 4).
        let a = AgentDef {
            id: "worker".into(),
            model_ladder: vec!["haiku".into(), "sonnet".into(), "opus".into()],
            ..Default::default()
        };
        assert_eq!(a.model_for_attempt(0), "haiku", "attempt 0 -> rung 0");
        assert_eq!(a.model_for_attempt(1), "sonnet", "attempt 1 -> rung 1");
        assert_eq!(a.model_for_attempt(2), "opus", "attempt 2 -> rung 2 (last)");
        assert_eq!(
            a.model_for_attempt(3),
            "opus",
            "past the end clamps at the last rung"
        );
        assert_eq!(
            a.model_for_attempt(99),
            "opus",
            "far past the end still clamps"
        );
    }

    #[test]
    fn a_single_model_is_a_one_rung_ladder_used_on_every_attempt() {
        // With no ladder, the single `model` alias is returned on every attempt - so an agent
        // that only sets `model` behaves EXACTLY as before the cascade existed (back-compat).
        let one = AgentDef {
            id: "worker".into(),
            model: "sonnet".into(),
            ..Default::default()
        };
        for attempt in [0, 1, 5, 42] {
            assert_eq!(
                one.model_for_attempt(attempt),
                "sonnet",
                "a lone model: does not ladder - attempt {attempt} still resolves it"
            );
        }
        // Neither declared: empty (the driver's default model is inherited), on every attempt.
        let none = AgentDef {
            id: "worker".into(),
            ..Default::default()
        };
        assert_eq!(none.model_for_attempt(0), "");
        assert_eq!(none.model_for_attempt(3), "");
    }

    #[test]
    fn a_declared_ladder_takes_precedence_over_a_lone_model() {
        // When both are set the ladder is authority and `model` is ignored, so the resolved
        // rung is never silently overridden by the shorthand field.
        let a = AgentDef {
            id: "worker".into(),
            model: "haiku".into(),
            model_ladder: vec!["sonnet".into(), "opus".into()],
            ..Default::default()
        };
        assert_eq!(a.model_for_attempt(0), "sonnet");
        assert_eq!(a.model_for_attempt(1), "opus");
    }

    #[test]
    fn parses_agent_max_wall_clock_and_defaults_to_none_when_absent() {
        let with = parse_agent(b"---\nid: slow\nmax_wall_clock: 1800\n---\nbody\n").unwrap();
        assert_eq!(
            with.max_wall_clock,
            Some(1800),
            "an explicit per-role max_wall_clock parses through"
        );
        let without = parse_agent(b"---\nid: plain\n---\nbody\n").unwrap();
        assert_eq!(
            without.max_wall_clock, None,
            "an absent max_wall_clock is None (inherits the workflow default)"
        );
    }

    /// Parsing `yaml` as a [`Workflow`] yields `expected` for the `field` it reads.
    fn assert_parsed<T: PartialEq + std::fmt::Debug>(
        yaml: &str,
        field: impl Fn(&Workflow) -> T,
        expected: T,
        why: &str,
    ) {
        let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(field(&wf), expected, "{why}");
    }

    crate::test_cases! {
        defaults_max_wall_clock_parses_and_is_zero_when_absent: {
            assert_parsed(
                "defaults:\n  max_wall_clock: 600\n",
                |w| w.defaults.max_wall_clock,
                600,
                "an explicit defaults.max_wall_clock parses through",
            );
            assert_parsed(
                "{}",
                |w| w.defaults.max_wall_clock,
                0,
                "an absent default is 0 (unbounded - liveness timeouts are opt-in)",
            );
        };
        /// The remediation-depth knob parses from `defaults.max_retries` and is 0 when omitted
        /// - the sentinel the conductor reads as "fall back to the historical default of 3", so
        /// an un-set workflow is exactly back-compatible.
        max_retries_parses_from_defaults_and_defaults_to_zero_when_absent: {
            assert_parsed(
                "defaults:\n  max_retries: 6\n",
                |w| w.defaults.max_retries,
                6,
                "an explicit defaults.max_retries must parse through",
            );
            assert_parsed(
                "defaults:\n  budget: 60\n",
                |w| w.defaults.max_retries,
                0,
                "an absent defaults.max_retries must default to 0 (the fall-back-to-3 sentinel)",
            );
        };
        /// Spec 65: `build.wrapper` / `build.cache_dir` are the config plumbing the shared
        /// build-environment resolver reads. An omitted `build:` section (the common case)
        /// resolves to the defaults - empty wrapper, empty cache_dir - so the workflow's ONE
        /// resolver ([`crate::gate::BuildEnv::resolve`]) treats it exactly like an explicit
        /// `wrapper: off`.
        build_config_parses_wrapper_and_cache_dir_and_defaults_when_omitted: {
            let wrapper_and_cache =
                |w: &Workflow| (w.build.wrapper.clone(), w.build.cache_dir.clone());
            assert_parsed(
                "{}",
                wrapper_and_cache,
                (String::new(), String::new()),
                "an omitted build: section defaults empty",
            );
            assert_parsed(
                "build:\n  wrapper: sccache\n  cache_dir: /shared/cache\n",
                wrapper_and_cache,
                ("sccache".to_string(), "/shared/cache".to_string()),
                "an explicit wrapper and cache_dir parse through",
            );
        };
        /// Spec 65 unit 4 (JOBS CAP): `build.jobs` is the config plumbing
        /// [`crate::gate::BuildEnv::resolve`]'s jobs facet reads. Omitted (the common case)
        /// parses to `0` - unset, matching this config's own zero-as-unset convention
        /// (`budget`, `max_retries`, `speculation_width`) - so a pre-existing workflow with no
        /// opinion on `build.jobs` is byte-for-byte back-compatible. An explicit positive value
        /// parses through untouched, and jobs is independent of wrapper - both may be set
        /// together without either suppressing the other's parse.
        build_config_parses_jobs_and_defaults_to_zero_when_omitted: {
            assert_parsed(
                "{}",
                |w| w.build.jobs,
                0,
                "an omitted build: section defaults jobs to 0 (unset)",
            );
            assert_parsed("build:\n  jobs: 4\n", |w| w.build.jobs, 4, "explicit jobs");
            assert_parsed(
                "build:\n  wrapper: sccache\n  jobs: 8\n",
                |w| (w.build.wrapper.clone(), w.build.jobs),
                ("sccache".to_string(), 8),
                "jobs and wrapper parse together",
            );
        };
        /// Spec 65: `build.max_concurrent` is the machine-wide build budget's config plumbing
        /// ([`crate::budget::BuildBudget`] is the resolver that reads it). Omitted resolves to
        /// the documented default of 4 - NOT to a bare `u32::default()` of 0, which the `0`
        /// value is separately reserved to mean (the `budget: 0` convention `defaults.budget`
        /// already uses): an explicit `max_concurrent: 0` must parse distinctly from an absent
        /// key.
        build_config_parses_max_concurrent_defaulting_to_four_when_omitted: {
            assert_parsed(
                "{}",
                |w| w.build.max_concurrent,
                4,
                "an omitted build: section defaults max_concurrent to 4",
            );
            assert_parsed(
                "build:\n  max_concurrent: 9\n",
                |w| w.build.max_concurrent,
                9,
                "explicit max_concurrent",
            );
            assert_parsed(
                "build:\n  max_concurrent: 0\n",
                |w| w.build.max_concurrent,
                0,
                "an EXPLICIT 0 must parse as 0 (unlimited), distinct from the omitted default",
            );
        };
        /// Spec 91: `build.mutation` is RETIRED - an omitted `build:` section (the common case,
        /// and every workflow committed before either the retired spec-73 key or this key
        /// existed) still resolves the field empty, so `Config::validate` never rejects a
        /// pre-existing project that never touched this key.
        build_config_parses_mutation_and_defaults_to_empty_when_omitted: {
            assert_parsed(
                "{}",
                |w| w.build.mutation.clone(),
                String::new(),
                "an omitted build: section defaults mutation empty",
            );
            assert_parsed(
                "build:\n  mutation: on\n",
                |w| w.build.mutation.clone(),
                "on".to_string(),
                "the field still parses (so validate can name it in its rejection), it is just \
                 no longer an accepted value",
            );
        };
    }

    #[test]
    fn sdet_author_is_a_distinct_write_capable_agent_separate_from_the_read_only_sdet_lens() {
        // Spec 32 c1: a NEW write-capable SDET-AUTHOR role exists as the shipped
        // `.rigger/agents/sdet-author.md` with Edit/Write tools + `isolation: worktree`,
        // DISTINCT from the read-only `sdet` review lens (which has neither Edit nor Write).
        // Proven over the REAL committed files (read from the crate manifest dir), so this
        // asserts the artifact the loop actually spawns, not a fixture.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let author = parse_agent(
            &std::fs::read(root.join(".rigger/agents/sdet-author.md"))
                .expect("the shipped .rigger/agents/sdet-author.md must exist"),
        )
        .expect("sdet-author.md parses as an agent definition");
        assert_eq!(author.id, "sdet-author");

        // Write-capable: it grants BOTH Edit and Write (case-insensitive), and neither is
        // stripped by fan-out filtering (they are not spawn tools), so an isolated spawn of
        // this agent keeps its authoring capability.
        let grants =
            |a: &AgentDef, tool: &str| a.tools.iter().any(|t| t.eq_ignore_ascii_case(tool));
        assert!(grants(&author, "Edit"), "sdet-author must grant Edit");
        assert!(grants(&author, "Write"), "sdet-author must grant Write");
        let allowed = author.allowed_tools();
        assert!(
            allowed.iter().any(|t| t.eq_ignore_ascii_case("Edit"))
                && allowed.iter().any(|t| t.eq_ignore_ascii_case("Write")),
            "Edit/Write survive fan-out stripping - the spawned author can actually write"
        );

        // `isolation: worktree`: it authors its periphery tests IN an isolated worktree.
        assert_eq!(author.isolation, "worktree");
        assert!(author.isolated());

        // DISTINCT from the read-only `sdet` review lens: a different agent id, and the lens
        // has NEITHER Edit nor Write - it only reviews, it cannot author. This is the
        // independence the spec demands: authorship and review are separate roles.
        let lens = parse_agent(
            &std::fs::read(root.join(".rigger/agents/sdet.md"))
                .expect("the shipped .rigger/agents/sdet.md must exist"),
        )
        .expect("sdet.md parses as an agent definition");
        assert_ne!(
            author.id, lens.id,
            "the author and the review lens are distinct agents"
        );
        assert!(
            !grants(&lens, "Edit") && !grants(&lens, "Write"),
            "the sdet review lens stays read-only (no Edit/Write) - it reviews, never authors"
        );

        // The role token exists in src/spawn.rs and names this agent's role.
        assert_eq!(crate::spawn::ROLE_SDET_AUTHOR, "sdet-author");
    }

    /// Loading `yaml` as a workflow fails with an error naming `dotted: unknown key`; `why` is
    /// the assertion's own reason.
    fn assert_load_workflow_names_the_unknown_key(yaml: &str, dotted: &str, why: &str) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("workflow.yml");
        std::fs::write(&path, yaml).expect("write workflow.yml");

        let msg = load_workflow(&path).unwrap_err().to_string();
        assert!(
            msg.contains(&format!("{dotted}: unknown key")),
            "{why}: {msg}"
        );
    }

    crate::test_cases! {
        /// Spec 102, criterion 3 (AN UNKNOWN KEY IS NAMED), the spec's own worked example:
        /// `defaults.max_parallel_unitz` (a typo of a real-world field this repo's own
        /// `.rigger/workflow.yml` once carried silently dead) fails naming that dotted path -
        /// this criterion OWNS unknown-key rejection at every config level, proven here at the
        /// `load_workflow` level [`load`] itself calls, and at the CLI level (`rigger validate`)
        /// in `tests/config_unknown_key_dotted_path_periphery.rs` - both read through this same
        /// function, so their text is identical by construction, not by convention.
        load_workflow_rejects_an_unknown_key_under_defaults_naming_its_dotted_path: assert_load_workflow_names_the_unknown_key(
            "defaults:\n  autonomy: auto_notify\n  max_parallel_unitz: 2\n",
            "defaults.max_parallel_unitz",
            "must name the dotted path of the unrecognized key",
        );
        /// The same rejection reached through a nested map key (`stages.<name>.<field>`), proving
        /// the dotted path composes correctly past a `BTreeMap` level too, not just a plain
        /// struct-in-struct nesting.
        load_workflow_rejects_an_unknown_key_under_a_named_stage_naming_its_dotted_path: assert_load_workflow_names_the_unknown_key(
            "stages:\n  implement:\n    agent: rust-engineer\n    gatez: [build]\n",
            "stages.implement.gatez",
            "must name the dotted path through the stage's own map key",
        );
    }

    #[test]
    fn resolve_wall_clocks_folds_the_default_only_onto_unset_agents() {
        let mut agents = BTreeMap::new();
        agents.insert(
            "unset".to_string(),
            AgentDef {
                id: "unset".into(),
                max_wall_clock: None,
                ..Default::default()
            },
        );
        agents.insert(
            "override".to_string(),
            AgentDef {
                id: "override".into(),
                max_wall_clock: Some(60),
                ..Default::default()
            },
        );
        let defaults = Defaults {
            max_wall_clock: 900,
            ..Default::default()
        };
        resolve_wall_clocks(&mut agents, &defaults);
        assert_eq!(
            agents["unset"].max_wall_clock,
            Some(900),
            "an unset agent inherits the workflow default"
        );
        assert_eq!(
            agents["override"].max_wall_clock,
            Some(60),
            "an agent's own per-role value overrides the default"
        );
    }

    #[test]
    fn resolve_wall_clocks_leaves_agents_unbounded_under_a_zero_default() {
        let mut agents = BTreeMap::new();
        agents.insert(
            "a".to_string(),
            AgentDef {
                id: "a".into(),
                max_wall_clock: None,
                ..Default::default()
            },
        );
        resolve_wall_clocks(&mut agents, &Defaults::default());
        assert_eq!(
            agents["a"].max_wall_clock, None,
            "a zero (absent) default leaves an unset agent unbounded, back-compatible"
        );
    }

    // ---- spec 19c, unit 3: `rigger validate` warns on an unbounded default ----

    #[test]
    fn unbounded_wall_clock_advisory_fires_when_default_unbounded_and_a_gating_role_is_unbounded() {
        // `config_with_default_adjudicator` leaves `defaults.max_wall_clock` at its `0`
        // (unbounded) default and inserts a gating adjudicator "adj" with no per-agent bound.
        let cfg = config_with_default_adjudicator(RESULT_LINE);
        let msg = unbounded_wall_clock_advisory(&cfg)
            .expect("an unbounded default over an unbounded gating role must warn");
        assert!(
            msg.contains("\"adj\""),
            "names the unbounded gating role: {msg}"
        );
        assert!(
            msg.contains("defaults.max_wall_clock"),
            "names the fix knob: {msg}"
        );
        assert!(
            msg.to_lowercase().contains("swept"),
            "explains a hung gating agent is never swept: {msg}"
        );
    }

    #[test]
    fn unbounded_wall_clock_advisory_is_silent_when_the_default_is_bounded() {
        let mut cfg = config_with_default_adjudicator(RESULT_LINE);
        cfg.workflow.defaults.max_wall_clock = 600;
        assert!(
            unbounded_wall_clock_advisory(&cfg).is_none(),
            "a bounded default sweeps every inheriting gating role, so no warning"
        );
    }

    #[test]
    fn unbounded_wall_clock_advisory_is_silent_when_every_gating_role_sets_its_own_bound() {
        let mut cfg = config_with_default_adjudicator(RESULT_LINE);
        // Default stays unbounded, but the gating role carries its own per-agent bound.
        cfg.agents.get_mut("adj").unwrap().max_wall_clock = Some(300);
        assert!(
            unbounded_wall_clock_advisory(&cfg).is_none(),
            "a per-agent bound on every gating role covers the risk even under a 0 default"
        );
    }

    #[test]
    fn unbounded_wall_clock_advisory_names_only_gating_roles_not_lenses() {
        // A lens/adversary that hangs is not a GATING role; the advisory targets only the
        // verdict-bearing roles the gate awaits, so an unbounded lens is not named.
        let mut cfg = config_with_default_adjudicator(RESULT_LINE);
        cfg.agents
            .insert("lens".into(), agent_with_prompt("lens", RESULT_LINE));
        cfg.workflow.defaults.review.lenses = vec!["lens".into()];
        let msg = unbounded_wall_clock_advisory(&cfg)
            .expect("the unbounded gating adjudicator still warns");
        assert!(msg.contains("\"adj\""), "names the gating role: {msg}");
        assert!(
            !msg.contains("\"lens\""),
            "an unbounded lens is not a gating role and must not be named: {msg}"
        );
    }

    #[test]
    fn recurse_false_strips_fan_out_tools() {
        let a = AgentDef {
            id: "impl".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: false,
            ..Default::default()
        };
        assert_eq!(a.allowed_tools(), ["Read"]);
    }

    #[test]
    fn recurse_true_keeps_fan_out_tools() {
        let a = AgentDef {
            id: "lead".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: true,
            ..Default::default()
        };
        assert_eq!(a.allowed_tools(), ["Read", "Agent"]);
    }

    #[test]
    fn isolation_none_opts_out_of_worktrees() {
        let none = AgentDef {
            id: "rev".into(),
            isolation: "none".into(),
            ..Default::default()
        };
        assert!(!none.isolated());
        let wt = AgentDef {
            id: "impl".into(),
            isolation: "worktree".into(),
            ..Default::default()
        };
        assert!(wt.isolated());
        let unset = AgentDef {
            id: "bare".into(),
            ..Default::default()
        };
        assert!(unset.isolated());
    }

    /// A config declaring agent `a` and each `(name, agent, needs)` stage of `stages` must fail
    /// validation.
    fn assert_stages_invalid(stages: &[(&str, &str, &[&str])]) {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        for (name, agent, needs) in stages {
            cfg.workflow.stages.insert(
                (*name).into(),
                Stage {
                    name: (*name).into(),
                    agent: (*agent).into(),
                    needs: needs.iter().map(|n| (*n).into()).collect(),
                    ..Default::default()
                },
            );
        }
        assert!(cfg.validate().is_err());
    }

    crate::test_cases! {
        validate_catches_unknown_ref: assert_stages_invalid(&[("s", "ghost", &[])]);
        validate_catches_cycle: assert_stages_invalid(&[("x", "a", &["y"]), ("y", "a", &["x"])]);
    }

    #[test]
    fn validate_catches_unknown_default_review_agent() {
        // The default review panel's agent ids are validated like every other agent
        // reference: an unknown lens/adversary/adjudicator fails validation.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.defaults.review = ReviewPanel {
            lenses: vec!["ghost".into()],
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "an unknown agent in defaults.review must fail validation"
        );
    }

    #[test]
    fn default_review_panel_parses_and_validates() {
        // A workflow declaring `defaults.review` parses the three-tier panel and
        // validates it referentially: known lenses + adversary + adjudicator load.
        let yaml = "defaults:\n  \
review:\n    \
lenses: [archlens, techlens]\n    \
adversary: adv\n    \
adjudicator: adj\n\
stages:\n  \
implement:\n    \
agent: worker\n";
        let mut wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        for name in wf.stages.keys().cloned().collect::<Vec<_>>() {
            if let Some(st) = wf.stages.get_mut(&name) {
                st.name = name;
            }
        }
        let review = &wf.defaults.review;
        assert_eq!(review.lenses, ["archlens", "techlens"]);
        assert_eq!(review.adversary, "adv");
        assert_eq!(review.adjudicator, "adj");
        assert_eq!(
            review.agent_ids(),
            ["archlens", "techlens", "adv", "adj"],
            "the panel reports every agent it references for validation"
        );

        // With those agents present, the config validates.
        let mut cfg = Config {
            workflow: wf,
            ..Default::default()
        };
        for id in ["archlens", "techlens", "adv", "adj", "worker"] {
            cfg.agents.insert(id.into(), agent_def(id));
        }
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn review_tiers_depth_policy_parses_from_yaml() {
        // spec 03 / spec 13 unit 4: `defaults.review` carries an OPT-IN `tiers` depth
        // policy - a light panel, a blast-radius threshold, and a high-risk path list -
        // that parse from the workflow YAML alongside the full panel.
        let yaml = "defaults:\n  \
review:\n    \
lenses: [archlens, techlens]\n    \
adversary: adv\n    \
adjudicator: adj\n    \
tiers:\n      \
threshold: 3\n      \
high_risk_paths: [\"src/conductor.rs\", \"specs/**\"]\n      \
light:\n        \
lenses: [archlens]\n        \
adjudicator: adj\n\
stages:\n  \
implement:\n    \
agent: worker\n";
        let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        let review = &wf.defaults.review;
        let depth = review.depth().expect("the tiers depth policy must parse");
        assert_eq!(depth.threshold, 3);
        assert_eq!(depth.high_risk_paths, ["src/conductor.rs", "specs/**"]);
        assert_eq!(depth.light.lenses, ["archlens"]);
        assert_eq!(depth.light.adjudicator, "adj");
        assert!(
            depth.light.adversary.is_empty(),
            "the light panel typically omits the adversary"
        );
    }

    /// A review panel of `lenses` gated by `adjudicator`.
    fn panel(lenses: &[&str], adjudicator: &str) -> ReviewPanel {
        ReviewPanel {
            lenses: lenses.iter().map(|l| (*l).into()).collect(),
            adjudicator: adjudicator.into(),
            ..Default::default()
        }
    }

    /// A config declaring agent `a` whose default review is `full` with a tiers policy routing
    /// risk below 2 to the `light` panel must fail validation (`why`).
    fn assert_tiered_review_invalid(full: ReviewPanel, light: ReviewPanel, why: &str) {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.defaults.review = ReviewPanel {
            tiers: Some(Box::new(ReviewDepth {
                light,
                threshold: 2,
                ..Default::default()
            })),
            ..full
        };
        assert!(cfg.validate().is_err(), "{why}");
    }

    crate::test_cases! {
        /// The light panel's agent ids are validated exactly like the full panel's: an unknown
        /// light-panel lens/adversary/adjudicator fails `config::load` (spec 03).
        validate_catches_an_unknown_light_panel_agent: assert_tiered_review_invalid(
            panel(&["a"], "a"),
            panel(&["ghost"], "a"),
            "an unknown light-panel lens must fail validation",
        );
        /// The adjudicator's gating verdict is mandatory on every tier - only the adversary
        /// flexes (spec 03 / spec 13 unit 4). A configured light panel that names no
        /// adjudicator would let a low-risk unit approve trivially, so it fails `config::load`
        /// loudly.
        validate_rejects_a_light_panel_with_no_adjudicator: assert_tiered_review_invalid(
            panel(&["a"], "a"),
            panel(&["a"], ""),
            "a light panel with no adjudicator must fail validation",
        );
        /// The gating verdict is mandatory on EVERY tier, including the FULL one a high-risk
        /// unit routes to (remediation of sdet-u13-empty-full-tiers-skips-adjudicator). A tiers
        /// policy whose ENCLOSING full panel names no adjudicator (here, no roster at all) -
        /// even with a perfectly valid light tier - would let a high-risk unit route to a panel
        /// that approves trivially via `is_empty()`, skipping the adjudicator. So it must fail
        /// `config::load` loudly. Before the fix, `validate_depth` guarded only the light tier,
        /// so `{empty full roster + valid tiers.light}` was ACCEPTED.
        validate_rejects_a_tiers_policy_on_a_full_panel_with_no_adjudicator:
            assert_tiered_review_invalid(
                panel(&[], ""),
                panel(&["a"], "a"),
                "a tiers policy on a full panel that names no adjudicator must fail validation",
            );
    }

    #[test]
    fn validate_accepts_a_well_formed_depth_policy() {
        // A depth policy whose light panel names a known adjudicator and only known
        // agents validates cleanly.
        let mut cfg = Config::default();
        for id in ["arch", "tech", "adv", "judge"] {
            cfg.agents.insert(id.into(), agent_def(id));
        }
        cfg.workflow.defaults.review = ReviewPanel {
            lenses: vec!["arch".into(), "tech".into()],
            adversary: "adv".into(),
            adjudicator: "judge".into(),
            tiers: Some(Box::new(ReviewDepth {
                light: ReviewPanel {
                    lenses: vec!["arch".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                threshold: 3,
                high_risk_paths: vec!["src/conductor.rs".into()],
            })),
        };
        assert!(
            cfg.validate().is_ok(),
            "a well-formed depth policy must validate"
        );
    }

    #[test]
    fn validate_rejects_a_stage_review_declaring_only_a_tiers_policy() {
        // A per-stage `review:` that declares ONLY a tiers policy (no roster, so no
        // adjudicator on the full panel) is malformed the same way: the per-stage
        // `validate_depth` branch (remediation of sdet-u13-stage-override-tiers-untested,
        // part a) now rejects it loudly instead of the runtime silently discarding it back
        // to defaults. This pins the stage-level validation branch that was untested.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "a".into(),
                review: ReviewPanel {
                    // Only a tiers policy, no enclosing roster/adjudicator.
                    tiers: Some(Box::new(ReviewDepth {
                        light: ReviewPanel {
                            lenses: vec!["a".into()],
                            adjudicator: "a".into(),
                            ..Default::default()
                        },
                        threshold: 2,
                        ..Default::default()
                    })),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(
            cfg.validate().is_err(),
            "a stage review declaring only a tiers policy (no full adjudicator) must fail validation"
        );
    }

    /// Spec 83 criterion 2, round 2 (the reject's own required fix): [`read_scratch_defaults`]
    /// must read BOTH `defaults.workdir` and `defaults.max_retries` from a project whose
    /// `.rigger/agents/` fleet is entirely absent - the exact shape [`load`] refuses outright
    /// (`load_agents` fails before `Config::validate` ever runs). A caller reading just these
    /// two lightweight fields must never inherit that unrelated failure.
    #[test]
    fn read_scratch_defaults_reads_workdir_and_max_retries_without_an_agents_fleet() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let rigger_dir = tmp.path().join(".rigger");
        std::fs::create_dir_all(&rigger_dir).expect("create .rigger dir");
        std::fs::write(
            rigger_dir.join("workflow.yml"),
            "defaults:\n  workdir: \"/configured/scratch\"\n  max_retries: 5\n",
        )
        .expect("write workflow.yml");
        // No `.rigger/agents/` dir at all - fixture guard confirming this test genuinely
        // exercises the validate-independent axis, not just the field-plumbing.
        assert!(
            load(tmp.path().to_str().unwrap()).is_err(),
            "fixture bug: config::load must fail on an agents-less project for this test to \
             discriminate the lightweight resolver from the full one"
        );

        let d = read_scratch_defaults(&rigger_dir)
            .expect("read_scratch_defaults must succeed with no agents fleet present");
        assert_eq!(
            d.workdir, "/configured/scratch",
            "must read the configured workdir via the lightweight resolver"
        );
        assert_eq!(
            d.max_retries, 5,
            "must read the configured max_retries via the lightweight resolver"
        );
    }

    /// Mirrors [`read_store_config`]'s / [`read_scratch_workdir`]'s own tolerant-absent
    /// contract: a project with no `workflow.yml` at all pins nothing, so every field resolves
    /// to its ordinary default rather than an error.
    #[test]
    fn read_scratch_defaults_resolves_to_the_default_when_workflow_yml_is_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let rigger_dir = tmp.path().join(".rigger");
        std::fs::create_dir_all(&rigger_dir).expect("create .rigger dir");

        let d = read_scratch_defaults(&rigger_dir)
            .expect("an absent workflow.yml must resolve to the default, not an error");
        assert_eq!(
            d.workdir, "",
            "absent config resolves to no configured workdir"
        );
        assert_eq!(
            d.max_retries, 0,
            "absent config resolves to no configured max_retries"
        );
    }

    /// [`read_scratch_workdir`] must keep delegating to [`read_scratch_defaults`] (the shared
    /// parse), not a second independently-maintained copy - a regression guard for the two
    /// already-established call sites (`rigger reset --build-cache`'s two flavors) that read
    /// `read_scratch_workdir` directly and must see byte-identical behavior after this refactor.
    #[test]
    fn read_scratch_workdir_still_matches_read_scratch_defaults_workdir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let rigger_dir = tmp.path().join(".rigger");
        std::fs::create_dir_all(&rigger_dir).expect("create .rigger dir");
        std::fs::write(
            rigger_dir.join("workflow.yml"),
            "defaults:\n  workdir: \"/some/other/scratch\"\n",
        )
        .expect("write workflow.yml");

        assert_eq!(
            read_scratch_workdir(&rigger_dir).unwrap(),
            read_scratch_defaults(&rigger_dir).unwrap().workdir,
            "read_scratch_workdir must stay byte-identical to read_scratch_defaults().workdir"
        );
    }

    #[test]
    fn demo_example_loads() {
        // The worked example the architecture references (§10, §11) must load and
        // validate into a real DAG, so it never rots. The path is relative to the
        // crate root (cargo runs tests there).
        let cfg = load("../../examples/demo").expect("the demo example must load and validate");
        // The full agent roster still loads from the dir.
        assert_eq!(cfg.agents.len(), 7, "demo agent count");
        // Per-unit model: plan -> implement (each unit implements, three-tier-reviews
        // ITSELF, and integrates in one lifecycle). The separate review/integrate
        // stages are folded out.
        assert_eq!(cfg.workflow.stages.len(), 2, "demo stage count");
        assert_eq!(cfg.workflow.gates.len(), 4, "demo gate count");
        // The shape: a producer, then a worktree-isolated non-recursive implementer
        // stage that runs the full per-unit lifecycle and integrates on_pass: merge.
        assert_eq!(cfg.workflow.stages["plan"].produces, "dag");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        assert_eq!(implement.on_pass, "merge");
        let impl_agent = &cfg.agents[&implement.agent];
        assert!(impl_agent.isolated(), "the implementer runs in a worktree");
        assert!(
            !impl_agent.recurse,
            "the implementer must not be able to fan out"
        );
        // The three-tier review panel is declared once on defaults.review and applied
        // to every implementer unit.
        let review = &cfg.workflow.defaults.review;
        assert_eq!(review.lenses.len(), 3, "tier 1: three expert lenses");
        assert_eq!(
            review.adversary, "adversary",
            "tier 2: the adversary refutes the lenses"
        );
        assert_eq!(
            review.adjudicator, "adjudicator",
            "tier 3: the neutral adjudicator's verdict gates"
        );
    }

    #[test]
    fn failure_rules_parse_into_an_ordered_taxonomy() {
        // An authored `defaults.failure_rules` block parses into a first-match-wins
        // taxonomy: the matcher fields, the class, the per-rule limit, and the backoff
        // all convert to the runtime form (spec 10, unit 2).
        let yaml = "defaults:\n  \
failure_rules:\n    \
- match: {output_regex: \"segfault|SIGSEGV\"}\n      \
class: flaky\n      \
limit: 3\n      \
backoff: {duration_ms: 500, factor: 2.0, max_ms: 8000}\n    \
- match: {exit_status: 137}\n      \
class: infra\n      \
limit: 1\n    \
- match: {}\n      \
class: product\n";
        let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(wf.defaults.failure_rules.len(), 3);
        let tax = wf.failure_taxonomy().expect("authored rules must convert");
        assert_eq!(tax.rules().len(), 3);
        // First-match-wins: the segfault signal hits the flaky rule with its limit and
        // a non-zero backoff, not the product catch-all.
        let flaky = tax
            .classify(&failure::Signal::from_output("thread panicked: SIGSEGV"))
            .unwrap();
        assert_eq!(flaky.class, failure::FailureClass::Flaky);
        assert_eq!(flaky.limit, 3);
        assert_eq!(flaky.backoff.duration, Duration::from_millis(500));
        // The exit-status rule matches a killed worker (137) as infra.
        let infra = tax
            .classify(&failure::Signal {
                exit_status: Some(137),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(infra.class, failure::FailureClass::Infra);
        // Anything else falls through to the product catch-all.
        assert_eq!(
            tax.classify(&failure::Signal::from_output("assertion failed"))
                .unwrap()
                .class,
            failure::FailureClass::Product
        );
    }

    #[test]
    fn absent_failure_rules_fall_back_to_the_spec07_preserving_defaults() {
        // The common case: a workflow authors no failure_rules, so the taxonomy is the
        // shipped default (infra faults reran, everything else product) - preserving
        // spec-07 semantics and every existing gate test.
        let wf: Workflow = serde_yaml::from_str("defaults:\n  budget: 10\n").unwrap();
        assert!(wf.defaults.failure_rules.is_empty());
        let tax = wf.failure_taxonomy().unwrap();
        assert!(!tax.is_empty(), "the default taxonomy ships rules");
        assert_eq!(
            tax.classify(&failure::Signal::from_output("FAIL\nassertion failed"))
                .unwrap()
                .class,
            failure::FailureClass::Product,
            "a plain gate failure classifies product by default (no rerun, today's behavior)"
        );
    }

    #[test]
    fn validate_rejects_an_unknown_failure_class_and_a_bad_regex() {
        let base = |rules: &str| -> Config {
            let wf: Workflow =
                serde_yaml::from_str(&format!("defaults:\n  failure_rules:\n{rules}")).unwrap();
            Config {
                workflow: wf,
                ..Default::default()
            }
        };
        // An unknown class is rejected at validation.
        assert!(base("    - match: {}\n      class: bogus\n")
            .validate()
            .is_err());
        // An uncompilable regex is rejected at validation.
        assert!(
            base("    - match: {output_regex: \"(\"}\n      class: flaky\n")
                .validate()
                .is_err()
        );
        // A well-formed rule validates.
        assert!(base(
            "    - match: {output_regex: \"flake\"}\n      class: flaky\n      limit: 2\n"
        )
        .validate()
        .is_ok());
    }

    /// Spec 50, criterion 4 (opt-out): the always-on dash auto-ensure is ON unless a workflow
    /// explicitly opts out. An omitted `dash` key keeps it ON (the always-on promise for a
    /// workflow that says nothing); the documented `dash: off` - and its `false`/`no` synonyms,
    /// case- and whitespace-insensitive - suppress it; `on`/`true`/empty keep it ON. This is the
    /// single config-side resolution the step-path opt-out reads, so it is proven here directly.
    #[test]
    fn dash_enabled_is_on_by_default_and_off_only_on_an_explicit_opt_out() {
        // An omitted `dash` key keeps the always-on dash (a workflow that says nothing gets one).
        let wf: Workflow = serde_yaml::from_str("{}").unwrap();
        assert!(
            wf.dash_enabled(),
            "an omitted `dash` key leaves the always-on dash ON"
        );
        // The documented bare `dash: off` opts out - the exact syntax the spec names.
        let off: Workflow = serde_yaml::from_str("dash: off\n").unwrap();
        assert!(
            !off.dash_enabled(),
            "the documented `dash: off` suppresses the ensure"
        );
        // Case- and whitespace-insensitive, and the `false`/`no` synonyms also opt out.
        for v in ["OFF", " off ", "Off", "false", "no"] {
            let w: Workflow = serde_yaml::from_str(&format!("dash: \"{v}\"\n")).unwrap();
            assert!(!w.dash_enabled(), "`dash: {v:?}` must opt out");
        }
        // `on` / `true` / an empty value keep the always-on dash ON.
        for v in ["on", "true", ""] {
            let w: Workflow = serde_yaml::from_str(&format!("dash: \"{v}\"\n")).unwrap();
            assert!(w.dash_enabled(), "`dash: {v:?}` keeps the dash ON");
        }
    }

    /// Spec 65 unit 2 (NO SILENT DEGRADE): a CONFIGURED (non-auto, non-off) `build.wrapper`
    /// absent from PATH must fail `Config::validate` - a run-start loud error naming both
    /// the missing binary and the `build.wrapper` config key - rather than silently letting
    /// the run proceed with a cache that will never actually engage. Uses a definitely-fake
    /// binary name against the REAL ambient PATH (a read, never a mutation, so this needs no
    /// env-race guard - see `gate::resolve_wrapper_name_from`'s own tests for the
    /// synthetic-PATH coverage of every branch).
    #[test]
    fn validate_rejects_a_named_build_wrapper_absent_from_path() {
        let mut cfg = Config::default();
        cfg.workflow.build.wrapper = "definitely-not-a-real-wrapper-rigger-u2-test".into();
        let err = cfg
            .validate()
            .expect_err("a named-but-absent build.wrapper must fail validation");
        let msg = err.to_string();
        assert!(
            msg.contains("definitely-not-a-real-wrapper-rigger-u2-test"),
            "the error must name the missing binary: {msg:?}"
        );
        assert!(
            msg.contains("build.wrapper"),
            "the error must name the config key: {msg:?}"
        );
    }

    /// `auto` and `off` never fail validation regardless of PATH content: `auto` finding
    /// nothing is a discovered-implicit DEGRADE (inject nothing), never a configured-explicit
    /// failure, and `off`/empty never even probes PATH. Only a NAMED wrapper's absence
    /// errors (`validate_rejects_a_named_build_wrapper_absent_from_path` above).
    #[test]
    fn validate_accepts_auto_and_off_wrapper_regardless_of_path() {
        for wrapper in ["auto", "off", "", "  "] {
            let mut cfg = Config::default();
            cfg.workflow.build.wrapper = wrapper.into();
            assert!(
                cfg.validate().is_ok(),
                "build.wrapper: {wrapper:?} must never fail validation"
            );
        }
    }

    /// Spec 91 (SCHEMA RETIREMENT): ANY explicit `build.mutation` value - not just the
    /// retired switch's old `on` - is a run-start config error naming this spec, since the
    /// key no longer does anything at all. Absent (empty) is unaffected - proven separately
    /// by `validate_accepts_an_absent_build_mutation`.
    #[test]
    fn validate_rejects_any_explicit_build_mutation_value_naming_spec_91() {
        for mutation in ["on", "off", "ON", "  off  ", "nonsense"] {
            let mut cfg = Config::default();
            cfg.workflow.build.mutation = mutation.into();
            let err = cfg.validate().expect_err(&format!(
                "build.mutation: {mutation:?} must fail validation"
            ));
            assert_eq!(
                err.to_string(),
                "config: the workflow key `build.mutation` is retired (spec 91 retired it: \
                 nothing reads it): delete the `mutation:` line under `build:` from your workflow.yml (config \
                 key: build.mutation)"
            );
        }
    }

    /// The top-level workflow `name` key is RETIRED: nothing ever read it. A workflow.yml that
    /// still carries a `name:` line parses, and validation refuses it naming the key and the
    /// line to delete - never a bare unknown-key error that leaves the operator guessing.
    #[test]
    fn validate_rejects_the_retired_workflow_name_key_with_the_line_to_delete() {
        let workflow: Workflow = crate::config::parse_yaml_naming_unknown_keys(
            "name: my-project\ndefaults:\n  budget: 10\n",
        )
        .expect("a workflow carrying the retired name key still parses, so validate can name it");
        let cfg = Config {
            workflow,
            ..Default::default()
        };
        let msg = cfg
            .validate()
            .expect_err("the retired name key must fail validation")
            .to_string();
        assert!(
            msg.contains("`name` is retired") && msg.contains("delete the `name:` line"),
            "the error must name the retired key and the line to delete: {msg:?}"
        );
    }

    /// The un-set default (every pre-existing workflow.yml) must never fail validation over
    /// a key it never touched.
    #[test]
    fn validate_accepts_an_absent_build_mutation() {
        let cfg = Config::default();
        assert_eq!(cfg.workflow.build.mutation, "");
        assert!(
            cfg.validate().is_ok(),
            "an absent build.mutation must never fail validation"
        );
    }

    /// A synthetic workflow declaring the one gate `sweep`, requiring `requires`.
    fn config_with_sweep_requiring(requires: &[&str]) -> Config {
        let mut cfg = Config::default();
        cfg.workflow.gates.insert(
            "sweep".into(),
            Gate {
                run: "true".into(),
                requires: requires.iter().map(|r| (*r).to_string()).collect(),
                ..Default::default()
            },
        );
        cfg
    }

    /// Spec 113 (A GATE DECLARES WHAT IT REQUIRES): a declared gate whose requirement is not
    /// an executable on `PATH` refuses validation with the resolver's one message, naming the
    /// FIRST missing entry in list order - never a later one. `sh` stands for a present entry
    /// (this crate already assumes a Unix `PATH`, as the named-wrapper tests below do).
    #[test]
    fn validate_refuses_the_first_missing_gate_requirement() {
        let cfg = config_with_sweep_requiring(&[
            "sh",
            "rigger-absent-requirement-one",
            "rigger-absent-requirement-two",
        ]);
        let msg = cfg
            .validate()
            .expect_err("a missing gate requirement refuses validation")
            .to_string();
        assert_eq!(
            msg,
            "config: gate \"sweep\" requires \"rigger-absent-requirement-one\", which is not \
             an executable on PATH (config key: gates.sweep.requires)"
        );
    }

    /// A declared gate that requires nothing validates, and validation answers the load's own
    /// resolution: one entry per declared gate, here `sweep` with nothing resolved.
    #[test]
    fn validate_accepts_a_gate_that_requires_nothing() {
        let cfg = config_with_sweep_requiring(&[]);
        assert_eq!(
            cfg.validate().expect("a gate requiring nothing validates"),
            vec![crate::gate::GateRequirements {
                gate: "sweep".into(),
                requires: vec![],
            }]
        );
    }

    /// The requirement check runs LAST, after `Workflow::failure_taxonomy` (the check before
    /// it), so a workflow failing both reports the earlier check's message.
    #[test]
    fn validate_reports_an_earlier_check_before_a_missing_gate_requirement() {
        let mut cfg = config_with_sweep_requiring(&["rigger-absent-requirement-one"]);
        cfg.workflow
            .defaults
            .failure_rules
            .push(crate::config::FailureRuleDef {
                class: "not-a-class".into(),
                ..Default::default()
            });
        assert_eq!(
            cfg.validate()
                .expect_err("an unknown failure class refuses validation")
                .to_string(),
            "config: failure rule has unknown class \"not-a-class\" (want infra | product | \
             flaky)",
            "the failure taxonomy's refusal must win over the requirement check"
        );
    }

    /// Spec 65 unit 2 (NO SILENT DEGRADE) - the SAME Design sentence (specs/65:26-28) that
    /// decides a named-but-absent wrapper BINARY must fail `Config::validate` also decides a
    /// named wrapper's UNCREATABLE cache dir must fail it too, naming both the dir and the
    /// `build.cache_dir` config key - mirroring `validate_rejects_a_named_build_wrapper_
    /// absent_from_path` above for the cache-dir axis. `wrapper: "true"` names a binary
    /// virtually guaranteed present on any real Unix PATH (this crate already assumes a Unix
    /// PATH elsewhere - `gate::is_executable_file` is `#[cfg(unix)]`-gated), so this
    /// deterministically reaches the cache-dir axis rather than failing on the binary axis
    /// first. The uncreatable dir is a real FILE blocking a path component - deterministic on
    /// every OS/user, unlike a permission-bit trick a root-run test could bypass.
    #[test]
    fn validate_rejects_a_named_wrapper_with_an_uncreatable_cache_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, "not a directory").expect("write blocker file");
        let cache_dir = blocker.join("nested").join("cache");

        let mut cfg = Config::default();
        cfg.workflow.build.wrapper = "true".into();
        cfg.workflow.build.cache_dir = cache_dir.to_string_lossy().into_owned();
        let err = cfg
            .validate()
            .expect_err("a named wrapper's uncreatable cache dir must fail validation");
        let msg = err.to_string();
        assert!(
            msg.contains(&cache_dir.to_string_lossy().into_owned()),
            "the error must name the cache dir: {msg:?}"
        );
        assert!(
            msg.contains("build.cache_dir"),
            "the error must name the config key: {msg:?}"
        );
    }

    /// The auto-discovery counterpart: `build.wrapper: auto` with an uncreatable cache dir
    /// must never fail validation (a discovered-implicit degrade skips the whole layer,
    /// regardless of whether `auto`'s PATH probe would otherwise have found a known
    /// wrapper), mirroring `validate_accepts_auto_and_off_wrapper_regardless_of_path` above
    /// for the cache-dir axis - only the NAMED-wrapper case
    /// (`validate_rejects_a_named_wrapper_with_an_uncreatable_cache_dir` above) is a
    /// configured-explicit failure.
    #[test]
    fn validate_accepts_auto_wrapper_with_an_uncreatable_cache_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, "not a directory").expect("write blocker file");
        let cache_dir = blocker.join("nested").join("cache");

        let mut cfg = Config::default();
        cfg.workflow.build.wrapper = "auto".into();
        cfg.workflow.build.cache_dir = cache_dir.to_string_lossy().into_owned();
        assert!(
            cfg.validate().is_ok(),
            "auto must never fail validation regardless of cache-dir usability"
        );
    }

    /// Spec 65 unit 2 round 2 (cache-dir checks CREATABLE, not just WRITABLE): the
    /// realistic steady state is a cache dir that ALREADY EXISTS (the shared
    /// `default_cache_dir` every project reuses after the first one creates it) but is not
    /// WRITABLE - `create_dir_all` alone is a no-op success against it, so only an actual
    /// write probe catches this. Mirrors `validate_rejects_a_named_wrapper_with_an_
    /// uncreatable_cache_dir` above, but chmod's an ALREADY-CREATED dir read+execute-only
    /// (0o555) instead of blocking a path component - the only way to make a dir that
    /// EXISTS yet cannot be written into. Unix-only: the mode bits are a POSIX concept.
    #[cfg(unix)]
    #[test]
    fn validate_rejects_a_named_wrapper_with_a_preexisting_unwritable_cache_dir() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let cache_dir = tmp.path().join("preexisting-cache");
        std::fs::create_dir_all(&cache_dir).expect("pre-create cache dir");
        std::fs::set_permissions(&cache_dir, std::fs::Permissions::from_mode(0o555))
            .expect("chmod cache dir read+execute-only");

        let mut cfg = Config::default();
        cfg.workflow.build.wrapper = "true".into();
        cfg.workflow.build.cache_dir = cache_dir.to_string_lossy().into_owned();
        let err = cfg.validate().expect_err(
            "a named wrapper's pre-existing-but-unwritable cache dir must fail validation, \
             not silently report the layer usable",
        );
        let msg = err.to_string();
        assert!(
            msg.contains(&cache_dir.to_string_lossy().into_owned()),
            "the error must name the cache dir: {msg:?}"
        );
        assert!(
            msg.contains("build.cache_dir"),
            "the error must name the config key: {msg:?}"
        );
    }

    /// The auto-discovery counterpart of the test directly above: `build.wrapper: auto`
    /// against a pre-existing-but-unwritable cache dir must degrade silently, never fail
    /// validation - mirroring `validate_accepts_auto_wrapper_with_an_uncreatable_cache_dir`
    /// for the writability (rather than creatability) failure mode.
    #[cfg(unix)]
    #[test]
    fn validate_accepts_auto_wrapper_with_a_preexisting_unwritable_cache_dir() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let cache_dir = tmp.path().join("preexisting-cache");
        std::fs::create_dir_all(&cache_dir).expect("pre-create cache dir");
        std::fs::set_permissions(&cache_dir, std::fs::Permissions::from_mode(0o555))
            .expect("chmod cache dir read+execute-only");

        let mut cfg = Config::default();
        cfg.workflow.build.wrapper = "auto".into();
        cfg.workflow.build.cache_dir = cache_dir.to_string_lossy().into_owned();
        assert!(
            cfg.validate().is_ok(),
            "auto must never fail validation regardless of cache-dir writability"
        );
    }
}
