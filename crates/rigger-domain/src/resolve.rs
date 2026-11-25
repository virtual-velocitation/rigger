//! The resolver: the one rule table that resolves every name in a code view at read time. A
//! file's facts hold names as written; the rules here turn them into the entities, edges, tiers,
//! placeholders and proofs readers get, against the definitions the whole view holds. It reads
//! kinds and names only, so it holds no language knowledge.
//!
//! | Rule | Resolution |
//! |---|---|
//! | qualified mention | a call or tagged reference written `Q::leaf` or `Q.leaf` keeps the candidates its rule reads whose parent is a definition named `Q` or an implementation enclosing an `impl` mention of `Q` (a product one for a product mention), else, when none is, those whose parent is a trait such an implementation names: one is an `extracted` edge to it in any file; several make a call's placeholder keyed by its qualified name (`F::%Q::leaf`, `F::%Q.leaf`), `inferred`, never one of them in its own file, and no tagged edge; none leave the call or tagged-reference rule under a member qualifier (`Q.leaf`) or a path qualifier (`Q::leaf`) naming a product type or trait in the mention's language, and make no edge and no placeholder under any other path qualifier |
//! | call | one same-file call-target candidate: an `extracted` edge to it; otherwise the placeholder `F::%<leaf>`, `inferred` when a candidate exists anywhere, `ambiguous` when none does |
//! | down-hop | a placeholder's call-target candidates, those of the leaf after the last `::` or `.` of the name it is keyed by, and for a qualified name those its qualifier keeps: none, one, or a frontier |
//! | up-hop | the edges landing on a definition, or on a placeholder whose candidates hold it |
//! | tagged reference | candidates of the tag's kinds in the file's language: one same-file is `extracted`, else one in the view is `inferred`, else no edge; a `field` reference no qualifier narrows reads its field and property candidates alone and skips the same-file step: one in the view, in its file or not, is `inferred`, else no edge, so it reaches a constant, a static or a variant only through a qualifier that keeps it |
//! | held reference | a tagged reference whose node already references the target, or the placeholder of its leaf from that node, keyed by the leaf or by a call's qualified name, writes no edge |
//! | module | the first path that is a code file in the view of its `as_child` list when the first declaration reaching the declaring file carries no `#[path]` attribute, walking down from the files no declaration names a depth at a time in path order, else of its `as_root` list ([`module_target`]): a `CONTAINS` edge to it from the declaring definition |
//! | structure | each id's one `CONTAINS`, from the parent of the definition that settles it, else from its file |
//! | proof | a test call its qualifier narrows, among every call-target candidate, test ones too: the one it keeps when that one is product, and none when it is test, it keeps several, or its qualifier makes it no link; any other test call: the one same-file call-target candidate when it is product and none when it is test, else one same-file product call-target candidate, else one in the view |
//! | duplicate id, kind precedence | one entity per id, its kind, line and `CONTAINS` parent by [`definition_precedence`] |
//! | test-ness | a definition's test mark, a Test-scope file, or a file in the test-module closure; any node in such a file, whatever fact names it, and any link onto one |
//! | design link, workflow relation | the node kinds and relations an intent fact may name |
//!
//! A product mention resolves against product candidates only, so test definitions never change a
//! product edge, tier, frontier or placeholder. A test call proves product definitions whatever
//! tests a reader shows, and resolves against every candidate when tests are shown.

use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::code_facts::{Def, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope};
use crate::code_view::{CodeView, DefRef, MentionRef, Proofs, Settled};
use crate::contextgraph::query::{
    entity_id, file_of, is_path_qualified, leaf_of, placeholder_id, split_id, split_path,
};
use crate::contextgraph::{
    definition_precedence, is_call_target, reference_candidate_kinds, referrer_label, Referrer,
    CODE_KIND_IMPL, CODE_KIND_TRAIT, KIND_AGENT, KIND_ARCH_DECISION, KIND_DESIGN_DOC, KIND_FILE,
    KIND_GATE, KIND_HANDBOOK_RULE, KIND_RATIONALE, KIND_STAGE, REF_KIND_CONST, REF_KIND_FIELD,
    REF_KIND_IMPL, REF_KIND_TYPE, REL_CALLS, REL_CONSTRAINS, REL_CONTAINS, REL_DOC_REFERENCES,
    REL_EXPLAINS, REL_GOVERNS, REL_NEEDS, REL_REFERENCES, REL_REVIEWS, REL_REVIEWS_LIGHT, REL_RUNS,
    REL_SPECIFIES, TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED,
};

/// The generation of what a store settles from the tree alone when it advances ([`Settled`]):
/// the test-module closure and the proof rule. Bump it whenever either can settle other rows for
/// a view than before; the store's fingerprint folds it in, so a snapshot settled under an older
/// one is computed anew. A golden test pins it beside a digest of the rows three fixtures
/// settle, so a change that moves those rows fails there until the generation is bumped and the
/// pair re-pinned.
pub const SETTLED_GENERATION: u64 = 5;

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

/// The file a module declaration reaches: the first path that is a code file of its `as_child`
/// list when its declaring file is a `child` ([`reach`]), else of its `as_root` list. The module
/// rule and the test-module closure both read it, so they cannot disagree.
pub fn module_target(
    decl: &ModuleDecl,
    child: bool,
    is_code_file: impl Fn(&str) -> bool,
) -> Option<&str> {
    let paths = if child { &decl.as_child } else { &decl.as_root };
    paths
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

impl<T> Hop<T> {
    /// The candidates the hop holds: none for a terminal, else the one or the frontier's.
    pub fn into_vec(self) -> Vec<T> {
        match self {
            Hop::Terminal => Vec::new(),
            Hop::One(c) => vec![c],
            Hop::Frontier(cs) => cs,
        }
    }
}

/// The one "exactly one" decision the rules take over a candidate set.
pub fn hop<T>(mut candidates: Vec<T>) -> Hop<T> {
    match candidates.len() {
        0 => Hop::Terminal,
        1 => Hop::One(candidates.remove(0)),
        _ => Hop::Frontier(candidates),
    }
}

/// What the tagged rule takes for a reference ([`Resolver::tagged`]): the one candidate it lands
/// on, with the edge's tier, or the candidates it took none from.
enum Taken<'c, 'v> {
    One(&'c Candidate<'v>, &'static str),
    Among(Vec<&'c Candidate<'v>>),
}

/// Which candidates a mention resolves against: a product mention the product ones, a test
/// mention all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    /// Whether a mention from this pool may resolve against a definition that is test code
    /// when `test`.
    fn admits(self, test: bool) -> bool {
        self == Pool::All || !test
    }

    /// Whether a call from this pool may land on `c`: the pool admits it and it is a call target.
    fn calls(self, c: &Candidate) -> bool {
        self.admits(c.test) && is_call_target(Some(c.kind))
    }

    /// Whether a reference from this pool written in `lang` may land on `c`: the pool admits
    /// it, it is in `lang` and it is one of `kinds`.
    fn references(self, c: &Candidate, lang: &str, kinds: &[&str]) -> bool {
        self.admits(c.test) && c.lang == lang && kinds.contains(&c.kind)
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
    /// The index in its file's definitions of the parent of the definition that settles the id,
    /// the one its `CONTAINS` runs from; none at file level.
    pub parent: Option<u32>,
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
    /// The test entity that calls it: the enclosing definition of the call when its id is test
    /// code, or the file's own id when the call sits at file level. None for test code inside
    /// a product entity (a `#[cfg(test)]` twin of a product definition), which proves what it
    /// calls but is no test entity of its own.
    pub caller: Option<String>,
    /// The call's `file:line`.
    pub evidence: String,
}

impl ProofCall {
    /// The file the call sits in, read off its evidence.
    pub fn file(&self) -> Option<&str> {
        self.evidence.rsplit_once(':').map(|(file, _)| file)
    }
}

/// One node a design concept or a workflow node names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentNode {
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    /// The file it is read from.
    pub doc: String,
}

/// One edge a design or workflow file writes, its ends as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentLink {
    pub from: String,
    pub rel: &'static str,
    pub to: String,
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

/// Whether a mention of `kind` is `impl`-tagged: it names a trait or type in an implementation's
/// header.
pub fn is_impl(kind: &MentionKind) -> bool {
    matches!(kind, MentionKind::Tagged(tag) if tag == REF_KIND_IMPL)
}

/// The qualifier of a mention written `name` whose leaf is `leaf`: the text before its last
/// `::` or `.` ([`split_path`]); none when it is written as its leaf alone.
pub(crate) fn qualifier<'n>(name: &'n str, leaf: &str) -> Option<&'n str> {
    if name == leaf {
        return None;
    }
    split_path(name).0
}

/// The parent definitions a qualifier names ([`Resolver::parents`]), each as (its file, its
/// index in the file's definitions), in two tiers.
struct Parents<'v> {
    /// The definitions named the qualifier and the implementations of it.
    own: FxHashSet<(&'v str, u32)>,
    /// The traits those implementations implement, read only when no candidate sits under `own`.
    traits: FxHashSet<(&'v str, u32)>,
}

/// `calls` in proof order: by the definition proved, then the calling file, then the evidence
/// `file:line` as text, then the caller.
fn proof_order(mut calls: Vec<ProofCall>) -> Vec<ProofCall> {
    fn key(c: &ProofCall) -> (&str, &str, &str, Option<&str>) {
        (
            &c.target,
            c.file().unwrap_or(""),
            &c.evidence,
            c.caller.as_deref(),
        )
    }
    calls.sort_by(|a, b| key(a).cmp(&key(b)));
    calls
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

/// The names a definition whose local path is `local` may carry as its leaf: the path itself,
/// then each text after one of its `::`, the shortest last.
pub(crate) fn local_leaves(local: &str) -> impl Iterator<Item = &str> {
    let suffixes = local
        .match_indices("::")
        .map(move |(at, sep)| &local[at + sep.len()..]);
    std::iter::once(local).chain(suffixes)
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

/// Where the module declarations of `view`, whose code files are `code_files`, `declaring` those
/// that declare a module held elsewhere, place their files: the files each reaches as a child,
/// and every declaration that reaches a code file, as (declaring file, test mark, target). The
/// walk starts at the roots, the declaring files no declaration's list names, and goes down a
/// depth at a time, each depth in path order. The first declaration to reach a file decides its
/// place: a child, whose declarations read their `as_child` lists, unless that declaration
/// carries a `#[path]` attribute. When a depth runs out, the first declaring file in path order
/// the walk has not visited starts the next as a root, so a cycle no root reaches is walked.
fn reach<'v>(
    view: &'v dyn CodeView,
    code_files: &BTreeSet<&'v str>,
    declaring: &[&'v str],
) -> (FxHashSet<&'v str>, Vec<(&'v str, bool, &'v str)>) {
    let modules: BTreeMap<&'v str, Cow<'v, [ModuleDecl]>> = (declaring.iter())
        .filter_map(|&path| Some((path, view.modules(path)?)))
        .collect();
    let named: FxHashSet<&str> = (modules.values().flat_map(|decls| decls.iter()))
        .flat_map(|decl| decl.as_root.iter().chain(&decl.as_child))
        .map(String::as_str)
        .collect();
    let mut depth: BTreeSet<&'v str> = (modules.keys().copied())
        .filter(|path| !named.contains(path))
        .collect();
    let mut known = depth.clone();
    let (mut children, mut declared) = (FxHashSet::default(), Vec::new());
    loop {
        if depth.is_empty() {
            let Some(&root) = modules.keys().find(|path| !known.contains(*path)) else {
                break;
            };
            known.insert(root);
            depth.insert(root);
        }
        let mut next = BTreeSet::new();
        for from in std::mem::take(&mut depth) {
            let child = children.contains(from);
            for decl in modules[from].iter() {
                let Some(&to) = module_target(decl, child, |p| code_files.contains(p))
                    .and_then(|to| code_files.get(to))
                else {
                    continue;
                };
                declared.push((from, decl.test, to));
                if known.insert(to) {
                    if !decl.path_attr {
                        children.insert(to);
                    }
                    if modules.contains_key(to) {
                        next.insert(to);
                    }
                }
            }
        }
        depth = next;
    }
    (children, declared)
}

/// The test files of a view whose code files are `code_files` and whose module declarations
/// reach the files `declared` names ([`reach`]): the Test-scope files it is passed
/// (`test_files`), then the test-module closure: every file a test-only module declaration, or
/// any declaration in a test file, reaches, unless a product declaration reaches it. The product
/// files are a fixed point from the roots, the code files no declaration reaches outside Test
/// scope, through every declaration not marked test that a product file makes; a Test-scope
/// file is never product.
fn test_closure<'v>(
    code_files: &BTreeSet<&'v str>,
    declared: &[(&'v str, bool, &'v str)],
    mut test_files: BTreeSet<&'v str>,
) -> BTreeSet<&'v str> {
    let reached: BTreeSet<&str> = declared.iter().map(|&(_, _, to)| to).collect();
    let mut product: BTreeSet<&str> = code_files
        .iter()
        .copied()
        .filter(|path| !reached.contains(path) && !test_files.contains(path))
        .collect();
    grow(declared, &mut product, |from, test, to, product| {
        !test && product.contains(from) && !test_files.contains(to)
    });
    grow(declared, &mut test_files, |from, test, to, test_files| {
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
    /// Every file a module declaration reaches as a child ([`reach`]).
    children: FxHashSet<&'v str>,
    /// Every test file: Test-scope by path, or in the test-module closure.
    test_files: FxHashSet<&'v str>,
    /// The candidates of each leaf asked for, settled once per resolver.
    candidates: RefCell<FxHashMap<String, Arc<Vec<Candidate<'v>>>>>,
    /// The parent definitions of each qualifier asked for in each pool ([`Self::parents`]),
    /// gathered once per resolver.
    parents: RefCell<FxHashMap<(String, Pool), Arc<Parents<'v>>>>,
    /// The test calls that prove a product definition, walked once per resolver.
    proof_calls: std::cell::OnceCell<Vec<ProofCall>>,
}

impl<'v> Resolver<'v> {
    /// The resolver of `view`. Where its module declarations place their files is walked once
    /// ([`reach`]); its test files are the ones the view settled, else [`test_closure`]'s over
    /// that walk.
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
        let (children, declared) = reach(view, &code_files, &declaring);
        let test_files = match settled {
            Some(settled) => settled.test_files().into_iter().collect(),
            None => test_closure(&code_files, &declared, scoped)
                .into_iter()
                .collect(),
        };
        Self {
            view,
            settled,
            code_files,
            children,
            test_files,
            candidates: RefCell::default(),
            parents: RefCell::default(),
            proof_calls: std::cell::OnceCell::new(),
        }
    }

    /// Whether `path` is test code by its scope or the test-module closure.
    pub fn is_test_file(&self, path: &str) -> bool {
        self.test_files.contains(path)
    }

    /// Every test file, in path order ([`Settled::test_files`]).
    pub fn test_files(&self) -> Vec<&'v str> {
        let mut files: Vec<&'v str> = self.test_files.iter().copied().collect();
        files.sort_unstable();
        files
    }

    /// Whether the node `id` is test code, whatever fact names it: it lies in a test file, or it
    /// is a test entity of a code file (section 8.1), as [`Self::candidates`] settles the id.
    pub fn is_test_id(&self, id: &str) -> bool {
        if file_of(id).is_some_and(|file| self.is_test_file(file)) {
            return true;
        }
        let Some(local) = split_id(id).1 else {
            return false;
        };
        local_leaves(local).any(|leaf| (self.candidates(leaf).iter()).any(|c| c.id == id && c.test))
    }

    /// Whether the design or workflow edge `from` -> `to` is test code: an end of it is
    /// ([`Self::is_test_id`]).
    pub fn is_test_link(&self, from: &str, to: &str) -> bool {
        self.is_test_id(from) || self.is_test_id(to)
    }

    /// Whether `path` is a code-half file of the view.
    pub fn is_code_file(&self, path: &str) -> bool {
        self.code_files.contains(path)
    }

    /// The file `decl`, a declaration of the file at `file`, reaches in this view: through its
    /// `as_child` list when `file` is a child ([`reach`]), else through its `as_root` list.
    pub fn module_target<'d>(&self, file: &str, decl: &'d ModuleDecl) -> Option<&'d str> {
        module_target(decl, self.children.contains(file), |p| self.is_code_file(p))
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
                let test = self.is_test(d.test, d.file.path);
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
                    parent: d.parent,
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

    /// The candidates of its rule, from `pool`, that a mention written `name` in `lang` to
    /// `leaf` keeps by its qualifier ([`qualifier`]): those whose parent in their file is in the
    /// qualifier's own tier of [`Self::parents`] in `pool`, else, when none is, those whose
    /// parent is in its trait tier. Terminal when the mention is unqualified, or its qualifier
    /// keeps none and is a member qualifier (`Q.leaf`) or a path qualifier (`Q::leaf`) that
    /// names a product type or trait in `lang` ([`Self::names_a_type`]), so the unqualified rule
    /// takes it; none when it keeps none and is any other path qualifier, so the mention makes
    /// no link.
    fn narrowed<'c>(
        &self,
        lang: &str,
        name: &str,
        leaf: &str,
        pool: Pool,
        candidates: impl Iterator<Item = &'c Candidate<'v>>,
    ) -> Option<Hop<&'c Candidate<'v>>> {
        let Some(qualifier) = qualifier(name, leaf) else {
            return Some(Hop::Terminal);
        };
        let kept = self.kept(qualifier, pool, candidates);
        if kept.is_empty() && is_path_qualified(name) && !self.names_a_type(qualifier, lang) {
            return None;
        }
        Some(hop(kept))
    }

    /// The `candidates` `qualifier` keeps in `pool`: those whose parent in their file is in its
    /// own tier of [`Self::parents`], else, when none is, those whose parent is in its trait
    /// tier, in the order given. The one split [`Self::narrowed`] and [`Self::down_hop`] read.
    fn kept<'c>(
        &self,
        qualifier: &str,
        pool: Pool,
        candidates: impl Iterator<Item = &'c Candidate<'v>>,
    ) -> Vec<&'c Candidate<'v>> {
        let parents = self.parents(qualifier, pool);
        let (mut own, mut traits) = (Vec::new(), Vec::new());
        for c in candidates {
            let Some(at) = c.parent.map(|p| (c.file, p)) else {
                continue;
            };
            if parents.own.contains(&at) {
                own.push(c);
            } else if parents.traits.contains(&at) {
                traits.push(c);
            }
        }
        if own.is_empty() {
            traits
        } else {
            own
        }
    }

    /// Whether `qualifier`, whole, names a product type or trait in `lang`: a definition a
    /// product type reference written in `lang` may land on ([`Pool::references`]), whatever
    /// the pool of the mention it qualifies, so a test type names no qualifier.
    fn names_a_type(&self, qualifier: &str, lang: &str) -> bool {
        let kinds = reference_candidate_kinds(REF_KIND_TYPE);
        (self.candidates(qualifier).iter()).any(|c| Pool::Product.references(c, lang, &kinds))
    }

    /// The parent definitions `qualifier` names, as (file, index in the file's definitions), in
    /// two tiers. Its own: every definition whose leaf it is, and every implementation (an
    /// `impl`-kind definition) enclosing an `impl`-tagged mention of it, so `T::new` reaches the
    /// methods of `impl T` and `T::m` the `m` that `impl Tr for T` defines. Its traits: every
    /// trait an `impl`-tagged mention such an implementation encloses names, so `T::m` reaches
    /// a trait's `m` through `impl Tr for T` when [`Self::narrowed`] keeps no candidate of the
    /// mention's rule under `T` or its implementations. A definition of another kind enclosing
    /// an `impl` mention of it, a class naming its base, adds nothing; nor does an
    /// implementation that is test code when `pool` does not admit it ([`Pool::admits`]), so a
    /// test implementation changes no product edge. Gathered once per qualifier and pool, by name
    /// alone in every language, as the call rule reads.
    fn parents(&self, qualifier: &str, pool: Pool) -> Arc<Parents<'v>> {
        let key = (qualifier.to_string(), pool);
        if let Some(held) = self.parents.borrow().get(&key) {
            return Arc::clone(held);
        }
        let mut parents = Parents {
            own: (self.view.defs_named(qualifier))
                .map(|d| (d.file.path, d.index))
                .collect(),
            traits: FxHashSet::default(),
        };
        // The mentions come in path order, so each file is read once.
        let mut facts: Option<Cow<'v, FileFacts>> = None;
        for m in self.view.mentions_named(qualifier) {
            let Some(at) = m.enclosing.filter(|_| is_impl(&m.kind)) else {
                continue;
            };
            if facts.as_ref().is_none_or(|f| f.path != m.file.path) {
                facts = self.view.file(m.file.path);
            }
            let Some(facts) = &facts else {
                continue;
            };
            let admitted = |d: &Def| {
                d.kind == CODE_KIND_IMPL && pool.admits(self.is_test(d.test, m.file.path))
            };
            if !(facts.defs.get(at as usize)).is_some_and(admitted) {
                continue;
            }
            parents.own.insert((m.file.path, at));
            let names =
                (facts.mentions.iter()).filter(|n| n.enclosing == Some(at) && is_impl(&n.kind));
            for named in names {
                parents.traits.extend(
                    (self.view.defs_named(&named.leaf))
                        .filter(|d| d.kind == CODE_KIND_TRAIT)
                        .map(|d| (d.file.path, d.index)),
                );
            }
        }
        let parents = Arc::new(parents);
        self.parents.borrow_mut().insert(key, Arc::clone(&parents));
        parents
    }

    /// The call rule: the target and tier of a call written `name` to `leaf` from `file`, in
    /// `lang`. A qualified call its qualifier narrows ([`Self::narrowed`]) lands on the one call
    /// target it keeps, EXTRACTED wherever it is, or, when it keeps several, on the placeholder
    /// `F::%<name>` keyed by its qualified name (`F::%Q::leaf`, `F::%Q.leaf`), INFERRED, never on
    /// a kept target in `file`; it lands nowhere when its qualifier makes it no link. Any other
    /// call lands on its one same-file call target, EXTRACTED, else on the placeholder
    /// `F::%<leaf>`.
    fn call(
        &self,
        file: &str,
        lang: &str,
        name: &str,
        leaf: &str,
        pool: Pool,
    ) -> Option<(String, &'static str)> {
        let candidates = self.candidates(leaf);
        let in_pool = candidates.iter().filter(|c| pool.calls(c));
        match self.narrowed(lang, name, leaf, pool, in_pool.clone())? {
            Hop::One(one) => return Some((one.id.clone(), TIER_EXTRACTED)),
            Hop::Frontier(_) => return Some((placeholder_id(file, name), TIER_INFERRED)),
            Hop::Terminal => {}
        }
        let (mut targets, mut here, mut one_here) = (0, 0, None);
        for c in in_pool {
            targets += 1;
            if c.file == file {
                here += 1;
                one_here = Some(c);
            }
        }
        if let (1, Some(one)) = (here, one_here) {
            return Some((one.id.clone(), TIER_EXTRACTED));
        }
        // No target is terminal ([`hop`]); one or more is a hop onward.
        let tier = if targets == 0 {
            TIER_AMBIGUOUS
        } else {
            TIER_INFERRED
        };
        Some((placeholder_id(file, leaf), tier))
    }

    /// The tagged-reference rule: what the reference `m` of `facts`, tagged `tag`, takes from
    /// `pool` over `candidates`, those of its leaf, kept to the tag's kinds in its file's language
    /// ([`Pool::references`]). It takes the one its qualifier narrows them to ([`Self::narrowed`]),
    /// EXTRACTED wherever it is, and none, among those it keeps, when it narrows them to several,
    /// or among none when its qualifier makes it no link; else, for a `field` reference, the one
    /// field or property candidate in the view, INFERRED even in its file, so it never prefers
    /// its file and reaches a constant, a static or a variant only through a qualifier that keeps
    /// it, and none among those when there are several; else the one same-file candidate,
    /// EXTRACTED, else the one in the view, INFERRED; none among them all when no one takes it.
    fn tagged<'c>(
        &self,
        facts: &FileFacts,
        m: &Mention,
        tag: &str,
        pool: Pool,
        candidates: &'c [Candidate<'v>],
    ) -> Taken<'c, 'v> {
        let (lang, kinds) = (facts.lang.as_str(), reference_candidate_kinds(tag));
        let targets: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| pool.references(c, lang, &kinds))
            .collect();
        match self.narrowed(lang, &m.name, &m.leaf, pool, targets.iter().copied()) {
            None => return Taken::Among(Vec::new()),
            Some(Hop::One(one)) => return Taken::One(one, TIER_EXTRACTED),
            Some(Hop::Frontier(kept)) => return Taken::Among(kept),
            Some(Hop::Terminal) => {}
        }
        if tag == REF_KIND_FIELD {
            let consts = reference_candidate_kinds(REF_KIND_CONST);
            let members = targets.into_iter().filter(|c| !consts.contains(&c.kind));
            return match hop(members.collect()) {
                Hop::One(one) => Taken::One(one, TIER_INFERRED),
                several => Taken::Among(several.into_vec()),
            };
        }
        match one_here_else_one_anywhere(&targets, &facts.path) {
            Some((c, true)) => Taken::One(c, TIER_EXTRACTED),
            Some((c, false)) => Taken::One(c, TIER_INFERRED),
            None => Taken::Among(targets),
        }
    }

    /// The proof rule: the product definition a test call written `name` to `leaf` from `file`,
    /// in `lang`, proves. Its qualifier narrows every call target of `leaf`, test ones too, as
    /// the call rule narrows a test call ([`Self::narrowed`]): the call proves the one it keeps
    /// when that one is product, and none when it is test, it keeps several, or its qualifier
    /// makes the call no link. Any other call, one the unqualified rule takes, from a `file`
    /// holding one call target of `leaf` proves that definition when it is product and none when
    /// it is test, and otherwise the one product call target in `file`, else the one in the view.
    /// So a test call's edge and its proof name one definition, or the proof none.
    pub(crate) fn proof(&self, file: &str, lang: &str, name: &str, leaf: &str) -> Option<String> {
        let candidates = self.candidates(leaf);
        let calls = candidates.iter().filter(|c| Pool::All.calls(c));
        match self.narrowed(lang, name, leaf, Pool::All, calls.clone())? {
            Hop::One(one) => return (!one.test).then(|| one.id.clone()),
            Hop::Frontier(_) => return None,
            Hop::Terminal => {}
        }
        let mut here = calls.filter(|c| c.file == file);
        if let (Some(one), None) = (here.next(), here.next()) {
            return (!one.test).then(|| one.id.clone());
        }
        let targets: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| Pool::Product.calls(c))
            .collect();
        one_here_else_one_anywhere(&targets, file).map(|(c, _)| c.id.clone())
    }

    /// The call-edge rule: where the edge of a call written `name` to `leaf` from `file`, in
    /// `lang`, lands, and its tier. A product call, and a test call the call rule resolves
    /// EXTRACTED, take the call rule. A test call the call rule sends to a placeholder lands,
    /// INFERRED, on the definition the proof rule credits with it, so a test's call edge and its
    /// proof name one definition; on the placeholder when the proof rule credits none, as it
    /// credits none for a call its qualifier narrows to several. INFERRED even when that
    /// definition is in `file`, where the call rule found no one target: a same-file test
    /// definition of the leaf contested it. None when the call rule makes the call no link, a
    /// test call too, which the proof rule then credits to no definition either.
    fn call_edge(
        &self,
        file: &str,
        lang: &str,
        name: &str,
        leaf: &str,
        test: bool,
    ) -> Option<(String, &'static str)> {
        let (to, tier) = self.call(file, lang, name, leaf, Pool::of(test))?;
        if !test || tier == TIER_EXTRACTED {
            return Some((to, tier));
        }
        Some(
            self.proof(file, lang, name, leaf)
                .map_or((to, tier), |id| (id, TIER_INFERRED)),
        )
    }

    /// The ids the mention `m` of `facts`, test code when `test`, lists its referrer under (spec
    /// 117, READERS): those its own rule's candidates hold, or the one it settles. A call lists
    /// under the definition its edge lands on ([`Self::call_edge`]), else under the candidates
    /// the placeholder it lands on keeps in its pool ([`Self::down_hop`]), and under none when
    /// the call rule makes it no link. A tagged reference lists under the one candidate its rule
    /// takes, else under those it took none from ([`Self::tagged`]).
    fn listed(&self, facts: &FileFacts, m: &Mention, test: bool) -> Vec<String> {
        let pool = Pool::of(test);
        let MentionKind::Tagged(tag) = &m.kind else {
            let edge = self.call_edge(&facts.path, &facts.lang, &m.name, &m.leaf, test);
            return match edge.map(|(to, _)| (self.down_hop(&to, pool), to)) {
                None => Vec::new(),
                Some((None, to)) => vec![to],
                Some((Some(hop), _)) => hop.into_vec().into_iter().map(|c| c.id).collect(),
            };
        };
        let candidates = self.candidates(&m.leaf);
        match self.tagged(facts, m, tag, pool, &candidates) {
            Taken::One(c, _) => vec![c.id.clone()],
            Taken::Among(cs) => cs.into_iter().map(|c| c.id.clone()).collect(),
        }
    }

    /// The referrers `mentions`, mentions of `facts` in file order, make, each with an id it is
    /// listed under ([`Self::listed`]): one per mention and id, in mention order. A mention of a
    /// tag [`crate::contextgraph::REFERRER_LABELS`] holds no row for makes none.
    pub fn file_referrers<'f>(
        &self,
        facts: &'f FileFacts,
        mentions: impl Iterator<Item = &'f Mention>,
    ) -> Vec<(String, Referrer)> {
        // Every local of `facts` has one settling definition, test or product.
        let kinds: FxHashMap<&str, &str> = (self.winners(facts, true).into_iter())
            .map(|(local, d, _)| (local, d.kind.as_str()))
            .collect();
        let mut referrers = Vec::new();
        for m in mentions {
            let Some(label) = referrer_label(&m.kind) else {
                continue;
            };
            let test = self.mention_is_test(facts, m);
            let listed = self.listed(facts, m, test);
            let (site, kind) = match m.enclosing.and_then(|i| facts.defs.get(i as usize)) {
                Some(d) => (entity_id(&facts.path, &d.local), kinds[d.local.as_str()]),
                None => (facts.path.clone(), KIND_FILE),
            };
            for to in listed {
                let (site, kind) = (site.clone(), kind.to_string());
                let line = m.line;
                referrers.push((
                    to,
                    Referrer {
                        site,
                        kind,
                        label,
                        line,
                        test,
                    },
                ));
            }
        }
        referrers
    }

    /// Every referrer of the code entity `id` ([`Self::file_referrers`]), in path then source
    /// order, read from the code files holding a mention of a name its definitions carry
    /// ([`def_leaves`]); none for an id no definition of a code file holds.
    pub fn referrers(&self, id: &str) -> Vec<Referrer> {
        let (file, Some(local)) = split_id(id) else {
            return Vec::new();
        };
        let facts = (self.is_code_file(file))
            .then(|| self.view.file(file))
            .flatten();
        let leaves = facts.map_or_else(Vec::new, |facts| def_leaves(&facts, local));
        let files: BTreeSet<&str> = (leaves.iter())
            .flat_map(|leaf| self.view.mentions_named(leaf))
            .filter(|m| m.file.half == Half::Code)
            .map(|m| m.file.path)
            .collect();
        let mut referrers = Vec::new();
        for facts in files.into_iter().filter_map(|path| self.view.file(path)) {
            let named = facts.mentions.iter().filter(|m| leaves.contains(&m.leaf));
            let listed = self.file_referrers(&facts, named).into_iter();
            referrers.extend(listed.filter_map(|(to, r)| (to == id).then_some(r)));
        }
        referrers
    }

    /// Whether `m`, a mention in `facts`, is test code.
    pub(crate) fn mention_is_test(&self, facts: &FileFacts, m: &Mention) -> bool {
        self.is_test(m.test, &facts.path)
    }

    /// The test calls of `facts`: its call mentions that are test code, in source order.
    fn test_calls<'a>(
        &'a self,
        facts: &'a FileFacts,
    ) -> impl Iterator<Item = &'a Mention> + use<'a, 'v> {
        (facts.mentions.iter())
            .filter(move |m| m.kind == MentionKind::Call && self.mention_is_test(facts, m))
    }

    /// Whether a fact in `path` that its own mark calls test code when `marked` is test code: by
    /// that mark, or by its file's scope or the test-module closure.
    pub(crate) fn is_test(&self, marked: bool, path: &str) -> bool {
        marked || self.is_test_file(path)
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

    /// The call-edge and tagged rules' edges of `mentions`, mentions of `facts` in file order, handed
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
        // each target maps to whether a product call references it, and a call landing on a
        // placeholder, under its leaf or its qualified name, references its leaf's `F::%<leaf>`.
        let mut referenced: FxHashMap<String, bool> = FxHashMap::default();
        let mut note = |to: String, test: bool| *referenced.entry(to).or_default() |= !test;
        for m in mentions.clone() {
            let test = self.mention_is_test(facts, m);
            if m.kind != MentionKind::Call || (test && !tests) {
                continue;
            }
            let Some((to, tier)) = self.call_edge(file, &facts.lang, &m.name, &m.leaf, test) else {
                continue;
            };
            sink(from_of(m.enclosing), REL_CALLS, &to, tier, test);
            sink(file, REL_REFERENCES, &to, tier, test);
            if leaf_of(&to).is_some() {
                note(placeholder_id(file, &m.leaf), test);
            }
            note(to, test);
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
            let candidates = self.candidates(&m.leaf);
            let Taken::One(to, tier) = self.tagged(facts, m, tag, Pool::of(test), &candidates)
            else {
                continue;
            };
            let (from, to) = (from_of(m.enclosing), to.id.as_str());
            if held(from, to, test) || held(from, &placeholder_id(from, &m.leaf), test) {
                continue;
            }
            sink(from, REL_REFERENCES, to, tier, test);
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
            let Some(target) = self.module_target(file, decl) else {
                continue;
            };
            // A declaration reaching a test file is test code wherever it is written.
            let test = decl.test || file_test || self.is_test_file(target);
            if test && !tests {
                continue;
            }
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

    /// [`Self::proof_calls`], moved out of the resolver rather than copied.
    pub fn into_proof_calls(self) -> Vec<ProofCall> {
        self.proof_calls();
        self.proof_calls.into_inner().unwrap_or_default()
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
        let test_entity = self.test_entity_of(facts);
        self.test_calls(facts)
            .filter_map(|m| {
                let target = self.proof(path, &facts.lang, &m.name, &m.leaf)?;
                Some(ProofCall {
                    target,
                    caller: test_entity(m.enclosing),
                    evidence: format!("{path}:{}", m.line),
                })
            })
            .collect()
    }

    /// The test entity a test mention of `facts` belongs to, by its enclosing definition: that
    /// definition's id when the id is test code by the rule settling ids ([`Self::winners`]), the
    /// file's own id when the mention sits at file level of a test file. None for test code
    /// inside a product entity (a `#[cfg(test)]` twin of a product definition): the id is
    /// product, and no entity holds the test code alone.
    fn test_entity_of<'a>(
        &'a self,
        facts: &'a FileFacts,
    ) -> impl Fn(Option<u32>) -> Option<String> + use<'a, 'v> {
        let file_test = self.is_test_file(&facts.path);
        let test_ids: std::cell::OnceCell<FxHashSet<&str>> = std::cell::OnceCell::new();
        move |enclosing| {
            let Some(d) = enclosing.and_then(|i| facts.defs.get(i as usize)) else {
                return file_test.then(|| facts.path.clone());
            };
            let test = file_test
                || test_ids
                    .get_or_init(|| {
                        (self.winners(facts, true).into_iter())
                            .filter(|&(_, _, test)| test)
                            .map(|(local, _, _)| local)
                            .collect()
                    })
                    .contains(d.local.as_str());
            test.then(|| entity_id(&facts.path, &d.local))
        }
    }

    /// [`Self::test_entity_of`] each test mention at (path, enclosing definition) of `at`,
    /// reading each file once and only when a mention in it sits in a definition.
    fn test_entities_at<'p>(
        &self,
        at: BTreeMap<&'p str, BTreeSet<Option<u32>>>,
    ) -> BTreeMap<(&'p str, Option<u32>), Option<String>> {
        let mut entities = BTreeMap::new();
        for (path, enclosing) in at {
            let facts = (enclosing.iter().any(Option::is_some))
                .then(|| self.view.file(path))
                .flatten();
            let file_test = self.is_test_file(path);
            let test_entity = facts.as_deref().map(|f| self.test_entity_of(f));
            for e in enclosing {
                let entity = match &test_entity {
                    Some(test_entity) => test_entity(e),
                    None => file_test.then(|| path.to_string()),
                };
                entities.insert((path, e), entity);
            }
        }
        entities
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
        let mut proving = Vec::new();
        let mut enclosing: BTreeMap<&str, BTreeSet<Option<u32>>> = BTreeMap::new();
        for leaf in leaves.iter().map(String::as_str) {
            for m in self.proving_calls(id, leaf) {
                let path = m.file.path;
                enclosing.entry(path).or_default().insert(m.enclosing);
                proving.push((path, m.enclosing, m.line));
            }
        }
        let entities = self.test_entities_at(enclosing);
        let calls = (proving.into_iter())
            .map(|(path, e, line)| ProofCall {
                target: id.to_string(),
                caller: entities[&(path, e)].clone(),
                evidence: format!("{path}:{line}"),
            })
            .collect();
        proof_order(calls)
    }

    /// The test calls to `leaf` in code files that the proof rule credits to `id`, read from the
    /// mentions of `leaf` alone. The rule credits a call by its file and its name as written, so
    /// each pair is asked once.
    pub(crate) fn proving_calls<'s>(
        &'s self,
        id: &'s str,
        leaf: &'s str,
    ) -> impl Iterator<Item = MentionRef<'v>> + 's {
        let mut credits: FxHashMap<(&'v str, &'v str), bool> = FxHashMap::default();
        self.test_calls_named(leaf).filter(move |m| {
            *(credits.entry((m.file.path, m.name))).or_insert_with(|| {
                self.proof(m.file.path, m.file.lang, m.name, leaf)
                    .as_deref()
                    == Some(id)
            })
        })
    }

    /// The test calls to `leaf` in code files, read from the mentions of `leaf` alone.
    fn test_calls_named<'s>(&'s self, leaf: &'s str) -> impl Iterator<Item = MentionRef<'v>> + 's {
        self.view.mentions_named(leaf).filter(|m| {
            m.file.half == Half::Code
                && m.kind == MentionKind::Call
                && self.is_test(m.test, m.file.path)
        })
    }

    /// The keys a test call written `name` to `leaf` from `file`, in `lang`, is filed under when
    /// it lands on a placeholder ([`Self::call_edge`]: the call rule sends it to one and the proof
    /// rule credits none): `leaf` for the placeholder `F::%<leaf>`, and for one keyed by a
    /// qualified name the id of each candidate its down-hop keeps in [`Pool::All`]
    /// ([`Self::down_hop`]). None when the call lands elsewhere, or nowhere because the call rule
    /// makes it no link.
    fn placeholder_keys(&self, file: &str, lang: &str, name: &str, leaf: &str) -> Vec<String> {
        let Some((to, _)) = self.call_edge(file, lang, name, leaf, true) else {
            return Vec::new();
        };
        match leaf_of(&to) {
            None => Vec::new(),
            Some(written) if written == leaf => vec![leaf.to_string()],
            Some(_) => (self.down_hop(&to, Pool::All).into_iter())
                .flat_map(Hop::into_vec)
                .map(|c| c.id)
                .collect(),
        }
    }

    /// The test calls in `facts`, a code file, that land on a placeholder, as (key, caller)
    /// pairs, one per key the call is filed under ([`Self::placeholder_keys`]), in mention then
    /// key order: [`Self::placeholder_callers`] of every leaf `facts` calls at once. Where a call
    /// lands depends only on its file and name, its leaf and qualifier, so each name is resolved
    /// once.
    pub fn file_placeholder_calls(&self, facts: &FileFacts) -> Vec<(String, String)> {
        let path = facts.path.as_str();
        let test_entity = self.test_entity_of(facts);
        let mut keys: FxHashMap<&str, Vec<String>> = FxHashMap::default();
        let mut calls = Vec::new();
        for m in self.test_calls(facts) {
            let filed = (keys.entry(&m.name))
                .or_insert_with(|| self.placeholder_keys(path, &facts.lang, &m.name, &m.leaf));
            if filed.is_empty() {
                continue;
            }
            if let Some(caller) = test_entity(m.enclosing) {
                calls.extend(filed.iter().map(|key| (key.clone(), caller.clone())));
            }
        }
        calls
    }

    /// The distinct callers of the test calls to `leaf` that land on a placeholder and are filed
    /// under `leaf` or under `id` ([`Self::placeholder_keys`]), in order: the test entities
    /// ([`Self::test_entity_of`]) a callers walk from the definition `id` named `leaf` reaches
    /// through a placeholder no proof names, read from the mentions of `leaf` alone. Where a call
    /// lands depends only on its file and name, its leaf and qualifier, so each name in each file
    /// is resolved once.
    pub fn placeholder_callers(&self, id: &str, leaf: &str) -> Vec<String> {
        let mut lands: FxHashMap<(&str, &str), bool> = FxHashMap::default();
        let mut enclosing: BTreeMap<&str, BTreeSet<Option<u32>>> = BTreeMap::new();
        for m in self.test_calls_named(leaf) {
            let (path, name) = (m.file.path, m.name);
            if *(lands.entry((path, name))).or_insert_with(|| {
                let keys = self.placeholder_keys(path, m.file.lang, name, leaf);
                keys.iter().any(|key| key == leaf || key == id)
            }) {
                enclosing.entry(path).or_default().insert(m.enclosing);
            }
        }
        let callers: BTreeSet<String> = (self.test_entities_at(enclosing).into_values())
            .flatten()
            .collect();
        callers.into_iter().collect()
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

    /// The down-hop of `id`: what its placeholder's call-target candidates in `pool` resolve to,
    /// those of its leaf, the text after the last `::` or `.` of the name it is keyed by, and for
    /// a name a qualifier writes, the ones that qualifier keeps ([`Self::kept`]); none when `id`
    /// is no placeholder.
    pub fn down_hop(&self, id: &str, pool: Pool) -> Option<Hop<Candidate<'v>>> {
        let (qualifier, leaf) = split_path(leaf_of(id)?);
        let targets = self.call_targets(leaf, pool);
        Some(match qualifier {
            None => hop(targets),
            Some(q) => hop(self
                .kept(q, pool, targets.iter())
                .into_iter()
                .cloned()
                .collect()),
        })
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
                || (self.down_hop(&l.to, Pool::of(l.test)))
                    .is_some_and(|h| h.into_vec().iter().any(|c| c.id == id))
        };
        files
            .into_iter()
            .filter_map(|path| self.view.file(path))
            .flat_map(|facts| self.links(&facts, pool == Pool::All))
            .filter(|l| l.rel != REL_CONTAINS && reaches(l))
            .collect()
    }

    /// The design and workflow nodes and edges of `facts`, of the kinds and relations they may
    /// name, as written: whether one is test code is settled by its ids where it is asserted
    /// ([`Self::is_test_id`], [`Self::is_test_link`]).
    pub fn intent(&self, facts: &FileFacts) -> (Vec<IntentNode>, Vec<IntentLink>) {
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
                Some(IntentLink {
                    from: from.clone(),
                    rel: rel_of(rel)?,
                    to: to.clone(),
                })
            })
            .collect();
        (nodes, links)
    }
}

/// The rows a store settles ([`Settled`]) for `view`, which holds `base`'s facts but for the
/// files at `differing`, from the rows `base` settled: what settling `view` whole computes,
/// re-settling only where a difference reaches. Its test files are `view`'s test-module closure.
/// Its proof calls are `base`'s but for those in a file a difference reaches, which are read from
/// `view`: a file that differs or flipped test-ness; one that makes a test call to a leaf whose
/// call targets ([`Resolver::proof`]'s candidates, product and test) differ; and one that makes a
/// test call whose qualifier ([`qualifier`]) is the leaf of a definition or of an `impl` mention
/// in either version of a file that differs or flipped, or a name an implementation's header
/// holds beside such a definition's leaf, since what a qualifier keeps ([`Resolver::narrowed`])
/// moves with its definitions, its implementations and the traits they name. Only a file that
/// differs or flipped holds a definition or an `impl` mention that moved, so no other proof call
/// can change.
pub fn resettle(
    base: &dyn CodeView,
    view: &dyn CodeView,
    differing: &BTreeSet<&str>,
) -> (Vec<String>, Vec<ProofCall>) {
    let (was, now) = (Resolver::new(base), Resolver::new(view));
    let flipped = (was.test_files.symmetric_difference(&now.test_files)).copied();
    let reached: BTreeSet<&str> = differing.iter().copied().chain(flipped).collect();
    let (mut leaves, mut qualifiers) = (BTreeSet::new(), BTreeSet::new());
    for path in &reached {
        for facts in [base.file(path), view.file(path)].into_iter().flatten() {
            leaves.extend(facts.defs.iter().map(|d| d.leaf.clone()));
            let impls = facts.mentions.iter().filter(|m| is_impl(&m.kind));
            qualifiers.extend(impls.map(|m| m.leaf.clone()));
        }
    }
    // A trait a reached file gains, loses or flips moves what each name an implementation's
    // header holds beside it keeps, wherever that implementation is.
    for leaf in &leaves {
        for m in view.mentions_named(leaf).filter(|m| is_impl(&m.kind)) {
            let header = |n: &&Mention| n.enclosing == m.enclosing && is_impl(&n.kind);
            if let Some(facts) = view.file(m.file.path) {
                qualifiers.extend(facts.mentions.iter().filter(header).map(|n| n.leaf.clone()));
            }
        }
    }
    qualifiers.extend(leaves.iter().cloned());
    let targets = |r: &Resolver, leaf: &str| -> Vec<(String, bool)> {
        let candidates = r.candidates(leaf);
        let calls = candidates.iter().filter(|c| Pool::All.calls(c));
        calls.map(|c| (c.id.clone(), c.test)).collect()
    };
    let mut redo: BTreeSet<String> = reached.iter().map(|p| p.to_string()).collect();
    for leaf in leaves
        .iter()
        .filter(|l| targets(&was, l) != targets(&now, l))
    {
        redo.extend(now.test_calls_named(leaf).map(|m| m.file.path.to_string()));
    }
    let qualified =
        |m: &Mention| qualifier(&m.name, &m.leaf).is_some_and(|q| qualifiers.contains(q));
    for &path in &now.code_files {
        let facts = view.file(path);
        if facts.is_some_and(|facts| now.test_calls(&facts).any(qualified)) {
            redo.insert(path.to_string());
        }
    }
    let redone = |c: &ProofCall| c.file().is_some_and(|file| redo.contains(file));
    let mut calls: Vec<ProofCall> = (was.proof_calls().iter())
        .filter(|c| !redone(c))
        .cloned()
        .collect();
    for path in redo.iter().filter(|p| now.is_code_file(p)) {
        if let Some(facts) = view.file(path) {
            calls.extend(now.file_proofs(&facts));
        }
    }
    let test_files = now.test_files().into_iter().map(str::to_string).collect();
    (test_files, proof_order(calls))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::code_facts::{Def, DocConcept, DocLink, Stamp, Tip, WfNode, WfRelation};
    use crate::code_graph::CodeGraph;
    use crate::code_view::{Compared, FactsView, Tests, ViewFilter};
    use crate::composed::{Composed, Honesty, Indexed};
    use crate::contextgraph::query::path_leaf;
    use crate::contextgraph::{ContextGraph, Direction};

    fn def(local: &str, kind: &str, line: u32, parent: Option<u32>, test: bool) -> Def {
        Def {
            local: local.into(),
            leaf: local.rsplit("::").next().unwrap().into(),
            kind: kind.into(),
            line,
            end_line: line,
            parent,
            test,
            doc: None,
        }
    }

    /// A member of kind `kind` whose local path is dotted, as a Python or C# member's is
    /// (`Base.area`): its leaf the text after its type's `.`, its parent the definition at
    /// `parent`.
    fn dotted(local: &str, kind: &str, line: u32, parent: u32) -> Def {
        Def {
            leaf: path_leaf(local).into(),
            ..def(local, kind, line, Some(parent), false)
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

    /// A mention written `name` (`Q::leaf`, `Q.leaf`), its leaf the text after the last
    /// separator.
    fn qualified(
        kind: MentionKind,
        name: &str,
        enclosing: Option<u32>,
        line: u32,
        test: bool,
    ) -> Mention {
        Mention {
            name: name.into(),
            ..mention(kind, path_leaf(name), enclosing, line, test)
        }
    }

    /// A test's call written `name` (`Q::leaf`) from the definition at `enclosing`.
    fn test_call(name: &str, enclosing: u32, line: u32) -> Mention {
        qualified(MentionKind::Call, name, Some(enclosing), line, true)
    }

    /// A module declaration whose `as_root` and `as_child` lists both hold `candidates`.
    fn decl(name: &str, def: u32, test: bool, candidates: &[&str]) -> ModuleDecl {
        let paths: Vec<String> = candidates.iter().map(|c| c.to_string()).collect();
        ModuleDecl {
            name: name.into(),
            def,
            test,
            as_root: paths.clone(),
            as_child: paths,
            path_attr: false,
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
    /// calls outside every definition, tagged references (one held by a call), out-of-line and
    /// test-only module declarations, a test module and a test helper sharing product leaves, a
    /// second language, and design and workflow facts.
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
            vec![
                call("compute", Some(1), 5, false),
                call("compute", None, 7, false),
            ],
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
                DocLink {
                    from: "docs/d.md".into(),
                    rel: REL_SPECIFIES.into(),
                    to: "src/a.rs::tests::checks".into(),
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
        PlaceholderCalls(&'static str),
        PlaceholderCallers(&'static str, &'static str),
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
                    let test = mark(r.is_test_id(&n.id));
                    format!("node {} [{}] {} {}{test}", n.id, n.kind, n.title, n.doc)
                });
                let links = links.iter().map(|l| {
                    let test = mark(r.is_test_link(&l.from, &l.to));
                    format!("{} -{}/{TIER_EXTRACTED}-> {}{test}", l.from, l.rel, l.to)
                });
                nodes.chain(links).collect()
            }
            Ask::PlaceholderCalls(path) => r
                .file_placeholder_calls(&view.file(path).unwrap())
                .into_iter()
                .map(|(key, caller)| format!("{key} {caller}"))
                .collect(),
            Ask::PlaceholderCallers(id, leaf) => r.placeholder_callers(id, leaf),
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

    /// The files of [`fixture`], `change` applied to them.
    fn changed(change: impl FnOnce(&mut Vec<FileFacts>)) -> FactsView {
        changed_from(&fixture(), change)
    }

    /// The files of `base`, `change` applied to them.
    fn changed_from(base: &FactsView, change: impl FnOnce(&mut Vec<FileFacts>)) -> FactsView {
        let mut files: Vec<FileFacts> = (base.files())
            .map(|f| base.file(f.path).unwrap().into_owned())
            .collect();
        change(&mut files);
        FactsView::new(stamp(), files)
    }

    /// The rows a store settles from `view` alone, settling it whole: its test files and its
    /// proof calls.
    fn settled_rows(view: &FactsView) -> (Vec<String>, Vec<ProofCall>) {
        let whole = Resolver::new(view);
        let test_files = whole.test_files().into_iter().map(str::to_string).collect();
        (test_files, whole.proof_calls().to_vec())
    }

    /// Re-settling `view`, which holds `base`'s facts but for the files at `differing`, from the
    /// rows `base` settled computes the test files and proof calls settling `view` whole
    /// computes, and those rows move from `base`'s exactly when a file differs: `case` fails
    /// otherwise.
    fn assert_resettles(case: &str, base: &FactsView, view: &FactsView, differing: &[&str]) {
        let (was, want) = (settled_rows(base), settled_rows(view));
        let differing: BTreeSet<&str> = differing.iter().copied().collect();
        assert_eq!(differing.is_empty(), want == was, "{case}: the rows move");
        assert_eq!(resettle(base, view, &differing), want, "{case}");
    }

    /// The file at `path` among `files`.
    fn at<'f>(files: &'f mut [FileFacts], path: &str) -> &'f mut FileFacts {
        files.iter_mut().find(|f| f.path == path).unwrap()
    }

    /// Re-settling reaches every row a difference moves: GIVEN [`fixture`] and a view that differs
    /// from it in some files, WHEN the view is re-settled from the fixture's rows, THEN its test
    /// files and proof calls are what settling the view whole computes, including the rows of a
    /// file that does not differ.
    #[test]
    fn resettling_a_view_answers_what_settling_it_whole_answers() {
        // (case, the files that differ, the change)
        let cases: [(&str, &[&str], FactsView); 6] = [
            ("no file differs", &[], changed(|_| {})),
            (
                "a second product candidate unproves an untouched test's call",
                &["src/c.rs"],
                changed(|files| {
                    let c = at(files, "src/c.rs");
                    c.defs.push(def("run", "function", 6, None, false));
                }),
            ),
            (
                "a deleted file's definitions leave every call to them",
                &["src/b.rs"],
                changed(|files| files.retain(|f| f.path != "src/b.rs")),
            ),
            (
                "a module declaration flips a file it reaches to test",
                &["src/a.rs"],
                changed(|files| at(files, "src/a.rs").modules[0].test = true),
            ),
            (
                "a module declaration flips the files it reaches to product, unproving their calls",
                &["src/a.rs"],
                changed(|files| at(files, "src/a.rs").modules[1].test = false),
            ),
            (
                "an added test file proves what it calls",
                &["tests/new.rs"],
                changed(|files| {
                    files.push(code(
                        "tests/new.rs",
                        "rust",
                        Scope::Test,
                        vec![def("new_works", "function", 1, None, true)],
                        vec![call("helper", Some(0), 2, true)],
                    ));
                }),
            ),
        ];
        let base = fixture();
        for (case, differing, view) in cases {
            assert_resettles(case, &base, &view, differing);
        }
    }

    /// A tree of qualified test calls, one for each outcome of the narrowed match in the proof
    /// rule ([`Resolver::proof`]): its qualifier keeps one product method (`Cursor::new`,
    /// `Store::new`), one test method (`Bell::ring`), or several (`Lamp::glow`, called from a
    /// test module beside one of them); it keeps none but names a type (`Gauge::read`), so the
    /// unqualified rule takes the call; or it keeps none and names nothing (`AtomicU64::new`,
    /// called from a test module beside a `new`, `Reader::new`, `Valve::shut`), so the call
    /// proves none.
    fn qualified_proof_fixture() -> FactsView {
        let fn_def = |name: &str| vec![def(name, "function", 1, None, true)];
        FactsView::new(
            stamp(),
            vec![
                code(
                    "src/cursor.rs",
                    "rust",
                    Scope::Product,
                    vec![
                        def("impl Cursor", "impl", 1, None, false),
                        def("Cursor::new", "method", 2, Some(0), false),
                    ],
                    vec![tagged("impl", "Cursor", Some(0), 1)],
                ),
                code(
                    "src/store.rs",
                    "rust",
                    Scope::Product,
                    vec![
                        def("impl Store", "impl", 1, None, false),
                        def("Store::new", "method", 2, Some(0), false),
                        def("tests", "module", 10, None, true),
                        def("tests::counts", "function", 11, Some(2), true),
                    ],
                    vec![
                        tagged("impl", "Store", Some(0), 1),
                        test_call("AtomicU64::new", 3, 12),
                    ],
                ),
                code(
                    "src/gauge.rs",
                    "rust",
                    Scope::Product,
                    vec![def("Gauge", "type", 1, None, false)],
                    vec![],
                ),
                code(
                    "src/lamp.rs",
                    "rust",
                    Scope::Product,
                    vec![
                        def("impl Lamp", "impl", 1, None, false),
                        def("Lamp::glow", "method", 2, Some(0), false),
                        def("tests", "module", 10, None, true),
                        def("tests::lit", "function", 11, Some(2), true),
                    ],
                    vec![
                        tagged("impl", "Lamp", Some(0), 1),
                        test_call("Lamp::glow", 3, 12),
                    ],
                ),
                code(
                    "src/lamp_ext.rs",
                    "rust",
                    Scope::Product,
                    vec![
                        def("impl Lamp", "impl", 1, None, false),
                        def("Lamp::glow", "method", 2, Some(0), false),
                    ],
                    vec![tagged("impl", "Lamp", Some(0), 1)],
                ),
                code(
                    "src/meter.rs",
                    "rust",
                    Scope::Product,
                    vec![
                        def("impl Meter", "impl", 1, None, false),
                        def("Meter::read", "method", 2, Some(0), false),
                    ],
                    vec![tagged("impl", "Meter", Some(0), 1)],
                ),
                code(
                    "src/seal.rs",
                    "rust",
                    Scope::Product,
                    vec![
                        def("Seal", "type", 1, None, false),
                        def("impl Seal", "impl", 2, None, false),
                        def("Seal::shut", "method", 3, Some(1), false),
                    ],
                    vec![tagged("impl", "Seal", Some(1), 2)],
                ),
                code(
                    "tests/bell.rs",
                    "rust",
                    Scope::Test,
                    vec![
                        def("impl Bell", "impl", 1, None, true),
                        def("Bell::ring", "method", 2, Some(0), true),
                        def("rings", "function", 5, None, true),
                    ],
                    vec![
                        tagged("impl", "Bell", Some(0), 1),
                        test_call("Bell::ring", 2, 6),
                    ],
                ),
                code(
                    "tests/gauge.rs",
                    "rust",
                    Scope::Test,
                    vec![
                        def("impl Probe for Gauge", "impl", 1, None, true),
                        def("probes", "function", 5, None, true),
                    ],
                    vec![
                        tagged("impl", "Probe", Some(0), 1),
                        tagged("impl", "Gauge", Some(0), 1),
                        test_call("Gauge::read", 1, 6),
                    ],
                ),
                code(
                    "tests/probe.rs",
                    "rust",
                    Scope::Test,
                    vec![def("Probe", "trait", 1, None, true)],
                    vec![],
                ),
                code(
                    "tests/read.rs",
                    "rust",
                    Scope::Test,
                    fn_def("reads"),
                    vec![test_call("Reader::new", 0, 2)],
                ),
                code(
                    "tests/seq.rs",
                    "rust",
                    Scope::Test,
                    fn_def("steps"),
                    vec![
                        test_call("Cursor::new", 0, 2),
                        test_call("Store::new", 0, 3),
                    ],
                ),
                code(
                    "tests/valve.rs",
                    "rust",
                    Scope::Test,
                    vec![
                        def("impl Seal for Valve", "impl", 1, None, true),
                        def("shuts", "function", 5, None, true),
                    ],
                    vec![
                        tagged("impl", "Seal", Some(0), 1),
                        tagged("impl", "Valve", Some(0), 1),
                        test_call("Valve::shut", 1, 6),
                    ],
                ),
            ],
        )
    }

    /// A QUALIFIED PROOF RESETTLES (spec 117, RESOLVER AT READ): a test call's proof narrows by
    /// its qualifier as its call edge does, so a second `impl Cursor` holding `new` moves the
    /// proof of a test's `Cursor::new()` from the first `new` to none, and a test beside the
    /// second `new` proves neither; and re-settling reaches every proof a qualifier moves, after
    /// a file that only adds the type `AtomicU64` a test's `AtomicU64::new()` names, one that only
    /// renames an `impl` mention over unchanged method ids, one that adds a test trait's method a
    /// qualifier keeps through an implementation in another file, one that turns test the call
    /// target a qualified call proves, and one that turns a type into a trait over unchanged
    /// method ids, a qualifier reaching it only through an implementation in a file it does not
    /// touch. The proofs of each definition say what the proof calls say.
    #[test]
    fn a_qualified_proof_resettles() {
        let base = qualified_proof_fixture();
        // (case, the files that differ, the change, the proofs settled after it)
        let cases: [(&str, &[&str], FactsView, &[&str]); 7] = [
            (
                "no file differs: a qualified call proves the one product call target its \
                 qualifier keeps, and none when that one is test, it keeps several, or it keeps \
                 none and its path qualifier names nothing",
                &[],
                changed_from(&base, |_| {}),
                &[
                    "src/cursor.rs::Cursor::new <- tests/seq.rs:2",
                    "src/meter.rs::Meter::read <- tests/gauge.rs:6",
                    "src/store.rs::Store::new <- tests/seq.rs:3",
                ],
            ),
            (
                "a second `impl Cursor` holding `new` moves the proof of `Cursor::new()` to none, \
                 and a test beside the second `new` proves neither",
                &["src/cursor_ext.rs"],
                changed_from(&base, |files| {
                    files.push(code(
                        "src/cursor_ext.rs",
                        "rust",
                        Scope::Product,
                        vec![
                            def("impl Cursor", "impl", 1, None, false),
                            def("Cursor::new", "method", 2, Some(0), false),
                            def("tests", "module", 10, None, true),
                            def("tests::opens", "function", 11, Some(2), true),
                        ],
                        vec![
                            tagged("impl", "Cursor", Some(0), 1),
                            test_call("Cursor::new", 3, 12),
                        ],
                    ));
                }),
                &[
                    "src/meter.rs::Meter::read <- tests/gauge.rs:6",
                    "src/store.rs::Store::new <- tests/seq.rs:3",
                ],
            ),
            (
                "a file that only adds `struct AtomicU64` lets `AtomicU64::new()` take the \
                 unqualified rule in a file it does not touch",
                &["src/atomic.rs"],
                changed_from(&base, |files| {
                    let atomic = vec![def("AtomicU64", "type", 1, None, false)];
                    files.push(code(
                        "src/atomic.rs",
                        "rust",
                        Scope::Product,
                        atomic,
                        vec![],
                    ));
                }),
                &[
                    "src/cursor.rs::Cursor::new <- tests/seq.rs:2",
                    "src/meter.rs::Meter::read <- tests/gauge.rs:6",
                    "src/store.rs::Store::new <- src/store.rs:12 tests/seq.rs:3",
                ],
            ),
            (
                "an `impl` mention renamed over unchanged method ids moves the proof between the \
                 files calling its old and its new name",
                &["src/cursor.rs"],
                changed_from(&base, |files| {
                    at(files, "src/cursor.rs").mentions[0] = tagged("impl", "Reader", Some(0), 1);
                }),
                &[
                    "src/cursor.rs::Cursor::new <- tests/read.rs:2",
                    "src/meter.rs::Meter::read <- tests/gauge.rs:6",
                    "src/store.rs::Store::new <- tests/seq.rs:3",
                ],
            ),
            (
                "a test trait's method, kept through an implementation in another file, moves \
                 the proof of `Gauge::read()` to none",
                &["tests/probe.rs"],
                changed_from(&base, |files| {
                    let read = def("Probe::read", "method", 2, Some(0), true);
                    at(files, "tests/probe.rs").defs.push(read);
                }),
                &[
                    "src/cursor.rs::Cursor::new <- tests/seq.rs:2",
                    "src/store.rs::Store::new <- tests/seq.rs:3",
                ],
            ),
            (
                "a call target turned test moves the proof of `Gauge::read()` to none",
                &["src/meter.rs"],
                changed_from(&base, |files| at(files, "src/meter.rs").defs[1].test = true),
                &[
                    "src/cursor.rs::Cursor::new <- tests/seq.rs:2",
                    "src/store.rs::Store::new <- tests/seq.rs:3",
                ],
            ),
            (
                "a type turned trait over unchanged method ids gives `Valve::shut()` its method \
                 through an implementation, in a file it does not touch, that names both",
                &["src/seal.rs"],
                changed_from(&base, |files| {
                    let seal = at(files, "src/seal.rs");
                    seal.defs = vec![
                        def("Seal", "trait", 1, None, false),
                        def("Seal::shut", "method", 2, Some(0), false),
                    ];
                    seal.mentions.clear();
                }),
                &[
                    "src/cursor.rs::Cursor::new <- tests/seq.rs:2",
                    "src/meter.rs::Meter::read <- tests/gauge.rs:6",
                    "src/seal.rs::Seal::shut <- tests/valve.rs:6",
                    "src/store.rs::Store::new <- tests/seq.rs:3",
                ],
            ),
        ];
        let ids = [
            "src/cursor.rs::Cursor::new",
            "src/cursor_ext.rs::Cursor::new",
            "src/meter.rs::Meter::read",
            "src/seal.rs::Seal::shut",
            "src/store.rs::Store::new",
        ];
        for (case, differing, view, proofs) in cases {
            let r = Resolver::new(&view);
            assert_eq!(answer(&view, &r, &Ask::Proofs), proofs, "{case}");
            for id in ids {
                let calls = r.proof_calls().iter().filter(|c| c.target == id);
                let want: Vec<ProofCall> = calls.cloned().collect();
                assert_eq!(r.proofs_of(id), want, "{case}: the proofs of {id}");
            }
            assert_resettles(case, &base, &view, differing);
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
                "call: no candidate is an ambiguous placeholder, outside every definition the \
                 file's call",
                Ask::Links("src/a.rs", false),
                &[
                    "src/a.rs -REFERENCES/ambiguous-> src/a.rs::%vanish",
                    "src/a.rs -CALLS/ambiguous-> src/a.rs::%vanish",
                ],
                &[],
            ),
            (
                "call: candidates of any language, a definition's call or its file's",
                Ask::Links("scripts/t.py", false),
                &[
                    "scripts/t.py::assist -CALLS/inferred-> scripts/t.py::%compute",
                    "scripts/t.py -CALLS/inferred-> scripts/t.py::%compute",
                ],
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
                "held: a tagged reference the call rule already references writes no edge, and \
                 no call from its file",
                Ask::Links("src/a.rs", false),
                &["src/a.rs -REFERENCES/inferred-> src/a.rs::%compute"],
                &[
                    "src/b.rs::compute",
                    "src/a.rs -CALLS/inferred-> src/a.rs::%compute",
                ],
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
                ],
                &["draw", "tests/it.rs::"],
            ),
            (
                "proof: none for a call the call rule lands on its own file's test definition",
                Ask::Proofs,
                &[],
                &["tests/it.rs:3"],
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

    /// A CHANGE TO WHAT A STORE SETTLES SHIPS WITH A BUMP: the test files and proof calls of
    /// [`fixture`], of [`qualified_proof_fixture`], whose qualified test calls take each outcome
    /// of the narrowed match in [`Resolver::proof`], and of [`module_root_fixture`], whose
    /// test-only declaration reaches a file through the list its declaring file's place picks,
    /// are pinned by digest beside the generation the store's fingerprint folds in, so a change
    /// that moves the rows these three fixtures settle fails here until `SETTLED_GENERATION` is
    /// bumped and the pair re-pinned.
    #[test]
    fn a_change_to_the_settled_rows_ships_with_a_generation_bump() {
        const PINNED: (u64, &str) = (5, "a9202848617a9138");
        let (test_files, proofs) = settled_rows(&fixture());
        assert!(
            !proofs.is_empty() && test_files.iter().any(|p| !p.starts_with("tests/")),
            "the fixture holds proofs and a test file by the closure alone"
        );
        let qualified = settled_rows(&qualified_proof_fixture());
        let rooted = settled_rows(&module_root_fixture());
        let rows = format!("{test_files:#?}{proofs:#?}{qualified:#?}{rooted:#?}");
        let digest = crate::community::fnv1a_hex(&rows);
        assert_eq!(
            (SETTLED_GENERATION, digest.as_str()),
            PINNED,
            "this build's settled generation and the digest of the fixtures' settled rows (left) \
             differ from the pinned pair: a change to what a store settles ships with a bump, so \
             bump `SETTLED_GENERATION` and re-pin the pair to this build's values together"
        );
    }

    /// A PROOF NAMES ITS TEST ENTITY: a test call's caller is its enclosing definition when that
    /// id is test code (an inline test, or twins that are both test), or its test file at file
    /// level; test code inside a product entity (the `#[cfg(test)]` twin of the product `helper`)
    /// still proves what it calls but names no caller, and no test entity reaches a placeholder
    /// through it. Every reader of proofs and of placeholder callers says alike.
    #[test]
    fn a_proof_names_its_test_entity_and_none_for_test_code_inside_a_product_entity() {
        let p = code(
            "src/p.rs",
            "rust",
            Scope::Product,
            vec![
                def("target", "function", 1, None, false),
                def("helper", "function", 3, None, true),
                def("helper", "function", 5, None, false),
                def("tests", "module", 7, None, true),
                def("tests::check", "function", 8, Some(3), true),
                def("both", "function", 11, None, true),
                def("both", "function", 13, None, true),
            ],
            vec![
                call("target", Some(1), 3, true),
                call("draw", Some(1), 4, true),
                call("target", Some(4), 8, true),
                call("draw", Some(4), 9, true),
                call("target", Some(5), 11, true),
            ],
        );
        let t = code(
            "tests/t.rs",
            "rust",
            Scope::Test,
            vec![],
            vec![call("target", None, 1, true)],
        );
        let draws = ["src/d1.rs", "src/d2.rs"].map(|path| {
            let draw = vec![def("draw", "function", 1, None, false)];
            code(path, "rust", Scope::Product, draw, vec![])
        });
        let view = FactsView::new(stamp(), [vec![p, t], draws.to_vec()].concat());
        let resolver = Resolver::new(&view);
        let calls = |calls: Vec<ProofCall>| -> Vec<String> {
            (calls.into_iter())
                .map(|c| format!("{} {:?}", c.evidence, c.caller))
                .collect()
        };
        let facts = view.file("src/p.rs").unwrap();
        // (the reader, what it answers)
        let cases: [(&str, Vec<String>); 4] = [
            ("proof calls", calls(resolver.proof_calls().to_vec())),
            ("proofs of", calls(resolver.proofs_of("src/p.rs::target"))),
            (
                "placeholder callers",
                resolver.placeholder_callers("src/d1.rs::draw", "draw"),
            ),
            (
                "file placeholder calls",
                (resolver.file_placeholder_calls(&facts).into_iter())
                    .map(|(key, caller)| format!("{key} {caller}"))
                    .collect(),
            ),
        ];
        let proofs = [
            "src/p.rs:11 Some(\"src/p.rs::both\")",
            "src/p.rs:3 None",
            "src/p.rs:8 Some(\"src/p.rs::tests::check\")",
            "tests/t.rs:1 Some(\"tests/t.rs\")",
        ];
        let wants: [&[&str]; 4] = [
            &proofs,
            &proofs,
            &["src/p.rs::tests::check"],
            &["draw src/p.rs::tests::check"],
        ];
        for ((reader, got), want) in cases.into_iter().zip(wants) {
            assert_eq!(got, want, "{reader}");
        }
    }

    /// THE NEW TAGS RESOLVE AS REFERENCES (spec 117, NEW TAGS and RESOLVER AT READ): an `impl`
    /// mention in an implementation header resolves onto the one trait or type of its leaf, and a
    /// `variant` mention onto the one variant, each a REFERENCES link and never CALLS. A
    /// same-leaf definition of a kind outside the tag's candidates - the implementation itself,
    /// a function, a constant - is no candidate, so it neither takes the edge nor contests it.
    #[test]
    fn the_new_tags_resolve_as_references() {
        let view = code(
            "src/view.rs",
            "rust",
            Scope::Product,
            vec![
                def("CodeView", "trait", 1, None, false),
                def("Located", "type", 5, None, false),
                def("Located::Many", "variant", 6, Some(1), false),
            ],
            vec![],
        );
        let facts = code(
            "src/facts.rs",
            "rust",
            Scope::Product,
            vec![
                def("Located", "impl", 1, None, false),
                def("locate", "function", 5, None, false),
            ],
            vec![
                tagged("impl", "CodeView", Some(0), 1),
                tagged("impl", "Located", Some(0), 1),
                tagged("variant", "Many", Some(1), 6),
            ],
        );
        let decoys = code(
            "src/decoys.rs",
            "rust",
            Scope::Product,
            vec![
                def("CodeView", "function", 1, None, false),
                def("Many", "constant", 3, None, false),
            ],
            vec![],
        );
        let view = FactsView::new(stamp(), vec![view, facts, decoys]);
        let resolver = Resolver::new(&view);
        let links: Vec<String> = (resolver.links(&view.file("src/facts.rs").unwrap(), false))
            .iter()
            .map(render_link)
            .collect();
        assert_eq!(
            links,
            [
                "src/facts.rs::Located -REFERENCES/inferred-> src/view.rs::CodeView",
                "src/facts.rs::Located -REFERENCES/inferred-> src/view.rs::Located",
                "src/facts.rs::locate -REFERENCES/inferred-> src/view.rs::Located::Many",
                "src/facts.rs -CONTAINS/extracted-> src/facts.rs::Located",
                "src/facts.rs -CONTAINS/extracted-> src/facts.rs::locate",
            ],
            "each tagged mention one REFERENCES link onto its trait, type or variant, no CALLS, \
             then the file's CONTAINS"
        );
    }

    /// READERS (spec 117): `Resolver::referrers` lists each mention under every id its own rule
    /// lists it under, in path then source order, one item per mention, a repeated call
    /// included: a settled call or reference under its target, the one its qualifier keeps or
    /// its one field included, a call through a placeholder under each candidate the
    /// placeholder keeps, a reference no one candidate takes under each it took none from, the
    /// several its qualifier keeps or its several fields included, and a mention whose qualifier
    /// makes no link, or an unqualified field reference naming only constants, under none. The
    /// item names the enclosing definition's id and the kind that id settles to (a type's, for a
    /// mention its impl encloses), or its file's when none encloses it, the label of its tag
    /// (none for a tag no label row holds), the mention's line, and whether the mention is test
    /// code, by its own mark or its file's scope, a test twin's sharing its product's id. A
    /// mention in a file outside the code half lists under none, and an id naming no definition
    /// of a code file has none. `Composed` answers the product items alone under every tests
    /// setting, near or whole, and none when the worktree cannot be compared.
    #[test]
    fn a_referrer_is_listed_under_each_id_its_mention_lists() {
        let lib = code(
            "src/lib.rs",
            "rust",
            Scope::Product,
            vec![
                def("LIMIT", "constant", 3, None, false),
                def("Bin", "type", 5, None, false),
                def("spill", "function", 9, None, false),
                def("fits", "function", 16, None, false),
                def("helper", "function", 20, None, false),
                def("helper", "function", 30, None, true),
                def("Bin::CAP", "constant", 6, Some(1), false),
                def("Bin::len", "field", 7, Some(1), false),
                def("Bin", "impl", 8, None, false),
            ],
            vec![
                tagged("const", "LIMIT", Some(8), 8),
                tagged("type", "Bin", Some(2), 9),
                tagged("field", "MAX", Some(2), 10),
                tagged("const", "LIMIT", Some(2), 11),
                tagged("bogus", "LIMIT", Some(2), 12),
                call("shared", Some(2), 13, false),
                tagged("field", "len", Some(2), 14),
                tagged("field", "w", Some(2), 15),
                tagged("type", "Shape", Some(3), 16),
                call("helper", Some(3), 17, false),
                call("helper", Some(3), 18, false),
                call("helper", Some(3), 19, true),
                qualified(MentionKind::Call, "Nowhere::helper", Some(3), 20, false),
                qualified(
                    MentionKind::Tagged("const".into()),
                    "Nowhere::LIMIT",
                    Some(3),
                    21,
                    false,
                ),
                qualified(
                    MentionKind::Tagged("const".into()),
                    "Bin::CAP",
                    Some(3),
                    22,
                    false,
                ),
                qualified(
                    MentionKind::Tagged("const".into()),
                    "Shape::MAX",
                    Some(3),
                    23,
                    false,
                ),
                tagged("const", "LIMIT", None, 25),
                mention(
                    MentionKind::Tagged("const".into()),
                    "LIMIT",
                    Some(5),
                    31,
                    true,
                ),
            ],
        );
        let other = |path: &str| {
            code(
                path,
                "rust",
                Scope::Product,
                vec![
                    def("shared", "function", 1, None, false),
                    def("Shape", "type", 3, None, false),
                    def("Shape::MAX", "constant", 4, Some(1), false),
                    def("Shape::w", "field", 5, Some(1), false),
                ],
                vec![],
            )
        };
        let checks = code(
            "tests/t.rs",
            "rust",
            Scope::Test,
            vec![def("checks", "function", 1, None, true)],
            vec![call("helper", Some(0), 2, true)],
        );
        let notes = FileFacts {
            half: Half::Design,
            ..code(
                "docs/d.md",
                "markdown",
                Scope::Product,
                vec![def("LIMIT", "constant", 1, None, false)],
                vec![call("helper", None, 2, false)],
            )
        };
        let view = FactsView::new(
            stamp(),
            vec![other("src/a.rs"), other("src/b.rs"), lib, checks, notes],
        );
        let shown = |referrers: Vec<Referrer>| -> Vec<String> {
            (referrers.iter())
                .map(|r| {
                    let mark = if r.test { " test" } else { "" };
                    format!("{} {} ({}) :{}{mark}", r.label, r.site, r.kind, r.line)
                })
                .collect()
        };
        // (the id, its referrers, the product ones `Composed` answers)
        let rows: [(&str, &[&str], &[&str]); 18] = [
            (
                "src/lib.rs::LIMIT",
                &[
                    "reads src/lib.rs::Bin (type) :8",
                    "reads src/lib.rs::spill (function) :11",
                    "reads src/lib.rs (file) :25",
                    "reads src/lib.rs::helper (function) :31 test",
                ],
                &[
                    "reads src/lib.rs::Bin (type) :8",
                    "reads src/lib.rs::spill (function) :11",
                    "reads src/lib.rs (file) :25",
                ],
            ),
            (
                "src/lib.rs::Bin",
                &["type use src/lib.rs::spill (function) :9"],
                &["type use src/lib.rs::spill (function) :9"],
            ),
            (
                "src/lib.rs::helper",
                &[
                    "called by src/lib.rs::fits (function) :17",
                    "called by src/lib.rs::fits (function) :18",
                    "called by src/lib.rs::fits (function) :19 test",
                    "called by tests/t.rs::checks (function) :2 test",
                ],
                &[
                    "called by src/lib.rs::fits (function) :17",
                    "called by src/lib.rs::fits (function) :18",
                ],
            ),
            (
                "src/a.rs::shared",
                &["called by src/lib.rs::spill (function) :13"],
                &["called by src/lib.rs::spill (function) :13"],
            ),
            (
                "src/b.rs::shared",
                &["called by src/lib.rs::spill (function) :13"],
                &["called by src/lib.rs::spill (function) :13"],
            ),
            (
                "src/a.rs::Shape",
                &["type use src/lib.rs::fits (function) :16"],
                &["type use src/lib.rs::fits (function) :16"],
            ),
            (
                "src/b.rs::Shape",
                &["type use src/lib.rs::fits (function) :16"],
                &["type use src/lib.rs::fits (function) :16"],
            ),
            (
                "src/lib.rs::Bin::CAP",
                &["reads src/lib.rs::fits (function) :22"],
                &["reads src/lib.rs::fits (function) :22"],
            ),
            (
                "src/a.rs::Shape::MAX",
                &["reads src/lib.rs::fits (function) :23"],
                &["reads src/lib.rs::fits (function) :23"],
            ),
            (
                "src/b.rs::Shape::MAX",
                &["reads src/lib.rs::fits (function) :23"],
                &["reads src/lib.rs::fits (function) :23"],
            ),
            (
                "src/lib.rs::Bin::len",
                &["field use src/lib.rs::spill (function) :14"],
                &["field use src/lib.rs::spill (function) :14"],
            ),
            (
                "src/a.rs::Shape::w",
                &["field use src/lib.rs::spill (function) :15"],
                &["field use src/lib.rs::spill (function) :15"],
            ),
            (
                "src/b.rs::Shape::w",
                &["field use src/lib.rs::spill (function) :15"],
                &["field use src/lib.rs::spill (function) :15"],
            ),
            ("src/lib.rs::spill", &[], &[]),
            ("src/lib.rs::Nope", &[], &[]),
            ("src/lib.rs", &[], &[]),
            ("src/gone.rs::LIMIT", &[], &[]),
            ("docs/d.md::LIMIT", &[], &[]),
        ];
        let resolver = Resolver::new(&view);
        let records = Indexed::default();
        let honesty = |compared| Honesty {
            worktree: "wt".into(),
            compared,
            behind: None,
        };
        for (id, every, product) in rows {
            assert_eq!(shown(resolver.referrers(id)), every, "{id}");
            for compared in [Compared::Overlaid(0), Compared::Uncomparable] {
                let want: &[&str] = if compared == Compared::Uncomparable {
                    &[]
                } else {
                    product
                };
                for tests in [Tests::Exclude, Tests::Include, Tests::Only] {
                    let filter = ViewFilter { tests };
                    let at = format!("{id} {compared:?} {tests:?}");
                    let whole = Composed::new(&records, &view, filter, honesty(compared)).unwrap();
                    assert_eq!(shown(whole.referrers(id)), want, "whole: {at}");
                    let near =
                        Composed::near(&records, &view, filter, honesty(compared), None).unwrap();
                    assert_eq!(shown(near.referrers(id)), want, "near: {at}");
                }
            }
        }
    }

    /// A tree of qualified mentions narrowing their candidates: `FactsView`'s inherent `new` and
    /// the `files` its `CodeView` implementation defines, beside `Snapshot`'s and `Composed`'s
    /// `files`; `src/composed.rs`'s `Composed::open`, calling `FactsView::new` (one candidate, in
    /// another file), `FactsView::stamp` (the trait's), `CodeView::files` (several) and
    /// `Composed::decode` (none under `Composed`), and referencing `Located::Many`,
    /// `Located::One` and `Composed::MAX`; a Python class whose base is the qualifier
    /// (`Base.area` from `main`); and the test function `tests/open.rs::opens`, calling
    /// `FactsView::files` and `CodeView::files`.
    pub(crate) fn narrowing_fixture() -> FactsView {
        let qualified_call = |name: &str, enclosing, line, test| {
            qualified(MentionKind::Call, name, enclosing, line, test)
        };
        let qualified_tag = |tag: &str, name: &str, line| {
            qualified(MentionKind::Tagged(tag.into()), name, Some(3), line, false)
        };
        let code_view = code(
            "src/code_view.rs",
            "rust",
            Scope::Product,
            vec![
                def("FactsView", "type", 1, None, false),
                def("impl FactsView", "impl", 3, None, false),
                def("FactsView::new", "method", 4, Some(1), false),
                def("CodeView", "trait", 10, None, false),
                def("CodeView::stamp", "method", 11, Some(3), false),
                def("impl CodeView for FactsView", "impl", 20, None, false),
                def("FactsView::files", "method", 21, Some(5), false),
                def("Located", "type", 30, None, false),
                def("Located::Many", "variant", 31, Some(7), false),
                def("Located::One", "variant", 32, Some(7), false),
            ],
            vec![
                tagged("impl", "FactsView", Some(1), 3),
                tagged("impl", "CodeView", Some(5), 20),
                tagged("impl", "FactsView", Some(5), 20),
            ],
        );
        let snapshot = code(
            "src/snapshot.rs",
            "rust",
            Scope::Product,
            vec![
                def("Snapshot", "type", 1, None, false),
                def("impl CodeView for Snapshot", "impl", 3, None, false),
                def("Snapshot::files", "method", 4, Some(1), false),
                def("decode", "function", 10, None, false),
                def("MAX", "constant", 12, None, false),
            ],
            vec![
                tagged("impl", "CodeView", Some(1), 3),
                tagged("impl", "Snapshot", Some(1), 3),
            ],
        );
        let composed = code(
            "src/composed.rs",
            "rust",
            Scope::Product,
            vec![
                def("Composed", "type", 1, None, false),
                def("impl Composed", "impl", 3, None, false),
                def("Composed::new", "method", 4, Some(1), false),
                def("Composed::open", "method", 6, Some(1), false),
                def("impl CodeView for Composed", "impl", 20, None, false),
                def("Composed::files", "method", 21, Some(4), false),
                def("Located", "type", 30, None, false),
                def("Located::One", "variant", 31, Some(6), false),
            ],
            vec![
                tagged("impl", "Composed", Some(1), 3),
                qualified_call("FactsView::new", Some(3), 7, false),
                qualified_call("FactsView::stamp", Some(3), 8, false),
                qualified_call("CodeView::files", Some(3), 9, false),
                qualified_call("Composed::decode", Some(3), 10, false),
                qualified_tag("variant", "Located::Many", 11),
                qualified_tag("variant", "Located::One", 12),
                qualified_tag("const", "Composed::MAX", 13),
                tagged("impl", "CodeView", Some(4), 20),
                tagged("impl", "Composed", Some(4), 20),
            ],
        );
        let shapes = code(
            "src/shapes.py",
            "python",
            Scope::Product,
            vec![
                def("Base", "type", 1, None, false),
                dotted("Base.area", "method", 2, 0),
                def("Child", "type", 5, None, false),
                dotted("Child.area", "method", 6, 2),
                def("main", "function", 10, None, false),
            ],
            vec![
                tagged("impl", "Base", Some(2), 5),
                qualified_call("Base.area", Some(4), 11, false),
            ],
        );
        let open = code(
            "tests/open.rs",
            "rust",
            Scope::Test,
            vec![def("opens", "function", 1, None, true)],
            vec![
                qualified_call("FactsView::files", Some(0), 2, true),
                qualified_call("CodeView::files", Some(0), 3, true),
            ],
        );
        FactsView::new(stamp(), vec![code_view, snapshot, composed, shapes, open])
    }

    /// A QUALIFIED CALL NARROWS ITS CANDIDATES (spec 117, RESOLVER AT READ): a call or a tagged
    /// reference written `Q::leaf` (or `Q.leaf`) keeps the candidates its rule reads whose parent
    /// is a definition named `Q` or an implementation enclosing an `impl` mention of `Q`, else,
    /// when none is, those whose parent is a trait such an implementation names (`FactsView`'s
    /// `stamp`). One kept candidate takes the edge EXTRACTED in any file, over a
    /// same-file definition of the leaf; several make a call's placeholder and no tagged edge;
    /// none, under a `Q` that names a type, leave the unqualified rule. A class whose base is `Q`
    /// is no implementation of `Q`.
    /// A call's placeholder for several kept candidates is keyed by its qualified name, and the
    /// placeholder readers file a test call under each candidate it keeps, so two calls of one
    /// leaf that land apart are read apart.
    #[test]
    fn a_qualified_call_narrows_its_candidates() {
        let view = narrowing_fixture();
        let r = Resolver::new(&view);
        // (what the answer shows, the question, the whole answer)
        let cases: [(&str, Ask, &[&str]); 6] = [
            (
                "a product file's qualified calls and tagged references",
                Ask::Links("src/composed.rs", false),
                &[
                    "src/composed.rs::Composed::open -CALLS/extracted-> \
                     src/code_view.rs::FactsView::new",
                    "src/composed.rs -REFERENCES/extracted-> src/code_view.rs::FactsView::new",
                    "src/composed.rs::Composed::open -CALLS/extracted-> \
                     src/code_view.rs::CodeView::stamp",
                    "src/composed.rs -REFERENCES/extracted-> src/code_view.rs::CodeView::stamp",
                    "src/composed.rs::Composed::open -CALLS/inferred-> \
                     src/composed.rs::%CodeView::files",
                    "src/composed.rs -REFERENCES/inferred-> src/composed.rs::%CodeView::files",
                    "src/composed.rs::Composed::open -CALLS/inferred-> src/composed.rs::%decode",
                    "src/composed.rs -REFERENCES/inferred-> src/composed.rs::%decode",
                    "src/composed.rs::impl Composed -REFERENCES/extracted-> \
                     src/composed.rs::Composed",
                    "src/composed.rs::Composed::open -REFERENCES/extracted-> \
                     src/code_view.rs::Located::Many",
                    "src/composed.rs::Composed::open -REFERENCES/inferred-> src/snapshot.rs::MAX",
                    "src/composed.rs::impl CodeView for Composed -REFERENCES/inferred-> \
                     src/code_view.rs::CodeView",
                    "src/composed.rs::impl CodeView for Composed -REFERENCES/extracted-> \
                     src/composed.rs::Composed",
                    "src/composed.rs -CONTAINS/extracted-> src/composed.rs::Composed",
                    "src/composed.rs::impl CodeView for Composed -CONTAINS/extracted-> \
                     src/composed.rs::Composed::files",
                    "src/composed.rs::impl Composed -CONTAINS/extracted-> \
                     src/composed.rs::Composed::new",
                    "src/composed.rs::impl Composed -CONTAINS/extracted-> \
                     src/composed.rs::Composed::open",
                    "src/composed.rs -CONTAINS/extracted-> src/composed.rs::Located",
                    "src/composed.rs::Located -CONTAINS/extracted-> src/composed.rs::Located::One",
                    "src/composed.rs -CONTAINS/extracted-> \
                     src/composed.rs::impl CodeView for Composed",
                    "src/composed.rs -CONTAINS/extracted-> src/composed.rs::impl Composed",
                ],
            ),
            (
                "a dotted qualifier, a base class no implementation",
                Ask::Links("src/shapes.py", false),
                &[
                    "src/shapes.py::main -CALLS/extracted-> src/shapes.py::Base.area",
                    "src/shapes.py -REFERENCES/extracted-> src/shapes.py::Base.area",
                    "src/shapes.py::Child -REFERENCES/extracted-> src/shapes.py::Base",
                    "src/shapes.py -CONTAINS/extracted-> src/shapes.py::Base",
                    "src/shapes.py::Base -CONTAINS/extracted-> src/shapes.py::Base.area",
                    "src/shapes.py -CONTAINS/extracted-> src/shapes.py::Child",
                    "src/shapes.py::Child -CONTAINS/extracted-> src/shapes.py::Child.area",
                    "src/shapes.py -CONTAINS/extracted-> src/shapes.py::main",
                ],
            ),
            (
                "a test file's qualified calls of one leaf",
                Ask::Links("tests/open.rs", true),
                &[
                    "tests/open.rs::opens -CALLS/extracted-> src/code_view.rs::FactsView::files",
                    "tests/open.rs -REFERENCES/extracted-> src/code_view.rs::FactsView::files",
                    "tests/open.rs::opens -CALLS/inferred-> tests/open.rs::%CodeView::files",
                    "tests/open.rs -REFERENCES/inferred-> tests/open.rs::%CodeView::files",
                    "tests/open.rs -CONTAINS/extracted-> tests/open.rs::opens",
                ],
            ),
            (
                "the narrowed definition's callers",
                Ask::UpHop("src/code_view.rs::FactsView::new", "new"),
                &[
                    "src/composed.rs::Composed::open -CALLS/extracted-> \
                     src/code_view.rs::FactsView::new",
                    "src/composed.rs -REFERENCES/extracted-> src/code_view.rs::FactsView::new",
                ],
            ),
            (
                "a test file's call landing on a placeholder, filed under each candidate it keeps",
                Ask::PlaceholderCalls("tests/open.rs"),
                &[
                    "src/code_view.rs::FactsView::files tests/open.rs::opens",
                    "src/composed.rs::Composed::files tests/open.rs::opens",
                    "src/snapshot.rs::Snapshot::files tests/open.rs::opens",
                ],
            ),
            (
                "a kept candidate's placeholder callers",
                Ask::PlaceholderCallers("src/snapshot.rs::Snapshot::files", "files"),
                &["tests/open.rs::opens"],
            ),
        ];
        for (shows, ask, want) in cases {
            assert_eq!(answer(&view, &r, &ask), want, "{shows}");
        }
    }

    /// A QUALIFIED CALL NARROWS ITS CANDIDATES (spec 117, RESOLVER AT READ), what a qualifier
    /// reaches: an implementation is `Q`'s only through an `impl`-tagged mention of `Q` it
    /// encloses, and it brings only the traits its own `impl`-tagged mentions name, read when no
    /// candidate sits under `Q` or its implementations (`Wrapper::area`). So `Q` in an
    /// implementation's header that does not implement it (`impl From<Source> for Wrapper`), a
    /// trait bound in its header (`impl<T: Draw> Holder<T>`), a trait another implementation in
    /// the file implements, and a type implementing the trait `Q` each add no parent, and the
    /// mention keeps the unqualified rule.
    #[test]
    fn a_qualifier_reaches_its_implementations_and_their_traits_alone() {
        let qualified_call =
            |name: &str, line| qualified(MentionKind::Call, name, Some(0), line, false);
        let shape = code(
            "src/shape.rs",
            "rust",
            Scope::Product,
            vec![
                def("Shape", "trait", 1, None, false),
                def("Shape::area", "method", 2, Some(0), false),
                def("Draw", "trait", 5, None, false),
                def("Draw::paint", "method", 6, Some(2), false),
                def("Source", "type", 9, None, false),
            ],
            vec![],
        );
        let wrap = code(
            "src/wrap.rs",
            "rust",
            Scope::Product,
            vec![
                def("Wrapper", "type", 1, None, false),
                def("Wrapper::Empty", "variant", 2, Some(0), false),
                def("impl From<Source> for Wrapper", "impl", 5, None, false),
                def("Wrapper::from", "method", 6, Some(2), false),
                def("impl Shape for Wrapper", "impl", 10, None, false),
                def("Holder", "type", 15, None, false),
                def("impl<T: Draw> Holder<T>", "impl", 17, None, false),
                def("Holder::get", "method", 18, Some(6), false),
            ],
            vec![
                tagged("impl", "From", Some(2), 5),
                tagged("type", "Source", Some(2), 5),
                tagged("impl", "Wrapper", Some(2), 5),
                tagged("impl", "Shape", Some(4), 10),
                tagged("impl", "Wrapper", Some(4), 10),
                tagged("type", "Draw", Some(6), 17),
                tagged("impl", "Holder", Some(6), 17),
            ],
        );
        let user = code(
            "src/user.rs",
            "rust",
            Scope::Product,
            vec![def("run", "function", 1, None, false)],
            vec![
                qualified_call("Wrapper::area", 2),
                qualified_call("Source::from", 3),
                qualified_call("Holder::area", 4),
                qualified_call("Holder::paint", 5),
                qualified(
                    MentionKind::Tagged("variant".into()),
                    "Shape::Empty",
                    Some(0),
                    6,
                    false,
                ),
            ],
        );
        let view = FactsView::new(stamp(), vec![shape, wrap, user]);
        let r = Resolver::new(&view);
        assert_eq!(
            answer(&view, &r, &Ask::Links("src/user.rs", false)),
            [
                // `impl Shape for Wrapper` brings `Shape`.
                "src/user.rs::run -CALLS/extracted-> src/shape.rs::Shape::area",
                "src/user.rs -REFERENCES/extracted-> src/shape.rs::Shape::area",
                // `Source` is a `type` mention in the header of `impl From<Source> for Wrapper`.
                "src/user.rs::run -CALLS/inferred-> src/user.rs::%from",
                "src/user.rs -REFERENCES/inferred-> src/user.rs::%from",
                // `Shape` is implemented by `impl Shape for Wrapper`, not by `Holder`'s.
                "src/user.rs::run -CALLS/inferred-> src/user.rs::%area",
                "src/user.rs -REFERENCES/inferred-> src/user.rs::%area",
                // `Draw` is a `type` mention, a bound, in the header of `Holder`'s.
                "src/user.rs::run -CALLS/inferred-> src/user.rs::%paint",
                "src/user.rs -REFERENCES/inferred-> src/user.rs::%paint",
                // `Wrapper`, which implements `Shape`, is no parent of `Shape`.
                "src/user.rs::run -REFERENCES/inferred-> src/wrap.rs::Wrapper::Empty",
                "src/user.rs -CONTAINS/extracted-> src/user.rs::run",
            ],
            "each qualified mention narrowed by its qualifier's implementations and their traits \
             alone, else by the unqualified rule"
        );
    }

    /// A QUALIFIED CALL NARROWS ITS CANDIDATES (spec 117, RESOLVER AT READ) without letting a
    /// test definition change a product edge: an implementation that is test code, by its own
    /// mark (`Keeper`'s) or by its file's scope (`Holder`'s), is a parent of a test mention's
    /// qualifier alone. So a product call to `Keeper::area` or `Holder::area`, and a product
    /// `const` reference to `Keeper::SIDES`, keep the unqualified rule, while the same test
    /// mentions reach `Shape::area` and `Shape::SIDES` through the test `impl Shape for ...`.
    /// One resolver answers the product file first, so a parent set held for the qualifier alone
    /// would hand the test mentions the product answer.
    #[test]
    fn a_test_implementation_narrows_test_mentions_alone() {
        let qualified_call =
            |name: &str, line, test| qualified(MentionKind::Call, name, Some(0), line, test);
        let qualified_const = |name: &str, line, test| {
            qualified(
                MentionKind::Tagged("const".into()),
                name,
                Some(0),
                line,
                test,
            )
        };
        let shape = code(
            "src/shape.rs",
            "rust",
            Scope::Product,
            vec![
                def("Shape", "trait", 1, None, false),
                def("Shape::area", "method", 2, Some(0), false),
                def("Shape::SIDES", "constant", 3, Some(0), false),
            ],
            vec![],
        );
        let keep = code(
            "src/keep.rs",
            "rust",
            Scope::Product,
            vec![
                def("Keeper", "type", 1, None, false),
                def("impl Shape for Keeper", "impl", 4, None, true),
            ],
            vec![
                tagged("impl", "Shape", Some(1), 4),
                tagged("impl", "Keeper", Some(1), 4),
            ],
        );
        let holder = code(
            "src/holder.rs",
            "rust",
            Scope::Product,
            vec![def("Holder", "type", 1, None, false)],
            vec![],
        );
        let holder_test = code(
            "tests/holder.rs",
            "rust",
            Scope::Test,
            vec![def("impl Shape for Holder", "impl", 1, None, false)],
            vec![
                tagged("impl", "Shape", Some(0), 1),
                tagged("impl", "Holder", Some(0), 1),
            ],
        );
        let user = code(
            "src/user.rs",
            "rust",
            Scope::Product,
            vec![def("run", "function", 1, None, false)],
            vec![
                qualified_call("Keeper::area", 2, false),
                qualified_call("Holder::area", 3, false),
                qualified_const("Keeper::SIDES", 4, false),
            ],
        );
        let user_test = code(
            "tests/user.rs",
            "rust",
            Scope::Test,
            vec![def("checks", "function", 1, None, true)],
            vec![
                qualified_call("Keeper::area", 2, true),
                qualified_call("Holder::area", 3, true),
                qualified_const("Keeper::SIDES", 4, true),
            ],
        );
        let view = FactsView::new(
            stamp(),
            vec![shape, keep, holder, holder_test, user, user_test],
        );
        let r = Resolver::new(&view);
        // (what the answer shows, the question, the whole answer)
        let cases: [(&str, Ask, &[&str]); 2] = [
            (
                "a product mention keeps the unqualified rule past a test implementation",
                Ask::Links("src/user.rs", false),
                &[
                    "src/user.rs::run -CALLS/inferred-> src/user.rs::%area",
                    "src/user.rs -REFERENCES/inferred-> src/user.rs::%area",
                    "src/user.rs::run -CALLS/inferred-> src/user.rs::%area",
                    "src/user.rs -REFERENCES/inferred-> src/user.rs::%area",
                    "src/user.rs::run -REFERENCES/inferred-> src/shape.rs::Shape::SIDES",
                    "src/user.rs -CONTAINS/extracted-> src/user.rs::run",
                ],
            ),
            (
                "a test mention reaches the trait a test implementation names",
                Ask::Links("tests/user.rs", true),
                &[
                    "tests/user.rs::checks -CALLS/extracted-> src/shape.rs::Shape::area",
                    "tests/user.rs -REFERENCES/extracted-> src/shape.rs::Shape::area",
                    "tests/user.rs::checks -CALLS/extracted-> src/shape.rs::Shape::area",
                    "tests/user.rs -REFERENCES/extracted-> src/shape.rs::Shape::area",
                    "tests/user.rs::checks -REFERENCES/extracted-> src/shape.rs::Shape::SIDES",
                    "tests/user.rs -CONTAINS/extracted-> tests/user.rs::checks",
                ],
            ),
        ];
        for (shows, ask, want) in cases {
            assert_eq!(answer(&view, &r, &ask), want, "{shows}");
        }
    }

    /// A QUALIFIED CALL NARROWS ITS CANDIDATES (spec 117, RESOLVER AT READ) to an
    /// implementation's own member before its trait's: `Square::area` and `Square::SIDES` land
    /// on the `area` and `SIDES` that `impl Shape for Square` defines, EXTRACTED, never beside
    /// `Shape`'s. A trait's member is kept only when no call target or reference target of the
    /// leaf sits under `Square` or an implementation of it, so `Square::name` reaches `Shape`'s
    /// default `name` past the field `name` of `Square`, which no call lands on.
    #[test]
    fn an_implementations_own_member_outranks_its_traits() {
        let shape = code(
            "src/shape.rs",
            "rust",
            Scope::Product,
            vec![
                def("Shape", "trait", 1, None, false),
                def("Shape::area", "method", 2, Some(0), false),
                def("Shape::SIDES", "constant", 3, Some(0), false),
                def("Shape::name", "method", 4, Some(0), false),
            ],
            vec![],
        );
        let square = code(
            "src/square.rs",
            "rust",
            Scope::Product,
            vec![
                def("Square", "type", 1, None, false),
                def("Square::name", "field", 2, Some(0), false),
                def("impl Shape for Square", "impl", 5, None, false),
                def("Square::area", "method", 6, Some(2), false),
                def("Square::SIDES", "constant", 7, Some(2), false),
            ],
            vec![
                tagged("impl", "Shape", Some(2), 5),
                tagged("impl", "Square", Some(2), 5),
            ],
        );
        let user = code(
            "src/user.rs",
            "rust",
            Scope::Product,
            vec![def("run", "function", 1, None, false)],
            vec![
                qualified(MentionKind::Call, "Square::area", Some(0), 2, false),
                qualified(
                    MentionKind::Tagged("const".into()),
                    "Square::SIDES",
                    Some(0),
                    3,
                    false,
                ),
                qualified(MentionKind::Call, "Square::name", Some(0), 4, false),
            ],
        );
        let view = FactsView::new(stamp(), vec![shape, square, user]);
        let r = Resolver::new(&view);
        assert_eq!(
            answer(&view, &r, &Ask::Links("src/user.rs", false)),
            [
                "src/user.rs::run -CALLS/extracted-> src/square.rs::Square::area",
                "src/user.rs -REFERENCES/extracted-> src/square.rs::Square::area",
                "src/user.rs::run -CALLS/extracted-> src/shape.rs::Shape::name",
                "src/user.rs -REFERENCES/extracted-> src/shape.rs::Shape::name",
                "src/user.rs::run -REFERENCES/extracted-> src/square.rs::Square::SIDES",
                "src/user.rs -CONTAINS/extracted-> src/user.rs::run",
            ],
            "the implementation's own member first, its trait's only when it has none"
        );
    }

    /// A QUALIFIED CALL NARROWS ITS CANDIDATES (spec 117, RESOLVER AT READ) over the candidates
    /// its rule reads alone. Each qualifier below holds, beside the one target, a definition of
    /// the leaf that rule refuses: a test double's `Config::load` for a product call, the field
    /// `Point::x` for any call, and a C++ `Limits::MAX` and the field `Limits::MAX` for a Rust
    /// `const` reference. None is kept, so each mention lands on its target, EXTRACTED, where
    /// keeping one would leave a call on its placeholder and a reference on no edge.
    #[test]
    fn a_qualified_mention_narrows_only_the_candidates_its_rule_reads() {
        let qualified_call = |name: &str| qualified(MentionKind::Call, name, Some(0), 2, false);
        let config = code(
            "src/config.hpp",
            "cpp",
            Scope::Product,
            vec![
                def("Config", "type", 1, None, false),
                def("Config::load", "method", 2, Some(0), false),
            ],
            vec![],
        );
        let fake_config = code(
            "tests/fake_config.hpp",
            "cpp",
            Scope::Test,
            vec![
                def("Config", "type", 1, None, false),
                def("Config::load", "method", 2, Some(0), false),
            ],
            vec![],
        );
        let app = code(
            "src/app.cpp",
            "cpp",
            Scope::Product,
            vec![def("main", "function", 1, None, false)],
            vec![qualified_call("Config::load")],
        );
        let point = code(
            "src/point.rs",
            "rust",
            Scope::Product,
            vec![
                def("Point", "type", 1, None, false),
                def("Point::x", "field", 2, Some(0), false),
            ],
            vec![],
        );
        let point_ops = code(
            "src/point_ops.rs",
            "rust",
            Scope::Product,
            vec![
                def("impl Point", "impl", 1, None, false),
                def("Point::x", "method", 2, Some(0), false),
            ],
            vec![tagged("impl", "Point", Some(0), 1)],
        );
        let plot = code(
            "src/plot.rs",
            "rust",
            Scope::Product,
            vec![def("plot", "function", 1, None, false)],
            vec![qualified_call("Point::x")],
        );
        let limits = code(
            "src/limits.rs",
            "rust",
            Scope::Product,
            vec![
                def("Limits", "type", 1, None, false),
                def("Limits::MAX", "field", 2, Some(0), false),
            ],
            vec![],
        );
        let consts = code(
            "src/consts.rs",
            "rust",
            Scope::Product,
            vec![
                def("impl Limits", "impl", 1, None, false),
                def("Limits::MAX", "constant", 2, Some(0), false),
            ],
            vec![tagged("impl", "Limits", Some(0), 1)],
        );
        let limits_hpp = code(
            "src/limits.hpp",
            "cpp",
            Scope::Product,
            vec![
                def("Limits", "type", 1, None, false),
                def("Limits::MAX", "constant", 2, Some(0), false),
            ],
            vec![],
        );
        let cap = code(
            "src/cap.rs",
            "rust",
            Scope::Product,
            vec![def("cap", "function", 1, None, false)],
            vec![qualified(
                MentionKind::Tagged("const".into()),
                "Limits::MAX",
                Some(0),
                2,
                false,
            )],
        );
        let view = FactsView::new(
            stamp(),
            vec![
                config,
                fake_config,
                app,
                point,
                point_ops,
                plot,
                limits,
                consts,
                limits_hpp,
                cap,
            ],
        );
        let r = Resolver::new(&view);
        // (what the answer shows, the question, the whole answer)
        let cases: [(&str, Ask, &[&str]); 3] = [
            (
                "a product call past a test definition of its leaf under its qualifier",
                Ask::Links("src/app.cpp", false),
                &[
                    "src/app.cpp::main -CALLS/extracted-> src/config.hpp::Config::load",
                    "src/app.cpp -REFERENCES/extracted-> src/config.hpp::Config::load",
                    "src/app.cpp -CONTAINS/extracted-> src/app.cpp::main",
                ],
            ),
            (
                "a call past a field of its leaf under its qualifier",
                Ask::Links("src/plot.rs", false),
                &[
                    "src/plot.rs::plot -CALLS/extracted-> src/point_ops.rs::Point::x",
                    "src/plot.rs -REFERENCES/extracted-> src/point_ops.rs::Point::x",
                    "src/plot.rs -CONTAINS/extracted-> src/plot.rs::plot",
                ],
            ),
            (
                "a reference past another language's and another kind's definition of its leaf",
                Ask::Links("src/cap.rs", false),
                &[
                    "src/cap.rs::cap -REFERENCES/extracted-> src/consts.rs::Limits::MAX",
                    "src/cap.rs -CONTAINS/extracted-> src/cap.rs::cap",
                ],
            ),
        ];
        for (shows, ask, want) in cases {
            assert_eq!(answer(&view, &r, &ask), want, "{shows}");
        }
    }

    /// A mention whose name is its leaf has no qualifier ([`qualifier`]), even when that leaf
    /// holds a separator, as a leaf written whole (`Ns.IShape`) does: it keeps the unqualified
    /// rule, so `Circle`'s reference to the `Ns.IShape` of another file is INFERRED, never
    /// narrowed by `Ns` to EXTRACTED.
    #[test]
    fn a_name_written_as_its_leaf_has_no_qualifier_though_it_holds_a_separator() {
        let shapes = code(
            "src/shapes.cs",
            "csharp",
            Scope::Product,
            vec![
                def("Ns", "module", 1, None, false),
                def("Ns.IShape", "trait", 2, Some(0), false),
            ],
            vec![],
        );
        let circle = code(
            "src/circle.cs",
            "csharp",
            Scope::Product,
            vec![def("Circle", "type", 1, None, false)],
            vec![tagged("type", "Ns.IShape", Some(0), 1)],
        );
        let view = FactsView::new(stamp(), vec![shapes, circle]);
        let r = Resolver::new(&view);
        assert_eq!(
            answer(&view, &r, &Ask::Links("src/circle.cs", false)),
            [
                "src/circle.cs::Circle -REFERENCES/inferred-> src/shapes.cs::Ns.IShape",
                "src/circle.cs -CONTAINS/extracted-> src/circle.cs::Circle",
            ],
            "a leaf holding a separator, written as its name, keeps the unqualified rule"
        );
    }

    /// A tree of qualified mentions whose qualifiers keep no candidate of their rule, but one:
    /// `AtomicU64::new` beside the project's `Cursor::new`, in its file and another, and
    /// `AtomicU64::fetch_add`, which nothing defines, from product and test code; `load`
    /// qualified by a module, a C++ type, a test type, a product type and a product trait, and
    /// the variant reference `ErrorKind::NotFound`; the C# member qualifiers
    /// `Request.GetClientIp`, `Request.Missing` and the type reference `Models.Shape`; the test
    /// calls `Gauge::unload` and `Atomic::unload`, whose leaf nothing defines; and the one whose
    /// qualifier keeps a candidate, the test call `FakeStore::open` to a test type's own method.
    pub(crate) fn qualifier_fixture() -> FactsView {
        let cursor = code(
            "src/cursor.rs",
            "rust",
            Scope::Product,
            vec![
                def("Cursor", "type", 1, None, false),
                def("Cursor::new", "method", 2, Some(0), false),
            ],
            vec![qualified(
                MentionKind::Call,
                "AtomicU64::new",
                Some(1),
                3,
                false,
            )],
        );
        let seq = code(
            "src/seq.rs",
            "rust",
            Scope::Product,
            vec![
                def("TEMP_SEQ", "static", 1, None, false),
                def("next", "function", 3, None, false),
            ],
            vec![
                qualified(MentionKind::Call, "AtomicU64::new", Some(0), 1, false),
                qualified(MentionKind::Call, "AtomicU64::fetch_add", Some(1), 4, false),
            ],
        );
        let seq_test = code(
            "tests/seq.rs",
            "rust",
            Scope::Test,
            vec![def("counts", "function", 1, None, true)],
            vec![
                qualified(MentionKind::Call, "AtomicU64::new", Some(0), 2, true),
                qualified(MentionKind::Call, "AtomicU64::fetch_add", Some(0), 3, true),
            ],
        );
        let loader = code(
            "src/loader.rs",
            "rust",
            Scope::Product,
            vec![
                def("load", "function", 1, None, false),
                def("cache", "module", 3, None, false),
                def("Gauge", "type", 5, None, false),
                def("Meter", "trait", 7, None, false),
                def("LoadError", "type", 9, None, false),
                def("LoadError::NotFound", "variant", 10, Some(4), false),
            ],
            vec![],
        );
        let atomic = code(
            "src/atomic.hpp",
            "cpp",
            Scope::Product,
            vec![def("Atomic", "type", 1, None, false)],
            vec![],
        );
        let fake = code(
            "tests/fake.rs",
            "rust",
            Scope::Test,
            vec![def("Fake", "type", 1, None, false)],
            vec![],
        );
        let app = code(
            "src/app.rs",
            "rust",
            Scope::Product,
            vec![def("main", "function", 1, None, false)],
            vec![
                qualified(MentionKind::Call, "cache::load", Some(0), 2, false),
                qualified(MentionKind::Call, "Atomic::load", Some(0), 3, false),
                qualified(MentionKind::Call, "Fake::load", Some(0), 4, false),
                qualified(MentionKind::Call, "Gauge::load", Some(0), 5, false),
                qualified(MentionKind::Call, "Meter::load", Some(0), 6, false),
                qualified(
                    MentionKind::Tagged("variant".into()),
                    "ErrorKind::NotFound",
                    Some(0),
                    7,
                    false,
                ),
            ],
        );
        let app_test = code(
            "tests/app.rs",
            "rust",
            Scope::Test,
            vec![def("checks", "function", 1, None, true)],
            vec![
                qualified(MentionKind::Call, "Fake::load", Some(0), 2, true),
                qualified(MentionKind::Call, "Gauge::load", Some(0), 3, true),
            ],
        );
        let http = code(
            "src/http.cs",
            "csharp",
            Scope::Product,
            vec![
                def("HttpExtensions", "type", 1, None, false),
                dotted("HttpExtensions.GetClientIp", "method", 2, 0),
                def("Shape", "type", 5, None, false),
            ],
            vec![],
        );
        let handler = code(
            "src/handler.cs",
            "csharp",
            Scope::Product,
            vec![
                def("Handler", "type", 1, None, false),
                dotted("Handler.Handle", "method", 2, 0),
            ],
            vec![
                qualified(MentionKind::Call, "Request.GetClientIp", Some(1), 3, false),
                qualified(MentionKind::Call, "Request.Missing", Some(1), 4, false),
                qualified(
                    MentionKind::Tagged("type".into()),
                    "Models.Shape",
                    Some(1),
                    5,
                    false,
                ),
            ],
        );
        let support = code(
            "tests/support.rs",
            "rust",
            Scope::Test,
            vec![
                def("FakeStore", "type", 1, None, true),
                def("FakeStore::open", "method", 2, Some(0), true),
            ],
            vec![],
        );
        let store = code(
            "tests/store.rs",
            "rust",
            Scope::Test,
            vec![def("opens", "function", 1, None, true)],
            vec![qualified(
                MentionKind::Call,
                "FakeStore::open",
                Some(0),
                2,
                true,
            )],
        );
        let gauge = code(
            "tests/gauge.rs",
            "rust",
            Scope::Test,
            vec![
                def("drains", "function", 1, None, true),
                def("swaps", "function", 4, None, true),
            ],
            vec![
                qualified(MentionKind::Call, "Gauge::unload", Some(0), 2, true),
                qualified(MentionKind::Call, "Atomic::unload", Some(1), 5, true),
            ],
        );
        FactsView::new(
            stamp(),
            vec![
                cursor, seq, seq_test, loader, atomic, fake, app, app_test, http, handler, support,
                store, gauge,
            ],
        )
    }

    /// A QUALIFIER NAMING NO DEFINITION (spec 117, RESOLVER AT READ): when a qualified mention's
    /// qualifier keeps no candidate of its rule ([`Resolver::narrowed`]), a member qualifier
    /// (`Q.leaf`) takes the unqualified rule, as does a path qualifier (`Q::leaf`) naming a
    /// product type or trait in the mention's language; any other path qualifier makes no link
    /// and no placeholder. So `AtomicU64::new` beside the project's `Cursor::new`, in its file or
    /// another, and `AtomicU64::fetch_add`, which nothing defines, land nowhere, a test call past
    /// its proof too; so do `cache::load`, `Atomic::load` and `Fake::load`, their qualifier named
    /// only by a module, a C++ type or a test type, from product and test code alike, and the
    /// variant reference `ErrorKind::NotFound`. `Gauge::load`, `Meter::load`,
    /// `Request.GetClientIp`, `Request.Missing` and the type reference `Models.Shape` keep the
    /// unqualified rule, and so the test call `Gauge::unload`, which nothing defines, lands on
    /// its placeholder past every proof, while `Atomic::unload`, its qualifier named only by a
    /// C++ type, lands nowhere. A qualifier that keeps a candidate links it whatever it names:
    /// the test call `FakeStore::open`, its qualifier a test type, lands on that type's `open`.
    #[test]
    fn a_qualifier_naming_no_definition_makes_no_link_and_a_member_qualifier_falls_back() {
        let view = qualifier_fixture();
        let r = Resolver::new(&view);
        // (what the answer shows, the question, the whole answer)
        let cases: [(&str, Ask, &[&str]); 12] = [
            (
                "a path qualifier naming none, past a same-file call target of its leaf",
                Ask::Links("src/cursor.rs", false),
                &[
                    "src/cursor.rs -CONTAINS/extracted-> src/cursor.rs::Cursor",
                    "src/cursor.rs::Cursor -CONTAINS/extracted-> src/cursor.rs::Cursor::new",
                ],
            ),
            (
                "a path qualifier naming none, past a leaf defined in another file or nowhere",
                Ask::Links("src/seq.rs", false),
                &[
                    "src/seq.rs -CONTAINS/extracted-> src/seq.rs::TEMP_SEQ",
                    "src/seq.rs -CONTAINS/extracted-> src/seq.rs::next",
                ],
            ),
            (
                "a test call whose path qualifier names none, past its proof",
                Ask::Links("tests/seq.rs", true),
                &["tests/seq.rs -CONTAINS/extracted-> tests/seq.rs::counts"],
            ),
            (
                "no test call of a path qualifier naming none lands on a placeholder",
                Ask::PlaceholderCalls("tests/seq.rs"),
                &[],
            ),
            (
                "no placeholder caller of a leaf nothing defines",
                Ask::PlaceholderCallers("tests/seq.rs::%fetch_add", "fetch_add"),
                &[],
            ),
            (
                "no caller of the project definition of the leaf",
                Ask::UpHop("src/cursor.rs::Cursor::new", "new"),
                &[],
            ),
            (
                "a qualifier named only by a module, a C++ type or a test type, then by a \
                 product type and a product trait, then a variant reference",
                Ask::Links("src/app.rs", false),
                &[
                    "src/app.rs::main -CALLS/inferred-> src/app.rs::%load",
                    "src/app.rs -REFERENCES/inferred-> src/app.rs::%load",
                    "src/app.rs::main -CALLS/inferred-> src/app.rs::%load",
                    "src/app.rs -REFERENCES/inferred-> src/app.rs::%load",
                    "src/app.rs -CONTAINS/extracted-> src/app.rs::main",
                ],
            ),
            (
                "a test mention's qualifier named only by a test type, then by a product type",
                Ask::Links("tests/app.rs", true),
                &[
                    "tests/app.rs::checks -CALLS/inferred-> src/loader.rs::load",
                    "tests/app.rs -REFERENCES/inferred-> src/loader.rs::load",
                    "tests/app.rs -CONTAINS/extracted-> tests/app.rs::checks",
                ],
            ),
            (
                "member qualifiers naming none keep the unqualified rule",
                Ask::Links("src/handler.cs", false),
                &[
                    "src/handler.cs::Handler.Handle -CALLS/inferred-> \
                     src/handler.cs::%GetClientIp",
                    "src/handler.cs -REFERENCES/inferred-> src/handler.cs::%GetClientIp",
                    "src/handler.cs::Handler.Handle -CALLS/ambiguous-> src/handler.cs::%Missing",
                    "src/handler.cs -REFERENCES/ambiguous-> src/handler.cs::%Missing",
                    "src/handler.cs::Handler.Handle -REFERENCES/inferred-> src/http.cs::Shape",
                    "src/handler.cs -CONTAINS/extracted-> src/handler.cs::Handler",
                    "src/handler.cs::Handler -CONTAINS/extracted-> \
                     src/handler.cs::Handler.Handle",
                ],
            ),
            (
                "a test call whose path qualifier, a test type, keeps that type's own method",
                Ask::Links("tests/store.rs", true),
                &[
                    "tests/store.rs::opens -CALLS/extracted-> tests/support.rs::FakeStore::open",
                    "tests/store.rs -REFERENCES/extracted-> tests/support.rs::FakeStore::open",
                    "tests/store.rs -CONTAINS/extracted-> tests/store.rs::opens",
                ],
            ),
            (
                "a test call whose path qualifier names a product type lands on a placeholder, \
                 one whose qualifier names only another language's type on none",
                Ask::PlaceholderCalls("tests/gauge.rs"),
                &["unload tests/gauge.rs::drains"],
            ),
            (
                "the placeholder caller of a leaf nothing defines, by its file's language",
                Ask::PlaceholderCallers("tests/gauge.rs::%unload", "unload"),
                &["tests/gauge.rs::drains"],
            ),
        ];
        for (shows, ask, want) in cases {
            assert_eq!(answer(&view, &r, &ask), want, "{shows}");
        }
    }

    /// SEVERAL NARROWED CANDIDATES NEVER PREFER THEIR FILE (spec 117, RESOLVER AT READ): a call
    /// whose qualifier keeps several candidates lands on the placeholder keyed by its qualified
    /// name (`F::%Q.leaf`), never on the candidate in its own file, and that placeholder's
    /// down-hop is the kept set alone. So `CodeView.file`, written in the trait's default method
    /// `describe` beside `FactsView`'s `file`, with `Snapshot`'s `file` in another file, reaches
    /// both implementations and never `Store`'s `file`: in the resolver's links, down-hop,
    /// up-hops and hidden test callers, and in `Composed`'s call walk over either code half,
    /// which lists both as `describe`'s callees and `describe` as the caller of each. A shown
    /// test call keeps the test implementation `Fake` beside them, and a file-level reference
    /// to the leaf stays held by the call. The walk reads a product call's placeholder in the
    /// product pool under every filter, as the resolver's up-hop reads a link in its own: so
    /// `Cube.size` keeps its traits' `size` methods, and a test-only `Cube`'s own `size`, which
    /// the test pool keeps in their place, joins them only when tests are shown, the set its
    /// node's `candidates` attribute carries.
    #[test]
    fn several_narrowed_candidates_never_prefer_their_file() {
        let qualified_call = |enclosing, line, test| {
            qualified(
                MentionKind::Call,
                "CodeView.file",
                Some(enclosing),
                line,
                test,
            )
        };
        let code_view = code(
            "src/code_view.rs",
            "rust",
            Scope::Product,
            vec![
                def("CodeView", "trait", 1, None, false),
                def("CodeView::describe", "method", 2, Some(0), false),
                def("FactsView", "type", 10, None, false),
                def("impl CodeView for FactsView", "impl", 12, None, false),
                def("FactsView::file", "method", 13, Some(3), false),
            ],
            vec![
                qualified_call(1, 3, false),
                tagged("field", "file", None, 5),
                tagged("impl", "CodeView", Some(3), 12),
                tagged("impl", "FactsView", Some(3), 12),
            ],
        );
        let snapshot = code(
            "src/snapshot.rs",
            "rust",
            Scope::Product,
            vec![
                def("Snapshot", "type", 1, None, false),
                def("impl CodeView for Snapshot", "impl", 3, None, false),
                def("Snapshot::file", "method", 4, Some(1), false),
            ],
            vec![
                tagged("impl", "CodeView", Some(1), 3),
                tagged("impl", "Snapshot", Some(1), 3),
            ],
        );
        let store = code(
            "src/store.rs",
            "rust",
            Scope::Product,
            vec![
                def("Store", "type", 1, None, false),
                def("impl Store", "impl", 3, None, false),
                def("Store::file", "method", 4, Some(1), false),
                def("Config", "type", 10, None, false),
                def("Config::file", "field", 11, Some(3), false),
            ],
            vec![tagged("impl", "Store", Some(1), 3)],
        );
        let describe = code(
            "tests/describe.rs",
            "rust",
            Scope::Test,
            vec![
                def("describes", "function", 1, None, true),
                def("Fake", "type", 5, None, true),
                def("impl CodeView for Fake", "impl", 6, None, true),
                def("Fake::file", "method", 7, Some(2), true),
            ],
            vec![
                qualified_call(0, 2, true),
                tagged("impl", "CodeView", Some(2), 6),
                tagged("impl", "Fake", Some(2), 6),
            ],
        );
        // `Cube.size`, written in `report`, whose qualifier's own tier holds no product `size`, so
        // its traits' methods are its product set, beside a test-only `Cube` whose own `size`
        // fills that tier in the test pool.
        let shape = code(
            "src/shape.rs",
            "rust",
            Scope::Product,
            vec![
                def("Area", "trait", 1, None, false),
                def("Area::size", "method", 2, Some(0), false),
                def("Volume", "trait", 5, None, false),
                def("Volume::size", "method", 6, Some(2), false),
            ],
            vec![],
        );
        let cube = code(
            "src/cube.rs",
            "rust",
            Scope::Product,
            vec![
                def("Cube", "type", 1, None, false),
                def("impl Area for Cube", "impl", 3, None, false),
                def("impl Volume for Cube", "impl", 5, None, false),
            ],
            vec![
                tagged("impl", "Area", Some(1), 3),
                tagged("impl", "Cube", Some(1), 3),
                tagged("impl", "Volume", Some(2), 5),
                tagged("impl", "Cube", Some(2), 5),
            ],
        );
        let report = code(
            "src/report.rs",
            "rust",
            Scope::Product,
            vec![def("report", "function", 1, None, false)],
            vec![qualified(MentionKind::Call, "Cube.size", Some(0), 2, false)],
        );
        let test_cube = code(
            "tests/cube.rs",
            "rust",
            Scope::Test,
            vec![
                def("Cube", "type", 1, None, true),
                def("impl Cube", "impl", 3, None, true),
                def("Cube::size", "method", 4, Some(1), true),
            ],
            vec![tagged("impl", "Cube", Some(1), 3)],
        );
        let view = FactsView::new(
            stamp(),
            vec![
                code_view, snapshot, store, describe, shape, cube, report, test_cube,
            ],
        );
        let r = Resolver::new(&view);
        // (what the answer shows, the question, the whole answer)
        let asks: [(&str, Ask, &[&str]); 13] = [
            (
                "a product call keeping two implementations, one in its own file, and a \
                 file-level reference to the leaf held by it",
                Ask::Links("src/code_view.rs", false),
                &[
                    "src/code_view.rs::CodeView::describe -CALLS/inferred-> \
                     src/code_view.rs::%CodeView.file",
                    "src/code_view.rs -REFERENCES/inferred-> src/code_view.rs::%CodeView.file",
                    "src/code_view.rs::impl CodeView for FactsView -REFERENCES/extracted-> \
                     src/code_view.rs::CodeView",
                    "src/code_view.rs::impl CodeView for FactsView -REFERENCES/extracted-> \
                     src/code_view.rs::FactsView",
                    "src/code_view.rs -CONTAINS/extracted-> src/code_view.rs::CodeView",
                    "src/code_view.rs::CodeView -CONTAINS/extracted-> \
                     src/code_view.rs::CodeView::describe",
                    "src/code_view.rs -CONTAINS/extracted-> src/code_view.rs::FactsView",
                    "src/code_view.rs::impl CodeView for FactsView -CONTAINS/extracted-> \
                     src/code_view.rs::FactsView::file",
                    "src/code_view.rs -CONTAINS/extracted-> \
                     src/code_view.rs::impl CodeView for FactsView",
                ],
            ),
            (
                "a test call keeping the test implementation too, past its proof",
                Ask::Links("tests/describe.rs", true),
                &[
                    "tests/describe.rs::describes -CALLS/inferred-> \
                     tests/describe.rs::%CodeView.file",
                    "tests/describe.rs -REFERENCES/inferred-> tests/describe.rs::%CodeView.file",
                    "tests/describe.rs::impl CodeView for Fake -REFERENCES/inferred-> \
                     src/code_view.rs::CodeView",
                    "tests/describe.rs::impl CodeView for Fake -REFERENCES/extracted-> \
                     tests/describe.rs::Fake",
                    "tests/describe.rs -CONTAINS/extracted-> tests/describe.rs::Fake",
                    "tests/describe.rs::impl CodeView for Fake -CONTAINS/extracted-> \
                     tests/describe.rs::Fake::file",
                    "tests/describe.rs -CONTAINS/extracted-> tests/describe.rs::describes",
                    "tests/describe.rs -CONTAINS/extracted-> \
                     tests/describe.rs::impl CodeView for Fake",
                ],
            ),
            (
                "the product placeholder's down-hop, the product implementations",
                Ask::DownHop("src/code_view.rs::%CodeView.file", Pool::Product),
                &["frontier src/code_view.rs::FactsView::file src/snapshot.rs::Snapshot::file"],
            ),
            (
                "the test placeholder's down-hop, the test implementation too",
                Ask::DownHop("tests/describe.rs::%CodeView.file", Pool::All),
                &[
                    "frontier src/code_view.rs::FactsView::file src/snapshot.rs::Snapshot::file \
                     tests/describe.rs::Fake::file",
                ],
            ),
            (
                "the caller of the implementation in the call's file",
                Ask::UpHop("src/code_view.rs::FactsView::file", "file"),
                &[
                    "src/code_view.rs::CodeView::describe -CALLS/inferred-> \
                     src/code_view.rs::%CodeView.file",
                    "src/code_view.rs -REFERENCES/inferred-> src/code_view.rs::%CodeView.file",
                ],
            ),
            (
                "the caller of the implementation in another file",
                Ask::UpHop("src/snapshot.rs::Snapshot::file", "file"),
                &[
                    "src/code_view.rs::CodeView::describe -CALLS/inferred-> \
                     src/code_view.rs::%CodeView.file",
                    "src/code_view.rs -REFERENCES/inferred-> src/code_view.rs::%CodeView.file",
                ],
            ),
            (
                "no caller of another type's method of the leaf",
                Ask::UpHop("src/store.rs::Store::file", "file"),
                &[],
            ),
            (
                "the test call's placeholder filed under each candidate it keeps",
                Ask::PlaceholderCalls("tests/describe.rs"),
                &[
                    "src/code_view.rs::FactsView::file tests/describe.rs::describes",
                    "src/snapshot.rs::Snapshot::file tests/describe.rs::describes",
                    "tests/describe.rs::Fake::file tests/describe.rs::describes",
                ],
            ),
            (
                "the hidden test caller of an implementation",
                Ask::PlaceholderCallers("src/snapshot.rs::Snapshot::file", "file"),
                &["tests/describe.rs::describes"],
            ),
            (
                "no hidden test caller of another type's method of the leaf",
                Ask::PlaceholderCallers("src/store.rs::Store::file", "file"),
                &[],
            ),
            (
                "a product call's down-hop in its own pool, its qualifier's traits' methods",
                Ask::DownHop("src/report.rs::%Cube.size", Pool::Product),
                &["frontier src/shape.rs::Area::size src/shape.rs::Volume::size"],
            ),
            (
                "the same placeholder in the test pool, the test type's own method alone",
                Ask::DownHop("src/report.rs::%Cube.size", Pool::All),
                &["one tests/cube.rs::Cube::size"],
            ),
            (
                "the product caller of a trait method, read in its link's pool",
                Ask::UpHop("src/shape.rs::Area::size", "size"),
                &[
                    "src/report.rs::report -CALLS/inferred-> src/report.rs::%Cube.size",
                    "src/report.rs -REFERENCES/inferred-> src/report.rs::%Cube.size",
                ],
            ),
        ];
        for (shows, ask, want) in asks {
            assert_eq!(answer(&view, &r, &ask), want, "{shows}");
        }
        let records = Indexed::default();
        let honesty = Honesty {
            worktree: "wt".into(),
            compared: Compared::Overlaid(0),
            behind: None,
        };
        // The walk from `seed` as lines: each node by layer, with the frontier it is when it is
        // one, each edge, each file referencing a seed's leaf and calling it from nowhere, and
        // the hidden test callers of the seed.
        let walk = |graph: &dyn ContextGraph, seed: &str, direction| -> Vec<String> {
            let seed = [seed.to_string()];
            let walked = graph.calls(&seed, direction, 2, TIER_INFERRED).unwrap();
            let nodes = walked.nodes.iter().map(|n| match &n.frontier {
                Some(cands) => format!("{} {} frontier {}", n.layer, n.node.id, cands.join(" ")),
                None => format!("{} {}", n.layer, n.node.id),
            });
            let edges = (walked.edges.iter())
                .map(|e| format!("{} -{}-> {}", e.edge.from, e.edge.tier, e.edge.to));
            let referenced =
                (walked.referenced_not_called.iter()).map(|n| format!("referenced {}", n.id));
            let hidden = graph.hidden_placeholder_callers(&seed).unwrap();
            (nodes.chain(edges).chain(referenced))
                .chain([format!("hidden {hidden:?}")])
                .collect()
        };
        // (what the walk shows, the tests shown, the seed, the direction, the whole walk)
        let walks: [(&str, Tests, &str, Direction, &[&str]); 9] = [
            (
                "the trait method's callees: both implementations, a frontier",
                Tests::Exclude,
                "src/code_view.rs::CodeView::describe",
                Direction::Down,
                &[
                    "0 src/code_view.rs::CodeView::describe",
                    "1 src/code_view.rs::%CodeView.file frontier \
                     src/code_view.rs::FactsView::file src/snapshot.rs::Snapshot::file",
                    "src/code_view.rs::CodeView::describe -inferred-> \
                     src/code_view.rs::%CodeView.file",
                    "hidden []",
                ],
            ),
            (
                "the caller of the implementation in the call's file, and its hidden test caller",
                Tests::Exclude,
                "src/code_view.rs::FactsView::file",
                Direction::Up,
                &[
                    "0 src/code_view.rs::FactsView::file",
                    "1 src/code_view.rs::CodeView::describe frontier \
                     src/code_view.rs::FactsView::file src/snapshot.rs::Snapshot::file",
                    "src/code_view.rs::CodeView::describe -inferred-> \
                     src/code_view.rs::FactsView::file",
                    "hidden [\"tests/describe.rs::describes\"]",
                ],
            ),
            (
                "the caller of the implementation in another file, and its hidden test caller",
                Tests::Exclude,
                "src/snapshot.rs::Snapshot::file",
                Direction::Up,
                &[
                    "0 src/snapshot.rs::Snapshot::file",
                    "1 src/code_view.rs::CodeView::describe frontier \
                     src/code_view.rs::FactsView::file src/snapshot.rs::Snapshot::file",
                    "src/code_view.rs::CodeView::describe -inferred-> \
                     src/snapshot.rs::Snapshot::file",
                    "hidden [\"tests/describe.rs::describes\"]",
                ],
            ),
            (
                "no caller of another type's method of the leaf",
                Tests::Exclude,
                "src/store.rs::Store::file",
                Direction::Up,
                &["0 src/store.rs::Store::file", "hidden []"],
            ),
            (
                "a shown test call's callees: the test implementation beside both",
                Tests::Include,
                "tests/describe.rs::describes",
                Direction::Down,
                &[
                    "0 tests/describe.rs::describes",
                    "1 tests/describe.rs::%CodeView.file frontier \
                     src/code_view.rs::FactsView::file src/snapshot.rs::Snapshot::file \
                     tests/describe.rs::Fake::file",
                    "tests/describe.rs::describes -inferred-> tests/describe.rs::%CodeView.file",
                    "hidden []",
                ],
            ),
            (
                "a product call's callees with tests hidden: its traits' methods",
                Tests::Exclude,
                "src/report.rs::report",
                Direction::Down,
                &[
                    "0 src/report.rs::report",
                    "1 src/report.rs::%Cube.size frontier \
                     src/shape.rs::Area::size src/shape.rs::Volume::size",
                    "src/report.rs::report -inferred-> src/report.rs::%Cube.size",
                    "hidden []",
                ],
            ),
            (
                "a product call's callees with tests shown: the test type's own method beside \
                 its traits' methods, never in their place",
                Tests::Include,
                "src/report.rs::report",
                Direction::Down,
                &[
                    "0 src/report.rs::report",
                    "1 src/report.rs::%Cube.size frontier \
                     src/shape.rs::Area::size src/shape.rs::Volume::size \
                     tests/cube.rs::Cube::size",
                    "src/report.rs::report -inferred-> src/report.rs::%Cube.size",
                    "hidden []",
                ],
            ),
            (
                "a trait method's product caller with tests hidden",
                Tests::Exclude,
                "src/shape.rs::Area::size",
                Direction::Up,
                &[
                    "0 src/shape.rs::Area::size",
                    "1 src/report.rs::report frontier \
                     src/shape.rs::Area::size src/shape.rs::Volume::size",
                    "src/report.rs::report -inferred-> src/shape.rs::Area::size",
                    "hidden []",
                ],
            ),
            (
                "a trait method's product caller with tests shown, under the frontier holding \
                 the test type's own method too",
                Tests::Include,
                "src/shape.rs::Area::size",
                Direction::Up,
                &[
                    "0 src/shape.rs::Area::size",
                    "1 src/report.rs::report frontier \
                     src/shape.rs::Area::size src/shape.rs::Volume::size \
                     tests/cube.rs::Cube::size",
                    "src/report.rs::report -inferred-> src/shape.rs::Area::size",
                    "hidden []",
                ],
            ),
        ];
        for (shows, tests, seed, direction, want) in walks {
            let filter = ViewFilter { tests };
            let whole = Composed::new(&records, &view, filter, honesty.clone()).unwrap();
            assert_eq!(walk(&whole, seed, direction), want, "whole: {shows}");
            let near = Composed::near(&records, &view, filter, honesty.clone(), None).unwrap();
            assert_eq!(walk(&near, seed, direction), want, "near: {shows}");
        }
        // (the tests shown, a node, the candidates its down-hop keeps and its `candidates`
        // attribute carries, none for a node that is no qualified placeholder)
        let product = ["src/shape.rs::Area::size", "src/shape.rs::Volume::size"];
        let implementations = [
            "src/code_view.rs::FactsView::file",
            "src/snapshot.rs::Snapshot::file",
        ];
        let carried: [(Tests, &str, Option<Vec<&str>>); 5] = [
            (
                Tests::Exclude,
                "src/report.rs::%Cube.size",
                Some(product.to_vec()),
            ),
            (
                Tests::Include,
                "src/report.rs::%Cube.size",
                Some([&product[..], &["tests/cube.rs::Cube::size"]].concat()),
            ),
            (
                Tests::Exclude,
                "src/code_view.rs::%CodeView.file",
                Some(implementations.to_vec()),
            ),
            (
                Tests::Include,
                "tests/describe.rs::%CodeView.file",
                Some([&implementations[..], &["tests/describe.rs::Fake::file"]].concat()),
            ),
            (Tests::Include, "src/shape.rs::Area::size", None),
        ];
        for (tests, id, want) in carried {
            let graph = CodeGraph::new(&view, ViewFilter { tests });
            let want: Option<Vec<String>> = want.map(|w| w.iter().map(|c| c.to_string()).collect());
            assert_eq!(graph.down_hop(id), want, "{tests:?} {id}");
            let whole = graph.whole();
            let node = whole.nodes.iter().find(|n| n.id == id).unwrap();
            let attr = node.attrs.get("candidates").cloned();
            let json = want.map(|w| serde_json::to_string(&w).unwrap());
            assert_eq!(attr, json, "{tests:?} {id}");
        }
    }

    /// AN UNQUALIFIED FIELD USE NEVER PREFERS ITS FILE (spec 117, RESOLVER AT READ): a `field`
    /// reference the unqualified rule takes - written unqualified (`x.lang`), or behind a member
    /// or type-path qualifier that keeps no candidate - reads the field and property definitions
    /// of its leaf alone and skips the same-file step: exactly one in the view is INFERRED,
    /// wherever it sits, and several or none give no edge. So `x.lang` beside a same-file `lang`
    /// field and one in another file lands nowhere, beside one in another file lands on it
    /// INFERRED, and beside one in its own file alone lands on that INFERRED too; a constant, a
    /// static or a variant of the leaf neither takes it nor contests the field, nor does a test
    /// field or another language's; a property takes it like a field. A qualifier that keeps a
    /// constant still links it, EXTRACTED: C#'s `Square.SIDES`.
    #[test]
    fn an_unqualified_field_use_never_prefers_its_file() {
        // `describe` reads `mention`, beside `Config`'s own `lang` field and the type `Opts`.
        let config = |mention: Mention| {
            code(
                "src/config.rs",
                "rust",
                Scope::Product,
                vec![
                    def("Config", "type", 1, None, false),
                    def("Config::lang", "field", 2, Some(0), false),
                    def("Opts", "type", 4, None, false),
                    def("describe", "function", 5, None, false),
                ],
                vec![mention],
            )
        };
        let unqualified = || tagged("field", "lang", Some(3), 6);
        let type_path = || {
            let field = MentionKind::Tagged("field".into());
            qualified(field, "Opts::lang", Some(3), 6, false)
        };
        let config_contains = [
            "src/config.rs -CONTAINS/extracted-> src/config.rs::Config",
            "src/config.rs::Config -CONTAINS/extracted-> src/config.rs::Config::lang",
            "src/config.rs -CONTAINS/extracted-> src/config.rs::Opts",
            "src/config.rs -CONTAINS/extracted-> src/config.rs::describe",
        ];
        let user = || {
            code(
                "src/user.rs",
                "rust",
                Scope::Product,
                vec![def("render", "function", 1, None, false)],
                vec![tagged("field", "lang", Some(0), 2)],
            )
        };
        let locale = || {
            code(
                "src/locale.rs",
                "rust",
                Scope::Product,
                vec![
                    def("Locale", "type", 1, None, false),
                    def("Locale::lang", "field", 2, Some(0), false),
                ],
                vec![],
            )
        };
        let fake_locale = || {
            code(
                "tests/locale.rs",
                "rust",
                Scope::Test,
                vec![
                    def("FakeLocale", "type", 1, None, true),
                    def("FakeLocale::lang", "field", 2, Some(0), true),
                ],
                vec![],
            )
        };
        let tags = || {
            code(
                "src/tags.rs",
                "rust",
                Scope::Product,
                vec![
                    def("lang", "constant", 1, None, false),
                    def("Tag", "type", 3, None, false),
                    def("Tag::lang", "variant", 4, Some(1), false),
                ],
                vec![],
            )
        };
        let cache = || {
            code(
                "src/cache.rs",
                "rust",
                Scope::Product,
                vec![def("lang", "static", 1, None, false)],
                vec![],
            )
        };
        // `Handle` reads `Request.lang`, whose qualifier names nothing, and `Square.SIDES`.
        let handler = |members: Vec<Def>| {
            let field = || MentionKind::Tagged("field".into());
            code(
                "src/handler.cs",
                "csharp",
                Scope::Product,
                [
                    vec![
                        def("Handler", "type", 1, None, false),
                        dotted("Handler.Handle", "method", 2, 0),
                    ],
                    members,
                ]
                .concat(),
                vec![
                    qualified(field(), "Request.lang", Some(1), 3, false),
                    qualified(field(), "Square.SIDES", Some(1), 4, false),
                ],
            )
        };
        let locale_cs = || {
            code(
                "src/locale.cs",
                "csharp",
                Scope::Product,
                vec![
                    def("Locale", "type", 1, None, false),
                    dotted("Locale.lang", "property", 2, 0),
                ],
                vec![],
            )
        };
        let square = || {
            code(
                "src/square.cs",
                "csharp",
                Scope::Product,
                vec![
                    def("Square", "type", 1, None, false),
                    dotted("Square.SIDES", "constant", 2, 0),
                ],
                vec![],
            )
        };
        let user_infers = [
            "src/user.rs::render -REFERENCES/inferred-> src/locale.rs::Locale::lang",
            "src/user.rs -CONTAINS/extracted-> src/user.rs::render",
        ];
        let user_contains = ["src/user.rs -CONTAINS/extracted-> src/user.rs::render"];
        let handler_contains = [
            "src/handler.cs -CONTAINS/extracted-> src/handler.cs::Handler",
            "src/handler.cs::Handler -CONTAINS/extracted-> src/handler.cs::Handler.Handle",
        ];
        let sides = "src/handler.cs::Handler.Handle -REFERENCES/extracted-> \
                     src/square.cs::Square.SIDES";
        // (what the answer shows, the view's files, the file asked, its whole answer)
        let cases: [(&str, Vec<FileFacts>, &str, Vec<&str>); 10] = [
            (
                "`x.lang` beside a same-file `lang` field and one in another file: no edge",
                vec![config(unqualified()), locale()],
                "src/config.rs",
                config_contains.to_vec(),
            ),
            (
                "`x.lang` beside one `lang` field in another file: INFERRED",
                vec![user(), locale()],
                "src/user.rs",
                user_infers.to_vec(),
            ),
            (
                "`x.lang` beside one `lang` field in its own file alone: INFERRED, not EXTRACTED",
                vec![config(unqualified())],
                "src/config.rs",
                [
                    &["src/config.rs::describe -REFERENCES/inferred-> \
                       src/config.rs::Config::lang"][..],
                    &config_contains[..],
                ]
                .concat(),
            ),
            (
                "a test field and another language's property contest no product field use",
                vec![user(), locale(), fake_locale(), locale_cs()],
                "src/user.rs",
                user_infers.to_vec(),
            ),
            (
                "a static of the leaf alone takes no field use",
                vec![user(), cache()],
                "src/user.rs",
                user_contains.to_vec(),
            ),
            (
                "a constant, a static and a variant of the leaf contest no field",
                vec![user(), locale(), tags(), cache()],
                "src/user.rs",
                user_infers.to_vec(),
            ),
            (
                "no definition of the leaf: no edge",
                vec![user()],
                "src/user.rs",
                user_contains.to_vec(),
            ),
            (
                "a type-path qualifier keeping none falls back to the field rule: `Opts::lang` \
                 beside a same-file `lang` field and one in another file, no edge",
                vec![config(type_path()), locale()],
                "src/config.rs",
                config_contains.to_vec(),
            ),
            (
                "a member qualifier keeping none falls back to the field rule: `Request.lang` \
                 beside a same-file field and a property in another file, no edge; a member \
                 qualifier keeping a constant links it, EXTRACTED",
                vec![
                    handler(vec![dotted("Handler.lang", "field", 6, 0)]),
                    locale_cs(),
                    square(),
                ],
                "src/handler.cs",
                [
                    &[sides][..],
                    &handler_contains[..],
                    &["src/handler.cs::Handler -CONTAINS/extracted-> \
                       src/handler.cs::Handler.lang"][..],
                ]
                .concat(),
            ),
            (
                "a member qualifier keeping none, beside one property in another file: INFERRED",
                vec![handler(vec![]), locale_cs(), square()],
                "src/handler.cs",
                [
                    &["src/handler.cs::Handler.Handle -REFERENCES/inferred-> \
                       src/locale.cs::Locale.lang"][..],
                    &[sides][..],
                    &handler_contains[..],
                ]
                .concat(),
            ),
        ];
        for (shows, files, asked, want) in cases {
            let view = FactsView::new(stamp(), files);
            let r = Resolver::new(&view);
            assert_eq!(
                answer(&view, &r, &Ask::Links(asked, false)),
                want,
                "{shows}"
            );
        }
    }

    /// ONE PATH IS ONE ENTITY (spec 119 criterion 10): of same-path twins of one kind the lower
    /// line settles the id wherever it sits among them, and of twins on one line the first.
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
                Ask::Links("tests/it.rs", true),
                &["tests/it.rs::it_works -CALLS/extracted-> tests/it.rs::compute"],
                &[],
            ),
            (
                "a contested test call lands inferred on the definition its proof credits",
                Ask::Links("src/a.rs", true),
                &["src/a.rs::tests::checks -CALLS/inferred-> src/a.rs::helper"],
                &["src/a.rs::tests::checks -CALLS/inferred-> src/a.rs::%helper"],
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
                "a design link's end by its path or its test entity",
                Ask::Intent("docs/d.md"),
                &[
                    "docs/d.md -SPECIFIES/extracted-> tests/it.rs test",
                    "docs/d.md -SPECIFIES/extracted-> src/a_tests.rs test",
                    "docs/d.md -SPECIFIES/extracted-> src/a.rs::tests::checks test",
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
        // Each row: the case, the declaration's candidates in both its lists, the code files, the
        // target.
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
                module_target(&d, true, |p| code_files.contains(&p)),
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

    /// A module declaration placed `as_root` as a crate root or a `mod.rs` file declares it and
    /// `as_child` as any other file does, `path_attr` when it carries a `#[path]` attribute.
    fn placed(
        name: &str,
        test: bool,
        as_root: &[&str],
        as_child: &[&str],
        path_attr: bool,
    ) -> ModuleDecl {
        ModuleDecl {
            as_child: as_child.iter().map(|c| c.to_string()).collect(),
            path_attr,
            ..decl(name, 0, test, as_root)
        }
    }

    /// A tree whose declarations place a module apart as a root and as a child, most with a code
    /// file at the first path of both lists, so the list read decides the file reached: crate
    /// roots, `#[path]` declarations in a root and in a child, files a root or a child reaches
    /// through a plain declaration, a file only an `as_child` list names, a `mod.rs` file, one
    /// file two roots reach and one a root and a deeper file reach, a test-only declaration, and
    /// a cycle no root reaches.
    pub(crate) fn module_root_fixture() -> FactsView {
        let file = |path: &str, modules: Vec<ModuleDecl>| FileFacts {
            modules,
            ..code(path, "rust", Scope::Product, vec![], vec![])
        };
        let both = |name: &str, paths: &[&str]| decl(name, 0, false, paths);
        let declaring = vec![
            file(
                "src/bin/tool.rs",
                vec![
                    placed(
                        "x",
                        false,
                        &["src/bin/x.rs", "src/bin/x/mod.rs"],
                        &["src/bin/tool/x.rs", "src/bin/tool/x/mod.rs"],
                        false,
                    ),
                    placed(
                        "tests",
                        true,
                        &["src/bin/tests.rs", "src/bin/tests/mod.rs"],
                        &["src/bin/tool/tests.rs", "src/bin/tool/tests/mod.rs"],
                        false,
                    ),
                ],
            ),
            file(
                "src/lib.rs",
                vec![
                    placed("y", false, &["src/p/y.rs"], &["src/p/y.rs"], true),
                    both("code", &["src/code.rs", "src/code/mod.rs"]),
                    both("a", &["src/a.rs", "src/a/mod.rs"]),
                    both("deep", &["src/deep.rs", "src/deep/mod.rs"]),
                    placed("m", false, &["src/m.rs"], &["src/m.rs"], true),
                ],
            ),
            file(
                "src/main.rs",
                vec![both("m", &["src/m.rs", "src/m/mod.rs"])],
            ),
            file(
                "src/p/y.rs",
                vec![placed(
                    "z",
                    false,
                    &["src/p/z.rs", "src/p/z/mod.rs"],
                    &["src/p/y/z.rs", "src/p/y/z/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/p/z.rs",
                vec![placed(
                    "w",
                    false,
                    &["src/p/w.rs", "src/p/w/mod.rs"],
                    &["src/p/z/w.rs", "src/p/z/w/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/code/mod.rs",
                vec![both(
                    "advance",
                    &["src/code/advance.rs", "src/code/advance/mod.rs"],
                )],
            ),
            file(
                "src/a.rs",
                vec![
                    placed(
                        "b",
                        false,
                        &["src/b.rs", "src/b/mod.rs"],
                        &["src/a/b.rs", "src/a/b/mod.rs"],
                        false,
                    ),
                    placed("alias", false, &["src/deep.rs"], &["src/deep.rs"], true),
                ],
            ),
            file(
                "src/a/b.rs",
                vec![
                    placed(
                        "c",
                        false,
                        &["src/a/c.rs", "src/a/c/mod.rs"],
                        &["src/a/b/c.rs", "src/a/b/c/mod.rs"],
                        false,
                    ),
                    placed("q", false, &["src/a/b/q.rs"], &["src/a/b/q.rs"], true),
                ],
            ),
            file(
                "src/a/b/q.rs",
                vec![placed(
                    "r",
                    false,
                    &["src/a/b/r.rs", "src/a/b/r/mod.rs"],
                    &["src/a/b/q/r.rs", "src/a/b/q/r/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/deep.rs",
                vec![placed(
                    "leaf",
                    false,
                    &["src/leaf.rs", "src/leaf/mod.rs"],
                    &["src/deep/leaf.rs", "src/deep/leaf/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/m.rs",
                vec![placed(
                    "n",
                    false,
                    &["src/n.rs", "src/n/mod.rs"],
                    &["src/m/n.rs", "src/m/n/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/bin/alpha.rs",
                vec![placed(
                    "shared",
                    false,
                    &["src/bin/shared.rs", "src/bin/shared/mod.rs"],
                    &["src/bin/alpha/shared.rs", "src/bin/alpha/shared/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/bin/beta.rs",
                vec![placed(
                    "shared",
                    false,
                    &["src/bin/shared.rs"],
                    &["src/bin/shared.rs"],
                    true,
                )],
            ),
            file(
                "src/bin/shared.rs",
                vec![placed(
                    "k",
                    false,
                    &["src/bin/k.rs", "src/bin/k/mod.rs"],
                    &["src/bin/shared/k.rs", "src/bin/shared/k/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/r.rs",
                vec![placed(
                    "s",
                    false,
                    &["src/s.rs", "src/s/mod.rs"],
                    &["src/r/s.rs", "src/r/s/mod.rs"],
                    false,
                )],
            ),
            file(
                "src/s.rs",
                vec![placed(
                    "r",
                    false,
                    &["src/r.rs", "src/r/mod.rs"],
                    &["src/s/r.rs", "src/s/r/mod.rs"],
                    false,
                )],
            ),
        ];
        // Each file reached that declares nothing, and, where a declaration's lists differ, the
        // file its other list would reach, the cycle's and `src/bin/alpha.rs`'s aside:
        // `src/p/y/z.rs` is `src/p/z.rs`'s.
        let declared = [
            "src/bin/x.rs",
            "src/bin/tool/x.rs",
            "src/bin/tests.rs",
            "src/bin/tool/tests.rs",
            "src/p/y/z.rs",
            "src/p/z/w.rs",
            "src/p/w.rs",
            "src/code/advance.rs",
            "src/b.rs",
            "src/a/b/c.rs",
            "src/a/c.rs",
            "src/a/b/r.rs",
            "src/a/b/q/r.rs",
            "src/deep/leaf.rs",
            "src/leaf.rs",
            "src/n.rs",
            "src/m/n.rs",
            "src/bin/shared/k.rs",
            "src/bin/k.rs",
        ];
        let leaves = declared.iter().map(|path| file(path, vec![]));
        FactsView::new(stamp(), declaring.into_iter().chain(leaves).collect())
    }

    /// A MODULE ROOT IS DECIDED AT READ: a declaration reaches the first code file of its
    /// `as_child` list when a declaration with no `#[path]` attribute reached its file first,
    /// walking down from the files no declaration names a depth at a time, each depth in path
    /// order, and of its `as_root` list otherwise. The module rule's edges and the test-module
    /// closure reach the same files.
    #[test]
    fn a_module_root_is_decided_at_read() {
        // Each row: the case, a declaring file of the fixture, and the file each of its
        // declarations reaches, in declaration order.
        type Case = (&'static str, &'static str, &'static [Option<&'static str>]);
        let cases: &[Case] = &[
            (
                "a file no declaration names is a root: a test-only declaration in it too",
                "src/bin/tool.rs",
                &[Some("src/bin/x.rs"), Some("src/bin/tests.rs")],
            ),
            (
                "the library root, its lists equal",
                "src/lib.rs",
                &[
                    Some("src/p/y.rs"),
                    Some("src/code/mod.rs"),
                    Some("src/a.rs"),
                    Some("src/deep.rs"),
                    Some("src/m.rs"),
                ],
            ),
            (
                "a file a `#[path]` declaration reaches is placed as a root",
                "src/p/y.rs",
                &[Some("src/p/z.rs")],
            ),
            (
                "a file a plain declaration reaches under one a `#[path]` declaration reaches",
                "src/p/z.rs",
                &[Some("src/p/z/w.rs")],
            ),
            (
                "a reached `mod.rs` file",
                "src/code/mod.rs",
                &[Some("src/code/advance.rs")],
            ),
            (
                "a file a root reaches through a plain declaration",
                "src/a.rs",
                &[Some("src/a/b.rs"), Some("src/deep.rs")],
            ),
            (
                "a file a root reaches plain and a deeper file earlier in path order reaches by \
                 `#[path]` takes the root's",
                "src/deep.rs",
                &[Some("src/deep/leaf.rs")],
            ),
            (
                "the second root reaching a file the first already reached",
                "src/main.rs",
                &[Some("src/m.rs")],
            ),
            (
                "two roots reaching one file: the first in path order, by `#[path]`, decides",
                "src/m.rs",
                &[Some("src/n.rs")],
            ),
            (
                "a binary root reaching a file plain",
                "src/bin/alpha.rs",
                &[Some("src/bin/shared.rs")],
            ),
            (
                "a later binary root reaching the same file by `#[path]`",
                "src/bin/beta.rs",
                &[Some("src/bin/shared.rs")],
            ),
            (
                "two roots reaching one file: the first in path order, plain, decides",
                "src/bin/shared.rs",
                &[Some("src/bin/shared/k.rs")],
            ),
            (
                "a cycle no root reaches starts at its first file in path order, as a root",
                "src/r.rs",
                &[Some("src/s.rs")],
            ),
            (
                "the rest of the cycle is reached plain, and no file is at its `as_child` path",
                "src/s.rs",
                &[None],
            ),
            (
                "a file only an `as_child` list names, reached plain from a child, is a child",
                "src/a/b.rs",
                &[Some("src/a/b/c.rs"), Some("src/a/b/q.rs")],
            ),
            (
                "a file a `#[path]` declaration in a child reaches is placed as a root",
                "src/a/b/q.rs",
                &[Some("src/a/b/r.rs")],
            ),
        ];
        let view = module_root_fixture();
        let r = Resolver::new(&view);
        for (case, file, want) in cases {
            let decls = view.modules(file).expect("a declaring file");
            let got: Vec<Option<&str>> = decls.iter().map(|d| r.module_target(file, d)).collect();
            assert_eq!(&got, want, "{case}: module_target");
            let mut links = answer(&view, &r, &Ask::Links(file, true));
            links.sort();
            let mut contains: Vec<String> = (want.iter().flatten())
                .map(|to| format!("{file} -{REL_CONTAINS}/{TIER_EXTRACTED}-> {to}"))
                .collect();
            contains.sort();
            assert_eq!(links, contains, "{case}: the module rule");
        }
        let declaring = view.files().filter(|f| f.modules).count();
        assert_eq!(cases.len(), declaring, "a row for each declaring file");
        assert_eq!(
            answer(&view, &r, &Ask::TestFiles),
            ["src/bin/tests.rs"],
            "the test-module closure"
        );
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

    #[test]
    fn a_module_edge_into_a_test_file_is_test_code() {
        // Each file: its path, its scope, and its module declarations as (test mark, target).
        type File = (&'static str, Scope, &'static [(bool, &'static str)]);
        // Each row: the case, the view's files, the file asked, and its module edges under
        // exclude, then under include, each followed by its test flag.
        type Case = (
            &'static str,
            &'static [File],
            &'static str,
            &'static [&'static str],
            &'static [&'static str],
        );
        let cases: &[Case] = &[
            (
                "a product declaration of a product file is product",
                &[
                    ("src/lib.rs", Scope::Product, &[(false, "src/m.rs")]),
                    ("src/m.rs", Scope::Product, &[]),
                ],
                "src/lib.rs",
                &["src/lib.rs -CONTAINS-> src/m.rs product"],
                &["src/lib.rs -CONTAINS-> src/m.rs product"],
            ),
            (
                "a product declaration of a Test-scope file is test",
                &[
                    (
                        "src/lib.rs",
                        Scope::Product,
                        &[(false, "src/examples/mod.rs")],
                    ),
                    ("src/examples/mod.rs", Scope::Test, &[]),
                ],
                "src/lib.rs",
                &[],
                &["src/lib.rs -CONTAINS-> src/examples/mod.rs test"],
            ),
            (
                "a declaration in a test file is test",
                &[
                    ("src/lib.rs", Scope::Product, &[(false, "tests/common.rs")]),
                    ("tests/common.rs", Scope::Test, &[(false, "src/c.rs")]),
                    ("src/c.rs", Scope::Product, &[]),
                ],
                "tests/common.rs",
                &[],
                &["tests/common.rs -CONTAINS-> src/c.rs test"],
            ),
            (
                "a test-only declaration is test",
                &[
                    ("src/lib.rs", Scope::Product, &[(true, "src/t.rs")]),
                    ("src/t.rs", Scope::Product, &[]),
                ],
                "src/lib.rs",
                &[],
                &["src/lib.rs -CONTAINS-> src/t.rs test"],
            ),
        ];
        for (name, files, asked, exclude, include) in cases {
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
            for (tests, want) in [(false, exclude), (true, include)] {
                let got: Vec<String> = r
                    .links(&view.file(asked).unwrap(), tests)
                    .iter()
                    .map(|l| {
                        let flag = if l.test { "test" } else { "product" };
                        format!("{} -{}-> {} {flag}", l.from, l.rel, l.to)
                    })
                    .collect();
                assert_eq!(&got, want, "{name}, tests {tests}");
            }
        }
    }
}
