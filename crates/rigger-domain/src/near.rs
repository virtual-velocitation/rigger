//! The code half resolved near the ids a reader asks about (section 6.3 of the code store
//! addendum): a neighbourhood answer reads the facts that can assert something about its ids,
//! never the whole tree.
//!
//! Resolving a view whole folds what every file asserts ([`crate::code_graph`]). What a file
//! asserts about an id is fixed by the rules: a code id is asserted by its own file, by the files
//! holding a tagged reference to one of its leaves, by the files declaring it as a module, and by
//! the design and workflow facts that name it. So an id's node and edges are the whole graph's
//! when exactly those assertions are folded, in file order, under the same filter; its proofs are
//! the test calls of its leaves; and its groupings are read from the view's groupings by id. Each
//! file's assertions are read once and kept for the answer.

use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::code_facts::{Half, MentionKind};
use crate::code_graph::{Asserts, Builder, CodeGraph};
use crate::code_view::{Cluster, CodeView, FileRef, Tests, ViewFilter};
use crate::composed::{binds_leaf, id_binds_leaf, joined, ordered, CodeHalf, Indexed};
use crate::contextgraph::query::{entity_id, member_file, placeholder_id, split_id};
use crate::contextgraph::{
    holds_attachment, Edge, Error, Graph, Neighbors, Node, KIND_ARTIFACT, KIND_CODE_ENTITY,
    KIND_COMMUNITY, KIND_CONCEPT, KIND_FILE, PLACEHOLDER_SIGIL, REL_IN_COMMUNITY, REL_REALIZES,
    REL_TESTS, TIER_INFERRED,
};
use crate::resolve::{ProofCall, INTENT_KINDS};
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
    /// The files holding a tagged reference to one of the id's leaves.
    tagged: bool,
    /// The files declaring the id as a module.
    modules: bool,
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

/// What a neighbourhood has read so far.
struct State<'v> {
    view: &'v dyn CodeView,
    graph: CodeGraph<'v>,
    tests: Tests,
    files: Vec<FileRef<'v>>,
    places: FxHashMap<&'v str, u32>,
    intent: Option<Intent>,
    code: FxHashMap<u32, Arc<Fragment>>,
    /// Each file a module declaration reaches, with the places of the files declaring it.
    modules: Option<FxHashMap<String, Vec<u32>>>,
    proofs: FxHashMap<u32, Arc<Vec<ProofCall>>>,
    clusters: Option<Arc<Clusters<'v>>>,
    nodes: FxHashMap<String, Option<Node>>,
    outs: FxHashMap<String, Arc<Vec<Edge>>>,
    ins: FxHashMap<String, Arc<Vec<Edge>>>,
    /// Each (arm, test entity)'s cluster place, the plurality rule's winner.
    attach: FxHashMap<(usize, String), Option<usize>>,
    /// Each (arm, cluster id)'s attached test entities, in id order.
    attached: FxHashMap<(usize, String), Arc<Vec<String>>>,
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
                code: FxHashMap::default(),
                modules: None,
                proofs: FxHashMap::default(),
                clusters: None,
                nodes: FxHashMap::default(),
                outs: FxHashMap::default(),
                ins: FxHashMap::default(),
                attach: FxHashMap::default(),
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
                joined(resolved.graph, groupings, proofs, state.tests),
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
                let Some(facts) = self.view.file(f.path) else {
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

    /// The code assertions of the code file at `place`.
    fn code(&mut self, place: u32) -> Arc<Fragment> {
        if let Some(held) = self.code.get(&place) {
            return Arc::clone(held);
        }
        let mut fragment = Fragment::default();
        if let Some(facts) = self.view.file(self.files[place as usize].path) {
            let tests = self.tests != Tests::Exclude;
            self.graph.code(&facts, tests, &mut fragment, None);
        }
        let fragment = Arc::new(fragment);
        self.code.insert(place, Arc::clone(&fragment));
        fragment
    }

    /// The places of the code files declaring `path` as a module.
    fn declaring(&mut self, path: &str) -> Vec<u32> {
        if self.modules.is_none() {
            let mut modules: FxHashMap<String, Vec<u32>> = FxHashMap::default();
            for (place, f) in self.files.iter().enumerate() {
                if f.half != Half::Code || !f.modules {
                    continue;
                }
                let Some(facts) = self.view.file(f.path) else {
                    continue;
                };
                let resolver = self.graph.resolver();
                for decl in &facts.modules {
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
    fn leaves_of(&self, id: &str) -> Vec<String> {
        let (file, Some(local)) = split_id(id) else {
            return Vec::new();
        };
        if self.code_place(file).is_none() {
            return Vec::new();
        }
        let Some(facts) = self.view.file(file) else {
            return Vec::new();
        };
        let mut leaves: Vec<String> = Vec::new();
        for d in facts.defs.iter().filter(|d| d.local == local) {
            if !leaves.contains(&d.leaf) {
                leaves.push(d.leaf.clone());
            }
        }
        leaves
    }

    /// The places of the code files holding a tagged reference to one of `leaves`.
    fn tagging(&self, leaves: &[String]) -> Vec<u32> {
        let mut places = Vec::new();
        for leaf in leaves {
            for m in self.view.mentions_named(leaf) {
                if m.file.half == Half::Code && matches!(m.kind, MentionKind::Tagged(_)) {
                    places.extend(self.code_place(m.file.path));
                }
            }
        }
        places
    }

    /// `id` resolved from the assertions `reach` folds, as the whole graph holds it.
    fn resolve(&mut self, id: &str, reach: Reach) -> Resolved {
        let mut sources: Vec<(u32, Part)> = Vec::new();
        let (file, rest) = split_id(id);
        let own = self.code_place(file);
        sources.extend(own.map(|at| (at, Part::Code)));
        let leaves = self.leaves_of(id);
        if reach.tagged {
            let tagging = self.tagging(&leaves);
            sources.extend(tagging.into_iter().map(|at| (at, Part::Code)));
        }
        if reach.modules && rest.is_none() && own.is_some() {
            let declaring = self.declaring(file);
            sources.extend(declaring.into_iter().map(|at| (at, Part::Code)));
        }
        let intent = self.intent().at.get(id).cloned().unwrap_or_default();
        sources.extend(intent.into_iter().map(|at| (at, Part::Intent)));
        sources.sort_unstable();
        sources.dedup();
        let mut b = Builder::default();
        for (place, part) in sources {
            match part {
                Part::Code => self.code(place).replay(id, &mut b),
                Part::Intent => {
                    let intent = self.intent.as_ref().expect("read above");
                    intent.fragments[&place].replay(id, &mut b);
                }
            }
        }
        if !leaves.is_empty() {
            let leaves: Vec<&str> = leaves.iter().map(String::as_str).collect();
            let evidence = self.graph.resolver().proof_evidence(id, &leaves);
            if !evidence.is_empty() {
                b.prove(id, &evidence);
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

    /// The code node `id`, without the groupings.
    fn code_node(&mut self, id: &str) -> Option<Node> {
        if let Some(held) = self.nodes.get(id) {
            return held.clone();
        }
        // A test file a product module declaration reaches is product code; shown alone, a node
        // is shown when an edge reaches it.
        let only = self.tests == Tests::Only;
        let test_file = split_id(id).1.is_none() && self.graph.resolver().is_test_file(id);
        let reach = Reach {
            tagged: only,
            modules: only || test_file,
        };
        let node = self.resolve(id, reach).node;
        self.nodes.insert(id.to_string(), node.clone());
        node
    }

    /// The code edges from `id`: only its own file and the intent facts assert them.
    fn code_out(&mut self, id: &str) -> Arc<Vec<Edge>> {
        if let Some(held) = self.outs.get(id) {
            return Arc::clone(held);
        }
        let reach = Reach {
            tagged: false,
            modules: false,
        };
        let out = Arc::new(self.resolve(id, reach).out);
        self.outs.insert(id.to_string(), Arc::clone(&out));
        out
    }

    /// The code edges into `id`.
    fn code_incoming(&mut self, id: &str) -> Arc<Vec<Edge>> {
        if let Some(held) = self.ins.get(id) {
            return Arc::clone(held);
        }
        let reach = Reach {
            tagged: true,
            modules: true,
        };
        let incoming = Arc::new(self.resolve(id, reach).incoming);
        self.ins.insert(id.to_string(), Arc::clone(&incoming));
        incoming
    }

    fn clusters(&mut self) -> Arc<Clusters<'v>> {
        let view = self.view;
        let clusters = self.clusters.get_or_insert_with(|| {
            let g = view.groupings();
            Arc::new(Clusters::new([&g.communities, &g.concepts]))
        });
        Arc::clone(clusters)
    }

    /// The proof calls of the test calls in the code file at `place`.
    fn proofs(&mut self, place: u32) -> Arc<Vec<ProofCall>> {
        if let Some(held) = self.proofs.get(&place) {
            return Arc::clone(held);
        }
        let calls = match self.view.file(self.files[place as usize].path) {
            Some(facts) => self.graph.resolver().file_proofs(&facts),
            None => Vec::new(),
        };
        let calls = Arc::new(calls);
        self.proofs.insert(place, Arc::clone(&calls));
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
        let won = match self.code_place(split_id(test).0) {
            Some(place) => {
                let calls = self.proofs(place);
                let targets: BTreeSet<&str> = (calls.iter())
                    .filter(|p| p.caller == test)
                    .map(|p| p.target.as_str())
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
        let mut files = BTreeSet::new();
        for m in &members {
            for leaf in self.leaves_of(m) {
                for mention in self.view.mentions_named(&leaf) {
                    let path = mention.file.path;
                    let test = mention.test || self.graph.resolver().is_test_file(path);
                    if mention.file.half == Half::Code && mention.kind == MentionKind::Call && test
                    {
                        files.extend(self.code_place(path));
                    }
                }
            }
        }
        let mut callers = BTreeSet::new();
        for place in files {
            for p in self.proofs(place).iter() {
                if members.contains(p.target.as_str()) {
                    callers.insert(p.caller.clone());
                }
            }
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
        let mut edges = self.code_out(id).to_vec();
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
        let mut edges = self.code_incoming(id).to_vec();
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
        let mut ids: Vec<String> = (self.code(place).touching.keys())
            .filter(|id| matches!(split_id(id), (f, Some(_)) if f == file))
            .cloned()
            .collect();
        ids.sort_unstable();
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
fn inferred(from: &str, rel: &str, to: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: TIER_INFERRED.to_string(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_facts::{FileFacts, ModuleDecl};
    use crate::code_view::{DefRef, FactsView, Groupings, MentionRef};
    use crate::composed::{Composed, Honesty, Whole};
    use crate::contextgraph::{ContextGraph, Direction, TIER_AMBIGUOUS};
    use crate::resolve::tests::{fixture, stamp};
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
        for tests in FILTERS {
            let filter = ViewFilter { tests };
            let whole = Whole::new(&view, filter);
            let shared = Near::new(&view, filter);
            let one = |near: &Near, id: &str, when: &str| {
                let at = format!("{tests:?} {when} {id}");
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
                one(&Near::new(&view, filter), id, "fresh");
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
    /// subgraph, the walks both ways, the lookup of a definition and the hidden tests.
    #[test]
    fn composed_near_answers_every_search_as_composed_whole() {
        let view = view();
        let records = Indexed::default();
        let honesty = || Honesty {
            worktree: "wt".into(),
            diff: Ok(Default::default()),
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
        for tests in FILTERS {
            let filter = ViewFilter { tests };
            let whole = Composed::new(&records, &view, filter, honesty()).unwrap();
            let near = Composed::near(&records, &view, filter, honesty()).unwrap();
            for seed in seeds {
                let seed = [seed.to_string()];
                for depth in [1, 2, 3] {
                    let at = format!("{tests:?} {seed:?} {depth}");
                    let (got, want) = (near.subgraph(&seed, depth), whole.subgraph(&seed, depth));
                    same!(got.unwrap(), want.unwrap(), "subgraph {at}");
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
            same!(near.stamp_label(), whole.stamp_label());
        }
    }

    /// A view that records every file, definition and mention read of it.
    struct Counting {
        view: FactsView,
        reads: Mutex<Vec<String>>,
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
    }

    /// Each row: an id, and every file, definition and mention a read of its node and edges
    /// makes once the view is open. A definition reads its own file, the design and workflow
    /// files, the files holding a tagged reference to its leaf, and the mentions of that leaf
    /// alone; a file read resolves every mention it holds, so the definitions read are those of
    /// the leaves its own file and the tagging files name. A file reads itself, the design files
    /// and the module declarations.
    #[test]
    fn a_neighbourhood_reads_one_ids_file_and_its_leaf_only() {
        let intent = [
            "file .rigger/workflow.yml",
            "file docs/d.md",
            "file tests/it.rs",
        ];
        let cases: &[(&str, &[&str])] = &[
            (
                "src/c.rs::Widget",
                &[
                    // src/b.rs tags Widget, and a mentioning file resolves as one unit.
                    "defs Shape",
                    "defs Widget",
                    "file src/b.rs",
                    "file src/c.rs",
                    "mentions Widget",
                ],
            ),
            (
                "src/a.rs::run",
                &[
                    "defs Shape",
                    "defs compute",
                    "defs draw",
                    "defs helper",
                    "defs run",
                    "defs vanish",
                    "file src/a.rs",
                    "mentions run",
                ],
            ),
            (
                "src/sub.rs",
                &[
                    // src/a.rs declares the module, and resolves as one unit.
                    "defs Shape",
                    "defs compute",
                    "defs draw",
                    "defs helper",
                    "defs vanish",
                    "file src/a.rs",
                    "file src/a_tests.rs",
                    "file src/b.rs",
                    "file src/sub.rs",
                ],
            ),
        ];
        for (id, want) in cases {
            let view = Counting {
                view: view(),
                reads: Mutex::new(Vec::new()),
            };
            let near = Near::new(&view, ViewFilter::default());
            view.taken();
            near.node(id).unwrap();
            near.out(id).unwrap();
            near.incoming(id).unwrap();
            let mut got: Vec<String> = view.taken();
            got.sort();
            got.dedup();
            let mut want: Vec<String> = want.iter().map(|r| r.to_string()).collect();
            want.extend(intent.map(String::from));
            want.sort();
            assert_eq!(got, want, "{id}");
        }
    }
}
