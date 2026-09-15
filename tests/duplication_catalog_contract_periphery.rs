//! Spec 85 criterion 2 (u85c2), SDET periphery layer: a serialized-form contract test for
//! `docs/audit/duplication-catalog.json`.
//!
//! Boundary-surface accounting (mechanical probes against base
//! `839a2fbb173adc66fda28e8e7730b727173269da`, see decision `sdet-u85c2-surface-accounting`):
//! new/changed `pub` API, trait impl, CLI surface, and cross-module seam all came back empty
//! (this unit changes no `src/` file at all - `tests/simplification_audit.rs` imports only
//! `std`/`serde`, never `rigger::`/`crate::`). The one item the "event type / serialized form"
//! probe surfaced is two private structs, `DupSite` and `DupCluster`, which both derive BOTH
//! `Serialize` and `Deserialize`. Spec 85's Design says this JSON is "machine-readable so
//! follow-up specs can pin counts and prove reductions" - same intent as criterion 1's
//! `MapEntry` (see `tests/responsibility_map_contract_periphery.rs`), and the same gap: nothing
//! in the unit's own tests ever deserializes the ACTUAL COMMITTED FILE through an
//! INDEPENDENTLY-declared struct. The unit's own two related tests are both self-consistency -
//! `catalog_to_json_round_trips_through_deserialize` round-trips a hand-built, single-cluster,
//! in-memory fixture (never the real file), and
//! `duplication_catalog_json_matches_the_tree_or_is_rewritten` string-compares the committed
//! bytes against `catalog_to_json(real_catalog())` - the SAME producer function on both sides.
//! Neither is the position a real downstream consumer (a later refactor spec reading this file
//! with only spec 85's documented shape, blind to the producer's internal Rust type) is
//! actually in. That is the boundary this file tests.
//!
//! DELIBERATE INDEPENDENCE: this file declares its OWN `ConsumedDupSite`/`ConsumedDupCluster`
//! structs rather than importing the producer's `DupSite`/`DupCluster` - not just because Cargo
//! integration-test binaries each compile as a separate crate and cannot see another test
//! file's private items anyway, but because a real downstream consumer is in exactly this
//! position. Field order below matches the producer's declaration order
//! (`tests/simplification_audit.rs:2134-2150`) exactly, which is also why the round-trip test
//! below can assert byte-identical re-encoding.
//!
//! This unit does NOT own the responsibility map, report sections 1/3-6, or any production code
//! (spec 85: "no production code changes"), so this file drives no binary and spawns no
//! process - the whole surface to prove is the persisted data contract itself.

use serde::Deserialize;
use std::path::PathBuf;

/// Mirrors `tests/simplification_audit.rs`'s private `DupSiteWire` shape field-for-field, from
/// the outside - see the module doc comment for why this is a deliberate re-declaration, not an
/// import. Spec 90 criterion 2: the guarded catalog carries `content_hash`, never a line span -
/// `start_line`/`end_line` moved to the unguarded `docs/audit/duplication-catalog.lines.json`
/// sibling, out of scope for this file (never drift-guarded, so not a "documented contract" a
/// downstream reader pins against).
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
struct ConsumedDupSite {
    file: String,
    name: String,
    content_hash: String,
}

/// Mirrors `tests/simplification_audit.rs`'s private `DupCluster` shape field-for-field.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
struct ConsumedDupCluster {
    id: String,
    classification: String,
    sites: Vec<ConsumedDupSite>,
    proposed_home: String,
    note: String,
}

const CATALOG_PATH: &str = "docs/audit/duplication-catalog.json";

/// The five mandatory sweeps, in the fixed order spec 85 Design names them - mirrors the
/// producer's private `MANDATORY_SWEEPS` constant (`tests/simplification_audit.rs:2309-2315`).
/// Independently redeclared for the same reason as the structs above: a downstream consumer
/// has only spec 85's documented names, not the producer's private constant.
const MANDATORY_SWEEPS: [&str; 5] = [
    "Command::new call sites",
    "/proc-path string literals",
    "sqlite Connection::open call sites",
    ".rigger-path string literals",
    "error-shaping helper functions",
];

/// The repo root this test binary was compiled from - never the process CWD (same convention
/// as `tests/simplification_audit.rs::repo_root`, `tests/responsibility_map_contract_periphery.rs`,
/// and `tests/no_os_kill_audit.rs`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_committed_catalog_raw() -> String {
    let path = repo_root().join(CATALOG_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{CATALOG_PATH} is missing or unreadable ({e})"))
}

fn deserialize_committed_catalog() -> Vec<ConsumedDupCluster> {
    let raw = read_committed_catalog_raw();
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!(
            "{CATALOG_PATH} does not deserialize as the documented DupCluster contract \
             (id/classification/sites/proposed_home/note, sites as \
             file/name/content_hash): {e}"
        )
    })
}

/// THE ROUND-TRIP PROOF: a downstream consumer who only has spec 85's documented field shape
/// (not the producer's private Rust type) can actually parse the committed artifact. This is
/// the specific gap the boundary probe found - `Deserialize` is derived but the unit's own
/// tests only ever exercise it against a synthetic fixture, never the real committed file.
#[test]
fn the_committed_duplication_catalog_deserializes_as_a_downstream_consumer_would() {
    let clusters = deserialize_committed_catalog();
    assert!(
        !clusters.is_empty(),
        "{CATALOG_PATH} deserialized to zero clusters - a downstream consumer pinning counts \
         against this file (spec 85: 'follow-up specs can pin counts and prove reductions') \
         would silently see nothing"
    );
}

/// Spec 85 Design: "every cluster of two or more sites" with one of the three named
/// classifications and "the ONE proposed home". A consumer reading a cluster to schedule a
/// refactor must never see a singleton, an unrecognized classification, or a blank home.
#[test]
fn every_deserialized_cluster_has_two_or_more_sites_a_recognized_classification_and_a_non_empty_home(
) {
    let clusters = deserialize_committed_catalog();
    for c in &clusters {
        assert!(
            c.sites.len() >= 2,
            "cluster {} has {} site(s), a duplicate needs >= 2",
            c.id,
            c.sites.len()
        );
        assert!(
            matches!(c.classification.as_str(), "exact" | "near" | "semantic"),
            "cluster {} has an unrecognized classification {:?}",
            c.id,
            c.classification
        );
        assert!(
            !c.proposed_home.is_empty(),
            "cluster {} has no proposed home",
            c.id
        );
        assert!(!c.note.is_empty(), "cluster {} has no note", c.id);
    }
}

/// Every site's identity is one a reader can act on: a non-empty file, function name, and
/// content_hash - the LINE-FREE contract spec 90 criterion 2 establishes (no `start_line`/
/// `end_line` at all: an extra field would silently fail to deserialize into this
/// independently-declared struct, making the round-trip test below the real proof of their
/// absence; this test pins presence and non-emptiness of what DOES remain).
#[test]
fn every_deserialized_site_has_a_non_empty_file_name_and_content_hash() {
    let clusters = deserialize_committed_catalog();
    for c in &clusters {
        for s in &c.sites {
            assert!(
                !s.file.is_empty(),
                "cluster {} has a site with an empty file",
                c.id
            );
            assert!(
                !s.name.is_empty(),
                "cluster {} has a site in {} with an empty name",
                c.id,
                s.file
            );
            assert!(
                !s.content_hash.is_empty(),
                "cluster {} has a site {:?} ({}) with an empty content_hash",
                c.id,
                s.name,
                s.file
            );
        }
    }
}

/// Spec 85 Design's `dup-NNNN` id scheme, checked against the PERSISTED file rather than the
/// generator's in-memory ids (`real_cluster_ids_are_unique_and_ascending` in
/// `tests/simplification_audit.rs` only ever checks the freshly-computed value). A consumer
/// that parses the numeric suffix (e.g. to sort, or to generate the NEXT id for a manually
/// added cluster) needs the exact `dup-` prefix plus 4 zero-padded digits, not merely "looks
/// like ascending strings".
#[test]
fn cluster_ids_in_the_committed_catalog_are_unique_ascending_and_dup_nnnn_formatted() {
    let clusters = deserialize_committed_catalog();
    let ids: Vec<&str> = clusters.iter().map(|c| c.id.as_str()).collect();
    let distinct: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        ids.len(),
        "duplicate cluster id in {CATALOG_PATH}"
    );
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "{CATALOG_PATH} cluster ids are not already in ascending order"
    );
    for (i, id) in ids.iter().enumerate() {
        let digits = id.strip_prefix("dup-").unwrap_or_else(|| {
            panic!("cluster id {id:?} does not start with the documented \"dup-\" prefix")
        });
        assert_eq!(
            digits.len(),
            4,
            "cluster id {id:?} does not carry exactly 4 digits after \"dup-\""
        );
        assert!(
            digits.chars().all(|c| c.is_ascii_digit()),
            "cluster id {id:?} has a non-digit after \"dup-\""
        );
        let n: usize = digits
            .parse()
            .unwrap_or_else(|e| panic!("cluster id {id:?} digits do not parse as a number: {e}"));
        assert_eq!(
            n,
            i + 1,
            "cluster id {id:?} at position {i} is not sequential (expected dup-{:04})",
            i + 1
        );
    }
}

/// THE MANDATORY SWEEPS, checked against the PERSISTED file (the implementer's own
/// `every_mandatory_sweep_appears_with_at_least_one_site_on_the_real_tree` only ever checks the
/// freshly-computed `real_catalog()`, never the committed bytes). Spec 85 Design: each of the
/// five sweeps is collected "unconditionally regardless of what the Jaccard pass finds" into
/// ONE dedicated cluster - so a consumer relying on "exactly one cluster per sweep name" (e.g.
/// to look up that sweep's site count directly) must find exactly one, not zero, not a split
/// across several, and it must be a `semantic` cluster (sweep_cluster always assigns this
/// classification - a consumer distinguishing "similarity found" from "mechanically swept"
/// clusters depends on this). Also checks the note's own embedded "N site(s)" count against the
/// actual `sites` array length - a human (or a future parser) reading only the note text must
/// never be told a stale count.
#[test]
fn every_mandatory_sweep_appears_as_exactly_one_semantic_cluster_in_the_committed_catalog() {
    let clusters = deserialize_committed_catalog();
    for name in MANDATORY_SWEEPS {
        let prefix = format!("mandatory sweep: {name} - ");
        let matches: Vec<&ConsumedDupCluster> = clusters
            .iter()
            .filter(|c| c.note.starts_with(&prefix))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "mandatory sweep {name:?} should appear as exactly 1 cluster in {CATALOG_PATH}, \
             found {}",
            matches.len()
        );
        let cluster = matches[0];
        assert_eq!(
            cluster.classification, "semantic",
            "mandatory sweep {name:?}'s cluster {} is classified {:?}, expected \"semantic\"",
            cluster.id, cluster.classification
        );
        assert!(
            !cluster.sites.is_empty(),
            "mandatory sweep {name:?}'s cluster {} has 0 sites on the real tree",
            cluster.id
        );
        let claimed = cluster
            .note
            .strip_prefix(&prefix)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or_else(|| {
                panic!(
                    "cluster {}'s note {:?} does not start with a parseable site count after \
                     {prefix:?}",
                    cluster.id, cluster.note
                )
            });
        assert_eq!(
            claimed,
            cluster.sites.len(),
            "cluster {}'s note claims {claimed} site(s) but sites carries {}",
            cluster.id,
            cluster.sites.len()
        );
    }
}

/// THE BACK-COMPAT / STABILITY PROOF: deserializing the committed file into this independently
/// declared struct and re-serializing it (same field order, `serde_json::to_string_pretty` plus
/// the producer's own trailing-newline convention, per `catalog_to_json`) reproduces the
/// committed bytes exactly. This is the strongest form of the round-trip contract - it proves
/// the JSON shape is lossless and canonical from an outside reader's perspective, not merely
/// that the producer's own function agrees with itself (both of the implementer's own
/// round-trip-adjacent tests compare the SAME producer type/function on both sides; this test
/// decodes and re-encodes through a SEPARATELY-declared type, the position any real future
/// consumer will be in).
#[test]
fn deserializing_then_reserializing_the_committed_catalog_reproduces_the_committed_bytes_exactly() {
    let committed = read_committed_catalog_raw();
    let clusters = deserialize_committed_catalog();
    let mut reencoded =
        serde_json::to_string_pretty(&clusters).expect("ConsumedDupCluster re-serializes");
    reencoded.push('\n');
    assert_eq!(
        committed, reencoded,
        "{CATALOG_PATH} does not round-trip byte-for-byte through the documented DupCluster \
         shape - a downstream consumer decoding and re-encoding this file would silently \
         diverge from the committed artifact"
    );
}
