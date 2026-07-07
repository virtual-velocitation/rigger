//! Agent liveness (spec 10, unit 3): a spawn carries a `max_wall_clock` bound and its
//! worker touches a per-spawn liveness MARKER file under the scratch root on a heartbeat
//! interval. `rigger step` treats a spawn whose marker is STALE beyond the wall-clock as
//! an infrastructure fault - a HUNG agent that stopped making progress - so it can no
//! longer stall a wave invisibly.
//!
//! This module is the framework-free domain of that mechanism: the single marker-path
//! authority, the pure staleness decision, and the classification that routes a hung
//! spawn through unit 2's [`failure::Taxonomy`]. The pure decisions ([`is_stale`],
//! [`classify_stale`], [`classify_hung`]) name no store and no config; [`sweep`] and
//! [`hung_spawns`] are the caller-facing helpers `rigger step` runs, which read marker
//! mtimes and record/fold the outcome on the run stream.
//!
//! ## Classification (the class is an operator-facing LABEL; the treatment is uniform)
//!
//! A hung worker is classified by feeding a distinctive [`stale_signal`] to the
//! configured [`failure::Taxonomy`]. A hung/unresponsive worker is an INFRASTRUCTURE
//! condition (the agent PROCESS stalled, not the unit's code), so [`classify_hung`]
//! defaults to [`FailureClass::Infra`] and lets a workflow RELABEL it only through a rule
//! that SPECIFICALLY targets the liveness signal - a NON-wildcard matcher (e.g. an
//! `output_regex` on the stale text). A catch-all rule (`match: {}`, the shipped
//! default's final `product` rule) classifies GATE output, not hung agents, so it does
//! NOT capture a hung spawn.
//!
//! The class is a DISPLAY/AUDIT label only: it rides the recorded fault (the
//! [`spawn::META_LIVENESS_CLASS`] meta value) and is surfaced in the step halt and the
//! stats, so an operator sees how the workflow named the stall. It does NOT change the
//! TREATMENT. EVERY hung spawn - whatever class a rule labels it - is recorded as a
//! no-attempt-charged liveness fault ([`SpawnResult::liveness_fault`]) and re-parked by
//! the replay driver, because a hung agent PROCESS is infrastructure regardless of any
//! rule's label: charging a unit's remediation counter for its agent hanging would be
//! exactly the misclassification unit 2's infra semantics exist to prevent. (A workflow
//! that wants a hung agent to CHARGE the unit would be asking the liveness mechanism to
//! do the dead-worker-exit driver's job, which is out of this unit's scope.) Recovery is
//! uniform too: an operator records a real result (last-write-wins) and re-drives.

use std::time::{Duration, SystemTime};

use crate::eventstore::{Error, Event, EventStore};
use crate::failure::{FailureClass, Signal, Taxonomy};
use crate::spawn::{self, SpawnResult};

/// The scratch subdirectory the per-spawn liveness markers live under, a sibling of the
/// worktrees and `agent-scratch`. Kept in ONE place so the sweep and the driver-framed
/// worker instruction (`workflows/rigger.js`) derive the same path.
pub const MARKER_SUBDIR: &str = "agent-live";

/// The filesystem-safe marker filename for a spawn id: every character that is not
/// ASCII alphanumeric, `.`, `-`, or `_` becomes `_`. A spawn id is `{unit}/{role}#{n}`,
/// so the `/` and `#` (which would otherwise create subdirectories or shell-quirky
/// names) collapse to `_`. This exact rule is mirrored in `workflows/rigger.js` so the
/// worker touches the SAME file the sweep reads.
pub fn marker_filename(spawn_id: &str) -> String {
    spawn_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The absolute marker path for a spawn:
/// `<scratch_root>/agent-live/<run_id>/<sanitized id>`.
///
/// The SINGLE authority for where a spawn's liveness marker lives - the worker touches it
/// (driver-framed instruction, over the path `rigger step` carries on the wave item) and
/// the sweep stats it, both through THIS function, so a re-hardcoded root can never make
/// the two diverge. The `run_id` component gives the marker RUN IDENTITY: a re-run that
/// reuses a unit-title slug computes the same spawn id, but a DIFFERENT run gets a
/// different subdir, so the sweep never reads a prior run's leftover mtime and records a
/// bogus multi-hour `silent_for`. An empty `run_id` (a caller outside a run - the pure-fold
/// tests) omits the run subdir, keeping the path stable for the no-run case.
pub fn marker_path(scratch_root: &str, run_id: &str, spawn_id: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(scratch_root).join(MARKER_SUBDIR);
    let dir = if run_id.is_empty() {
        dir
    } else {
        dir.join(marker_filename(run_id))
    };
    dir.join(marker_filename(spawn_id))
}

/// Whether a spawn last seen alive at `last_seen` is STALE at `now` given its wall-clock
/// bound: `now - last_seen > max_wall_clock`. A `last_seen` in the future (clock skew) is
/// never stale. A zero `max_wall_clock` means "no bound" and is never stale.
pub fn is_stale(now: SystemTime, last_seen: SystemTime, max_wall_clock: Duration) -> bool {
    if max_wall_clock.is_zero() {
        return false;
    }
    match now.duration_since(last_seen) {
        Ok(elapsed) => elapsed > max_wall_clock,
        Err(_) => false,
    }
}

/// The failure SIGNAL a hung spawn presents to the taxonomy: a distinctive,
/// human-readable output line describing the stall. A workflow that wants to reclassify
/// (or explicitly pin infra) can match on this text via a `failure_rules` `output_regex`.
pub fn stale_signal() -> Signal {
    Signal::from_output(
        "rigger: liveness marker stale beyond the spawn's max_wall_clock (the agent is unresponsive/hung)",
    )
}

/// Classify a hung spawn: a rule that SPECIFICALLY (non-wildcard) matches the hung-agent
/// signal governs, letting a workflow RELABEL liveness faults; a wildcard catch-all match
/// (the shipped default's final `product` rule classifies GATE output) or no match at all
/// defaults to [`FailureClass::Infra`] - a hung worker is infrastructure, not the unit's
/// code, and the generic gate catch-all must never label a hung agent as the unit's fault.
///
/// The returned class is a DISPLAY/AUDIT label only (see the module docs): every hung spawn
/// is recorded and re-parked no-charge regardless of it. The wildcard test routes through
/// [`Matcher::is_any`](crate::failure::Matcher::is_any) - the single authority - rather
/// than re-checking the matcher fields.
pub fn classify_hung(taxonomy: &Taxonomy) -> FailureClass {
    match taxonomy.classify(&stale_signal()) {
        Some(rule) if !rule.matcher.is_any() => rule.class,
        _ => FailureClass::Infra,
    }
}

/// One in-flight spawn the sweep is evaluating: its deterministic id, the unit it belongs
/// to, when it was last seen alive (its marker mtime), and its wall-clock bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InFlightSpawn {
    pub id: String,
    pub unit: String,
    pub last_seen: SystemTime,
    pub max_wall_clock: Duration,
}

/// A hung spawn the sweep found stale, with the class the taxonomy assigned it and how
/// long past its last heartbeat it has been silent - the descriptor `rigger step`
/// surfaces and records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleSpawn {
    pub id: String,
    pub unit: String,
    pub class: FailureClass,
    pub silent_for: Duration,
}

/// The pure staleness core: given the in-flight spawns (each with the time it was last
/// seen alive and its wall-clock bound), the taxonomy, and `now`, return the ones that
/// are stale, classified. No IO - the caller reads marker mtimes and records outcomes.
pub fn classify_stale(
    in_flight: &[InFlightSpawn],
    taxonomy: &Taxonomy,
    now: SystemTime,
) -> Vec<StaleSpawn> {
    let class = classify_hung(taxonomy);
    in_flight
        .iter()
        .filter(|s| is_stale(now, s.last_seen, s.max_wall_clock))
        .map(|s| StaleSpawn {
            id: s.id.clone(),
            unit: s.unit.clone(),
            class,
            silent_for: now.duration_since(s.last_seen).unwrap_or(Duration::ZERO),
        })
        .collect()
}

/// The human-readable error text recorded on a hung spawn's result and surfaced in the
/// step halt. Names the spawn, its class, and the no-attempt-charged semantics so an
/// operator reading the halt knows exactly what happened and that the unit was not blamed.
pub fn stale_result_message(s: &StaleSpawn) -> String {
    format!(
        "spawn {:?} (unit {:?}) hung: its liveness marker went stale for {}s beyond its \
         max_wall_clock, classified {} - no remediation attempt is charged (the unit's code \
         is not at fault). Re-drive it once the agent/driver is healthy: record a real result \
         with `rigger result {}` (last-write-wins supersedes this liveness fault).",
        s.id,
        s.unit,
        s.silent_for.as_secs(),
        s.class.as_str(),
        s.id,
    )
}

/// A hung spawn whose LATEST recorded result is a liveness fault (spec 10, unit 3) - the
/// not-yet-recovered set `rigger step` surfaces in its output so a hung agent is visible,
/// not a silent stall. A real result recorded later supersedes the fault (last-write-wins)
/// and the spawn drops out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HungSpawn {
    pub id: String,
    pub unit: String,
    pub class: String,
}

/// The liveness SWEEP `rigger step` runs against the run stream (spec 10, unit 3): find
/// every IN-FLIGHT spawn (a recorded request with no result yet) whose per-spawn liveness
/// marker under `scratch_root` is STALE beyond its `max_wall_clock`, classify it through
/// the `taxonomy`, and record a no-attempt-charged liveness fault on its id (the existing
/// [`SpawnResult`], recorded `--if-absent`, NEVER a new event type). Returns the spawns it
/// freshly recorded a fault for.
///
/// A spawn with NO marker is left alone (conservative: a not-yet-started or dead-on-arrival
/// worker is dead-worker-exit territory, unchanged here - only a marker that WAS being
/// touched and then went stale is a hung agent). A recorded liveness fault charges no
/// remediation attempt: the sweep records only a [`SpawnResult`], never a `UnitFailed`, and
/// the replay driver re-parks it - so a hung agent process never blames the unit's code.
pub fn sweep(
    store: &dyn EventStore,
    events: &[Event],
    scratch_root: &str,
    run_id: &str,
    taxonomy: &Taxonomy,
    now: SystemTime,
) -> Result<Vec<StaleSpawn>, Error> {
    let requested = spawn::recorded(events).map_err(|e| Error::Backend(e.to_string()))?;
    let mut in_flight = Vec::new();
    for req in requested.values() {
        // Only a spawn with a positive wall-clock bound is subject to a liveness timeout.
        let bound = match req.max_wall_clock {
            Some(secs) if secs > 0 => Duration::from_secs(secs),
            _ => continue,
        };
        // In-flight = requested but not yet answered. A spawn already carrying a result
        // (including a prior liveness fault) is not re-swept.
        if spawn::result_of(events, &req.id)
            .map_err(|e| Error::Backend(e.to_string()))?
            .is_some()
        {
            continue;
        }
        // The marker's mtime is the spawn's last proof of life. The path carries the run id
        // ([`marker_path`]), so a prior run's leftover marker for a slug-colliding id lives
        // under a different subdir and is never read here. A MISSING marker is left alone
        // (conservative - see the fn docs); only a present-but-stale marker is hung.
        let last_seen = match std::fs::metadata(marker_path(scratch_root, run_id, &req.id))
            .and_then(|m| m.modified())
        {
            Ok(mtime) => mtime,
            Err(_) => continue,
        };
        in_flight.push(InFlightSpawn {
            id: req.id.clone(),
            unit: req.unit.clone(),
            last_seen,
            max_wall_clock: bound,
        });
    }
    let stale = classify_stale(&in_flight, taxonomy, now);
    for s in &stale {
        let fault = SpawnResult::liveness_fault(&s.id, stale_result_message(s), s.class.as_str());
        spawn::record_result_if_absent(store, &fault)?;
    }
    Ok(stale)
}

/// Every spawn whose LATEST recorded result is a liveness fault (spec 10, unit 3): the
/// hung, not-yet-recovered spawns. `rigger step` folds this from the post-sweep stream to
/// SURFACE hung agents (a halt reason) every step until they recover, so a stall is never
/// silent - even on a step that recorded no NEW fault. A real result recorded later has a
/// larger position and supersedes the fault, so a recovered spawn drops out. Ordered by id.
pub fn hung_spawns(events: &[Event]) -> Result<Vec<HungSpawn>, Error> {
    let requested = spawn::recorded(events).map_err(|e| Error::Backend(e.to_string()))?;
    let mut hung = Vec::new();
    for (id, req) in &requested {
        if let Some(res) =
            spawn::result_of(events, id).map_err(|e| Error::Backend(e.to_string()))?
        {
            if res.is_liveness_fault() {
                hung.push(HungSpawn {
                    id: id.clone(),
                    unit: req.unit.clone(),
                    class: res.liveness_class(),
                });
            }
        }
    }
    Ok(hung)
}

/// The step halt reason for a non-empty set of hung spawns (spec 10, unit 3). Surfaced on
/// the `Step`'s `halted` channel so the driver stops LOUDLY - a hung agent halts the wave
/// VISIBLY rather than stalling it invisibly - naming each hung spawn and the recovery.
pub fn halt_reason(hung: &[HungSpawn]) -> String {
    let names = hung
        .iter()
        .map(|h| format!("{} (unit {}, classified {})", h.id, h.unit, h.class))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "liveness: {} spawn(s) hung past their max_wall_clock and were classified as \
         infrastructure faults (no remediation attempt charged): {names}. Re-drive each once \
         its agent/driver is healthy by recording a real result (`rigger result <id> ...`, \
         last-write-wins supersedes the liveness fault).",
        hung.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_filename_collapses_id_separators_to_underscores() {
        assert_eq!(
            marker_filename("unit-3-spawns-a-wall-clock/implementer#1"),
            "unit-3-spawns-a-wall-clock_implementer_1"
        );
        // Alphanumerics and . - _ survive; everything else (/, #, spaces, colons) is _.
        assert_eq!(marker_filename("a b:c/d#e.f-g_h"), "a_b_c_d_e.f-g_h");
    }

    #[test]
    fn marker_path_is_scratch_root_joined_with_the_run_subdir_and_filename() {
        // With a run id: `<scratch>/agent-live/<run>/<sanitized id>` - the run subdir gives
        // the marker RUN IDENTITY, so a slug-colliding re-run never reads a prior mtime.
        let p = marker_path("/scratch", "run-7", "u/implementer#0");
        assert_eq!(
            p,
            std::path::Path::new("/scratch/agent-live/run-7/u_implementer_0")
        );
        // An empty run id (a caller outside a run) omits the run subdir - the no-run path.
        let p = marker_path("/scratch", "", "u/implementer#0");
        assert_eq!(
            p,
            std::path::Path::new("/scratch/agent-live/u_implementer_0")
        );
        // A run id carrying id-structure characters is sanitized like a spawn id.
        let p = marker_path("/scratch", "run/7#a", "u/implementer#0");
        assert_eq!(
            p,
            std::path::Path::new("/scratch/agent-live/run_7_a/u_implementer_0")
        );
    }

    #[test]
    fn is_stale_is_elapsed_past_the_bound() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let bound = Duration::from_secs(60);
        // Last seen 61s ago: past the 60s bound -> stale.
        assert!(is_stale(now, now - Duration::from_secs(61), bound));
        // Last seen 59s ago: within the bound -> alive.
        assert!(!is_stale(now, now - Duration::from_secs(59), bound));
        // Exactly at the bound is NOT past it (strict >).
        assert!(!is_stale(now, now - Duration::from_secs(60), bound));
    }

    #[test]
    fn is_stale_never_fires_on_a_future_last_seen_or_a_zero_bound() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        // Clock skew: marker touched "in the future" - never stale, never panics.
        assert!(!is_stale(
            now,
            now + Duration::from_secs(10),
            Duration::from_secs(1)
        ));
        // A zero bound means unbounded - a spawn is never stale, however old.
        assert!(!is_stale(
            now,
            now - Duration::from_secs(99999),
            Duration::ZERO
        ));
    }

    #[test]
    fn classify_hung_defaults_to_infra_under_the_shipped_taxonomy() {
        // The shipped default taxonomy's narrow infra regex does NOT match the hung-agent
        // signal, so classify() returns None and the caller default (infra) governs - a
        // hung worker is infrastructure, never a charged product defect.
        assert_eq!(classify_hung(&Taxonomy::default()), FailureClass::Infra);
    }

    #[test]
    fn classify_hung_is_infra_even_when_a_catch_all_product_rule_would_match() {
        use crate::failure::{Backoff, FailureRule, Matcher};
        // A workflow whose ONLY rule is a catch-all product (the shape of the shipped
        // default's final rule): it classifies gate output, but a hung AGENT is not a
        // product defect, so the wildcard must NOT capture it - infra governs.
        let tax = Taxonomy::new(vec![FailureRule {
            matcher: Matcher::any(),
            class: FailureClass::Product,
            limit: 0,
            backoff: Backoff::default(),
        }]);
        assert_eq!(classify_hung(&tax), FailureClass::Infra);
    }

    #[test]
    fn classify_hung_honors_a_workflow_rule_that_matches_the_stale_signal() {
        use crate::failure::{Backoff, FailureRule, Matcher};
        use regex::Regex;
        // A workflow can reclassify a hung spawn by matching the stale signal's text.
        let tax = Taxonomy::new(vec![FailureRule {
            matcher: Matcher {
                exit_status: None,
                signal: None,
                output_regex: Some(Regex::new("liveness marker stale").unwrap()),
            },
            class: FailureClass::Flaky,
            limit: 3,
            backoff: Backoff::default(),
        }]);
        assert_eq!(classify_hung(&tax), FailureClass::Flaky);
    }

    // --- Sweep / hung-spawns tests (a synthetic stale marker, the done-when pin) ---

    use crate::conductor::STREAM;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, EventStore};
    use crate::spawn::{self, park, SpawnRequest, ROLE_IMPLEMENTER};

    /// Park a spawn carrying a wall-clock bound, so the sweep considers it.
    fn park_bounded(store: &Store, unit: &str, secs: u64) -> SpawnRequest {
        let mut req = SpawnRequest::new(unit, unit, ROLE_IMPLEMENTER, 0, "task");
        req.max_wall_clock = Some(secs);
        park(store, &req).unwrap();
        req
    }

    fn read(store: &Store) -> Vec<Event> {
        store.read_stream(STREAM, 0, Direction::Forward).unwrap()
    }

    /// The run id every sweep test scopes its markers under (the run-identity subdir).
    const TEST_RUN: &str = "r1";

    /// Plant a synthetic liveness marker (under the test run's subdir) touched "now" - its
    /// mtime is the wall-clock at creation. The sweep's `now` parameter is advanced past the
    /// bound to make it stale, so no mtime manipulation is needed.
    fn plant_marker(root: &str, id: &str) {
        let path = marker_path(root, TEST_RUN, id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"heartbeat").unwrap();
    }

    #[test]
    fn sweep_classifies_a_stale_marker_as_infra_records_it_and_charges_no_attempt() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        let store = Store::open(":memory:").unwrap();

        // An in-flight spawn with a 300s wall-clock bound and a SYNTHETIC marker touched
        // "now"; the sweep is run at now+400s, so the marker is 400s stale past the bound.
        let hung = park_bounded(&store, "hung-unit", 300);
        plant_marker(root, &hung.id);

        let events = read(&store);
        let taxonomy = Taxonomy::default();
        let now = SystemTime::now() + Duration::from_secs(400);
        let stale = sweep(&store, &events, root, TEST_RUN, &taxonomy, now).unwrap();

        // Classified infra and returned.
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, hung.id);
        assert_eq!(stale[0].class, FailureClass::Infra);

        // Recorded on the spawn's id as a liveness fault (existing SpawnResult type).
        let after = read(&store);
        let res = spawn::result_of(&after, &hung.id).unwrap().unwrap();
        assert!(
            res.is_liveness_fault(),
            "recorded a liveness fault on the spawn id"
        );
        assert_eq!(res.liveness_class(), "infra");
        assert!(
            res.is_error(),
            "a hung spawn's fault carries a describing error"
        );

        // NO attempt charged: the sweep records NO UnitFailed - only the SpawnResult, and
        // NO new event TYPE was introduced (only SpawnRequested + SpawnResult exist).
        let types: std::collections::BTreeSet<&str> =
            after.iter().map(|e| e.type_.as_str()).collect();
        assert!(
            !types.contains(crate::ledger::TYPE_UNIT_FAILED),
            "a hung spawn charges no remediation attempt (no UnitFailed)"
        );
        assert_eq!(
            types,
            [spawn::TYPE_SPAWN_REQUESTED, spawn::TYPE_SPAWN_RESULT]
                .into_iter()
                .collect(),
            "no new event type - the fault rides SpawnResult on the spawn id"
        );
    }

    #[test]
    fn sweep_leaves_a_fresh_marker_and_a_missing_marker_alone() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        let store = Store::open(":memory:").unwrap();

        // Fresh marker (alive): touched now, and the sweep runs only 10s later.
        let alive = park_bounded(&store, "alive-unit", 300);
        plant_marker(root, &alive.id);

        // No marker at all (never started touching): left alone, conservative.
        let _no_marker = park_bounded(&store, "no-marker-unit", 300);

        let events = read(&store);
        let now = SystemTime::now() + Duration::from_secs(10);
        let stale = sweep(&store, &events, root, TEST_RUN, &Taxonomy::default(), now).unwrap();
        assert!(
            stale.is_empty(),
            "a fresh marker and a missing marker are not hung"
        );
        assert!(
            spawn::result_of(&read(&store), &alive.id)
                .unwrap()
                .is_none(),
            "no fault is recorded for a live spawn"
        );
    }

    #[test]
    fn sweep_ignores_a_spawn_without_a_wall_clock_bound() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        let store = Store::open(":memory:").unwrap();

        // No max_wall_clock: unbounded, exempt from liveness timeouts (back-compat). Its
        // marker is planted "now" but the sweep runs far in the future - still not stale.
        let unbounded = SpawnRequest::new("u", "u", ROLE_IMPLEMENTER, 0, "task");
        park(&store, &unbounded).unwrap();
        plant_marker(root, &unbounded.id);

        let events = read(&store);
        let now = SystemTime::now() + Duration::from_secs(99_999);
        let stale = sweep(&store, &events, root, TEST_RUN, &Taxonomy::default(), now).unwrap();
        assert!(
            stale.is_empty(),
            "an unbounded spawn is never timed out, however old its marker"
        );
    }

    #[test]
    fn hung_spawns_surfaces_the_fault_until_a_real_result_supersedes_it() {
        let store = Store::open(":memory:").unwrap();
        let hung = park_bounded(&store, "u", 300);

        // Record a liveness fault directly (as the sweep would).
        let fault = SpawnResult::liveness_fault(&hung.id, "hung", "infra");
        spawn::record_result(&store, &fault).unwrap();
        let surfaced = hung_spawns(&read(&store)).unwrap();
        assert_eq!(surfaced.len(), 1);
        assert_eq!(surfaced[0].id, hung.id);
        assert_eq!(surfaced[0].unit, "u");
        assert_eq!(surfaced[0].class, "infra");

        // A real result recorded LATER (last-write-wins) supersedes the fault - recovered.
        spawn::record_result(&store, &SpawnResult::ok(&hung.id, "recovered output")).unwrap();
        assert!(
            hung_spawns(&read(&store)).unwrap().is_empty(),
            "a real result supersedes the liveness fault; the spawn is no longer hung"
        );
    }

    #[test]
    fn classify_stale_returns_only_the_stale_spawns_classified() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let bound = Duration::from_secs(300);
        let in_flight = vec![
            // Hung: silent for 400s past a 300s bound.
            InFlightSpawn {
                id: "u/implementer#0".into(),
                unit: "u".into(),
                last_seen: now - Duration::from_secs(400),
                max_wall_clock: bound,
            },
            // Alive: touched 10s ago.
            InFlightSpawn {
                id: "v/implementer#0".into(),
                unit: "v".into(),
                last_seen: now - Duration::from_secs(10),
                max_wall_clock: bound,
            },
        ];
        let stale = classify_stale(&in_flight, &Taxonomy::default(), now);
        assert_eq!(stale.len(), 1, "only the hung spawn is returned");
        assert_eq!(stale[0].id, "u/implementer#0");
        assert_eq!(stale[0].class, FailureClass::Infra);
        assert_eq!(stale[0].silent_for, Duration::from_secs(400));
    }
}
