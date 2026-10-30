//! THE RUST DEFINITIONS FIXTURE: the one source text RUST DEFINITIONS ARE NODES (every Rust
//! definition type is a definition of its kind) is proven over, once on the symbols the
//! registered Rust query extracts and once on the graph a build folds; and the enum `E` before
//! and after its variant `B` is removed, the sources CONTAINS IS STRUCTURE (spec 119) retires a
//! variant's `CONTAINS` over; and the method `a::B::c` in two impls and then in one, the sources
//! ONE PATH IS ONE EVENT (spec 119) picks and hands over a path's one event over; and the tree
//! REFERENCES ARE EXTRACTED AND ISOLATED (spec 117) is proven over. It names no crate, so every
//! lane compiles it.

/// THE RUST DEFINITIONS FIXTURE (spec 116 criterion 6): one Rust file holding every Rust
/// definition type. Lines 1-2 an `extern crate` without and with an alias; 3-4 an item const
/// and an item static whose initializer calls; 5 a tuple struct whose third field is a
/// test-attributed `pub` one; 6 an enum of a unit, a tuple and a struct variant and a
/// test-attributed unit variant; 7-8 a union and a type alias; 9 a trait holding an
/// associated type and a required fn; 10 an impl holding an associated const and a method; 11
/// a `macro_rules!`; 12-13 a proc-macro fn holding a const whose initializer calls; 14 a fn
/// holding a const whose initializer calls and a static; 15 a mod; 16-18 a fn `size` above a
/// struct whose fields are `size`, `area` and a test-attributed `spy`, above a fn `area`;
/// 19-20 a test-attributed const and static; 21-25 a struct, an enum, an impl, a fn and a mod
/// whose body each opens with a test-attributed member, variant, method, statement or mod.
pub const RUST_DEFINITIONS_SOURCE: &str = "\
extern crate alloc;
extern crate core as base;
pub const LIMIT: u8 = 9;
static TABLE: u8 = build();
pub struct Pair(u8, u16, #[cfg(test)] pub u32);
pub enum Shape { Unit, Tuple(u8), Struct { side: u8 }, #[cfg(test)] Probe }
pub union Bits { whole: u8 }
pub type Alias = u8;
pub trait Draw { type Canvas; fn paint(&self); }
impl Pair { const WIDTH: u8 = 2; fn width(&self) {} }
macro_rules! shout { () => {} }
#[proc_macro]
pub fn derive(input: u8) -> u8 { const SEED: u8 = seed(); input }
fn host() { const LOCAL: u8 = compute(); static CACHE: u8 = 1; }
mod nest {}
fn size() {}
struct Plot { size: u8, area: u8, #[cfg(test)] spy: u8 }
fn area() {}
#[cfg(test)] const PROBE_LIMIT: u8 = 1;
#[cfg(test)] static PROBE_TABLE: u8 = 1;
struct S { #[cfg(test)] probe: u32, real: u32 }
enum E { #[cfg(test)] A, B }
impl X { #[cfg(test)] fn t() {} fn real() {} }
fn f() { #[cfg(test)] let x = 1; }
mod outer { #[cfg(test)] mod inner {} }
";

/// The enum `E` holding the variants `A` and `B` (spec 119, CONTAINS IS STRUCTURE), each
/// contained by `E` and `E` by its file.
pub const ENUM_E_HOLDING_A_AND_B: &str = "enum E {\n    A,\n    B,\n}\n";

/// [`ENUM_E_HOLDING_A_AND_B`] with its variant `B` removed: `E` holds `A` alone.
pub const ENUM_E_HOLDING_A: &str = "enum E {\n    A,\n}\n";

/// The method `c` of `a::B` defined twice (spec 119, ONE PATH IS ONE EVENT): at line 4 in the
/// `impl B` inside `mod a`, and at line 8 in the file-level `impl a::B`.
pub const METHOD_C_IN_TWO_IMPLS: &str =
    "mod a {\n    struct B;\n    impl B {\n        fn c() {}\n    }\n}\nimpl a::B {\n    fn c() {}\n}\n";

/// [`METHOD_C_IN_TWO_IMPLS`] with the `impl B` inside `mod a` blanked out: `c` at line 8 alone,
/// in the file-level `impl a::B`.
pub const METHOD_C_IN_THE_FILE_LEVEL_IMPL: &str =
    "mod a {\n    struct B;\n\n\n\n}\nimpl a::B {\n    fn c() {}\n}\n";

/// The definer file of [`REFERENCE_KINDS_TREE`].
pub const REFERENCE_KINDS_DEFINER: &str = "src/shapes.rs";

/// The user file of [`REFERENCE_KINDS_TREE`], whose references are of every kind.
pub const REFERENCE_KINDS_USER: &str = "src/user.rs";

/// The tree REFERENCES ARE EXTRACTED AND ISOLATED (spec 117) is proven over, each `(path,
/// source)`: `src/shapes.rs` defines the struct `Kestrel`, the const `QUOTA_CAP` and the fn
/// `draw`; `src/user.rs` imports `Kestrel` by a `use` (line 1) and, in `fn paint` (line 2), takes
/// it as a parameter type, calls `draw` and reads `QUOTA_CAP` - one reference of each kind beside
/// one call.
pub const REFERENCE_KINDS_TREE: [(&str, &str); 2] = [
    (
        REFERENCE_KINDS_DEFINER,
        "pub struct Kestrel;\npub const QUOTA_CAP: u32 = 3;\npub fn draw() {}\n",
    ),
    (
        REFERENCE_KINDS_USER,
        "use crate::shapes::Kestrel;\nfn paint(k: Kestrel) -> u32 { draw(); QUOTA_CAP }\n",
    ),
];

/// The tagged references of [`REFERENCE_KINDS_TREE`]'s `src/user.rs`, one line each, `<line>
/// <ref kind> <name> <caller>`: the `use` of `Kestrel` at the top level, then `fn paint`'s type
/// reference to `Kestrel` and const read of `QUOTA_CAP`.
pub const REFERENCE_KINDS_USER_TAGGED: &str =
    "1 use Kestrel -\n2 type Kestrel paint\n2 const QUOTA_CAP paint";
