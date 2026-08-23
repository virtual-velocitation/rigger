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

/// Round-2 fix (`d-u66c3-r2-ownership-scans-full-block`): an OWNS sentence on a WRAPPED
/// CONTINUATION line - this repo's own standard Done-when convention, the exact shape that
/// drove `adj-u66c3-reject-genuine-defects` (all 6/6 criteria of specs/66 itself were
/// wrongly F1-flagged) - must satisfy the ownership check through the real binary, not only
/// inside `src/spec.rs`'s own unit tests. Criterion 1's OWNS sentence sits on its checkbox's
/// SECOND physical line; it must draw no F1 ownership advisory.
#[test]
fn validate_spec_finds_an_owns_sentence_on_a_wrapped_continuation_line() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the daemon writes a pidfile that is mode 0644 and readable only by the\n\
         \x20\x20service account. This criterion OWNS the pidfile permissions.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
         - [ ] the graph supersedes an older decision. This criterion OWNS the supersede \
         path.\n";
    let path = root.join("wrapped-owns-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F1 ownership"),
        "criterion 1's OWNS sentence sits on a wrapped continuation line, not the \
         checkbox's first physical line - it must still satisfy the ownership check on the \
         real binary; stderr:\n{err}"
    );
}

/// Round-2 fix (`d-u66c3-r2-either-word-boundary`): "either" is a substring of "neither", so
/// a "neither ... or" sentence must not be misread as the "either ... or" draft-smell
/// pairing - the exact false positive `sdet-u66c3-either-substring-matches-inside-neither`
/// reproduced on ordinary English through the real binary.
#[test]
fn validate_spec_does_not_misread_neither_or_as_either_or() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         This works in neither case A or case B.\n\n\
         ## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let path = root.join("neither-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "\"neither\" contains \"either\" as a substring; that must not false-fire the \
         either...or pairing on the real binary; stderr:\n{err}"
    );
}

/// Round-2 fix (`d-u66c3-r2-quoted-phrase-exempt`): a draft-smell phrase NAMED in double
/// quotes - the field guide's own convention for listing its exact phrases, the exact shape
/// that made this lint self-trip on its own governing spec
/// (`adv-u66c3-disposition-lint-self-trips-on-its-own-governing-spec`) - must not
/// false-positive through the real binary, sitting outside any checkbox (Design prose), so
/// this also pins the criterion-less `None` attribution for a quoted-phrase hit.
#[test]
fn validate_spec_ignores_a_smell_phrase_named_in_double_quotes() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The lint watches for draft-smell phrases: \"worth considering\", \"either ... \
         or\", \"could instead\".\n\n\
         ## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let path = root.join("quoted-phrase-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a smell phrase NAMED in double quotes must not false-positive on the real binary, \
         the same as a backtick code span already does not; stderr:\n{err}"
    );
}

/// Round-2 REJECT remedy (`sdet-u66c3-r2-or-side-still-bare-substring`,
/// `adv-u66c3-r2-or-side-substring-confirmed-live`): the round-2 fix gave "either" a word
/// boundary but left the "or" side a bare substring check, so a standalone "either" earlier
/// on a line made ANY later or-prefixed non-disjunctive word ("original", "order", "orphan")
/// false-fire the either...or draft-smell pairing with zero real disjunction. Reproduced on
/// the real binary here.
#[test]
fn validate_spec_does_not_misread_a_later_or_prefixed_word_as_either_or() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         Either approach works well; the original design remains valid throughout.\n\n\
         ## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let path = root.join("or-prefixed-word-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "\"original\" contains \" or\" as a substring but is not a real either...or \
         disjunction; a standalone \"either\" earlier on the line must not make it \
         false-fire on the real binary; stderr:\n{err}"
    );
}

/// Round-3 REJECT remedy
/// (`adv-u66c3-r3-ownership-sentinel-inverted-on-explicit-no-owner-prose`):
/// `carries_owner_sentence` matched the bare substring "owner" with no negation guard, so
/// an explicit DENIAL of ownership ("No owner has been assigned to this criterion yet.")
/// was misread as carrying an ownership sentence - inverted from F1's documented purpose of
/// flagging exactly this twin-risk case. Reproduced on the real binary here.
#[test]
fn validate_spec_flags_an_explicit_ownership_denial_as_twin_risk() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the daemon writes a pidfile. No owner has been assigned to this criterion \
         yet.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
         - [ ] the graph supersedes an older decision. This criterion OWNS the supersede \
         path.\n";
    let path = root.join("owner-denial-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F1 ownership") && err.contains("criterion 1"),
        "an explicit ownership denial contains the bare substring \"owner\" but does NOT \
         carry an ownership sentence; criterion 1 must still be flagged twin-risk on the \
         real binary; stderr:\n{err}"
    );
}

/// Round-3 REJECT remedy
/// (`adv-u66c3-r3-worth-considering-still-bare-substring-same-bug-class-as-fixed-either-or`):
/// the "worth considering" check remained a bare substring test after either/or's own
/// word-boundary fix, so a hyphenated compound noun like "self-worth" immediately followed
/// by "considering" false-fired F4 even though it carries no hedging disposition.
/// Reproduced on the real binary here.
#[test]
fn validate_spec_does_not_misread_worth_considering_across_a_hyphenated_compound() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         A fair price reflects self-worth considering every relevant factor.\n\n\
         ## Done when\n\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n\
         - [ ] the conductor integrates an approved unit. This criterion OWNS the \
         integration step.\n";
    let path = root.join("hyphenated-worth-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "\"self-worth\" is a hyphenated compound noun; its trailing \"worth\" followed by \
         \"considering\" must not false-fire the worth-considering draft-smell phrase on \
         the real binary; stderr:\n{err}"
    );
}

/// Round-4 residual gap (`d-u66c3-r4-periphery-tests` reproduced only the "no owner"
/// phrasing that drove the round-3 REJECT; `denies_ownership`'s other two branches -
/// "ownerless" and "not owned" - carried a unit test each but no real-binary reproduction).
/// Both denial phrasings must still make `carries_owner_sentence` return false through the
/// real binary, so a criterion using either one is flagged F1 twin-risk rather than misread
/// as carrying an ownership sentence just because it contains the bare substring "owner".
#[test]
fn validate_spec_flags_ownerless_and_not_owned_denials_as_twin_risk() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the daemon writes a pidfile. This criterion is ownerless for now.\n\
         - [ ] the retry handler backs off exponentially. The backoff logic is not owned \
         by this criterion.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n";
    let path = root.join("ownerless-not-owned-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F1 ownership") && err.contains("(criterion 1)"),
        "\"ownerless\" is an explicit denial, not an ownership sentence; criterion 1 must \
         be flagged twin-risk on the real binary; stderr:\n{err}"
    );
    assert!(
        err.contains("F1 ownership") && err.contains("(criterion 2)"),
        "\"not owned\" is an explicit denial, not an ownership sentence; criterion 2 must \
         be flagged twin-risk on the real binary; stderr:\n{err}"
    );
    assert!(
        !err.contains("(criterion 3)"),
        "criterion 3 carries a genuine OWNS sentence and must draw no F1 advisory; \
         stderr:\n{err}"
    );
}

/// Round-4 preemptive hardening (`d-u66c3-r4-owns-owner-word-boundary`,
/// `ownership_check_does_not_match_owns_or_owner_inside_an_unrelated_word` at the unit
/// layer) never got a real-binary reproduction: `carries_owner_sentence`'s own "owns"/
/// "owner" match switched from a bare substring test to `find_word`, since "owns" is a
/// substring of "drowns" and "owner" is a substring of "downer" with zero ownership claim -
/// the identical bare-substring-across-word-boundary defect class this unit already paid
/// two REJECT cycles to fix for either/or and worth-considering. Without the fix, either
/// word would false-satisfy the check and silently suppress a genuine F1 twin-risk
/// advisory.
#[test]
fn validate_spec_does_not_misread_owns_or_owner_inside_an_unrelated_word_as_ownership() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the retry handler drowns duplicate signals during a backoff storm.\n\
         - [ ] a stale cache entry is a real downer for latency.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n";
    let path = root.join("drowns-downer-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F1 ownership") && err.contains("(criterion 1)"),
        "\"drowns\" contains the bare substring \"owns\" but claims no ownership; \
         criterion 1 must still be flagged twin-risk on the real binary; stderr:\n{err}"
    );
    assert!(
        err.contains("F1 ownership") && err.contains("(criterion 2)"),
        "\"downer\" contains the bare substring \"owner\" but claims no ownership; \
         criterion 2 must still be flagged twin-risk on the real binary; stderr:\n{err}"
    );
    assert!(
        !err.contains("(criterion 3)"),
        "criterion 3 carries a genuine OWNS sentence and must draw no F1 advisory; \
         stderr:\n{err}"
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

/// Round-4 REJECT remedy (a)
/// (`adj-u66c3-r4-reject-owner-veto-and-compound-hyphen-defects`): `denies_ownership`
/// vetoed the WHOLE block the instant it contained "ownerless"/"no owner"/"not owned"
/// anywhere, even when a genuine, unrelated "OWNS" sentence sat elsewhere in the same
/// block - exactly the shape of this unit's own governing spec, whose criterion 3
/// affirmatively OWNS its lint surface while separately describing "an ownerless
/// criterion" as fixture prose for the test it specifies. Reproduced on the real binary
/// here with a synthetic fixture in the same shape, AND against the governing spec file
/// itself (the adjudicator's own required fixture).
#[test]
fn validate_spec_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] a test proves the lint fires on a fixture containing an ownerless \
         criterion among three-plus. This criterion OWNS the pre-launch lint surface.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n";
    let path = root.join("affirmative-wins-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F1 ownership"),
        "criterion 1's own \"ownerless\" mention describes the FIXTURE the test builds, \
         not a self-referential denial - it must not veto criterion 1's real, separate \
         OWNS sentence on the real binary; stderr:\n{err}"
    );

    let governing_spec = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("66-ship-the-planning-discipline.md");
    let (out2, err2, ok2) = run_rigger(root, &["validate", governing_spec.to_str().unwrap()]);
    assert!(
        ok2,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err2}"
    );
    assert!(
        out2.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out2}"
    );
    assert!(
        !err2.contains("(criterion 3)"),
        "this unit's own governing spec must no longer self-trip its own lint on \
         criterion 3 (the adjudicator's own required fixture); stderr:\n{err2}"
    );
}

/// Round-4 REJECT remedy (b): `find_word`'s hyphen-as-word-forming rule (added to fix
/// "self-worth considering") also silenced "owner" inside a legitimate hyphenated
/// compound like "co-owner". Reproduced on the real binary here.
#[test]
fn validate_spec_recognizes_owner_inside_a_hyphenated_compound() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the daemon writes a pidfile. The co-owner of this criterion is the widget \
         team.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n";
    let path = root.join("co-owner-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F1 ownership"),
        "\"co-owner\" is a real ownership claim; the hyphen must not hide the standalone \
         \"owner\" inside it on the real binary; stderr:\n{err}"
    );
}

/// Round-5 residual gap: the fix commit's own periphery tests (the two above) both prove
/// the round-4 REJECT remedy through the "owns" half of `carries_owner_sentence`'s
/// `find_word_across_hyphen(&lower, "owns").is_some() || affirmative_owner_occurs(&lower)`,
/// which short-circuits before `affirmative_owner_occurs` ever runs. The remedy's own
/// doc comment and commit message both claim a standalone "owner" (not just "owns") wins
/// over an unrelated denial elsewhere in the block, but no test at any layer ever
/// constructs a block whose ONLY affirmative signal is a standalone "owner" word coexisting
/// with an unrelated "no owner" denial, the exact scenario `affirmative_owner_occurs` and
/// `denied_owner_positions` exist to arbitrate. Reproduced here on the real binary with no
/// "owns" word anywhere in the fixture, so the assertion can only pass if the
/// "owner"-specific position-exclusion logic itself is correct.
#[test]
fn validate_spec_lets_a_standalone_owner_win_over_an_unrelated_denial_elsewhere() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the daemon writes a pidfile; no owner is named for the legacy format \
         quirk, but the widget team is the owner of this criterion overall.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n";
    let path = root.join("standalone-owner-wins-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("(criterion 1)"),
        "criterion 1's unrelated \"no owner\" mention must not veto its own separate, \
         genuine standalone \"owner\" claim later in the same block - the block contains no \
         \"owns\" word at all, so this can only pass if the \"owner\"-specific \
         position-exclusion logic itself is correct on the real binary; stderr:\n{err}"
    );
}

/// Round-5 mutation-accounting closed a genuine coverage gap in `criterion_blocks`'s
/// line-join (`d-u66c3-r5-mutation-accounting`): deleting the `!` guard that skips a
/// leading space before the block's first line silently drops the JOINING space between
/// every later wrapped-continuation line instead, welding two independent words across a
/// line break into one - e.g. a checkbox whose first physical line ends "own" and whose
/// continuation line starts "er ..." welds into the exact five letters "owner", which
/// passes `find_word_across_hyphen`'s own boundary check on both sides. The implementer's
/// remedy (`ownership_check_does_not_let_a_dropped_word_boundary_weld_own_and_er_into_owner`)
/// pins this only at the private-helper unit layer; reproduced here through the real binary
/// so a regression in the joining space is caught at the observable boundary too, not only
/// inside `src/spec.rs mod tests`.
#[test]
fn validate_spec_does_not_weld_own_and_er_into_owner_across_a_wrapped_continuation_line() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Done when\n\n\
         - [ ] the widget locks down its own\n\
         \x20\x20er and simpler path through the config.\n\
         - [ ] the store passes the contract suite. This criterion OWNS the contract \
         coverage.\n\
         - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
         supersede path.\n";
    let path = root.join("own-er-weld-spec.md");
    std::fs::write(&path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F1 ownership") && err.contains("(criterion 1)"),
        "criterion 1 has no real OWNS/owner sentence - \"own\" and \"er\" sit on separate \
         physical lines and must NOT be welded into a false standalone \"owner\" match on \
         the real binary; stderr:\n{err}"
    );
}
