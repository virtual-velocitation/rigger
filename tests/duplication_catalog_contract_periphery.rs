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
//!
//! SPEC 90 CRITERION 2 ACCOUNTING (decision `sdet-u90c2-surface-accounting`, correction
//! `sdet-u90c2-claim1-subsumed-by-roundtrip`): CLAIM 1 ("the guarded catalog carries no line
//! numbers") needs no new test - the pre-existing
//! `deserializing_then_reserializing_the_committed_catalog_reproduces_the_committed_bytes_exactly`
//! below already proves it strictly (any stray key breaks byte-exact re-encoding through a
//! struct that lacks it). Two genuine gaps closed here: (1) `docs/audit/duplication-
//! catalog.lines.json`, spec 90's new unguarded sibling, had ZERO test coverage anywhere -
//! `ConsumedDupClusterLines`/`deserialize_committed_catalog_lines` plus
//! `the_committed_catalog_and_its_lines_sibling_are_position_joined_by_id_and_site_count` close
//! it, joined by cluster id (never bare position - a future edit that reorders clusters should
//! fail loudly by id mismatch, not silently compare the wrong pair). (2) CLAIM 4 ("the report
//! still cites file:line from the unguarded lines file") had only an in-memory, unit-level proof
//! (`report_section_2_cites_file_line_exactly_as_the_lines_sibling_records_them` in
//! `tests/simplification_audit.rs`, against `render_section_2`'s freshly-computed output, never
//! the persisted report) -
//! `the_committed_report_section_2_cites_file_line_exactly_as_the_lines_sibling_records_them_in_order`
//! below closes the periphery half, reading the COMMITTED report and the COMMITTED lines sibling
//! directly. Extraction is bounded to the "### Clusters" span and stops before "### Adversarial
//! sample" - that subsection's own bullets share the identical `` `file:start-end` `name` ``
//! citation shape (same real sites, a different purpose) and would otherwise be misread as
//! belonging to whichever cluster happens to render last.

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

// -----------------------------------------------------------------------------------------
// Spec 90 criterion 2, THE DRIFT GUARD IS LINE-FREE: the unguarded `.lines.json` sibling
// (zero prior coverage) and CLAIM 4 at the true periphery level (the committed report against
// the committed sidecar, never a regenerated value).
// -----------------------------------------------------------------------------------------

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`CATALOG_PATH`]'s line spans, joined to
/// it by cluster `id` (both are lists of clusters in the SAME id-ascending order, but joining by
/// id rather than bare position fails loudly, naming the id, if that ever changes) - never
/// drift-guarded, so not itself a "documented contract" a downstream reader pins against.
const CATALOG_LINES_PATH: &str = "docs/audit/duplication-catalog.lines.json";

/// Mirrors `tests/simplification_audit.rs`'s private `DupSiteLines` shape field-for-field - no
/// `name` field (spec 90 Design: the sidecar carries line data only; a site's identity lives in
/// the guarded file, joined by array position within the cluster).
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ConsumedDupSiteLines {
    file: String,
    start_line: usize,
    end_line: usize,
}

/// Mirrors `tests/simplification_audit.rs`'s private `DupClusterLines` shape field-for-field.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ConsumedDupClusterLines {
    id: String,
    sites: Vec<ConsumedDupSiteLines>,
}

fn deserialize_committed_catalog_lines() -> Vec<ConsumedDupClusterLines> {
    let path = repo_root().join(CATALOG_LINES_PATH);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{CATALOG_LINES_PATH} is missing or unreadable ({e})"));
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("{CATALOG_LINES_PATH} does not deserialize as the documented lines contract: {e}")
    })
}

/// Spec 90 criterion 2: the join holds - every cluster `id` in [`CATALOG_PATH`] has a
/// same-`id` cluster in [`CATALOG_LINES_PATH`] carrying exactly as many sites, in the same
/// per-cluster order (both come from the SAME `Vec<DupCluster>` in the same pass, per the
/// producer's own doc comment - never independently re-sorted).
#[test]
fn the_committed_catalog_and_its_lines_sibling_are_position_joined_by_id_and_site_count() {
    let clusters = deserialize_committed_catalog();
    let lines = deserialize_committed_catalog_lines();
    assert_eq!(
        clusters.len(),
        lines.len(),
        "{CATALOG_PATH} has {} clusters but {CATALOG_LINES_PATH} has {} - they are joined by \
         cluster id and must list the same clusters",
        clusters.len(),
        lines.len()
    );
    for (c, l) in clusters.iter().zip(lines.iter()) {
        assert_eq!(
            c.id, l.id,
            "{CATALOG_PATH} and {CATALOG_LINES_PATH} disagree on cluster order at this \
             position - expected the same id in both"
        );
        assert_eq!(
            c.sites.len(),
            l.sites.len(),
            "cluster {}'s site count disagrees between {CATALOG_PATH} ({}) and \
             {CATALOG_LINES_PATH} ({})",
            c.id,
            c.sites.len(),
            l.sites.len()
        );
        for (i, (cs, ls)) in c.sites.iter().zip(l.sites.iter()).enumerate() {
            assert_eq!(
                &cs.file, &ls.file,
                "cluster {} site {i} disagrees on file between {CATALOG_PATH} and \
                 {CATALOG_LINES_PATH}",
                c.id
            );
        }
    }
}

const REPORT_PATH: &str = "docs/audit/2026-09-simplification-audit.md";

/// One cited site's `(file, start_line, end_line)`.
type CitedSite = (String, usize, usize);

/// Section 2's per-cluster site citations (`render_section_2`'s own template:
/// `` - `{file}:{start}-{end}` `{name}` ``), grouped by `#### \`dup-NNNN\`` cluster header, in
/// report order. Bounded to the "### Clusters" span and cut off before "### Adversarial sample" -
/// that subsection's own bullets share the identical citation shape (the same real sites, read
/// for a different purpose) and would otherwise be misattributed to whichever cluster renders
/// last.
fn section_2_cluster_site_citations(report: &str) -> Vec<(String, Vec<CitedSite>)> {
    let start = report
        .find("### Clusters (")
        .expect("report has a ### Clusters heading");
    let rest = &report[start..];
    let end = rest.find("### Adversarial sample").unwrap_or(rest.len());
    let clusters_text = &rest[..end];
    let header_re = regex::Regex::new(r"(?m)^#### `(dup-\d+)`").expect("valid regex");
    let site_re = regex::Regex::new(r"(?m)^- `([^`]+):(\d+)-(\d+)` `[^`]+`").expect("valid regex");
    let headers: Vec<(usize, String)> = header_re
        .captures_iter(clusters_text)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();
    headers
        .iter()
        .enumerate()
        .map(|(i, (pos, id))| {
            let block_end = headers
                .get(i + 1)
                .map(|(p, _)| *p)
                .unwrap_or(clusters_text.len());
            let block = &clusters_text[*pos..block_end];
            let sites = site_re
                .captures_iter(block)
                .map(|c| {
                    (
                        c[1].to_string(),
                        c[2].parse().expect("digits"),
                        c[3].parse().expect("digits"),
                    )
                })
                .collect();
            (id.clone(), sites)
        })
        .collect()
}

/// CLAIM 4: "the report still cites `file:line` from the unguarded lines file." Every cluster's
/// per-site `(file, start, end)` citation in section 2 of the COMMITTED report matches, in order,
/// the same cluster's sites in the COMMITTED `CATALOG_LINES_PATH` - joined by `id`, never bare
/// position, so a future reordering fails loudly naming the id rather than silently comparing
/// the wrong pair.
#[test]
fn the_committed_report_section_2_cites_file_line_exactly_as_the_lines_sibling_records_them_in_order(
) {
    let report = std::fs::read_to_string(repo_root().join(REPORT_PATH))
        .unwrap_or_else(|e| panic!("{REPORT_PATH} is missing or unreadable ({e})"));
    let cited = section_2_cluster_site_citations(&report);
    assert!(
        !cited.is_empty(),
        "found zero section 2 cluster citations in {REPORT_PATH} - the extraction regex or the \
         section boundary is broken"
    );
    let lines = deserialize_committed_catalog_lines();
    assert_eq!(
        cited.len(),
        lines.len(),
        "section 2 of {REPORT_PATH} renders {} cluster(s) but {CATALOG_LINES_PATH} records {}",
        cited.len(),
        lines.len()
    );
    for (cited_id, sites) in &cited {
        let entry = lines.iter().find(|l| &l.id == cited_id).unwrap_or_else(|| {
            panic!(
                "report section 2 renders cluster {cited_id}, but no cluster with that id \
                     exists in {CATALOG_LINES_PATH}"
            )
        });
        assert_eq!(
            sites.len(),
            entry.sites.len(),
            "cluster {cited_id} renders {} site citation(s) in {REPORT_PATH} but \
             {CATALOG_LINES_PATH} records {}",
            sites.len(),
            entry.sites.len()
        );
        for (i, ((file, start, end), ls)) in sites.iter().zip(entry.sites.iter()).enumerate() {
            assert_eq!(
                (file.as_str(), *start, *end),
                (ls.file.as_str(), ls.start_line, ls.end_line),
                "cluster {cited_id} site {i}: {REPORT_PATH} cites `{file}:{start}-{end}`, but \
                 {CATALOG_LINES_PATH} records `{}:{}-{}`",
                ls.file,
                ls.start_line,
                ls.end_line
            );
        }
    }
}
