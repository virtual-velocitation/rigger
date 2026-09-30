//! Machine-global instance registry (spec 50): discovery metadata ONLY.
//!
//! Every `rigger` invocation that starts or advances a run registers its instance here, so a
//! single machine-level dash can discover every local project's runs (and any configured
//! shared store) without a coordination protocol - "see any instance" becomes a discovery
//! problem, not a dash-to-dash protocol. An entry is pure discovery metadata: the project
//! identity, the project root, a CREDENTIAL-FREE store identity, and a heartbeat the live
//! instance refreshes while it works.
//!
//! One entry holds TWO facts, because a process that DRIVES a run and a one-shot COURIER verb
//! both write the same file: discovery (the project is active, so a dash lists it), which every
//! writer refreshes, and driver liveness (a process that can dispatch a spawn or land a unit is
//! alive), which only a driver stamps. [`Writer`] records which kind of process wrote an entry.
//!
//! The registry is NEVER a source of truth and NEVER holds a credential (spec 50 secrets
//! discipline); its loss is harmless, because live instances repopulate it as they heartbeat. A
//! dead instance's entry ages out and is eventually deleted - but pruning is reserved to the
//! machine-level self-reap watcher's own read tick ([`read_live`]), never to any of the several
//! OTHER concurrent readers ([`read_live_no_prune`]): a prune outside that one ordered sequence
//! could delete a foreign project's still-relevant entry before the watcher has ever observed its
//! root (spec 62 criterion 5 round 4), so every other reader filters staleness without deleting.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The registry's default staleness window (spec 50): an entry whose heartbeat has not been
/// refreshed within this many milliseconds counts as stale. Mirrors the dash's own idle bound
/// (900s) so a live run's normal between-step gap is never read as gone. Staleness alone does
/// NOT mean prunable by whoever happens to read it - deletion is reserved to the self-reap
/// watcher's own tick ([`read_live`]); every other reader filters by this same window through
/// [`read_live_no_prune`] without deleting anything. Both readers take the window as a
/// parameter so a caller may tune it.
pub const DEFAULT_IDLE_MS: u64 = 900_000;

/// A CREDENTIAL-FREE description of the event store an instance reports to. This is discovery
/// metadata: the dash uses it to LABEL an instance and, through the store-resolution authority,
/// to re-open that store read-only - it never carries the credential that opens a shared store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoreIdentity {
    /// The embedded sqlite event log at this local filesystem path (a project's
    /// `.rigger/events.db`). Local by construction, so it holds no credential.
    Local { path: String },
    /// A shared server backend addressed by this credential-free endpoint (`scheme://host:port`,
    /// with any `user:password@` userinfo and any `?query` stripped by the crate's single
    /// redaction authority - [`crate::eventstore::endpoint_label`] - before it is ever persisted).
    Shared { endpoint: String },
}

/// Which kind of process last wrote an [`Instance`] entry. A driver and a courier of the same
/// project write the SAME entry file (its [`id`](Instance::id) keys on the root and store
/// alone), so the entry carries two facts that must never be read as one:
///   - DISCOVERY: the project is active on this machine, so a dash lists it. Every writer
///     refreshes it through [`Instance::heartbeat_ms`].
///   - DRIVER LIVENESS: a process that drives the run - one that can dispatch a spawn or land a
///     unit at any moment - is alive. Only a [`Writer::Driver`] stamps it.
///
/// A consumer asking whether a run may be advancing right now must read the driver fact: a
/// courier that merely recorded an event would otherwise read as a live driver for the whole
/// idle window after it exited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Writer {
    /// A process that drives the run for as long as it lives - `rigger run`, `serve` and
    /// `workflow`, and `rigger step` - re-stamping from a heartbeat thread while it holds the
    /// run, so its `heartbeat_ms` is also its driver-liveness stamp. The DEFAULT, and what an
    /// entry written before the role was recorded reads as: an unknown writer is taken for a
    /// live driver, which can only delay a dead-driver judgment by the idle window, never
    /// approve one.
    #[default]
    Driver,
    /// A one-shot courier verb (`emit`, `result`, `progress`, `hook stop-failure`) re-stamping
    /// discovery: it records that the project is active and drives nothing.
    Courier {
        /// The last heartbeat (unix-epoch ms) a DRIVER stamped this entry with, or `None` when
        /// no driver ever did. [`write`] carries it forward from the entry already on disk, so a
        /// courier's re-stamp never erases the record of a driver that is still heartbeating.
        driver_heartbeat_ms: Option<u64>,
    },
}

/// One registered rigger instance: a project reporting to a resolved store, with a heartbeat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    /// The project identity that scopes its event streams (the stream-namespace token) - a
    /// stable, opinion-free label the dash shows alongside the root.
    pub project: String,
    /// The project root directory (absolute path), so the dash can name the instance by where it
    /// lives on disk.
    pub root: String,
    /// The credential-free store this instance reports to.
    pub store: StoreIdentity,
    /// Unix-epoch milliseconds of the last DISCOVERY heartbeat, from whichever [`Writer`] wrote
    /// last. A live instance refreshes it every time it starts or advances a run; an entry whose
    /// heartbeat is older than the idle window is stale (see [`is_stale`]) - deleted only by the
    /// self-reap watcher's own [`read_live`] tick, and merely filtered out (never deleted) by
    /// every other reader via [`read_live_no_prune`]. It is not, on its own, a driver's liveness
    /// stamp: see [`Writer`].
    pub heartbeat_ms: u64,
    /// Which kind of process wrote this entry last. An entry written before the role was
    /// recorded has no such field and reads as [`Writer::Driver`].
    #[serde(default)]
    pub writer: Writer,
}

impl Instance {
    /// The registry FILE stem for this instance: a deterministic token over the project root and
    /// its store identity, so the same project reporting to the same store always writes ONE
    /// entry (a re-registration refreshes the heartbeat in place instead of accumulating
    /// duplicates), while two projects - or one project across two stores - never collide. The
    /// heartbeat is deliberately NOT part of the id, so a refresh keeps the same file.
    pub fn id(&self) -> String {
        format!("{:016x}", fnv1a_64(self.identity_key().as_bytes()))
    }

    /// The bytes the [`id`](Self::id) hashes: the root and the store identity, separated by a NUL
    /// so `(root, store)` boundaries can never alias across two different pairs.
    fn identity_key(&self) -> String {
        let store = match &self.store {
            StoreIdentity::Local { path } => format!("local:{path}"),
            StoreIdentity::Shared { endpoint } => format!("shared:{endpoint}"),
        };
        format!("{}\0{}", self.root, store)
    }

    /// The last heartbeat a DRIVER stamped this entry with: its own `heartbeat_ms` when a driver
    /// wrote it last, the carried stamp when a courier did, `None` when no driver ever did.
    fn driver_heartbeat_ms(&self) -> Option<u64> {
        match self.writer {
            Writer::Driver => Some(self.heartbeat_ms),
            Writer::Courier {
                driver_heartbeat_ms,
            } => driver_heartbeat_ms,
        }
    }
}

/// FNV-1a offset basis / prime - the same 64-bit constants the rest of the crate hashes with.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over `bytes`: a fast, dependency-free hash for the registry file stem (a stable id, not
/// a security digest).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The machine-global state directory that roots the instance registry, honoring the platform
/// convention with a first-class override: `$XDG_STATE_HOME` when set, else `$HOME/.local/state`
/// (the freedesktop base-directory default). Returns `None` only in a truly homeless environment
/// (neither variable resolvable), so the caller can degrade to no registration - the registry's
/// loss is harmless. Honoring `XDG_STATE_HOME` is also what lets a test redirect the whole
/// registry into a temp dir with no process-global fs writes.
pub fn state_home() -> Option<PathBuf> {
    state_home_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

/// The PURE core of [`state_home`] over explicit `XDG_STATE_HOME` / `HOME` values, so the
/// precedence (XDG wins; an empty value is "unset") is unit-tested with no process-global env
/// mutation - which would race the crate's other multi-threaded tests that read the environment.
/// `pub(crate)` (not just test-visible) so any OTHER site in the crate that needs this same
/// precedence over its own explicitly-read env values - [`crate::gate::default_cache_dir`] is
/// the first - composes it directly instead of reaching for the ambient-reading [`state_home`]
/// and duplicating this logic behind it.
pub fn state_home_from(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(dir) = xdg.filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = home.filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join(".local").join("state"))
}

/// The instances directory under a given state home: `<state_home>/rigger/instances`. The single
/// place the registry's on-disk location is composed, so a reader and a writer never disagree.
pub fn instances_dir(state_home: &Path) -> PathBuf {
    state_home.join("rigger").join("instances")
}

/// The default instances directory from the ambient environment, or `None` in a homeless
/// environment (registration then degrades to a no-op - the registry's loss is harmless).
pub fn default_dir() -> Option<PathBuf> {
    state_home().map(|h| instances_dir(&h))
}

/// Unix-epoch milliseconds now, for stamping a heartbeat. Saturates at 0 before the epoch so it
/// never panics on a mis-set clock.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A per-writer nonce that makes every temp file name unique WITHIN this process. Combined with the
/// process id (unique ACROSS processes), it guarantees two concurrent writers of the SAME entry id
/// never share a temp path. See [`write`] for why that matters.
static TMP_NONCE: AtomicU64 = AtomicU64::new(0);

/// Write (or refresh) `inst`'s entry in the registry directory `dir`, creating the directory if
/// needed. The final file name is `inst.id()`, so a re-registration of the same project+store
/// OVERWRITES its own entry - refreshing the heartbeat in place rather than accumulating
/// duplicates. Returns the entry path. The write is atomic (write-temp-then-rename) so a
/// concurrent reader never observes a half-written entry.
///
/// The temp file name is PER-WRITER unique (`.{id}.{pid}.{nonce}.tmp`), not the shared `.{id}.tmp`:
/// two writers of the SAME id can now run concurrently - a run driver's periodic heartbeat thread
/// (§50) overlapping a `rigger step`, or two `rigger` invocations for one project+store - and a
/// shared temp path would let one writer's `rename` fire while the other's `write` was mid-flight,
/// so the second `rename` would hit a temp its peer already moved away (`ENOENT`) and fail the
/// write. A unique temp per writer removes the shared name they could race on; both renames target
/// the same final path, which is itself an atomic last-writer-wins swap. A reader only ever reads
/// `*.json`, so the `.tmp` files are invisible to it (and a crashed writer's leftover temp likewise).
///
/// A COURIER's write carries forward the driver heartbeat the entry on disk already holds
/// ([`carry_driver_heartbeat`]), so couriers re-stamping discovery never erase the record of a
/// driver that is still heartbeating. The read and the rename are not one atomic step: a
/// courier that reads just before a driver's write and renames just after it carries that
/// driver's previous stamp - at most one heartbeat interval old, still inside the idle window -
/// or, against a driver's very first write, none, until that driver's next heartbeat.
pub fn write(dir: &Path, inst: &Instance) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let id = inst.id();
    let path = dir.join(format!("{id}.json"));
    let inst = carry_driver_heartbeat(inst, read_entry(&path).as_ref());
    let body = serde_json::to_vec_pretty(&inst)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let pid = std::process::id();
    let nonce = TMP_NONCE.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{id}.{pid}.{nonce}.tmp"));
    std::fs::write(&tmp, &body)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// `inst` as [`write`] lands it over `on_disk`, the entry already at its id: a driver's entry as
/// given, a courier's carrying the LATER of its own and `on_disk`'s driver heartbeat - so a
/// courier's discovery re-stamp never erases the stamp of a driver that is, or recently was,
/// alive.
fn carry_driver_heartbeat(inst: &Instance, on_disk: Option<&Instance>) -> Instance {
    let writer = match inst.writer {
        Writer::Driver => Writer::Driver,
        Writer::Courier {
            driver_heartbeat_ms,
        } => Writer::Courier {
            driver_heartbeat_ms: driver_heartbeat_ms
                .max(on_disk.and_then(Instance::driver_heartbeat_ms)),
        },
    };
    Instance {
        writer,
        ..inst.clone()
    }
}

/// Whether a heartbeat is stale relative to `now_ms` and an idle window `ttl_ms`: it has not been
/// refreshed within the window, so a reader may prune it. A heartbeat in the FUTURE (clock skew)
/// is never stale - `saturating_sub` floors the age at 0.
pub fn is_stale(heartbeat_ms: u64, now_ms: u64, ttl_ms: u64) -> bool {
    now_ms.saturating_sub(heartbeat_ms) > ttl_ms
}

/// One parsed registry entry, paired with the file it was read from - the file path is needed
/// only by [`read_live`]'s prune step, so [`read_all`] discards it and keeps just the [`Instance`].
struct ParsedEntry {
    path: PathBuf,
    inst: Instance,
}

/// The entry at `path`, or `None` when it is absent, unreadable or unparseable - the registry's
/// loss is harmless, so a bad entry reads as no entry, never as an error.
fn read_entry(path: &Path) -> Option<Instance> {
    let body = std::fs::read(path).ok()?;
    serde_json::from_slice(&body).ok()
}

/// Parse every `.json` entry currently under `dir`, applying NO staleness filter and pruning
/// NOTHING - the shared read+parse core behind both [`read_live`] (which additionally prunes
/// stale entries) and [`read_all`] (which returns everything as-is). An absent directory yields
/// no entries (no error); a non-`.json` sibling, an unreadable file, or an unparseable body are
/// each skipped, never fatal - the registry's loss is harmless. Order is unspecified.
fn parse_entries(dir: &Path) -> Vec<ParsedEntry> {
    let mut parsed = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return parsed; // absent dir => empty registry
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(inst) = read_entry(&path) else {
            continue;
        };
        parsed.push(ParsedEntry { path, inst });
    }
    parsed
}

/// Read every LIVE instance from `dir`, PRUNING (deleting) any entry whose heartbeat is stale past
/// `ttl_ms` (spec 50's original "entries whose heartbeat goes stale are pruned by any reader",
/// narrowed by spec 62 criterion 5 round 4: pruning is safe ONLY from the self-reap watcher's own
/// tick, where it is guaranteed to run after that SAME tick's [`read_all`] - use
/// [`read_live_no_prune`] anywhere else). An absent directory is an empty registry (no error).
/// Unreadable or unparseable entries are skipped, never fatal - the registry's loss is harmless.
/// Live entries are returned; their order is unspecified.
pub fn read_live(dir: &Path, now_ms: u64, ttl_ms: u64) -> Vec<Instance> {
    let mut live = Vec::new();
    for entry in parse_entries(dir) {
        if is_stale(entry.inst.heartbeat_ms, now_ms, ttl_ms) {
            let _ = std::fs::remove_file(&entry.path); // prune stale, best-effort
            continue;
        }
        live.push(entry.inst);
    }
    live
}

/// Read every LIVE instance from `dir`, filtering by heartbeat freshness exactly like
/// [`read_live`] but WITHOUT the prune side effect - no entry, stale or not, is ever deleted
/// (spec 62 criterion 5 round 4, `adv-u62c5r4-known-roots-prune-race-with-instances-provider`).
///
/// `read_live`'s delete is safe ONLY from the self-reap watcher's own tick, where it always runs
/// immediately after that SAME tick's [`read_all`] has already captured every currently-registered
/// root into `known_roots` - so a prune there can never outrun the one place `known_roots` is ever
/// seeded from. Every OTHER concurrent reader of the registry (a `/api/instances` landing poll, an
/// attach resolve, the `reset --derived` live-writer check) runs on its OWN schedule, with no such
/// ordering guarantee relative to the watcher's tick: if one of them pruned a foreign project's
/// stale-heartbeat-but-agent-still-live entry before the watcher's `read_all` had EVER captured
/// that root, the root would be gone from disk and permanently unreachable - excluding that
/// project from the cross-project agent-liveness check for the rest of the singleton's lifetime,
/// even though its agent liveness marker stays fresh. This function is what those three call sites
/// use instead: the SAME freshness filter, with deletion reserved exclusively for the watcher's own
/// `read_all`-then-`read_live` sequence. An absent directory is an empty registry (no error);
/// unreadable or unparseable entries are skipped, never fatal, matching `read_live`. Order is
/// unspecified.
pub fn read_live_no_prune(dir: &Path, now_ms: u64, ttl_ms: u64) -> Vec<Instance> {
    parse_entries(dir)
        .into_iter()
        .filter(|e| !is_stale(e.inst.heartbeat_ms, now_ms, ttl_ms))
        .map(|e| e.inst)
        .collect()
}

/// Read EVERY registered instance currently present under `dir`, regardless of heartbeat
/// freshness - unlike [`read_live`], this applies NO staleness filter and PRUNES NOTHING (spec
/// 62 criterion 5 round 2, cross-project agent liveness). The dash's self-reap watcher needs to
/// find every registered project's OWN scratch root even once that project's registry HEARTBEAT
/// has itself aged out - the same courier-cadence-lapse gap criterion 5 already closes for the
/// launching project alone - and [`read_live`]'s freshness filter (plus its prune side effect)
/// would silently exclude, and then permanently forget, exactly the entry this check exists to
/// find. An absent directory is an empty registry (no error); an unreadable or unparseable entry
/// is skipped, never fatal, matching [`read_live`]. Order is unspecified.
pub fn read_all(dir: &Path) -> Vec<Instance> {
    parse_entries(dir).into_iter().map(|e| e.inst).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(root: &str, path: &str, hb: u64) -> Instance {
        Instance {
            project: "proj".to_string(),
            root: root.to_string(),
            store: StoreIdentity::Local {
                path: path.to_string(),
            },
            heartbeat_ms: hb,
            writer: Writer::Driver,
        }
    }

    /// Register `inst` in a fresh instances dir, then read it back through `read`: the entry is
    /// returned verbatim exactly when `returned` (`why`), and its file survives the read
    /// exactly when `kept`.
    fn assert_read_back(
        inst: Instance,
        read: impl Fn(&Path) -> Vec<Instance>,
        returned: bool,
        kept: bool,
        why: &str,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        let path = write(&dir, &inst).expect("write");
        assert!(path.exists(), "entry file exists after write");
        let expected = if returned { vec![inst] } else { vec![] };
        assert_eq!(read(&dir), expected, "{why}");
        assert_eq!(
            path.exists(),
            kept,
            "{why}: the entry file is kept exactly when expected"
        );
    }

    crate::test_cases! {
        write_then_read_round_trips_a_live_entry: assert_read_back(
            local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 1_000),
            |dir| read_live(dir, 1_000, DEFAULT_IDLE_MS),
            true,
            true,
            "the live reader returns the entry verbatim",
        );
        /// now is well past the idle window from the heartbeat at 0 => stale => pruned: the
        /// stale entry is not returned, and its file is pruned from disk by the reader.
        a_reader_prunes_a_stale_heartbeat: assert_read_back(
            local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 0),
            |dir| read_live(dir, DEFAULT_IDLE_MS + 1, DEFAULT_IDLE_MS),
            false,
            false,
            "the stale entry is not returned",
        );
        /// Spec 62 criterion 5 round 4 (adv-u62c5r4-known-roots-prune-race-with-instances-provider):
        /// `read_live`'s prune side effect is only safe from the self-reap watcher's OWN
        /// read_all-then-read_live tick, which is the sole place `known_roots` is ever seeded
        /// from. Every OTHER concurrent reader (a `/api/instances` poll, an attach resolve, the
        /// `reset --derived` live-writer check) must filter the SAME staleness bound WITHOUT
        /// deleting - otherwise one of those reads can win a race against the watcher's own
        /// first tick and permanently erase a foreign project's only route into `known_roots`,
        /// even though that project's own agent is still live. Well past the idle window,
        /// `read_live` would prune this outright; deletion is reserved exclusively for the
        /// watcher's own read_live tick.
        read_live_no_prune_filters_a_stale_heartbeat_but_never_deletes_its_file: assert_read_back(
            local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 0),
            |dir| read_live_no_prune(dir, DEFAULT_IDLE_MS + 1, DEFAULT_IDLE_MS),
            false,
            true,
            "a stale entry is filtered from the returned set, exactly like read_live",
        );
        read_live_no_prune_still_returns_a_fresh_entry: assert_read_back(
            local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 1_000),
            |dir| read_live_no_prune(dir, 1_500, DEFAULT_IDLE_MS),
            true,
            true,
            "a within-window heartbeat is returned verbatim, same as read_live",
        );
        /// Spec 62 criterion 5 round 2: `read_all` is the non-pruning, non-filtering sibling of
        /// `read_live` - a project whose registry heartbeat has aged out (the exact
        /// courier-cadence-lapse gap criterion 5 exists to survive) must still be discoverable by
        /// its `root`, not silently dropped the way `read_live`'s freshness filter would. Well
        /// past the idle window, `read_live` would prune this entry outright; `read_all` must
        /// never delete an entry, stale or not.
        read_all_returns_a_stale_entry_read_live_would_have_pruned_and_never_deletes_it:
            assert_read_back(
                local("/home/dev/proj-b", "/home/dev/proj-b/.rigger/events.db", 0),
                read_all,
                true,
                true,
                "a stale-heartbeat entry is still returned verbatim by read_all",
            );
    }

    /// Projects `/a` and `/b` registered with heartbeats `a_hb` and `b_hb` are both returned by
    /// `read`, as two distinct entries (`why`).
    fn assert_both_roots_read(
        a_hb: u64,
        b_hb: u64,
        read: impl Fn(&Path) -> Vec<Instance>,
        why: &str,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        write(&dir, &local("/a", "/a/.rigger/events.db", a_hb)).unwrap();
        write(&dir, &local("/b", "/b/.rigger/events.db", b_hb)).unwrap();
        let mut roots: Vec<String> = read(&dir).into_iter().map(|i| i.root).collect();
        roots.sort();
        assert_eq!(roots, vec!["/a".to_string(), "/b".to_string()], "{why}");
    }

    crate::test_cases! {
        two_projects_get_distinct_entries: assert_both_roots_read(
            5,
            5,
            |dir| read_live(dir, 5, DEFAULT_IDLE_MS),
            "two projects get distinct entries",
        );
        read_all_returns_every_registered_root_regardless_of_freshness: assert_both_roots_read(
            0,
            u64::MAX,
            read_all,
            "read_all returns a fresh AND a hopelessly stale entry alike",
        );
    }

    #[test]
    fn reregistration_updates_one_entry_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        let root = "/home/dev/proj";
        let db = "/home/dev/proj/.rigger/events.db";

        let p1 = write(&dir, &local(root, db, 1_000)).unwrap();
        let p2 = write(&dir, &local(root, db, 2_000)).unwrap();
        assert_eq!(p1, p2, "same project+store => same file (idempotent id)");

        let live = read_live(&dir, 2_000, DEFAULT_IDLE_MS);
        assert_eq!(live.len(), 1, "no duplicate entries pile up");
        assert_eq!(
            live[0].heartbeat_ms, 2_000,
            "the heartbeat is refreshed in place"
        );
    }

    #[test]
    fn a_fresh_heartbeat_survives_the_reader() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        let inst = local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 1_000);
        let path = write(&dir, &inst).unwrap();
        let live = read_live(&dir, 1_500, DEFAULT_IDLE_MS);
        assert_eq!(live.len(), 1, "a within-window heartbeat survives");
        assert!(path.exists(), "and its file is left on disk");
    }

    #[test]
    fn absent_directory_is_an_empty_registry_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path()).join("does-not-exist");
        assert!(read_live(&dir, 0, DEFAULT_IDLE_MS).is_empty());
    }

    #[test]
    fn read_all_over_an_absent_directory_is_an_empty_registry_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path()).join("does-not-exist");
        assert!(read_all(&dir).is_empty());
    }

    #[test]
    fn is_stale_treats_a_future_heartbeat_as_live() {
        // Clock skew: a heartbeat stamped in the future is never stale.
        assert!(!is_stale(2_000, 1_000, 10));
        assert!(!is_stale(1_000, 1_000, 0), "exactly-now is live");
        assert!(is_stale(0, 11, 10), "aged past the window is stale");
        assert!(!is_stale(0, 10, 10), "at the window boundary is still live");
    }

    #[test]
    fn a_shared_entry_persists_no_credential() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        // The wiring derives the entry's endpoint through the crate's SINGLE redaction authority
        // (`eventstore::endpoint_label`), never a registry-local parser; assert end-to-end that the
        // value it hands `write` - and thus what lands on disk - carries no credential, even for a
        // malformed conn whose password hides an unencoded delimiter before its `@` (the leak GROUND
        // 1 closed: a naive parse would have persisted the `admin:hun` head).
        let endpoint = crate::eventstore::endpoint_label(
            "kurrentdb://admin:hun/ter2@db.example:2113?tls=true",
        );
        let inst = Instance {
            project: "proj".to_string(),
            root: "/home/dev/proj".to_string(),
            store: StoreIdentity::Shared { endpoint },
            heartbeat_ms: 1_000,
            writer: Writer::Driver,
        };
        let path = write(&dir, &inst).unwrap();
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(
            !on_disk.contains("hunter2") && !on_disk.contains("hun") && !on_disk.contains("admin"),
            "no credential fragment is ever written to the registry entry: {on_disk}"
        );
        assert!(
            on_disk.contains("db.example:2113"),
            "the credential-free endpoint is retained"
        );
    }

    #[test]
    fn state_home_prefers_xdg_over_home() {
        use std::ffi::OsString;
        // XDG set => it wins outright.
        assert_eq!(
            state_home_from(
                Some(OsString::from("/xdg/state")),
                Some(OsString::from("/home/dev"))
            ),
            Some(PathBuf::from("/xdg/state"))
        );
        assert_eq!(
            instances_dir(&PathBuf::from("/xdg/state")),
            PathBuf::from("/xdg/state/rigger/instances")
        );
        // XDG unset (or empty) => the freedesktop default under HOME.
        assert_eq!(
            state_home_from(None, Some(OsString::from("/home/dev"))),
            Some(PathBuf::from("/home/dev/.local/state"))
        );
        assert_eq!(
            state_home_from(Some(OsString::new()), Some(OsString::from("/home/dev"))),
            Some(PathBuf::from("/home/dev/.local/state")),
            "an empty XDG_STATE_HOME is treated as unset"
        );
        // Truly homeless => None, so the caller degrades to no registration.
        assert_eq!(state_home_from(None, None), None);
    }

    /// A courier's writer role with no driver heartbeat of its own to carry.
    fn courier() -> Writer {
        Writer::Courier {
            driver_heartbeat_ms: None,
        }
    }

    /// Both writer roles survive the registry verbatim, in the on-disk shape a reader of any
    /// version parses: `{"kind":"driver"}` and `{"kind":"courier","driver_heartbeat_ms":..}`.
    #[test]
    fn both_writer_roles_round_trip_through_the_registry() {
        let carrying = Writer::Courier {
            driver_heartbeat_ms: Some(7),
        };
        for (writer, wire) in [
            (Writer::Driver, serde_json::json!({"kind": "driver"})),
            (
                carrying,
                serde_json::json!({"kind": "courier", "driver_heartbeat_ms": 7}),
            ),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let dir = instances_dir(tmp.path());
            let inst = Instance {
                writer,
                ..local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 1_000)
            };
            let path = write(&dir, &inst).unwrap();
            assert_eq!(read_all(&dir), vec![inst], "{writer:?} reads back verbatim");
            let on_disk: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(
                on_disk["writer"], wire,
                "{writer:?} lands in its wire shape"
            );
        }
    }

    /// COMPATIBILITY PIN: an entry written before the role was recorded has no `writer` field.
    /// It must parse, and parse as a DRIVER - an unknown writer only delays a dead-driver
    /// judgment by the idle window, never approves one.
    #[test]
    fn an_entry_without_a_writer_parses_as_a_driver() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("0000000000000001.json"),
            r#"{"project":"proj","root":"/home/dev/proj","store":{"kind":"local","path":"/home/dev/proj/.rigger/events.db"},"heartbeat_ms":1000}"#,
        )
        .unwrap();
        let entries = read_all(&dir);
        assert_eq!(entries.len(), 1, "the writer-less entry still parses");
        assert_eq!(entries[0].writer, Writer::Driver);
    }

    /// Writes `first` then `second` (the same project and store, so the same entry file) and
    /// asserts the one entry left is re-stamped at the second write's heartbeat and carries
    /// `writer` (`why`).
    fn assert_second_write_lands(
        first: (Writer, u64),
        second: (Writer, u64),
        writer: Writer,
        why: &str,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        let entry = |(writer, hb): (Writer, u64)| Instance {
            writer,
            ..local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", hb)
        };
        write(&dir, &entry(first)).unwrap();
        write(&dir, &entry(second)).unwrap();
        let entries = read_all(&dir);
        assert_eq!(entries.len(), 1, "{why}: one entry per project and store");
        assert_eq!(
            entries[0].heartbeat_ms, second.1,
            "{why}: discovery is re-stamped"
        );
        assert_eq!(entries[0].writer, writer, "{why}");
    }

    crate::test_cases! {
        /// A courier re-stamping a live driver's entry refreshes discovery and keeps the
        /// driver's stamp.
        a_courier_write_carries_a_live_drivers_heartbeat: assert_second_write_lands(
            (Writer::Driver, 1_000),
            (courier(), 2_000),
            Writer::Courier { driver_heartbeat_ms: Some(1_000) },
            "a courier over a live driver",
        );
        /// A courier over a courier keeps the driver stamp the first one carried.
        a_courier_write_keeps_a_carried_driver_heartbeat: assert_second_write_lands(
            (Writer::Courier { driver_heartbeat_ms: Some(1_000) }, 1_500),
            (courier(), 2_000),
            Writer::Courier { driver_heartbeat_ms: Some(1_000) },
            "a courier over a courier that carried a driver stamp",
        );
        /// A courier whose entry no driver ever stamped records discovery only.
        a_courier_only_entry_carries_no_driver_heartbeat: assert_second_write_lands(
            (courier(), 1_000),
            (courier(), 2_000),
            courier(),
            "couriers alone",
        );
        /// A driver dead past the idle window keeps its old stamp under a later courier's
        /// re-stamp, never a fresh one.
        a_courier_write_over_a_stale_driver_carries_its_stale_heartbeat: assert_second_write_lands(
            (Writer::Driver, 0),
            (courier(), DEFAULT_IDLE_MS + 1),
            Writer::Courier { driver_heartbeat_ms: Some(0) },
            "a courier over a stale driver",
        );
        /// A driver's write is its own fresh stamp whatever a courier wrote before it.
        a_driver_write_restamps_a_courier_entry: assert_second_write_lands(
            (courier(), 1_000),
            (Writer::Driver, 2_000),
            Writer::Driver,
            "a driver over a courier",
        );
    }
}
