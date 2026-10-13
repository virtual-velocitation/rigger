//! THE NAME FILTER SOURCES: the three source texts the name filter (a definition whose leaf is
//! empty, or `_` in Go or Rust, makes no definition) is proven over, once on the symbols a
//! registered query extracts and once on the graph a build folds. It names no crate, so every
//! lane compiles it.

/// A Go file holding a named type (line 2), a function (line 3), a method (line 4) and a type
/// (line 5) each named `_`, the blank identifier, and a named function (line 6); each function
/// and method body holds one call.
pub const GO_BLANK_SOURCE: &str = "\
package main
type T struct{}
func _() { probe() }
func (T) _() { reach() }
type _ int
func named() { other() }
";

/// A JS file binding a `require` call to a const named `_`.
pub const JS_UNDERSCORE_SOURCE: &str = "const _ = require('lodash');\n";

/// A Python file binding an attribute read to a module-level `_` (line 2).
pub const PY_UNDERSCORE_SOURCE: &str = "import gettext\n_ = gettext.gettext\n";
