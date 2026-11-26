//! Name resolution over the graph's entity names: each name's normalised form
//! ([`super::normalise`]), sorted, with the names that carry it, searched exactly, then by prefix,
//! then by fst's Levenshtein automaton, so a term misspelt, abbreviated or written in another case
//! still reaches its entity, and a term that reaches none names its nearest names. The index holds
//! names only: a name reaches the entities the graph answers it with, read when a search reaches
//! the name ([`Held`]), and a name the graph answers with none is passed over as though unlisted.

use std::collections::{BTreeSet, HashMap};

use fst::automaton::{Levenshtein, Str};
use fst::Automaton;

use super::normalise;
use crate::contextgraph::Error;

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
    /// No entity: the nearest names the graph answers, none when it answers no name.
    Missing { nearest: Vec<String> },
}

/// The most names a prefix or fuzzy search gathers before it ranks them.
const GATHER: usize = 256;
/// The most nearest names a missing term is answered with.
pub(crate) const NEAREST: usize = 5;

/// The ids of the entities the graph answers a name with, read once per name.
pub(crate) struct Held<'a> {
    read: &'a dyn Fn(&str) -> Result<Vec<String>, Error>,
    read_once: HashMap<String, Vec<String>>,
}

impl<'a> Held<'a> {
    /// The entities `read` answers each name with, in id order.
    pub(crate) fn new(read: &'a dyn Fn(&str) -> Result<Vec<String>, Error>) -> Held<'a> {
        Held {
            read,
            read_once: HashMap::new(),
        }
    }

    /// The ids of the entities named `name`.
    fn ids(&mut self, name: &str) -> Result<&[String], Error> {
        if !self.read_once.contains_key(name) {
            let mut ids = (self.read)(name)?;
            ids.sort_unstable();
            self.read_once.insert(name.to_string(), ids);
        }
        Ok(&self.read_once[name])
    }
}

/// The graph's entity names, indexed by normalised name.
pub(crate) struct Names {
    /// The distinct normalised names, in byte order.
    keys: Vec<String>,
    /// Per key, at its place: the names sharing that normalised name, in order.
    groups: Vec<Vec<String>>,
}

impl Names {
    /// The index over `names`, each under its normalised form; a name that normalises to nothing
    /// is left out.
    pub(crate) fn build(names: impl IntoIterator<Item = String>) -> Names {
        let mut keyed: Vec<(String, String)> = (names.into_iter())
            .map(|name| (normalise(&name), name))
            .filter(|(key, _)| !key.is_empty())
            .collect();
        keyed.sort_unstable();
        keyed.dedup();
        let (mut keys, mut groups) = (Vec::<String>::new(), Vec::<Vec<String>>::new());
        for (key, name) in keyed {
            if keys.last() != Some(&key) {
                keys.push(key);
                groups.push(Vec::new());
            }
            groups.last_mut().expect("a group per key").push(name);
        }
        Names { keys, groups }
    }

    /// Resolve `term`: its leaf (the segment after its last `::`) normalised, matched exactly, then
    /// as a prefix, then within the edit distance its length allows ([`fuzziness`]), each tier
    /// over the names `held` answers some entity with.
    pub(crate) fn resolve(&self, term: &str, held: &mut Held) -> Result<Resolved, Error> {
        if let Some((tier, ids)) = self.found(term, held)? {
            return Ok(Resolved::Found { tier, ids });
        }
        let key = normalise(leaf(term));
        let nearest = if key.is_empty() {
            Vec::new()
        } else {
            self.nearest(&key, held)?
        };
        Ok(Resolved::Missing { nearest })
    }

    /// The tier and the ids [`Self::resolve`] finds `term` reaching, `None` when it reaches none.
    pub(crate) fn found(
        &self,
        term: &str,
        held: &mut Held,
    ) -> Result<Option<(Tier, Vec<String>)>, Error> {
        let leaf = leaf(term);
        let key = normalise(leaf);
        if key.is_empty() {
            return Ok(None);
        }
        if let Ok(value) = self.keys.binary_search(&key) {
            let mut exact = false;
            for name in &self.groups[value] {
                let ids = held.ids(name)?;
                exact |= !ids.is_empty() && (name == leaf || ids.iter().any(|id| id == term));
            }
            let ids = self.ids(&[value], held)?;
            if !ids.is_empty() {
                let tier = if exact { Tier::Exact } else { Tier::Variant };
                return Ok(Some((tier, ids)));
            }
        }
        let prefixed = self.gather(Str::new(&key).starts_with(), held)?;
        if !prefixed.is_empty() {
            let ids = self.ids(&ranked(prefixed, &key), held)?;
            return Ok(Some((Tier::Prefix, ids)));
        }
        if let Ok(automaton) = Levenshtein::new(&key, fuzziness(&key)) {
            let near = self.gather(automaton, held)?;
            if !near.is_empty() {
                let ids = self.ids(&ranked(near, &key), held)?;
                return Ok(Some((Tier::Fuzzy, ids)));
            }
        }
        Ok(None)
    }

    /// The held names sharing the longest prefix with `key` that any held name shares, shortest
    /// first.
    fn nearest(&self, key: &str, held: &mut Held) -> Result<Vec<String>, Error> {
        let chars: Vec<char> = key.chars().collect();
        for len in (1..chars.len()).rev() {
            let prefix: String = chars[..len].iter().collect();
            let found = self.gather(Str::new(&prefix).starts_with(), held)?;
            if !found.is_empty() {
                let mut names = BTreeSet::new();
                for value in ranked(found, key) {
                    for name in &self.groups[value] {
                        if !held.ids(name)?.is_empty() {
                            names.insert(name.clone());
                        }
                    }
                    if names.len() >= NEAREST {
                        break;
                    }
                }
                return Ok(names.into_iter().take(NEAREST).collect());
            }
        }
        Ok(Vec::new())
    }

    /// The keys `automaton` accepts, in key order, with their places, whose names `held` answers
    /// some entity with, at most [`GATHER`].
    fn gather<A: Automaton>(
        &self,
        automaton: A,
        held: &mut Held,
    ) -> Result<Vec<(String, usize)>, Error> {
        let mut found = Vec::new();
        for (value, key) in self.keys.iter().enumerate() {
            if accepts(&automaton, key) && !self.ids(&[value], held)?.is_empty() {
                found.push((key.clone(), value));
                if found.len() >= GATHER {
                    break;
                }
            }
        }
        Ok(found)
    }

    /// The ids of the entities under the keys at `values`, in id order within each key.
    fn ids(&self, values: &[usize], held: &mut Held) -> Result<Vec<String>, Error> {
        let mut all = Vec::new();
        for &v in values {
            let mut ids = Vec::new();
            for name in &self.groups[v] {
                ids.extend(held.ids(name)?.iter().cloned());
            }
            ids.sort_unstable();
            all.extend(ids);
        }
        Ok(all)
    }
}

/// The segment of `term` after its last `::`.
fn leaf(term: &str) -> &str {
    term.rsplit("::").next().unwrap_or(term)
}

/// Whether `automaton` accepts `key`, read byte by byte until no match can follow.
fn accepts(automaton: &impl Automaton, key: &str) -> bool {
    let mut state = automaton.start();
    for &byte in key.as_bytes() {
        if !automaton.can_match(&state) {
            return false;
        }
        state = automaton.accept(&state, byte);
    }
    automaton.is_match(&state)
}

/// `found`'s values, the name closest to `key` in length first, then in name order.
fn ranked(mut found: Vec<(String, usize)>, key: &str) -> Vec<usize> {
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
