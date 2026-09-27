//! The context graph's sqlite adapter: the `Projector` that folds the event log into
//! bi-temporal node and edge tables, and the payload shapes only its fold reads. The model, the
//! `Projection` port and the query engine live in `rigger-domain`'s `contextgraph`, re-exported
//! here so the fold names them by their historical paths.

// The sqlite projector is the concrete adapter Design names as `store`-gated, so it is excluded
// from the `core` lane (same predicate as every other store-gated module - see the root
// `lib.rs`'s own doc).
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sqlite;

use serde::Deserialize;

pub use rigger_domain::contextgraph::*;

#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
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
struct AliasDefined {
    alias: String,
    canonical: String,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct AliasUnresolved {
    mention: String,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct LessonLearned {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
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
