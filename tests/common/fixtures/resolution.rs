//! The `Capability` a test of spec 135's ONE RESOLUTION expects `capability::resolve` to return,
//! built from its fields. Read by `resolve`'s own table test and by the periphery suite that
//! resolves a loaded project's personas.

use rigger_domain::capability::Capability;

/// The capability with each field as given.
pub fn capability(
    [model, effort, helper_model, helper_effort, why]: [&str; 5],
    from_table: bool,
) -> Capability {
    Capability {
        model: model.into(),
        effort: effort.into(),
        helper_model: helper_model.into(),
        helper_effort: helper_effort.into(),
        why: why.into(),
        from_table,
    }
}
