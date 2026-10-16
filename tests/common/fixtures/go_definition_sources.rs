//! THE GO DEFINITIONS FIXTURE: the one source text GO DEFINITIONS ARE NODES (every Go
//! definition type is a definition of its kind) is proven over, on the symbols the registered Go
//! query extracts, beside the closure `var` spec 119's id and `CONTAINS` tests both extract. It
//! names no crate, so every lane compiles it.

/// THE GO DEFINITIONS FIXTURE (spec 116 criterion 7): one Go file holding every Go definition
/// type. Line 2 a const; 3 a const spec naming two, each initializer a call; 4-5 a package-level
/// `var` and a grouped one; 6 a `var` spec naming two, each initializer a call; 7 a struct type;
/// 8 a struct embedding a plain, a pointer, a package-qualified and a generic type above a named
/// field and a field declaration naming two; 9 a type alias; 10 an interface holding a method;
/// 11 a function holding a `var`, a grouped `var` and a const whose initializer calls; 12 a
/// method; 13-14 a function and a method with no body; 15 a type alias named `_`, the blank
/// identifier.
pub const GO_DEFINITIONS_SOURCE: &str = "\
package shapes
const Limit = 9
const a, b = f(), g()
var single = 1
var ( grouped = 2; )
var m, n = h(), k()
type Base struct{}
type Shape struct { Base; *Ptr; pkg.Qual; Gen[int]; side Side; x, y Coord }
type Alias = Base
type Drawer interface { Draw(c Canvas) }
func host() { var local = 1; var ( deep = 2; ); const inner = probe(); _ = local }
func (s Shape) area() {}
func ext(c Canvas)
func (s Shape) bare()
type _ = Base
";

/// `(file, source)` of a Go `var` spec naming two whose first initializer is a closure holding a
/// type (spec 119): the type sits inside a range the two equal-range siblings share.
pub const GO_CLOSURE_VAR: (&str, &str) = (
    "h.go",
    "package h\n\nvar a, b = func() { type T struct{} }(), 1\n",
);
