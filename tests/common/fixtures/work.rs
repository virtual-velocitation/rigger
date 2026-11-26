//! The persona work key's expected text, shared by `parse_agent`'s own test and the periphery
//! test that drives the key through `rigger validate`.

/// The refusal `parse_agent` gives the persona `id` whose `work:` is none of the three kinds.
pub fn work_refusal(id: &str) -> String {
    format!("agent \"{id}\": work must be one of judging, writing, reading")
}
