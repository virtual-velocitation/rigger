//! THE TABLE's `defaults.capability` declarations (spec 135, criterion THE TABLE PARSES AND
//! HOLDS THE FLOOR), each a flow mapping: the judging rows the floor accepts and the ones it
//! refuses, and the misshapen tables the parse refuses, each with its exact verdict. Read by the
//! domain parse and floor tests, by the test of `Config::validate` and `read_scratch_defaults`,
//! and by the `rigger validate` and `rigger reset --derived` periphery suites.

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

/// Each misshapen `defaults.capability` workflow with the parse's exact refusal, naming its path:
/// a missing row, a missing cell, an empty `model`, an absent `model` value, a cell with no
/// `model` key, and an unknown key in the table, in a row and in a cell.
pub fn capability_shape_refusals() -> [(String, &'static str); 8] {
    let judging = |row: &str| workflow_declaring_capability(&capability_table(row));
    let rows = |rows: &str| workflow_declaring_capability(&format!("{{ {rows} }}"));
    [
        (
            rows(&format!("judging: {READING_ROW}, writing: {WRITING_ROW}")),
            "defaults.capability: missing field `reading` at line 1 column 25",
        ),
        (
            judging("{ routine: { model: j }, involved: { model: j } }"),
            "defaults.capability.judging: missing field `hard` at line 1 column 36",
        ),
        (
            judging("{ routine: { model: '' }, involved: { model: j }, hard: { model: j } }"),
            "defaults.capability.judging.routine.model: invalid value: string \"\", \
             expected a non-empty host model string at line 1 column 56",
        ),
        (
            judging("{ routine: { model: }, involved: { model: j }, hard: { model: j } }"),
            "defaults.capability.judging.routine.model: invalid value: string \"\", \
             expected a non-empty host model string at line 1 column 56",
        ),
        (
            judging("{ routine: { effort: high }, involved: { model: j }, hard: { model: j } }"),
            "defaults.capability.judging.routine: missing field `model` at line 1 column 47",
        ),
        (
            rows(&format!(
                "judging: {READING_ROW}, writing: {WRITING_ROW}, reading: {READING_ROW}, \
                 planning: {READING_ROW}"
            )),
            "defaults.capability.planning: unknown key",
        ),
        (
            judging(
                "{ routine: { model: j }, involved: { model: j }, hard: { model: j }, \
                 trivial: { model: j } }",
            ),
            "defaults.capability.judging.trivial: unknown key",
        ),
        (
            judging(
                "{ routine: { model: j, effrot: high }, involved: { model: j }, \
                 hard: { model: j } }",
            ),
            "defaults.capability.judging.routine.effrot: unknown key",
        ),
    ]
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
