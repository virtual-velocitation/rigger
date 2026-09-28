//! Grounding gives each agent only the context it needs. The `Grounder` port lives in
//! `rigger-domain` and the grounders in `rigger-grounder`; this module re-exports both under the
//! historical `rigger::grounder` path.

pub use rigger_grounder::grounder::*;
