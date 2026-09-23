//! Periphery for spec 104 criterion 1 (THE LAUNCH IS TYPED): the parts of the new surface
//! that the implementer's own in-process tests structurally cannot reach - the
//! `SpawnLaunched` record's WIRE CONTRACT across a real process boundary, and the two
//! empty-string field fallbacks on `Driver` that only a caller building the struct
//! directly (not `Driver::default()`) ever exercises.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. `src/driver/claude_code.rs`'s
//! own `mod tests`, and `src/progress.rs`'s / `src/progress_store.rs`'s own `mod tests`,
//! prove `Driver::launch`, `build_args`, `SpawnLaunched::to_event` and
//! `progress_store::record_launch` IN PROCESS - every one of them against
//! `Store::open(":memory:")`, a store that lives and dies with the single test function
//! that opened it, and every one of them constructing `Driver` with BOTH `bin` and
//! `rigger_bin` set to a real, non-empty value. Two things follow from that which no
//! implementer test proves:
//!
//! 1. THE ROUND-TRIP + BACK-COMPAT the new `SpawnLaunched` serialized form needs (this
//!    unit's diff adds a fresh `TYPE_SPAWN_LAUNCHED` event type). `src/progress.rs`'s own
//!    module doc names the record's reason to exist: "a fresh process can always tell
//!    whether a spawn's latest launch ever actually started, from the log alone" - and
//!    spec 104's own CONSTRAINTS WALK: "Cold start - the log and the progress store are
//!    the only state." No implementer test opens a SECOND `Store` instance against the
//!    SAME on-disk file after the first is dropped, so none proves that property; and none
//!    proves the JSON shape itself tolerates a field arriving late or a field the reader
//!    has never heard of, the two compatibility directions the type's own
//!    `#[serde(default, skip_serializing_if = "Option::is_none")]` on `resumed_from` and
//!    the absence of `#[serde(deny_unknown_fields)]` are there to buy.
//! 2. `Driver.bin` and `Driver.rigger_bin` are both `pub` fields documented "Empty resolves
//!    to ... on $PATH" - a caller can and does reach that branch directly (a struct
//!    literal with one field left as `String::new()`, `..Default::default()` filling the
//!    rest, or simply an empty string threaded through from an unset config value)
//!    WITHOUT going through `Driver::default()`, which sets both fields to a literal
//!    non-empty value and so never touches either fallback. Every implementer test either
//!    uses `Driver::default()` or sets both fields explicitly.
//!
//! NOT OWNED HERE: argv/cwd/env/stdin shape (`build_args`, `launch`'s own process
//! plumbing with both fields already set) - `src/driver/claude_code.rs`'s own tests
//! already drive the real fixture binary and assert every one of those; THE STREAM
//! reader, session resume and failure classification - criterion 2 onward, not yet
//! landed (`impl AgentDriver for Driver` does not exist yet on this branch); the
//! spawn-bound MCP server, the write guard and the StopFailure hooks - criteria 3, 4 and
//! 5, separate units.

use std::io::Read;
use std::process::Child;
use std::sync::Mutex;

use rigger::conductor::SpawnOpts;
use rigger::config::AgentDef;
use rigger::driver::claude_code::Driver;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::progress::{SpawnLaunched, STREAM, TYPE_SPAWN_LAUNCHED};

/// The checked-in fixture `src/driver/claude_code.rs`'s own tests already point `bin` at -
/// see its header comment (`tests/fixtures/claude-code-echo-agent.sh`) for why it is
/// checked in rather than written at test time.
fn fixture_bin() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-echo-agent.sh")
        .to_string_lossy()
        .into_owned()
}

fn opts(id: &str) -> SpawnOpts {
    SpawnOpts {
        id: id.to_string(),
        attempt: 0,
        system_prompt: "You implement findings.".to_string(),
        ..Default::default()
    }
}

/// Reap a fixture child exactly as the implementer's own `read_fixture_lines` does - the
/// fixture reads one stdin line then exits on its own, so this only drains and waits on
/// the handle rigger's own no-os-kill discipline requires (never a signal, never a pid
/// read back from a marker).
fn reap(child: &mut Child) {
    child.wait().expect("fixture agent exits on its own");
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
}

// ---- SpawnLaunched: round-trip through a REAL on-disk store, across a cold start ----

#[test]
fn spawn_launched_survives_a_cold_start_a_second_store_instance_reads_the_first_ones_write() {
    let scratch = tempfile::tempdir().expect("throwaway dir for a real on-disk store");
    let db_path = scratch
        .path()
        .join("progress.db")
        .to_str()
        .expect("utf8 scratch path")
        .to_string();

    let driver = Driver {
        bin: fixture_bin(),
        rigger_bin: "rigger".to_string(),
    };

    // First "process": open the store, perform one launch, then DROP the store - the
    // shape of a supervisor that recorded a launch and then exited (crash, restart, or
    // simply a fresh `rigger` invocation).
    let first_session = {
        let store = Store::open(&db_path).expect("open a real on-disk progress store");
        let mut o = opts("u104-launch/implementer#0");
        o.run_id = "run-cold-start".to_string();
        o.launch = 0;
        let mut launch = driver
            .launch(&AgentDef::default(), "first launch", &o, &store)
            .expect("first launch records and starts");
        reap(&mut launch.child);
        launch.session_id
    }; // `store` dropped here.

    // Second "process": a FRESH Store instance, same path - proves the write is durable
    // on disk, not merely alive in the first instance's own memory.
    let reopened = Store::open(&db_path).expect("reopen the same on-disk progress store");
    let recorded = reopened
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("read the progress stream back after reopening");
    assert_eq!(
        recorded.len(),
        1,
        "the launch the first store instance recorded must survive the reopen"
    );
    assert_eq!(recorded[0].type_, TYPE_SPAWN_LAUNCHED);
    let sl: SpawnLaunched = serde_json::from_slice(&recorded[0].data)
        .expect("the on-disk bytes deserialize as SpawnLaunched");
    assert_eq!(sl.spawn, "u104-launch/implementer#0");
    assert_eq!(sl.launch, 0);
    assert_eq!(
        sl.session_id, first_session,
        "the session id the first process minted is the one durable on disk"
    );
    assert_eq!(sl.resumed_from, None);
    assert!(sl.started > 0, "started: {}", sl.started);

    // A second launch, from the SECOND store instance - proves a fresh process can not
    // only READ the prior launch but keep APPENDING to the same on-disk log, the shape a
    // real relaunch-after-restart takes.
    let mut o2 = opts("u104-launch/implementer#0");
    o2.run_id = "run-cold-start".to_string();
    o2.launch = 1;
    let mut second = driver
        .launch(&AgentDef::default(), "second launch", &o2, &reopened)
        .expect("second launch, from the reopened store, records and starts");
    reap(&mut second.child);

    let recorded = reopened
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("read both launches back");
    assert_eq!(
        recorded.len(),
        2,
        "both launches, across two store instances, land on the one log"
    );
    let launches: Vec<u32> = recorded
        .iter()
        .map(|e| {
            serde_json::from_slice::<SpawnLaunched>(&e.data)
                .unwrap()
                .launch
        })
        .collect();
    assert_eq!(launches, vec![0, 1]);
    assert_ne!(
        second.session_id, first_session,
        "a fresh process mints a fresh session id too"
    );
}

// ---- SpawnLaunched: the JSON wire shape tolerates missing and unknown fields ----

#[test]
fn spawn_launched_json_back_compat_the_minimal_pre_resumed_from_shape_still_parses() {
    // The exact minimal shape every fresh launch spec 104 itself performs actually
    // serializes as (`#[serde(skip_serializing_if = "Option::is_none")]` on
    // `resumed_from`), proven here independently of that serializer: a hand-authored
    // record with no `resumed_from` key at all - what an older or partial writer would
    // leave on disk - still parses, and the missing field defaults to `None` rather than
    // failing to parse.
    let minimal =
        r#"{"spawn":"u9/implementer#0","launch":0,"session_id":"s-1","started":1700000000}"#;
    let sl: SpawnLaunched = serde_json::from_str(minimal)
        .expect("a SpawnLaunched record with no resumed_from key must still parse");
    assert_eq!(sl.spawn, "u9/implementer#0");
    assert_eq!(sl.launch, 0);
    assert_eq!(sl.session_id, "s-1");
    assert_eq!(sl.resumed_from, None);
    assert_eq!(sl.started, 1_700_000_000);
}

#[test]
fn spawn_launched_json_forward_compat_an_unknown_future_field_does_not_break_parsing() {
    // No `#[serde(deny_unknown_fields)]` on SpawnLaunched: a later criterion adding a key
    // to the record (closing it on exit is explicitly left to "whichever later criterion
    // reads the stream") must not break an earlier binary's ability to read the rest of
    // an on-disk record that already carries it. Proven directly against the type.
    let from_the_future = r#"{"spawn":"u9/implementer#0","launch":2,"session_id":"s-2",
        "started":1700000001,"resumed_from":"s-1","a_field_this_criterion_never_heard_of":42}"#;
    let sl: SpawnLaunched = serde_json::from_str(from_the_future)
        .expect("an unknown extra field must not break deserialization");
    assert_eq!(sl.launch, 2);
    assert_eq!(sl.resumed_from.as_deref(), Some("s-1"));
}

// ---- Driver: the empty-string field fallbacks, reachable only by direct construction ----

#[test]
fn launch_resolves_an_empty_rigger_bin_to_the_literal_rigger_on_path() {
    // `Driver.rigger_bin`'s own doc comment: empty resolves to `"rigger"` on $PATH. Every
    // implementer test sets it explicitly (`"rigger"` or a custom path); this proves the
    // EMPTY-STRING branch itself, reachable because the field is `pub` - a caller can
    // build `Driver { rigger_bin: String::new(), .. }` directly, not only through
    // `Driver::default()`, which sets a literal non-empty value and so never exercises
    // this branch.
    let driver = Driver {
        bin: fixture_bin(),
        rigger_bin: String::new(),
    };
    let store = Store::open(":memory:").expect("in-memory store for a pure-argv assertion");
    let mut launch = driver
        .launch(
            &AgentDef::default(),
            "task",
            &opts("u/implementer#0"),
            &store,
        )
        .expect("an empty rigger_bin must not fail the launch");
    reap(&mut launch.child);

    let i = launch
        .args
        .iter()
        .position(|x| x == "--mcp-config")
        .expect("--mcp-config is always present");
    let cfg: serde_json::Value =
        serde_json::from_str(&launch.args[i + 1]).expect("mcp-config is valid JSON");
    assert_eq!(
        cfg["mcpServers"]["rigger"]["command"], "rigger",
        "an empty rigger_bin resolves to the literal on $PATH, never an empty argv token"
    );
}

/// Guards every test in this file that reads or writes the real process `PATH` - `cargo
/// test` runs the functions in one binary as concurrent threads by default, and a
/// concurrent env read racing a concurrent env write is a hazard at the POSIX
/// `setenv`/`getenv` level regardless of which keys either side touches (same discipline
/// `tests/build_env_authority_periphery.rs`'s own `ENV_TEST_LOCK` documents). Only one
/// test in this file touches `PATH` today; the lock stays so a future addition here does
/// not have to rediscover the hazard.
static PATH_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
#[cfg(unix)]
fn launch_resolves_an_empty_bin_to_the_literal_claude_found_on_path() {
    // Same fallback convention as `crate::driver::cli::Driver` (`cli.rs:40`), extended
    // here to this driver's own `bin` field - and, unlike that sibling copy (which no
    // test in cli.rs's own suite exercises either), closed end-to-end here: a real PATH
    // entry named `claude` (the checked-in fixture, staged under that name in a
    // throwaway dir and PREPENDED to the real PATH so the fixture's own `#!/bin/sh`
    // shebang and the coreutils it calls keep resolving) proves an empty `bin` resolves
    // to the literal `"claude"` and is actually found and run - not merely that the
    // private fallback method returns a string nobody ever spawns.
    let _guard = PATH_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let bindir = tempfile::tempdir().expect("throwaway PATH dir");
    let claude_path = bindir.path().join("claude");
    std::fs::copy(fixture_bin(), &claude_path).expect("stage the fixture as `claude`");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&claude_path).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&claude_path, perm).expect("chmod staged claude");
    }

    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    let new_path = std::env::join_paths(
        std::iter::once(bindir.path().to_path_buf()).chain(std::env::split_paths(&orig_path)),
    )
    .expect("join synthetic PATH");
    std::env::set_var("PATH", new_path);

    let driver = Driver {
        bin: String::new(),
        rigger_bin: "rigger".to_string(),
    };
    let store = Store::open(":memory:").expect("in-memory store");
    let result = driver.launch(
        &AgentDef::default(),
        "task",
        &opts("u/implementer#0"),
        &store,
    );

    std::env::set_var("PATH", orig_path);

    let mut launch = result.expect("an empty bin resolves to `claude` and is found on PATH");
    reap(&mut launch.child);
}
