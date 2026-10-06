//! Spec 102, criterion 1 (THE WIDTH IS ENFORCED) - the ONE seam the implementer's own
//! `run_wave`-pinned unit tests are structurally blind to: whether `defaults.max_parallel_units`
//! actually bounds REAL, concurrently-scheduled agent spawns, not just set membership across
//! sequential in-process calls.
//!
//! WHAT THE INSIDE-OUT TESTS CANNOT REACH. Both of `conductor.rs`'s own new tests
//! (`run_wave_admits_at_most_max_parallel_units_leaving_the_rest_neither_failed_nor_terminal`,
//! `occupancy_survives_a_crash_resume_so_a_still_parked_unit_keeps_its_slot_over_a_fresh_one`)
//! drive either the private `run_wave` directly or the public `run()` entry point, but every
//! spawn in both runs through the crate-internal `Stub` driver, which resolves synchronously
//! with no subprocess and no real elapsed time. That proves the admission bookkeeping (which
//! names land in `admitted`/`in_flight`) is correct, but it can never prove the other half of
//! the claim `run_wave`'s own doc comment makes: that the bound actually constrains
//! `run_batch`'s REAL `std::thread::scope` concurrency (spec 102's whole motivation - three
//! real 54 GB build caches alive at once) rather than merely bookkeeping names nobody ever
//! runs in parallel. Nor does it prove the converse - that a width greater than one truly
//! ADMITS real concurrent spawns rather than accidentally over-serializing (a width-2 config
//! that silently behaves like width-1 would still satisfy every assertion in the two unit
//! tests above, since `Stub` never overlaps regardless).
//!
//! This file closes both directions over the crate's PUBLIC surface only
//! (`conductor::run`, `conductor::Deps`, `driver::cli::Driver`), with REAL production types
//! and a REAL subprocess for every spawn: `max_parallel_units: 2` against three independent
//! (`needs: []`) ready stages, each spawn a real `sh` process that sleeps and then records its
//! own start/end window, reconstructed afterward into the exact number of spawns that
//! genuinely overlapped in wall-clock time. THE WIDTH IS ENFORCED (never more than 2
//! overlapped) and THE WIDTH IS NOT OVER-ENFORCED (2 genuinely did overlap at least once) are
//! both asserted, so a regression toward stricter-than-configured serialization is caught
//! exactly as loudly as a regression toward unbounded concurrency.
//!
//! Out of scope, already closed elsewhere: the scaffolded default and the sizing comment
//! (`tests/cli.rs`'s `a_fresh_scaffolded_init_writes_max_parallel_units_and_validates_...`,
//! criterion 2); crash-resume re-derivation of occupancy from the log is a `Stub`-parking
//! scenario the synchronous `cli::Driver` here cannot even produce (only a stepwise/replay
//! driver returns a parked error) and stays owned by the implementer's own
//! `occupancy_survives_a_crash_resume_...` unit test, exactly as the spec's own Done-when says
//! ("pinned at `run_wave`").

use std::path::Path;

use rigger::conductor::{run, Deps};
use rigger::config::{self, AgentDef, Config, Stage};
use rigger::driver::cli;
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;

/// Write a real, executable `sh` stub standing in for the agent binary `cli::Driver`
/// shells out to: it sleeps `sleep_secs`, then appends its own `<start_ns> <end_ns>`
/// window as ONE line to `marker_log` (a single small `printf ... >>` append is one
/// `write(2)` call under `PIPE_BUF`, so concurrent writers from independent processes never
/// interleave mid-line), and finally prints the emit-protocol-free final-result line the cli
/// driver only checks the exit status of.
#[cfg(unix)]
fn write_marker_agent(path: &Path, marker_log: &Path, sleep_secs: &str) {
    use std::os::unix::fs::PermissionsExt;
    let script = format!(
        "#!/bin/sh\n\
         start=$(date +%s%N)\n\
         sleep {sleep_secs}\n\
         end=$(date +%s%N)\n\
         printf '%s %s\\n' \"$start\" \"$end\" >> '{log}'\n\
         echo '{{\"id\":\"final\",\"pass\":true}}'\n",
        sleep_secs = sleep_secs,
        log = marker_log.display(),
    );
    std::fs::write(path, script).unwrap_or_else(|e| panic!("write marker agent {path:?}: {e}"));
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

/// Parse the marker log into `(start_ns, end_ns)` windows, one per real agent spawn.
fn parse_windows(contents: &str) -> Vec<(i128, i128)> {
    contents
        .lines()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let start: i128 = parts
                .next()
                .unwrap_or_else(|| panic!("marker line missing start: {line:?}"))
                .parse()
                .unwrap_or_else(|e| panic!("parse start in {line:?}: {e}"));
            let end: i128 = parts
                .next()
                .unwrap_or_else(|| panic!("marker line missing end: {line:?}"))
                .parse()
                .unwrap_or_else(|e| panic!("parse end in {line:?}: {e}"));
            (start, end)
        })
        .collect()
}

/// The maximum number of `(start, end)` windows overlapping at any single instant, via a
/// sweep over start(+1)/end(-1) events. Ties sort end-before-start (Rust's tuple order puts
/// `-1 < 1` at equal timestamps), so a spurious exact-tie can only ever UNDER-count overlap,
/// never manufacture a false one - the width-enforced assertion below stays sound even in
/// that vanishingly unlikely nanosecond-tie case.
fn max_overlap(windows: &[(i128, i128)]) -> i32 {
    let mut events: Vec<(i128, i32)> = Vec::with_capacity(windows.len() * 2);
    for (start, end) in windows {
        events.push((*start, 1));
        events.push((*end, -1));
    }
    events.sort();
    let mut current = 0;
    let mut peak = 0;
    for (_, delta) in events {
        current += delta;
        peak = peak.max(current);
    }
    peak
}

#[test]
#[cfg(unix)]
fn max_parallel_units_bounds_real_concurrent_agent_spawns_through_a_real_conductor_run() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let marker_log = dir.path().join("marker.log");
    let agent_bin = dir.path().join("marker-agent.sh");
    write_marker_agent(&agent_bin, &marker_log, "0.4");

    let mut cfg = Config::default();
    cfg.workflow.defaults.max_parallel_units = 2;
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "ok".into(),
        config::Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
            requires: Vec::new(),
        },
    );
    // Three INDEPENDENT stages (`needs` empty on all three): `wave_ready` offers every one
    // of them in the SAME first wave, so `run_wave`'s admission control - not scheduling
    // order - is what must keep the third one out.
    for name in ["a", "b", "c"] {
        cfg.workflow.stages.insert(
            name.to_string(),
            Stage {
                name: name.to_string(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "none".into(),
                ..Default::default()
            },
        );
    }

    let driver = cli::Driver {
        bin: agent_bin.to_string_lossy().into_owned(),
        ..cli::Driver::default()
    };
    let store = Store::open(":memory:").unwrap();
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
        log: &|_| {},
    };

    run(&cfg, &deps)
        .expect("the run must complete: three real agent spawns bounded by max_parallel_units");

    let contents = std::fs::read_to_string(&marker_log).expect("read the marker log");
    let windows = parse_windows(&contents);
    assert_eq!(
        windows.len(),
        3,
        "all three real agent spawns must have run exactly once each: {windows:?}"
    );

    let peak = max_overlap(&windows);
    assert!(
        peak <= 2,
        "max_parallel_units: 2 must never let more than 2 real agent spawns overlap in \
         wall-clock time: windows={windows:?}, peak overlap={peak}"
    );
    assert!(
        peak >= 2,
        "max_parallel_units: 2 must actually admit 2 real agent spawns at once, not \
         silently over-serialize to 1: windows={windows:?}, peak overlap={peak}"
    );
}
