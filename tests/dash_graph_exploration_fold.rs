//! Periphery (API) test for the whole-graph exploration FOLD KEY (spec 42, criterion c1):
//! [`rigger::dash::cluster_key`] folds every knowledge-graph node `(id, kind)` into one exploration
//! super-node bucket, so the KG panel can render a ~7k-node graph as a few dozen clusters. A node
//! whose id NAMES A FILE clusters by that file's DIRECTORY (its module); a directory-less repo-root
//! file falls back to [`rigger::dash::CLUSTER_ROOT`]; every other node - a dev-loop node with no path
//! id - clusters by its KIND.
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
//!     trailing slash, an extensionless file sitting in a directory, and ids carrying multiple `::`
//!     or `#` separators. Each is an API edge an external caller (a real graph node) can hit.
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

/// The public fold key is reachable across the crate boundary and folds the three canonical node
/// shapes as documented: a path-bearing id clusters by its file's DIRECTORY, and a non-path dev-loop
/// id clusters by its KIND. This test's VALUE is structural: it proves `cluster_key` and
/// `CLUSTER_ROOT` are genuinely `pub` and usable by an external consumer (the downstream c2/c3
/// aggregations), which the in-module unit test cannot prove.
#[test]
fn cluster_key_is_reachable_over_the_public_crate_boundary() {
    // A code entity `<file>::<name>` clusters by its file's module (directory).
    assert_eq!(
        cluster_key("src/contextgraph/sqlite.rs::project", KIND_CODE_ENTITY),
        "src/contextgraph",
        "a code entity folds to its file's full directory path"
    );
    // A plain path id clusters by its directory whatever its kind.
    assert_eq!(
        cluster_key("docs/architecture.md", KIND_DESIGN_DOC),
        "docs",
        "a plain path id folds to its directory"
    );
    // A non-path dev-loop node folds to its KIND, echoed verbatim.
    assert_eq!(
        cluster_key("adj-u42c1-approve", KIND_DECISION),
        KIND_DECISION,
        "a dev-loop node with no path id folds to its kind"
    );
    // The repo-root sentinel is reachable and is what a directory-less file folds to.
    assert_eq!(
        cluster_key("Cargo.toml", KIND_FILE),
        CLUSTER_ROOT,
        "a directory-less repo-root file folds to the CLUSTER_ROOT sentinel"
    );
}

/// The boundary EDGES the doc-comment claims but the implementer's unit test never drives. Every
/// assertion here pins a distinct rule of the `names_a_file` predicate that an external caller (a real
/// graph node id) can reach; a regression that relaxed any one rule stays green in the unit test but
/// turns this test RED.
#[test]
fn cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate() {
    // LEADING-DOT DOTFILE: `.gitignore`'s only `.` is leading, so its stem is empty - it is NOT an
    // extensioned path and folds by KIND, never to a directory. (Doc: "a dotfile like `.gitignore`
    // ... is NOT treated as an extensioned path".)
    assert_eq!(
        cluster_key(".gitignore", KIND_FILE),
        KIND_FILE,
        "a repo-root leading-dot dotfile has an empty stem, so it folds by kind, not to (root)"
    );
    // A leading-dot dotfile SITTING IN A DIRECTORY still folds by kind, never to its directory - the
    // empty-stem rule fires on the last segment regardless of the parent path.
    assert_eq!(
        cluster_key("config/.gitignore", KIND_FILE),
        KIND_FILE,
        "a dotfile inside a directory still folds by kind, not to `config`"
    );
    // A bare-stem last segment like `.env` (leading dot, no directory) - empty stem again.
    assert_eq!(
        cluster_key("src/.env", KIND_FILE),
        KIND_FILE,
        "a leading-dot last segment folds by kind even with a directory present"
    );
    // TRAILING DOT: `notes.` has a non-empty stem but an EMPTY suffix, so it is not extensioned and
    // folds by KIND. (Doc: an extension needs "a non-empty stem AND a non-empty suffix".)
    assert_eq!(
        cluster_key("notes/todo.", KIND_FILE),
        KIND_FILE,
        "a trailing-dot last segment has an empty suffix, so it folds by kind"
    );
    // EXTENSIONLESS FILE IN A DIRECTORY: `Makefile` has a directory but no `.`, so it is not a file
    // and folds by KIND - a directory alone does NOT make an id a path.
    assert_eq!(
        cluster_key("src/utils/Makefile", KIND_FILE),
        KIND_FILE,
        "an extensionless last segment folds by kind even nested under directories"
    );
    // MULTI-DOT EXTENSION: only the LAST `.` splits stem from suffix, so `app.min.js` is extensioned
    // and clusters by its directory (`stem = app.min`, `ext = js`).
    assert_eq!(
        cluster_key("assets/app.min.js", KIND_FILE),
        "assets",
        "a multi-dot filename splits on its last dot and clusters by directory"
    );
    // MULTIPLE `::`: the id reduces on the FIRST `::`, so a nested code-entity path
    // `<file>::<mod>::<name>` still reduces to `<file>` and clusters by its directory.
    assert_eq!(
        cluster_key("src/a/b.rs::outer::inner", KIND_CODE_ENTITY),
        "src/a",
        "an id with multiple `::` reduces on the first and clusters by the file's directory"
    );
    // MULTIPLE `#`: the id reduces on the FIRST `#`, so a doc section with a nested anchor
    // `<doc>#<sec>#<sub>` reduces to `<doc>` and clusters by its directory.
    assert_eq!(
        cluster_key("docs/spec.md#section#sub", KIND_DESIGN_DOC),
        "docs",
        "an id with multiple `#` reduces on the first and clusters by the doc's directory"
    );
    // A ROOT-LEVEL code entity (`<root-file>::<name>`) reduces to a directory-less file and folds to
    // the repo-root sentinel, not to a directory.
    assert_eq!(
        cluster_key("main.rs::main", KIND_CODE_ENTITY),
        CLUSTER_ROOT,
        "a root-level code entity folds to the repo-root sentinel"
    );
    // EMPTY id: totality - an empty id names no file, so it folds by KIND and never panics.
    assert_eq!(
        cluster_key("", KIND_DECISION),
        KIND_DECISION,
        "an empty id folds by kind (totality: no panic, no false path match)"
    );
    // TRAILING SLASH: the last segment is empty (no extension), so a directory-shaped id folds by
    // KIND rather than being mistaken for a file.
    assert_eq!(
        cluster_key("src/nested/", KIND_FILE),
        KIND_FILE,
        "a trailing-slash id has an empty last segment and folds by kind"
    );
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
