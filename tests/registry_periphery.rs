//! Periphery layer for the machine-global instance registry (spec 50, criterion 2).
//!
//! The registry's `Instance`/`StoreIdentity` JSON is a DURABLE, CROSS-PROCESS, CROSS-VERSION
//! contract: one project's rigger WRITES an entry that a separately-invoked, possibly
//! differently-versioned machine-level dash READS to discover the run. The module's own
//! inside-out tests round-trip through the SAME code version, so a field rename or an enum-tag
//! change moves writer and reader together and stays green while silently breaking every other
//! reader on the machine.
//!
//! These outside-in tests pin the two boundaries a same-version round-trip cannot. First, the
//! on-disk wire format: a frozen byte string a future reader must still parse, and the concrete
//! keys and discriminator a writer must still emit. Second, the dash-facing reader's "loss is
//! harmless, never fatal" contract in the face of a corrupt, foreign, or stale registry directory.
//! They drive only the crate's public `rigger::registry` API - the exact surface an external
//! consumer (the dash) sees.

use rigger::registry::{self, Instance, StoreIdentity, DEFAULT_IDLE_MS};

/// A live heartbeat relative to a chosen `now`, so a freshly-written or frozen entry is never
/// pruned as stale by the reader under test.
const LIVE_NOW: u64 = 1_000;

fn local_instance(project: &str, root: &str, db: &str, heartbeat_ms: u64) -> Instance {
    Instance {
        project: project.to_string(),
        root: root.to_string(),
        store: StoreIdentity::Local {
            path: db.to_string(),
        },
        heartbeat_ms,
    }
}

/// CONTRACT / back-compat: the registry entry's on-disk JSON is stable in BOTH directions for the
/// Local and Shared store identities.
///
/// Reader back-compat: a FROZEN entry byte string (the shape a prior rigger version wrote, and the
/// shape the dash must keep reading) deserializes through the public `read_live` reader into the
/// exact `Instance` - so renaming a field, dropping the `heartbeat_ms` key, or changing the
/// `StoreIdentity` tag representation makes the frozen entry unparseable, the reader drops it, and
/// this test fails. A same-version round-trip cannot catch that; a frozen literal can.
///
/// Writer format: a freshly-written entry's JSON carries the concrete keys and the snake_case
/// `kind` discriminator (`local` / `shared`, never `Local` / `Shared`) an external reader keys on.
#[test]
fn registry_entry_wire_format_is_stable_for_local_and_shared() {
    // --- Reader back-compat: FROZEN entries a future reader must still parse. ---
    let reader_home = tempfile::tempdir().unwrap();
    let dir = registry::instances_dir(reader_home.path());
    std::fs::create_dir_all(&dir).unwrap();

    // A pinned Local entry as written by the current schema. If any field is renamed or the enum
    // tagging changes, this literal stops deserializing and the entry silently vanishes.
    let frozen_local = r#"{
  "project": "acme",
  "root": "/home/dev/acme",
  "store": {
    "kind": "local",
    "path": "/home/dev/acme/.rigger/events.db"
  },
  "heartbeat_ms": 1000
}"#;
    // A pinned Shared entry: the credential-free endpoint the wiring persists.
    let frozen_shared = r#"{
  "project": "acme",
  "root": "/srv/acme",
  "store": {
    "kind": "shared",
    "endpoint": "kurrentdb://db.example:2113"
  },
  "heartbeat_ms": 1000
}"#;
    std::fs::write(dir.join("frozen-local.json"), frozen_local).unwrap();
    std::fs::write(dir.join("frozen-shared.json"), frozen_shared).unwrap();

    let mut live = registry::read_live(&dir, LIVE_NOW, DEFAULT_IDLE_MS);
    assert_eq!(
        live.len(),
        2,
        "both frozen entries deserialize through the reader; a wire-format break would drop one: {live:?}"
    );
    // Order is unspecified, so key each read-back entry by its store variant.
    live.sort_by_key(|i| matches!(i.store, StoreIdentity::Shared { .. }));
    let local = &live[0];
    let shared = &live[1];

    assert_eq!(local.project, "acme");
    assert_eq!(local.root, "/home/dev/acme");
    assert_eq!(local.heartbeat_ms, 1000);
    assert_eq!(
        local.store,
        StoreIdentity::Local {
            path: "/home/dev/acme/.rigger/events.db".to_string()
        },
        "the frozen `local` tag maps to StoreIdentity::Local with its path"
    );

    assert_eq!(shared.project, "acme");
    assert_eq!(shared.root, "/srv/acme");
    assert_eq!(shared.heartbeat_ms, 1000);
    assert_eq!(
        shared.store,
        StoreIdentity::Shared {
            endpoint: "kurrentdb://db.example:2113".to_string()
        },
        "the frozen `shared` tag maps to StoreIdentity::Shared with its endpoint"
    );

    // --- Writer format: a freshly-written entry emits the concrete keys + snake_case tag. ---
    let writer_home = tempfile::tempdir().unwrap();
    let wdir = registry::instances_dir(writer_home.path());

    let local_path = registry::write(
        &wdir,
        &local_instance(
            "acme",
            "/home/dev/acme",
            "/home/dev/acme/.rigger/events.db",
            LIVE_NOW,
        ),
    )
    .expect("write local");
    let local_body = std::fs::read_to_string(&local_path).unwrap();
    for key in ["\"project\"", "\"root\"", "\"store\"", "\"heartbeat_ms\""] {
        assert!(
            local_body.contains(key),
            "the entry JSON must carry the {key} field; got {local_body}"
        );
    }
    assert!(
        local_body.contains("\"kind\"") && local_body.contains("\"local\""),
        "the Local variant is tagged with a snake_case `local` discriminator; got {local_body}"
    );
    assert!(
        !local_body.contains("\"Local\""),
        "the discriminator is snake_case `local`, never the PascalCase variant name; got {local_body}"
    );

    let shared_path = registry::write(
        &wdir,
        &Instance {
            project: "acme".to_string(),
            root: "/srv/acme".to_string(),
            store: StoreIdentity::Shared {
                endpoint: "kurrentdb://db.example:2113".to_string(),
            },
            heartbeat_ms: LIVE_NOW,
        },
    )
    .expect("write shared");
    let shared_body = std::fs::read_to_string(&shared_path).unwrap();
    assert!(
        shared_body.contains("\"kind\"") && shared_body.contains("\"shared\""),
        "the Shared variant is tagged with a snake_case `shared` discriminator; got {shared_body}"
    );
    assert!(
        shared_body.contains("\"endpoint\"") && shared_body.contains("kurrentdb://db.example:2113"),
        "the Shared variant persists its credential-free endpoint; got {shared_body}"
    );
    assert!(
        !shared_body.contains("\"Shared\""),
        "the discriminator is snake_case `shared`, never the PascalCase variant name; got {shared_body}"
    );
}

/// API EDGE: the dash-facing reader survives a corrupt, foreign, or stale registry directory
/// without failing - "the registry's loss is harmless" (spec 50). A partially-written entry, a
/// syntactically-valid-but-wrong-shape entry, a non-`.json` neighbor file, and a stale entry all
/// coexist with one good live entry; `read_live` must return ONLY the live entry, prune the stale
/// one, and leave the entries it cannot understand untouched (never destroying a file that may be a
/// transient partial write from a concurrent writer), all without panicking.
#[test]
fn read_live_skips_corrupt_and_foreign_entries_without_failing() {
    let home = tempfile::tempdir().unwrap();
    let dir = registry::instances_dir(home.path());

    // `now` sits a full idle window past the epoch so a heartbeat of 0 is unambiguously stale
    // while a heartbeat stamped at `now` is unambiguously live.
    let now = DEFAULT_IDLE_MS + 5_000;

    // One good, live entry (heartbeat == now).
    let good_path = registry::write(
        &dir,
        &local_instance("live", "/live", "/live/.rigger/events.db", now),
    )
    .expect("write good");

    // One well-formed but STALE entry (heartbeat == 0), which the reader must prune.
    let stale_path = registry::write(
        &dir,
        &local_instance("dead", "/dead", "/dead/.rigger/events.db", 0),
    )
    .expect("write stale");

    // Junk that must NOT crash the reader and must NOT be destroyed by it:
    //  - a syntactically-broken JSON entry (a half-written file),
    //  - a syntactically-valid JSON entry of the WRONG shape (parses as JSON, not as an Instance),
    //  - a non-`.json` neighbor the reader ignores by extension.
    let corrupt_path = dir.join("corrupt.json");
    std::fs::write(&corrupt_path, b"{ \"project\": \"x\", not valid json ").unwrap();
    let wrong_shape_path = dir.join("wrong-shape.json");
    std::fs::write(&wrong_shape_path, br#"{"unrelated": 5}"#).unwrap();
    let foreign_path = dir.join("README.txt");
    std::fs::write(&foreign_path, b"not a registry entry").unwrap();

    // The reader the dash uses returns EXACTLY the good live entry.
    let live = registry::read_live(&dir, now, DEFAULT_IDLE_MS);
    assert_eq!(
        live.len(),
        1,
        "only the good live entry is returned; junk and stale entries are excluded: {live:?}"
    );
    assert_eq!(live[0].root, "/live", "the surviving entry is the live one");

    // The stale (parseable) entry is pruned from disk; the good one is left.
    assert!(good_path.exists(), "the live entry file is left on disk");
    assert!(
        !stale_path.exists(),
        "the stale entry file is pruned by the reader"
    );

    // The entries the reader cannot understand are left untouched: a corrupt or foreign file may
    // be a transient partial write, so the reader never destroys it.
    assert!(
        corrupt_path.exists(),
        "an unparseable entry is skipped, not deleted"
    );
    assert!(
        wrong_shape_path.exists(),
        "a valid-JSON wrong-shape entry is skipped, not deleted"
    );
    assert!(
        foreign_path.exists(),
        "a non-json neighbor is ignored, not deleted"
    );

    // A second read is idempotent: the corrupt/foreign files still do not surface, and it still
    // does not panic.
    let again = registry::read_live(&dir, now, DEFAULT_IDLE_MS);
    assert_eq!(again.len(), 1, "a re-read is stable: {again:?}");
}
