//! Machine-global instance registry (spec 50): discovery metadata ONLY.
//!
//! Every `rigger` invocation that starts or advances a run registers its instance here, so a
//! single machine-level dash can discover every local project's runs (and any configured
//! shared store) without a coordination protocol - "see any instance" becomes a discovery
//! problem, not a dash-to-dash protocol. An entry is pure discovery metadata: the project
//! identity, the project root, a CREDENTIAL-FREE store identity, and a heartbeat the live
//! instance refreshes while it works.
//!
//! The registry is NEVER a source of truth and NEVER holds a credential (spec 50 secrets
//! discipline); its loss is harmless, because live instances repopulate it as they heartbeat.
//! Any reader prunes entries whose heartbeat has gone stale - a dead instance's entry ages out
//! on the next read, so nothing has to reap it eagerly.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The registry's default staleness window (spec 50): an entry whose heartbeat has not been
/// refreshed within this many milliseconds is prunable by any reader. Mirrors the dash's own
/// idle bound (900s) so a live run's normal between-step gap is never read as gone; the reader
/// takes the window as a parameter ([`read_live`]) so a caller may tune it.
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
    /// with any `user:password@` userinfo and any `?query` stripped - see [`shared_endpoint`]).
    Shared { endpoint: String },
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
    /// Unix-epoch milliseconds of the last heartbeat. A live instance refreshes it every time it
    /// starts or advances a run; a reader prunes an entry whose heartbeat is older than the idle
    /// window (see [`is_stale`]).
    pub heartbeat_ms: u64,
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

/// Reduce a server connection string to a CREDENTIAL-FREE endpoint safe to persist and print:
/// keep the scheme and `host[:port]`, DROP any `user:password@` userinfo AND any trailing
/// `?query`/`#fragment` (a connection string can hide the credential in either). So
/// `kurrentdb://admin:secret@db.example:2113?tls=true` reduces to `kurrentdb://db.example:2113`.
/// Pure, so the "the registry never holds a credential" invariant is unit-tested without a store.
pub fn shared_endpoint(conn: &str) -> String {
    let conn = conn.trim();
    // Split off the scheme, preserving it in the output (it names the backend).
    let (scheme, rest) = match conn.find("://") {
        Some(i) => (&conn[..i + 3], &conn[i + 3..]),
        None => ("", conn),
    };
    // The authority ends at the first '/', '?', or '#'; everything after (path, query, fragment)
    // is dropped - a query string is a common place to smuggle a credential (`?password=...`).
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    // Drop any "userinfo@" prefix (`user`, or `user:password`) - the credential. The host has no
    // '@', so the LAST '@' is the userinfo boundary.
    let host_port = match authority.rfind('@') {
        Some(i) => &authority[i + 1..],
        None => authority,
    };
    format!("{scheme}{host_port}")
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
fn state_home_from(
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

/// Write (or refresh) `inst`'s entry in the registry directory `dir`, creating the directory if
/// needed. The file name is `inst.id()`, so a re-registration of the same project+store
/// OVERWRITES its own entry - refreshing the heartbeat in place rather than accumulating
/// duplicates. Returns the entry path. The write is atomic (write-temp-then-rename) so a
/// concurrent reader never observes a half-written entry.
pub fn write(dir: &Path, inst: &Instance) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let body = serde_json::to_vec_pretty(inst)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let id = inst.id();
    let path = dir.join(format!("{id}.json"));
    let tmp = dir.join(format!(".{id}.tmp"));
    std::fs::write(&tmp, &body)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Whether a heartbeat is stale relative to `now_ms` and an idle window `ttl_ms`: it has not been
/// refreshed within the window, so a reader may prune it. A heartbeat in the FUTURE (clock skew)
/// is never stale - `saturating_sub` floors the age at 0.
pub fn is_stale(heartbeat_ms: u64, now_ms: u64, ttl_ms: u64) -> bool {
    now_ms.saturating_sub(heartbeat_ms) > ttl_ms
}

/// Read every LIVE instance from `dir`, PRUNING (deleting) any entry whose heartbeat is stale past
/// `ttl_ms` - "entries whose heartbeat goes stale are pruned by any reader" (spec 50). An absent
/// directory is an empty registry (no error). Unreadable or unparseable entries are skipped, never
/// fatal - the registry's loss is harmless. Live entries are returned; their order is unspecified.
pub fn read_live(dir: &Path, now_ms: u64, ttl_ms: u64) -> Vec<Instance> {
    let mut live = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return live; // absent dir => empty registry
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = std::fs::read(&path) else {
            continue;
        };
        let Ok(inst) = serde_json::from_slice::<Instance>(&body) else {
            continue;
        };
        if is_stale(inst.heartbeat_ms, now_ms, ttl_ms) {
            let _ = std::fs::remove_file(&path); // prune stale, best-effort
            continue;
        }
        live.push(inst);
    }
    live
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
        }
    }

    #[test]
    fn write_then_read_round_trips_a_live_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        let inst = local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 1_000);

        let path = write(&dir, &inst).expect("write");
        assert!(path.exists(), "entry file exists after write");

        let live = read_live(&dir, 1_000, DEFAULT_IDLE_MS);
        assert_eq!(
            live,
            vec![inst],
            "the live reader returns the entry verbatim"
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
    fn two_projects_get_distinct_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        write(&dir, &local("/a", "/a/.rigger/events.db", 5)).unwrap();
        write(&dir, &local("/b", "/b/.rigger/events.db", 5)).unwrap();
        let mut roots: Vec<String> = read_live(&dir, 5, DEFAULT_IDLE_MS)
            .into_iter()
            .map(|i| i.root)
            .collect();
        roots.sort();
        assert_eq!(roots, vec!["/a".to_string(), "/b".to_string()]);
    }

    #[test]
    fn a_reader_prunes_a_stale_heartbeat() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        let inst = local("/home/dev/proj", "/home/dev/proj/.rigger/events.db", 0);
        let path = write(&dir, &inst).unwrap();

        // now is well past the idle window from the heartbeat at 0 => stale => pruned.
        let live = read_live(&dir, DEFAULT_IDLE_MS + 1, DEFAULT_IDLE_MS);
        assert!(live.is_empty(), "the stale entry is not returned");
        assert!(
            !path.exists(),
            "the stale entry file is pruned from disk by the reader"
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
    fn is_stale_treats_a_future_heartbeat_as_live() {
        // Clock skew: a heartbeat stamped in the future is never stale.
        assert!(!is_stale(2_000, 1_000, 10));
        assert!(!is_stale(1_000, 1_000, 0), "exactly-now is live");
        assert!(is_stale(0, 11, 10), "aged past the window is stale");
        assert!(!is_stale(0, 10, 10), "at the window boundary is still live");
    }

    #[test]
    fn shared_endpoint_strips_userinfo_and_query_credentials() {
        assert_eq!(
            shared_endpoint("kurrentdb://admin:secret@db.example:2113?tls=true"),
            "kurrentdb://db.example:2113",
            "userinfo AND query are stripped, scheme+host:port kept"
        );
        assert_eq!(
            shared_endpoint("esdb+discover://user@cluster.internal:2113"),
            "esdb+discover://cluster.internal:2113",
            "a bare user (no password) is still stripped"
        );
        assert_eq!(
            shared_endpoint("kurrentdb://db.example:2113"),
            "kurrentdb://db.example:2113",
            "an already-credential-free endpoint is unchanged"
        );
        assert_eq!(
            shared_endpoint("kurrentdb://host:2113/stream?user=u&password=p"),
            "kurrentdb://host:2113",
            "a credential smuggled after the path is dropped with the path"
        );
    }

    #[test]
    fn a_shared_entry_persists_no_credential() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instances_dir(tmp.path());
        // Register with the credential-free endpoint the wiring derives via shared_endpoint.
        let endpoint = shared_endpoint("kurrentdb://admin:hunter2@db.example:2113?tls=true");
        let inst = Instance {
            project: "proj".to_string(),
            root: "/home/dev/proj".to_string(),
            store: StoreIdentity::Shared { endpoint },
            heartbeat_ms: 1_000,
        };
        let path = write(&dir, &inst).unwrap();
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(
            !on_disk.contains("hunter2") && !on_disk.contains("admin"),
            "no credential is ever written to the registry entry: {on_disk}"
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
}
