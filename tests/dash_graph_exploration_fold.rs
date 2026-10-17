//! Periphery (API) test for the whole-graph exploration FOLD KEY (spec 42, criterion c1):
//! [`rigger::dash::cluster_key`] folds every knowledge-graph node `(id, kind)` into one exploration
//! super-node bucket, so the KG panel can render a ~7k-node graph as a few dozen clusters. A node
//! whose id NAMES A FILE under its kind's arm of `node_file`'s table clusters by that file's
//! DIRECTORY (its module); a directory-less repo-root file falls back to
//! [`rigger::dash::CLUSTER_ROOT`]; every node whose id names no file under its kind's arm - a
//! dev-loop node, or an extensionless design doc - clusters by its KIND.
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::cluster_key` and
//! `rigger::dash::CLUSTER_ROOT`). The implementer's inside-out unit test in `dash.rs` calls
//! `cluster_key` IN-MODULE, so it is structurally blind to two things this layer guards:
//!   - EXPORT REACHABILITY: that `cluster_key` and `CLUSTER_ROOT` are genuinely `pub` and reachable
//!     across the crate boundary, which is their whole reason to exist - the c2 overview and c3 drill
//!     aggregations (downstream units) consume this key. A `pub` accidentally narrowed to `pub(crate)`
//!     would keep the unit test green but break the boundary; here it fails to compile.
//!   - The BOUNDARY CLAIMS the doc-comment makes but the unit test does NOT exercise: a leading-dot
//!     dotfile, a trailing-dot (empty suffix), an empty stem, a multi-dot extension, an empty id, a
//!     trailing slash, an extensionless file sitting in a directory, each as a `file` node (which
//!     names its file whatever its last segment) and as a design doc (which names one only by its
//!     extension), and ids carrying multiple `::` or `#` separators. Each is an API edge an
//!     external caller (a real graph node) can hit.
//!
//! This layer does NOT re-assert the implementer's happy-path unit cases; it owns the crate-boundary
//! reachability and the adversarial edges of the fold contract. `dash` and `contextgraph` are compiled
//! on BOTH the default and the `--no-default-features` lane (neither is feature-gated), so this guards
//! the fold boundary in both lanes.

use rigger::contextgraph::{
    KIND_AGENT, KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, KIND_FINDING,
    KIND_RATIONALE, KIND_UNIT,
};
use rigger::dash::{cluster_key, CLUSTER_ROOT};

/// Holds each `(id, kind, cluster, why)` case: `cluster_key` folds a node `id` of `kind` to
/// exactly `cluster`.
fn assert_cluster_keys(cases: &[(&str, &str, &str, &str)]) {
    for (id, kind, cluster, why) in cases {
        assert_eq!(cluster_key(id, kind), *cluster, "{why}");
    }
}

rigger::test_cases! {
    /// The public fold key is reachable across the crate boundary and folds the three canonical
    /// node shapes as documented: an id naming a file under its kind's arm of `node_file`
    /// clusters by its file's DIRECTORY, and a decision id, naming none under its kind's arm,
    /// clusters by its KIND. This test's VALUE is structural: it proves `cluster_key` and
    /// `CLUSTER_ROOT` are genuinely `pub` and usable by an external consumer (the downstream c2/c3
    /// aggregations), which the in-module unit test cannot prove.
    cluster_key_is_reachable_over_the_public_crate_boundary: assert_cluster_keys(&[
        // A code entity `<file>::<name>` clusters by its file's module (directory).
        (
            "src/contextgraph/sqlite.rs::project",
            KIND_CODE_ENTITY,
            "src/contextgraph",
            "a code entity folds to its file's full directory path",
        ),
        // A design doc's plain path id clusters by its directory.
        (
            "docs/architecture.md",
            KIND_DESIGN_DOC,
            "docs",
            "a plain path id folds to its directory",
        ),
        // A decision id names no file under its kind's arm, so it folds to its KIND, echoed
        // verbatim.
        (
            "adj-u42c1-approve",
            KIND_DECISION,
            KIND_DECISION,
            "a decision id names no file under its kind's arm, so it folds to its kind",
        ),
        // The repo-root sentinel is reachable and is what a directory-less file folds to.
        (
            "Cargo.toml",
            KIND_FILE,
            CLUSTER_ROOT,
            "a directory-less repo-root file folds to the CLUSTER_ROOT sentinel",
        ),
    ]);
    /// The boundary EDGES the doc-comment claims but the implementer's unit test never drives:
    /// a `file` node names its file whatever its last segment, and a design doc's id names one
    /// only by the `names_a_file` extension predicate. Every assertion here pins a distinct rule
    /// an external caller (a real graph node id) can reach; a regression that relaxed any one
    /// rule stays green in the unit test but turns this test RED.
    cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate: assert_cluster_keys(&[
        // A `file` node names its file WHATEVER its last segment: a leading-dot dotfile, a
        // trailing dot, an extensionless name and a trailing slash each fold to the directory
        // their path names, a directory-less one to the repo-root sentinel.
        (
            ".gitignore",
            KIND_FILE,
            CLUSTER_ROOT,
            "a repo-root dotfile `file` node folds to (root)",
        ),
        (
            "config/.gitignore",
            KIND_FILE,
            "config",
            "a dotfile `file` node inside a directory folds to that directory",
        ),
        (
            "src/.env",
            KIND_FILE,
            "src",
            "a leading-dot last segment of a `file` node folds to its directory",
        ),
        (
            "notes/todo.",
            KIND_FILE,
            "notes",
            "a trailing-dot `file` node folds to its directory",
        ),
        (
            "src/utils/Makefile",
            KIND_FILE,
            "src/utils",
            "an extensionless `file` node folds to its directory",
        ),
        (
            "src/nested/",
            KIND_FILE,
            "src/nested",
            "a trailing-slash `file` node folds to the directory before its empty last segment",
        ),
        // A design doc names a file only when its last segment carries an extension: a `.`
        // with a non-empty stem AND a non-empty suffix. A dotfile (empty stem), a trailing dot
        // (empty suffix), an extensionless name and a trailing slash (empty last segment) name
        // none, so each folds by KIND.
        (
            "config/.gitignore",
            KIND_DESIGN_DOC,
            KIND_DESIGN_DOC,
            "a dotfile design doc has an empty stem, so it folds by kind",
        ),
        (
            "notes/todo.",
            KIND_DESIGN_DOC,
            KIND_DESIGN_DOC,
            "a trailing-dot design doc has an empty suffix, so it folds by kind",
        ),
        (
            "docs/README",
            KIND_DESIGN_DOC,
            KIND_DESIGN_DOC,
            "an extensionless design doc folds by kind",
        ),
        (
            "docs/nested/",
            KIND_DESIGN_DOC,
            KIND_DESIGN_DOC,
            "a trailing-slash design doc has an empty last segment and folds by kind",
        ),
        // MULTI-DOT EXTENSION: only the LAST `.` splits stem from suffix, so `app.min.js` is
        // extensioned and clusters by its directory (`stem = app.min`, `ext = js`).
        (
            "assets/app.min.js",
            KIND_FILE,
            "assets",
            "a multi-dot filename splits on its last dot and clusters by directory",
        ),
        // MULTIPLE `::`: the id reduces on the FIRST `::`, so a nested code-entity path
        // `<file>::<mod>::<name>` still reduces to `<file>` and clusters by its directory.
        (
            "src/a/b.rs::outer::inner",
            KIND_CODE_ENTITY,
            "src/a",
            "an id with multiple `::` reduces on the first and clusters by the file's \
             directory",
        ),
        // MULTIPLE `#`: the id reduces on the FIRST `#`, so a doc section with a nested anchor
        // `<doc>#<sec>#<sub>` reduces to `<doc>` and clusters by its directory.
        (
            "docs/spec.md#section#sub",
            KIND_DESIGN_DOC,
            "docs",
            "an id with multiple `#` reduces on the first and clusters by the doc's \
             directory",
        ),
        // A ROOT-LEVEL code entity (`<root-file>::<name>`) reduces to a directory-less file
        // and folds to the repo-root sentinel, not to a directory.
        (
            "main.rs::main",
            KIND_CODE_ENTITY,
            CLUSTER_ROOT,
            "a root-level code entity folds to the repo-root sentinel",
        ),
        // EMPTY id: totality - an empty id names no file, so it folds by KIND and never
        // panics.
        (
            "",
            KIND_DECISION,
            KIND_DECISION,
            "an empty id folds by kind (totality: no panic, no false path match)",
        ),
    ]);
}

/// The `CLUSTER_ROOT` sentinel is a load-bearing part of the contract: the c2 overview names and
/// colours the repo-root cluster by this exact value, and it must NEVER collide with a real directory
/// bucket (a directory bucket is a slash-joined path segment sequence). This pins the parenthesised
/// shape the doc-comment promises - a `(` ... `)` wrapper with no path separator - so a real directory
/// key can never equal it.
#[test]
fn cluster_root_sentinel_can_never_collide_with_a_real_directory_bucket() {
    assert!(
        CLUSTER_ROOT.starts_with('('),
        "CLUSTER_ROOT must be parenthesised so it cannot equal a real directory name"
    );
    assert!(
        CLUSTER_ROOT.ends_with(')'),
        "CLUSTER_ROOT must be parenthesised so it cannot equal a real directory name"
    );
    assert!(
        !CLUSTER_ROOT.contains('/'),
        "CLUSTER_ROOT carries no path separator, so it cannot equal a multi-segment directory key"
    );
    // And a directory bucket derived from a real path never carries the parentheses, so the two
    // namespaces stay disjoint by construction.
    let dir_bucket = cluster_key("src/dash.rs::cluster_key", KIND_CODE_ENTITY);
    assert_ne!(
        dir_bucket, CLUSTER_ROOT,
        "a real directory bucket must never equal the repo-root sentinel"
    );
}

/// The fold is FORM-INVARIANT for a file: a plain path id, a code entity `<file>::<name>`, and a
/// rationale anchor `<file>#L<n>` that all name the SAME file collapse to the one identical module
/// bucket. This is the whole point of the fold - a file's every graph node lands in one cluster - and
/// it is an API-level invariant the unit test does not state directly. The fold is also PURE: the
/// same `(id, kind)` yields the same bucket on every call.
#[test]
fn cluster_key_is_form_invariant_for_a_file_and_pure() {
    let path = cluster_key("src/conductor.rs", KIND_FILE);
    let entity = cluster_key("src/conductor.rs::route_review_tier", KIND_CODE_ENTITY);
    let rationale = cluster_key("src/conductor.rs#L20616", KIND_RATIONALE);
    assert_eq!(
        path, entity,
        "a file path and one of its code entities fold to the same module bucket"
    );
    assert_eq!(
        entity, rationale,
        "a code entity and a rationale anchor of the same file fold to the same module bucket"
    );
    assert_eq!(path, "src", "the shared bucket is the file's directory");

    // Purity/determinism across an adversarial corpus: two calls on identical input agree, so the
    // exploration overview the spec requires is deterministic by construction.
    let corpus: [(&str, &str); 8] = [
        ("src/contextgraph/mod.rs::KIND_FILE", KIND_CODE_ENTITY),
        (".gitignore", KIND_FILE),
        ("notes/todo.", KIND_FILE),
        ("assets/app.min.js", KIND_FILE),
        ("adv-u42c1-untested", KIND_FINDING),
        ("u42-c1/implementer#0", KIND_AGENT),
        ("", KIND_UNIT),
        ("main.rs::main", KIND_CODE_ENTITY),
    ];
    for (id, kind) in corpus {
        assert_eq!(
            cluster_key(id, kind),
            cluster_key(id, kind),
            "cluster_key is a pure function of (id, kind): repeated calls agree for {id:?}"
        );
    }
}
