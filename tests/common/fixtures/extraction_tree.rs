//! THE EXTRACTION TREE: the one small project tree the bytes-form tests extract, holding one file
//! of each kind the code store reads - a source file, the out-of-line test module it declares, a
//! design document and the workflow definition. It names no workspace crate and needs `tempfile`
//! from the including crate's dev-dependencies.

use std::path::Path;

/// The source file: two product functions, a rationale line, an in-file test module that
/// references a product function, and the declaration of the out-of-line test module.
pub const SOURCE_PATH: &str = "src/lib.rs";
pub const SOURCE_BODY: &str = "\
// WHY: the entry stays small so the walk has one product file
fn product() {
    helper();
}

fn helper() {}

#[cfg(test)]
mod checks;

#[cfg(test)]
mod inline {
    #[test]
    fn it_works() {
        product();
    }
}
";

/// The out-of-line test module [`SOURCE_BODY`] declares. Its rationale line gives the same path a
/// design-half fact too.
pub const TEST_MODULE_PATH: &str = "src/checks.rs";
pub const TEST_MODULE_BODY: &str = "\
// WHY: the checks live out of line so the product file stays short
fn checks_product() {
    product();
}
";

/// The `gd` design document: a titled reference architecture with one section, one code path it
/// specifies and one document it cites.
pub const DOCUMENT_PATH: &str = "docs/architecture.md";
pub const DOCUMENT_BODY: &str = "\
# Reference architecture

## The walk

The entry is `src/lib.rs`.

See the [handbook](docs/handbook.md).
";

/// The workflow definition: one stage run by one agent under one gate.
pub const WORKFLOW_PATH: &str = ".rigger/workflow.yml";
pub const WORKFLOW_BODY: &str = "\
stages:
  implement:
    agent: rust-engineer
    gates: [fmt]

gates:
  fmt: { run: \"cargo fmt --check\" }
";

/// The tree planted in a fresh directory, kept alive by the returned guard: the four files
/// written through `write_file`, the caller's writer of one file with its parent directories.
pub fn planted_extraction_tree(write_file: impl Fn(&Path, &[u8])) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a fresh directory for the extraction tree");
    for (path, body) in [
        (SOURCE_PATH, SOURCE_BODY),
        (TEST_MODULE_PATH, TEST_MODULE_BODY),
        (DOCUMENT_PATH, DOCUMENT_BODY),
        (WORKFLOW_PATH, WORKFLOW_BODY),
    ] {
        write_file(&dir.path().join(path), body.as_bytes());
    }
    dir
}
