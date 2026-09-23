//! Periphery (CLI, real-binary) test for spec 102, criterion 3 - AN UNKNOWN KEY IS NAMED.
//!
//! `config_store::load_workflow`'s own unit tests (`src/config_store.rs`) already pin the
//! dotted-path reformatting at the library level. This file proves the criterion's OTHER
//! explicit clause - "`rigger validate` on the same file fails with the same text" - end to
//! end, through the compiled binary, and against the library call `load()`/`validate` itself
//! resolve through: both are driven against the byte-identical fixture file in one test, so
//! "same text" is measured, not assumed from the two call sites sharing code.
//!
//! The criterion OWNS unknown-key rejection "at every config level" - and `dotted_unknown_key`
//! is wired at THREE independent call sites (`config.rs`'s own doc comment on the commit that
//! added it), not one: `load_workflow` (proven above), `read_store_config`, and
//! `read_scratch_defaults`. The second test below closes `read_store_config`'s own wiring,
//! which no other test (unit or periphery) reaches - it is a SEPARATE `.map_err` call over a
//! DIFFERENT struct (`StoreConfig`, not `Workflow`/`Defaults`), driven by `rigger status`
//! rather than `rigger validate` because `read_store_config` is the store-selection probe
//! (§48 rung 4) every store-opening command resolves through BEFORE it ever requires an
//! existing run, so it is observable without first bootstrapping an `events.db`.
//! `read_scratch_defaults`'s own wiring is EXEMPT: it wraps the identical `Defaults` struct
//! through the identical `dotted_unknown_key` call and the identical `"parse workflow: {}"`
//! prefix `load_workflow` already uses (byte-for-byte, per `src/config_store.rs`), so its
//! dotted-path text is already proven by the first test above; its one CLI-propagating caller
//! (`reset --build-cache`, via `read_scratch_workdir`) has no additional branching logic to
//! diverge on, and its other production caller (`scratch_defaults` in `main.rs`) deliberately
//! discards the error via `.unwrap_or_default()` (a pre-existing, documented contract this
//! diff does not change), so no new behavior is observable there at all.
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
//! The LAST two tests below test the criterion itself, not any one round's patch, per
//! spec 102's Design amendment (rigger-run@3df56f8, "THE DOTTED PATH IS TRACKED
//! STRUCTURALLY"): the path must come from a structural tracker (`serde_path_to_error`),
//! never from searching the rendered message, because a key OR a value can echo any
//! delimiter that search anchors on. The round-3 fix (`addc1d0`) is still a string search
//! under a `"`, "`/`": "`-anchored scan that DECLINES TO GUESS - passes the raw,
//! un-reformatted message through - whenever a key or value makes that scan see more than
//! one candidate boundary. The first test below drives that exact ambiguity through the
//! real `defaults:` field and proves the criterion ("an error naming that dotted path") is
//! NOT met for it: `rigger validate`/`config_store::load` fail, but with no dotted-path
//! line at all, only serde_yaml's raw message - this is EXPECTED TO FAIL until the
//! structural tracker replaces the string search. The second test pins a Design clause the
//! CURRENT code already satisfies and must keep satisfying: a genuinely nested (two-level)
//! non-unknown-key parse error (a real type mismatch) keeps its dotted path prefix in the
//! message, because `serde_yaml` already carries it natively and the passthrough branch
//! leaves it untouched.

mod common;

use std::path::Path;
use std::process::Command;

fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

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

#[test]
fn rigger_validate_and_config_store_load_name_the_same_dotted_path_for_an_unknown_key() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        WORKFLOW_WITH_UNKNOWN_KEY,
    )
    .expect("overwrite workflow.yml with the unknown-key fixture");

    // The library call `rigger validate` itself resolves through.
    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("a workflow.yml carrying an unrecognized key must fail to load")
        .to_string();
    assert!(
        lib_err.contains("defaults.max_parallel_unitz: unknown key"),
        "config_store::load must name the dotted path of the unrecognized key: {lib_err}"
    );

    // The CLI surface, through the compiled binary.
    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on an unrecognized config key"
    );
    assert!(
        cli_err.contains("defaults.max_parallel_unitz: unknown key"),
        "rigger validate's stderr must name the same dotted path; stderr:\n{cli_err}"
    );

    // Both surfaces must report the identical text (spec 102 criterion 3: "the same text"),
    // not merely each contain a match for the same substring pattern independently.
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

/// `read_store_config`'s own `dotted_unknown_key` wiring (a SEPARATE call site from
/// `load_workflow`'s, over the `StoreConfig` struct rather than `Workflow`/`Defaults`) -
/// unreached by any other test in this tree. `rigger status` drives it because store
/// selection (§48 rung 4, the committed `store:` block) resolves before any command
/// requires an existing `events.db`, so this needs no run bootstrapped first.
#[test]
fn rigger_status_and_read_store_config_name_the_same_dotted_path_for_an_unknown_store_key() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\nstore:\n  backend: sqlite\n  urll: bogus\n",
    )
    .expect("overwrite workflow.yml with the unknown-key fixture");

    // The library call `read_store_config` itself resolves through.
    let lib_err = rigger::config_store::read_store_config(&root.join(".rigger"))
        .expect_err("a store: block carrying an unrecognized key must fail to load")
        .to_string();
    assert!(
        lib_err.contains("store.urll: unknown key"),
        "read_store_config must name the dotted path of the unrecognized key: {lib_err}"
    );

    // The CLI surface, through the compiled binary.
    let (_out, cli_err, cli_ok) = run_rigger(root, &["status"]);
    assert!(
        !cli_ok,
        "rigger status must fail on an unrecognized store: key"
    );
    assert!(
        cli_err.contains("store.urll: unknown key"),
        "rigger status's stderr must name the same dotted path; stderr:\n{cli_err}"
    );

    // Both surfaces must report the identical text, exactly as the load_workflow case above
    // proves for its own two call sites.
    assert!(
        cli_err.contains(&lib_err),
        "rigger status and read_store_config must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

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
#[test]
fn rigger_validate_passes_through_a_type_mismatch_whose_value_echoes_the_unknown_field_marker() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\ndefaults:\n  max_retries: \"unknown field `evil`, expected `max_retries`\"\n",
    )
    .expect("overwrite workflow.yml with the embedded-marker fixture");

    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("a wrongly-typed max_retries must fail to load")
        .to_string();
    assert!(
        !lib_err.contains("unknown key"),
        "a type mismatch must never be misreported as an unknown key: {lib_err}"
    );
    assert!(
        lib_err.contains("invalid type"),
        "config_store::load must surface serde_yaml's real type-mismatch message: {lib_err}"
    );

    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on a wrongly-typed field"
    );
    assert!(
        !cli_err.contains("unknown key"),
        "rigger validate's stderr must never misreport a type mismatch as an unknown key; stderr:\n{cli_err}"
    );
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

/// The second adversary-found edge case: an unknown key whose own NAME contains a
/// backtick must be named in full, not silently truncated at the embedded backtick.
#[test]
fn rigger_validate_names_the_full_key_when_the_unknown_key_itself_contains_a_backtick() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\ndefaults:\n  weird`field: 1\n",
    )
    .expect("overwrite workflow.yml with the backtick-key fixture");

    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("an unknown key containing a backtick must still fail to load")
        .to_string();
    assert!(
        lib_err.contains("defaults.weird`field: unknown key"),
        "config_store::load must name the FULL field, not truncate at the embedded backtick: {lib_err}"
    );

    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on the backtick-bearing unknown key"
    );
    assert!(
        cli_err.contains("defaults.weird`field: unknown key"),
        "rigger validate's stderr must name the same full field; stderr:\n{cli_err}"
    );
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

/// Round 2's first adversary-found edge case: an unknown key whose own NAME contains the
/// exact `"`, "` run `dotted_unknown_key` anchors the field terminator on (round 1 fixed
/// truncation at a bare backtick; a field name echoing the FULL terminator text still
/// truncated a first-occurrence search). `defaults:` has more than a handful of known
/// fields, so the real "expected one of `a`, `b`, ..." trailer this produces itself
/// contains several more `"`, "` runs beyond the crafted field name's own - proving the
/// fix picks the true (rightmost) terminator through a real multi-field expected-list,
/// not just the single-item synthetic list the implementer's own unit test constructs.
#[test]
fn rigger_validate_names_the_full_key_when_the_unknown_key_itself_contains_the_terminator_run() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\ndefaults:\n  \"weird`, field\": 1\n",
    )
    .expect("overwrite workflow.yml with the terminator-embedded-key fixture");

    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("an unknown key embedding the terminator run must still fail to load")
        .to_string();
    assert!(
        lib_err.contains("defaults.weird`, field: unknown key"),
        "config_store::load must name the FULL field, not truncate at the embedded \
         terminator run: {lib_err}"
    );

    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on the terminator-embedding unknown key"
    );
    assert!(
        cli_err.contains("defaults.weird`, field: unknown key"),
        "rigger validate's stderr must name the same full field; stderr:\n{cli_err}"
    );
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

/// Round 2's second adversary-found edge case: a nested path segment sourced from a real
/// YAML MAP KEY - a `stages:` name, arbitrary operator text unlike a plain struct field -
/// that embeds its own `": "`, the exact text the path/marker boundary search anchors on.
/// Drives the real `Workflow.stages: BTreeMap<String, Stage>` production seam (not the
/// synthetic `WithStages`/`Stage` fixture the implementer's own unit test constructs) end
/// to end through `rigger validate`, so the recomposed dotted path - `stages` joined to
/// the colon-space-bearing stage name joined to the unknown field - is proven at the real
/// CLI boundary, not merely against a local struct.
#[test]
fn rigger_validate_recomposes_a_stage_path_through_a_stage_name_containing_its_own_colon_space() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\nstages:\n  \"foo: bar\":\n    unknown_stage_field: 1\n",
    )
    .expect("overwrite workflow.yml with the colon-space-stage-name fixture");

    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("a stage name embedding its own colon-space must still fail to load")
        .to_string();
    assert!(
        lib_err.contains("stages.foo: bar.unknown_stage_field: unknown key"),
        "config_store::load must recompose the full path through the embedded colon-space, \
         not split at it: {lib_err}"
    );

    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on the unrecognized stage field"
    );
    assert!(
        cli_err.contains("stages.foo: bar.unknown_stage_field: unknown key"),
        "rigger validate's stderr must name the same recomposed path; stderr:\n{cli_err}"
    );
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

/// Spec 102's Design (amended at rigger-run@3df56f8) requires the dotted path come from a
/// STRUCTURAL tracker: "it is never recovered by searching the rendered error text, because
/// a key or a value can echo any delimiter the search would anchor on and the recovered
/// message is then wrong." Round 3's fix is still a string search - it improved the anchor
/// (a marker position must be immediately preceded by `": "`), but when a crafted key makes
/// TWO such positions exist, it declines to guess and returns the raw, un-reformatted
/// message rather than a wrong one. That is safer than corrupting the message, but it means
/// the criterion itself - "an error naming that dotted path" - is unmet for this input: no
/// dotted-path line is produced at all. An unknown key literally named
/// `` z: unknown field `y `` under `defaults:` is real operator-writable YAML text (quoted),
/// not a contrived internal fixture, and it makes the scan see its own real path/marker
/// boundary AND a second, embedded one inside the key's own name - genuinely ambiguous by
/// the function's own documented rule. EXPECTED TO FAIL on the current string-search
/// implementation; closes only once the structural tracker (`serde_path_to_error`) replaces
/// it, per `op-102-c3-dotted-path-structural-tracker`.
#[test]
fn rigger_validate_names_the_dotted_path_even_when_the_unknown_key_echoes_the_marker_boundary() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\ndefaults:\n  \"z: unknown field `y\": 1\n",
    )
    .expect("overwrite workflow.yml with the boundary-echoing-key fixture");

    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("an unknown key echoing the marker boundary must still fail to load")
        .to_string();
    assert!(
        lib_err.contains("defaults.z: unknown field `y: unknown key"),
        "config_store::load must still name the dotted path of the unrecognized key even \
         when the key's own name echoes the path/marker boundary the reformatter scans for \
         - a string search over untrusted content cannot disambiguate this, only a \
         structural tracker (serde_path_to_error) can: {lib_err}"
    );

    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on the boundary-echoing unknown key"
    );
    assert!(
        cli_err.contains("defaults.z: unknown field `y: unknown key"),
        "rigger validate's stderr must name the same dotted path; stderr:\n{cli_err}"
    );
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}

/// The other half of the same Design clause: "every other parse error is rendered with the
/// same tracked path prefix and its message otherwise unchanged." `dotted_unknown_key`'s
/// passthrough branch (the error is not an unknown-field violation at all) does nothing to
/// the message - it relies entirely on `serde_yaml` already having prefixed its own tracked
/// path onto a nested violation. This pins that today, at two levels of real nesting
/// (`stages.<name>.<field>`, not the one-level `defaults.<field>` the existing echoed-marker
/// passthrough test above already covers), so a future structural-tracker rewrite that
/// replaces the whole function is proven not to have dropped this already-working case.
#[test]
fn rigger_validate_keeps_the_nested_dotted_path_on_a_genuine_type_mismatch() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\nstages:\n  foo:\n    partition: [1, 2]\n",
    )
    .expect("overwrite workflow.yml with the nested-type-mismatch fixture");

    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("a wrongly-typed nested field must fail to load")
        .to_string();
    assert!(
        !lib_err.contains("unknown key"),
        "a type mismatch must never be misreported as an unknown key: {lib_err}"
    );
    assert!(
        lib_err.contains("stages.foo.partition"),
        "a genuinely nested type mismatch must keep its two-level dotted path prefix, not \
         just the bare field or the bare stage: {lib_err}"
    );
    assert!(
        lib_err.contains("invalid type"),
        "config_store::load must surface serde_yaml's real type-mismatch message: {lib_err}"
    );

    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on the wrongly-typed nested field"
    );
    assert!(
        cli_err.contains("stages.foo.partition"),
        "rigger validate's stderr must keep the same nested dotted path; stderr:\n{cli_err}"
    );
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}
