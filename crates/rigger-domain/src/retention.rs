//! THE CLASSES OF EVENT, decided by type (spec 107): the lists that say which events the log
//! keeps as knowledge, which are a run's episodes and which are perception the tree re-derives.
//!
//! DERIVED is [`crate::ingest::DERIVED_INDEX_TYPES`], the derived index. EPISODIC is
//! [`EPISODIC_TYPES`]. KNOWLEDGE is every other type, the ledger entry
//! [`TYPE_GENERATION_INGESTED`] among them, so this module declares no knowledge list. The lists
//! cite each type's own constant and re-spell no type string.

use serde::{Deserialize, Serialize};

use crate::ingest::DERIVED_INDEX_TYPES;

/// The ledger entry of perception: one file generation an ingest extracted. The log keeps this
/// entry in place of the generation's derived batch, which the tree re-derives.
pub const TYPE_GENERATION_INGESTED: &str = "GenerationIngested";

/// The payload of a [`TYPE_GENERATION_INGESTED`] event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationIngested {
    /// Which half extracted the batch: `gc` (code), `gd` (design documents) or `gw` (the workflow
    /// definition).
    pub prefix: String,
    /// The file's path, relative to the tree's root.
    pub file: String,
    /// The generation of the extracted batch: the content hash of its events.
    pub generation: String,
    /// The object id of the bytes the batch was extracted from, as git prints it, or empty when
    /// the path held no file.
    pub blob: String,
    /// The walk's flag: whether `walk_exclusions` names the entry's identity. It changes only a
    /// parsed `gc` batch.
    pub excluded: bool,
}

impl GenerationIngested {
    /// The entry a [`TYPE_GENERATION_INGESTED`] event's payload `data` records, or why it is not
    /// one. The whole of `data` must be the payload: bytes trailing it are refused.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| format!("{TYPE_GENERATION_INGESTED} payload: {e}"))
    }

    /// The `<prefix>/<file>` identity whose generation the entry records.
    pub fn identity(&self) -> String {
        format!("{}/{}", self.prefix, self.file)
    }
}

/// PERCEPTION: the derived index and the ledger entry that stands for it, the one list every
/// reader that skips perception cites.
pub const PERCEPTION_TYPES: [&str; 5] = [
    DERIVED_INDEX_TYPES[0],
    DERIVED_INDEX_TYPES[1],
    DERIVED_INDEX_TYPES[2],
    DERIVED_INDEX_TYPES[3],
    TYPE_GENERATION_INGESTED,
];

/// EPISODIC: a run's mechanics. No cross-run fold reads these types, and folding one changes
/// neither the live projection nor the fold state: it writes only its `applied` row.
pub const EPISODIC_TYPES: [&str; 16] = [
    crate::spawn::TYPE_SPAWN_REQUESTED,
    crate::contextgraph::TYPE_GATE_VERDICT,
    crate::ledger::TYPE_GATE_PROMOTED,
    crate::ledger::TYPE_GATE_DEMOTED,
    crate::agent::TYPE_UNIT_PROPOSED,
    crate::metrics::TYPE_BLAST_RADIUS_COMPUTED,
    crate::ledger::TYPE_SCOPE_CREEP,
    crate::contextgraph::TYPE_FILE_TOUCHED,
    crate::ledger::TYPE_SPEC_DEFECT,
    crate::ledger::TYPE_MANUAL_REVIEW,
    crate::ledger::TYPE_DEFERRED_GATE_FAILED,
    crate::ledger::TYPE_TASK_ABORTED,
    crate::blocker::TYPE_BUDGET_EXHAUSTED,
    crate::progress::TYPE_AGENT_PROGRESS,
    crate::progress::TYPE_SPAWN_LAUNCHED,
    crate::progress::TYPE_STOP_FAILURE,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The entry every case names.
    fn named() -> GenerationIngested {
        GenerationIngested {
            prefix: "gc".to_string(),
            file: "src/a.rs".to_string(),
            generation: "h1".to_string(),
            blob: "b10b".to_string(),
            excluded: true,
        }
    }

    #[test]
    fn parse_answers_the_five_fields_of_an_entrys_payload() {
        let data =
            br#"{"prefix":"gc","file":"src/a.rs","generation":"h1","blob":"b10b","excluded":true}"#;
        assert_eq!(GenerationIngested::parse(data), Ok(named()));
    }

    #[test]
    fn parse_refuses_a_payload_that_is_not_an_entry_naming_the_type_and_the_fault() {
        assert_eq!(
            GenerationIngested::parse(b"{}"),
            Err(
                "GenerationIngested payload: missing field `prefix` at line 1 column 2".to_string()
            )
        );
        let trailing =
            br#"{"prefix":"gc","file":"src/a.rs","generation":"h1","blob":"","excluded":false} x"#;
        assert_eq!(
            GenerationIngested::parse(trailing),
            Err("GenerationIngested payload: trailing characters at line 1 column 80".to_string())
        );
    }

    #[test]
    fn identity_is_the_prefix_and_the_file_joined_by_a_slash() {
        assert_eq!(named().identity(), "gc/src/a.rs");
    }

    #[test]
    fn identity_parts_cuts_an_identity_at_its_first_slash_and_refuses_an_empty_prefix() {
        assert_eq!(
            [
                "gc/src/a.rs",
                named().identity().as_str(),
                "gd/",
                "/src/a.rs",
                "/",
                "gc",
                "",
            ]
            .map(GenerationIngested::identity_parts),
            [
                Some(("gc", "src/a.rs")),
                Some(("gc", "src/a.rs")),
                Some(("gd", "")),
                None,
                None,
                None,
                None,
            ]
        );
    }
}
