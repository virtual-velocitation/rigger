//! The test views: the `tests` filter's attachment of test entities to the communities and concepts
//! they prove. Groupings never cluster test code, so a test is shown beside the cluster that holds
//! the plurality of the product definitions it proves: as that cluster's "tests" group, with its
//! count, and never as a member.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};

use crate::code_view::{Cluster, Tests};
use crate::contextgraph::{Edge, Graph, REL_TESTS, TIER_INFERRED};
use crate::resolve::ProofCall;

/// The graph attribute of a cluster node that carries its attached test count, whatever tests a
/// reader shows.
pub const TESTS_ATTR: &str = "tests";

/// Which cluster each test entity is attached to.
///
/// THE PLURALITY RULE: a test entity attaches to the cluster holding the most of the distinct
/// product definitions it proves (a definition proved by many calls counts once). A tie goes to
/// the cluster earliest in the grouping's own order, which is its numbering order, so
/// `community/1/2` wins over `community/1/10`. A test that proves no grouped product definition is
/// attached to none; a call outside any definition is the file's own, and the file is the test
/// entity.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TestAttachment {
    cluster_of: BTreeMap<String, String>,
    counts: FxHashMap<String, usize>,
}

impl TestAttachment {
    /// The attachment of the tests `proofs` name to `clusters`.
    pub fn new(proofs: &[ProofCall], clusters: &[Cluster]) -> Self {
        let mut clusters_of: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (at, cluster) in clusters.iter().enumerate() {
            for member in &cluster.members {
                clusters_of.entry(member).or_default().push(at);
            }
        }
        let mut proved: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for p in proofs {
            proved.entry(&p.caller).or_default().insert(&p.target);
        }
        let mut attachment = Self::default();
        for (test, targets) in proved {
            let mut votes: BTreeMap<usize, usize> = BTreeMap::new();
            for target in targets {
                for &at in clusters_of.get(target).into_iter().flatten() {
                    *votes.entry(at).or_default() += 1;
                }
            }
            // The first maximum in cluster order: the tie goes to the earliest cluster.
            let winner = votes
                .into_iter()
                .fold(None, |best: Option<(usize, usize)>, (at, n)| match best {
                    Some((_, most)) if most >= n => best,
                    _ => Some((at, n)),
                });
            if let Some((at, _)) = winner {
                let id = clusters[at].id.clone();
                *attachment.counts.entry(id.clone()).or_default() += 1;
                attachment.cluster_of.insert(test.to_string(), id);
            }
        }
        attachment
    }

    /// The cluster the test entity `test` is attached to.
    pub fn cluster_of(&self, test: &str) -> Option<&str> {
        self.cluster_of.get(test).map(String::as_str)
    }

    /// How many test entities are attached to the cluster `cluster`.
    pub fn count(&self, cluster: &str) -> usize {
        self.counts.get(cluster).copied().unwrap_or(0)
    }

    /// The test entities attached to the cluster `cluster`, in id order.
    pub fn tests_of<'a>(&'a self, cluster: &'a str) -> impl Iterator<Item = &'a str> {
        self.attached()
            .filter(move |(_, c)| *c == cluster)
            .map(|(t, _)| t)
    }

    /// Every attached test entity with its cluster, in test id order.
    pub fn attached(&self) -> impl Iterator<Item = (&str, &str)> {
        self.cluster_of
            .iter()
            .map(|(t, c)| (t.as_str(), c.as_str()))
    }
}

/// `graph`'s cluster nodes carrying their attached test count (`tests`), whatever `tests` shows;
/// and, when tests are shown, the [`REL_TESTS`] edge from each attached test entity `graph` holds
/// to its cluster.
pub fn join_tests(graph: &mut Graph, attachment: &TestAttachment, tests: Tests) {
    for node in &mut graph.nodes {
        let count = attachment.count(&node.id);
        if count > 0 {
            node.attrs.insert(TESTS_ATTR.to_string(), count.to_string());
        }
    }
    if tests == Tests::Exclude {
        return;
    }
    let held: FxHashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for (test, cluster) in attachment.attached() {
        if held.contains(test) {
            graph.edges.push(Edge {
                from: test.to_string(),
                to: cluster.to_string(),
                rel: REL_TESTS.into(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: TIER_INFERRED.into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_facts::{Def, FileFacts, Mention, MentionKind, Scope, Stamp, Tip};
    use crate::code_view::{Cluster, FactsView, Groupings, ViewFilter};
    use crate::composed::{Composed, Honesty, Indexed};
    use crate::contextgraph::{ContextGraph, REL_IN_COMMUNITY};
    use crate::resolve::Resolver;

    fn def(local: &str, test: bool) -> Def {
        Def {
            local: local.into(),
            leaf: local.into(),
            kind: "function".into(),
            line: 1,
            end_line: 1,
            parent: None,
            test,
        }
    }

    fn call(leaf: &str, enclosing: u32, line: u32) -> Mention {
        Mention {
            enclosing: Some(enclosing),
            name: leaf.into(),
            leaf: leaf.into(),
            kind: MentionKind::Call,
            line,
            test: false,
        }
    }

    fn cluster(id: &str, members: &[&str]) -> Cluster {
        Cluster {
            id: id.into(),
            label: id.into(),
            resolution: 1.0,
            hash: "h".into(),
            members: members.iter().map(|m| m.to_string()).collect(),
        }
    }

    /// Product definitions a, b (community 0), c, d (community 1) and e (community 2), and the
    /// tests that call them:
    /// - t1 proves a, b, c: community 0 holds two of three;
    /// - t2 proves a (three calls, one definition), c, d: community 1 holds two of three;
    /// - t3 proves a and c: a tie, which goes to the earlier community 0;
    /// - t4 proves nothing and t5 proves only ungrouped `f`: both attach to none.
    fn view() -> FactsView {
        let product = FileFacts {
            path: "src/p.rs".into(),
            lang: "rust".into(),
            scope: Scope::Product,
            defs: ["a", "b", "c", "d", "e", "f"]
                .map(|l| def(l, false))
                .to_vec(),
            ..Default::default()
        };
        let tests = FileFacts {
            path: "tests/t.rs".into(),
            lang: "rust".into(),
            scope: Scope::Test,
            defs: ["t1", "t2", "t3", "t4", "t5"]
                .map(|l| def(l, true))
                .to_vec(),
            mentions: vec![
                call("a", 0, 10),
                call("b", 0, 11),
                call("c", 0, 12),
                call("a", 1, 20),
                call("a", 1, 21),
                call("a", 1, 22),
                call("c", 1, 23),
                call("d", 1, 24),
                call("a", 2, 30),
                call("c", 2, 31),
                call("f", 4, 50),
            ],
            ..Default::default()
        };
        let stamp = Stamp {
            tip: Tip {
                head: "main".into(),
                commit: None,
                tree: "t".into(),
            },
            fingerprint: 0,
            seq: 1,
        };
        FactsView::new(stamp, vec![product, tests]).with_groupings(Groupings {
            communities: vec![
                cluster("community/1/0", &["src/p.rs::a", "src/p.rs::b"]),
                cluster("community/1/1", &["src/p.rs::c", "src/p.rs::d"]),
                cluster("community/1/2", &["src/p.rs::e"]),
            ],
            concepts: Vec::new(),
        })
    }

    fn read(view: &FactsView, tests: Tests) -> Graph {
        let records = Indexed::default();
        let honesty = Honesty {
            worktree: "W".into(),
            diff: Ok(Default::default()),
            behind: None,
        };
        Composed::new(&records, view, ViewFilter { tests }, honesty)
            .unwrap()
            .whole()
            .unwrap()
    }

    fn edges_of(graph: &Graph, rel: &str) -> Vec<(String, String)> {
        let mut found: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.rel == rel)
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn tests_include_attaches_each_test_to_its_plurality_community() {
        let attached = [
            ("tests/t.rs::t1", "community/1/0"),
            ("tests/t.rs::t2", "community/1/1"),
            ("tests/t.rs::t3", "community/1/0"),
        ]
        .map(|(t, c)| (t.to_string(), c.to_string()));
        let view = view();
        for tests in [Tests::Include, Tests::Only] {
            let graph = read(&view, tests);
            assert_eq!(
                edges_of(&graph, REL_TESTS),
                attached.to_vec(),
                "{tests:?}: each test is drawn at its community, t4 and t5 at none"
            );
            let members: Vec<_> = edges_of(&graph, REL_IN_COMMUNITY)
                .into_iter()
                .map(|(m, _)| m)
                .collect();
            assert!(
                members.iter().all(|m| m.starts_with("src/")),
                "{tests:?}: a test is never a member: {members:?}"
            );
        }
        let counts = |graph: &Graph| {
            ["community/1/0", "community/1/1", "community/1/2"].map(|id| {
                graph
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| n.attrs.get(TESTS_ATTR).cloned())
            })
        };
        let expected = [Some("2".to_string()), Some("1".to_string()), None];
        for tests in [Tests::Exclude, Tests::Include, Tests::Only] {
            assert_eq!(counts(&read(&view, tests)), expected, "{tests:?}");
        }
        let excluded = read(&view, Tests::Exclude);
        assert!(edges_of(&excluded, REL_TESTS).is_empty());
        assert!(excluded.nodes.iter().all(|n| !n.id.starts_with("tests/")));
    }

    /// Each row: the proofs' callers and the clusters they vote across, and the cluster the
    /// plurality rule settles (none when no grouped definition is proved).
    #[test]
    fn plurality_rule_settles_each_vote() {
        let clusters = [
            cluster("community/1/0", &["x", "y"]),
            cluster("community/1/1", &["z", "w"]),
        ];
        let proofs = |targets: &[&str]| -> Vec<ProofCall> {
            targets
                .iter()
                .map(|t| ProofCall {
                    target: t.to_string(),
                    caller: "T".into(),
                    evidence: "f:1".into(),
                })
                .collect()
        };
        let cases: [(&str, &[&str], Option<&str>); 6] = [
            ("one definition", &["z"], Some("community/1/1")),
            ("the larger share", &["x", "z", "w"], Some("community/1/1")),
            (
                "a tie goes to the earlier",
                &["z", "x"],
                Some("community/1/0"),
            ),
            (
                "repeat calls count once",
                &["z", "z", "z", "x", "y"],
                Some("community/1/0"),
            ),
            ("ungrouped only", &["q"], None),
            ("nothing proved", &[], None),
        ];
        for (name, targets, expected) in cases {
            let attachment = TestAttachment::new(&proofs(targets), &clusters);
            assert_eq!(attachment.cluster_of("T"), expected, "{name}");
        }
    }

    #[test]
    fn proof_counts_survive_the_tests_exclude_filter() {
        let view = view();
        let proven = |tests: Tests| -> Vec<(String, Option<String>)> {
            let graph = read(&view, tests);
            ["a", "b", "c", "d", "e"]
                .map(|l| {
                    let node = graph
                        .nodes
                        .iter()
                        .find(|n| n.id == format!("src/p.rs::{l}"));
                    let proven = node.and_then(|n| n.attrs.get("proven_by").cloned());
                    (l.to_string(), proven)
                })
                .to_vec()
        };
        let expected: Vec<(String, Option<String>)> = [
            ("a", Some("5")),
            ("b", Some("1")),
            ("c", Some("3")),
            ("d", Some("1")),
            ("e", None),
        ]
        .map(|(l, n)| (l.to_string(), n.map(str::to_string)))
        .to_vec();
        for tests in [Tests::Exclude, Tests::Include] {
            assert_eq!(proven(tests), expected, "{tests:?}");
        }
        let evidence = read(&view, Tests::Exclude)
            .nodes
            .into_iter()
            .find(|n| n.id == "src/p.rs::b")
            .and_then(|n| n.attrs.get("proof_evidence").cloned());
        assert_eq!(evidence.as_deref(), Some(r#"["tests/t.rs:11"]"#));
        let resolver = Resolver::new(&view);
        assert_eq!(resolver.proofs()["src/p.rs::b"], ["tests/t.rs:11"]);
    }

    /// The tests a filter hides are the graph's to report: `exclude` holds them back as
    /// `hidden_tests`, and `include` and `only` show them, so nothing is hidden. A hidden test
    /// whose file the caller's worktree changed is withheld and counted as a shown one is.
    #[test]
    fn the_graph_reports_the_tests_it_hides() {
        let view = view();
        let hidden = |tests: Tests, changed: &[&str]| -> (Vec<String>, usize) {
            let records = Indexed::default();
            let diff = crate::code_view::WorktreeDiff {
                changed: changed.iter().map(|f| f.to_string()).collect(),
                untracked: Vec::new(),
            };
            let honesty = Honesty {
                worktree: "W".into(),
                diff: Ok(diff),
                behind: None,
            };
            let graph = Composed::new(&records, &view, ViewFilter { tests }, honesty).unwrap();
            let ids = graph.hidden_tests().unwrap().into_iter().map(|n| n.id);
            (ids.collect(), graph.stamp_label().withheld)
        };
        let all = ["t1", "t2", "t3", "t4", "t5"].map(|t| format!("tests/t.rs::{t}"));
        let cases: [(Tests, &[&str], &[String], usize); 5] = [
            (Tests::Exclude, &[], &all, 0),
            (Tests::Exclude, &["src/p.rs"], &all, 0),
            (Tests::Exclude, &["tests/t.rs"], &[], 5),
            (Tests::Include, &[], &[], 0),
            (Tests::Only, &[], &[], 0),
        ];
        for (tests, changed, ids, withheld) in cases {
            let want = (ids.to_vec(), withheld);
            assert_eq!(hidden(tests, changed), want, "{tests:?} {changed:?}");
        }
    }

    /// Every test mention is walked once per open: the proofs and the attachment both read the
    /// resolver's one walk, so asking again reads no file.
    #[test]
    fn the_proof_walk_runs_once_per_resolver() {
        use crate::code_view::{CodeView, DefRef, FileRef, MentionRef};
        use std::borrow::Cow;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Counting {
            inner: FactsView,
            reads: AtomicUsize,
        }
        impl CodeView for Counting {
            fn stamp(&self) -> &Stamp {
                self.inner.stamp()
            }
            fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
                self.inner.files()
            }
            fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
                self.reads.fetch_add(1, Ordering::SeqCst);
                self.inner.file(path)
            }
            fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
                self.inner.defs_named(leaf)
            }
            fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
                self.inner.mentions_named(leaf)
            }
            fn groupings(&self) -> &Groupings {
                self.inner.groupings()
            }
        }

        let view = Counting {
            inner: view(),
            reads: AtomicUsize::new(0),
        };
        let resolver = Resolver::new(&view);
        let first = resolver.proof_calls().len();
        let after_first = view.reads.load(Ordering::SeqCst);
        assert!(first > 0 && after_first > 0, "the walk reads the files");
        let again = (resolver.proof_calls().len(), resolver.proofs().len());
        assert_eq!(again.0, first);
        assert_eq!(
            view.reads.load(Ordering::SeqCst),
            after_first,
            "no second walk: {again:?}"
        );
    }

    /// The tests an excluding read hides are counted by the graph it opened, attached to a
    /// cluster or not, and the graph it drew holds only entity and record nodes: no node stands
    /// in for the count.
    #[test]
    fn an_excluding_read_counts_every_hidden_test_without_a_node_of_its_own() {
        use crate::contextgraph::{
            KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_FILE,
        };

        let view = view();
        let records = Indexed::default();
        let honesty = || Honesty {
            worktree: "W".into(),
            diff: Ok(Default::default()),
            behind: None,
        };
        let hidden = |tests: Tests| {
            Composed::new(&records, &view, ViewFilter { tests }, honesty())
                .unwrap()
                .hidden_tests()
                .unwrap()
                .len()
        };
        assert_eq!(hidden(Tests::Exclude), 5);
        assert_eq!(hidden(Tests::Include), 0);
        assert_eq!(hidden(Tests::Only), 0);
        let kinds: std::collections::BTreeSet<String> = read(&view, Tests::Exclude)
            .nodes
            .into_iter()
            .map(|n| n.kind)
            .collect();
        let allowed = [
            KIND_ARTIFACT,
            KIND_CODE_ENTITY,
            KIND_COMMUNITY,
            KIND_CONCEPT,
            KIND_FILE,
        ];
        assert!(
            kinds.iter().all(|k| allowed.contains(&k.as_str())),
            "{kinds:?}"
        );
    }
}
