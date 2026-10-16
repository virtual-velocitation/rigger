//! THE JS AND TS DEFINITIONS FIXTURES: the three source texts JS AND TS DEFINITIONS ARE NODES
//! (every JS and TS definition type is a definition of its kind) is proven over, on the symbols
//! the JS, TS and TSX queries extract; and the TS `namespace A.B` beside a field `B` of a
//! `class A`, the source ONE PATH IS ONE EVENT (spec 119) picks the field's event over. They name
//! no crate, so every lane compiles them.

/// The JS lines all three fixtures open with, every one of them parsed alike by the JS, TS and
/// TSX grammars. Line 1 a function holding a const whose initializer calls, a `for .. of`
/// declaring an array pattern and one declaring an identifier, and a `for .. of` assigning an
/// identifier and one assigning a member; 2 a generator function; 3-4 a declarator whose value
/// is a named function and one whose value is a named generator; 5 one whose value is an
/// anonymous generator calling; 6 one whose value is an anonymous class whose static field
/// calls; 7 one whose value is a named class; 8 a function holding a valueless `let`; 9-10 a
/// declarator named by an object pattern (shorthand, rename, default and rest) and one named by an array
/// pattern (nested, a hole and a default), each initializer a call; 11 a class holding a
/// private, a string-named, a numeric and a computed method, a constructor whose body calls and
/// holds a const whose initializer calls, a field, an arrow-valued and a function-valued field, a valueless private
/// field, a string-named and a numeric field, a computed field and a method that calls; 12 a
/// function assigned to a member; 13 an object holding an arrow-valued pair; 14 an exported
/// assignment.
macro_rules! js_lines {
    () => {
        "\
function host() { const loc = probe(); for (const [k, v] of m) { use(k); } for (const j of m) {} for (q of m) {} for (o.p of m) {} }
function* gen() {}
const named = function fexp() {};
const gnamed = function* gexp() {};
const g = function* () { spin(); };
const Foo = class { static s = tick(); };
const Bar = class Baz {};
function lazy() { let y; }
const {a, b: c, d = 2, ...r} = require(\"x\");
const [x, [y], , z = 1] = f();
class Shape { #m() {} \"a-b\"() {} 1() {} [key]() {} constructor() { init(); const local = make(); } field = 1; act = () => {}; cb = function () {}; #hid; \"q-r\" = 2; 3 = 4; [kk] = 5; area() { paint(); } }
obj.handler = function () { run(); };
const o2 = { p: () => {} };
export default ex = 1;
"
    };
}

/// The TS lines the TS and TSX fixtures close with, from line 16. Line 16 an ambient function
/// signature; 17 an interface holding method signatures named by an identifier, a string and a
/// number, property signatures named by a string, a number and an identifier, one named by a
/// string alone, and an index signature; 18 an abstract class holding abstract methods named by
/// an identifier, a string and a number, a typed field, a function-valued field and a
/// string-named field; 19 an enum holding a member, a string-named member, a member whose value
/// calls and members with a value named by a string and by a number; 20 a type alias; 21 a module
/// named by a string whose body calls; 22 a namespace of a dotted name whose body calls; 23 an
/// `as const` declarator; 24 a module of a dotted name.
macro_rules! ts_lines {
    () => {
        "\
declare function sig(a: Arg): Ret;
interface Shape2 { m(): Ret; \"s-m\"(): void; 4(): void; \"p-q\": PQ; 2: Two; prop: PType; \"bare\"; [n: string]: V; }
abstract class Abs { abstract run(): void; abstract \"s-a\"(): void; abstract 5(): void; fld: FT = 1; act2 = function () {}; \"s-t\": number; }
enum Color { Red, \"Dark-red\", Blue = mk(), \"e-f\" = 1, 6 = 2 }
type Alias = Base;
declare module \"x\" { boot(); }
namespace A.B { setup(); }
const X = { k: 1 } as const;
module M.N {}
"
    };
}

/// THE JS DEFINITIONS FIXTURE (spec 116 criterion 9): the JS lines, then at line 15 a declarator
/// whose value is a JSX element.
pub const JS_DEFINITIONS_SOURCE: &str = concat!(js_lines!(), "const el = <div/>;\n");

/// THE TS DEFINITIONS FIXTURE (spec 116 criterion 9): the JS lines, then at line 15 a declarator
/// whose value is a type assertion, then the TS lines.
pub const TS_DEFINITIONS_SOURCE: &str = concat!(js_lines!(), "const el = <T>u;\n", ts_lines!());

/// THE TSX DEFINITIONS FIXTURE (spec 116 criterion 9): the JS lines, then at line 15 a
/// declarator whose value is a JSX element, then the TS lines.
pub const TSX_DEFINITIONS_SOURCE: &str = concat!(js_lines!(), "const el = <div/>;\n", ts_lines!());

/// A TS `namespace A.B` at line 1 beside a `class A` whose field `B` is at line 3 (spec 119, ONE
/// PATH IS ONE EVENT): two definitions at the one path `A.B`.
pub const NAMESPACE_A_B_BESIDE_CLASS_A_FIELD_B: &str =
    "namespace A.B {}\nclass A {\n    B = 1;\n}\n";
