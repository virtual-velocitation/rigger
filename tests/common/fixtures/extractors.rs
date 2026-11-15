//! THE EXTRACTORS KEY's declarations (spec 125, criterion 7), each a flow mapping of the
//! workflow's `extractors:` key: the well-formed ones the workflow check accepts, and each
//! malformed one with its exact refusal. One table, read by the domain check's test, by
//! `Config::validate`'s, by the code store's read of the declared map and by the binary's
//! periphery suite.

/// The `extractors:` mappings the check accepts: none, one, a name of each character class at
/// each end of its range, a name mixing them with two extensions and a run holding a space, and
/// one extractor listing its extension twice.
pub const ACCEPTED_EXTRACTORS: [&str; 6] = [
    "{}",
    "{ zig: { extensions: [zig], run: zig-tags } }",
    "{ a: { extensions: [a], run: x }, '0': { extensions: [b], run: x }, \
     _: { extensions: [c], run: x }, '-': { extensions: [d], run: x } }",
    "{ z: { extensions: [z], run: x }, '9': { extensions: [n], run: x } }",
    "{ lua_5-4: { extensions: [lua, luau], run: lua facts.lua } }",
    "{ zig: { extensions: [zig, zig], run: zig-tags } }",
];

/// Each malformed `extractors:` mapping, one defect a row, with the exact refusal naming the
/// extractor it lies in; the names one byte outside each end of the name's character ranges
/// included.
pub const EXTRACTOR_REFUSALS: [(&str, &str); 22] = [
    (
        "{ '': { extensions: [zig], run: zig-tags } }",
        "extractors.: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ '`': { extensions: [zig], run: zig-tags } }",
        "extractors.`: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ '{': { extensions: [zig], run: zig-tags } }",
        "extractors.{: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ '/': { extensions: [zig], run: zig-tags } }",
        "extractors./: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ ':': { extensions: [zig], run: zig-tags } }",
        "extractors.:: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ '^': { extensions: [zig], run: zig-tags } }",
        "extractors.^: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ ',': { extensions: [zig], run: zig-tags } }",
        "extractors.,: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ Zig: { extensions: [zig], run: zig-tags } }",
        "extractors.Zig: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ zïg: { extensions: [zig], run: zig-tags } }",
        "extractors.zïg: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ z.g: { extensions: [zig], run: zig-tags } }",
        "extractors.z.g: a name is one or more of a-z, 0-9, '_' and '-'",
    ),
    (
        "{ zig: { extensions: [zig] } }",
        "extractors.zig: run names no command",
    ),
    (
        "{ zig: { extensions: [zig], run: '' } }",
        "extractors.zig: run names no command",
    ),
    (
        "{ zig: { extensions: [zig], run: ' ' } }",
        "extractors.zig: run names no command",
    ),
    (
        "{ zig: { run: zig-tags } }",
        "extractors.zig: extensions lists no extension",
    ),
    (
        "{ zig: { extensions: [], run: zig-tags } }",
        "extractors.zig: extensions lists no extension",
    ),
    (
        "{ zig: { extensions: [''], run: zig-tags } }",
        "extractors.zig: extension \"\" is empty or holds '.' or '/'",
    ),
    (
        "{ zig: { extensions: [.zig], run: zig-tags } }",
        "extractors.zig: extension \".zig\" is empty or holds '.' or '/'",
    ),
    (
        "{ zig: { extensions: [d.ts], run: zig-tags } }",
        "extractors.zig: extension \"d.ts\" is empty or holds '.' or '/'",
    ),
    (
        "{ zig: { extensions: [a/b], run: zig-tags } }",
        "extractors.zig: extension \"a/b\" is empty or holds '.' or '/'",
    ),
    (
        "{ zig: { extensions: [zig, z/g], run: zig-tags } }",
        "extractors.zig: extension \"z/g\" is empty or holds '.' or '/'",
    ),
    (
        "{ zig: { extensions: [zig], run: a }, zag: { extensions: [zig], run: b } }",
        "extractors.zig: extension \"zig\" is also listed by extractors.zag",
    ),
    (
        "{ a: { extensions: [x, y], run: a }, b: { extensions: [z, y], run: b } }",
        "extractors.b: extension \"y\" is also listed by extractors.a",
    ),
];

/// A workflow definition whose only key is `extractors:`, declaring the flow mapping `mapping`.
pub fn workflow_declaring_extractors(mapping: &str) -> String {
    format!("extractors: {mapping}\n")
}
