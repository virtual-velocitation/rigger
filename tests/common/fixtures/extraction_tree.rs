//! THE EXTRACTION TREE: the one small project tree the bytes-form tests extract, holding one file
//! of each kind an ingest reads - a `gc` source file, the out-of-line test module it declares, a
//! `gd` design document and the workflow definition. It names no crate, so every lane and every
//! crate's tests compile it.

use std::path::Path;

/// The `gc` source file: two product functions, a rationale line, an in-file test module that
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
/// `gd` batch too, so the path has an identity under two prefixes.
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

/// One batch the walk hands a sink: the identity's prefix and path, the batch's generation, and
/// its events as `(type, payload text)`.
pub struct WalkedBatch {
    pub prefix: &'static str,
    pub path: &'static str,
    pub generation: &'static str,
    pub events: &'static [(&'static str, &'static str)],
}

/// What the walk hands a sink for the tree, in emit order. The out-of-line test module's `gc`
/// batch is the one boundary event of a hollowed file; the source file's last event is the
/// evidence its in-file test module gives.
pub const WALKED: [WalkedBatch; 6] = [
    WalkedBatch {
        prefix: "gc",
        path: TEST_MODULE_PATH,
        generation: "878ec204b714de6b",
        events: &[(
            "EdgeInferred",
            r#"{"file":"src/checks.rs","name":"","lang":"rust","fresh":true}"#,
        )],
    },
    WalkedBatch {
        prefix: "gc",
        path: SOURCE_PATH,
        generation: "f81a57a5c4f55f52",
        events: &[
            (
                "CodeEntityExtracted",
                r#"{"file":"src/lib.rs","name":"helper","kind":"function","line":6,"lang":"rust","fresh":true}"#,
            ),
            (
                "CodeEntityExtracted",
                r#"{"file":"src/lib.rs","name":"product","kind":"function","line":2,"lang":"rust"}"#,
            ),
            (
                "EdgeInferred",
                r#"{"file":"src/lib.rs","name":"helper","lang":"rust","caller":"product"}"#,
            ),
            (
                "EdgeInferred",
                r#"{"file":"src/lib.rs","name":"product","lang":"rust","fresh":true,"line":15,"is_test":true}"#,
            ),
        ],
    },
    WalkedBatch {
        prefix: "gd",
        path: DOCUMENT_PATH,
        generation: "ea5177040caf5338",
        events: &[
            (
                "DocConceptExtracted",
                r#"{"kind":"design-doc","id":"docs/architecture.md","title":"Reference architecture","doc":"docs/architecture.md"}"#,
            ),
            (
                "DocConceptExtracted",
                r#"{"kind":"design-doc","id":"docs/architecture.md#the-walk","title":"The walk","doc":"docs/architecture.md"}"#,
            ),
            (
                "DocLinkExtracted",
                r#"{"from":"docs/architecture.md","to":"src/lib.rs","rel":"SPECIFIES"}"#,
            ),
            (
                "DocLinkExtracted",
                r#"{"from":"docs/architecture.md","to":"docs/handbook.md","rel":"references"}"#,
            ),
        ],
    },
    WalkedBatch {
        prefix: "gd",
        path: TEST_MODULE_PATH,
        generation: "8c6020acb1774c78",
        events: &[
            (
                "DocConceptExtracted",
                r#"{"kind":"rationale","id":"src/checks.rs#L1","title":"WHY: the checks live out of line so the product file stays short","doc":"src/checks.rs"}"#,
            ),
            (
                "DocLinkExtracted",
                r#"{"from":"src/checks.rs#L1","to":"src/checks.rs","rel":"explains"}"#,
            ),
        ],
    },
    WalkedBatch {
        prefix: "gd",
        path: SOURCE_PATH,
        generation: "88eadaf4024b4a86",
        events: &[
            (
                "DocConceptExtracted",
                r#"{"kind":"rationale","id":"src/lib.rs#L1","title":"WHY: the entry stays small so the walk has one product file","doc":"src/lib.rs"}"#,
            ),
            (
                "DocLinkExtracted",
                r#"{"from":"src/lib.rs#L1","to":"src/lib.rs","rel":"explains"}"#,
            ),
        ],
    },
    WalkedBatch {
        prefix: "gw",
        path: WORKFLOW_PATH,
        generation: "08eb9cb734e95dc1",
        events: &[
            (
                "DocConceptExtracted",
                r#"{"kind":"agent","id":"agent:rust-engineer","title":"rust-engineer","doc":".rigger/workflow.yml"}"#,
            ),
            (
                "DocConceptExtracted",
                r#"{"kind":"gate","id":"gate:fmt","title":"fmt","doc":".rigger/workflow.yml"}"#,
            ),
            (
                "DocConceptExtracted",
                r#"{"kind":"stage","id":"stage:implement","title":"implement","doc":".rigger/workflow.yml"}"#,
            ),
            (
                "DocLinkExtracted",
                r#"{"from":"stage:implement","to":"agent:rust-engineer","rel":"RUNS"}"#,
            ),
            (
                "DocLinkExtracted",
                r#"{"from":"stage:implement","to":"gate:fmt","rel":"RUNS"}"#,
            ),
        ],
    },
];

/// The events of the batch [`WALKED`] holds under `prefix` for `path`.
pub fn walked_batch(prefix: &str, path: &str) -> &'static [(&'static str, &'static str)] {
    WALKED
        .iter()
        .find(|batch| batch.prefix == prefix && batch.path == path)
        .expect("the walk lowers a batch under the prefix for the path")
        .events
}

/// Plant the four files under `root` through `write_file`, the caller's writer of one file with
/// its parent directories.
pub fn plant_extraction_tree(root: &Path, write_file: impl Fn(&Path, &[u8])) {
    for (path, body) in [
        (SOURCE_PATH, SOURCE_BODY),
        (TEST_MODULE_PATH, TEST_MODULE_BODY),
        (DOCUMENT_PATH, DOCUMENT_BODY),
        (WORKFLOW_PATH, WORKFLOW_BODY),
    ] {
        write_file(&root.join(path), body.as_bytes());
    }
}
