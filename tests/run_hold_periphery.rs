//! Periphery for spec 105 criterion 2, THE EVENTS: a run's hold as it reads back from the log on
//! disk, and the forge `rigger emit` refuses. These run OUTSIDE the crates, over the root crate's
//! public surface and the compiled binary.
//!
//! OWNED HERE: the seam from the on-disk store through `read_current_run` into `ledger::project`,
//! which the crate's own fold table never crosses, since it hand-builds its events and cuts them
//! with `current_run`; and `rigger emit`, the one surface a worker reaches the run stream through.
//! The hold events are minted by the supervisor and the operator's verbs, so a worker that could
//! emit one could hold a run or end a hold.
//!
//! NOT OWNED HERE: the hold events' wire shape and the fold's rules over a hand-built log
//! (`rigger-domain`'s own tests); the ordered episodic list (`event_classes_are_one_table.rs`);
//! the disposition table, the sightings, the done run's and the mint's releases and the hold sink
//! (the second part of this criterion); and every surface that reads the hold (the criteria after
//! this one).

mod common;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::cli::{emit_refused, run_rigger, seed_run_events, temp_store_project, with_run_store};
use common::fixtures::types_of;
use rigger::eventstore::{Event, ExpectedRevision};
use rigger::ledger::{project, Hold, TYPE_UNIT_STARTED};
use rigger::run::read::read_current_run;
use rigger::run::{RunHeld, STREAM, TYPE_RUN_HELD, TYPE_RUN_RELEASED, TYPE_RUN_STARTED};

/// A `RunStarted` body as the log holds one.
const STARTED: &str = r#"{"run":"r1"}"#;

/// The `RunStarted` body of the run after [`STARTED`].
const NEXT_STARTED: &str = r#"{"run":"r2"}"#;

/// A host's `RunHeld` as a writer leaves it on disk, every field spelled by its decided name.
const HELD_BY_HOST: &str = r#"{"cause":"billing_error","detail":"credit balance too low","action":"fix the account's billing","needs_person":true,"by":"host","model":"m1"}"#;

/// An operator's `RunHeld` as a writer leaves it on disk.
const HELD_BY_OPERATOR: &str = r#"{"cause":"operator","detail":"","action":"back after the patch window","needs_person":true,"by":"operator","model":"m1"}"#;

/// [`HELD_BY_HOST`] carrying a field no writer of this binary spells: what a later binary's
/// append reads as, here.
const HELD_WITH_A_LATER_FIELD: &str = r#"{"cause":"billing_error","detail":"credit balance too low","action":"fix the account's billing","needs_person":true,"by":"host","model":"m1","attempt":3}"#;

/// A `RunReleased` as a writer leaves it on disk.
const RELEASED: &str = r#"{"by":"probe","held_for_s":60}"#;

/// The valid time of the seeded event at `offset`: a time well apart from the moment the store
/// records the event, with nanoseconds a store persisting anything coarser would lose.
fn valid_at(offset: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(1_700_000_000_123_456_789 + offset)
}

/// The types of the current run's slice as `root`'s store hands it to a fold, and the open hold
/// `ledger::project` folds from that slice: the read every production reader of the hold makes.
fn read_back(root: &Path) -> (Vec<String>, Option<Hold>) {
    let slice = with_run_store(root, |store| read_current_run(store, STREAM).unwrap().0);
    let types = types_of(&slice).into_iter().map(String::from).collect();
    (types, project(&slice).unwrap().hold)
}

/// THE EVENTS, through the log on disk: each row appends its events one at a time to a real store
/// at the valid times it names, then reads the current run back as a reader does. Each row crosses
/// something the crate's own fold table cannot: the body, the valid time and the revision the
/// store stamped surviving it, the boundary `read_run` finds by position, or an order the log and the valid times disagree on.
/// The order that counts is the log's, so a hold's start and its end are decided by position.
#[test]
fn the_open_hold_reads_back_from_the_log_on_disk_by_the_rules_of_the_events() {
    let by_host = RunHeld {
        cause: "billing_error".to_string(),
        detail: "credit balance too low".to_string(),
        action: "fix the account's billing".to_string(),
        needs_person: true,
        by: "host".to_string(),
        model: "m1".to_string(),
    };
    let by_operator = RunHeld {
        cause: "operator".to_string(),
        detail: String::new(),
        action: "back after the patch window".to_string(),
        needs_person: true,
        by: "operator".to_string(),
        model: "m1".to_string(),
    };
    let open = |offset: u64, operator: bool, latest: &RunHeld, revision: i64| {
        Some(Hold {
            since: valid_at(offset),
            operator,
            latest: latest.clone(),
            revision,
        })
    };

    let cases = vec![
        (
            "a log from before the hold events folds unheld",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_UNIT_STARTED, r#"{"id":"u"}"#, 2),
            ],
            vec!["RunStarted", "UnitStarted"],
            None,
        ),
        (
            "with no run started the whole stream is the slice, its RunHeld with it",
            vec![(TYPE_RUN_HELD, HELD_BY_HOST, 5)],
            vec!["RunHeld"],
            open(5, false, &by_host, 0),
        ),
        (
            "a stored RunHeld keeps every field and its valid time to the nanosecond",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_HOST, 5),
            ],
            vec!["RunStarted", "RunHeld"],
            open(5, false, &by_host, 1),
        ),
        (
            "a stored operator RunHeld reads as an operator hold",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_OPERATOR, 5),
            ],
            vec!["RunStarted", "RunHeld"],
            open(5, true, &by_operator, 1),
        ),
        (
            "a body carrying a field this binary never heard of still holds the run",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_WITH_A_LATER_FIELD, 5),
            ],
            vec!["RunStarted", "RunHeld"],
            open(5, false, &by_host, 1),
        ),
        (
            "a stored RunReleased ends the hold",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_HOST, 5),
                (TYPE_RUN_RELEASED, RELEASED, 7),
            ],
            vec!["RunStarted", "RunHeld", "RunReleased"],
            None,
        ),
        (
            "the first RunHeld in the log starts the hold, whatever a later one's valid time",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_HOST, 9),
                (TYPE_RUN_HELD, HELD_BY_OPERATOR, 5),
            ],
            vec!["RunStarted", "RunHeld", "RunHeld"],
            open(9, true, &by_operator, 2),
        ),
        (
            "a RunReleased later in the log ends the hold, whatever its valid time",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_HOST, 9),
                (TYPE_RUN_RELEASED, RELEASED, 5),
            ],
            vec!["RunStarted", "RunHeld", "RunReleased"],
            None,
        ),
        (
            "a hold left open stays in its run's slice, so the next run starts unheld",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_OPERATOR, 5),
                (TYPE_RUN_STARTED, NEXT_STARTED, 7),
            ],
            vec!["RunStarted"],
            None,
        ),
        (
            "the mint's release and its RunStarted leave the new run unheld",
            vec![
                (TYPE_RUN_STARTED, STARTED, 1),
                (TYPE_RUN_HELD, HELD_BY_HOST, 3),
                (TYPE_RUN_RELEASED, RELEASED, 5),
                (TYPE_RUN_STARTED, NEXT_STARTED, 6),
            ],
            vec!["RunStarted"],
            None,
        ),
    ];

    for (name, log, slice, want) in cases {
        let project_dir = temp_store_project();
        with_run_store(project_dir.path(), |store| {
            for (type_, body, offset) in &log {
                let event =
                    Event::new(*type_, body.as_bytes().to_vec()).with_valid_from(valid_at(*offset));
                store
                    .append(STREAM, ExpectedRevision::Any, &[event])
                    .unwrap();
            }
        });
        let (types, hold) = read_back(project_dir.path());
        assert_eq!(types, slice, "{name}: the slice a fold is handed");
        assert_eq!(hold, want, "{name}: the open hold");
    }
}

/// Given a run, held or unheld, when a worker reaches for `rigger emit` with a hold event, then
/// the binary refuses it by name, the log gains nothing and the run's hold stands as it did: no
/// worker opens a hold, and none ends one.
#[test]
fn rigger_emit_refuses_a_forged_hold_event_and_the_run_stands_as_it_did() {
    for (name, type_, body, log, cause) in [
        (
            "a forged RunHeld on an unheld run",
            TYPE_RUN_HELD,
            HELD_BY_OPERATOR,
            vec![(TYPE_RUN_STARTED, STARTED)],
            None,
        ),
        (
            "a forged RunReleased on a held run",
            TYPE_RUN_RELEASED,
            RELEASED,
            vec![(TYPE_RUN_STARTED, STARTED), (TYPE_RUN_HELD, HELD_BY_HOST)],
            Some("billing_error"),
        ),
    ] {
        let project_dir = temp_store_project();
        let root = project_dir.path();
        seed_run_events(root, &log);
        let before = read_back(root);
        assert_eq!(
            before.1.as_ref().map(|hold| hold.latest.cause.as_str()),
            cause,
            "{name}: the hold the seeded log folds to"
        );

        let (stdout, stderr, ok) = run_rigger(root, &["emit", type_, body]);

        let refusal = format!(
            "rigger: rigger_emit: {} Run-lifecycle and orchestration boundaries (RunStarted and \
             its peers) are minted by the conductor, not `rigger emit` - start a new run with \
             `rigger run --fresh`.\n",
            emit_refused(type_)
        );
        assert_eq!(
            (stdout.as_str(), stderr.as_str(), ok),
            ("", refusal.as_str(), false),
            "{name}: the refusal"
        );
        assert_eq!(read_back(root), before, "{name}: the run after the refusal");
    }
}
