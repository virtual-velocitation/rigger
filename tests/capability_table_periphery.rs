//! Periphery (CLI, real-binary) tests for spec 135, criterion THE TABLE PARSES AND HOLDS THE
//! FLOOR: `rigger validate` over a project whose workflow declares `defaults.capability`.
//!
//! The domain's unit tests pin the table's parse and `CapabilityTable::validate` at the library
//! level; this file proves the operator's surface end to end. A one-worker project whose worker
//! pins its model is valid with a null table and with each table the judging floor accepts, and
//! fails, printing only the refusal, on each table the floor refuses (through `Config::validate`)
//! and on each misshapen table the parse refuses (naming its path). The project is scaffolded,
//! never `rigger init`'s fleet, so the rows read the table alone. `read_scratch_defaults`, which
//! parses the table without the floor, is proven beside the scratch root it resolves, in
//! `tests/reset_derived_live_writer_guard_periphery.rs`.

use crate::common;

use common::cli::assert_one_worker_validates;
use common::cli::temp_project;
use common::cli::UNISOLATED_WORKER;
use common::fixtures::capability_shape_refusals;
use common::fixtures::capability_table;
use common::fixtures::workflow_declaring_capability;
use common::fixtures::JUDGING_FLOOR;

/// Given a one-worker project whose workflow declares a capability table, when the operator
/// validates it, a null table and each table whose judging `routine` and `involved` cells name one
/// model are valid, and each table naming two models there, or misshapen, fails printing nothing
/// but the one refusal naming its path.
#[test]
fn rigger_validate_holds_the_capability_table_to_its_shape_and_its_judging_floor() {
    let project = temp_project();
    let floor = JUDGING_FLOOR.map(|(judging, verdict)| {
        (
            workflow_declaring_capability(&capability_table(judging)),
            verdict.map_err(|refusal| format!("config: defaults.capability.{refusal}")),
        )
    });
    let shape = capability_shape_refusals()
        .map(|(workflow, refusal)| (workflow, Err(format!("config: parse workflow: {refusal}"))));
    let null = (workflow_declaring_capability("~"), Ok(()));
    for (workflow, verdict) in [null].into_iter().chain(floor).chain(shape) {
        assert_one_worker_validates(project.path(), UNISOLATED_WORKER, &workflow, verdict);
    }
}
