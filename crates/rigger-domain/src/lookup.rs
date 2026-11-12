//! The combined lookup interface: one call that answers a list of terms from the code graph, a live
//! text scan of the caller's working tree and ranked prose together. The engine lives in
//! `rigger-grounder`; the MCP server and the CLI hold it behind this trait.

use crate::contextgraph::ContextGraph;

/// The combined lookup over one working tree, the caller's own.
pub trait Lookup: Send + Sync {
    /// The reply for `terms`: a `graph` section answered from `graph` under its label (a section
    /// stating its reason when `graph` is the reason there is none: the caller could not open it,
    /// or its record half owes its rebuild), a `text` section scanned live from the tree and a
    /// `prose` section ranked over the tree's prose and the graph's records, each labelled by its
    /// source and the whole within one size budget. Fails only when `terms` holds no non-empty
    /// term.
    fn lookup(
        &self,
        terms: &[String],
        graph: Result<&dyn ContextGraph, String>,
    ) -> Result<String, String>;
}
