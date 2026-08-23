//! CLI periphery for spec 66, unit c3 (SPEC LINT): `rigger validate <spec>` on a fixture
//! carrying all four defect kinds this criterion names (a multi-behavior criterion, an
//! ownerless criterion among three-plus, a disposition smell outside Notes, and an em
//! dash) reports each with its criterion and field-guide class, exits 0, and reports a
//! clean fixture clean.
//!
//! What this file OWNS: the end-to-end `cmd_validate` wiring (the compiled binary's
//! stdout/stderr/exit code on a real spec file), driving the real binary so the observable
//! surface an operator sees is pinned exactly. NOT owned: the pure lint heuristics
//! themselves (ownership/disposition/hygiene detection, Notes/fence/inline-code exclusion),
//! which carry their own unit tests beside their implementation in `src/spec.rs`.

mod common;

use std::path::Path;
use std::process::Command;

/// A throwaway project dir that is its own git repo (so `project_identity()` is stable).
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Run `rigger <args...>` in `cwd`, returning (stdout, stderr, success). Mirrors
/// `tests/cli.rs::run_rigger`'s conventions (opt out of the auto-dashboard, isolate the
/// instance registry) so this suite behaves identically to the rest of the CLI periphery.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// The Done-when-c3 acceptance test: a fixture carrying all four defect kinds - a
/// multi-behavior criterion (1), an ownerless criterion among three-plus (2), a
/// disposition smell outside Notes (3), and an em dash (4) - each reported with its
/// criterion and field-guide class; `rigger validate` still exits 0.
#[test]
fn validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let bad_spec = "# Widget\n\n## Done when\n\n\
         - [ ] the daemon starts on boot, and it writes a pidfile, and it rotates the log \
         nightly. This criterion OWNS the startup sequence.\n\
         - [ ] the store passes the contract suite\n\
         - [ ] either the recovery path retries or it escalates immediately. This \
         criterion OWNS the recovery path.\n\
         - [ ] the report renders a trailing summary line \u{2014} appended at the end. \
         This criterion OWNS the summary render.\n";
    let bad_path = root.join("bad-spec.md");
    std::fs::write(&bad_path, bad_spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", bad_path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );

    // 1: multi-behavior criterion - the pre-existing shape class, still named verbatim.
    assert!(
        err.contains("multi-behavior") && err.contains("criterion 1"),
        "criterion 1's multi-behavior defect must be named on stderr; stderr:\n{err}"
    );
    // 2: ownerless criterion among three-plus - F1 ownership, naming criterion 2.
    assert!(
        err.contains("F1 ownership") && err.contains("criterion 2"),
        "criterion 2's missing OWNS sentence must be flagged F1 ownership; stderr:\n{err}"
    );
    // 3: disposition smell (either...or) outside Notes - F4 disposition, criterion 3.
    assert!(
        err.contains("F4 disposition") && err.contains("criterion 3"),
        "criterion 3's either...or smell must be flagged F4 disposition; stderr:\n{err}"
    );
    // 4: the em dash - hygiene, criterion 4.
    assert!(
        err.contains("hygiene") && err.contains("criterion 4"),
        "criterion 4's em dash must be flagged hygiene; stderr:\n{err}"
    );
}

/// The line of `err` containing `needle`, or a panic naming what was searched for - so a
/// missing line fails with the same diagnostic detail as a `contains` assertion.
fn find_line<'a>(err: &'a str, needle: &str) -> &'a str {
    err.lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no stderr line contains {needle:?}; stderr:\n{err}"))
}

/// A defect sitting in Design prose - OUTSIDE every Done-when checkbox - draws no
/// criterion attribution: `LintAdvisory::criterion` is `None`, and `Display`'s `None` arm
/// (`"{class}: {detail}"`, no `(criterion N)` clause) is what an operator actually sees on
/// the real, compiled binary's stderr. The two dirty-fixture tests above only ever place a
/// defect INSIDE a checkbox, so this is the only test proving the `None` branch of the
/// `LintAdvisory` `Display` contract - and the criterion-less disposition/hygiene wiring -
/// end to end through `cmd_validate`, not merely inside `src/spec.rs`'s own unit tests.
#[test]
fn validate_spec_attributes_a_prose_level_defect_to_no_criterion() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         the daemon starts \u{2014} then it writes a pidfile.\n\n\
         the retry policy could instead retry indefinitely.\n\n\
         ## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let path = root.join("prose-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "a prose-level advisory must never fail validate's exit status; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );

    let hygiene_line = find_line(&err, "hygiene:");
    assert!(
        !hygiene_line.contains("(criterion"),
        "an em dash in Design prose sits outside every checkbox, so its advisory must \
         carry NO criterion clause; line:\n{hygiene_line}"
    );

    let disposition_line = find_line(&err, "F4 disposition:");
    assert!(
        !disposition_line.contains("(criterion"),
        "a disposition smell in Design prose sits outside every checkbox, so its advisory \
         must carry NO criterion clause; line:\n{disposition_line}"
    );
}

/// Two independent checks (F1 ownership and hygiene) firing on the SAME criterion both
/// surface, each naming that criterion - proving `spec_lint_advisories`' aggregation loop
/// (four independent checks folded into one `Vec`, printed one `eprintln!` per advisory in
/// `cmd_validate`) never lets a later advisory replace or suppress an earlier one for a
/// criterion with more than one defect. No fixture anywhere else - unit or periphery -
/// exercises two simultaneous defects on one criterion.
#[test]
fn validate_spec_reports_two_simultaneous_defects_on_the_same_criterion() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the report renders a trailing summary line \u{2014} appended at the end.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let path = root.join("double-defect-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "two simultaneous advisories must never fail validate's exit status; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );

    let ownership_line = find_line(&err, "F1 ownership");
    assert!(
        ownership_line.contains("(criterion 2)"),
        "criterion 2's missing OWNS sentence must be flagged; line:\n{ownership_line}"
    );
    let hygiene_line = find_line(&err, "hygiene");
    assert!(
        hygiene_line.contains("(criterion 2)"),
        "criterion 2's em dash must ALSO be flagged - neither advisory suppresses the \
         other; line:\n{hygiene_line}"
    );
}

/// A clean fixture - three-plus criteria, each carrying an OWNS sentence, single-behavior,
/// no disposition smells, no em dash - draws no spec-lint advisory at all, and `rigger
/// validate` still exits 0.
#[test]
fn validate_reports_a_clean_spec_clean() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let clean_spec = "# Widget\n\n## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let clean_path = root.join("clean-spec.md");
    std::fs::write(&clean_path, clean_spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", clean_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed on a clean spec; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("warning: spec "),
        "a fully clean spec must draw NO spec-lint advisory; stderr:\n{err}"
    );
}
