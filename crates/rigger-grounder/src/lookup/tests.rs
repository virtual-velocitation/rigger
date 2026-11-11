//! The combined lookup's unit tests: the word rule, name resolution's tiers, the live text scan,
//! the prose index's refresh, the graph half's freshness filter, the refused graph and the reply
//! budget, each a table over one check.

use std::collections::BTreeMap;
use std::path::Path;

#[cfg(feature = "symbols")]
use super::budget::WITHHELD;
use super::budget::{self, REPLY_LIMIT, TEXT_TOTAL};
use super::graph::{Entity, Site, TermGraph};
#[cfg(feature = "symbols")]
use super::normalise;
use super::prose::{ProseHit, ProseIndex, Ranked};
use super::text::{self, TermText, TextHit};
use super::{words, Lookup, TreeLookup};
#[cfg(feature = "symbols")]
use crate::contextgraph::KIND_CODE_ENTITY;
use crate::contextgraph::{
    CallGraph, CallNode, Direction, EntryBatch, EntryFold, Error, FoldAccess, Graph, Node,
    Projection, KIND_DECISION, REBUILD_OWED,
};
use rigger_domain::eventstore::Event;

/// A graph held in memory: its nodes, the generation it holds per identity, whether it owes its
/// rebuild, and its calls as (caller, callee) ids.
#[derive(Default)]
struct Held {
    nodes: Vec<Node>,
    generations: BTreeMap<String, String>,
    owed: bool,
    calls: Vec<(String, String)>,
}

impl Projection for Held {
    fn apply(&self, _e: &Event, _access: FoldAccess) -> Result<(), Error> {
        unreachable!("the lookup never folds")
    }
    fn apply_generation(&self, _entry: &Event, _batch: EntryBatch<'_>) -> Result<EntryFold, Error> {
        unreachable!("the lookup never folds")
    }
    fn current_generation(&self, identity: &str) -> Result<Option<String>, Error> {
        Ok(self.generations.get(identity).cloned())
    }
    fn rebuild_owed(&self) -> Result<bool, Error> {
        Ok(self.owed)
    }
    fn subgraph(&self, _seed: &[String], _depth: i64) -> Result<Graph, Error> {
        Ok(Graph::default())
    }
    fn resolve(&self, _mention: &str) -> Result<Option<String>, Error> {
        Ok(None)
    }
    fn calls(
        &self,
        seed: &[String],
        direction: Direction,
        _depth: i64,
        _tier_floor: &str,
    ) -> Result<CallGraph, Error> {
        let other = |(caller, callee): &(String, String)| match direction {
            Direction::Up if seed.contains(callee) => Some(caller.clone()),
            Direction::Down if seed.contains(caller) => Some(callee.clone()),
            _ => None,
        };
        let nodes = self
            .calls
            .iter()
            .filter_map(other)
            .filter_map(|id| self.nodes.iter().find(|n| n.id == id).cloned())
            .map(|node| CallNode {
                node,
                layer: 1,
                frontier: None,
            })
            .collect();
        Ok(CallGraph {
            nodes,
            ..CallGraph::default()
        })
    }
    fn nodes_of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        Ok(self
            .nodes
            .iter()
            .filter(|n| n.kind == kind)
            .cloned()
            .collect())
    }
}

/// The code entity `<file>::<name>`, a function at `line`.
#[cfg(feature = "symbols")]
fn entity(file: &str, name: &str, line: u32) -> Node {
    let attrs = [
        ("name", name),
        ("kind", "function"),
        ("line", &line.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    Node {
        id: format!("{file}::{name}"),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs,
    }
}

/// A decision record summarised as `summary`.
fn decision(id: &str, summary: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_DECISION.to_string(),
        attrs: [("summary".to_string(), summary.to_string())].into(),
    }
}

/// A fresh tree holding `files`.
fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, body) in files {
        write(dir.path(), path, body);
    }
    dir
}

fn write(root: &Path, path: &str, body: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn terms(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

/// The lines of `reply`'s `section` (`graph`, `text` or `prose`), its heading excluded.
fn section<'a>(reply: &'a str, section: &str) -> &'a str {
    let start = reply
        .find(&format!("## {section}\n"))
        .unwrap_or_else(|| panic!("no {section} section in:\n{reply}"));
    let body = &reply[start + section.len() + 4..];
    body.find("\n## ").map_or(body, |end| &body[..end])
}

// --- the word rule -------------------------------------------------------------------------

fn words_are(term: &str, expected: &[&str]) {
    assert_eq!(words(term), expected, "the words of {term:?}");
    #[cfg(feature = "symbols")]
    assert_eq!(normalise(term), expected.concat(), "{term:?} normalised");
}

crate::test_cases! {
    snake_case_splits_at_underscores: words_are("apply_damage", &["apply", "damage"]);
    camel_case_splits_at_case_boundaries: words_are("applyDamage", &["apply", "damage"]);
    pascal_case_splits_at_case_boundaries: words_are("ApplyDamage", &["apply", "damage"]);
    kebab_case_splits_at_hyphens: words_are("apply-damage", &["apply", "damage"]);
    screaming_case_splits_at_underscores: words_are("APPLY_DAMAGE", &["apply", "damage"]);
    an_acronym_ends_before_the_next_word: words_are("HTTPServer", &["http", "server"]);
    a_digit_ends_before_an_uppercase_letter: words_are("utf8Decode", &["utf8", "decode"]);
    a_path_splits_at_every_separator: words_are("a.rs::Foo", &["a", "rs", "foo"]);
}

// --- name resolution -----------------------------------------------------------------------

#[cfg(feature = "symbols")]
fn resolves(term: &str, expected: super::names::Resolved) {
    use super::names::Names;
    let names = Names::build(vec![
        entity("a.rs", "apply_damage", 3),
        entity("b.rs", "HttpServer", 7),
        entity("c.rs", "parse_config", 1),
        entity("d.rs", "render", 9),
    ]);
    assert_eq!(names.resolve(term), expected, "the term {term:?}");
}

#[cfg(feature = "symbols")]
fn found(tier: super::names::Tier, ids: &[&str]) -> super::names::Resolved {
    super::names::Resolved::Found {
        tier,
        ids: ids.iter().map(|id| id.to_string()).collect(),
    }
}

#[cfg(feature = "symbols")]
crate::test_cases! {
    a_bare_name_resolves_exactly: resolves("apply_damage", found(super::names::Tier::Exact, &["a.rs::apply_damage"]));
    a_full_id_resolves_exactly: resolves("a.rs::apply_damage", found(super::names::Tier::Exact, &["a.rs::apply_damage"]));
    a_camel_variant_resolves_as_a_variant: resolves("applyDamage", found(super::names::Tier::Variant, &["a.rs::apply_damage"]));
    a_snake_variant_of_a_pascal_name_resolves_as_a_variant: resolves("http_server", found(super::names::Tier::Variant, &["b.rs::HttpServer"]));
    a_kebab_variant_resolves_as_a_variant: resolves("apply-damage", found(super::names::Tier::Variant, &["a.rs::apply_damage"]));
    a_prefix_resolves_by_prefix: resolves("parse_conf", found(super::names::Tier::Prefix, &["c.rs::parse_config"]));
    a_misspelling_resolves_fuzzily: resolves("rendr", found(super::names::Tier::Fuzzy, &["d.rs::render"]));
    a_longer_misspelling_resolves_fuzzily: resolves("aply_damage", found(super::names::Tier::Fuzzy, &["a.rs::apply_damage"]));
    a_missing_name_names_its_nearest: resolves("apple", super::names::Resolved::Missing { nearest: vec!["apply_damage".into()] });
    a_name_near_nothing_names_none: resolves("zzz", super::names::Resolved::Missing { nearest: vec![] });
}

// --- the text half -------------------------------------------------------------------------

/// The text scan of a tree holding `files` for `terms`: each term's (file, line) hits and match
/// count.
fn scans(files: &[(&str, &str)], terms: &[&str], expected: &[(&[(&str, u64)], usize)]) {
    let dir = tree(files);
    let found = text::scan(dir.path(), terms).unwrap();
    let got: Vec<(Vec<(String, u64)>, usize)> = found
        .iter()
        .map(|t| {
            (
                t.hits.iter().map(|h| (h.file.clone(), h.line)).collect(),
                t.matched,
            )
        })
        .collect();
    let want: Vec<(Vec<(String, u64)>, usize)> = expected
        .iter()
        .map(|(hits, n)| (hits.iter().map(|(f, l)| (f.to_string(), *l)).collect(), *n))
        .collect();
    assert_eq!(got, want, "the scan of {files:?} for {terms:?}");
}

crate::test_cases! {
    a_lowercase_term_matches_any_case: scans(
        &[("a.rs", "Alpha\nalpha\nbeta\n")], &["alpha"], &[(&[("a.rs", 1), ("a.rs", 2)], 2)]);
    an_uppercase_term_matches_its_case_only: scans(
        &[("a.rs", "Alpha\nalpha\n")], &["Alpha"], &[(&[("a.rs", 1)], 1)]);
    a_term_is_literal_not_a_pattern: scans(
        &[("a.rs", "a.b\naxb\n")], &["a.b"], &[(&[("a.rs", 1)], 1)]);
    each_term_is_answered_in_order: scans(
        &[("a.rs", "one\ntwo\n")], &["two", "one"], &[(&[("a.rs", 2)], 1), (&[("a.rs", 1)], 1)]);
    an_ignored_file_is_not_scanned: scans(
        &[(".gitignore", "out/\n"), ("out/x.rs", "alpha\n"), ("a.rs", "alpha\n")], &["alpha"], &[(&[("a.rs", 1)], 1)]);
    a_file_keeps_its_first_five_lines_and_counts_every_match: scans(
        &[("a.rs", &"alpha\n".repeat(7))], &["alpha"],
        &[(&[("a.rs", 1), ("a.rs", 2), ("a.rs", 3), ("a.rs", 4), ("a.rs", 5)], 7)]);
}

// --- the prose half ------------------------------------------------------------------------

/// The (kind, place) of each ranked hit.
fn ranked_places(ranked: &Ranked) -> Vec<(String, String)> {
    ranked
        .hits
        .iter()
        .map(|h| (h.kind.clone(), h.place.clone()))
        .collect()
}

/// A prose index over a tree holding `before` ranks `term` to `first`; after `edit` (path, new
/// body, or `None` to remove the file) it ranks it to `then`, from the tree's new bytes.
fn reranks(
    before: &[(&str, &str)],
    term: &str,
    first: &[(&str, &str)],
    edit: (&str, Option<&str>),
    then: &[(&str, &str)],
) {
    let dir = tree(before);
    let mut index = ProseIndex::default();
    let owned = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(k, p)| (k.to_string(), p.to_string()))
            .collect()
    };
    let ranked = index.rank(dir.path(), &[term], Ok(Vec::new())).unwrap();
    assert_eq!(ranked_places(&ranked), owned(first), "before the edit");
    match edit {
        (path, Some(body)) => write(dir.path(), path, body),
        (path, None) => std::fs::remove_file(dir.path().join(path)).unwrap(),
    }
    let ranked = index.rank(dir.path(), &[term], Ok(Vec::new())).unwrap();
    assert_eq!(ranked_places(&ranked), owned(then), "after the edit");
}

crate::test_cases! {
    a_changed_prose_file_is_ranked_from_its_new_text: reranks(
        &[("docs/a.md", "# Intro\nnothing here\n")], "gizmo", &[],
        ("docs/a.md", Some("# Intro\nthe gizmo spins\n")), &[("doc", "docs/a.md:1")]);
    an_added_prose_file_is_ranked: reranks(
        &[("docs/a.md", "# Intro\nnothing\n")], "gizmo", &[],
        ("specs/s.md", Some("# Spec\n\n## Gizmo\nwhy\n")), &[("spec", "specs/s.md:3")]);
    a_removed_prose_file_leaves_the_ranking: reranks(
        &[("docs/a.md", "# Intro\nthe gizmo\n")], "gizmo", &[("doc", "docs/a.md:1")],
        ("docs/a.md", None), &[]);
    a_specs_done_when_items_rank_as_criteria: reranks(
        &[("docs/a.md", "# A\n")], "gizmo", &[],
        ("specs/s.md", Some("# S\n\n## Done when\n\n- [ ] the gizmo spins\n- [ ] other\n")),
        &[("criterion", "specs/s.md:5")]);
    a_code_file_is_not_prose: reranks(
        &[("a.rs", "// gizmo\n")], "gizmo", &[],
        ("skills/x/SKILL.md", Some("# Skill\nuse the gizmo\n")), &[("skill", "skills/x/SKILL.md:1")]);
}

/// The graph's records rank as prose, and records left out are said to be.
#[test]
fn records_rank_as_prose_and_a_left_out_record_set_is_stated() {
    let dir = tree(&[("docs/a.md", "# A\n")]);
    let mut index = ProseIndex::default();
    let ranked = index
        .rank(
            dir.path(),
            &["gizmo"],
            Ok(vec![decision("d1", "keep the gizmo in memory")]),
        )
        .unwrap();
    assert_eq!(
        ranked_places(&ranked),
        vec![("decision".to_string(), "d1".to_string())]
    );
    let ranked = index
        .rank(dir.path(), &["gizmo"], Err("the graph is away".to_string()))
        .unwrap();
    assert_eq!(
        ranked.records_left_out.as_deref(),
        Some("the graph is away")
    );
    assert!(
        ranked.hits.is_empty(),
        "a record held is not ranked while its graph cannot be read"
    );
}

// --- the whole lookup ----------------------------------------------------------------------

/// A graph over a tree whose `kept.rs` is unchanged since the graph ingested it, `edited.rs` was
/// edited since and `gone.rs` deleted since, and whose `added.rs` is new since; each defines a
/// `widget_*` function, and `kept.rs`'s `widget_kept` is called by `edited.rs`'s `widget_edited`.
#[cfg(feature = "symbols")]
fn diverged() -> (tempfile::TempDir, Held) {
    let dir = tree(&[
        ("kept.rs", "fn widget_kept() {}\n"),
        ("edited.rs", "fn widget_edited() { widget_kept() }\n"),
    ]);
    let generation = |file: &str| crate::ingest::tree_generation(dir.path(), file, false);
    let generations = [
        ("gc/kept.rs".to_string(), generation("kept.rs")),
        ("gc/edited.rs".to_string(), generation("edited.rs")),
        (
            "gc/gone.rs".to_string(),
            "a generation of bytes gone".to_string(),
        ),
    ]
    .into();
    write(
        dir.path(),
        "edited.rs",
        "fn widget_renamed() { widget_kept() }\n",
    );
    write(dir.path(), "added.rs", "fn widget_added() {}\n");
    let graph = Held {
        nodes: vec![
            entity("kept.rs", "widget_kept", 1),
            entity("edited.rs", "widget_edited", 1),
            entity("gone.rs", "widget_gone", 1),
        ],
        generations,
        calls: vec![(
            "edited.rs::widget_edited".to_string(),
            "kept.rs::widget_kept".to_string(),
        )],
        ..Held::default()
    };
    (dir, graph)
}

/// One freshness case: the term, the entity lines the graph half shows, what else the graph half
/// says, and what the text half says.
#[cfg(feature = "symbols")]
type FreshnessCase<'a> = (&'a str, &'a [&'a str], &'a [&'a str], &'a [&'a str]);

/// A file changed, deleted or added in the caller's tree is answered from the tree by the text
/// half and withheld by the graph half, with an unchanged file's hit kept.
#[cfg(feature = "symbols")]
#[test]
fn a_file_changed_deleted_or_added_is_answered_by_text_and_withheld_by_graph() {
    let (dir, graph) = diverged();
    let lookup = TreeLookup::new(dir.path());
    let cases: [FreshnessCase; 4] = [
        (
            "widget_kept",
            &["kept.rs:1 kept.rs::widget_kept (function)"],
            &[&format!("{WITHHELD}: edited.rs")],
            &["kept.rs:1:", "edited.rs:1:"],
        ),
        (
            "widget_edited",
            &[],
            &[&format!("{WITHHELD}: edited.rs")],
            &["no line of your tree holds widget_edited"],
        ),
        (
            "widget_gone",
            &[],
            &[&format!("{WITHHELD}: gone.rs")],
            &["no line of your tree holds widget_gone"],
        ),
        (
            "widget_added",
            &[],
            &["no entity named widget_added"],
            &["added.rs:1: fn widget_added() {}"],
        ),
    ];
    for (term, shown, graph_says, text_says) in cases {
        let reply = lookup.lookup(&terms(&[term]), Ok(&graph)).unwrap();
        let graph_part = section(&reply, "graph");
        for line in shown.iter().chain(graph_says) {
            assert!(
                graph_part.contains(line),
                "{term}: the graph half says {line:?}:\n{reply}"
            );
        }
        if shown.is_empty() {
            assert!(
                !graph_part.contains("\n- "),
                "{term}: the graph half shows no entity:\n{reply}"
            );
        }
        for line in text_says {
            assert!(
                section(&reply, "text").contains(line),
                "{term}: the text half says {line:?}:\n{reply}"
            );
        }
    }
}

/// A multi-term lookup groups every section's answers by term, in the order given.
#[cfg(feature = "symbols")]
#[test]
fn a_multi_term_lookup_groups_each_section_per_term() {
    let (dir, graph) = diverged();
    let reply = TreeLookup::new(dir.path())
        .lookup(
            &terms(&["widget_kept", "widget_added", "widget_kept"]),
            Ok(&graph),
        )
        .unwrap();
    for part in ["graph", "text"] {
        let body = section(&reply, part);
        let kept = body.find("### widget_kept\n").expect("widget_kept group");
        let added = body.find("### widget_added\n").expect("widget_added group");
        assert!(
            kept < added,
            "{part}: the groups follow the terms' order:\n{reply}"
        );
        assert_eq!(
            body.matches("### widget_kept\n").count(),
            1,
            "a repeated term is answered once"
        );
    }
}

/// The reason a caller that could not open its graph hands the lookup in its place.
const UNOPENED: &str = "the context graph could not be opened: disk full";

/// A graph that cannot answer, or that the caller could not open, still leaves the text and prose
/// halves answering, the graph's section stating why.
#[test]
fn a_graph_that_cannot_answer_still_returns_text_and_prose() {
    let dir = tree(&[
        ("a.rs", "fn gizmo() {}\n"),
        ("docs/a.md", "# Gizmo\nthe gizmo\n"),
    ]);
    let owed = Held {
        owed: true,
        ..Held::default()
    };
    let cases: [(Result<&dyn Projection, String>, &str); 2] = [
        (Ok(&owed), REBUILD_OWED),
        (Err(UNOPENED.to_string()), UNOPENED),
    ];
    for (graph, reason) in cases {
        let reply = TreeLookup::new(dir.path())
            .lookup(&terms(&["gizmo"]), graph)
            .unwrap();
        assert!(
            section(&reply, "graph").contains(&format!("not answered: {reason}")),
            "the graph section states {reason:?}:\n{reply}"
        );
        assert!(
            section(&reply, "text").contains("a.rs:1: fn gizmo() {}"),
            "{reply}"
        );
        assert!(
            section(&reply, "prose").contains("prose:doc docs/a.md:1"),
            "{reply}"
        );
    }
}

/// No term to look up is the one error.
#[test]
fn a_lookup_of_no_term_is_refused() {
    let dir = tree(&[]);
    for list in [&[][..], &["", "  "]] {
        assert!(TreeLookup::new(dir.path())
            .lookup(&terms(list), Err(UNOPENED.to_string()))
            .is_err());
    }
}

// --- the reply budget ----------------------------------------------------------------------

/// The worst case each section can be handed for `n` terms: every term reaching the most entities
/// with the most callers, the most lines kept of the longest length, and the top prose results
/// at their longest.
fn worst_case(n: usize) -> (Vec<String>, String) {
    let terms: Vec<String> = (0..n)
        .map(|i| format!("term_{i}_{}", "x".repeat(40)))
        .collect();
    let refs: Vec<&str> = terms.iter().map(String::as_str).collect();
    let long = "y".repeat(100);
    let site = |i: usize| Site {
        id: format!("src/{long}/f{i}.rs::{long}"),
        kind: "function".to_string(),
        file: Some((format!("src/{long}/f{i}.rs"), 12)),
    };
    let graph = refs
        .iter()
        .map(|_| TermGraph::Found {
            tier: "exact",
            entities: (0..super::budget::GRAPH_ENTITIES)
                .map(|i| Entity {
                    site: site(i),
                    callers: (0..40).map(site).collect(),
                    callees: (0..40).map(site).collect(),
                    referenced_in: (0..40).map(|i| format!("src/{long}/r{i}.rs")).collect(),
                })
                .collect(),
            reached: 500,
            unread: 492,
            withheld: (0..40).map(|i| format!("src/{long}/w{i}.rs")).collect(),
        })
        .collect();
    let text = refs
        .iter()
        .map(|_| TermText {
            hits: (0..TEXT_TOTAL)
                .map(|i| TextHit {
                    file: format!("src/{long}/t{i}.rs"),
                    line: 1,
                    text: long.clone(),
                })
                .collect(),
            matched: 10_000,
        })
        .collect();
    let prose = Ranked {
        hits: (0..super::budget::PROSE_TOP)
            .map(|i| ProseHit {
                kind: "spec".to_string(),
                place: format!("specs/{i}.md:1"),
                title: long.clone(),
                snippet: long.clone(),
            })
            .collect(),
        matched: 900,
        records_left_out: None,
    };
    let reply = budget::reply(&refs, Ok(graph), Ok(text), Ok(prose));
    (terms, reply)
}

/// The worst-case reply for `n` terms stays under the limit and states each cut.
fn worst_case_is_bounded(n: usize, cuts: &[&str]) {
    let (_, reply) = worst_case(n);
    assert!(
        reply.chars().count() <= REPLY_LIMIT,
        "{n} terms: {} characters",
        reply.chars().count()
    );
    for cut in cuts {
        assert!(reply.contains(cut), "{n} terms: the reply states {cut:?}");
    }
}

crate::test_cases! {
    one_worst_case_term_is_bounded_with_every_cut_stated: worst_case_is_bounded(1, &[
        "entities dropped to the graph section's size bound",
        "entities not read past the first",
        "(+30 more)",
        "w9.rs (+30 more)",
        "matching lines shown",
        "top 10 of 900 matching sections shown",
    ]);
    eight_worst_case_terms_are_bounded: worst_case_is_bounded(8, &["matching lines shown"]);
    two_hundred_terms_are_cut_whole_and_say_so: worst_case_is_bounded(200, &[
        &format!("truncated: the reply reached its {REPLY_LIMIT}-character bound"),
    ]);
}
