//! The context graph's sqlite adapter: the `Projector` that folds the event log into
//! bi-temporal node and edge tables, and the payload shapes only its fold reads. The model, the
//! `Projection` port and the query engine live in `rigger-domain`'s `contextgraph`, re-exported
//! here so the fold names them by their historical paths.

// The sqlite projector is the concrete adapter Design names as `store`-gated, so it is excluded
// from the `core` lane (same predicate as every other store-gated module - see the root
// `lib.rs`'s own doc).
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sqlite;

use serde::de::DeserializeOwned;
use serde::Deserialize;

pub use rigger_domain::contextgraph::*;

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
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct UnitIntegrated {
    // The conductor emits UNIT_INTEGRATED with an `id` key (`{"id": <unit>, "commit": ...}`),
    // unlike UNIT_STARTED which redundantly carries both `id` and `unit`. Accept `id` as an
    // alias so this fold parses what production actually records; without it the fold fails to
    // deserialize every real event and its disposition-expiry effect is dead in production. Only
    // the unit id is read (to drive disposition-expiry); the commit is not projected (de-noise).
    #[serde(alias = "id")]
    unit: String,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct AliasUnresolved {
    mention: String,
}
#[derive(Deserialize)]
struct LessonLearned {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}
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
/// never drift from what the fold applies. An event whose payload fails here would sit in the log
/// unfolded (the fold skips it and never records its position), so an emit surface calls this
/// BEFORE appending. The error names the offending field and the shape expected; a type whose
/// payload this module does not define passes unchecked.
pub fn check_fold_payload(type_: &str, data: &[u8]) -> Result<(), String> {
    fn shape<T: DeserializeOwned>(type_: &str, data: &[u8]) -> Result<(), String> {
        serde_path_to_error::deserialize::<_, T>(&mut serde_json::Deserializer::from_slice(data))
            .map(drop)
            .map_err(|e| {
                let why = e.inner().to_string().replace("a sequence", "an array");
                match e.path().to_string().as_str() {
                    "." => format!("{type_} payload: {why}"),
                    field => format!("{type_} payload field `{field}`: {why}"),
                }
            })
    }
    match type_ {
        TYPE_DECISION_MADE => shape::<DecisionMade>(type_, data),
        TYPE_LESSON_LEARNED => shape::<LessonLearned>(type_, data),
        TYPE_REVIEW_FINDING => shape::<ReviewFinding>(type_, data),
        _ => Ok(()),
    }
}
