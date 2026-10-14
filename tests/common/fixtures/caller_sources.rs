//! THE CALLER SOURCES: the two source texts the caller rule (a reference's caller is its
//! innermost enclosing function, else its innermost enclosing definition, else its file) is
//! proven over, once on the extracted references and once on the graph a build folds. It names
//! no crate, so every lane compiles it.

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
