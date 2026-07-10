//! The symbol index: a projection of the code tree into definitions, references, and a
//! name-level, per-language cross-reference graph (architecture 5.5). Designed once for its
//! several consumers (the grounder, persistence, and - in spec 16 - blast-radius).
//!
//! Dependency direction (principle 7): the `model` is PARSER-FREE and always compiled, in
//! both feature lanes, so a build without the `symbols` feature - which never links
//! tree-sitter - still compiles the model. That is the compile-time proof that no
//! `tree_sitter::` type crosses into the data-model API. tree-sitter lives ONLY behind the
//! `symbols` feature, confined to `extract` (and, in later units, the registry).

pub mod model;

/// Tags-based extraction over an INJECTED `(grammar, tag query)` pair - the ONE place
/// tree-sitter is touched. Feature-gated: the light lane drops it entirely.
#[cfg(feature = "symbols")]
pub mod extract;

/// The `extension -> (grammar, tag query)` registry: maps a file to the grammar the
/// extractor injects, for the five shipped languages, with a `--language` override
/// (unit 2). Names `tree_sitter::Language` types, so it is confined to the `symbols`
/// feature exactly like `extract`.
#[cfg(feature = "symbols")]
pub mod registry;
