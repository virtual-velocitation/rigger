//! THE EXTENT SOURCES: the Rust source texts ONE EXTENT's kind-reading tiers are proven over,
//! once on the bound `extract::locate_extent` answers and once on the body `rigger graph --show`
//! prints. It names no crate, so every lane compiles it.

/// A definition whose kind changed in place: the function `f` recorded at line 1 is now a
/// constant spanning lines 1-2, beside a function `f` at lines 3-4.
pub const KIND_CHANGED_IN_PLACE_SOURCE: &str = "const f: u8 =\n    0;\nfn f() {\n}\n";

/// A function `h` at lines 5-6 beside a same-named struct field `h` at line 2, neither at line 1.
pub const FN_BESIDE_SAME_NAMED_FIELD_SOURCE: &str = "struct S {\n    h: u8,\n}\n\nfn h() {\n}\n";

/// Two same-named definitions of other kinds, a struct `g` and a constant `g`, both at line 1,
/// and no function `g` anywhere.
pub const TWO_OTHER_KINDS_AT_ONE_LINE_SOURCE: &str = "struct g {} const g: u8 = 0;\n";
