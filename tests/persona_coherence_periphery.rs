//! Periphery (CLI, real-binary) tests for spec 135, criterion THE DECLARATIONS ARE COHERENT:
//! `rigger validate` over a project whose persona's declarations `config::coherence` weighs
//! against the workflow's `defaults.capability` table or its absence.
//!
//! The domain's unit tests pin `config::coherence` and `Config::validate` at the library level;
//! this file proves the operator's surface end to end. A one-worker project is valid when its
//! persona pins a model or declares nothing with no table, and pins a model, a model ladder or
//! declares work under a table. It fails, printing only the refusal, when its persona declares
//! work with no table, or declares neither work nor a model pin under a table. The project is
//! scaffolded, never `rigger init`'s fleet, so the rows read the one persona alone. The refusals
//! of `rigger init`, `rigger setup --agents` and a template set are proven in
//! `tests/init_template_sets_periphery.rs`.

use crate::common;

use common::cli::assert_one_worker_validates;
use common::cli::temp_project;
use common::fixtures::persona_coherence;
use common::fixtures::workflow_declaring_capability;

/// Given a one-worker project whose workflow declares a capability table or none, when the
/// operator validates it, each persona coherent with that declaration is valid, and each
/// incoherent one fails printing nothing but the one refusal naming the persona.
#[test]
fn rigger_validate_refuses_a_persona_incoherent_with_the_workflows_capability_table() {
    let project = temp_project();
    for (persona, table, verdict) in persona_coherence() {
        assert_one_worker_validates(
            project.path(),
            &persona,
            &workflow_declaring_capability(&table),
            verdict.map_err(|refusal| format!("config: {refusal}")),
        );
    }
}
