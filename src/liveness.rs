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

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

use crate::eventstore::{Error, Event, EventStore};
use crate::failure::{FailureClass, Signal, Taxonomy};
use crate::spawn::{self, SpawnResult};

/// The scratch subdirectory the per-spawn liveness markers live under, a sibling of the
/// worktrees and `agent-scratch`. Kept in ONE place so the sweep and the driver-framed
/// worker instruction (`workflows/rigger.js`) derive the same path.
pub const MARKER_SUBDIR: &str = "agent-live";

/// The filesystem-safe marker filename for a spawn id: an INJECTIVE byte-for-byte
/// encoding (spec 77 Design, decision `d77-injective-scratch-naming`) - every BYTE that
/// is not ASCII alphanumeric or `-` becomes `_` followed by its two-lowercase-hex-digit
/// value, and `_` itself is escaped the same way (to `_5f`), so a literal `_` never
/// appears in the output except as the first byte of a 3-byte escape. A spawn id is
/// `{unit}/{role}#{n}`, so `/` becomes `_2f` and `#` becomes `_23`. `workflows/rigger.js`
/// never recomputes this rule itself - it receives the fully-resolved absolute path
/// `rigger step` stamps on the wire (through THIS function, via [`marker_path`]) and just
/// touches it verbatim, so the worker and the sweep can never compute two different
/// filenames for the same spawn.
///
/// INJECTIVE BY CONSTRUCTION, not by guard: every caller of this function derives a
/// filesystem path with `<registered_root>.join(marker_filename(id))` (this module's own
/// [`marker_path`], plus [`crate::driver::replay::spawn_scratch_path`] and
/// [`crate::driver::replay::mutation_scratch_path`]), and several of those roots are
/// later reaped with a bare `remove_dir_all` - so two DISTINCT ids must never produce the
/// SAME encoded name (a same-level collision misattributes a reap to the wrong unit's
/// live scratch), and no id may encode to `""`, `"."`, or `".."` (a `PathBuf::join`
/// no-op or an upward walk that lets a reaper delete a sibling or an ancestor). This
/// encoding is injective: since `_` is NEVER emitted as a bare passthrough byte (it is
/// always escaped to `_5f`), a decoder scanning left to right can unambiguously tell a
/// literal allowed byte from the start of a 3-byte escape - so distinct inputs can never
/// collapse onto the same output, and since only alphanumerics and `-` ever appear bare,
/// no encoded output can ever consist entirely of `.` characters either. Three earlier
/// rounds instead tried substituting a FIXED placeholder for each degenerate shape one
/// at a time (`"_empty_"` for an empty mapped result, mapping an all-dots result's own
/// dots to `_`) - both wrong the same way: a placeholder drawn from the map's own
/// non-injective output alphabet can never be proven disjoint from a real id's own
/// mapped output (a real unit literally named `"_empty_"`, or any underscore-run,
/// collided with one of them). This encoding has no such alphabet-reuse hazard because
/// the alphabet itself makes every output unique to its input.
///
/// The ONE remaining degenerate shape - the EMPTY input, which encodes to the EMPTY
/// string (there are no bytes to escape) - still returns `None` rather than `Some("")`:
/// an empty string still makes a caller's `.join` a no-op. It is reachable via
/// `reclaim_spawn_scratch`'s own `spawn_id.split('/').next()` unit-extraction, which
/// yields `""` for any leading-slash spawn id. Every caller skips a `None`, mirroring
/// this module's own established idiom for uncertainty elsewhere in the same call chain
/// ([`sweep`]: "a spawn with NO marker is left alone").
pub fn marker_filename(spawn_id: &str) -> Option<String> {
    let mut encoded = String::with_capacity(spawn_id.len());
    for byte in spawn_id.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("_{byte:02x}"));
        }
    }
    if encoded.is_empty() {
        None
    } else {
        Some(encoded)
    }
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
///
/// Returns `None` when `run_id` or `spawn_id` maps to the ONE remaining degenerate
/// [`marker_filename`] shape (an empty INPUT - never a non-empty one, since the injective
/// encoding gives every non-empty input its own unique, never-empty output), so every
/// caller treats a degenerate id exactly like the existing "marker absent" no-op
/// ([`sweep`]'s own doc comment: "a spawn with NO marker is left alone") instead of
/// stat-ing or touching a fabricated placeholder path.
pub fn marker_path(scratch_root: &str, run_id: &str, spawn_id: &str) -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(scratch_root).join(MARKER_SUBDIR);
    let dir = match marker_filename(run_id) {
        Some(safe) => dir.join(safe),
        None => dir,
    };
    marker_filename(spawn_id).map(|safe| dir.join(safe))
}

/// Whether ANY per-spawn liveness marker under `scratch_root` - any run, any spawn - has been
/// touched more recently than `max_age` ago (spec 62, criterion 5: the machine-level
/// singleton's self-reap watcher must see agent liveness, not just the instance registry).
///
/// The watcher has no run id or wave to check ONE spawn's marker against - the registry it
/// already polls deliberately captures no per-run inputs (spec 50 retargets the reap trigger
/// at the machine-global registry, not one project's run) - so this generalizes
/// [`marker_path`]'s exact per-spawn lookup into "is anything alive at all" over the SAME
/// [`MARKER_SUBDIR`] convention every other liveness reader walks (`sweep`, `rigger status`,
/// the dash's per-spawn ages): reusing the ONE marker mechanism rather than standing up a
/// second, divergent liveness check. An empty `scratch_root` (no repo, mirroring every other
/// caller's repo-less degrade), an absent marker directory (no agent has ever run here), or
/// any unreadable entry along the way all read as "no live agent" - never an error - the same
/// conservative default [`sweep`]'s own doc names for a spawn with no marker at all.
pub fn any_marker_fresh(scratch_root: &str, now: SystemTime, max_age: Duration) -> bool {
    if scratch_root.is_empty() {
        return false;
    }
    let root = std::path::Path::new(scratch_root).join(MARKER_SUBDIR);
    any_fresh_file_under(&root, now, max_age)
}

/// The recursive freshness walk behind [`any_marker_fresh`]: true iff any REGULAR file under
/// `dir` (searched depth-first, following one level of run-id subdirectory the way
/// [`marker_path`] nests a spawn's marker below `MARKER_SUBDIR`) has an mtime within
/// `max_age` of `now`. A directory that cannot be listed, or a file whose mtime cannot be
/// read, is skipped rather than failing the whole scan - matching this module's established
/// "absent/unreadable degrades to no signal" idiom.
fn any_fresh_file_under(dir: &std::path::Path, now: SystemTime, max_age: Duration) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if any_fresh_file_under(&entry.path(), now, max_age) {
                return true;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if !is_stale(now, mtime, max_age) {
            return true;
        }
    }
    false
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
        // (conservative - see the fn docs); only a present-but-stale marker is hung. A
        // degenerate id ([`marker_path`] returns `None`) is treated identically - the same
        // conservative no-op, never a fabricated path to stat.
        let Some(path) = marker_path(scratch_root, run_id, &req.id) else {
            continue;
        };
        let last_seen = match std::fs::metadata(path).and_then(|m| m.modified()) {
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

/// The persisted CROSSING BOUNDARY for the hung-liveness half of the push-side `attention`
/// signal (spec 69, criterion 5; review u69c5 round 3, cause genuine-defect): which hung
/// spawn ids `rigger step` has ALREADY surfaced, as of the end of the PREVIOUS invocation.
///
/// Every other piece of state `rigger step` uses is re-derived fresh from the event log on
/// every invocation - the log is the single source of truth, and no other cross-process
/// state exists anywhere in this codebase's conductor path. This is the one deliberate
/// exception, forced by what "once per crossing" needs here specifically: the fault that
/// makes a spawn HUNG is not always recorded by the process that goes on to surface it. A
/// driver's out-of-band `rigger result --error` (the outer-wall-clock backstop, spec 19c
/// unit 2, for a spawn with no per-spawn `max_wall_clock` the in-process sweep can never
/// time out) runs as a wholly separate process strictly BETWEEN two `rigger step`
/// invocations. By the time the NEXT invocation opens the store, that write already
/// predates every read it could possibly take - so "this process's own start" is always too
/// late a boundary; a fault recorded that way is indistinguishable, from inside a fresh
/// process, from one this run already reported steps ago. The one boundary that IS early
/// enough is "the end of the PREVIOUS `rigger step` invocation" - and only a value persisted
/// OUTSIDE the log, by that previous process, for this one to read, can supply it. This is
/// the exact role [`crate::dash::DashMarker`] already plays for "is a dash already serving
/// on this machine" - a cross-process fact the append-only log cannot answer either, so a
/// small on-disk record is the established escape hatch, not a new one.
///
/// One id per line (spawn ids never contain a newline), written in the caller's sorted
/// order so two folds of the same state produce a byte-identical file. Scoped by `run_id` -
/// a SIBLING of the run's own marker subdir ([`MARKER_SUBDIR`]`/<run>/`), never a file
/// inside it, so it can never collide with a sanitized spawn-id marker filename regardless
/// of what a workflow names its units. Reclaimed with the rest of `agent-live` at run
/// teardown (the same lifecycle a per-spawn marker already has), so it never outlives the
/// run it tracks. An empty `run_id` (a caller outside a run) still produces a stable path,
/// mirroring [`marker_path`]'s own convention for the no-run case - the only degenerate
/// [`marker_filename`] shape under the injective encoding.
pub fn hung_cursor_path(scratch_root: &str, run_id: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(scratch_root).join(MARKER_SUBDIR);
    let name = match marker_filename(run_id) {
        Some(safe) => format!("{safe}.hung-cursor"),
        None => ".hung-cursor".to_string(),
    };
    dir.join(name)
}

/// Read the hung-attention cursor ([`hung_cursor_path`]): the spawn ids already surfaced as
/// of the end of the PREVIOUS `rigger step`. Absent, unreadable, or malformed all read as
/// EMPTY - "nothing surfaced yet" - which is the safe default in both directions this can be
/// wrong: correct on the very first step of a run (nothing has surfaced), and on any read
/// failure the safe direction is toward one extra, harmless re-notification of a still-true
/// halt, never toward silently swallowing a genuine new crossing (the exact failure mode
/// this mechanism exists to close - see [`hung_cursor_path`]'s own doc comment).
pub fn read_hung_cursor(scratch_root: &str, run_id: &str) -> BTreeSet<String> {
    std::fs::read_to_string(hung_cursor_path(scratch_root, run_id))
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Persist the hung-attention cursor ([`hung_cursor_path`]): the FULL set of currently-hung
/// spawn ids, as computed at the end of THIS `rigger step`, for the NEXT invocation's
/// [`read_hung_cursor`] to seed its own crossing check from. Unconditionally overwritten
/// every step (cheap - a handful of short ids) rather than only on a change, so a recovered
/// spawn drops out of the file the same step it drops out of [`hung_spawns`] and a later
/// re-hang of the same id is correctly treated as fresh. Best-effort at the call site, like
/// every other scratch write `rigger step` performs - a failed write only means the NEXT
/// step may re-stamp a still-true halt once more than strictly necessary, never that this
/// step fails.
pub fn write_hung_cursor(
    scratch_root: &str,
    run_id: &str,
    hung: &BTreeSet<String>,
) -> std::io::Result<()> {
    let path = hung_cursor_path(scratch_root, run_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, hung.iter().cloned().collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_filename_hex_escapes_every_byte_outside_alphanumeric_and_hyphen() {
        // Spec 77 Design, decision `d77-injective-scratch-naming`: `/` and `#` (the
        // spawn-id-structure characters) each become a 3-byte `_XX` escape.
        assert_eq!(
            marker_filename("unit-3-spawns-a-wall-clock/implementer#1"),
            Some("unit-3-spawns-a-wall-clock_2fimplementer_231".to_string())
        );
        // Every byte outside [A-Za-z0-9-] escapes - space, colon, `/`, `#`, `.`, and `_`
        // itself (`_` -> `_5f`, never left bare) - while `-` and alphanumerics survive.
        assert_eq!(
            marker_filename("a b:c/d#e.f-g_h"),
            Some("a_20b_3ac_2fd_23e_2ef-g_5fh".to_string())
        );
    }

    #[test]
    fn marker_filename_hex_escapes_dots_so_no_encoded_result_can_ever_be_a_path_traversal_component(
    ) {
        // `cmd_result`'s positional spawn id is otherwise unvalidated (only checked
        // non-empty). Under the PRIOR char-by-char map (rounds 1-6), `.` passed through
        // unescaped, so a spawn id of literally ".." resolved, unchanged, to a traversal
        // component: `<registered_root>.join("..")` walked UP to the root's parent, and
        // `reap_then_remove_dir`'s bare `remove_dir_all` deleted everything beside it. The
        // injective encoding (`d77-injective-scratch-naming`) escapes `.` like any other
        // disallowed byte (`_2e`), so no encoded output can ever contain a literal `.`
        // character at all - a dotted input is just an ordinary, uniquely-encoded id now,
        // never a traversal shape.
        assert_eq!(marker_filename(".."), Some("_2e_2e".to_string()));
        assert_eq!(marker_filename("."), Some("_2e".to_string()));
        assert_eq!(marker_filename("..."), Some("_2e_2e_2e".to_string()));
        assert_eq!(marker_filename("a.b"), Some("a_2eb".to_string()));
        assert_eq!(
            marker_filename("u/implementer#0.retry"),
            Some("u_2fimplementer_230_2eretry".to_string())
        );
    }

    #[test]
    fn marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op() {
        // The ONE degenerate shape the injective encoding does not close structurally: an
        // EMPTY input encodes to the EMPTY string (there are no bytes to escape), and
        // `<registered_root>.join("")` is a documented `PathBuf::join` no-op that collapses
        // the derived path to the registered root itself - letting a reaper delete every
        // sibling leaf alongside it. Reachable directly: a spawn id of `""` cannot reach
        // here (`cmd_result` already requires non-empty), but `reclaim_spawn_scratch`'s own
        // `unit = spawn_id.split('/').next()` extraction yields `""` for any LEADING-SLASH
        // spawn id (e.g. `rigger result "/foo" "text"`), and that empty `unit` is exactly
        // what `mutation_scratch_path` feeds this function. `None` (skip entirely) is the
        // answer here - never a fixed placeholder (rounds 3 and 5 each tried exactly that
        // for this and the all-dots shape, and round 6 proved a placeholder drawn from the
        // map's own output alphabet can collide with a real id; see
        // `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`).
        assert_eq!(marker_filename(""), None);
    }

    #[test]
    fn marker_filename_hex_escapes_a_literal_underscore_so_a_real_underscore_run_unit_never_collides(
    ) {
        // Round-6 REQUIRED FIX regression: a real unit literally named an underscore-run of
        // ANY length collided with rounds 3 and 5's fixed placeholders under the OLD scheme,
        // which passed `_` through unescaped. The injective encoding escapes `_` itself too
        // (to `_5f`, always 3 bytes, never bare) - so a real underscore-run id now encodes
        // to its own unique filename, and an unrelated all-dots id of the IDENTICAL length
        // (the exact round-6 collision shape: `.` escapes to `_2e`, not `_5f`) can never
        // produce the same encoded output.
        assert_eq!(marker_filename("_"), Some("_5f".to_string()));
        assert_eq!(marker_filename("__"), Some("_5f_5f".to_string()));
        assert_eq!(marker_filename("___"), Some("_5f_5f_5f".to_string()));
        for len in [1usize, 3, 7, 12] {
            let underscores = "_".repeat(len);
            let dots = ".".repeat(len);
            assert_ne!(
                marker_filename(&underscores),
                marker_filename(&dots),
                "an underscore-run and an all-dots id of the same length ({len}) must never \
                 encode to the same filename"
            );
        }
    }

    #[test]
    fn marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do(
    ) {
        // Rounds 3 and 5 each substituted a FIXED placeholder for one degenerate shape
        // (`"_empty_"` for an empty mapped result; an all-dots result's own dots mapped to
        // `_` for the all-dots shape) - each collided with an unrelated, perfectly ordinary
        // id that happened to equal the placeholder textually (`adv-u77c2r5-empty-sentinel-
        // collides-with-a-literal-unit-id`, `adj-u77c2r6-alldots-vs-underscore-collision-
        // extends-empty-sentinel-gap`). The injective hex encoding (`d77-injective-scratch-
        // naming`) makes every one of these pairs distinguishable now, by construction: `_`
        // is never emitted bare, so distinct inputs can never collapse onto the same output.
        assert_ne!(marker_filename(""), marker_filename("_empty_"));
        assert_ne!(marker_filename("..."), marker_filename("___"));
        assert_ne!(marker_filename(".."), marker_filename("__"));
        assert_ne!(marker_filename("."), marker_filename("_"));
    }

    #[test]
    fn marker_path_is_scratch_root_joined_with_the_run_subdir_and_filename() {
        // With a run id: `<scratch>/agent-live/<run>/<sanitized id>` - the run subdir gives
        // the marker RUN IDENTITY, so a slug-colliding re-run never reads a prior mtime.
        let p = marker_path("/scratch", "run-7", "u/implementer#0");
        assert_eq!(
            p,
            Some(std::path::PathBuf::from(
                "/scratch/agent-live/run-7/u_2fimplementer_230"
            ))
        );
        // An empty run id (a caller outside a run) omits the run subdir - the no-run path.
        let p = marker_path("/scratch", "", "u/implementer#0");
        assert_eq!(
            p,
            Some(std::path::PathBuf::from(
                "/scratch/agent-live/u_2fimplementer_230"
            ))
        );
        // A run id carrying id-structure characters is encoded like a spawn id.
        let p = marker_path("/scratch", "run/7#a", "u/implementer#0");
        assert_eq!(
            p,
            Some(std::path::PathBuf::from(
                "/scratch/agent-live/run_2f7_23a/u_2fimplementer_230"
            ))
        );
    }

    #[test]
    fn marker_path_hex_escapes_a_dotdot_run_or_spawn_id_so_it_can_never_walk_upward() {
        // A `..` run id or spawn id (reachable via the otherwise-unvalidated `rigger
        // result`/courier CLI ids this function's callers key on) must never make
        // `<scratch>.join(marker_filename(id))` resolve to the PARENT of `<scratch>`. The
        // injective encoding closes this structurally rather than by falling back to a
        // no-subdir/no-path special case: `..` encodes to `_2e_2e` - a perfectly ordinary,
        // unique, non-traversal filename - for EITHER position.
        let p = marker_path("/scratch", "..", "u/implementer#0");
        assert_eq!(
            p,
            Some(std::path::PathBuf::from(
                "/scratch/agent-live/_2e_2e/u_2fimplementer_230"
            ))
        );
        assert!(p.unwrap().starts_with("/scratch/agent-live"));
        let p = marker_path("/scratch", "run-7", "..");
        assert_eq!(
            p,
            Some(std::path::PathBuf::from("/scratch/agent-live/run-7/_2e_2e"))
        );
        assert!(p.unwrap().starts_with("/scratch/agent-live/run-7"));
    }

    #[test]
    fn marker_path_is_none_rather_than_collapsing_to_its_registered_root_for_an_empty_spawn_id() {
        // Round-4 sibling of the dotdot regression above: an EMPTY spawn id (reachable via
        // `reclaim_spawn_scratch`'s own `unit = spawn_id.split('/').next()` extraction for a
        // leading-slash spawn id) must not make `<scratch>.join(marker_filename(id))` a no-op
        // that resolves to `<scratch>` itself - which would let a reaper delete every sibling
        // leaf under it, not just the reporting one's. `None` (skip entirely) is the ONE
        // degenerate shape the injective encoding does not close structurally (an empty
        // input encodes to the empty string), so it is still handled explicitly here.
        let p = marker_path("/scratch", "run-7", "");
        assert_eq!(p, None);
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
        let path = marker_path(root, TEST_RUN, id).expect("test ids are never degenerate");
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

    #[test]
    fn hung_cursor_path_is_a_sibling_of_the_run_marker_subdir_never_inside_it() {
        // With a run id: `<scratch>/agent-live/<sanitized run>.hung-cursor` - a FILE
        // alongside the `<run>/` marker directory, never a filename inside it, so it can
        // never collide with a sanitized spawn-id marker regardless of what a workflow
        // names its units.
        let p = hung_cursor_path("/scratch", "run/7#a");
        assert_eq!(
            p,
            std::path::Path::new("/scratch/agent-live/run_2f7_23a.hung-cursor")
        );
        assert_ne!(
            p,
            marker_path("/scratch", "run/7#a", "run/7#a").unwrap(),
            "the cursor file must never collide with any sanitized spawn-id marker path"
        );
        // An empty run id (a caller outside a run) still produces a stable path, mirroring
        // `marker_path`'s own no-run convention.
        let p = hung_cursor_path("/scratch", "");
        assert_eq!(p, std::path::Path::new("/scratch/agent-live/.hung-cursor"));
    }

    #[test]
    fn read_hung_cursor_is_empty_when_absent_and_round_trips_through_write() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();

        // No cursor has ever been written for this run: empty, not an error - the correct
        // reading for the very first step of a run.
        assert!(read_hung_cursor(root, "r1").is_empty());

        // A round trip persists the exact set, and only that run's set - a DIFFERENT run id
        // reads its own (still-empty) cursor, never another run's.
        let mut hung = BTreeSet::new();
        hung.insert("a/implementer#0".to_string());
        hung.insert("b/implementer#0".to_string());
        write_hung_cursor(root, "r1", &hung).unwrap();
        assert_eq!(read_hung_cursor(root, "r1"), hung);
        assert!(
            read_hung_cursor(root, "r2").is_empty(),
            "a cursor is scoped per run id; a sibling run must not see it"
        );

        // Overwriting with a SMALLER set (a recovery) drops the recovered id - the cursor
        // always reflects THIS step's own full hung set, never a sticky union.
        let mut recovered = BTreeSet::new();
        recovered.insert("b/implementer#0".to_string());
        write_hung_cursor(root, "r1", &recovered).unwrap();
        assert_eq!(read_hung_cursor(root, "r1"), recovered);
    }

    #[test]
    fn read_hung_cursor_reads_a_malformed_or_empty_file_as_empty() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        let path = hung_cursor_path(root, "r1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // A blank line and surrounding whitespace never produce a phantom empty-string id.
        std::fs::write(&path, "\n  \n\n").unwrap();
        assert!(
            read_hung_cursor(root, "r1").is_empty(),
            "blank lines must never surface as a phantom hung id"
        );
    }

    #[test]
    fn any_marker_fresh_is_false_for_an_empty_or_absent_scratch_root() {
        let now = SystemTime::now();
        let bound = Duration::from_secs(900);

        // No scratch root at all (a repo-less caller) - the same degrade every other reader
        // in this module gives an empty scratch root.
        assert!(!any_marker_fresh("", now, bound));

        // A real, but never-populated, scratch root - no `agent-live/` dir has ever been
        // created, so there is nothing to find.
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        assert!(!any_marker_fresh(root, now, bound));
    }

    #[test]
    fn any_marker_fresh_finds_a_fresh_marker_nested_under_a_run_id_directory() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        // Mirrors the real shape `marker_path` builds: `<root>/agent-live/<run>/<spawn>`.
        let path = marker_path(root, "run-1", "u1c1/implementer#0").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();

        let now = SystemTime::now();
        assert!(
            any_marker_fresh(root, now, Duration::from_secs(900)),
            "a just-written marker nested under a run-id directory must be found fresh"
        );
    }

    #[test]
    fn any_marker_fresh_is_false_once_every_marker_is_older_than_max_age() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        let path = marker_path(root, "run-1", "u1c1/implementer#0").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();

        // `now` far enough past the marker's real (just-now) mtime that it falls outside a
        // tiny bound - the marker exists but is stale relative to `max_age`.
        let far_future = SystemTime::now() + Duration::from_secs(3600);
        assert!(
            !any_marker_fresh(root, far_future, Duration::from_secs(1)),
            "a marker older than max_age must not read as a live agent signal"
        );
    }

    #[test]
    fn any_marker_fresh_skips_unreadable_entries_without_erroring() {
        // A degenerate marker directory (no run subdir, no spawn file) is simply empty -
        // `any_marker_fresh` must degrade to false, not panic.
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        std::fs::create_dir_all(std::path::Path::new(root).join(MARKER_SUBDIR)).unwrap();
        assert!(!any_marker_fresh(
            root,
            SystemTime::now(),
            Duration::from_secs(900)
        ));
    }
}
