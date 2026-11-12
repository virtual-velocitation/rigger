//! THE RESOLVER TREE: the fixture tree holding a row for every rule of the resolver's table
//! (`rigger_domain::resolve`). The resolver's golden reads it, and the worktree overlay's
//! equivalence reads it as the commit a worktree changes. It names no workspace crate.

/// The resolver's fixture tree, one `(path, contents)` per file. Each file's comment names the
/// rows of the resolver's table it holds.
pub const RESOLVER_TREE: &[(&str, &str)] = &[
    // Same-file call (helper), cross-file unique call (compute), ambiguous call (draw), a name
    // no definition holds (vanish), type, const and use references, module declarations (one
    // nested in an inline module, one test-only), a nested definition, a test helper sharing a
    // product leaf, and test calls that prove product definitions.
    (
        "src/lib.rs",
        r#"use crate::shapes::Shape;
use crate::util::{compute, LIMIT};

mod shapes;
mod util;
#[cfg(test)]
mod lib_tests;

pub mod inner {
    pub fn deep() -> u32 {
        super::helper()
    }

    mod nested;
}

pub fn run() -> u32 {
    let shape: Shape = Shape::new();
    helper();
    compute();
    draw();
    vanish();
    inner::deep() + LIMIT + shape.0
}

fn helper() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    fn helper() -> u32 {
        2
    }

    #[test]
    fn runs() {
        let total = super::run();
        super::helper();
        assert_eq!(total, helper());
    }
}
"#,
    ),
    // Duplicate ids: the struct and its impls share `Shape`, the cfg twins share `origin`.
    (
        "src/shapes.rs",
        r#"pub struct Shape(pub u32);

impl Shape {
    pub fn new() -> Self {
        Shape(origin())
    }
}

impl Default for Shape {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
pub fn origin() -> u32 {
    0
}

#[cfg(not(unix))]
pub fn origin() -> u32 {
    1
}

pub fn draw() {}
"#,
    ),
    // A same-file const read, the second `draw`, and a field sharing the leaf `draw`, which is
    // no call target.
    (
        "src/util.rs",
        r#"pub const LIMIT: u32 = 3;

pub fn compute() -> u32 {
    LIMIT
}

pub fn draw() {}

pub struct Config {
    pub draw: u32,
}
"#,
    ),
    // The module the inline `inner` declares: a cross-file call whose only product candidate
    // shares its leaf with a test helper.
    (
        "src/inner/nested.rs",
        r#"pub fn nested_value() -> u32 {
    super::super::helper()
}
"#,
    ),
    // Two inline modules each declaring an out-of-line module of one name: each declaration's
    // edge leaves its own definition.
    (
        "src/twins.rs",
        r#"pub mod left {
    mod leaf;
}

pub mod right {
    mod leaf;
}
"#,
    ),
    (
        "src/twins/left/leaf.rs",
        "pub fn left_value() -> u32 {\n    1\n}\n",
    ),
    (
        "src/twins/right/leaf.rs",
        "pub fn right_value() -> u32 {\n    2\n}\n",
    ),
    // The out-of-line test module, the closure's seed, which declares a module of its own.
    (
        "src/lib_tests.rs",
        r#"mod more;

#[test]
fn computes() {
    let value = super::util::compute();
    assert_eq!(value, 3);
}
"#,
    ),
    // A file reached only through a test module: test code by the closure alone.
    (
        "src/lib_tests/more.rs",
        r#"pub fn more_origin() -> u32 {
    crate::shapes::origin()
}
"#,
    ),
    // A Test-scope file by the path rule: a unique proof (run) and an ambiguous one (draw).
    (
        "tests/integration.rs",
        r#"#[test]
fn runs_end_to_end() {
    let total = fixture::run();
    fixture::draw();
    assert_eq!(total, 4);
}
"#,
    ),
    // A second language sharing the leaf `Shape` (a type reference filters on language) and
    // calling `compute` (a call does not).
    (
        "scripts/tool.py",
        r#"class Shape:
    pass


def assist():
    return compute()
"#,
    ),
    // Design links: a design doc specifying a source file and citing an ADR, an ADR
    // constraining one, and a source rationale explaining its file.
    (
        "docs/architecture.md",
        "# Reference architecture\n\n## Shapes\n\nThe shapes live in `src/shapes.rs`.\n\nSee [the decision](adr/0001-shapes.md).\n",
    ),
    (
        "docs/adr/0001-shapes.md",
        "# Shapes are values\n\nThe decision binds `src/util.rs`.\n",
    ),
    // A workflow definition: stages, a gate and an agent.
    (
        ".rigger/workflow.yml",
        "stages:\n  plan:\n    agent: planner\n  implement:\n    agent: worker\n    needs: [plan]\n    gates: [fmt]\ngates:\n  fmt:\n    run: cargo fmt --check\n",
    ),
];
