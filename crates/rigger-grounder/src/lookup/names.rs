//! Name resolution over the graph's entity names: an `fst` map from each name's normalised form
//! ([`super::normalise`]) to the entities that carry it, searched exactly, then by prefix, then
//! by fst's Levenshtein automaton, so a term misspelt, abbreviated or written in another case
//! still reaches its entity, and a term that reaches none names its nearest names.

use std::collections::{BTreeMap, BTreeSet};

use fst::automaton::{Levenshtein, Str};
use fst::{Automaton, IntoStreamer, Map, Streamer};

use super::normalise;
use crate::contextgraph::query::node_name;
use crate::contextgraph::Node;

/// How a term reached its entities, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    /// An entity's name or id is the term as written.
    Exact,
    /// An entity's name is the term in another case or separator spelling.
    Variant,
    /// An entity's normalised name begins with the term's.
    Prefix,
    /// An entity's normalised name is within the term's edit distance.
    Fuzzy,
}

impl Tier {
    /// The tier's label in a reply.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tier::Exact => "exact",
            Tier::Variant => "case or separator variant",
            Tier::Prefix => "prefix",
            Tier::Fuzzy => "fuzzy",
        }
    }
}

/// What a term resolves to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    /// The entities the term reached, in name order, by the best tier that reached any.
    Found { tier: Tier, ids: Vec<String> },
    /// No entity: the nearest names the graph holds, none when it holds no name.
    Missing { nearest: Vec<String> },
}

/// The most names a prefix or fuzzy search gathers before it ranks them.
const GATHER: usize = 256;
/// The most nearest names a missing term is answered with.
const NEAREST: usize = 5;

/// The graph's named entities, indexed by normalised name.
pub(crate) struct Names {
    map: Map<Vec<u8>>,
    /// Per fst value: the entities sharing that normalised name, as indexes into `nodes`.
    groups: Vec<Vec<usize>>,
    nodes: Vec<Node>,
    by_id: BTreeMap<String, usize>,
}

impl Names {
    /// The index over `nodes`, each named node ([`node_name`]) under its name's normalised form.
    pub(crate) fn build(nodes: Vec<Node>) -> Names {
        let mut keyed: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, node) in nodes.iter().enumerate() {
            if let Some(name) = node_name(node) {
                let key = normalise(name);
                if !key.is_empty() {
                    keyed.entry(key).or_default().push(i);
                }
            }
        }
        let by_id = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut groups = Vec::with_capacity(keyed.len());
        let map = Map::from_iter(keyed.into_iter().map(|(key, members)| {
            groups.push(members);
            (key, (groups.len() - 1) as u64)
        }))
        .expect("keys from a BTreeMap are sorted and distinct");
        Names {
            map,
            groups,
            nodes,
            by_id,
        }
    }

    /// The indexed node with id `id`.
    pub(crate) fn node(&self, id: &str) -> Option<&Node> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    /// Resolve `term`: its leaf (the segment after its last `::`) normalised, matched exactly, then
    /// as a prefix, then within the edit distance its length allows ([`fuzziness`]).
    pub(crate) fn resolve(&self, term: &str) -> Resolved {
        let leaf = term.rsplit("::").next().unwrap_or(term);
        let key = normalise(leaf);
        if key.is_empty() {
            return Resolved::Missing {
                nearest: Vec::new(),
            };
        }
        if let Some(value) = self.map.get(&key) {
            let ids = self.ids(&[value]);
            let exact = self.groups[value as usize].iter().any(|&i| {
                let node = &self.nodes[i];
                node.id == term || node_name(node) == Some(leaf)
            });
            let tier = if exact { Tier::Exact } else { Tier::Variant };
            return Resolved::Found { tier, ids };
        }
        let prefixed = self.gather(Str::new(&key).starts_with());
        if !prefixed.is_empty() {
            return Resolved::Found {
                tier: Tier::Prefix,
                ids: self.ids(&ranked(prefixed, &key)),
            };
        }
        if let Ok(automaton) = Levenshtein::new(&key, fuzziness(&key)) {
            let near = self.gather(automaton);
            if !near.is_empty() {
                return Resolved::Found {
                    tier: Tier::Fuzzy,
                    ids: self.ids(&ranked(near, &key)),
                };
            }
        }
        Resolved::Missing {
            nearest: self.nearest(&key),
        }
    }

    /// The names sharing the longest prefix with `key` that any name shares, shortest first.
    fn nearest(&self, key: &str) -> Vec<String> {
        let chars: Vec<char> = key.chars().collect();
        for len in (1..chars.len()).rev() {
            let prefix: String = chars[..len].iter().collect();
            let found = self.gather(Str::new(&prefix).starts_with());
            if !found.is_empty() {
                let mut names = BTreeSet::new();
                for value in ranked(found, key) {
                    for &i in &self.groups[value as usize] {
                        if let Some(name) = node_name(&self.nodes[i]) {
                            names.insert(name.to_string());
                        }
                    }
                    if names.len() >= NEAREST {
                        break;
                    }
                }
                return names.into_iter().take(NEAREST).collect();
            }
        }
        Vec::new()
    }

    /// The (key, value) pairs `automaton` accepts, at most [`GATHER`].
    fn gather<A: Automaton>(&self, automaton: A) -> Vec<(String, u64)> {
        let mut stream = self.map.search(automaton).into_stream();
        let mut found = Vec::new();
        while let Some((key, value)) = stream.next() {
            found.push((String::from_utf8_lossy(key).into_owned(), value));
            if found.len() >= GATHER {
                break;
            }
        }
        found
    }

    /// The ids of the entities under `values`, in id order within each value.
    fn ids(&self, values: &[u64]) -> Vec<String> {
        values
            .iter()
            .flat_map(|&v| {
                let mut ids: Vec<&str> = self.groups[v as usize]
                    .iter()
                    .map(|&i| self.nodes[i].id.as_str())
                    .collect();
                ids.sort_unstable();
                ids.into_iter().map(str::to_string)
            })
            .collect()
    }
}

/// `found`'s values, the name closest to `key` in length first, then in name order.
fn ranked(mut found: Vec<(String, u64)>, key: &str) -> Vec<u64> {
    let len = key.chars().count();
    found.sort_by(|(a, _), (b, _)| {
        let gap = |s: &str| s.chars().count().abs_diff(len);
        gap(a).cmp(&gap(b)).then_with(|| a.cmp(b))
    });
    found.into_iter().map(|(_, v)| v).collect()
}

/// The edit distance a fuzzy match allows a normalised name of `key`'s length: none up to two
/// characters, one up to five, two beyond (the AUTO fuzziness convention of Lucene-family search).
fn fuzziness(key: &str) -> u32 {
    match key.chars().count() {
        0..=2 => 0,
        3..=5 => 1,
        _ => 2,
    }
}
