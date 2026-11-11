//! Extraction to facts: the [`Extract`] implementation that reads one file's path and bytes into
//! its [`FileFacts`]. It re-shapes what the per-language symbol extractors, the design-document
//! extractor and the workflow-definition extractor already find; it is no extractor of its own,
//! and holds no language knowledge. Every name is kept as written and nothing is resolved across
//! files, so a file's facts are a pure function of its path and bytes.

use std::collections::HashMap;

use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, WfNode,
    WfRelation,
};
use rigger_domain::code_view::{Entry, Extract};
use rigger_domain::contextgraph::definition_precedence;
use rigger_domain::playbooks::fnv1a_64;

use crate::config_store;
use crate::grounder::design::extract::{extract_concepts, extract_links};
use crate::grounder::symbols::events::{is_dev_only, kind_str, lang_str, module_candidates};
use crate::grounder::symbols::model::{FileSymbols, GRAMMAR_TAGS_VERSION};
use crate::grounder::symbols::{extracted, registry};
use crate::grounder::{text_of, workflow_doc, workflowdef};

/// The generation of everything this module adds to the extractors it re-shapes: the design and
/// workflow extractors, which carry no generation of their own, and the shape of the facts. Bump
/// it whenever any of them can give a file other facts than before, as the symbol extractors'
/// [`GRAMMAR_TAGS_VERSION`] is bumped for theirs; the two together make the fingerprint.
const FACTS_GENERATION: &str = "facts-v1";

/// The fingerprint of an extractor built from these generations: a stable hash, since a snapshot
/// stores it and a later process compares against it.
fn fingerprint_of(grammar_generation: &str, facts_generation: &str) -> u64 {
    fnv1a_64(format!("{grammar_generation}\n{facts_generation}").as_bytes())
}

/// Reads a file's facts over the existing symbol, design and workflow extractors.
#[derive(Clone, Copy, Debug, Default)]
pub struct FactsExtractor;

impl Extract for FactsExtractor {
    fn fingerprint(&self) -> u64 {
        fingerprint_of(GRAMMAR_TAGS_VERSION, FACTS_GENERATION)
    }

    /// Total over its input: bytes that are not UTF-8, a failed extraction and a workflow
    /// definition that does not parse give a file's identity and no lists, as a file that
    /// extracts to nothing.
    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
        let path = entry.path.as_str();
        let mut facts = FileFacts {
            path: entry.path.clone(),
            blob: entry.blob.clone(),
            scope: if is_dev_only(path) {
                Scope::Test
            } else {
                Scope::Product
            },
            ..FileFacts::default()
        };
        let text = text_of(Some(bytes));
        if path == workflow_doc() {
            facts.half = Half::Workflow;
            if let Some(text) = text {
                workflow_facts(text, &mut facts);
            }
            return facts;
        }
        if let Some(grammar) = registry::for_path(path, None) {
            facts.half = Half::Code;
            facts.lang = lang_str(grammar.lang).to_string();
            if let Some((_, symbols)) = extracted(path, || text, None).flatten() {
                code_facts(path, &symbols, &mut facts);
            }
        } else {
            facts.half = Half::Design;
        }
        if let Some(text) = text {
            design_facts(path, text, &mut facts);
        }
        facts
    }
}

/// Fills the definitions, mentions and module declarations of `facts` from the symbols the
/// extractor found in the file at `path`.
fn code_facts(path: &str, symbols: &FileSymbols, facts: &mut FileFacts) {
    let by_path = def_indices_by_path(symbols);
    let index_of = |def_path: &str| by_path.get(def_path).copied();
    facts.defs = symbols
        .defs
        .iter()
        .map(|d| Def {
            local: d.path.clone(),
            leaf: d.name.clone(),
            kind: kind_str(d.kind).to_string(),
            line: d.line,
            end_line: d.end_line,
            parent: d.parent.as_deref().and_then(index_of),
            test: d.is_test,
        })
        .collect();
    facts.mentions = symbols
        .refs
        .iter()
        .map(|r| Mention {
            enclosing: r.enclosing.as_deref().and_then(index_of),
            name: r.name.clone(),
            // The extractor names a reference by its identifier alone, and a definition's leaf is
            // its name whole, so the reference's name is the leaf the resolver finds it by.
            leaf: r.name.clone(),
            kind: match &r.ref_kind {
                None => MentionKind::Call,
                Some(kind) => MentionKind::Tagged(kind.clone()),
            },
            line: r.line,
            test: r.is_test,
        })
        .collect();
    facts.modules = symbols
        .defs
        .iter()
        .filter(|d| d.is_out_of_line_module)
        .map(|d| ModuleDecl {
            name: d.name.clone(),
            test: d.is_test,
            candidates: module_candidates(path, d),
        })
        .collect();
}

/// The index in `symbols.defs` of the definition a path names, for the paths the extractor gives
/// a parent or a caller by. Definitions can share a path (cfg twins, a type and a function of
/// one name); the one the path names is the one that becomes the entity, the first by
/// [`definition_precedence`], the earlier in the file's order on a tie.
fn def_indices_by_path(symbols: &FileSymbols) -> HashMap<&str, u32> {
    let mut held: HashMap<&str, (u32, (usize, u32))> = HashMap::new();
    for (i, d) in symbols.defs.iter().enumerate() {
        let rank = definition_precedence(kind_str(d.kind), d.line);
        held.entry(d.path.as_str())
            .and_modify(|best| {
                if rank < best.1 {
                    *best = (i as u32, rank);
                }
            })
            .or_insert((i as u32, rank));
    }
    held.into_iter().map(|(path, (i, _))| (path, i)).collect()
}

/// Fills the design concepts and links of `facts` from the design-document extractor, which reads
/// a document's intent and a source file's `WHY:` and `NOTE:` rationale alike.
fn design_facts(path: &str, text: &str, facts: &mut FileFacts) {
    facts.doc_concepts = extract_concepts(path, text)
        .into_iter()
        .map(|c| DocConcept {
            kind: c.kind.node_kind().to_string(),
            id: c.id,
            title: c.title,
        })
        .collect();
    facts.doc_links = extract_links(path, text)
        .into_iter()
        .map(|l| DocLink {
            from: l.from,
            rel: l.rel.rel().to_string(),
            to: l.to,
        })
        .collect();
}

/// Fills the nodes and relations of `facts` from the workflow definition `text`, none when it does
/// not parse.
fn workflow_facts(text: &str, facts: &mut FileFacts) {
    let Ok(workflow) = config_store::parse_workflow(text) else {
        return;
    };
    let (concepts, links) = workflowdef::extract(&workflow);
    facts.wf_nodes = concepts
        .into_iter()
        .map(|(kind, id, title)| WfNode {
            kind: kind.to_string(),
            id,
            title,
        })
        .collect();
    facts.wf_relations = links
        .into_iter()
        .map(|(from, rel, to)| WfRelation {
            from,
            rel: rel.to_string(),
            to,
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csharp_definition_sources::CSHARP_DEFINITIONS_SOURCE;
    use crate::go_definition_sources::GO_DEFINITIONS_SOURCE;
    use crate::js_ts_definition_sources::{
        JS_DEFINITIONS_SOURCE, TSX_DEFINITIONS_SOURCE, TS_DEFINITIONS_SOURCE,
    };
    use crate::python_definition_sources::PYTHON_DEFINITIONS_SOURCE;
    use crate::rust_definition_sources::RUST_DEFINITIONS_SOURCE;
    use rigger_domain::contextgraph::{
        KIND_AGENT, KIND_DESIGN_DOC, KIND_GATE, KIND_RATIONALE, KIND_STAGE, REL_EXPLAINS, REL_RUNS,
        REL_SPECIFIES,
    };

    fn facts_of(path: &str, bytes: &[u8]) -> FileFacts {
        let entry = Entry {
            path: path.to_string(),
            blob: format!("blob-of-{path}"),
        };
        FactsExtractor.extract(&entry, bytes)
    }

    /// A definition kind's name as the facts spell it, written here rather than taken from the
    /// production mapping the facts are built with.
    fn kind_name(kind: crate::grounder::symbols::model::Kind) -> String {
        format!("{kind:?}").to_lowercase()
    }

    #[test]
    fn facts_hold_every_def_and_mention_the_symbol_extractor_finds() {
        let table = [
            ("src/lib.rs", "rust", RUST_DEFINITIONS_SOURCE),
            ("pkg/main.go", "go", GO_DEFINITIONS_SOURCE),
            ("App/Program.cs", "csharp", CSHARP_DEFINITIONS_SOURCE),
            ("web/app.js", "js", JS_DEFINITIONS_SOURCE),
            ("web/app.ts", "ts", TS_DEFINITIONS_SOURCE),
            ("web/app.tsx", "ts", TSX_DEFINITIONS_SOURCE),
            ("tool/run.py", "python", PYTHON_DEFINITIONS_SOURCE),
        ];
        for (path, lang, source) in table {
            let symbols = extracted(path, || Some(source), None)
                .flatten()
                .unwrap_or_else(|| panic!("{path}: the symbol extractor reads the fixture"))
                .1;
            let facts = facts_of(path, source.as_bytes());
            assert_eq!(facts.path, path, "{path}");
            assert_eq!(facts.blob, format!("blob-of-{path}"), "{path}");
            assert_eq!(
                (facts.half, facts.lang.as_str()),
                (Half::Code, lang),
                "{path}"
            );
            assert!(
                !symbols.defs.is_empty() && !symbols.refs.is_empty(),
                "{path}: the fixture finds a definition and a reference"
            );

            let found_defs: Vec<_> = symbols
                .defs
                .iter()
                .map(|d| {
                    (
                        d.path.clone(),
                        d.name.clone(),
                        kind_name(d.kind),
                        d.line,
                        d.end_line,
                        d.is_test,
                        d.parent.clone(),
                    )
                })
                .collect();
            let held_defs: Vec<_> = facts
                .defs
                .iter()
                .map(|d| {
                    (
                        d.local.clone(),
                        d.leaf.clone(),
                        d.kind.clone(),
                        d.line,
                        d.end_line,
                        d.test,
                        d.parent.map(|p| facts.defs[p as usize].local.clone()),
                    )
                })
                .collect();
            assert_eq!(held_defs, found_defs, "{path}: definitions");

            let found_refs: Vec<_> = symbols
                .refs
                .iter()
                .map(|r| {
                    let kind = match &r.ref_kind {
                        None => "call".to_string(),
                        Some(kind) => format!("tagged:{kind}"),
                    };
                    (r.name.clone(), kind, r.line, r.is_test, r.enclosing.clone())
                })
                .collect();
            let held_refs: Vec<_> = facts
                .mentions
                .iter()
                .map(|m| {
                    assert_eq!(m.leaf, m.name, "{path}: a reference is named by its leaf");
                    let kind = match &m.kind {
                        MentionKind::Call => "call".to_string(),
                        MentionKind::Plain => "plain".to_string(),
                        MentionKind::Tagged(kind) => format!("tagged:{kind}"),
                    };
                    (
                        m.name.clone(),
                        kind,
                        m.line,
                        m.test,
                        m.enclosing.map(|e| facts.defs[e as usize].local.clone()),
                    )
                })
                .collect();
            assert_eq!(held_refs, found_refs, "{path}: mentions");
        }
    }

    #[test]
    fn a_path_two_definitions_share_names_the_precedence_winner() {
        // (source, the kind and line of the definition the mention's enclosing path names)
        let table = [
            ("fn x() {}\nfn x() { y(); }\n", ("function", 1)),
            ("fn x() { y(); }\nstruct x;\n", ("type", 2)),
        ];
        for (source, (kind, line)) in table {
            let facts = facts_of("src/lib.rs", source.as_bytes());
            let mention = facts
                .mentions
                .iter()
                .find(|m| m.name == "y")
                .unwrap_or_else(|| panic!("{source:?}: the call is a mention"));
            let enclosing = &facts.defs[mention.enclosing.expect("the call has a caller") as usize];
            assert_eq!(
                (enclosing.kind.as_str(), enclosing.line),
                (kind, line),
                "{source:?}"
            );
        }
    }

    #[test]
    fn facts_mark_test_defs_and_test_mentions() {
        // (path, source, the (leaf, test) of each definition, the (name, test) of each mention,
        // the scope)
        type Marks = &'static [(&'static str, bool)];
        type Row = (&'static str, &'static str, Marks, Marks, Scope);
        let table: [Row; 4] = [
            (
                "src/lib.rs",
                "pub fn product() { helper(); }\nfn helper() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn checks() { product(); }\n}\n",
                &[("product", false), ("helper", false), ("tests", true), ("checks", true)],
                &[("helper", false), ("product", true)],
                Scope::Product,
            ),
            (
                "src/lib.rs",
                "fn plain() { other(); }\nfn other() {}\n",
                &[("plain", false), ("other", false)],
                &[("other", false)],
                Scope::Product,
            ),
            (
                "tests/it.rs",
                "fn works() { product(); }\n",
                &[("works", false)],
                &[("product", false)],
                Scope::Test,
            ),
            (
                "pkg/test_a.py",
                "def test_x():\n    run()\n",
                &[("test_x", false)],
                &[("run", false)],
                Scope::Test,
            ),
        ];
        for (path, source, defs, mentions, scope) in table {
            let facts = facts_of(path, source.as_bytes());
            let held_defs: Vec<_> = facts
                .defs
                .iter()
                .map(|d| (d.leaf.as_str(), d.test))
                .collect();
            let held_mentions: Vec<_> = facts
                .mentions
                .iter()
                .map(|m| (m.name.as_str(), m.test))
                .collect();
            assert_eq!(held_defs, defs, "{path}: definitions");
            assert_eq!(held_mentions, mentions, "{path}: mentions");
            assert_eq!(facts.scope, scope, "{path}: scope");
        }
    }

    #[test]
    fn scope_follows_the_dev_only_path_rule() {
        let table = [
            ("tests/a.rs", Scope::Test),
            ("crates/x/benches/b.rs", Scope::Test),
            ("examples/demo.rs", Scope::Test),
            ("web/__tests__/a.ts", Scope::Test),
            ("docs/tests/notes.md", Scope::Test),
            ("pkg/x_test.go", Scope::Test),
            ("pkg/test_x.py", Scope::Test),
            ("pkg/x_test.py", Scope::Test),
            ("web/a.test.ts", Scope::Test),
            ("web/a.spec.js", Scope::Test),
            ("App.Tests/Case.cs", Scope::Test),
            ("AppTest/Case.cs", Scope::Test),
            ("src/lib.rs", Scope::Product),
            ("src/tests.rs", Scope::Product),
            ("src/testsuite.rs", Scope::Product),
            ("pkg/x_test.rs", Scope::Product),
            ("web/a.test.rs", Scope::Product),
            ("pkg/testing.py", Scope::Product),
            ("contest/Case.cs", Scope::Product),
            ("App/AppTests.cs", Scope::Product),
        ];
        for (path, scope) in table {
            assert_eq!(facts_of(path, b"").scope, scope, "{path}");
        }
    }

    #[test]
    fn module_candidates_come_from_the_declaring_file_alone() {
        // (declaring path, source, the (name, test, candidates) of each declaration)
        type Row = (
            &'static str,
            &'static str,
            &'static [(&'static str, bool, &'static [&'static str])],
        );
        let table: [Row; 7] = [
            (
                "src/lib.rs",
                "mod alpha;\n",
                &[("alpha", false, &["src/alpha.rs", "src/alpha/mod.rs"])],
            ),
            (
                "lib/other.rs",
                "mod alpha;\n",
                &[(
                    "alpha",
                    false,
                    &["lib/other/alpha.rs", "lib/other/alpha/mod.rs"],
                )],
            ),
            (
                "src/lib.rs",
                "#[cfg(test)]\nmod tests;\n",
                &[("tests", true, &["src/tests.rs", "src/tests/mod.rs"])],
            ),
            (
                "src/parent.rs",
                "#[path = \"other/x.rs\"]\nmod renamed;\n",
                &[("renamed", false, &["src/other/x.rs"])],
            ),
            (
                "src/lib.rs",
                "mod outer {\n    mod x;\n}\n",
                &[("x", false, &["src/outer/x.rs", "src/outer/x/mod.rs"])],
            ),
            ("src/lib.rs", "mod inline { fn f() {} }\n", &[]),
            ("web/app.ts", "namespace N { function f() {} }\n", &[]),
        ];
        for (path, source, modules) in table {
            let facts = facts_of(path, source.as_bytes());
            let held: Vec<(&str, bool, Vec<&str>)> = facts
                .modules
                .iter()
                .map(|m| {
                    (
                        m.name.as_str(),
                        m.test,
                        m.candidates.iter().map(String::as_str).collect(),
                    )
                })
                .collect();
            let expected: Vec<(&str, bool, Vec<&str>)> = modules
                .iter()
                .map(|(name, test, candidates)| (*name, *test, candidates.to_vec()))
                .collect();
            assert_eq!(held, expected, "{path}: {source:?}");
        }
    }

    #[test]
    fn design_and_workflow_files_yield_facts_as_written() {
        type Triples = &'static [(&'static str, &'static str, &'static str)];
        // (path, bytes, half, lang, doc concepts, doc links, workflow nodes, workflow relations,
        // whether it holds definitions)
        type Row = (
            &'static str,
            &'static [u8],
            Half,
            &'static str,
            Triples,
            Triples,
            Triples,
            Triples,
            bool,
        );
        let table: [Row; 8] = [
            (
                "docs/widget-design.md",
                b"# Widget design\n\nThe code is `src/widget.rs`.\n\n## Parts\n",
                Half::Design,
                "",
                &[
                    (KIND_DESIGN_DOC, "docs/widget-design.md", "Widget design"),
                    (KIND_DESIGN_DOC, "docs/widget-design.md#parts", "Parts"),
                ],
                &[("docs/widget-design.md", REL_SPECIFIES, "src/widget.rs")],
                &[],
                &[],
                false,
            ),
            (
                "docs/using-widget.md",
                b"# Using widget\n\nRun `widget start` to begin.\n",
                Half::Design,
                "",
                &[],
                &[],
                &[],
                &[],
                false,
            ),
            (
                "scripts/build.sh",
                b"# WHY: keep the cache warm\nmake\n",
                Half::Design,
                "",
                &[(
                    KIND_RATIONALE,
                    "scripts/build.sh#L1",
                    "WHY: keep the cache warm",
                )],
                &[("scripts/build.sh#L1", REL_EXPLAINS, "scripts/build.sh")],
                &[],
                &[],
                false,
            ),
            (
                "src/lib.rs",
                b"// WHY: the cap bounds the queue\nfn cap() {}\n",
                Half::Code,
                "rust",
                &[(
                    KIND_RATIONALE,
                    "src/lib.rs#L1",
                    "WHY: the cap bounds the queue",
                )],
                &[("src/lib.rs#L1", REL_EXPLAINS, "src/lib.rs")],
                &[],
                &[],
                true,
            ),
            (
                ".rigger/workflow.yml",
                b"stages:\n  implement:\n    agent: worker\n    gates: [fmt]\n",
                Half::Workflow,
                "",
                &[],
                &[],
                &[
                    (KIND_AGENT, "agent:worker", "worker"),
                    (KIND_GATE, "gate:fmt", "fmt"),
                    (KIND_STAGE, "stage:implement", "implement"),
                ],
                &[
                    ("stage:implement", REL_RUNS, "agent:worker"),
                    ("stage:implement", REL_RUNS, "gate:fmt"),
                ],
                false,
            ),
            (
                ".rigger/workflow.yml",
                b"stages: [unclosed\n",
                Half::Workflow,
                "",
                &[],
                &[],
                &[],
                &[],
                false,
            ),
            (
                "docs/broken-design.md",
                &[0xff, 0xfe, 0x00],
                Half::Design,
                "",
                &[],
                &[],
                &[],
                &[],
                false,
            ),
            (
                "src/broken.rs",
                &[0xff, 0xfe, 0x00],
                Half::Code,
                "rust",
                &[],
                &[],
                &[],
                &[],
                false,
            ),
        ];
        for (path, bytes, half, lang, concepts, links, nodes, relations, has_defs) in table {
            let facts = facts_of(path, bytes);
            let triples = |rows: Vec<(&str, &str, &str)>| -> Vec<(String, String, String)> {
                rows.into_iter()
                    .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
                    .collect()
            };
            let expected = |rows: Triples| triples(rows.to_vec());
            assert_eq!((facts.half, facts.lang.as_str()), (half, lang), "{path}");
            assert_eq!(
                triples(
                    facts
                        .doc_concepts
                        .iter()
                        .map(|c| (c.kind.as_str(), c.id.as_str(), c.title.as_str()))
                        .collect()
                ),
                expected(concepts),
                "{path}: doc concepts"
            );
            assert_eq!(
                triples(
                    facts
                        .doc_links
                        .iter()
                        .map(|l| (l.from.as_str(), l.rel.as_str(), l.to.as_str()))
                        .collect()
                ),
                expected(links),
                "{path}: doc links"
            );
            assert_eq!(
                triples(
                    facts
                        .wf_nodes
                        .iter()
                        .map(|n| (n.kind.as_str(), n.id.as_str(), n.title.as_str()))
                        .collect()
                ),
                expected(nodes),
                "{path}: workflow nodes"
            );
            assert_eq!(
                triples(
                    facts
                        .wf_relations
                        .iter()
                        .map(|r| (r.from.as_str(), r.rel.as_str(), r.to.as_str()))
                        .collect()
                ),
                expected(relations),
                "{path}: workflow relations"
            );
            assert_eq!(!facts.defs.is_empty(), has_defs, "{path}: definitions");
        }
    }

    #[test]
    fn the_fingerprint_changes_with_the_extractor_generation() {
        // (grammar generation, facts generation, the same pair or another, whether they differ)
        let table = [
            ("g1", "f1", "g1", "f1", false),
            ("g1", "f1", "g1", "f2", true),
            ("g1", "f1", "g2", "f1", true),
            ("g", "1f", "g1", "f", true),
        ];
        for (grammar, facts, other_grammar, other_facts, differ) in table {
            assert_eq!(
                fingerprint_of(grammar, facts) != fingerprint_of(other_grammar, other_facts),
                differ,
                "({grammar}, {facts}) against ({other_grammar}, {other_facts})"
            );
        }
        let held = FactsExtractor.fingerprint();
        assert_eq!(held, fingerprint_of(GRAMMAR_TAGS_VERSION, FACTS_GENERATION));
        assert_ne!(held, fingerprint_of(GRAMMAR_TAGS_VERSION, "facts-next"));
        assert_ne!(held, fingerprint_of("ts-tags-next", FACTS_GENERATION));
    }
}
