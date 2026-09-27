//! Fixtures for the dash's lens suites: the lens under test and the graphs it folds.

use rigger::contextgraph::{Graph, KIND_CODE_ENTITY, KIND_CONCEPT, REL_REALIZES, TIER_EXTRACTED};
use rigger::dash::Lens;

use super::fixtures::{edge, labelled_node, node_with_attrs};

/// Lens `name` (`code`, `concepts`, ...) at resolution grain `1`.
pub fn lens(name: &str) -> Lens {
    Lens::from_query(Some(name), Some("1"))
}

/// The concept subject of [`shared_member_graph`].
pub const SUB_C: &str = "concept/1/0";
/// The other, larger concept of [`shared_member_graph`].
pub const OTHER_D: &str = "concept/1/1";
/// The member of [`shared_member_graph`] that realizes BOTH concepts.
pub const SHARED_MEMBER: &str = "src/a.rs::m";
/// The member of [`shared_member_graph`] that realizes only [`SUB_C`].
pub const SOLO_MEMBER: &str = "src/b.rs::n";

/// Concept [`SUB_C`] has members `{m, n}`; `m` ALSO realizes [`OTHER_D`], which has two more
/// realizers (`p`, `q`) and so is the larger concept - `m`'s PRIMARY bucket.
pub fn shared_member_graph() -> Graph {
    let def = |id: &str, name: &str| node_with_attrs(id, KIND_CODE_ENTITY, &[("name", name)]);
    let realizes = |from: &str, to: &str| edge(from, to, REL_REALIZES, TIER_EXTRACTED);
    Graph {
        nodes: vec![
            labelled_node(SUB_C, KIND_CONCEPT, Some("cc")),
            labelled_node(OTHER_D, KIND_CONCEPT, Some("dd")),
            def(SHARED_MEMBER, "m"),
            def(SOLO_MEMBER, "n"),
            def("src/c.rs::p", "p"),
            def("src/d.rs::q", "q"),
        ],
        edges: vec![
            realizes(SHARED_MEMBER, SUB_C),
            realizes(SOLO_MEMBER, SUB_C),
            realizes(SHARED_MEMBER, OTHER_D),
            realizes("src/c.rs::p", OTHER_D),
            realizes("src/d.rs::q", OTHER_D),
        ],
    }
}
