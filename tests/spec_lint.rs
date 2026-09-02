//! CLI periphery for spec 66, unit c3 (SPEC LINT): `rigger validate <spec>` on a fixture
//! carrying all four defect kinds this criterion names (a multi-behavior criterion, an
//! ownerless criterion among three-plus, a disposition smell outside Notes, and an em
//! dash) reports each with its criterion and field-guide class, exits 0, and reports a
//! clean fixture clean.
//!
//! What this file OWNS: the end-to-end `cmd_validate` wiring (the compiled binary's
//! stdout/stderr/exit code on a real spec file), driving the real binary so the observable
//! surface an operator sees is pinned exactly, PLUS the corpus-wide SELF-CLEAN regression
//! guard (`spec_lint_self_clean_over_the_committed_corpus`) the mid-run Design amendment
//! `d66-lint-heuristic-semantics` requires - that one test calls `rigger::spec` directly
//! (walking every committed `specs/*.md` through the real binary would be prohibitively
//! slow) rather than driving the compiled binary. NOT owned: the pure lint heuristics
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

/// Round-5 REJECT remedy (`adj-u66c3-r5-reject-selfclean-live-violation`,
/// `adv-u66c3-r5-f4-either-or-false-fires-on-a-decided-disposition-rule`): F4 fired 5 times
/// on specs/68's own "Disposition: satisfied either by fresh implementation or by
/// independently re-verifying already-integrated code..., evidence bar = ..." clause
/// (Global constraints plus all four Done-when criteria) - a decided evidence-acceptance
/// policy naming two concrete, already-accepted paths, not an open hedge. Reproduced
/// against the REAL committed governing spec file (the adversary's own required
/// reproduction, `./target/debug/rigger validate specs/68-ship-the-operating-discipline.md`)
/// so a regression in the decided-disposition exemption is caught at the observable
/// boundary, not only inside `src/spec.rs`'s own unit tests.
#[test]
fn validate_does_not_flag_specs_68_own_satisfied_either_or_disposition_clause() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec68 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("68-ship-the-operating-discipline.md");
    let (out, err, ok) = run_rigger(root, &["validate", spec68.to_str().unwrap()]);
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
        "specs/68's own \"satisfied either ... or ...\" decided-disposition clause (Global \
         constraints plus all four Done-when criteria) must no longer self-trip F4 on the \
         real binary; stderr:\n{err}"
    );
}

/// The decided-disposition exemption is scoped to the "satisfied either" idiom, not a
/// blanket F4 suppression: specs/57's genuine, unresolved hedge ("reindex either retires or
/// re-points to the symbol index, whichever the surviving command surface makes honest")
/// must still be flagged on the real binary - proving the fix closes the false-positive
/// side of the defect without silently reopening the false-negative side.
#[test]
fn validate_still_flags_specs_57_genuine_either_or_hedge() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec57 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("57-retire-turbovec.md");
    let (out, err, ok) = run_rigger(root, &["validate", spec57.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F4 disposition"),
        "specs/57's genuine unresolved either...or hedge must still be flagged on the real \
         binary - the decided-disposition exemption must not blanket-suppress F4; \
         stderr:\n{err}"
    );
}

/// Corpus-wide sweep residual: specs/68 criterion 1's "cannot bypass either surface" uses
/// "either" in its ordinary, non-disjunctive sense ("one of the two"), with a genuine
/// standalone "or" only much later, in an unrelated clause ("installs, replaces, or
/// modifies"). Before the either...or pairing was bounded to one grammatical clause, this
/// unrelated pair false-fired F4 on criterion 1 even after the "satisfied either" exemption
/// closed the other four hits on the same file. Reproduced on the real binary against the
/// full committed spec 68, including this exact criterion.
#[test]
fn validate_does_not_pair_a_non_disjunctive_either_with_a_faraway_or_on_specs_68() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec68 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("68-ship-the-operating-discipline.md");
    let (out, err, ok) = run_rigger(root, &["validate", spec68.to_str().unwrap()]);
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
        "criterion 1's non-disjunctive \"either surface\" must not pair with the faraway, \
         unrelated \"or\" in \"installs, replaces, or modifies\" on the real binary; \
         stderr:\n{err}"
    );
}

/// Round-6 sharpening (`specs/66-ship-the-planning-discipline.md`'s Design bullet,
/// "the acceptance property, made precise") replaced the original, machine-unjudgeable
/// "zero false findings over all historical specs" bar with two narrower, precise
/// properties: (1) SELF-CLEAN NARROW - spec 66 itself, alone, raises zero findings
/// (`spec_lint_self_clean_on_spec_66_itself`, below); (2) this test, the LABELED FIXTURE
/// CORPUS half applied at corpus scale - historical `specs/*.md` are explicitly NOT a
/// zero-findings corpus, so a finding there is "advisory output, not a test failure". This
/// test pins the human-vetted (`sdet-u66c3-r5-self-clean-not-proven-by-test`,
/// `adv-u66c3-r5-reject-selfclean-live-violation`) corpus-wide result as an executable
/// REGRESSION SNAPSHOT, not a zero-findings claim: hygiene is verifiably zero everywhere (a
/// real invariant - the diff gate forbids U+2014 anywhere, so no committed spec ever
/// carries one); F4 disposition fires on EXACTLY the reviewed set of historical hedges and
/// nowhere else; and F1 ownership / F2+F6 shape - independently cross-checked as legitimate
/// findings by sdet's and the adversary's round-5 manual sweeps - are pinned to their
/// current corpus-wide totals. ANY future drift (a new false positive, or a lint change
/// that silently drops a true one) fails this test and forces a conscious human review
/// before it can land, the same way the F4 defect should have been caught five rounds ago
/// instead of by hand.
#[test]
fn spec_lint_self_clean_over_the_committed_corpus() {
    let specs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("specs");
    let mut entries: Vec<_> = std::fs::read_dir(&specs_dir)
        .expect("read specs/ directory")
        .map(|e| e.expect("read specs/ dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    entries.sort();
    assert!(
        entries.len() >= 3,
        "sanity: the committed specs/ corpus must be non-trivial; got {} files",
        entries.len()
    );

    let mut f1_total = 0usize;
    let mut shape_total = 0usize;
    let mut hygiene_total = 0usize;
    let mut f4_hits: Vec<String> = Vec::new();

    for path in &entries {
        let text = std::fs::read_to_string(path).expect("read committed spec file");
        let advisories = rigger::spec::spec_lint_advisories(&text);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for a in &advisories {
            match a.class {
                "F1 ownership" => f1_total += 1,
                "F2 bundling" | "F6 copyability" => shape_total += 1,
                "hygiene" => hygiene_total += 1,
                "F4 disposition" => f4_hits.push(name.clone()),
                other => panic!("unknown lint class {other:?} on {name}; got: {a}"),
            }
        }
    }

    assert_eq!(
        hygiene_total, 0,
        "no committed spec may carry a U+2014 em dash - the diff gate forbids it, so this \
         must always be zero"
    );
    // A regression SNAPSHOT, not a zero-false-positive claim (the spec's own Design:
    // historical specs are advisory output, never a test failure). specs/57 is the one
    // known-GENUINE hedge ("either retires or re-points ... whichever the surviving
    // command surface makes honest" - an explicitly open question). specs/18 and 73
    // are decided ENUMERATIONS ("each fix either refuses ... or makes visible", "is
    // either KILLED ... or JUSTIFIED") that became visible when F4 gained the cross-line
    // paragraph join (adv-u66c3-r6-crossline-hedge-invisible-to-f4): mechanically
    // hedge-shaped, semantically decided - tolerated advisory noise on historical prose
    // by the Design's own rule. specs/74 DROPPED from this snapshot this round
    // (`impl-u66c3-r14-mask-to-last-occurrence`): its lone hedge-shaped phrase ("either
    // side is `+unversioned`" beside a faraway "or") sits between two independent
    // backtick-delimited code spans in the same paragraph (`` `rigger validate` `` earlier,
    // `` `+unversioned` `` right at the hedge itself); the round-14 mask-to-last-occurrence
    // closer fix (mandated by `adv-u66c3-r13-standing-remedy-direction-unsound` to close
    // the 8th recurrence of the quoted-text-can-never-false-positive class) now pairs the
    // FIRST backtick with the LAST remaining backtick in the paragraph, fusing those two
    // independent spans into one and masking the enclosed hedge along with them - an
    // accepted, deliberate RECALL loss (over-masking can only ever mask MORE, never
    // produce a false positive; the spec's own invariant is recall is expendable, a false
    // positive is not), not a heuristic regression. The snapshot keeps the net taut both
    // ways: a NEW name here is a false-positive regression to investigate, and 57
    // vanishing is a recall regression on the one KNOWN-genuine hedge - either way this
    // assertion fails loudly rather than drifting.
    assert_eq!(
        f4_hits,
        vec![
            "18-fail-fast-validation.md".to_string(),
            "57-retire-turbovec.md".to_string(),
            "73-mutation-testing-implementer-efficacy.md".to_string(),
        ],
        "F4's committed-corpus fire set must match the reviewed snapshot (57 genuine; \
         18/73 decided-enumeration advisory noise per the Design's \
         historical-specs-are-advisory rule; 74 dropped this round by the accepted \
         mask-to-last-occurrence over-masking trade-off, see comment above); a new name is \
         a false-positive regression, a missing one (other than 74, already accounted for) \
         a recall regression; got: {f4_hits:?}"
    );
    assert_eq!(
        f1_total, 186,
        "F1 ownership's corpus-wide total is pinned to sdet's round-5 independently \
         cross-checked count (194), minus the 3 hits removed by giving specs/66's own \
         criteria 4/5/6 an OWNS sentence (`u66c3-self-clean-ownership-gap-fix`, required by \
         the SELF-CLEAN NARROW property below), minus a further 6 hits removed by giving \
         specs/61's c2/c3/c4/c5/c6/c8 an OWNS/exclusion sentence \
         (`plan61r2-clusterAB-ownership-text-actually-landed`), plus 1 hit added by \
         specs/79-reap-before-removal.md landing in the corpus \
         (`impl-u62c3-pin-f1-total-186-specs79-landed`): its criterion 3 is the same ordinary, \
         unnumbered \"both feature lanes green\" closing checkbox every other spec in this \
         corpus ends with (no OWNS sentence anywhere else in the corpus carries one on that \
         checkbox either - it is not a claimable concern a neighbor could contest), reviewed \
         directly against the advisory text, not a heuristic regression; a changed total means \
         either a real spec edit (update this pin after reviewing the new/removed hits) \
         or a regression in the heuristic"
    );
    assert_eq!(
        shape_total, 74,
        "F2 bundling / F6 copyability's corpus-wide total is pinned as a regression guard \
         (spot-checked legitimate by the round-5 adversary sweep); a changed total means \
         either a real spec edit (update this pin after reviewing the new/removed hits) or \
         a regression in the heuristic"
    );
}

/// SELF-CLEAN NARROW (`specs/66-ship-the-planning-discipline.md`'s Design bullet, round-6
/// sharpening): "spec 66 itself raises zero lint findings at the unit's HEAD" - the
/// half of the acceptance property machine-judgeable with no human oracle, since spec 66
/// is the ONE spec this lint's own authors control end to end (unlike the historical
/// corpus above, which may carry TRUE smells this lint is right to surface). Reading the
/// real committed file (not a fixture standing in for it) so an edit to spec 66 that
/// reintroduces a self-trip - an unquoted draft-smell phrase, a criterion that loses its
/// OWNS sentence, an em dash - fails this test immediately rather than waiting for a
/// human to run `rigger validate` by hand the way five straight review rounds had to
/// (`adv-u66c3-disposition-lint-self-trips-on-its-own-governing-spec`,
/// `adv-u66c3-r5-reject-selfclean-live-violation`).
#[test]
fn spec_lint_self_clean_on_spec_66_itself() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("66-ship-the-planning-discipline.md");
    let text = std::fs::read_to_string(&path).expect("read specs/66 itself");
    let advisories = rigger::spec::spec_lint_advisories(&text);
    assert!(
        advisories.is_empty(),
        "specs/66-ship-the-planning-discipline.md must raise zero lint findings against its \
         own lint (SELF-CLEAN NARROW); got: {advisories:?}"
    );
}

/// Round-6 sdet-author periphery gap (`d-u66c3-r6-periphery-accounting`): the round-6 fix's
/// own three CLI tests reproduce the committed corpus's exact false-fires
/// (specs/68's "satisfied either", specs/57's bare hedge, specs/68 criterion 1's
/// clause-bounded non-pairing), but `is_decided_disposition`'s negation guard - a
/// standalone "unsatisfied" immediately before "either" is a DIFFERENT word from
/// "satisfied" and must NOT be exempted - has no committed-corpus occurrence, so it was
/// proven only inside `src/spec.rs`'s own unit tests
/// (`disposition_check_does_not_exempt_unsatisfied_either_or`), never through the compiled
/// binary. Closes that gap with a synthetic fixture, matching this suite's established
/// precedent of driving the real binary against text that mirrors a corpus-absent branch
/// (see `validate_spec_attributes_a_prose_level_defect_to_no_criterion`).
#[test]
fn validate_still_flags_an_unsatisfied_either_or_as_an_open_hedge() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         the recovery criterion remains unsatisfied either by an automatic retry or by a \
         manual escalation, undecided.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F4 disposition"),
        "\"unsatisfied either ... or\" is a different word from \"satisfied either ... or\" \
         - the decided-disposition exemption must not swallow it, and it must still be \
         flagged as an open hedge on the real binary; stderr:\n{err}"
    );
}

/// Round-6 sdet-author periphery gap (`d-u66c3-r6-periphery-accounting`): `either_or_hedge`
/// now scans every standalone "either" on a line rather than stopping at the first, so a
/// non-disjunctive "either" (no "or" in its own clause) cannot shadow a genuine disjunction
/// later on the SAME line. The round-6 fix's CLI test
/// (`validate_does_not_pair_a_non_disjunctive_either_with_a_faraway_or_on_specs_68`) proves
/// only the negative half, that a faraway "or" in a LATER, unrelated clause must not
/// false-fire, reproducing specs/68 criterion 1 verbatim. The positive half, a genuine
/// hedge that DOES follow an earlier non-disjunctive "either" on the same line, has no
/// committed-corpus occurrence (proven only by
/// `disposition_check_finds_a_genuine_hedge_after_an_earlier_non_disjunctive_either`'s unit
/// fixture), so a regression that stopped the scan at the first "either" again (silencing a
/// real hedge that happens to follow a non-disjunctive one) would pass the round-6 CLI suite
/// unnoticed. Closed with a synthetic fixture combining both halves on one line.
#[test]
fn validate_still_flags_a_genuine_hedge_after_an_earlier_non_disjunctive_either_on_specs_68_shape()
{
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         an entry cannot bypass either surface; either the daemon retries or it escalates, \
         undecided.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-lint advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F4 disposition"),
        "a genuine hedge (\"either the daemon retries or it escalates\") following an \
         earlier non-disjunctive \"either\" (\"either surface\") on the same line must still \
         be flagged on the real binary - the scan must not stop at the first \"either\"; \
         stderr:\n{err}"
    );
}

/// Round-6 upheld finding sdet-u66c3-r6-decided-disposition-comma-boundary-still-false-fires
/// (the escalation remedy): the decided-disposition idiom survives intervening punctuation -
/// "satisfied, either by X or by Y" is the same decided shape as "satisfied either by X or
/// by Y", so the exemption is token-based (nearest WORD before "either"), never
/// adjacent-characters-based.
#[test]
fn validate_exempts_the_comma_separated_decided_disposition() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         each criterion may be satisfied, either by fresh implementation or by \
         re-verifying already-integrated code.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        !err.contains("F4 disposition"),
        "\"satisfied, either ... or\" is the decided-disposition idiom with a comma - the \
         exemption must survive intervening punctuation; stderr:\n{err}"
    );
}

/// Round-6 upheld finding sdet-u66c3-r6-spaced-negation-wrongly-exempts-a-real-open-hedge
/// (the escalation remedy): an explicitly NEGATED satisfaction ("not yet satisfied
/// either ... or ...") is an open question, the opposite of a decided disposition - a
/// negator within the two tokens before "satisfied" cancels the exemption.
#[test]
fn validate_still_flags_a_spaced_negation_before_the_decided_idiom() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         the recovery criterion is not yet satisfied either by an automatic retry or by a \
         manual escalation.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        err.contains("F4 disposition"),
        "\"not yet satisfied either ... or\" is an explicitly negated satisfaction - an \
         open hedge the exemption must not swallow; stderr:\n{err}"
    );
}

/// Round-6 upheld finding adv-u66c3-r6-crossline-hedge-invisible-to-f4 (the escalation
/// remedy): this repo hard-wraps prose, so a hedge split across a wrap is one sentence to
/// a reader and must be one haystack to the lint - F4 scans logical paragraphs (joined
/// continuation lines), not physical lines.
#[test]
fn validate_flags_a_hedge_split_across_hard_wrapped_lines() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         on a failure the daemon either retries the whole request with a fresh\n\
         connection or escalates to the operator, undecided which.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        err.contains("F4 disposition"),
        "a hedge split across a hard wrap (\"either ...\\n... or ...\") is one sentence \
         and must still be seen by F4's paragraph join; stderr:\n{err}"
    );
}

/// Round-8 mutation-testing gap (sdet periphery pass): `starts_new_element`'s heading arm
/// (a line starting with `#`) is what stops the paragraph joiner at a structural boundary.
/// This spec puts an "either" clause immediately before a heading and its would-be "or"
/// immediately after, with NO blank line separating either from the heading: two
/// standalone, unrelated fragments that must never be read as one hedge. If the heading
/// were not recognized as a new element, the joiner would fuse both fragments into a
/// single haystack and misfire F4 across the boundary; because it correctly stops the
/// paragraph at the heading, "either the automatic retry" alone pairs with no "or" and the
/// heading-plus-tail paragraph alone contains no "either", so F4 stays silent on both.
#[test]
fn validate_does_not_fuse_a_hedge_across_a_heading_boundary() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         the recovery approach is either the automatic retry\n\
         ## Constraints\n\
         or the manual escalation, undecided which path applies.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        !err.contains("F4 disposition"),
        "a heading between an \"either\" fragment and an unrelated \"or\" fragment must \
         stop the paragraph join - the two must never read as one fused hedge; \
         stderr:\n{err}"
    );
}

/// Round-8 mutation-testing gap (sdet periphery pass), the table-row twin of the heading
/// case above: `starts_new_element`'s table-row arm (a line starting with `|`) must also
/// stop the paragraph joiner, so an "either" fragment before a table row and an unrelated
/// "or" fragment after it are never fused into one hedge.
#[test]
fn validate_does_not_fuse_a_hedge_across_a_table_row_boundary() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         the recovery approach is either the automatic retry\n\
         | before | after |\n\
         or the manual escalation, undecided which path applies.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        !err.contains("F4 disposition"),
        "a table row between an \"either\" fragment and an unrelated \"or\" fragment must \
         stop the paragraph join - the two must never read as one fused hedge; \
         stderr:\n{err}"
    );
}

/// Round-10 fix (`impl-u66c3-r10-cross-paragraph-mask-fix`) for the round-9 REJECT
/// (`adj-u66c3-run3-reject-crossline-quote-falsifies-design-claim`,
/// `adv-u66c3-r9-strip-inline-code-resets-per-line-crossline-quote-false-fires`): the
/// paragraph joiner used to mask inline-code/quoted spans PER PHYSICAL LINE before joining,
/// so a double-quoted span whose closing `"` fell on a hard-wrapped continuation line lost
/// its exemption partway through and the smell phrase inside it (here, "could instead")
/// false-fired F4. The fix joins the paragraph's raw lines first and masks once over the
/// whole joined string, so the open-quote state carries across the line boundary. Drives
/// the real binary (the implementer's own regression test in `src/spec.rs` only proves the
/// library function directly - the adjudicator's remedy asked for this proof "alongside the
/// existing spec_lint.rs boundary-gap tests").
#[test]
fn validate_ignores_a_double_quoted_span_that_crosses_a_hard_wrapped_line() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The plan states: \"we\n\
         could instead retry\" as the documented phrasing.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a double-quoted span split across a hard-wrapped line must stay exempt for its \
         whole span, including the continuation line's content up to the close - the \
         quote-open state must carry across the line boundary, not reset at it; \
         stderr:\n{err}"
    );
}

/// The backtick twin of the double-quote case above: `strip_inline_code` shares ONE
/// open-delimiter toggle between `` ` `` and `"` (keyed on whichever delimiter opened the
/// current span), so the round-10 fix that carries quote state across a hard-wrapped line
/// carries backtick state the same way - but had no coverage at any layer (unit or
/// periphery) proving it, since the implementer's own regression test exercises only the
/// double-quote fixture. Proves the fix is not an accident of the one fixture it was
/// written against.
#[test]
fn validate_ignores_a_backtick_span_that_crosses_a_hard_wrapped_line() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The plan states: `we\n\
         could instead retry` as the documented phrasing.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a backtick-delimited span split across a hard-wrapped line must stay exempt for \
         its whole span the same as a double-quoted span does - both share one \
         open-delimiter toggle in strip_inline_code; stderr:\n{err}"
    );
}

/// The EVEN-count (balanced) recall arm at the CLI seam (specs/66 Design,
/// `d66-mask-one-span-per-kind`): the periphery twin of `disposition_check_still_fires_
/// outside_a_balanced_quote_pair` (`src/spec.rs`) - an sdet-author gap closed this round.
/// The two fail-closed tests below this one both cite "the balanced-pair test above" as
/// proof that recall survives wherever the invariant permits it, but no such test existed
/// anywhere in this file (confirmed: neither this commit nor the archived, previously
/// rebuilt tip `archive/u66c3-escalation-remedy-c533796` ever carried a CLI-level
/// balanced-pair-recall test, despite the unit-level test existing since round 14) - a
/// dangling reference, not a covered boundary. With an EVEN double-quote count the mask
/// covers first-through-last mark only, so a genuine, unquoted F4 smell OUTSIDE the pair
/// must still fire on the real binary, not just in the library call `disposition_check_
/// still_fires_outside_a_balanced_quote_pair` already proves directly.
#[test]
fn validate_still_flags_a_smell_outside_a_balanced_quote_pair() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The report labels this \"a tolerance issue\" in passing, but the team\n\
         could instead retry the whole approach if this keeps recurring.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F4 disposition"),
        "an unquoted smell after a balanced double-quote pair must still fire on the real \
         binary - recall survives wherever the invariant permits it; stderr:\n{err}"
    );
}

/// The backtick twin of the test above, for the same reason
/// `validate_ignores_a_backtick_span_that_crosses_a_hard_wrapped_line` exists beside its
/// double-quote sibling: `strip_inline_code` runs the identical even/odd rule per kind
/// (`for kind in ['`', '"']`), so proving balanced-pair recall for `"` alone would leave the
/// backtick arm an accident of the one fixture it was never written against.
#[test]
fn validate_still_flags_a_smell_outside_a_balanced_backtick_pair() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The report labels this `a tolerance issue` in passing, but the team\n\
         could instead retry the whole approach if this keeps recurring.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("F4 disposition"),
        "an unquoted smell after a balanced backtick pair must still fire on the real \
         binary, independently of the quote kind; stderr:\n{err}"
    );
}

/// The ODD-count fail-closed arm at the CLI seam (specs/66 Design,
/// `d66-mask-one-span-per-kind`, SUPERSEDING round-10's closed-span-only disposition):
/// a stray unmatched delimiter makes the paragraph's quote state unknowable, and the
/// invariant (quoted text can NEVER false-positive) outranks advisory recall, so the
/// masker blanks from the first mark to the paragraph end and the later unquoted smell
/// is deliberately NOT reported. The balanced-pair test above proves recall survives
/// wherever the invariant permits it.
#[test]
fn validate_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The gap measured 6\" today, well within tolerance for the current\n\
         build, and unrelated to the next point entirely, but the team\n\
         could instead retry the whole approach if this keeps recurring.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "an odd quote count fails closed: the masker blanks from the first mark to the \
         paragraph end, so the later smell is deliberately unreported - the invariant \
         outranks recall; stderr:\n{err}"
    );
}

/// The backtick twin of the fail-closed case above: each delimiter kind computes its own
/// span independently under the one-span-per-kind rule, so an odd backtick count fails
/// closed exactly as an odd quote count does - proven at the CLI seam so the rule is not
/// an accident of the one double-quote fixture it was written against.
#[test]
fn validate_fails_closed_after_a_stray_unmatched_backtick_earlier_in_the_paragraph() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The gap measured `6 today, well within tolerance for the current\n\
         build, and unrelated to the next point entirely, but the team\n\
         could instead retry the whole approach if this keeps recurring.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "an odd backtick count fails closed to the paragraph end, independently of the \
         quote kind - the invariant outranks recall; stderr:\n{err}"
    );
}

/// Round-11 REJECT remedy (`adj-u66c3-r11-reject-stray-mark-steals-real-quote`,
/// `sdet-u66c3-r11-stray-mark-unmasks-a-later-real-quote`): the MIRROR of the two
/// stray-mark-still-fires tests above. Those prove a real, UNQUOTED smell phrase still
/// fires after an earlier stray mark; this proves a real, GENUINELY QUOTED smell phrase
/// still stays EXEMPT after that same earlier stray mark - the forward-greedy pairing
/// `strip_inline_code` used before this round's fix had no notion of which quote in the
/// paragraph a given mark was meant to close, so the earlier stray inches-mark quote
/// would consume the later real span's own opening delimiter as its "close", leaving the
/// real quoted hedge phrase unmasked and F4 false-firing on it - the exact
/// quoted-or-named-text-can-never-false-positive intent (specs/66 Design;
/// `disposition_advisories`'s own doc comment) this unit was rejected for at rounds 4, 5,
/// 6, 9, 10, and 11. Drives the real compiled binary, same fixture shape as the
/// adjudicator's own round-11 reproduction probe.
#[test]
fn validate_a_stray_unmatched_quote_does_not_unmask_a_later_real_quoted_disposition_phrase() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The gap measured 6\" today, well within tolerance for the current\n\
         build, and the plan states \"we could instead retry\" as the\n\
         documented phrasing, unrelated to the rest of this paragraph.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a real double-quoted \"could instead\" phrase must stay exempt even after an \
         earlier stray unmatched quote mark in the same paragraph - only the stray mark is \
         spurious, the later span is genuinely quoted; stderr:\n{err}"
    );
}

/// Round-12 fix (`impl-u66c3-r12-candidate-delimiter-exclusion-fix`) excludes a `"`
/// immediately preceded by a digit from delimiter candidacy, but DELIBERATELY scopes the
/// exclusion to `"` only - `is_candidate` (`src/spec.rs`) guards it with `ch == '"'`, so a
/// backtick keeps its old unconditional candidacy regardless of what precedes it. The
/// commit's own stated reason is that this repo's corpus routinely closes real inline-code
/// spans immediately after a digit (an IP address, a version number), so a digit-adjacent
/// CLOSING backtick must keep pairing. Nothing at any layer proved that: the implementer's
/// own round-12 tests (`disposition_check_a_stray_unmatched_quote_does_not_unmask_a_later_
/// real_quoted_phrase`, `validate_a_stray_unmatched_quote_does_not_unmask_a_later_real_
/// quoted_disposition_phrase`, this file above) exercise only the `"` fixture, so a future
/// slip that widened the `ch == '"'` guard to cover both delimiters (e.g. dropping it, or
/// copying the digit check onto the shared `is_candidate` prefix) would silently break
/// backtick-masked code spans and reopen the same quoted-or-named-text-can-never-
/// false-positive class this unit has been REJECTed for six times (rounds 4, 5, 6, 9, 10,
/// 11) - just for the sibling delimiter. Drives the real compiled binary; the fixture's
/// closing backtick sits immediately after `127`, a digit, with no separating whitespace,
/// the same shape the commit message names.
#[test]
fn validate_ignores_a_backtick_span_whose_closing_mark_is_immediately_after_a_digit() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The plan states `we could instead retry 127` as the documented approach.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a real backtick-delimited \"could instead\" phrase must stay exempt even though \
         its closing backtick sits immediately after a digit with no separating whitespace \
         - the round-12 digit-adjacency exclusion is scoped to double quotes only, a \
         backtick must keep pairing regardless of what precedes it; stderr:\n{err}"
    );
}

/// Round-13 REJECT remedy (`adj-u66c3-r13-role-based-digit-adjacency`,
/// `sdet-u66c3-r12-closing-quote-digit-adjacency-false-positive`,
/// `adv-u66c3-r12-confirmed-closing-quote-digit-adjacency-live-repro`): the mirror defect
/// the round-12 opener-only fix left open on the CLOSER side. `is_candidate` (renamed
/// `is_opener_candidate` this round, `src/spec.rs`) gated BOTH ends of the forward search
/// with the same digit-adjacency check, so a genuinely quoted span whose own closing `"`
/// happened to sit immediately after a digit could never close - the opener was left
/// unmatched-to-end and, per this unit's closed-span-only masking design, left completely
/// unmasked, so F4 false-fired on content that is genuinely double-quoted in the source.
/// Drives the real compiled binary; same fixture shape as the adjudicator's own round-12
/// live reproduction probe (`"...retry 10"`, closing quote immediately after the digit
/// `10`). Paired with the opener-exclusion test immediately below so both directions of
/// the role-based predicate are proven together in this round's diff, per the
/// adjudicator's explicit instruction not to fix and ship one side at a time again.
#[test]
fn validate_ignores_a_quoted_span_whose_closing_quote_is_immediately_after_a_digit() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The plan states \"we could instead retry 10\" as documented, unrelated\n\
         to the rest of this paragraph.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a real double-quoted \"could instead\" phrase must stay exempt even when its own \
         closing quote sits immediately after a digit with no separating whitespace - the \
         digit-adjacency exclusion is opener-only, a closer must keep pairing regardless \
         of what precedes it; stderr:\n{err}"
    );
}

/// Round-13 remedy (`adj-u66c3-r13-role-based-digit-adjacency`): the OPENER direction,
/// paired with the closer-direction test immediately above through the real compiled
/// binary. A `"` immediately after a digit must stay excluded from OPENER candidacy -
/// unchanged from round-12 - so a stray inches-mark quote can never itself start a span
/// and thereby steal a later real span's own opening delimiter. Scanned together with the
/// test above so this round's diff proves the new role-based predicate holds in both
/// directions at once, not just the direction this round happened to fix.
#[test]
fn validate_a_digit_adjacent_quote_stays_excluded_as_an_opener() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The gap measured 6\" today, and the plan states \"we could instead\n\
         retry\" as documented, unrelated to the rest of this paragraph.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a stray digit-adjacent quote must stay excluded as an opener - if it wrongly \
         opened, it would forward-pair with the real span's own opening quote as its \
         \"close\", leaving the real quoted hedge phrase unmasked; stderr:\n{err}"
    );
}

/// Mutation-efficacy gap (round-13 accounting, `mutants.out/outcomes.json`): a mutant
/// replacing the `i > 0` bounds guard in `is_opener_candidate` (`src/spec.rs`) with
/// `i >= 0` survived the suite untouched by every test above - `i >= 0` is vacuously true
/// for a `usize`, so the mutant only diverges from the real guard when `i == 0` AND the
/// character there is a `"`, a case no prior fixture in this file (or `src/spec.rs`'s own
/// `mod tests`, before this round) exercised: every quote in every existing fixture is
/// preceded by at least one other character. Drives the real compiled binary with a
/// paragraph whose very first character is a genuine opening `"` - proving `validate`
/// treats it as a valid opener rather than panicking on an out-of-bounds look-back one
/// character before the paragraph starts. Same fixture shape as the colocated unit test
/// `disposition_check_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener`
/// (`src/spec.rs`), which independently proves the RED/GREEN pair against the mutant
/// itself (temporarily applied, reverted).
#[test]
fn validate_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         \"we could instead retry\" is the documented approach, unrelated to \
         anything else.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a paragraph that opens with a genuine double quote as its very first character \
         must treat that quote as a valid opener, not panic on an out-of-bounds look-back; \
         stderr:\n{err}"
    );
}

/// Round-13 REGRESSION (`sdet-u66c3-r13-embedded-digit-mark-premature-close`), the mirror
/// cost of this same round's own closer-matches-on-delimiter-alone fix (the diff directly
/// above this test in `src/spec.rs`). Round-12 excluded a digit-adjacent `"` from
/// candidacy on BOTH ends, so the forward closer search SKIPPED PAST a spurious
/// digit-adjacent mark and kept looking for the next candidate - which incidentally made
/// an embedded units mark inside a still-open real span transparent to the scan (verified
/// by re-running this exact fixture's spec text against the round-12 binary, `03ac0a8`:
/// zero F4 hits there). Round-13's closer search now matches on the delimiter character
/// ALONE with no digit-adjacency check, so a digit-adjacent mark sitting INSIDE a
/// genuinely quoted span - a realistic technical-prose shape, e.g. a quoted passage that
/// itself uses a `6"` inches-style unit notation before its own real closing quote - now
/// closes the span too early, leaving everything after it (up to the true closing quote)
/// unmasked. The genuinely double-quoted "could instead" phrase in this fixture sits in
/// that unmasked tail and false-fires F4 - the SAME documented-intent violation ("quoted
/// or named text can never false-positive", `src/spec.rs` doc comment above
/// `disposition_advisories`) this unit has been REJECTed for at rounds 4, 5, 6, 9, 10, 11
/// and 12 - an eighth instance, this time reintroduced by the very fix that closed the
/// seventh. Per this unit's own author charter, a boundary bug drives remediation of the
/// CODE, never a weakened test: this assertion states the CORRECT behavior. Round-14 fixes
/// it by pairing the opener with the LAST remaining occurrence of the delimiter in the
/// paragraph, not the nearest non-digit-adjacent one - the adjudicator proved the tempting
/// "prefer nearest non-digit-adjacent" remedy unsound on a further shape (`adj-u66c3-r13-
/// standing-remedy-direction-unsound`) - see `strip_inline_code`'s doc comment in
/// `src/spec.rs` and `impl-u66c3-r14-mask-to-last-occurrence`.
#[test]
fn validate_an_embedded_digit_adjacent_mark_does_not_prematurely_close_a_real_quoted_span() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The report states \"we measured a 6\" gap between the brackets and could\n\
         instead retry the weld\" as documented, unrelated to the rest of this\n\
         paragraph.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a real double-quoted phrase must stay exempt even when the same quoted span \
         contains an embedded digit-adjacent mark (a units notation like 6\") before its \
         own true closing quote - the closer search must not stop at the embedded mark \
         when a later, non-digit-adjacent closer exists for the same open span; \
         stderr:\n{err}"
    );
}

/// COMBINATORIAL FIXTURE (round-14 remedy, `adj-u66c3-r13-standing-remedy-direction-
/// unsound`): the quoted-or-named-text-can-never-false-positive class has now been
/// REJECTed 8 times (rounds 4, 5, 6, 9, 10, 11, 12, 13), each round's fix proven only
/// against the ONE shape a lens happened to construct that round, leaving the next shape
/// free to reopen the class again next round. Rather than a 9th single-shape fixture, this
/// test drives all four known digit-adjacency trigger shapes at once, through the real
/// compiled binary, in one Design section: each is its own bullet (`starts_new_element`
/// makes every `- ` line its own paragraph, so the four shapes cannot mask into or shield
/// each other - each stands or falls on its own):
/// - Bullet 1, mark-before-open: a stray digit-adjacent `"` (`6" today`) precedes a real
///   quoted span (round-11 shape, `adv-u66c3-r11-reject-stray-mark-steals-real-quote`).
/// - Bullet 2, mark-as-true-closer: a real quoted span's own closing `"` sits immediately
///   after a digit (`retry 10"`, round-12 shape,
///   `adv-u66c3-r12-confirmed-closing-quote-digit-adjacency-live-repro`).
/// - Bullet 3, mark-embedded-inside-open-span: a digit-adjacent `"` (`a 6" gap`) sits
///   INSIDE an already-open real span, before its own true close (round-13 shape,
///   `sdet-u66c3-r13-review-both-lanes-red-embedded-digit-mark`, the test immediately
///   above).
/// - Bullet 4, two-independent-spans-first-closer-digit-adjacent: two separate genuine
///   quoted spans in one paragraph, where the FIRST span's own true closer is
///   digit-adjacent (`defaults to 10"` immediately followed by `"worth considering"`) -
///   the shape that refuted the tempting "prefer nearest non-digit-adjacent closer" remedy
///   (`adv-u66c3-r13-standing-remedy-direction-unsound`'s own counter-example probe,
///   reproduced verbatim here). Mask-to-last-occurrence deliberately FUSES this bullet's
///   two spans into one (an accepted over-masking recall loss, never a false positive -
///   see `strip_inline_code`'s doc comment in `src/spec.rs`), so `"worth considering"`
///   ends up masked too, alongside the text between the two spans.
///
/// None of the four bullets' genuinely-quoted disposition-smell phrases may fire F4; a
/// single assertion over the whole `validate` run proves all four survive together, not
/// just individually.
#[test]
fn validate_all_four_digit_adjacency_shapes_together_never_false_positive() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         - The gap measured 6\" today, and the plan states \"we could instead retry\" \
         as documented.\n\
         - The plan states \"we could instead retry 10\" as documented, unrelated to \
         anything else.\n\
         - The report states \"we measured a 6\" gap between the brackets and could \
         instead retry the weld\" as documented.\n\
         - The report states \"the field defaults to 10\" and separately \"worth \
         considering\" is deferred.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "all four digit-adjacency trigger shapes (mark-before-open, mark-as-true-closer, \
         mark-embedded-inside-open-span, two-independent-spans-first-closer-digit-adjacent) \
         must stay exempt SIMULTANEOUSLY - a real quoted disposition-smell phrase can \
         never false-positive regardless of which digit-adjacency shape surrounds it; \
         stderr:\n{err}"
    );
}

/// NEW GAP FOUND THIS ROUND (`sdet-u66c3-r14-digit-glued-real-opener-excluded`), pre-existing
/// since round 12's digit-exclusion fix (`impl-u66c3-r12-candidate-delimiter-exclusion-fix`)
/// and never independently found across rounds 12, 13, or 14: `is_opener_candidate` excludes
/// ANY double quote immediately preceded by a digit with no separating whitespace from
/// opener candidacy, on the theory that such a mark is always a spurious units notation
/// (`6"`) rather than a real quotation delimiter. That theory does not hold when the digit
/// happens to sit directly against the quote that IS a real quotation's own opening mark (no
/// space between a preceding number and the quote): the exclusion still fires, so the scan
/// never treats that position as an opener. The scan then reaches the span's own true
/// CLOSING quote (not digit-adjacent, so it passes `is_opener_candidate`) and misreads IT as
/// a fresh opener instead; since no further same-char occurrence remains in the paragraph
/// for it to pair with, `close` is `None` and nothing is masked at all - the entire quoted
/// phrase, opener through closer, is left as ordinary visible prose. Reproduced live on BOTH
/// the round-13 (`cca7e3b`) and round-14 (`cceb93a`) compiled binaries with the identical
/// fixture below, confirming this predates round-14's mask-to-last-occurrence change (which
/// only alters closer SELECTION among multiple already-candidate occurrences and cannot
/// affect a case where the true opener was never recognized as a candidate in the first
/// place) and has been latent since round 12. This is the SAME quoted-or-named-text-can-
/// never-false-positive class this unit has been REJECTed for at rounds 4, 5, 6, 9, 10, 11,
/// 12, and 13 - a 9th instance, via a mechanism ("the exclusion eats the real opener itself")
/// none of those eight prior rounds' fixtures exercised (all eight glued the digit to a
/// SPURIOUS mark, never to the real span's own genuine opening delimiter). Per this unit's
/// author charter, captured as a FAILING periphery test rather than a weakened assertion or
/// a silent exemption, since it lands squarely on the boundary surface this round's
/// accounting is required to cover.
#[test]
fn validate_a_digit_glued_to_a_quotes_own_opening_mark_still_masks_the_real_span() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let spec = "# Widget\n\n## Design\n\n\
         The report cites section 5\"we could instead retry the weld\" per the review, \
         unrelated to anything else.\n\n\
         ## Done when\n\n- [ ] the daemon retries on failure. This criterion OWNS retry.\n";
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", spec_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("F4 disposition"),
        "a real double-quoted phrase must stay exempt even when a digit sits directly \
         against its own OPENING quote with no separating whitespace - the digit-adjacency \
         exclusion must not eat the genuine opener itself and leave the whole span unmasked; \
         stderr:\n{err}"
    );
}
