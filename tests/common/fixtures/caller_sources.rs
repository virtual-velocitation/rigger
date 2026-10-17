//! THE CALLER SOURCES: the source texts the caller rule (a reference's caller is its innermost
//! enclosing function, else its innermost enclosing definition, else its file) is proven over,
//! once on the extracted references and once on the graph a build folds, and the two a caller's
//! path (spec 119: a caller is named by its definition's path) is proven over the same two ways,
//! with the `CALLS` edges each of those two folds.
//! It names no crate, so every lane compiles it.

/// A Rust file holding every placement the rule tells apart: a file-level impl with a method
/// (line 3); `fn host` holding a local impl with a method (line 6), a trait with a macro call and
/// a method (line 7) and an inline mod with a macro call and a fn (line 8); a file-level mod
/// (line 10) and trait (line 11) each holding a macro call; and a top-level macro call (line 12).
pub const RUST_CALLER_SOURCE: &str = "\
trait Draw {}
struct Widget;
impl Draw for Widget { fn draw(&self) { paint(); } }
fn host() {
    struct Local;
    impl Draw for Local { fn draw(&self) { brush(); } }
    trait Shape { declare!(); fn area(&self) { measure(); } }
    mod inner { setup!(); fn deep() { dig(); } }
}
mod outer { boot!(); }
trait Top { announce!(); }
start!();
";

/// A Rust file calling `helper` from the method `new` of `impl T` (line 3), whose caller is named
/// by its path `T::new`.
pub const RUST_METHOD_CALLER_SOURCE: &str = "\
struct T;
impl T {
    fn new() -> T { helper(); T }
}
fn helper() {}
";

/// The `CALLS` edges of [`RUST_METHOD_CALLER_SOURCE`] folded as `a.rs`, each `(from, to)`: the
/// call in `new` from the method's path `T::new`, and the impl header's self type from the impl.
pub const RUST_METHOD_CALLER_CALLS: &[(&str, &str)] = &[
    ("a.rs::T::new", "a.rs::helper"),
    ("a.rs::impl T", "a.rs::T"),
];

/// A Go file calling `helper` from the receiver method `name` of `*Recv` (line 5), whose caller
/// is named by its path `Recv::name`.
pub const GO_RECEIVER_CALLER_SOURCE: &str = "\
package g

type Recv struct{}

func (r *Recv) name() { helper() }

func helper() {}
";

/// The `CALLS` edges of [`GO_RECEIVER_CALLER_SOURCE`] folded as `g.go`, each `(from, to)`: the
/// receiver type and the call to `helper`, both from the method's path `Recv::name`.
pub const GO_RECEIVER_CALLER_CALLS: &[(&str, &str)] = &[
    ("g.go::Recv::name", "g.go::Recv"),
    ("g.go::Recv::name", "g.go::helper"),
];

/// A JS file binding a call to a const inside a function declaration (line 2), at the top level
/// (line 4) and inside an arrow-valued const (line 6), with a top-level call (line 8).
pub const JS_CALLER_SOURCE: &str = "\
function outer() {
  const x = f();
}
const y = g();
const h = () => {
  const z = k();
};
top();
";
