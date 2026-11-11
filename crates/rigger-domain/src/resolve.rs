//! The resolver: the one rule table that resolves every name in a code view at read time. A
//! file's facts hold names as written; the rules here turn them into the entities, edges, tiers,
//! placeholders and proofs readers get, against the definitions the whole view holds. It reads
//! kinds and names only, so it holds no language knowledge.
//!
//! | Rule | Resolution |
//! |---|---|
//! | call | one same-file call-target candidate: an `extracted` edge to it; otherwise the placeholder `F::%<leaf>`, `inferred` when a candidate exists anywhere, `ambiguous` when none does |
//! | down-hop | a placeholder's call-target candidates: none, one, or a frontier |
//! | up-hop | the edges landing on a definition, or on a placeholder whose candidates hold it |
//! | tagged reference | candidates of the tag's kinds in the file's language: one same-file is `extracted`, else one in the view is `inferred`, else no edge |
//! | held reference | a tagged reference whose node already references the target, or its placeholder from that node, writes no edge |
//! | module | the first candidate path that is a code file in the view ([`module_target`]): a `CONTAINS` edge to it from the declaring definition |
//! | structure | each id's one `CONTAINS`, from the parent of the definition that settles it, else from its file |
//! | proof | a test call: one same-file product call-target candidate, else one in the view |
//! | duplicate id, kind precedence | one entity per id, its kind, line and `CONTAINS` parent by [`definition_precedence`] |
//! | test-ness | a definition's test mark, a Test-scope file, or a file in the test-module closure; any node in such a file, whatever fact names it, and any link onto one |
//! | design link, workflow relation | the node kinds and relations an intent fact may name |
//!
//! A product mention resolves against product candidates only, so test definitions never change a
//! product edge, tier, frontier or placeholder. A test call proves product definitions whatever
//! tests a reader shows, and resolves against every candidate when tests are shown.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::code_facts::{Def, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope};
use crate::code_view::{CodeView, DefRef, Proofs, Settled};
use crate::contextgraph::query::{entity_id, file_of, leaf_of, placeholder_id, split_id};
use crate::contextgraph::{
    definition_precedence, is_call_target, reference_candidate_kinds, KIND_AGENT,
    KIND_ARCH_DECISION, KIND_DESIGN_DOC, KIND_GATE, KIND_HANDBOOK_RULE, KIND_RATIONALE, KIND_STAGE,
    REL_CALLS, REL_CONSTRAINS, REL_CONTAINS, REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS,
    REL_NEEDS, REL_REFERENCES, REL_REVIEWS, REL_REVIEWS_LIGHT, REL_RUNS, REL_SPECIFIES,
    TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED,
};

/// The generation of what a store settles from the tree alone when it advances ([`Settled`]):
/// the test-module closure and the proof rule. Bump it whenever either can settle other rows for
/// a view than before; the store's fingerprint folds it in, so a snapshot settled under an older
/// one is computed anew. A golden test pins it beside a digest of the rows over a fixture, so a
/// change that is not bumped fails there.
pub const SETTLED_GENERATION: u64 = 1;

/// The node kinds a design concept or a workflow node may name.
pub(crate) const INTENT_KINDS: [&str; 7] = [
    KIND_DESIGN_DOC,
    KIND_ARCH_DECISION,
    KIND_HANDBOOK_RULE,
    KIND_RATIONALE,
    KIND_STAGE,
    KIND_GATE,
    KIND_AGENT,
];

/// The relations a design link or a workflow relation may name.
const INTENT_RELS: [&str; 9] = [
    REL_SPECIFIES,
    REL_CONSTRAINS,
    REL_GOVERNS,
    REL_EXPLAINS,
    REL_DOC_REFERENCES,
    REL_NEEDS,
    REL_RUNS,
    REL_REVIEWS,
    REL_REVIEWS_LIGHT,
];

/// The file a module declaration reaches: the first of its candidate paths that is a code file.
/// The module rule and the test-module closure both read it, so they cannot disagree.
pub fn module_target(decl: &ModuleDecl, is_code_file: impl Fn(&str) -> bool) -> Option<&str> {
    decl.candidates
        .iter()
        .map(String::as_str)
        .find(|path| is_code_file(path))
}

/// What a set of candidates resolves to: nothing, the one candidate, or a frontier of several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hop<T> {
    Terminal,
    One(T),
    Frontier(Vec<T>),
}

/// The one "exactly one" decision the rules take over a candidate set.
pub fn hop<T>(mut candidates: Vec<T>) -> Hop<T> {
    match candidates.len() {
        0 => Hop::Terminal,
        1 => Hop::One(candidates.remove(0)),
        _ => Hop::Frontier(candidates),
    }
}

/// Which candidates a mention resolves against: a product mention the product ones, a test
/// mention all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Product,
    All,
}

impl Pool {
    /// The pool a mention of this test-ness resolves against.
    pub fn of(test: bool) -> Self {
        if test {
            Pool::All
        } else {
            Pool::Product
        }
    }

    fn admits(self, c: &Candidate) -> bool {
        self == Pool::All || !c.test
    }

    /// Whether a call from this pool may land on `c`: the pool admits it and it is a call target.
    fn calls(self, c: &Candidate) -> bool {
        self.admits(c) && is_call_target(Some(c.kind))
    }
}

/// One entity id a leaf names, settled over the definitions that share it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate<'v> {
    pub id: String,
    pub file: &'v str,
    pub lang: &'v str,
    pub kind: &'v str,
    pub line: u32,
    pub test: bool,
}

/// One entity of a file: an id, settled over the definitions that share it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub id: String,
    /// The definition's own name.
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub test: bool,
}

/// One resolved edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub from: String,
    pub rel: &'static str,
    pub to: String,
    pub tier: &'static str,
    /// Whether test code produced it.
    pub test: bool,
}

/// One test call that proves a product definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofCall {
    /// The product definition proved.
    pub target: String,
    /// The test entity that calls it: the enclosing definition of the call, or the file's own id
    /// when the call sits at file level.
    pub caller: String,
    /// The call's `file:line`.
    pub evidence: String,
}

/// One node a design concept or a workflow node names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentNode {
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    /// The file it is read from.
    pub doc: String,
    /// Whether its path is test code.
    pub test: bool,
}

/// The one of `defs` sharing an id that gives the id its kind and line: the first by
/// [`definition_precedence`], the lesser kind string breaking a tie.
fn settled<T>(defs: Vec<T>, kind_line: impl Fn(&T) -> (&str, u32)) -> Option<T> {
    defs.into_iter().min_by(|a, b| {
        let (ka, la) = kind_line(a);
        let (kb, lb) = kind_line(b);
        (definition_precedence(ka, la), ka).cmp(&(definition_precedence(kb, lb), kb))
    })
}

/// The product share of `defs`, each paired with its test-ness, or all of them when none is
/// product, with whether the share is test.
fn product_else_test<T>(defs: Vec<(T, bool)>) -> (Vec<T>, bool) {
    let test = defs.iter().all(|(_, test)| *test);
    let share = defs
        .into_iter()
        .filter(|(_, t)| test || !t)
        .map(|(d, _)| d)
        .collect();
    (share, test)
}

/// The one candidate of `targets` a reference from `file` takes: the one in `file` when it holds
/// exactly one, else the one in the view; with whether it is in `file`.
fn one_here_else_one_anywhere<'c, 'v>(
    targets: &[&'c Candidate<'v>],
    file: &str,
) -> Option<(&'c Candidate<'v>, bool)> {
    let here: Vec<&Candidate> = targets.iter().copied().filter(|c| c.file == file).collect();
    match (here.as_slice(), targets) {
        ([one], _) => Some((*one, true)),
        (_, [one]) => Some((*one, false)),
        _ => None,
    }
}

/// `calls` in proof order: by the definition proved, then the calling file, then the evidence
/// `file:line` as text, then the caller.
fn proof_order(mut calls: Vec<ProofCall>) -> Vec<ProofCall> {
    fn key(c: &ProofCall) -> (&str, &str, &str, &str) {
        let file = c.evidence.rsplit_once(':').map_or("", |(file, _)| file);
        (&c.target, file, &c.evidence, &c.caller)
    }
    calls.sort_by(|a, b| key(a).cmp(&key(b)));
    calls
}

/// The test entity making a call in `facts` whose enclosing definition is `enclosing`: that
/// definition, or the file itself when the call sits at file level.
fn caller_of(facts: &FileFacts, enclosing: Option<u32>) -> String {
    enclosing
        .and_then(|i| facts.defs.get(i as usize))
        .map_or_else(|| facts.path.clone(), |d| entity_id(&facts.path, &d.local))
}

/// Grows `set` to its fixed point: the target of every declaration `admits` takes, given the set
/// so far, joins it.
fn grow<'v>(
    declared: &[(&'v str, bool, &'v str)],
    set: &mut BTreeSet<&'v str>,
    admits: impl Fn(&str, bool, &str, &BTreeSet<&'v str>) -> bool,
) {
    let mut grew = true;
    while grew {
        grew = false;
        for &(from, test, to) in declared {
            if !set.contains(to) && admits(from, test, to, set) {
                set.insert(to);
                grew = true;
            }
        }
    }
}

/// The leaves of the definitions in `facts` whose local is `local`, each once, in source order:
/// the names a call to the code entity `local` of that file is written with.
pub(crate) fn def_leaves(facts: &FileFacts, local: &str) -> Vec<String> {
    let mut leaves: Vec<String> = Vec::new();
    for d in facts.defs.iter().filter(|d| d.local == local) {
        if !leaves.contains(&d.leaf) {
            leaves.push(d.leaf.clone());
        }
    }
    leaves
}

/// The test files of `view`, whose code files are `code_files`, `declaring` those that declare a
/// module held elsewhere: the Test-scope files (`scoped`), then the test-module closure: every
/// file a test-only module declaration, or any declaration in a test file, reaches, unless a
/// product declaration reaches it. The product files are a fixed point from the roots, the code
/// files no declaration reaches outside Test scope, through every declaration not marked test
/// that a product file makes; a Test-scope file is never product.
fn test_closure<'v>(
    view: &'v dyn CodeView,
    code_files: &BTreeSet<&'v str>,
    declaring: &[&'v str],
    mut test_files: BTreeSet<&'v str>,
) -> BTreeSet<&'v str> {
    let mut declared: Vec<(&'v str, bool, &'v str)> = Vec::new();
    for &path in declaring {
        let Some(modules) = view.modules(path) else {
            continue;
        };
        for decl in modules.iter() {
            if let Some(target) = module_target(decl, |p| code_files.contains(p))
                .and_then(|target| code_files.get(target))
            {
                declared.push((path, decl.test, target));
            }
        }
    }
    let reached: BTreeSet<&str> = declared.iter().map(|&(_, _, to)| to).collect();
    let mut product: BTreeSet<&str> = code_files
        .iter()
        .copied()
        .filter(|path| !reached.contains(path) && !test_files.contains(path))
        .collect();
    grow(&declared, &mut product, |from, test, to, product| {
        !test && product.contains(from) && !test_files.contains(to)
    });
    grow(&declared, &mut test_files, |from, test, to, test_files| {
        (test || test_files.contains(from)) && !product.contains(to)
    });
    test_files
}

/// The rule table applied to one view.
pub struct Resolver<'v> {
    view: &'v dyn CodeView,
    /// What the store settled for the view, when it holds it.
    settled: Option<&'v dyn Settled>,
    /// Every code-half file of the view.
    code_files: BTreeSet<&'v str>,
    /// Every test file: Test-scope by path, or in the test-module closure.
    test_files: FxHashSet<&'v str>,
    /// The candidates of each leaf asked for, settled once per resolver.
    candidates: RefCell<FxHashMap<String, Arc<Vec<Candidate<'v>>>>>,
    /// The test calls that prove a product definition, walked once per resolver.
    proof_calls: std::cell::OnceCell<Vec<ProofCall>>,
}

impl<'v> Resolver<'v> {
    /// The resolver of `view`. Its test files are the ones the view settled, else
    /// [`test_closure`]'s.
    pub fn new(view: &'v dyn CodeView) -> Self {
        let settled = view.settled();
        let mut code_files = BTreeSet::new();
        // Only a file that declares a module held elsewhere has a declaration to read.
        let mut declaring = Vec::new();
        let mut scoped = BTreeSet::new();
        for f in view.files() {
            if f.half == Half::Code {
                code_files.insert(f.path);
                if f.modules {
                    declaring.push(f.path);
                }
            }
            if f.scope == Scope::Test {
                scoped.insert(f.path);
            }
        }
        let test_files = match settled {
            Some(settled) => settled.test_files().into_iter().collect(),
            None => test_closure(view, &code_files, &declaring, scoped)
                .into_iter()
                .collect(),
        };
        Self {
            view,
            settled,
            code_files,
            test_files,
            candidates: RefCell::default(),
            proof_calls: std::cell::OnceCell::new(),
        }
    }

    /// Whether `path` is test code by its scope or the test-module closure.
    pub fn is_test_file(&self, path: &str) -> bool {
        self.test_files.contains(path)
    }

    /// Whether the node `id` is test code: it lies in a test file, whatever fact names it.
    pub fn is_test_id(&self, id: &str) -> bool {
        file_of(id).is_some_and(|file| self.is_test_file(file))
    }

    /// Whether `path` is a code-half file of the view.
    pub fn is_code_file(&self, path: &str) -> bool {
        self.code_files.contains(path)
    }

    /// The file `decl` reaches in this view.
    pub fn module_target<'d>(&self, decl: &'d ModuleDecl) -> Option<&'d str> {
        module_target(decl, |p| self.is_code_file(p))
    }

    /// Every entity id the leaf `leaf` names in the view's code, in id order. An id a product
    /// definition holds is product, settled over its product definitions; one only test
    /// definitions hold is test.
    pub fn candidates(&self, leaf: &str) -> Arc<Vec<Candidate<'v>>> {
        if let Some(held) = self.candidates.borrow().get(leaf) {
            return Arc::clone(held);
        }
        let mut by_id: BTreeMap<(&'v str, &'v str), Vec<(DefRef<'v>, bool)>> = BTreeMap::new();
        for d in self.view.defs_named(leaf) {
            if d.file.half == Half::Code {
                let test = d.test || self.is_test_file(d.file.path);
                by_id
                    .entry((d.file.path, d.local))
                    .or_default()
                    .push((d, test));
            }
        }
        let found: Vec<Candidate<'v>> = by_id
            .into_iter()
            .filter_map(|((file, local), defs)| {
                let (share, test) = product_else_test(defs);
                let d = settled(share, |d| (d.kind, d.line))?;
                Some(Candidate {
                    id: entity_id(file, local),
                    file,
                    lang: d.file.lang,
                    kind: d.kind,
                    line: d.line,
                    test,
                })
            })
            .collect();
        let found = Arc::new(found);
        self.candidates
            .borrow_mut()
            .insert(leaf.to_string(), Arc::clone(&found));
        found
    }

    /// The call-target candidates of `leaf` in `pool`.
    fn call_targets(&self, leaf: &str, pool: Pool) -> Vec<Candidate<'v>> {
        self.candidates(leaf)
            .iter()
            .filter(|c| pool.calls(c))
            .cloned()
            .collect()
    }

    /// The call rule: the target and tier of a call to `leaf` from `file`.
    fn call(&self, file: &str, leaf: &str, pool: Pool) -> (String, &'static str) {
        let candidates = self.candidates(leaf);
        let (mut targets, mut here, mut one_here) = (0, 0, None);
        for c in candidates.iter().filter(|c| pool.calls(c)) {
            targets += 1;
            if c.file == file {
                here += 1;
                one_here = Some(c);
            }
        }
        if let (1, Some(one)) = (here, one_here) {
            return (one.id.clone(), TIER_EXTRACTED);
        }
        // No target is terminal ([`hop`]); one or more is a hop onward.
        let tier = if targets == 0 {
            TIER_AMBIGUOUS
        } else {
            TIER_INFERRED
        };
        (placeholder_id(file, leaf), tier)
    }

    /// The tagged-reference rule: the target and tier of a reference tagged `tag` to `leaf` from
    /// `file`, written in `lang`; none when no one candidate takes it.
    fn tagged(
        &self,
        file: &str,
        lang: &str,
        tag: &str,
        leaf: &str,
        pool: Pool,
    ) -> Option<(String, &'static str)> {
        let kinds = reference_candidate_kinds(tag);
        let candidates = self.candidates(leaf);
        let targets: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| pool.admits(c) && c.lang == lang && kinds.contains(&c.kind))
            .collect();
        one_here_else_one_anywhere(&targets, file).map(|(c, here)| {
            let tier = if here { TIER_EXTRACTED } else { TIER_INFERRED };
            (c.id.clone(), tier)
        })
    }

    /// The proof rule: the product definition a test call to `leaf` from `file` proves.
    pub(crate) fn proof(&self, file: &str, leaf: &str) -> Option<String> {
        let candidates = self.candidates(leaf);
        let targets: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| Pool::Product.calls(c))
            .collect();
        one_here_else_one_anywhere(&targets, file).map(|(c, _)| c.id.clone())
    }

    /// Whether `m`, a mention in `facts`, is test code.
    pub(crate) fn mention_is_test(&self, facts: &FileFacts, m: &Mention) -> bool {
        m.test || self.is_test_file(&facts.path)
    }

    /// The entities of `facts`, one per id, in id order; test entities only when `tests`.
    pub fn entities(&self, facts: &FileFacts, tests: bool) -> Vec<Entity> {
        self.winners(facts, tests)
            .into_iter()
            .map(|(local, d, test)| Entity {
                id: entity_id(&facts.path, local),
                name: d.leaf.clone(),
                kind: d.kind.clone(),
                line: d.line,
                test,
            })
            .collect()
    }

    /// The one definition of `facts` that settles each id, in id order, with its local path and
    /// whether the id is test code: the product share's first by [`definition_precedence`]
    /// ([`settled`]), the test share only when no definition of the id is product; test ids only
    /// when `tests`.
    fn winners<'f>(&self, facts: &'f FileFacts, tests: bool) -> Vec<(&'f str, &'f Def, bool)> {
        let file_test = self.is_test_file(&facts.path);
        let mut by_local: BTreeMap<&str, Vec<(&Def, bool)>> = BTreeMap::new();
        for d in &facts.defs {
            by_local
                .entry(&d.local)
                .or_default()
                .push((d, d.test || file_test));
        }
        by_local
            .into_iter()
            .filter_map(|(local, defs)| {
                let (share, test) = product_else_test(defs);
                if test && !tests {
                    return None;
                }
                let d = settled(share, |d| (d.kind.as_str(), d.line))?;
                Some((local, d, test))
            })
            .collect()
    }

    /// The code edges of `facts`: the call, tagged, held and module rules over its mentions and
    /// declarations, and each id's one `CONTAINS`, from the parent (or the file) of the definition
    /// that settles it ([`Self::winners`]); test code's edges only when `tests`.
    pub fn links(&self, facts: &FileFacts, tests: bool) -> Vec<Link> {
        let mut links = Vec::new();
        self.each_link(facts, tests, |from, rel, to, tier, test| {
            links.push(Link {
                from: from.to_string(),
                rel,
                to: to.to_string(),
                tier,
                test,
            })
        });
        links
    }

    /// [`Self::links`] handed to `sink` one at a time as (from, relation, to, tier, test), in
    /// the same order, without holding them: the mentions' edges
    /// ([`Self::each_mention_link`]), then the declarations' ([`Self::each_declared_link`]).
    pub fn each_link(
        &self,
        facts: &FileFacts,
        tests: bool,
        mut sink: impl FnMut(&str, &'static str, &str, &'static str, bool),
    ) {
        self.each_mention_link(facts, facts.mentions.iter(), tests, &mut sink);
        self.each_declared_link(facts, tests, sink);
    }

    /// The call and tagged rules' edges of `mentions`, mentions of `facts` in file order, handed
    /// to `sink` in [`Self::each_link`] order. A tagged reference a call holds stays held when
    /// `mentions` hold every call whose leaf a file-level tagged reference among them names.
    pub fn each_mention_link<'f>(
        &self,
        facts: &'f FileFacts,
        mentions: impl Iterator<Item = &'f Mention> + Clone,
        tests: bool,
        mut sink: impl FnMut(&str, &'static str, &str, &'static str, bool),
    ) {
        let file = facts.path.as_str();
        let ids: Vec<std::cell::OnceCell<String>> = vec![Default::default(); facts.defs.len()];
        let entity = |i: u32| {
            let held = ids.get(i as usize)?;
            Some(
                held.get_or_init(|| entity_id(file, &facts.defs[i as usize].local))
                    .as_str(),
            )
        };
        let from_of = |i: Option<u32>| i.and_then(entity).unwrap_or(file);
        // A product reference is held only by a product edge, so showing tests changes no
        // product edge. A call's reference runs from the file, so only one from the file is held;
        // each target maps to whether a product call references it.
        let mut referenced: FxHashMap<String, bool> = FxHashMap::default();
        for m in mentions.clone() {
            let test = self.mention_is_test(facts, m);
            if m.kind != MentionKind::Call || (test && !tests) {
                continue;
            }
            let (to, tier) = self.call(file, &m.leaf, Pool::of(test));
            if let Some(caller) = m.enclosing.and_then(entity) {
                sink(caller, REL_CALLS, &to, tier, test);
            }
            sink(file, REL_REFERENCES, &to, tier, test);
            match referenced.get_mut(&to) {
                Some(product) => *product |= !test,
                None => {
                    referenced.insert(to, !test);
                }
            }
        }
        let held = |from: &str, to: &str, test: bool| {
            from == file && referenced.get(to).is_some_and(|&product| test || product)
        };
        for m in mentions {
            let MentionKind::Tagged(tag) = &m.kind else {
                continue;
            };
            let test = self.mention_is_test(facts, m);
            if test && !tests {
                continue;
            }
            let Some((to, tier)) = self.tagged(file, &facts.lang, tag, &m.leaf, Pool::of(test))
            else {
                continue;
            };
            let from = from_of(m.enclosing);
            if held(from, &to, test) || held(from, &placeholder_id(from, &m.leaf), test) {
                continue;
            }
            sink(from, REL_REFERENCES, &to, tier, test);
        }
    }

    /// The module declarations' and the definitions' `CONTAINS` edges of `facts`, handed to
    /// `sink` in [`Self::each_link`] order.
    pub fn each_declared_link(
        &self,
        facts: &FileFacts,
        tests: bool,
        mut sink: impl FnMut(&str, &'static str, &str, &'static str, bool),
    ) {
        let file = facts.path.as_str();
        let file_test = self.is_test_file(file);
        let from_of = |i: Option<u32>| match i.and_then(|i| facts.defs.get(i as usize)) {
            Some(d) => entity_id(file, &d.local),
            None => file.to_string(),
        };
        for decl in &facts.modules {
            let test = decl.test || file_test;
            let Some(target) = self.module_target(decl).filter(|_| tests || !test) else {
                continue;
            };
            sink(
                &from_of(Some(decl.def)),
                REL_CONTAINS,
                target,
                TIER_EXTRACTED,
                test,
            );
        }
        for (local, d, test) in self.winners(facts, tests) {
            let to = entity_id(file, local);
            sink(&from_of(d.parent), REL_CONTAINS, &to, TIER_EXTRACTED, test);
        }
    }

    /// Every test call that proves a product definition, ordered by the definition proved, then
    /// the calling file, then the evidence `file:line` as text. The same whatever tests a reader
    /// shows.
    ///
    /// Read once per resolver, from the view's settled rows when it holds them, else walked over
    /// every test mention: proofs and test attachment both read this result.
    pub fn proof_calls(&self) -> &[ProofCall] {
        self.proof_calls.get_or_init(|| match self.settled {
            Some(settled) => settled.proofs(Proofs::All),
            None => self.walk_proof_calls(),
        })
    }

    /// Whether the view holds its proof calls settled, so a reader need not walk them
    /// ([`Self::file_proofs`]).
    pub fn proofs_settled(&self) -> bool {
        self.settled.is_some()
    }

    fn walk_proof_calls(&self) -> Vec<ProofCall> {
        let mut rows = Vec::new();
        for &path in &self.code_files {
            if let Some(facts) = self.view.file(path) {
                rows.extend(self.file_proofs(&facts));
            }
        }
        proof_order(rows)
    }

    /// The proofs of the test calls in `facts`, a code file, unordered.
    pub fn file_proofs(&self, facts: &FileFacts) -> Vec<ProofCall> {
        let path = facts.path.as_str();
        facts
            .mentions
            .iter()
            .filter(|m| m.kind == MentionKind::Call && self.mention_is_test(facts, m))
            .filter_map(|m| {
                let target = self.proof(path, &m.leaf)?;
                Some(ProofCall {
                    target,
                    caller: caller_of(facts, m.enclosing),
                    evidence: format!("{path}:{}", m.line),
                })
            })
            .collect()
    }

    /// The proof calls in the file at `path`, in proof order: the view's settled rows, else
    /// [`Self::file_proofs`] of its facts when it is a code file.
    pub fn proofs_in(&self, path: &str) -> Vec<ProofCall> {
        if let Some(settled) = self.settled {
            return settled.proofs(Proofs::In(path));
        }
        let facts = (self.is_code_file(path))
            .then(|| self.view.file(path))
            .flatten();
        proof_order(facts.map_or_else(Vec::new, |facts| self.file_proofs(&facts)))
    }

    /// The proof calls landing on the definition `id`, in proof order: the view's settled rows,
    /// else each test call of one of its definitions' leaves (`def_leaves`) in a code file that
    /// the proof rule resolves onto `id`, read from the mentions of those leaves alone.
    pub fn proofs_of(&self, id: &str) -> Vec<ProofCall> {
        if let Some(settled) = self.settled {
            return settled.proofs(Proofs::Of(id));
        }
        let (file, Some(local)) = split_id(id) else {
            return Vec::new();
        };
        let facts = (self.is_code_file(file))
            .then(|| self.view.file(file))
            .flatten();
        let leaves = facts.map_or_else(Vec::new, |facts| def_leaves(&facts, local));
        let mut calls = Vec::new();
        for leaf in leaves.iter().map(String::as_str) {
            for m in self.view.mentions_named(leaf) {
                let path = m.file.path;
                let test = m.test || self.is_test_file(path);
                if m.file.half != Half::Code || m.kind != MentionKind::Call || !test {
                    continue;
                }
                if self.proof(path, leaf).as_deref() == Some(id) {
                    let caller = match m.enclosing {
                        Some(_) => self
                            .view
                            .file(path)
                            .map_or_else(|| path.to_string(), |f| caller_of(&f, m.enclosing)),
                        None => path.to_string(),
                    };
                    calls.push(ProofCall {
                        target: id.to_string(),
                        caller,
                        evidence: format!("{path}:{}", m.line),
                    });
                }
            }
        }
        proof_order(calls)
    }

    /// Holds `calls`, the [`Self::file_proofs`] of every code file, as this resolver's proof calls
    /// unless it already holds them.
    pub fn settle_proofs(&self, calls: Vec<ProofCall>) {
        let _ = self.proof_calls.set(proof_order(calls));
    }

    /// Every product definition a test call proves, with its evidence `file:line`s in file then
    /// evidence order. The same whatever tests a reader shows.
    pub fn proofs(&self) -> BTreeMap<String, Vec<String>> {
        let mut proofs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for call in self.proof_calls() {
            proofs
                .entry(call.target.clone())
                .or_default()
                .push(call.evidence.clone());
        }
        proofs
    }

    /// The down-hop of `id`: what its placeholder's call-target candidates in `pool` resolve to;
    /// none when `id` is no placeholder.
    pub fn down_hop(&self, id: &str, pool: Pool) -> Option<Hop<Candidate<'v>>> {
        leaf_of(id).map(|leaf| hop(self.call_targets(leaf, pool)))
    }

    /// The up-hop of the definition `id` named `leaf`: every call and reference edge that lands
    /// on it, or on a placeholder whose candidates hold it; test edges only for [`Pool::All`].
    pub fn callers(&self, id: &str, leaf: &str, pool: Pool) -> Vec<Link> {
        let files: BTreeSet<&str> = self
            .view
            .mentions_named(leaf)
            .map(|m| m.file.path)
            .collect();
        let reaches = |l: &Link| {
            l.to == id
                || match self.down_hop(&l.to, Pool::of(l.test)) {
                    Some(Hop::One(c)) => c.id == id,
                    Some(Hop::Frontier(cs)) => cs.iter().any(|c| c.id == id),
                    Some(Hop::Terminal) | None => false,
                }
        };
        files
            .into_iter()
            .filter_map(|path| self.view.file(path))
            .flat_map(|facts| self.links(&facts, pool == Pool::All))
            .filter(|l| l.rel != REL_CONTAINS && reaches(l))
            .collect()
    }

    /// The design and workflow nodes and edges of `facts`, of the kinds and relations they may
    /// name; a link's ends are as written, and a node or link is test code by its paths.
    pub fn intent(&self, facts: &FileFacts) -> (Vec<IntentNode>, Vec<Link>) {
        let kind_of = |k: &str| INTENT_KINDS.into_iter().find(|known| *known == k);
        let rel_of = |r: &str| INTENT_RELS.into_iter().find(|known| *known == r);
        let concepts = facts
            .doc_concepts
            .iter()
            .map(|c| (&c.kind, &c.id, &c.title))
            .chain(facts.wf_nodes.iter().map(|n| (&n.kind, &n.id, &n.title)));
        let nodes = concepts
            .filter_map(|(kind, id, title)| {
                Some(IntentNode {
                    id: id.clone(),
                    kind: kind_of(kind)?,
                    title: title.clone(),
                    doc: facts.path.clone(),
                    test: self.is_test_id(id),
                })
            })
            .collect();
        let relations = facts
            .doc_links
            .iter()
            .map(|l| (&l.from, &l.rel, &l.to))
            .chain(facts.wf_relations.iter().map(|r| (&r.from, &r.rel, &r.to)));
        let links = relations
            .filter_map(|(from, rel, to)| {
                Some(Link {
                    from: from.clone(),
                    rel: rel_of(rel)?,
                    to: to.clone(),
                    tier: TIER_EXTRACTED,
                    test: self.is_test_id(from) || self.is_test_id(to),
                })
            })
            .collect();
        (nodes, links)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::code_facts::{Def, DocConcept, DocLink, Stamp, Tip, WfNode, WfRelation};
    use crate::code_view::FactsView;

    fn def(local: &str, kind: &str, line: u32, parent: Option<u32>, test: bool) -> Def {
        Def {
            local: local.into(),
            leaf: local.rsplit("::").next().unwrap().into(),
            kind: kind.into(),
            line,
            end_line: line,
            parent,
            test,
        }
    }

    fn mention(
        kind: MentionKind,
        leaf: &str,
        enclosing: Option<u32>,
        line: u32,
        test: bool,
    ) -> Mention {
        Mention {
            enclosing,
            name: leaf.into(),
            leaf: leaf.into(),
            kind,
            line,
            test,
        }
    }

    fn call(leaf: &str, enclosing: Option<u32>, line: u32, test: bool) -> Mention {
        mention(MentionKind::Call, leaf, enclosing, line, test)
    }

    fn tagged(tag: &str, leaf: &str, enclosing: Option<u32>, line: u32) -> Mention {
        mention(
            MentionKind::Tagged(tag.into()),
            leaf,
            enclosing,
            line,
            false,
        )
    }

    fn decl(name: &str, def: u32, test: bool, candidates: &[&str]) -> ModuleDecl {
        ModuleDecl {
            name: name.into(),
            def,
            test,
            candidates: candidates.iter().map(|c| c.to_string()).collect(),
        }
    }

    fn code(
        path: &str,
        lang: &str,
        scope: Scope,
        defs: Vec<Def>,
        mentions: Vec<Mention>,
    ) -> FileFacts {
        FileFacts {
            path: path.into(),
            lang: lang.into(),
            scope,
            defs,
            mentions,
            ..Default::default()
        }
    }

    pub(crate) fn stamp() -> Stamp {
        Stamp {
            tip: Tip {
                head: "main".into(),
                commit: None,
                tree: "t".into(),
            },
            fingerprint: 0,
            seq: 1,
        }
    }

    /// A tree holding a case of every rule: same-file, cross-file, ambiguous and unknown calls,
    /// tagged references (one held by a call), out-of-line and test-only module declarations, a
    /// test module and a test helper sharing product leaves, a second language, and design and
    /// workflow facts.
    pub(crate) fn fixture() -> FactsView {
        let mut a = code(
            "src/a.rs",
            "rust",
            Scope::Product,
            vec![
                def("run", "function", 1, None, false),
                def("sub", "module", 3, None, false),
                def("helper", "function", 5, None, false),
                def("Shape", "type", 10, None, false),
                def("Shape", "impl", 12, None, false),
                def("tests", "module", 20, None, true),
                def("tests::helper", "function", 21, Some(5), true),
                def("tests::checks", "function", 24, Some(5), true),
                def("a_tests", "module", 32, None, true),
            ],
            vec![
                call("helper", Some(0), 2, false),
                call("compute", Some(0), 2, false),
                call("draw", Some(0), 2, false),
                call("vanish", None, 30, false),
                tagged("type", "Shape", Some(0), 2),
                tagged("use", "compute", None, 31),
                call("helper", Some(7), 25, true),
                call("run", Some(7), 26, true),
            ],
        );
        a.modules = vec![
            decl("sub", 1, false, &["src/a/sub.rs", "src/sub.rs"]),
            decl("a_tests", 8, true, &["src/a/a_tests.rs", "src/a_tests.rs"]),
        ];
        let b = code(
            "src/b.rs",
            "rust",
            Scope::Product,
            vec![
                def("compute", "function", 3, None, false),
                def("draw", "function", 7, None, false),
                def("Config", "type", 9, None, false),
                def("Config::draw", "field", 10, Some(2), false),
            ],
            vec![
                tagged("type", "Shape", None, 1),
                tagged("type", "Widget", None, 1),
            ],
        );
        let c = code(
            "src/c.rs",
            "rust",
            Scope::Product,
            vec![
                def("draw", "function", 2, None, false),
                def("Widget", "type", 4, None, false),
            ],
            vec![],
        );
        let sub = code(
            "src/sub.rs",
            "rust",
            Scope::Product,
            vec![
                def("sub_fn", "function", 1, None, false),
                def("Widget", "type", 3, None, false),
            ],
            vec![],
        );
        let mut a_tests = code(
            "src/a_tests.rs",
            "rust",
            Scope::Product,
            vec![
                def("checks_run", "function", 3, None, false),
                def("deeper", "module", 1, None, false),
            ],
            vec![call("run", Some(0), 4, false)],
        );
        a_tests.modules = vec![decl("deeper", 1, false, &["src/a_tests/deeper.rs"])];
        let deeper = code(
            "src/a_tests/deeper.rs",
            "rust",
            Scope::Product,
            vec![def("deep_helper", "function", 1, None, false)],
            vec![call("draw", Some(0), 2, false)],
        );
        let mut it = code(
            "tests/it.rs",
            "rust",
            Scope::Test,
            vec![
                def("it_works", "function", 1, None, true),
                def("compute", "function", 5, None, false),
                def("vanish", "function", 8, None, false),
            ],
            vec![
                call("run", Some(0), 2, true),
                call("compute", Some(0), 3, true),
            ],
        );
        it.doc_concepts = vec![DocConcept {
            kind: KIND_RATIONALE.into(),
            id: "tests/it.rs#why".into(),
            title: "Why".into(),
        }];
        let py = code(
            "scripts/t.py",
            "python",
            Scope::Product,
            vec![
                def("Shape", "type", 1, None, false),
                def("assist", "function", 4, None, false),
            ],
            vec![call("compute", Some(1), 5, false)],
        );
        let doc = FileFacts {
            path: "docs/d.md".into(),
            lang: "markdown".into(),
            half: Half::Design,
            doc_concepts: vec![
                DocConcept {
                    kind: KIND_DESIGN_DOC.into(),
                    id: "docs/d.md".into(),
                    title: "D".into(),
                },
                DocConcept {
                    kind: "bogus".into(),
                    id: "bogus-concept".into(),
                    title: "B".into(),
                },
            ],
            doc_links: vec![
                DocLink {
                    from: "docs/d.md".into(),
                    rel: REL_SPECIFIES.into(),
                    to: "src/b.rs".into(),
                },
                DocLink {
                    from: "docs/d.md".into(),
                    rel: "bogus".into(),
                    to: "src/c.rs".into(),
                },
                DocLink {
                    from: "docs/d.md".into(),
                    rel: REL_SPECIFIES.into(),
                    to: "tests/it.rs".into(),
                },
                DocLink {
                    from: "docs/d.md".into(),
                    rel: REL_SPECIFIES.into(),
                    to: "src/a_tests.rs".into(),
                },
            ],
            ..Default::default()
        };
        let workflow = FileFacts {
            path: ".rigger/workflow.yml".into(),
            lang: "yaml".into(),
            half: Half::Workflow,
            wf_nodes: vec![WfNode {
                kind: KIND_STAGE.into(),
                id: "stage/plan".into(),
                title: "plan".into(),
            }],
            wf_relations: vec![WfRelation {
                from: "stage/plan".into(),
                rel: REL_RUNS.into(),
                to: "agent/planner".into(),
            }],
            ..Default::default()
        };
        FactsView::new(
            stamp(),
            vec![a, b, c, sub, a_tests, deeper, it, py, doc, workflow],
        )
    }

    /// One question put to the resolver.
    enum Ask {
        Links(&'static str, bool),
        DownHop(&'static str, Pool),
        UpHop(&'static str, &'static str),
        Proofs,
        Entities(&'static str, bool),
        TestFiles,
        Intent(&'static str),
    }

    fn render_link(l: &Link) -> String {
        format!("{} -{}/{}-> {}", l.from, l.rel, l.tier, l.to)
    }

    /// The resolver's answer to `ask` as lines.
    fn answer(view: &FactsView, r: &Resolver, ask: &Ask) -> Vec<String> {
        match ask {
            Ask::Links(path, tests) => r
                .links(&view.file(path).unwrap(), *tests)
                .iter()
                .map(render_link)
                .collect(),
            Ask::DownHop(id, pool) => vec![match r.down_hop(id, *pool) {
                None => "no placeholder".to_string(),
                Some(Hop::Terminal) => "terminal".to_string(),
                Some(Hop::One(c)) => format!("one {}", c.id),
                Some(Hop::Frontier(cs)) => {
                    let ids: Vec<_> = cs.iter().map(|c| c.id.as_str()).collect();
                    format!("frontier {}", ids.join(" "))
                }
            }],
            Ask::UpHop(id, leaf) => r
                .callers(id, leaf, Pool::Product)
                .iter()
                .map(render_link)
                .collect(),
            Ask::Proofs => r
                .proofs()
                .into_iter()
                .map(|(target, evidence)| format!("{target} <- {}", evidence.join(" ")))
                .collect(),
            Ask::Entities(path, tests) => r
                .entities(&view.file(path).unwrap(), *tests)
                .iter()
                .map(|e| {
                    let mark = if e.test { " test" } else { "" };
                    format!("{} {} {} {}{mark}", e.id, e.name, e.kind, e.line)
                })
                .collect(),
            Ask::TestFiles => view
                .files()
                .filter(|f| r.is_test_file(f.path))
                .map(|f| f.path.to_string())
                .collect(),
            Ask::Intent(path) => {
                let (nodes, links) = r.intent(&view.file(path).unwrap());
                let mark = |test: bool| if test { " test" } else { "" };
                let nodes = nodes.iter().map(|n| {
                    let test = mark(n.test);
                    format!("node {} [{}] {} {}{test}", n.id, n.kind, n.title, n.doc)
                });
                let links = links
                    .iter()
                    .map(|l| format!("{}{}", render_link(l), mark(l.test)));
                nodes.chain(links).collect()
            }
        }
    }

    /// Each row: a rule, the question that shows it, the lines the answer holds, and the text no
    /// line of it holds.
    type Row = (
        &'static str,
        Ask,
        &'static [&'static str],
        &'static [&'static str],
    );

    fn check(rows: Vec<Row>) {
        let view = fixture();
        let r = Resolver::new(&view);
        for (rule, ask, present, absent) in rows {
            let lines = answer(&view, &r, &ask);
            for want in present {
                assert!(
                    lines.iter().any(|l| l == want),
                    "{rule}: {want} not in {lines:#?}"
                );
            }
            for unwanted in absent {
                assert!(
                    !lines.iter().any(|l| l.contains(unwanted)),
                    "{rule}: {unwanted} in {lines:#?}"
                );
            }
        }
    }

    #[test]
    fn resolver_rule_table() {
        check(vec![
            (
                "call: one same-file candidate is extracted",
                Ask::Links("src/a.rs", false),
                &[
                    "src/a.rs::run -CALLS/extracted-> src/a.rs::helper",
                    "src/a.rs -REFERENCES/extracted-> src/a.rs::helper",
                ],
                &[],
            ),
            (
                "call: a candidate elsewhere is an inferred placeholder",
                Ask::Links("src/a.rs", false),
                &[
                    "src/a.rs::run -CALLS/inferred-> src/a.rs::%compute",
                    "src/a.rs::run -CALLS/inferred-> src/a.rs::%draw",
                ],
                &[],
            ),
            (
                "call: no candidate is an ambiguous placeholder from the file",
                Ask::Links("src/a.rs", false),
                &["src/a.rs -REFERENCES/ambiguous-> src/a.rs::%vanish"],
                &["CALLS/ambiguous"],
            ),
            (
                "call: candidates of any language",
                Ask::Links("scripts/t.py", false),
                &["scripts/t.py::assist -CALLS/inferred-> scripts/t.py::%compute"],
                &[],
            ),
            (
                "down-hop: one candidate",
                Ask::DownHop("src/a.rs::%compute", Pool::Product),
                &["one src/b.rs::compute"],
                &[],
            ),
            (
                "down-hop: a frontier of call targets, no field",
                Ask::DownHop("src/a.rs::%draw", Pool::Product),
                &["frontier src/b.rs::draw src/c.rs::draw"],
                &[],
            ),
            (
                "down-hop: no candidate",
                Ask::DownHop("src/a.rs::%vanish", Pool::Product),
                &["terminal"],
                &[],
            ),
            (
                "down-hop: no placeholder",
                Ask::DownHop("src/b.rs::compute", Pool::Product),
                &["no placeholder"],
                &[],
            ),
            (
                "up-hop: edges onto placeholders whose candidates hold the definition",
                Ask::UpHop("src/b.rs::compute", "compute"),
                &[
                    "src/a.rs::run -CALLS/inferred-> src/a.rs::%compute",
                    "scripts/t.py::assist -CALLS/inferred-> scripts/t.py::%compute",
                ],
                &["tests/it.rs", "CONTAINS"],
            ),
            (
                "tagged: one same-file candidate is extracted",
                Ask::Links("src/a.rs", false),
                &["src/a.rs::run -REFERENCES/extracted-> src/a.rs::Shape"],
                &[],
            ),
            (
                "tagged: one candidate of the file's language is inferred",
                Ask::Links("src/b.rs", false),
                &["src/b.rs -REFERENCES/inferred-> src/a.rs::Shape"],
                &["scripts/t.py::Shape"],
            ),
            (
                "tagged: several candidates write no edge",
                Ask::Links("src/b.rs", false),
                &[],
                &["Widget"],
            ),
            (
                "held: a tagged reference the call rule already references writes no edge",
                Ask::Links("src/a.rs", false),
                &["src/a.rs -REFERENCES/inferred-> src/a.rs::%compute"],
                &["src/b.rs::compute"],
            ),
            (
                "module: the first candidate that is a code file",
                Ask::Links("src/a.rs", false),
                &["src/a.rs::sub -CONTAINS/extracted-> src/sub.rs"],
                &["src/a/sub.rs"],
            ),
            (
                "structure: from the parent, else the file",
                Ask::Links("src/b.rs", false),
                &[
                    "src/b.rs -CONTAINS/extracted-> src/b.rs::Config",
                    "src/b.rs::Config -CONTAINS/extracted-> src/b.rs::Config::draw",
                ],
                &[],
            ),
            (
                "proof: one same-file product candidate, else one in the view",
                Ask::Proofs,
                &[
                    "src/a.rs::helper <- src/a.rs:25",
                    "src/a.rs::run <- src/a.rs:26 src/a_tests.rs:4 tests/it.rs:2",
                    "src/b.rs::compute <- tests/it.rs:3",
                ],
                &["draw", "tests/it.rs::"],
            ),
            (
                "duplicate id, kind precedence: one entity, the type's kind and line",
                Ask::Entities("src/a.rs", false),
                &["src/a.rs::Shape Shape type 10"],
                &["impl"],
            ),
            (
                "test-ness: scope, closure seed and closure",
                Ask::TestFiles,
                &["src/a_tests.rs", "src/a_tests/deeper.rs", "tests/it.rs"],
                &["src/a.rs", "src/sub.rs"],
            ),
            (
                "design link: the kinds and relations a document may name",
                Ask::Intent("docs/d.md"),
                &[
                    "node docs/d.md [design-doc] D docs/d.md",
                    "docs/d.md -SPECIFIES/extracted-> src/b.rs",
                ],
                &["bogus"],
            ),
            (
                "workflow relation: the workflow's nodes and relations",
                Ask::Intent(".rigger/workflow.yml"),
                &[
                    "node stage/plan [stage] plan .rigger/workflow.yml",
                    "stage/plan -RUNS/extracted-> agent/planner",
                ],
                &[],
            ),
        ]);
    }

    /// ONE PATH IS ONE ENTITY (spec 119 criterion 10): of same-path twins of one kind the lower
    /// line settles the id wherever it sits among them, and of twins on one line the first.
    /// A CHANGE TO WHAT A STORE SETTLES SHIPS WITH A BUMP: the fixture's test files and proof
    /// calls are pinned by digest beside the generation the store's fingerprint folds in, so a
    /// change to the test-module closure or the proof rule fails here until `SETTLED_GENERATION`
    /// is bumped and the pair re-pinned.
    #[test]
    fn a_change_to_the_settled_rows_ships_with_a_generation_bump() {
        const PINNED: (u64, &str) = (1, "b668e495a0e24465");
        let view = fixture();
        let resolver = Resolver::new(&view);
        let test_files: Vec<&str> = (view.files())
            .map(|f| f.path)
            .filter(|path| resolver.is_test_file(path))
            .collect();
        let proofs = resolver.proof_calls();
        assert!(
            !proofs.is_empty() && test_files.iter().any(|p| !p.starts_with("tests/")),
            "the fixture holds proofs and a test file by the closure alone"
        );
        let digest = crate::community::fnv1a_hex(&format!("{test_files:#?}{proofs:#?}"));
        assert_eq!(
            (SETTLED_GENERATION, digest.as_str()),
            PINNED,
            "this build's settled generation and the digest of the fixture's settled rows (left) \
             differ from the pinned pair: a change to what a store settles ships with a bump, so \
             bump `SETTLED_GENERATION` and re-pin the pair to this build's values together"
        );
    }

    #[test]
    fn settled_takes_the_lower_line_wherever_it_sits_then_the_first_of_a_tie() {
        fn kind_line<'d>(d: &'d (&'static str, u32, &'static str)) -> (&'d str, u32) {
            (d.0, d.1)
        }
        assert_eq!(
            settled(vec![("function", 9, "A"), ("function", 8, "B")], kind_line),
            Some(("function", 8, "B")),
            "the lower line, though it comes last"
        );
        assert_eq!(
            settled(vec![("function", 1, "A"), ("function", 1, "B")], kind_line),
            Some(("function", 1, "A")),
            "of one line, the first"
        );
    }

    #[test]
    fn product_tiers_ignore_test_candidates() {
        check(vec![
            (
                "a same-file test helper sharing the leaf leaves the call extracted",
                Ask::Links("src/a.rs", true),
                &["src/a.rs::run -CALLS/extracted-> src/a.rs::helper"],
                &[],
            ),
            (
                "a test definition of the leaf leaves the call ambiguous",
                Ask::Links("src/a.rs", true),
                &["src/a.rs -REFERENCES/ambiguous-> src/a.rs::%vanish"],
                &[],
            ),
            (
                "a test definition of the leaf joins no product frontier",
                Ask::DownHop("src/a.rs::%compute", Pool::Product),
                &["one src/b.rs::compute"],
                &[],
            ),
            (
                "a test call resolves against every candidate",
                Ask::Links("src/a.rs", true),
                &["src/a.rs::tests::checks -CALLS/inferred-> src/a.rs::%helper"],
                &[],
            ),
        ]);
        let view = fixture();
        let r = Resolver::new(&view);
        for path in ["src/a.rs", "src/b.rs", "scripts/t.py"] {
            let facts = view.file(path).unwrap();
            let shown: Vec<Link> = r
                .links(&facts, true)
                .into_iter()
                .filter(|l| !l.test)
                .collect();
            assert_eq!(
                shown,
                r.links(&facts, false),
                "{path}: showing tests changed a product edge"
            );
        }
    }

    #[test]
    fn test_ness_is_def_mark_or_scope_or_closure() {
        check(vec![
            (
                "a definition's own mark",
                Ask::Entities("src/a.rs", true),
                &[
                    "src/a.rs::tests::helper helper function 21 test",
                    "src/a.rs::helper helper function 5",
                ],
                &[],
            ),
            (
                "a marked definition is hidden by default",
                Ask::Entities("src/a.rs", false),
                &["src/a.rs::helper helper function 5"],
                &["tests::"],
            ),
            (
                "a Test-scope file",
                Ask::Entities("tests/it.rs", true),
                &["tests/it.rs::compute compute function 5 test"],
                &[],
            ),
            (
                "a Test-scope file is hidden by default",
                Ask::Entities("tests/it.rs", false),
                &[],
                &["tests/it.rs"],
            ),
            (
                "a file in the closure",
                Ask::Entities("src/a_tests/deeper.rs", true),
                &["src/a_tests/deeper.rs::deep_helper deep_helper function 1 test"],
                &[],
            ),
            (
                "a closure file's edges are test edges",
                Ask::Links("src/a_tests/deeper.rs", false),
                &[],
                &["deeper"],
            ),
            (
                "a design link's end by its path",
                Ask::Intent("docs/d.md"),
                &[
                    "docs/d.md -SPECIFIES/extracted-> tests/it.rs test",
                    "docs/d.md -SPECIFIES/extracted-> src/a_tests.rs test",
                    "docs/d.md -SPECIFIES/extracted-> src/b.rs",
                ],
                &[],
            ),
            (
                "an intent node in a test file",
                Ask::Intent("tests/it.rs"),
                &["node tests/it.rs#why [rationale] Why tests/it.rs test"],
                &[],
            ),
        ]);
    }

    #[test]
    fn module_target_is_shared_by_the_module_rule_and_the_closure() {
        // Each row: the case, the declaration's candidates, the code files, the target.
        type Case = (
            &'static str,
            &'static [&'static str],
            &'static [&'static str],
            Option<&'static str>,
        );
        let cases: &[Case] = &[
            (
                "the first candidate",
                &["x.rs", "y.rs"],
                &["x.rs", "y.rs"],
                Some("x.rs"),
            ),
            (
                "a candidate not in the view is passed",
                &["x.rs", "y.rs"],
                &["y.rs"],
                Some("y.rs"),
            ),
            ("no candidate in the view", &["x.rs"], &["y.rs"], None),
        ];
        for (name, candidates, code_files, want) in cases {
            let d = decl("m", 0, false, candidates);
            assert_eq!(
                module_target(&d, |p| code_files.contains(&p)),
                *want,
                "{name}"
            );
        }
        // The test-only declaration's first candidate is absent: the module rule and the closure
        // both reach its second.
        check(vec![
            (
                "the module rule",
                Ask::Links("src/a.rs", true),
                &["src/a.rs::a_tests -CONTAINS/extracted-> src/a_tests.rs"],
                &["src/a/a_tests.rs"],
            ),
            (
                "the closure",
                Ask::TestFiles,
                &["src/a_tests.rs"],
                &["src/a/a_tests.rs"],
            ),
        ]);
    }

    #[test]
    fn the_test_module_closure_holds_only_files_no_product_declaration_reaches() {
        // Each file: its path, its scope, and its module declarations as (test mark, target).
        type File = (&'static str, Scope, &'static [(bool, &'static str)]);
        // Each row: the case, the view's files, and every test file of the view.
        type Case = (&'static str, &'static [File], &'static [&'static str]);
        let cases: &[Case] = &[
            (
                "a file a product file and a test file both declare is product",
                &[
                    ("build.rs", Scope::Product, &[(false, "build/sv.rs")]),
                    ("build/sv.rs", Scope::Product, &[]),
                    ("tests/sv.rs", Scope::Test, &[(false, "build/sv.rs")]),
                ],
                &["tests/sv.rs"],
            ),
            (
                "a file reached through a product file a test file also declares is product",
                &[
                    ("src/main.rs", Scope::Product, &[(false, "src/m.rs")]),
                    ("src/m.rs", Scope::Product, &[(false, "src/m/deep.rs")]),
                    ("src/m/deep.rs", Scope::Product, &[]),
                    ("tests/m.rs", Scope::Test, &[(false, "src/m.rs")]),
                ],
                &["tests/m.rs"],
            ),
            (
                "a file a test-only and a product declaration both reach is product",
                &[
                    ("src/lib.rs", Scope::Product, &[(true, "src/x.rs")]),
                    ("src/main.rs", Scope::Product, &[(false, "src/x.rs")]),
                    ("src/x.rs", Scope::Product, &[]),
                ],
                &[],
            ),
            (
                "files only test-only declarations reach are test, and so is what they declare",
                &[
                    ("src/lib.rs", Scope::Product, &[(true, "src/t.rs")]),
                    ("src/t.rs", Scope::Product, &[(false, "src/t/deep.rs")]),
                    ("src/t/deep.rs", Scope::Product, &[]),
                    ("tests/it.rs", Scope::Test, &[(false, "src/h.rs")]),
                    ("src/h.rs", Scope::Product, &[]),
                ],
                &["src/h.rs", "src/t.rs", "src/t/deep.rs", "tests/it.rs"],
            ),
            (
                "a Test-scope file a product file declares stays test",
                &[
                    ("src/main.rs", Scope::Product, &[(false, "tests/common.rs")]),
                    ("tests/common.rs", Scope::Test, &[(false, "src/c.rs")]),
                    ("src/c.rs", Scope::Product, &[]),
                ],
                &["src/c.rs", "tests/common.rs"],
            ),
        ];
        for (name, files, want) in cases {
            let facts = files
                .iter()
                .map(|(path, scope, decls)| {
                    let mut f = code(path, "rust", *scope, vec![], vec![]);
                    f.modules = decls
                        .iter()
                        .map(|(test, target)| decl("m", 0, *test, &[target]))
                        .collect();
                    f
                })
                .collect();
            let view = FactsView::new(stamp(), facts);
            let r = Resolver::new(&view);
            let mut got: Vec<&str> = view
                .files()
                .filter(|f| r.is_test_file(f.path))
                .map(|f| f.path)
                .collect();
            got.sort_unstable();
            assert_eq!(&got, want, "{name}");
        }
    }
}
