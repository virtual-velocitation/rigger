//! PERIPHERY (CLI) test for spec 68, criterion 3 - the bare-menu branch's OWN identity-migration
//! call.
//!
//! `cmd_reset`'s flagless branch runs the one-time spec-09 identity migration itself
//! (`if selection.is_sqlite() { migrate_identity_at(&loc)?; }`) BEFORE building the menu - a
//! SEPARATE occurrence of that call from the one the flagged path already runs, which
//! `tests/reset_derived_compaction.rs`'s
//! `reset_derived_compacts_a_log_whose_history_predates_the_minted_project_identity` already
//! proves migrates a legacy-identity store correctly for `--derived`. No existing test drives the
//! BARE path against a store whose history predates the minted project identity, so a bug that
//! dropped, reordered, or mis-scoped the bare branch's own call would read as: the menu silently
//! reports "0 dead-run node(s)" / "0 redundant derived-index event(s)" on a store that is, in fact, full of
//! both - a perfectly successful preview of nothing, the exact silent-lie this whole feature
//! exists to prevent, and a shape a fixture that always seeds AFTER `rigger init` can never
//! reproduce.

mod common;

use common::cli::emit;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_stream_identity;
use common::cli::seed_derived_duplicates;
use common::cli::seed_run_events;
use common::cli::temp_store_project;
use common::cli::DUP_ROUNDS;

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/reset_menu.rs and tests/reset_derived_compaction.rs; each integration
// suite is its own binary, so a small harness is duplicated per file by this codebase's existing
// convention rather than shared).
// ---------------------------------------------------------------------------------------

#[test]
fn bare_reset_previews_the_migrated_stores_real_counts_when_history_predates_the_minted_project_identity(
) {
    let dir = temp_store_project();
    let root = dir.path();

    // Seeded BEFORE `rigger init` mints an identity: filed under the LEGACY basename namespace,
    // exactly the shape a bloated store actually has (mirrors
    // reset_derived_compaction.rs::reset_derived_compacts_a_log_whose_history_predates_the_minted_project_identity,
    // now for the bare-menu's own migration call rather than the flagged path's).
    seed_run_events(root, &[("RunStarted", r#"{"run":"r1","criteria":["c"]}"#)]);
    emit(
        root,
        "DecisionMade",
        r#"{"id":"dead-d","summary":"dead","governs":["f.rs"]}"#,
    );
    // A second run starts, ALSO seeded under the (still unminted) legacy identity, so r1 is
    // provably dead (superseded by r2) before the migration ever runs. Seeding it here rather
    // than after `rigger init` keeps the WHOLE history under one namespace at mint time - the
    // migration is a one-time move of everything under the legacy prefix, not a merge of two
    // namespaces that each already hold data (a store that legitimately holds both is the
    // separate, deliberately-refused "ambiguous identity" case, not this one).
    seed_run_events(root, &[("RunStarted", r#"{"run":"r2","criteria":["c"]}"#)]);
    seed_derived_duplicates(root);
    let legacy = run_stream_identity(root);

    let (_, ierr, iok) = run_rigger(root, &["init"]);
    assert!(iok, "rigger init must scaffold the project; stderr: {ierr}");
    let minted = run_stream_identity(root);
    assert_ne!(
        minted, legacy,
        "rigger init must mint an identity distinct from the basename, or this fixture does not \
         reproduce the shape it exists for"
    );

    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(
        ok,
        "a bare `rigger reset` must exit 0 even on a legacy-identity store; stderr: {err}"
    );

    // THE ASSERTION THIS TEST EXISTS FOR: a migration bug on the bare-menu's own call would leave
    // the menu reading under the minted identity while the seeded history still sits under the
    // legacy one, and it would report ZERO on both lines - a perfectly successful preview of
    // nothing, on a store that is not empty. Both counts must be the real, non-zero, migrated ones.
    assert!(
        out.contains("--runs: 1 dead-run node(s)"),
        "the bare menu must report the migrated store's real dead-run node count (1), not zero \
         from an unmigrated identity mismatch; got: {out:?}"
    );
    assert!(
        out.contains(&format!(
            "--derived: {} redundant derived-index event(s)",
            DUP_ROUNDS - 1
        )),
        "the bare menu must report the migrated store's real duplicate count ({}), not zero from \
         an unmigrated identity mismatch; got: {out:?}",
        DUP_ROUNDS - 1
    );

    // And the migration is real, not a duplication: every stream now lives under the MINTED
    // prefix, none under the legacy one.
    let minted_prefix = format!("proj-{minted}-");
    let legacy_prefix = format!("proj-{legacy}-");
    let conn = rusqlite::Connection::open(rigger_file(root, "events.db")).unwrap();
    let mut stmt = conn.prepare("SELECT DISTINCT stream FROM events").unwrap();
    let streams: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(
        streams.iter().any(|s| s.starts_with(&minted_prefix)),
        "at least one stream must live under the minted namespace after the bare menu ran; got \
         {streams:?}"
    );
    assert!(
        !streams.iter().any(|s| s.starts_with(&legacy_prefix)),
        "no stream may be left behind under the legacy namespace after the bare menu ran; got \
         {streams:?}"
    );
}
