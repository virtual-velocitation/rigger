//! THE TABLE's `defaults.capability` declarations (spec 135, criterion THE TABLE PARSES AND
//! HOLDS THE FLOOR), each a flow mapping: the judging rows the floor accepts and the ones it
//! refuses, each with its exact verdict. One table, read by the domain floor rule's test and by
//! the test of `Config::validate` and `read_scratch_defaults`.

/// The workflow text declaring the flow mapping `table` as its `defaults.capability`, on one line.
pub fn workflow_declaring_capability(table: &str) -> String {
    format!("defaults: {{ capability: {table} }}\n")
}

/// The `writing` row of [`capability_table`]: its `routine` and `involved` cells name different
/// models, which the floor allows outside the judging row.
pub const WRITING_ROW: &str =
    "{ routine: { model: w1 }, involved: { model: w2 }, hard: { model: w3, effort: max } }";

/// The `reading` row of [`capability_table`]: its cells name one model and differ in effort.
pub const READING_ROW: &str =
    "{ routine: { model: r, effort: low }, involved: { model: r }, hard: { model: r } }";

/// The capability table whose `judging` row is the flow mapping `judging`, beside
/// [`WRITING_ROW`] and [`READING_ROW`].
pub fn capability_table(judging: &str) -> String {
    format!("{{ judging: {judging}, writing: {WRITING_ROW}, reading: {READING_ROW} }}")
}

/// Each judging row with the floor's verdict on it, `CapabilityTable::validate`'s exact answer.
/// The accepted rows: `routine` and `involved` naming one model with two different efforts, with
/// one effort absent, and with `hard` naming another model. The refused rows: two models under
/// one effort, and two models differing only in case.
pub const JUDGING_FLOOR: [(&str, Result<(), &str>); 5] = [
    (
        "{ routine: { model: m, effort: low }, involved: { model: m, effort: high }, \
         hard: { model: m } }",
        Ok(()),
    ),
    (
        "{ routine: { model: m, effort: high }, involved: { model: m }, hard: { model: m } }",
        Ok(()),
    ),
    (
        "{ routine: { model: m }, involved: { model: m }, hard: { model: top } }",
        Ok(()),
    ),
    (
        "{ routine: { model: low, effort: high }, involved: { model: m, effort: high }, \
         hard: { model: m } }",
        Err(
            "judging: the routine cell names model \"low\" and the involved cell \"m\"; a \
             judging spawn runs on one model whether planned routine or involved, so the two \
             cells name the same model and may differ only in effort",
        ),
    ),
    (
        "{ routine: { model: m }, involved: { model: M }, hard: { model: M } }",
        Err(
            "judging: the routine cell names model \"m\" and the involved cell \"M\"; a \
             judging spawn runs on one model whether planned routine or involved, so the two \
             cells name the same model and may differ only in effort",
        ),
    ),
];
