//! The context graph's sqlite adapter: the `Projector` that folds the event log into
//! bi-temporal node and edge tables, and the payload shapes only its fold reads. The model, the
//! `Projection` port and the query engine live in `rigger-domain`'s `contextgraph`, re-exported
//! here so the fold names them by their historical paths.

// The sqlite projector is the concrete adapter Design names as `store`-gated, so it is excluded
// from the `core` lane (same predicate as every other store-gated module - see the root
// `lib.rs`'s own doc).
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sqlite;

#[cfg(any(feature = "store", not(feature = "core")))]
use serde::de::DeserializeOwned;
#[cfg(any(feature = "store", not(feature = "core")))]
use serde::Deserialize;

pub use rigger_domain::contextgraph::*;

#[cfg(any(feature = "store", not(feature = "core")))]
#[derive(Deserialize)]
struct DecisionMade {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    governs: Vec<String>,
    #[serde(default)]
    supersedes: String,
}
// De-noise (spec 43): the FileTouched / GateVerdict / UnitStarted fold DTOs are gone. Their fold
// arms project only harness machinery (agent/unit/gate nodes and TOUCHES/ASSIGNED_TO/BLOCKS/
// GATED_BY edges), which the graph no longer models - so those arms are now graph no-ops that
// deserialize nothing. The events themselves stay in the log, read by metrics and the run-tree.
#[cfg(any(feature = "store", not(feature = "core")))]
#[derive(Deserialize)]
struct UnitIntegrated {
    // The conductor emits UNIT_INTEGRATED with an `id` key (`{"id": <unit>, "commit": ...}`),
    // unlike UNIT_STARTED which redundantly carries both `id` and `unit`. Accept `id` as an
    // alias so this fold parses what production actually records; without it the fold fails to
    // deserialize every real event and its disposition-expiry effect is dead in production. Only
    // the unit id is read (to drive disposition-expiry); the commit is not projected (de-noise).
    #[serde(alias = "id")]
    unit: String,
}
#[cfg(any(feature = "store", not(feature = "core")))]
#[derive(Deserialize)]
struct AliasUnresolved {
    mention: String,
}
#[cfg(any(feature = "store", not(feature = "core")))]
#[derive(Deserialize)]
struct LessonLearned {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}
#[cfg(any(feature = "store", not(feature = "core")))]
#[derive(Deserialize)]
struct ReviewFinding {
    id: String,
    #[serde(default)]
    by: String,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}

/// Checks that `data` has the payload shape the fold reads for an event of type `type_`, by
/// deserializing it into the fold's own payload type: the one shape definition, so the check can
/// never drift from what the fold applies. It answers for every type whose payload the fold reads,
/// so it is the one judge of a payload the fold deterministically rejects: an emit surface calls it
/// BEFORE appending, so such an event never reaches the log through it, and a rebuild passes over
/// exactly the events it rejects - any other fold failure is the store's, not the payload's. The
/// error names the offending field and the shape expected; a type whose payload the fold does not
/// read passes unchecked. Only the store-gated emit surfaces call it, so it compiles where they
/// and the fold do.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn check_fold_payload(type_: &str, data: &[u8]) -> Result<(), String> {
    // The fold parses with `serde_json::from_slice`, which also refuses bytes trailing the value,
    // so the judge ends the same parse the same way.
    fn shape<T: DeserializeOwned>(type_: &str, data: &[u8]) -> Result<(), String> {
        let mut de = serde_json::Deserializer::from_slice(data);
        serde_path_to_error::deserialize::<_, T>(&mut de)
            .map_err(|e| {
                let why = e.inner().to_string().replace("a sequence", "an array");
                match e.path().to_string().as_str() {
                    "." => format!("{type_} payload: {why}"),
                    field => format!("{type_} payload field `{field}`: {why}"),
                }
            })
            .and_then(|_| de.end().map_err(|e| format!("{type_} payload: {e}")))
    }
    match type_ {
        TYPE_DECISION_MADE => shape::<DecisionMade>(type_, data),
        TYPE_LESSON_LEARNED => shape::<LessonLearned>(type_, data),
        TYPE_REVIEW_FINDING => shape::<ReviewFinding>(type_, data),
        TYPE_UNIT_INTEGRATED => shape::<UnitIntegrated>(type_, data),
        TYPE_CODE_ENTITY_EXTRACTED => shape::<CodeEntityExtracted>(type_, data),
        TYPE_EDGE_INFERRED => shape::<EdgeInferred>(type_, data),
        TYPE_DOC_CONCEPT_EXTRACTED => shape::<DocConceptExtracted>(type_, data),
        TYPE_DOC_LINK_EXTRACTED => shape::<DocLinkExtracted>(type_, data),
        TYPE_ALIAS_DEFINED => shape::<AliasDefined>(type_, data),
        TYPE_ALIAS_UNRESOLVED => shape::<AliasUnresolved>(type_, data),
        TYPE_COMMUNITY_ASSIGNED => shape::<CommunityAssigned>(type_, data),
        TYPE_CONCEPT_DERIVED => shape::<ConceptDerived>(type_, data),
        TYPE_CONCEPT_REALIZED => shape::<ConceptRealized>(type_, data),
        _ => Ok(()),
    }
}

#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod tests {
    use super::*;

    /// Every type whose payload the fold reads has its payload judged - so a rebuild passes over
    /// exactly the events the fold rejects, a ledger entry of perception among them - and a type
    /// whose payload the fold does not read passes unchecked.
    #[test]
    fn every_type_whose_payload_the_fold_reads_is_judged_and_no_other() {
        let judged = [
            TYPE_DECISION_MADE,
            TYPE_LESSON_LEARNED,
            TYPE_REVIEW_FINDING,
            TYPE_UNIT_INTEGRATED,
            TYPE_CODE_ENTITY_EXTRACTED,
            TYPE_EDGE_INFERRED,
            TYPE_DOC_CONCEPT_EXTRACTED,
            TYPE_DOC_LINK_EXTRACTED,
            TYPE_ALIAS_DEFINED,
            TYPE_ALIAS_UNRESOLVED,
            TYPE_COMMUNITY_ASSIGNED,
            TYPE_CONCEPT_DERIVED,
            TYPE_CONCEPT_REALIZED,
            rigger_domain::retention::TYPE_GENERATION_INGESTED,
        ];
        assert_eq!(
            judged
                .iter()
                .map(|t| check_fold_payload(t, b"not json"))
                .collect::<Vec<_>>(),
            judged
                .iter()
                .map(|t| Err(format!("{t} payload: expected ident at line 1 column 2")))
                .collect::<Vec<_>>(),
            "each is refused, naming its type"
        );
        // One object carrying every field any judged type requires, so each type accepts it -
        // and the same object followed by trailing bytes, which the fold's own parse
        // (`serde_json::from_slice`) rejects, so the judge must too.
        let every_field = br#"{"id":"i","mention":"m","file":"f","name":"n","kind":"k","line":1,"from":"a","to":"b","rel":"r","alias":"a","canonical":"c","node":"n","community":"c","concept":"c","prefix":"gc","generation":"h1","blob":"","excluded":false}"#;
        let trailing = [&every_field[..], b" x"].concat();
        assert_eq!(
            judged
                .iter()
                .map(|t| (
                    check_fold_payload(t, every_field),
                    check_fold_payload(t, &trailing)
                ))
                .collect::<Vec<_>>(),
            judged
                .iter()
                .map(|t| (
                    Ok(()),
                    Err(format!(
                        "{t} payload: trailing characters at line 1 column {}",
                        every_field.len() + 2
                    ))
                ))
                .collect::<Vec<_>>(),
            "a payload the fold's parse rejects for its trailing bytes is refused, naming its type"
        );
        assert_eq!(
            [TYPE_FILE_TOUCHED, TYPE_GATE_VERDICT, "Unheard"].map(|t| check_fold_payload(t, b"x")),
            [Ok(()), Ok(()), Ok(())],
            "a type whose payload the fold does not read passes unchecked"
        );
    }

    /// A ledger entry is judged by the parse its fold reads it with: a payload missing a field is
    /// refused in that parse's own words, and one carrying all five passes.
    #[test]
    fn a_ledger_entry_is_judged_by_the_parse_its_fold_reads_it_with() {
        let type_ = rigger_domain::retention::TYPE_GENERATION_INGESTED;
        let whole =
            br#"{"prefix":"gc","file":"src/f.rs","generation":"h1","blob":"","excluded":false}"#;
        assert_eq!(check_fold_payload(type_, whole), Ok(()));
        assert_eq!(
            check_fold_payload(type_, b"{}"),
            Err(
                "GenerationIngested payload: missing field `prefix` at line 1 column 2".to_string()
            )
        );
        assert_eq!(
            check_fold_payload(
                type_,
                br#"{"prefix":"gc","file":"src/f.rs","generation":"h1","blob":""}"#
            ),
            Err(
                "GenerationIngested payload: missing field `excluded` at line 1 column 61"
                    .to_string()
            )
        );
    }
}
