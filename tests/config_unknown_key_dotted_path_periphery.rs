//! Periphery (CLI, real-binary) test for spec 102, criterion 3 - AN UNKNOWN KEY IS NAMED.
//!
//! `config_store::load_workflow`'s own unit tests (`src/config_store.rs`) already pin the
//! dotted-path reformatting at the library level. This file proves the criterion's OTHER
//! explicit clause - "`rigger validate` on the same file fails with the same text" - end to
//! end, through the compiled binary, and against the library call `load()`/`validate` itself
//! resolve through: both are driven against the byte-identical fixture file in one test, so
//! "same text" is measured, not assumed from the two call sites sharing code.
//!
//! The criterion OWNS unknown-key rejection "at every config level" - and the shared parser
//! (`dotted_unknown_key`, then `config::parse_yaml_naming_unknown_keys` from commit 9bfb4a9
//! on) is wired at multiple independent call sites, not one: `load_workflow` (proven
//! above), `read_store_config`, and `read_scratch_defaults` from round one; `parse_agent`
//! joins them in round four (see the file's last paragraph below). The second test below
//! closes `read_store_config`'s own wiring, which no other test (unit or periphery) reaches -
//! it is a SEPARATE `.map_err` call over a DIFFERENT struct (`StoreConfig`, not
//! `Workflow`/`Defaults`), driven by `rigger status` rather than `rigger validate` because
//! `read_store_config` is the store-selection probe (§48 rung 4) every store-opening command
//! resolves through BEFORE it ever requires an existing run, so it is observable without first
//! bootstrapping an `events.db`. `read_scratch_defaults`'s own wiring is EXEMPT: it wraps the
//! identical `Defaults` struct through the identical `dotted_unknown_key` call and the
//! identical `"parse workflow: {}"` prefix `load_workflow` already uses (byte-for-byte, per
//! `src/config_store.rs`), so its dotted-path text is already proven by the first test above;
//! its one CLI-propagating caller (`reset --build-cache`, via `read_scratch_workdir`) has no
//! additional branching logic to diverge on, and its other production caller
//! (`scratch_defaults` in `main.rs`) deliberately discards the error via `.unwrap_or_default()`
//! (a pre-existing, documented contract this diff does not change), so no new behavior is
//! observable there at all.
//!
//! The next two tests close a SEPARATE gap: the adjudicator rejected the first round
//! on two adversary-found edge cases inside `dotted_unknown_key` itself (a real error
//! message corrupted when a type mismatch's own value text echoes the marker wording; a
//! field name truncated at an embedded backtick). The implementer's fix is pinned by its
//! own unit tests, but only against synthetic fixture structs local to that test - never
//! through the real `Workflow`/`Defaults` schema or the compiled binary. These two tests
//! reproduce both edge cases through a real `defaults:` field and `rigger validate`,
//! proving the fix holds at the actual CLI boundary the criterion is about.
//!
//! The next two tests close a THIRD gap, from a second remediation round: two more
//! adversary-found edge cases in the same function (a field name that itself embeds the
//! `"`, "` terminator run, previously found by a first-occurrence `.find`; a nested path
//! built from a YAML MAP KEY - a `stages:` name, unlike a plain struct field, arbitrary
//! operator text - that embeds its own `": "`, previously found by a first-occurrence
//! `.find(": ")`). Both fixes are pinned by the implementer's own unit tests against the
//! same kind of synthetic fixture structs as round one's - never through the real
//! `Workflow`/`Defaults`/`stages:` schema or the compiled binary. These two tests close
//! that gap the same way the pair above does.
//!
//! The next two tests prove the criterion itself, not any one round's patch, per spec 102's
//! Design amendment (rigger-run@3df56f8, "THE DOTTED PATH IS TRACKED STRUCTURALLY"): the
//! path must come from a structural tracker (`serde_path_to_error`), never from searching
//! the rendered message, because a key OR a value can echo any delimiter that search
//! anchors on. Round 3's fix (`addc1d0`) was still a string search that DECLINED TO GUESS -
//! passed the raw, un-reformatted message through - on exactly this ambiguity, so this test
//! was EXPECTED TO FAIL there (`op-102-c3-r3-review-note-stale-spec-in-worktree`); the
//! implementer's round-4 rewrite (`config::parse_yaml_naming_unknown_keys`, wrapping
//! `serde_path_to_error`, commit 9bfb4a9) closes it - the tracker's path IS the literal key
//! text, so the same crafted input that defeated every text search is no longer ambiguous
//! at all. This test now asserts the PASSING criterion, not the documented gap: a
//! regression back to a string-search reformatter would make it fail again. The second test
//! pins the other half of the same Design clause: a genuinely nested (two-level)
//! non-unknown-key parse error (a real type mismatch) keeps its dotted path prefix in the
//! message, under the structural tracker exactly as it did under the passthrough branch the
//! earlier string-search rounds relied on.
//!
//! The LAST two tests close a FOURTH gap this same round opened: the structural-tracker
//! rewrite (commit 9bfb4a9) wires a genuinely NEW call site into the shared parser -
//! `config::parse_agent`, previously a bare `serde_yaml::from_str` and explicitly OUT of
//! every earlier round's scope ("not testing parse_agent ... untouched by this diff"). It
//! is a real cross-module seam (`.rigger/agents/*.md` frontmatter, driven through
//! `read_agents_dir` -> `config_store::load` -> `rigger validate`, exactly like the
//! workflow-side call sites above) and a genuine behavior-mechanism change (the whole
//! deserialization path, not just an error-formatting detail), so it gets the same
//! real-schema, real-binary treatment as every other call site in this file rather than
//! resting on the implementer's own synthetic-struct unit test. The first proves a real
//! type mismatch in agent frontmatter still surfaces unchanged (never misreported as an
//! unknown key); the second proves `AgentDef`'s deliberate non-`deny_unknown_fields`
//! exemption (foreign frontmatter import) survives the rewrite of the mechanism it now
//! shares with every `deny_unknown_fields` struct.
//!
//! The next two tests close a FIFTH gap: the adjudicator rejected round 4's structural
//! tracker itself on two bugs in HOW it recomposes `e.path()`'s segments back into text,
//! not in where the path comes from. `expected_prefix` (the parent-path composition used
//! to classify an unknown-field violation) joined segments unconditionally, putting a "."
//! before a `Segment::Seq` index that `serde_yaml`'s own rendering never does - missing the
//! classification entirely for an unknown field inside a `Vec<T>` element, real
//! pre-existing schema, no crafted input. Separately, the path REPORTED to the operator
//! joined a `Segment::Map` key's raw text with the same "." the format uses as a separator,
//! so a key that itself contains a literal "." recomposed a path indistinguishable from
//! genuine nesting. Both fixes are pinned by the implementer's own unit tests against
//! synthetic fixture structs (`src/config_store.rs`) - these two close the same gap every
//! earlier round's pair did, through the real schema and the compiled binary.
//!
//! The FINAL test closes a SIXTH gap the round-5 fix itself opened: `render_path_segments`'s
//! `escape_keys` mode backslash-escapes TWO characters inside a `Segment::Map`/`Enum` key -
//! a literal "." (closed above) AND a literal "\" (the escape character itself, needed so
//! the escaping is unambiguous - without it a key already containing a "\" followed by what
//! looks like an escaped "." could misread as one). The round-5 implementer's own tests (unit
//! and periphery) cover only the "." branch; the "\" branch had no test anywhere. A stage
//! name is arbitrary operator-writable text with no character restriction anywhere in
//! `config_store::validate` (the same fact the dot-bearing-stage-name test above rests on),
//! so a stage containing a literal backslash is exactly as real and producible as one
//! containing a dot - same defect family this unit has been rejected on four rounds running,
//! caught here before a fifth.

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

/// `rigger init` scaffolds a `.rigger/agents/` fleet that parses cleanly on its own, so
/// overwriting ONLY `workflow.yml` afterward isolates the unknown-key failure to the one
/// key under test rather than an unrelated agents-dir problem masking it.
const WORKFLOW_WITH_UNKNOWN_KEY: &str =
    "name: fixture\ndefaults:\n  autonomy: auto_notify\n  max_parallel_unitz: 2\n";

/// The surface pair that must fail on a fixture with the same text: the CLI command and the
/// library call it resolves through.
#[derive(Clone, Copy)]
enum Surface {
    /// `rigger validate` over [`rigger::config_store::load`].
    Validate,
    /// `rigger status` over [`rigger::config_store::read_store_config`] (store selection
    /// resolves before any command requires an existing `events.db`).
    Status,
}

/// A fresh `rigger init`ed project whose `.rigger/<rel>` is `content`, failing on both
/// halves of `surface`: returns `(library error, CLI stderr)` after asserting the CLI fails
/// and carries the library's exact text (spec 102 criterion 3: "the same text", not merely
/// each containing a match for the same substring pattern independently).
fn both_fail(surface: Surface, rel: &str, content: &str) -> (String, String) {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(root.join(".rigger").join(rel), content)
        .unwrap_or_else(|e| panic!("write the .rigger/{rel} fixture: {e}"));

    let (lib_err, command) = match surface {
        Surface::Validate => (
            rigger::config_store::load(root.to_str().unwrap())
                .expect_err("the fixture must fail to load")
                .to_string(),
            "validate",
        ),
        Surface::Status => (
            rigger::config_store::read_store_config(&root.join(".rigger"))
                .expect_err("the fixture's store: block must fail to load")
                .to_string(),
            "status",
        ),
    };

    let (_out, cli_err, cli_ok) = run_rigger(root, &[command]);
    assert!(!cli_ok, "rigger {command} must fail on the fixture");
    assert!(
        cli_err.contains(&lib_err),
        "rigger {command} and its library call must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
    (lib_err, cli_err)
}

/// Both halves of `surface` fail on `workflow` naming the unknown key's dotted path `named`.
fn both_name(surface: Surface, workflow: &str, named: &str) {
    let (lib_err, cli_err) = both_fail(surface, "workflow.yml", workflow);
    assert!(
        lib_err.contains(named),
        "the library call must name the dotted path {named:?}: {lib_err}"
    );
    assert!(
        cli_err.contains(named),
        "the CLI's stderr must name the same dotted path {named:?}; stderr:\n{cli_err}"
    );
}

/// `rigger validate` and `config_store::load` both fail on `.rigger/<rel>` = `content` with a
/// genuine type mismatch, never misreported as an unknown key, carrying every `kept` text.
fn both_keep_a_type_mismatch(rel: &str, content: &str, kept: &[&str]) {
    let (lib_err, cli_err) = both_fail(Surface::Validate, rel, content);
    assert!(
        !lib_err.contains("unknown key"),
        "a type mismatch must never be misreported as an unknown key: {lib_err}"
    );
    for text in kept {
        assert!(
            lib_err.contains(text),
            "config_store::load must keep {text:?} in the type-mismatch message: {lib_err}"
        );
    }
    assert!(
        !cli_err.contains("unknown key"),
        "rigger validate's stderr must never misreport a type mismatch as an unknown key; \
         stderr:\n{cli_err}"
    );
}

rigger::test_cases! {

    rigger_validate_and_config_store_load_name_the_same_dotted_path_for_an_unknown_key:
        both_name(
            Surface::Validate,
            WORKFLOW_WITH_UNKNOWN_KEY,
            "defaults.max_parallel_unitz: unknown key",
        );

    /// `read_store_config`'s own `dotted_unknown_key` wiring (a SEPARATE call site from
    /// `load_workflow`'s, over the `StoreConfig` struct rather than `Workflow`/`Defaults`) -
    /// unreached by any other test in this tree. `rigger status` drives it because store
    /// selection (§48 rung 4, the committed `store:` block) resolves before any command
    /// requires an existing `events.db`, so this needs no run bootstrapped first.
    rigger_status_and_read_store_config_name_the_same_dotted_path_for_an_unknown_store_key:
        both_name(
            Surface::Status,
            "name: fixture\nstore:\n  backend: sqlite\n  urll: bogus\n",
            "store.urll: unknown key",
        );

    /// The adjudicator rejected the first round on two adversary-found edge cases in
    /// `dotted_unknown_key`, both fixed purely inside that pure function and pinned there by
    /// the implementer's own unit tests (`src/config_store.rs`) against synthetic fixture
    /// structs unrelated to the real schema. Neither edge case had ever been driven through
    /// the REAL `Workflow`/`Defaults` schema or the compiled binary - this proves the fix
    /// survives the actual CLI boundary, not just the synthetic pure-function case the unit
    /// tests constructed.
    ///
    /// A genuine type mismatch on a real config field (`max_retries: u32`) whose invalid
    /// string VALUE happens to echo the "unknown field `" marker wording must still surface
    /// serde_yaml's real type-mismatch message unchanged - never be corrupted into a
    /// fabricated "unknown key" report that hides the real defect.
    rigger_validate_passes_through_a_type_mismatch_whose_value_echoes_the_unknown_field_marker:
        both_keep_a_type_mismatch(
            "workflow.yml",
            "name: fixture\ndefaults:\n  max_retries: \"unknown field `evil`, expected `max_retries`\"\n",
            &["invalid type"],
        );

    /// The second adversary-found edge case: an unknown key whose own NAME contains a
    /// backtick must be named in full, not silently truncated at the embedded backtick.
    rigger_validate_names_the_full_key_when_the_unknown_key_itself_contains_a_backtick:
        both_name(
            Surface::Validate,
            "name: fixture\ndefaults:\n  weird`field: 1\n",
            "defaults.weird`field: unknown key",
        );

    /// Round 2's first adversary-found edge case: an unknown key whose own NAME contains the
    /// exact `"`, "` run `dotted_unknown_key` anchors the field terminator on (round 1 fixed
    /// truncation at a bare backtick; a field name echoing the FULL terminator text still
    /// truncated a first-occurrence search). `defaults:` has more than a handful of known
    /// fields, so the real "expected one of `a`, `b`, ..." trailer this produces itself
    /// contains several more `"`, "` runs beyond the crafted field name's own - proving the
    /// fix picks the true (rightmost) terminator through a real multi-field expected-list,
    /// not just the single-item synthetic list the implementer's own unit test constructs.
    rigger_validate_names_the_full_key_when_the_unknown_key_itself_contains_the_terminator_run:
        both_name(
            Surface::Validate,
            "name: fixture\ndefaults:\n  \"weird`, field\": 1\n",
            "defaults.weird`, field: unknown key",
        );

    /// Round 2's second adversary-found edge case: a nested path segment sourced from a real
    /// YAML MAP KEY - a `stages:` name, arbitrary operator text unlike a plain struct field -
    /// that embeds its own `": "`, the exact text the path/marker boundary search anchors on.
    /// Drives the real `Workflow.stages: BTreeMap<String, Stage>` production seam (not the
    /// synthetic `WithStages`/`Stage` fixture the implementer's own unit test constructs) end
    /// to end through `rigger validate`, so the recomposed dotted path - `stages` joined to
    /// the colon-space-bearing stage name joined to the unknown field - is proven at the real
    /// CLI boundary, not merely against a local struct.
    rigger_validate_recomposes_a_stage_path_through_a_stage_name_containing_its_own_colon_space:
        both_name(
            Surface::Validate,
            "name: fixture\nstages:\n  \"foo: bar\":\n    unknown_stage_field: 1\n",
            "stages.foo: bar.unknown_stage_field: unknown key",
        );

    /// Spec 102's Design (amended at rigger-run@3df56f8) requires the dotted path come from a
    /// STRUCTURAL tracker: "it is never recovered by searching the rendered error text, because
    /// a key or a value can echo any delimiter the search would anchor on and the recovered
    /// message is then wrong." Every earlier round was still a string search - round 3
    /// (`addc1d0`) improved the anchor (a marker position must be immediately preceded by
    /// `": "`), but when a crafted key made TWO such positions exist, it declined to guess and
    /// returned the raw, un-reformatted message rather than a wrong one; the criterion itself -
    /// "an error naming that dotted path" - was unmet for this input under that round. An
    /// unknown key literally named `` z: unknown field `y `` under `defaults:` is real
    /// operator-writable YAML text (quoted), not a contrived internal fixture, and it makes a
    /// string scan see its own real path/marker boundary AND a second, embedded one inside the
    /// key's own name - genuinely ambiguous by construction to anything that searches rendered
    /// text. `config::parse_yaml_naming_unknown_keys` (commit 9bfb4a9, wrapping
    /// `serde_path_to_error`) is immune: its path comes from the actual key the deserializer
    /// read, never from scanning what it rendered afterward, so this is no longer ambiguous at
    /// all. This test now PINS that fix at the real CLI boundary - a regression back to any
    /// text-search reformatter would fail it again.
    rigger_validate_names_the_dotted_path_even_when_the_unknown_key_echoes_the_marker_boundary:
        both_name(
            Surface::Validate,
            "name: fixture\ndefaults:\n  \"z: unknown field `y\": 1\n",
            "defaults.z: unknown field `y: unknown key",
        );

    /// The other half of the same Design clause: "every other parse error is rendered with the
    /// same tracked path prefix and its message otherwise unchanged." `dotted_unknown_key`'s
    /// passthrough branch (the error is not an unknown-field violation at all) does nothing to
    /// the message - it relies entirely on `serde_yaml` already having prefixed its own tracked
    /// path onto a nested violation. This pins that today, at two levels of real nesting
    /// (`stages.<name>.<field>`, not the one-level `defaults.<field>` the existing echoed-marker
    /// passthrough test above already covers), so a future structural-tracker rewrite that
    /// replaces the whole function is proven not to have dropped this already-working case.
    rigger_validate_keeps_the_nested_dotted_path_on_a_genuine_type_mismatch:
        both_keep_a_type_mismatch(
            "workflow.yml",
            "name: fixture\nstages:\n  foo:\n    partition: [1, 2]\n",
            &["stages.foo.partition", "invalid type"],
        );

    /// A FIFTH gap, from a fourth remediation round: the adjudicator rejected round 4's
    /// structural-tracker rewrite itself on two path/prefix-composition bugs, both inside
    /// `parse_yaml_naming_unknown_keys`'s own recomposition of `e.path()`'s segments, never
    /// pinned through the real `Workflow` schema or the compiled binary - only against
    /// synthetic fixture structs in `src/config_store.rs`.
    ///
    /// `defaults.failure_rules: Vec<FailureRuleDef>` is real, PRE-EXISTING production schema
    /// (spec 10 unit 2, untouched by this diff) - an unknown key inside one of its elements is
    /// an ordinary typo, no crafted input at all. Round 4's `parent` computation joined
    /// segments with an unconditional "." that put a separator before the `[N]` sequence
    /// index; `serde_yaml`'s own rendered message never does, so the classification's
    /// `starts_with` check silently missed a GENUINE unknown-key violation and fell through to
    /// the raw, un-reformatted message - criterion 3 ("AN UNKNOWN KEY IS NAMED ... at every
    /// config level") unmet for any Vec-nested field, through no fault of the operator's input.
    rigger_validate_names_the_dotted_path_for_an_unknown_key_inside_a_vec_element:
        both_name(
            Surface::Validate,
            "name: fixture\ndefaults:\n  failure_rules:\n    - {}\n    - bogus_field: 1\n",
            "defaults.failure_rules[1].bogus_field: unknown key",
        );

    /// The round-4 adjudicator's second path-composition finding: a `stages:` name (arbitrary
    /// operator-writable YAML text, already legal today - no character restriction anywhere in
    /// `config_store::validate`) containing its own literal "." recomposed, under round 4's
    /// unescaped join, into a dotted path structurally indistinguishable from a stage "foo"
    /// genuinely nested under a substructure "bar" - an operator reading the message could not
    /// tell which stage was actually wrong. This proves the fix's escaping through the real
    /// `Workflow.stages: BTreeMap<String, Stage>` production seam and the compiled binary, not
    /// just the synthetic fixture structs the implementer's own unit test constructs.
    rigger_validate_escapes_a_literal_dot_inside_a_stage_name:
        both_name(
            Surface::Validate,
            "name: fixture\nstages:\n  \"foo.bar\":\n    unknown_stage_field: 1\n",
            "stages.foo\\.bar.unknown_stage_field: unknown key",
        );

    /// A SIXTH gap, from the same round-5 fix: `render_path_segments`'s `escape_keys` mode
    /// escapes a literal "\" inside a map key the same way it escapes a literal "." - both are
    /// documented as escaped (src/config.rs, `render_path_segments`'s doc comment), but only the
    /// "." branch had a test anywhere (unit or periphery) before this one. A stage name
    /// containing its own literal backslash is exactly as legal and operator-producible as one
    /// containing a dot (no character restriction anywhere in `config_store::validate`), so this
    /// proves the fix's other escape branch through the real `Workflow.stages:
    /// BTreeMap<String, Stage>` production seam and the compiled binary, matching the sibling
    /// dot-escaping test above.
    ///
    /// YAML double-quoted `"foo\\bar"` decodes to one literal backslash between "foo" and
    /// "bar" - the stage name the deserializer actually sees is `foo\bar` (7 bytes, one `\`).
    rigger_validate_escapes_a_literal_backslash_inside_a_stage_name:
        both_name(
            Surface::Validate,
            "name: fixture\nstages:\n  \"foo\\\\bar\":\n    unknown_stage_field: 1\n",
            "stages.foo\\\\bar.unknown_stage_field: unknown key",
        );
}

/// Spec 102's amended Design names TWO parse sites for the structural tracker: "the
/// workflow file ... and the agent frontmatter" (`op-102-c3-dotted-path-structural-tracker`,
/// `op-102-c3-r3-review-note-stale-spec-in-worktree`). `config::parse_agent` is wired
/// through the SAME [`rigger::config::parse_yaml_naming_unknown_keys`] `load_workflow`
/// uses (`src/config.rs`, the `frontmatter:` call site) - a genuinely NEW cross-module
/// seam this diff adds (round 1/2 explicitly scoped it OUT: "not testing parse_agent,
/// untouched by this diff"). The implementer's own unit test
/// (`parse_agent_routes_a_type_mismatch_through_the_shared_structural_parser`,
/// `src/config_store.rs`) pins this against a synthetic hand-rolled `AgentDef`-shaped
/// struct with a loose `.contains()` check, never through the real `AgentDef` schema, a
/// real on-disk `.rigger/agents/*.md` file, or the compiled binary. This proves the SAME
/// property end to end: a real `recurse: not-a-bool` type mismatch in a real agent
/// frontmatter file, loaded by [`rigger::config_store::load`] and by `rigger validate`,
/// must surface `serde_yaml`'s own message UNCHANGED (not merely a superstring of it,
/// exact byte-for-byte) - proving the shared parser's passthrough branch runs here with
/// no divergence, and is never misclassified as an unknown-key violation.
#[test]
fn rigger_validate_and_config_store_load_preserve_an_agent_frontmatter_type_mismatch_unchanged() {
    // The exact raw message a direct serde_yaml parse of the same frontmatter produces,
    // measured (not assumed) in this same test run, so "unchanged" is a real comparison.
    let raw = serde_yaml::from_str::<rigger::config::AgentDef>("id: probe\nrecurse: not-a-bool\n")
        .expect_err("a bool field given a string must fail to parse")
        .to_string();

    both_keep_a_type_mismatch(
        "agents/zzz-probe.md",
        "---\nid: probe\nrecurse: not-a-bool\n---\nBody.\n",
        &[&raw],
    );
}

/// The other half of the same new seam: `AgentDef` is deliberately NOT
/// `#[serde(deny_unknown_fields)]` (`src/config.rs`'s own doc comment on the struct) so
/// `rigger setup --agents` can import foreign frontmatter carrying fields Rigger does not
/// model. Routing `parse_agent` through the shared structural parser is a genuine rewrite
/// of ITS deserialization mechanism (`serde_yaml::from_str` directly, to
/// `serde_path_to_error::deserialize` wrapping a `serde_yaml::Deserializer`) - a
/// regression risk this criterion's own accounting must rule out, not assume: an unknown
/// key in agent frontmatter must still parse cleanly, exactly as before this diff, through
/// the real compiled binary.
#[test]
fn rigger_validate_still_ignores_an_unrecognized_agent_frontmatter_key() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("agents").join("zzz-foreign.md"),
        "---\nid: zzz-foreign\ndescription: a foreign field rigger does not model\n---\nBody.\n",
    )
    .expect("add the foreign-key agent fixture");

    let cfg = rigger::config_store::load(root.to_str().unwrap())
        .expect("an unrecognized agent frontmatter key must still load cleanly");
    assert!(
        cfg.agents.contains_key("zzz-foreign"),
        "the foreign-keyed agent must be present in the loaded fleet: {:?}",
        cfg.agents.keys().collect::<Vec<_>>()
    );

    let (out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        cli_ok,
        "rigger validate must still succeed on an agent file carrying an unrecognized key; \
         stderr:\n{cli_err}"
    );
    assert!(
        out.contains("7 agents"),
        "rigger validate must count the foreign-keyed agent among the loaded fleet (the 6 \
         scaffolded defaults plus this one): {out}"
    );
}
