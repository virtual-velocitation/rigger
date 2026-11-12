//! The code half resolved near the ids a reader asks about (section 6.3 of the code store
//! addendum): a neighbourhood answer reads the facts that can assert something about its ids,
//! never the whole tree.
//!
//! Resolving a view whole folds what every file asserts ([`crate::code_graph`]). What a file
//! asserts about an id is fixed by the rules: a code id is asserted by its own file, by the files
//! whose mentions of one of its leaves land on it from elsewhere (a tagged reference, and with
//! tests shown a test call), by the files declaring it as a module, and by the design and workflow
//! facts that name it. So an id's node and edges are the whole graph's when exactly those
//! assertions are folded, in file order, under the same filter; its proofs are the test calls of
//! its leaves; and its groupings are read from the view's groupings by id. Each file's assertions
//! are read once and kept for the answer.

use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::code_facts::{FileFacts, Half, MentionKind};
use crate::code_graph::{Asserts, Builder, CodeGraph};
use crate::code_view::{Cluster, CodeView, FileRef, Groupings, Tests, ViewFilter};
use crate::composed::{
    attachments, binds_leaf, cluster_files, id_binds_leaf, joined, ordered, provers_of, CodeHalf,
    Indexed,
};
use crate::contextgraph::query::{entity_id, leaf_of, member_file, placeholder_id, split_id};
use crate::contextgraph::{
    holds_attachment, Edge, Error, Graph, Neighbors, Node, KIND_ARTIFACT, KIND_CODE_ENTITY,
    KIND_COMMUNITY, KIND_CONCEPT, KIND_FILE, PLACEHOLDER_SIGIL, REL_IN_COMMUNITY, REL_REALIZES,
    REL_TESTS, TIER_INFERRED,
};
use crate::resolve::{def_leaves, ProofCall, INTENT_KINDS};
use crate::test_view::TESTS_ATTR;

/// The groupings' two arms, in the order they are joined: each cluster's node kind and the
/// relation a member holds to it.
const ARMS: [(&str, &str); 2] = [
    (KIND_COMMUNITY, REL_IN_COMMUNITY),
    (KIND_CONCEPT, REL_REALIZES),
];

/// The concepts' arm.
const CONCEPTS: usize = 1;

/// One assertion a file makes.
enum Said {
    Node {
        id: String,
        kind: &'static str,
        attrs: Vec<(&'static str, String)>,
        test: bool,
    },
    Link {
        from: String,
        rel: &'static str,
        to: String,
        tier: &'static str,
        test: bool,
    },
}

/// What one file asserts, in the order asserted, with the assertions touching each id.
#[derive(Default)]
struct Fragment {
    said: Vec<Said>,
    touching: FxHashMap<String, Vec<u32>>,
}

impl Fragment {
    /// The last assertion said touches `id`.
    fn touch(&mut self, id: &str) {
        let at = self.said.len() as u32 - 1;
        match self.touching.get_mut(id) {
            Some(held) => held.push(at),
            None => {
                self.touching.insert(id.to_string(), vec![at]);
            }
        }
    }

    /// The assertions touching `id`, folded into `b` in the order said.
    fn replay(&self, id: &str, b: &mut Builder) {
        for &at in self.touching.get(id).into_iter().flatten() {
            match &self.said[at as usize] {
                Said::Node {
                    id,
                    kind,
                    attrs,
                    test,
                } => b.node(id, kind, attrs.iter().cloned(), *test),
                Said::Link {
                    from,
                    rel,
                    to,
                    tier,
                    test,
                } => Asserts::link(b, from, rel, to, tier, *test),
            }
        }
    }
}

impl Asserts for Fragment {
    fn node(
        &mut self,
        id: &str,
        kind: &'static str,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
        test: bool,
    ) {
        self.said.push(Said::Node {
            id: id.to_string(),
            kind,
            attrs: attrs.into_iter().collect(),
            test,
        });
        self.touch(id);
    }
    fn link(&mut self, from: &str, rel: &'static str, to: &str, tier: &'static str, test: bool) {
        self.said.push(Said::Link {
            from: from.to_string(),
            rel,
            to: to.to_string(),
            tier,
            test,
        });
        self.touch(from);
        if to != from {
            self.touch(to);
        }
    }
}

/// What a code file asserts of its definitions and declarations, with its mentions indexed by
/// the definition enclosing them and by leaf, as places in its mentions in file order.
struct Declared {
    fragment: Fragment,
    enclosed: FxHashMap<u32, Vec<u32>>,
    named: FxHashMap<String, Vec<u32>>,
}

/// The assertions touching one id, folded into a builder in the order said.
struct Touching<'a> {
    id: &'a str,
    b: &'a mut Builder,
}

impl Asserts for Touching<'_> {
    fn node(
        &mut self,
        id: &str,
        kind: &'static str,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
        test: bool,
    ) {
        if id == self.id {
            self.b.node(id, kind, attrs, test);
        }
    }
    fn link(&mut self, from: &str, rel: &'static str, to: &str, tier: &'static str, test: bool) {
        if from == self.id || to == self.id {
            Asserts::link(self.b, from, rel, to, tier, test);
        }
    }
}

/// Which of a file's assertions a source names: its code facts', or its design and workflow
/// facts'. A code file's code assertions come before its intent ones, as the whole graph reads
/// them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Code,
    Intent,
}

/// Which files' assertions a read folds.
#[derive(Clone, Copy)]
struct Reach {
    /// The files whose mentions of one of the id's leaves land on it from elsewhere
    /// ([`State::landing`]).
    landing: bool,
    /// The files declaring the id as a module.
    modules: bool,
}

impl Reach {
    /// This reach's place among the four.
    fn at(self) -> usize {
        usize::from(self.landing) * 2 + usize::from(self.modules)
    }
}

/// The view's groupings, indexed by member and by cluster id.
struct Clusters<'v> {
    arms: [&'v [Cluster]; 2],
    /// Each member's clusters, (arm, place), one per occurrence, in grouping order.
    membership: FxHashMap<&'v str, Vec<(usize, usize)>>,
    /// Each cluster id's clusters, (arm, place), in grouping order.
    ids: FxHashMap<&'v str, Vec<(usize, usize)>>,
}

impl<'v> Clusters<'v> {
    fn new(arms: [&'v [Cluster]; 2]) -> Self {
        let mut membership: FxHashMap<&str, Vec<(usize, usize)>> = FxHashMap::default();
        let mut ids: FxHashMap<&str, Vec<(usize, usize)>> = FxHashMap::default();
        for (arm, clusters) in arms.iter().enumerate() {
            for (at, c) in clusters.iter().enumerate() {
                ids.entry(&c.id).or_default().push((arm, at));
                for m in &c.members {
                    membership.entry(m).or_default().push((arm, at));
                }
            }
        }
        Clusters {
            arms,
            membership,
            ids,
        }
    }

    fn cluster(&self, (arm, at): (usize, usize)) -> &'v Cluster {
        &self.arms[arm][at]
    }

    /// The clusters with the id `id`, (arm, place), in grouping order.
    fn with_id(&self, id: &str) -> &[(usize, usize)] {
        self.ids.get(id).map_or(&[], Vec::as_slice)
    }
}

/// The design and workflow assertions of the view, read when the first id is resolved: few files
/// hold them, and any of them may name any id.
struct Intent {
    /// Each intent-holding file's place and assertions, in file order.
    fragments: FxHashMap<u32, Fragment>,
    /// The places whose intent assertions touch each id.
    at: FxHashMap<String, Vec<u32>>,
    /// The ids those assertions touch that bind each leaf.
    leaves: FxHashMap<String, BTreeSet<String>>,
}

/// An id's code node and edges as the whole graph holds them.
struct Resolved {
    node: Option<Node>,
    out: Vec<Edge>,
    incoming: Vec<Edge>,
}

/// The files a resolve folds, by place and part, in file order.
type Sources = Vec<(u32, Part)>;

/// An id's resolves: each distinct set of files folded, with what it resolved to, and the one
/// each reach reads ([`Reach::at`]).
#[derive(Default)]
struct Reads {
    folded: Vec<(Sources, Arc<Resolved>)>,
    reaches: [Option<Arc<Resolved>>; 4],
}

/// What a neighbourhood has read so far.
struct State<'v> {
    view: &'v dyn CodeView,
    graph: CodeGraph<'v>,
    tests: Tests,
    files: Vec<FileRef<'v>>,
    places: FxHashMap<&'v str, u32>,
    intent: Option<Intent>,
    /// The facts of each code file read so far, by place.
    facts: FxHashMap<u32, Arc<Cow<'v, FileFacts>>>,
    /// What each code file read so far asserts of its definitions and declarations, and its
    /// mentions indexed, by place.
    declared: FxHashMap<u32, Arc<Declared>>,
    /// Each file a module declaration reaches, with the places of the files declaring it.
    modules: Option<FxHashMap<String, Vec<u32>>>,
    clusters: Option<Arc<Clusters<'v>>>,
    /// Each id resolved so far.
    resolved: FxHashMap<String, Reads>,
    /// Each (arm, test entity)'s cluster place, the plurality rule's winner.
    attach: FxHashMap<(usize, String), Option<usize>>,
    /// Each code file's proof calls, by place.
    proofs_in: FxHashMap<u32, Arc<Vec<ProofCall>>>,
    /// The test entities proving each id resolved so far, kept while the filter hides tests.
    provers: FxHashMap<String, Vec<String>>,
    /// Each (arm, cluster id)'s attached test entities, in id order.
    attached: FxHashMap<(usize, String), Arc<Vec<String>>>,
    /// Each (place, id) whose code assertions were read, in the order read.
    #[cfg(test)]
    read_code: Vec<(u32, String)>,
}

/// The code half of a view under one filter, resolved only near the ids it is asked about. Its
/// answers are the whole graph's ([`crate::composed::Indexed`] over [`joined`]): the reads that
/// need every node (`of_kind`, `whole`) resolve the view whole once.
pub(crate) struct Near<'v> {
    state: Mutex<State<'v>>,
    whole: OnceLock<Indexed>,
}

impl<'v> Near<'v> {
    pub(crate) fn new(view: &'v dyn CodeView, filter: ViewFilter) -> Self {
        let files: Vec<FileRef<'v>> = view.files().collect();
        let places = (files.iter().enumerate())
            .map(|(at, f)| (f.path, at as u32))
            .collect();
        Near {
            state: Mutex::new(State {
                view,
                graph: CodeGraph::new(view, filter),
                tests: filter.tests,
                files,
                places,
                intent: None,
                facts: FxHashMap::default(),
                declared: FxHashMap::default(),
                modules: None,
                clusters: None,
                resolved: FxHashMap::default(),
                attach: FxHashMap::default(),
                proofs_in: FxHashMap::default(),
                provers: FxHashMap::default(),
                #[cfg(test)]
                read_code: Vec::new(),
                attached: FxHashMap::default(),
            }),
            whole: OnceLock::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State<'v>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The view resolved whole, with its groupings joined on: what [`Indexed`] reads the whole
    /// code half from.
    fn resolved_whole(&self) -> &Indexed {
        self.whole.get_or_init(|| {
            let state = self.state();
            let resolved = state.graph.resolved();
            let groupings = state.view.groupings();
            let proofs = state.graph.resolver().proof_calls();
            Indexed::new(
                joined(
                    resolved.graph,
                    groupings,
                    &attachments(proofs, groupings),
                    state.tests,
                ),
                Vec::new(),
            )
        })
    }
}

impl<'v> State<'v> {
    /// The place of the code file `path`.
    fn code_place(&self, path: &str) -> Option<u32> {
        let &at = self.places.get(path)?;
        (self.files[at as usize].half == Half::Code).then_some(at)
    }

    /// The design and workflow assertions of every file holding them.
    fn intent(&mut self) -> &Intent {
        if self.intent.is_none() {
            let mut fragments = FxHashMap::default();
            let mut at: FxHashMap<String, Vec<u32>> = FxHashMap::default();
            let mut leaves: FxHashMap<String, BTreeSet<String>> = FxHashMap::default();
            for (place, f) in self.files.iter().enumerate() {
                if !f.intent {
                    continue;
                }
                let Some(facts) = self.view.intent_facts(f.path) else {
                    continue;
                };
                let mut fragment = Fragment::default();
                self.graph.intent(&facts, &mut fragment);
                for id in fragment.touching.keys() {
                    at.entry(id.clone()).or_default().push(place as u32);
                    if let Some(rest) = split_id(id).1 {
                        for leaf in [Some(rest), rest.strip_prefix(PLACEHOLDER_SIGIL)]
                            .into_iter()
                            .flatten()
                        {
                            let held = leaves.entry(leaf.to_string()).or_default();
                            held.insert(id.clone());
                        }
                    }
                }
                fragments.insert(place as u32, fragment);
            }
            self.intent = Some(Intent {
                fragments,
                at,
                leaves,
            });
        }
        self.intent.as_ref().expect("read above")
    }

    /// The facts of the code file at `place`.
    fn facts(&mut self, place: u32) -> Option<Arc<Cow<'v, FileFacts>>> {
        if let Some(held) = self.facts.get(&place) {
            return Some(Arc::clone(held));
        }
        let facts = Arc::new(self.view.file(self.files[place as usize].path)?);
        self.facts.insert(place, Arc::clone(&facts));
        Some(facts)
    }

    /// The file node, entities and declared edges of the code file at `place`
    /// ([`CodeGraph::code_nodes`], [`CodeGraph::code_declared`]), and its mentions indexed.
    fn declared(&mut self, place: u32, facts: &FileFacts) -> Arc<Declared> {
        if let Some(held) = self.declared.get(&place) {
            return Arc::clone(held);
        }
        let tests = self.tests != Tests::Exclude;
        let mut fragment = Fragment::default();
        self.graph.code_nodes(facts, tests, &mut fragment, None);
        self.graph.code_declared(facts, tests, &mut fragment);
        let mut enclosed: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
        let mut named: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        for (at, m) in facts.mentions.iter().enumerate() {
            if let Some(d) = m.enclosing {
                enclosed.entry(d).or_default().push(at as u32);
            }
            match named.get_mut(m.leaf.as_str()) {
                Some(held) => held.push(at as u32),
                None => {
                    named.insert(m.leaf.clone(), vec![at as u32]);
                }
            }
        }
        let declared = Arc::new(Declared {
            fragment,
            enclosed,
            named,
        });
        self.declared.insert(place, Arc::clone(&declared));
        declared
    }

    /// The code assertions touching `id` that the code file at `place` makes, folded into `b`:
    /// its definitions' and declarations', then its mentions' ([`CodeGraph::code`] asserts the
    /// mentions' edges between the two, but no mention's edge is a declaration's, so the fold is
    /// the same). Only the mentions that can touch `id` are resolved: in `id`'s own file, every
    /// mention when `id` is the file, else the mentions its definitions enclose; in any file, the
    /// mentions of `named`, the leaves a mention lands on `id` by.
    fn code(&mut self, place: u32, id: &str, named: &[&str], b: &mut Builder) {
        let Some(facts) = self.facts(place) else {
            return;
        };
        let declared = self.declared(place, &facts);
        declared.fragment.replay(id, b);
        let (file, rest) = split_id(id);
        let own = facts.path == file;
        let mut at: Vec<u32> = if own && rest.is_none() {
            (0..facts.mentions.len() as u32).collect()
        } else {
            let enclosing = (facts.defs.iter().enumerate())
                .filter(|(_, d)| own && Some(d.local.as_str()) == rest)
                .filter_map(|(d, _)| declared.enclosed.get(&(d as u32)));
            let of_leaves = named.iter().filter_map(|leaf| declared.named.get(*leaf));
            enclosing.chain(of_leaves).flatten().copied().collect()
        };
        at.sort_unstable();
        at.dedup();
        let mentions = at.iter().map(|&at| &facts.mentions[at as usize]);
        let tests = self.tests != Tests::Exclude;
        let mut touching = Touching { id, b };
        #[cfg(test)]
        self.read_code.push((place, id.to_string()));
        self.graph
            .code_mentions(&facts, mentions, tests, &mut touching);
    }

    /// The places of the code files declaring `path` as a module.
    fn declaring(&mut self, path: &str) -> Vec<u32> {
        if self.modules.is_none() {
            let mut modules: FxHashMap<String, Vec<u32>> = FxHashMap::default();
            for (place, f) in self.files.iter().enumerate() {
                if f.half != Half::Code || !f.modules {
                    continue;
                }
                let Some(declared) = self.view.modules(f.path) else {
                    continue;
                };
                let resolver = self.graph.resolver();
                for decl in declared.iter() {
                    if let Some(target) = resolver.module_target(decl) {
                        let held = modules.entry(target.to_string()).or_default();
                        if held.last() != Some(&(place as u32)) {
                            held.push(place as u32);
                        }
                    }
                }
            }
            self.modules = Some(modules);
        }
        let modules = self.modules.as_ref().expect("read above");
        modules.get(path).cloned().unwrap_or_default()
    }

    /// The leaves of the definitions settling the code entity `id`; none for any other id.
    fn leaves_of(&mut self, id: &str) -> Vec<String> {
        let (file, Some(local)) = split_id(id) else {
            return Vec::new();
        };
        let Some(facts) = self.code_place(file).and_then(|at| self.facts(at)) else {
            return Vec::new();
        };
        def_leaves(&facts, local)
    }

    /// The places of the code files whose mentions of one of `leaves` can land on a definition in
    /// another file: those holding a tagged reference to one, and, with tests shown, a test call of
    /// one, which lands on the definition its proof credits.
    fn landing(&self, leaves: &[String]) -> Vec<u32> {
        let tests = self.tests != Tests::Exclude;
        let resolver = self.graph.resolver();
        let mut places = Vec::new();
        for leaf in leaves {
            for m in self.view.mentions_named(leaf) {
                let lands = match m.kind {
                    MentionKind::Tagged(_) => true,
                    MentionKind::Call => tests && resolver.is_test(m.test, m.file.path),
                };
                if m.file.half == Half::Code && lands {
                    places.extend(self.code_place(m.file.path));
                }
            }
        }
        places
    }

    /// The files whose assertions `reach` folds for `id`, in file order, and the leaves of the
    /// definitions settling `id`.
    fn sources(&mut self, id: &str, reach: Reach) -> (Sources, Vec<String>) {
        let mut sources: Sources = Vec::new();
        let (file, rest) = split_id(id);
        let own = self.code_place(file);
        sources.extend(own.map(|at| (at, Part::Code)));
        let leaves = self.leaves_of(id);
        if reach.landing {
            let landing = self.landing(&leaves);
            sources.extend(landing.into_iter().map(|at| (at, Part::Code)));
        }
        if reach.modules && rest.is_none() && own.is_some() {
            let declaring = self.declaring(file);
            sources.extend(declaring.into_iter().map(|at| (at, Part::Code)));
        }
        let intent = self.intent().at.get(id).cloned().unwrap_or_default();
        sources.extend(intent.into_iter().map(|at| (at, Part::Intent)));
        sources.sort_unstable();
        sources.dedup();
        (sources, leaves)
    }

    /// `id` resolved from the assertions of `sources`, as the whole graph holds it.
    fn resolve(&mut self, id: &str, sources: &[(u32, Part)], leaves: &[String]) -> Resolved {
        let mut named: Vec<&str> = leaves.iter().map(String::as_str).collect();
        named.extend(leaf_of(id));
        let mut b = Builder::default();
        for &(place, part) in sources {
            match part {
                Part::Code => self.code(place, id, &named, &mut b),
                Part::Intent => {
                    let intent = self.intent.as_ref().expect("read above");
                    intent.fragments[&place].replay(id, &mut b);
                }
            }
        }
        if !leaves.is_empty() {
            let (evidence, provers): (Vec<String>, Vec<String>) =
                (self.graph.resolver().proofs_of(id))
                    .into_iter()
                    .map(|c| (c.evidence, c.caller))
                    .unzip();
            if !evidence.is_empty() {
                b.prove(id, &evidence);
            }
            if self.tests == Tests::Exclude && !provers.is_empty() {
                self.provers.insert(id.to_string(), provers);
            }
        }
        let Graph { nodes, edges } = b.into_graph(self.tests);
        let out = edges.iter().filter(|e| e.from == id).cloned().collect();
        let incoming = edges.into_iter().filter(|e| e.to == id).collect();
        Resolved {
            node: nodes.into_iter().find(|n| n.id == id),
            out,
            incoming,
        }
    }

    /// `id` resolved from the assertions `reach` folds: resolved once for each distinct set of
    /// files the reaches it is read at fold, and read once per reach.
    fn resolved(&mut self, id: &str, reach: Reach) -> Arc<Resolved> {
        if let Some(held) = self
            .resolved
            .get(id)
            .and_then(|r| r.reaches[reach.at()].as_ref())
        {
            return Arc::clone(held);
        }
        let (sources, leaves) = self.sources(id, reach);
        let folded = (self.resolved.get(id).into_iter())
            .flat_map(|r| &r.folded)
            .find(|(folded, _)| *folded == sources)
            .map(|(_, resolved)| Arc::clone(resolved));
        let resolved = match folded {
            Some(resolved) => resolved,
            None => {
                let resolved = Arc::new(self.resolve(id, &sources, &leaves));
                let held = self.resolved.entry(id.to_string()).or_default();
                held.folded.push((sources, Arc::clone(&resolved)));
                resolved
            }
        };
        let held = self.resolved.entry(id.to_string()).or_default();
        held.reaches[reach.at()] = Some(Arc::clone(&resolved));
        resolved
    }

    /// The code node `id`, without the groupings.
    fn code_node(&mut self, id: &str) -> Option<Node> {
        // A test file a product module declaration reaches is product code; shown alone, a node
        // is shown when an edge reaches it.
        let only = self.tests == Tests::Only;
        let test_file = split_id(id).1.is_none() && self.graph.resolver().is_test_file(id);
        let reach = Reach {
            landing: only,
            modules: only || test_file,
        };
        self.resolved(id, reach).node.clone()
    }

    /// The code edges from `id`: only its own file and the intent facts assert them, so the
    /// resolve that reads the node of a product id reads them.
    fn code_out(&mut self, id: &str) -> Arc<Resolved> {
        let reach = Reach {
            landing: false,
            modules: false,
        };
        self.resolved(id, reach)
    }

    /// The code edges into `id`.
    fn code_incoming(&mut self, id: &str) -> Arc<Resolved> {
        let reach = Reach {
            landing: true,
            modules: true,
        };
        self.resolved(id, reach)
    }

    fn clusters(&mut self) -> Arc<Clusters<'v>> {
        let view = self.view;
        let clusters = self.clusters.get_or_insert_with(|| {
            let g = view.groupings();
            Arc::new(Clusters::new([&g.communities, &g.concepts]))
        });
        Arc::clone(clusters)
    }

    /// The proof calls of the code file at `place`: the calls each test entity there makes that
    /// prove a definition.
    fn proofs_in(&mut self, place: u32) -> Arc<Vec<ProofCall>> {
        if let Some(held) = self.proofs_in.get(&place) {
            return Arc::clone(held);
        }
        let path = self.files[place as usize].path;
        let calls = Arc::new(self.graph.resolver().proofs_in(path));
        self.proofs_in.insert(place, Arc::clone(&calls));
        calls
    }

    /// The place in `arm` of the cluster the test entity `test` is attached to: the plurality of
    /// the distinct definitions it proves, a tie to the earliest cluster
    /// ([`crate::test_view::TestAttachment`]).
    fn attach(&mut self, arm: usize, test: &str) -> Option<usize> {
        let key = (arm, test.to_string());
        if let Some(&held) = self.attach.get(&key) {
            return held;
        }
        let (file, _) = split_id(test);
        let won = match self.code_place(file) {
            Some(place) => {
                let calls = self.proofs_in(place);
                let targets: BTreeSet<&str> = (calls.iter())
                    .filter(|c| c.caller == test)
                    .map(|c| c.target.as_str())
                    .collect();
                let clusters = self.clusters();
                let mut votes: std::collections::BTreeMap<usize, usize> = Default::default();
                for target in targets {
                    for &(a, at) in clusters.membership.get(target).into_iter().flatten() {
                        if a == arm {
                            *votes.entry(at).or_default() += 1;
                        }
                    }
                }
                votes
                    .into_iter()
                    .fold(None, |best: Option<(usize, usize)>, (at, n)| match best {
                        Some((_, most)) if most >= n => best,
                        _ => Some((at, n)),
                    })
                    .map(|(at, _)| at)
            }
            None => None,
        };
        self.attach.insert(key, won);
        won
    }

    /// The test entities attached to the clusters of `arm` with the id `id`, in id order.
    fn attached_to(&mut self, arm: usize, id: &str) -> Arc<Vec<String>> {
        let key = (arm, id.to_string());
        if let Some(held) = self.attached.get(&key) {
            return Arc::clone(held);
        }
        let clusters = self.clusters();
        let members: BTreeSet<&str> = (clusters.with_id(id).iter())
            .filter(|(a, _)| *a == arm)
            .flat_map(|&c| clusters.cluster(c).members.iter().map(String::as_str))
            .collect();
        // A test call proves one member: its caller is the definition enclosing it, or its file.
        let mut callers = BTreeSet::new();
        for m in &members {
            let calls = self.graph.resolver().proofs_of(m);
            callers.extend(calls.into_iter().map(|c| c.caller));
        }
        let mut tests = Vec::new();
        for t in callers {
            if self
                .attach(arm, &t)
                .is_some_and(|at| clusters.arms[arm][at].id == id)
            {
                tests.push(t);
            }
        }
        let tests = Arc::new(tests);
        self.attached.insert(key, Arc::clone(&tests));
        tests
    }

    /// The node `id`: the last cluster with that id, carrying its attached test count, else the
    /// code node.
    fn node(&mut self, id: &str) -> Option<Node> {
        let clusters = self.clusters();
        let Some(&(arm, at)) = clusters.with_id(id).last() else {
            return self.code_node(id);
        };
        let c = clusters.cluster((arm, at));
        let mut attrs: std::collections::BTreeMap<String, String> = [
            ("hash", c.hash.clone()),
            ("label", c.label.clone()),
            ("resolution", format!("{}", c.resolution)),
        ]
        .map(|(k, v)| (k.to_string(), v))
        .into();
        let count = self.attached_to(arm, id).len();
        if count > 0 {
            attrs.insert(TESTS_ATTR.to_string(), count.to_string());
        }
        Some(Node {
            id: id.to_string(),
            kind: ARMS[arm].0.to_string(),
            attrs,
        })
    }

    /// Whether the half holds a node `id`.
    fn holds(&mut self, id: &str) -> bool {
        !self.clusters().with_id(id).is_empty() || self.code_node(id).is_some()
    }

    /// The edges from `id`: its code edges, its memberships and its tests' attachments.
    fn out(&mut self, id: &str) -> Vec<Edge> {
        let mut edges = self.code_out(id).out.clone();
        if let Some(node) = self.code_node(id) {
            let clusters = self.clusters();
            let memberships = clusters.membership.get(id).map_or(&[][..], Vec::as_slice);
            for (arm, (_, rel)) in ARMS.into_iter().enumerate() {
                if holds_attachment(&node.kind, rel) {
                    for &(a, at) in memberships.iter().filter(|(a, _)| *a == arm) {
                        edges.push(inferred(id, rel, &clusters.cluster((a, at)).id));
                    }
                }
                if self.tests != Tests::Exclude {
                    if let Some(at) = self.attach(arm, id) {
                        edges.push(inferred(id, REL_TESTS, &clusters.arms[arm][at].id));
                    }
                }
            }
        }
        ordered(edges)
    }

    /// The edges into `id`: its code edges, and into a cluster its members' and its tests'.
    fn incoming(&mut self, id: &str) -> Vec<Edge> {
        let mut edges = self.code_incoming(id).incoming.clone();
        let clusters = self.clusters();
        for (arm, (_, rel)) in ARMS.into_iter().enumerate() {
            let with_id: Vec<(usize, usize)> = (clusters.with_id(id).iter())
                .filter(|(a, _)| *a == arm)
                .copied()
                .collect();
            if with_id.is_empty() {
                continue;
            }
            for c in with_id {
                for m in &clusters.cluster(c).members {
                    if self
                        .code_node(m)
                        .is_some_and(|n| holds_attachment(&n.kind, rel))
                    {
                        edges.push(inferred(m, rel, id));
                    }
                }
            }
            if self.tests != Tests::Exclude {
                for t in self.attached_to(arm, id).iter() {
                    if self.code_node(t).is_some() {
                        edges.push(inferred(t, REL_TESTS, id));
                    }
                }
            }
        }
        ordered(edges)
    }

    /// The named code entities of the file `file`, in id order.
    fn members(&mut self, file: &str) -> Vec<Node> {
        let Some(place) = self.code_place(file) else {
            return Vec::new();
        };
        let Some(facts) = self.facts(place) else {
            return Vec::new();
        };
        // A file's named entities are its definitions' ids and the placeholders of its calls.
        let mut ids: Vec<String> = (facts.defs.iter())
            .map(|d| entity_id(file, &d.local))
            .chain(
                (facts.mentions.iter())
                    .filter(|m| m.kind == MentionKind::Call)
                    .map(|m| placeholder_id(file, &m.leaf)),
            )
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut nodes = Vec::new();
        for id in ids {
            nodes.extend(self.node(&id).filter(|n| member_file(n) == Some(file)));
        }
        nodes
    }

    /// The nodes bound by `leaf` ([`binds_leaf`]), in id order: the definitions named `leaf`,
    /// the placeholders of calls to it, the entities whose id ends in it, and the design and
    /// workflow ids and cluster ids it binds.
    fn leaf_nodes(&mut self, leaf: &str) -> Vec<Node> {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        for d in self.view.defs_named(leaf) {
            if d.file.half == Half::Code {
                ids.insert(entity_id(d.file.path, d.local));
            }
        }
        // An entity's id rest is its local path, its parent chain then its leaf joined by `::`:
        // the definitions whose local is `leaf` (or `leaf` behind the sigil) are named by one of
        // its `::` suffixes.
        let sigiled = format!("{PLACEHOLDER_SIGIL}{leaf}");
        for local in [leaf, sigiled.as_str()] {
            let suffixes = local
                .match_indices("::")
                .map(|(at, sep)| &local[at + sep.len()..]);
            for name in std::iter::once(local).chain(suffixes) {
                for d in self.view.defs_named(name) {
                    if d.file.half == Half::Code && d.local == local {
                        ids.insert(entity_id(d.file.path, d.local));
                    }
                }
            }
        }
        // A placeholder binds the leaf it calls, and the leaf behind the sigil binds its id's rest.
        let called = [Some(leaf), leaf.strip_prefix(PLACEHOLDER_SIGIL)];
        for called in called.into_iter().flatten() {
            for m in self.view.mentions_named(called) {
                if m.file.half == Half::Code && m.kind == MentionKind::Call {
                    ids.insert(placeholder_id(m.file.path, called));
                }
            }
        }
        ids.extend(
            self.intent()
                .leaves
                .get(leaf)
                .into_iter()
                .flatten()
                .cloned(),
        );
        let clusters = self.clusters();
        ids.extend(
            (clusters.ids.keys())
                .filter(|id| id_binds_leaf(id, leaf))
                .map(|id| id.to_string()),
        );
        let mut nodes = Vec::new();
        for id in ids {
            nodes.extend(self.node(&id).filter(|n| binds_leaf(n, leaf)));
        }
        nodes
    }
}

/// The inferred edge `from` -`rel`-> `to` the groupings join on.
fn inferred(from: &str, rel: &'static str, to: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.into(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: TIER_INFERRED.into(),
    }
}

/// Whether a code half can hold a node of `kind`: a file, a code entity, an artifact, a design
/// or workflow node, or a cluster.
fn holds_kind(kind: &str) -> bool {
    [
        KIND_FILE,
        KIND_CODE_ENTITY,
        KIND_ARTIFACT,
        KIND_COMMUNITY,
        KIND_CONCEPT,
    ]
    .contains(&kind)
        || INTENT_KINDS.contains(&kind)
}

impl Neighbors for Near<'_> {
    // Once a read has resolved the view whole, every read answers from that whole.
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        match self.whole.get() {
            Some(whole) => whole.node(id),
            None => Ok(self.state().node(id)),
        }
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        match self.whole.get() {
            Some(whole) => whole.out(id),
            None => Ok(self.state().out(id)),
        }
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        match self.whole.get() {
            Some(whole) => whole.incoming(id),
            None => Ok(self.state().incoming(id)),
        }
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        match self.whole.get() {
            Some(whole) => whole.members(file),
            None => Ok(self.state().members(file)),
        }
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        match self.whole.get() {
            Some(whole) => whole.leaf_nodes(leaf),
            None => Ok(self.state().leaf_nodes(leaf)),
        }
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        if !holds_kind(kind) {
            return Ok(Vec::new());
        }
        self.resolved_whole().of_kind(kind)
    }
    /// A view records no reference rows, so it holds no unresolved leaf.
    fn unresolved(&self, _node: &Node) -> Result<Vec<String>, Error> {
        Ok(Vec::new())
    }
    fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
        Ok(Vec::new())
    }
    fn whole(&self) -> Result<Graph, Error> {
        self.resolved_whole().whole()
    }
}

impl CodeHalf for Near<'_> {
    fn groupings(&self) -> &Groupings {
        let view = self.state().view;
        view.groupings()
    }
    fn holds(&self, id: &str) -> bool {
        match self.whole.get() {
            Some(whole) => whole.holds(id),
            None => self.state().holds(id),
        }
    }
    fn concepts_of(&self, member: &str) -> Vec<String> {
        let mut state = self.state();
        if !state.holds(member) {
            return Vec::new();
        }
        let clusters = state.clusters();
        let memberships = clusters.membership.get(member).into_iter().flatten();
        memberships
            .filter(|(arm, _)| *arm == CONCEPTS)
            .map(|&c| clusters.cluster(c).id.clone())
            .collect()
    }
    fn concept_members(&self, concept: &str) -> Vec<String> {
        let mut state = self.state();
        let clusters = state.clusters();
        let mut members = Vec::new();
        for &c in clusters
            .with_id(concept)
            .iter()
            .filter(|(a, _)| *a == CONCEPTS)
        {
            for m in &clusters.cluster(c).members {
                if state.holds(m) {
                    members.push(m.clone());
                }
            }
        }
        members
    }
    fn hidden_tests(&self) -> Vec<Node> {
        self.state().graph.hidden_tests()
    }
    /// The view's definition leaves: every entity is named by its definition's leaf.
    fn entity_names(&self) -> Vec<String> {
        self.state().view.def_leaves()
    }
    fn hidden_tests_named(&self, name: &str) -> Vec<Node> {
        self.state().graph.hidden_tests_named(name)
    }
    /// Counted from the proofs read resolving `ids`, or, once the view is resolved whole, from
    /// the proof calls that read.
    fn hidden_provers(&self, ids: &[String]) -> usize {
        let state = self.state();
        if state.tests != Tests::Exclude {
            return 0;
        }
        match self.whole.get() {
            Some(_) => {
                let proofs = state.graph.resolver().proof_calls().iter();
                provers_of(proofs.map(|c| (c.target.as_str(), c.caller.as_str())), ids)
            }
            None => {
                let proofs = (state.provers.iter()).flat_map(|(id, provers)| {
                    provers.iter().map(move |p| (id.as_str(), p.as_str()))
                });
                provers_of(proofs, ids)
            }
        }
    }
    fn summarised_files(&self, id: &str) -> Vec<String> {
        let (members, tests) = {
            let mut state = self.state();
            let clusters = state.clusters();
            let held = clusters.with_id(id).to_vec();
            let members: Vec<String> = (held.iter())
                .flat_map(|&c| clusters.cluster(c).members.clone())
                .collect();
            let mut tests = Vec::new();
            for (arm, _) in held {
                tests.extend(state.attached_to(arm, id).iter().cloned());
            }
            (members, tests)
        };
        cluster_files(members.iter(), tests.iter().map(String::as_str), |m| {
            Neighbors::node(self, m).ok().flatten()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_facts::{FileFacts, ModuleDecl};
    use crate::code_view::{Compared, DefRef, FactsView, MentionRef, Proofs, Settled};
    use crate::composed::{Composed, Honesty, Whole};
    use crate::contextgraph::query::node_name;
    use crate::contextgraph::{ContextGraph, Direction, TIER_AMBIGUOUS};
    use crate::resolve::tests::{fixture, stamp};
    use crate::resolve::Resolver;
    use std::borrow::Cow;

    /// Asserts two answers are the same, by their debug text: nodes and edges are compared
    /// field by field.
    macro_rules! same {
        ($got:expr, $want:expr $(, $($at:tt)+)?) => {
            assert_eq!(format!("{:#?}", $got), format!("{:#?}", $want) $(, $($at)+)?)
        };
    }

    fn cluster(id: &str, label: &str, members: &[&str]) -> Cluster {
        Cluster {
            id: id.into(),
            label: label.into(),
            resolution: 1.0,
            hash: format!("h-{label}"),
            members: members.iter().map(|m| m.to_string()).collect(),
        }
    }

    /// The resolver's fixture, which holds a case of every rule, with a product module
    /// declaration reaching a test file and groupings over it: a community holding a member
    /// twice, a cluster id named twice, members of every kind (one a test entity, one missing),
    /// and test entities whose proofs vote for clusters, a tie among them.
    fn view() -> FactsView {
        let base = fixture();
        let mut files: Vec<FileFacts> = (base.files())
            .map(|f| base.file(f.path).expect("listed").into_owned())
            .collect();
        let b = files.iter_mut().find(|f| f.path == "src/b.rs").unwrap();
        b.modules.push(ModuleDecl {
            name: "it".into(),
            def: 0,
            test: false,
            candidates: vec!["tests/missing.rs".into(), "tests/it.rs".into()],
        });
        let groupings = Groupings {
            communities: vec![
                cluster(
                    "community/1/0",
                    "first",
                    &[
                        "src/a.rs::run",
                        "src/a.rs::helper",
                        "src/b.rs::compute",
                        "src/b.rs",
                        "docs/d.md",
                    ],
                ),
                cluster(
                    "community/1/1",
                    "second",
                    &[
                        "src/b.rs::draw",
                        "src/c.rs::draw",
                        "src/a.rs::run",
                        "src/a.rs::run",
                        "src/a.rs::tests::checks",
                        "src/a.rs::%draw",
                    ],
                ),
                cluster("community/1/2", "third", &["src/a.rs::helper"]),
                cluster("community/1/0", "again", &["scripts/t.py::assist"]),
            ],
            concepts: vec![
                cluster(
                    "concept/1/0",
                    "design",
                    &["docs/d.md", "src/b.rs", "src/a.rs::run", "stage/plan"],
                ),
                cluster(
                    "concept/1/1",
                    "drawing",
                    &["src/c.rs::draw", "src/b.rs::compute", "missing/x.rs::gone"],
                ),
            ],
        };
        FactsView::new(stamp(), files).with_groupings(groupings)
    }

    const FILTERS: [Tests; 3] = [Tests::Exclude, Tests::Include, Tests::Only];

    /// `view` holding what a store settles at advance as rows, as a resolver computes them over
    /// it: a read of it takes the rows and computes neither.
    struct Settles {
        view: FactsView,
        test_files: Vec<String>,
        proofs: Vec<ProofCall>,
    }

    impl Settles {
        fn of(view: FactsView) -> Self {
            let resolver = Resolver::new(&view);
            let test_files = (view.files())
                .filter(|f| resolver.is_test_file(f.path))
                .map(|f| f.path.to_string())
                .collect();
            let proofs = resolver.proof_calls().to_vec();
            assert!(!proofs.is_empty(), "the fixture holds proofs");
            drop(resolver);
            Settles {
                view,
                test_files,
                proofs,
            }
        }
    }

    impl CodeView for Settles {
        fn stamp(&self) -> &crate::code_facts::Stamp {
            self.view.stamp()
        }
        fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
            self.view.files()
        }
        fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
            self.view.file(path)
        }
        fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
            self.view.defs_named(leaf)
        }
        fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
            self.view.mentions_named(leaf)
        }
        fn groupings(&self) -> &Groupings {
            self.view.groupings()
        }
        fn settled(&self) -> Option<&dyn Settled> {
            Some(self)
        }
    }

    impl Settled for Settles {
        fn test_files(&self) -> Vec<&str> {
            self.test_files.iter().map(String::as_str).collect()
        }
        fn proofs(&self, of: Proofs<'_>) -> Vec<ProofCall> {
            (self.proofs.iter())
                .filter(|c| match of {
                    Proofs::All => true,
                    Proofs::Of(id) => c.target == id,
                    Proofs::In(path) => c.file() == Some(path),
                })
                .cloned()
                .collect()
        }
    }

    /// Every id worth asking about: the whole graph's nodes and edge ends under every filter, and
    /// ids no graph holds.
    fn ids(view: &FactsView) -> BTreeSet<String> {
        let mut ids: BTreeSet<String> = [
            "missing",
            "src/a.rs::%nothing",
            "community/1/9",
            "tests/missing.rs",
        ]
        .map(String::from)
        .into();
        for tests in FILTERS {
            let g = Whole::new(view, ViewFilter { tests }).whole().unwrap();
            ids.extend(g.nodes.into_iter().map(|n| n.id));
            for e in g.edges {
                ids.insert(e.from);
                ids.insert(e.to);
            }
        }
        ids
    }

    /// Every leaf an id or node of `ids` can be bound by.
    fn leaves(view: &FactsView, ids: &BTreeSet<String>) -> BTreeSet<String> {
        let mut leaves: BTreeSet<String> = ["nothing", "%draw", "Widget"].map(String::from).into();
        for id in ids {
            if let Some(rest) = split_id(id).1 {
                leaves.insert(rest.to_string());
                leaves.extend(rest.strip_prefix(PLACEHOLDER_SIGIL).map(String::from));
            }
        }
        for f in view.files() {
            for d in &view.file(f.path).unwrap().defs {
                leaves.insert(d.leaf.clone());
            }
        }
        leaves
    }

    /// THE NEIGHBOURHOOD IS THE WHOLE GRAPH, READ NEAR AN ID: under every filter, every read of
    /// every id the whole graph holds (and of ids it does not) answers what the whole graph,
    /// groupings joined on, answers. It covers every resolver rule row (calls, inferred and
    /// ambiguous placeholders, frontiers, tagged references, CONTAINS, module declarations,
    /// proofs, design and workflow links) and the groupings' memberships and test attachments.
    /// Each id is read once from a neighbourhood shared across reads and once from a fresh one,
    /// so no answer depends on what was read before it.
    #[test]
    fn a_neighbourhood_answers_what_the_whole_graph_answers_under_every_filter() {
        let view = view();
        let ids = ids(&view);
        let leaves = leaves(&view, &ids);
        let whole_graph = Whole::new(&view, ViewFilter::default()).whole().unwrap();
        let tiers: BTreeSet<&str> = whole_graph.edges.iter().map(|e| e.tier.as_str()).collect();
        assert!(tiers.contains(TIER_AMBIGUOUS) && tiers.contains(TIER_INFERRED));
        let settles = Settles::of(self::view());
        for (tests, (held, read)) in FILTERS.into_iter().flat_map(|t| {
            [
                (t, ("facts", &view as &dyn CodeView)),
                (t, ("settled", &settles)),
            ]
        }) {
            let filter = ViewFilter { tests };
            let whole = Whole::new(&view, filter);
            same!(
                Whole::new(read, filter).whole().unwrap(),
                whole.whole().unwrap()
            );
            let shared = Near::new(read, filter);
            let one = |near: &Near, id: &str, when: &str| {
                let at = format!("{tests:?} {held} {when} {id}");
                same!(near.node(id).unwrap(), whole.node(id).unwrap(), "node {at}");
                same!(near.out(id).unwrap(), whole.out(id).unwrap(), "out {at}");
                let (got, want) = (near.incoming(id).unwrap(), whole.incoming(id).unwrap());
                same!(got, want, "incoming {at}");
                let (got, want) = (near.members(id).unwrap(), whole.members(id).unwrap());
                same!(got, want, "members {at}");
                same!(near.holds(id), whole.holds(id), "holds {at}");
                same!(near.concepts_of(id), whole.concepts_of(id), "concepts {at}");
                let (got, want) = (near.concept_members(id), whole.concept_members(id));
                same!(got, want, "concept members {at}");
            };
            for id in &ids {
                one(&shared, id, "shared");
                one(&Near::new(read, filter), id, "fresh");
            }
            for leaf in &leaves {
                let (got, want) = (shared.leaf_nodes(leaf).unwrap(), whole.leaf_nodes(leaf));
                same!(got, want.unwrap(), "{tests:?} leaf {leaf}");
            }
            for kind in [
                KIND_CODE_ENTITY,
                KIND_FILE,
                KIND_COMMUNITY,
                "decision",
                "nothing",
            ] {
                let (got, want) = (shared.of_kind(kind).unwrap(), whole.of_kind(kind));
                same!(got, want.unwrap(), "{tests:?} kind {kind}");
            }
            same!(shared.whole().unwrap(), whole.whole().unwrap());
            same!(shared.hidden_tests(), whole.hidden_tests(), "{tests:?}");
            // Resolved whole by the reads above, the neighbourhood answers from the whole.
            for id in &ids {
                one(&shared, id, "settled");
            }
        }
    }

    /// Composed over a neighbourhood answers every search as composed over the whole graph: the
    /// subgraph, the walks both ways, the lookup of a definition, the hidden tests and the hidden
    /// tests proving what a subgraph shows.
    #[test]
    fn composed_near_answers_every_search_as_composed_whole() {
        let view = view();
        let records = Indexed::default();
        let honesty = || Honesty {
            worktree: "wt".into(),
            compared: crate::code_view::Compared::Overlaid(0),
            behind: None,
        };
        let seeds = [
            "src/a.rs::run",
            "src/a.rs",
            "src/b.rs::draw",
            "src/c.rs::Widget",
            "community/1/0",
            "docs/d.md",
            "tests/it.rs",
        ];
        let settles = Settles::of(self::view());
        for (tests, (held, read)) in FILTERS.into_iter().flat_map(|t| {
            [
                (t, ("facts", &view as &dyn CodeView)),
                (t, ("settled", &settles)),
            ]
        }) {
            let filter = ViewFilter { tests };
            let whole = Composed::new(&records, &view, filter, honesty()).unwrap();
            let near = Composed::near(&records, read, filter, honesty()).unwrap();
            for seed in seeds {
                let seed = [seed.to_string()];
                for depth in [1, 2, 3] {
                    let at = format!("{tests:?} {held} {seed:?} {depth}");
                    let (got, want) = (near.subgraph(&seed, depth), whole.subgraph(&seed, depth));
                    let (got, want) = (got.unwrap(), want.unwrap());
                    let shown: Vec<String> = want.nodes.iter().map(|n| n.id.clone()).collect();
                    same!(got, want, "subgraph {at}");
                    let (got, want) = (near.hidden_provers(&shown), whole.hidden_provers(&shown));
                    same!(got.unwrap(), want.unwrap(), "hidden provers {at}");
                    for direction in [Direction::Up, Direction::Down] {
                        for floor in [TIER_INFERRED, TIER_AMBIGUOUS] {
                            let got = near.calls(&seed, direction, depth, floor).unwrap();
                            let want = whole.calls(&seed, direction, depth, floor).unwrap();
                            same!(got, want, "calls {at}");
                        }
                    }
                }
            }
            for query in ["run", "draw", "src/b.rs::draw", "Widget", "nothing"] {
                let (got, want) = (near.locate(query), whole.locate(query));
                same!(got.unwrap(), want.unwrap(), "{tests:?} locate {query}");
            }
            let (got, want) = (near.hidden_tests(), whole.hidden_tests());
            same!(got.unwrap(), want.unwrap());
            // Every name an entity carries is listed, and each name reads the entities, shown
            // and hidden, the whole graph's named so.
            let (listed, named) = (near.entity_names().unwrap(), whole.entity_names().unwrap());
            let shown = whole.nodes_of_kind(KIND_CODE_ENTITY).unwrap();
            let hidden = whole.hidden_tests().unwrap();
            let carried = |n: &Node| node_name(n).map(str::to_string);
            for name in shown.iter().chain(&hidden).filter_map(carried) {
                assert!(
                    listed.contains(&name) && named.contains(&name),
                    "{tests:?} {name}"
                );
            }
            for name in listed
                .iter()
                .chain(&named)
                .map(String::as_str)
                .chain(["nothing"])
            {
                let want = |nodes: &[Node]| -> Vec<Node> {
                    let named = nodes.iter().filter(|n| node_name(n) == Some(name));
                    named.cloned().collect()
                };
                for graph in [&near as &dyn ContextGraph, &whole] {
                    let at = format!("{tests:?} named {name}");
                    same!(graph.entities_named(name).unwrap(), want(&shown), "{at}");
                    same!(
                        graph.hidden_tests_named(name).unwrap(),
                        want(&hidden),
                        "{at}"
                    );
                }
            }
            same!(near.stamp_label(), whole.stamp_label());
        }
    }

    /// HIDDEN PROVERS ARE COUNTED FROM THE PROOFS READ: under exclude, the distinct test entities
    /// proving the nodes asked about, a test proving two counted once; none when tests are shown
    /// or the worktree cannot be compared. The whole graph and a neighbourhood count alike, a
    /// neighbourhood from the proofs it read showing those nodes, reading nothing more of the
    /// view, before and after it is resolved whole.
    #[test]
    fn hidden_provers_are_the_distinct_tests_proving_the_nodes_shown_while_tests_are_hidden() {
        let records = Indexed::default();
        let honesty = |compared| Honesty {
            worktree: "wt".into(),
            compared,
            behind: None,
        };
        let (run, helper) = ("src/a.rs::run", "src/a.rs::helper");
        let (compared, uncomparable) = (Compared::Overlaid(0), Compared::Uncomparable);
        let cases: &[(Tests, Compared, &[&str], usize)] = &[
            // checks, checks_run and it_works call run; checks calls helper too.
            (Tests::Exclude, compared, &[run], 3),
            (Tests::Exclude, compared, &[helper], 1),
            (Tests::Exclude, compared, &[run, helper], 3),
            (
                Tests::Exclude,
                compared,
                &["src/b.rs::compute", "src/a.rs", "docs/d.md", "missing"],
                0,
            ),
            (Tests::Include, compared, &[run, helper], 0),
            (Tests::Only, compared, &[run, helper], 0),
            (Tests::Exclude, uncomparable, &[run, helper], 0),
        ];
        let settles = Settles::of(view());
        for &(tests, compared, ids, want) in cases {
            let filter = ViewFilter { tests };
            let ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
            let counting = Counting {
                view: view(),
                reads: Mutex::new(Vec::new()),
                settles: None,
            };
            for (held, read) in [("facts", &counting as &dyn CodeView), ("settled", &settles)] {
                let at = format!("{tests:?} {compared:?} {held} {ids:?}");
                let whole = Composed::new(&records, read, filter, honesty(compared)).unwrap();
                assert_eq!(whole.hidden_provers(&ids).unwrap(), want, "whole {at}");
                let near = Composed::near(&records, read, filter, honesty(compared)).unwrap();
                near.subgraph(&ids, 1).unwrap();
                counting.taken();
                assert_eq!(near.hidden_provers(&ids).unwrap(), want, "near {at}");
                assert_eq!(
                    counting.taken(),
                    Vec::<String>::new(),
                    "near reads more {at}"
                );
                near.whole().unwrap();
                counting.taken();
                assert_eq!(near.hidden_provers(&ids).unwrap(), want, "near whole {at}");
                assert_eq!(
                    counting.taken(),
                    Vec::<String>::new(),
                    "whole reads more {at}"
                );
            }
        }
    }

    /// A view that records every file, definition and mention read of it, holding settled rows
    /// when `settles` does.
    struct Counting {
        view: FactsView,
        reads: Mutex<Vec<String>>,
        settles: Option<Settles>,
    }

    impl Counting {
        fn taken(&self) -> Vec<String> {
            std::mem::take(&mut *self.reads.lock().unwrap())
        }
    }

    impl CodeView for Counting {
        fn stamp(&self) -> &crate::code_facts::Stamp {
            self.view.stamp()
        }
        fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
            self.view.files()
        }
        fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
            self.reads.lock().unwrap().push(format!("file {path}"));
            self.view.file(path)
        }
        fn intent_facts(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
            self.reads.lock().unwrap().push(format!("intent {path}"));
            self.view.intent_facts(path)
        }
        fn modules(&self, path: &str) -> Option<Cow<'_, [ModuleDecl]>> {
            self.reads.lock().unwrap().push(format!("modules {path}"));
            self.view.modules(path)
        }
        fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
            self.reads.lock().unwrap().push(format!("defs {leaf}"));
            self.view.defs_named(leaf)
        }
        fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
            self.reads.lock().unwrap().push(format!("mentions {leaf}"));
            self.view.mentions_named(leaf)
        }
        fn groupings(&self) -> &Groupings {
            self.view.groupings()
        }
        fn settled(&self) -> Option<&dyn Settled> {
            self.settles.as_ref().map(|s| s as &dyn Settled)
        }
    }

    /// Each row: an id, and every file, definition and mention a read of its node and edges
    /// makes, counted from before the resolver is built: building it reads the files declaring a
    /// module held elsewhere. A definition reads its own file, the design and workflow facts, the
    /// files holding a tagged reference to its leaf, and the mentions of that leaf; it resolves
    /// only the mentions it encloses and those of its leaf, so the definitions read are those of
    /// the leaves it names and its own. A file reads itself, the design files and the files
    /// declaring modules, and resolves the mentions it holds alone.
    #[test]
    fn a_neighbourhood_reads_one_ids_file_and_its_leaf_only() {
        let opened = [
            // The modules of the files declaring one held elsewhere, which the resolver reads.
            "modules src/a.rs",
            "modules src/a_tests.rs",
            "modules src/b.rs",
            // The design and workflow facts of the files holding them, which every read reads.
            "intent .rigger/workflow.yml",
            "intent docs/d.md",
            "intent tests/it.rs",
        ];
        let cases: &[(&str, &[&str])] = &[
            (
                "src/c.rs::Widget",
                &[
                    "defs Widget",
                    "file src/b.rs",
                    "file src/c.rs",
                    "mentions Widget",
                ],
            ),
            (
                "src/a.rs::run",
                &[
                    // run calls helper, compute and draw and tags Shape.
                    "defs Shape",
                    "defs compute",
                    "defs draw",
                    "defs helper",
                    "defs run",
                    "file src/a.rs",
                    // Tests there call run.
                    "file src/a_tests.rs",
                    "file tests/it.rs",
                    "mentions run",
                ],
            ),
            (
                "src/sub.rs",
                // The file declaring it, whose declaration's edge it folds, and itself.
                &["file src/a.rs", "file src/sub.rs"],
            ),
        ];
        for (id, want) in cases {
            let view = Counting {
                view: view(),
                reads: Mutex::new(Vec::new()),
                settles: None,
            };
            let near = Near::new(&view, ViewFilter::default());
            near.node(id).unwrap();
            near.out(id).unwrap();
            near.incoming(id).unwrap();
            let mut got: Vec<String> = view.taken();
            got.sort();
            got.dedup();
            let mut want: Vec<String> = want.iter().map(|r| r.to_string()).collect();
            want.extend(opened.map(String::from));
            want.sort();
            want.dedup();
            assert_eq!(got, want, "{id}");
        }
    }

    /// An id whose reaches fold the same files is resolved once: `run` is tagged nowhere and is no
    /// file a module declaration reaches, so its edges in fold the files its node and edges out
    /// fold, and reading them reads no file's assertions again.
    #[test]
    fn an_id_its_reaches_fold_alike_is_resolved_once() {
        let view = view();
        let id = "src/a.rs::run";
        let near = Near::new(&view, ViewFilter::default());
        near.node(id).unwrap();
        near.out(id).unwrap();
        let first = std::mem::take(&mut near.state().read_code);
        assert!(!first.is_empty(), "the first read reads its own file");
        near.incoming(id).unwrap();
        assert_eq!(near.state().read_code, Vec::new());
    }

    /// Over a view that settles its proofs, a cluster's node, which counts the tests attached to
    /// it, reads no file's facts: the settled rows name each member's proofs and each test's
    /// calls.
    #[test]
    fn a_settled_clusters_node_reads_no_files_facts() {
        let groupings = view().groupings().clone();
        let clusters = groupings.communities.iter().chain(&groupings.concepts);
        let mut counted = 0;
        for cluster in clusters {
            let view = Counting {
                view: view(),
                reads: Mutex::new(Vec::new()),
                settles: Some(Settles::of(view())),
            };
            let near = Near::new(&view, ViewFilter::default());
            view.taken();
            let node = near.node(&cluster.id).unwrap().expect("a cluster's node");
            counted += usize::from(node.attrs.contains_key(TESTS_ATTR));
            let files: Vec<String> = (view.taken().into_iter())
                .filter(|r| r.starts_with("file "))
                .collect();
            assert_eq!(files, Vec::<String>::new(), "{}", cluster.id);
        }
        assert!(counted > 0, "the fixture attaches tests to a cluster");
    }
}
