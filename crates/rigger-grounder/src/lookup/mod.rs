//! The combined lookup engine (the `Lookup` interface's one implementation): one call answers a list
//! of terms from three sources at once, each in its own labelled section of one bounded reply.
//!
//! - `graph`: each term resolved against the code graph's entity names ([`names`]) and answered
//!   with its definition site and its callers and callees, every hit withheld whose file's bytes
//!   in the caller's tree are not the bytes the graph ingested ([`graph`]).
//! - `text`: a live literal scan of the caller's tree, no index ([`text`]).
//! - `prose`: the tree's prose and the graph's records ranked by relevance ([`prose`]).
//!
//! Every section is answered from the tree at the engine's root, the caller's own working tree,
//! and the one reply budget ([`budget`]) bounds the whole reply.

mod budget;
mod graph;
#[cfg(feature = "symbols")]
mod names;
mod prose;
mod text;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::contextgraph::Projection;
pub use rigger_domain::lookup::Lookup;

/// The combined lookup over the working tree at one root: the caller's own tree. It keeps one
/// thing between calls, the in-memory prose index, which each call brings up to date with the
/// tree's prose and the graph's records before it ranks.
pub struct TreeLookup {
    root: PathBuf,
    prose: Mutex<prose::ProseIndex>,
}

impl TreeLookup {
    /// The lookup over the working tree at `root`.
    pub fn new(root: impl Into<PathBuf>) -> TreeLookup {
        TreeLookup {
            root: root.into(),
            prose: Mutex::new(prose::ProseIndex::default()),
        }
    }

    /// The tree this lookup answers from.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Lookup for TreeLookup {
    fn lookup(
        &self,
        terms: &[String],
        graph: Result<&dyn Projection, String>,
    ) -> Result<String, String> {
        let terms = distinct_terms(terms);
        if terms.is_empty() {
            return Err("lookup: pass one or more non-empty terms".to_string());
        }
        let usable = graph::usable(graph);
        let graph_half = usable
            .clone()
            .and_then(|g| graph::answer(&self.root, &terms, g));
        let text_half = text::scan(&self.root, &terms);
        let records = usable.and_then(prose::records);
        let prose_half = self
            .prose
            .lock()
            .map_err(|_| "the prose index is poisoned by an earlier failed lookup".to_string())
            .and_then(|mut index| index.rank(&self.root, &terms, records));
        Ok(budget::reply(&terms, graph_half, text_half, prose_half))
    }
}

/// The non-empty terms of `terms`, trimmed, each once, in the order given.
fn distinct_terms(terms: &[String]) -> Vec<&str> {
    let mut seen = std::collections::BTreeSet::new();
    terms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty() && seen.insert(*t))
        .collect()
}

/// The lowercase words of `term`, split at every character that is not a letter or digit and at
/// every case boundary (`applyDamage`, `apply_damage`, `apply-damage` and `ApplyDamage` are all
/// `apply`, `damage`; `HTTPServer` is `http`, `server`): the one word rule name resolution and the
/// prose query share.
pub(crate) fn words(term: &str) -> Vec<String> {
    let chars: Vec<char> = term.chars().collect();
    let mut out = Vec::new();
    let mut word = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            continue;
        }
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1).copied();
        let boundary = c.is_uppercase()
            && prev.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(char::is_lowercase))
            });
        if boundary && !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        word.extend(c.to_lowercase());
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// The normalised form of a name: its [`words`] run together, so every spelling of one name
/// across snake, camel, kebab and letter case is one key.
pub(crate) fn normalise(term: &str) -> String {
    words(term).concat()
}

#[cfg(test)]
mod tests;
