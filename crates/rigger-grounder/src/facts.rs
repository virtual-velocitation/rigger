//! Extraction to facts: the [`Extract`] implementation that reads one file's path and bytes into
//! its [`FileFacts`]. It re-shapes what the per-language symbol extractors, the design-document
//! extractor and the workflow-definition extractor already find; it is no extractor of its own,
//! and holds no language knowledge. Every name is kept as written and nothing is resolved across
//! files, so a file's facts are a pure function of its path and bytes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, WfNode,
    WfRelation,
};
use rigger_domain::code_view::{CodeView, Entry, Extract, Head, OpenCodeView, TreeSource};
use rigger_domain::contextgraph::definition_precedence;
use rigger_domain::groupings::ProductionGroupings;
use rigger_domain::playbooks::fnv1a_64;
use rigger_store::code::{fingerprint, open_current, DirTreeSource, Mode, Opened};

use crate::config::{RIGGER_DIR, WORKFLOW_FILE};
use crate::config_store;
use crate::grounder::design::extract::{extract_concepts, extract_links};
use crate::grounder::symbols::extract::{kind_str, lang_str};
use crate::grounder::symbols::model::{FileSymbols, GRAMMAR_TAGS_VERSION};
use crate::grounder::symbols::paths::{is_dev_only, module_candidates};
use crate::grounder::symbols::{extracted, registry};
use crate::grounder::workflowdef;

/// The generation of everything this module adds to the extractors it re-shapes: the design and
/// workflow extractors, which carry no generation of their own, and the shape of the facts. Bump
/// it whenever any of them can give a file other facts than before, as the symbol extractors'
/// [`GRAMMAR_TAGS_VERSION`] is bumped for theirs; the two together make the extractor's fingerprint,
/// which the store's fingerprint ([`store_fingerprint`]) folds with the grouper's generation. A golden
/// test pins it beside the digest of the facts read over a fixture set, so a change that is not
/// bumped fails there.
const FACTS_GENERATION: &str = "facts-v2";

/// The fingerprint of an extractor built from these generations: a stable hash, since a snapshot
/// stores it and a later process compares against it.
fn fingerprint_of(grammar_generation: &str, facts_generation: &str) -> u64 {
    fnv1a_64(format!("{grammar_generation}\n{facts_generation}").as_bytes())
}

/// The fingerprint every snapshot of the store [`open_store`] opens is stamped with: the
/// extractor's and the grouper's folded, as the advance folds them.
pub fn store_fingerprint() -> u64 {
    fingerprint(&FactsExtractor, &ProductionGroupings)
}

/// The code store at `root` opened as `mode` says over `source`: its facts read by
/// [`FactsExtractor`] and grouped by [`ProductionGroupings`]. Every opener of the store composes it
/// here.
pub fn open_store(root: &Path, source: &dyn TreeSource, mode: Mode) -> Result<Opened, String> {
    open_current(root, source, &FactsExtractor, &ProductionGroupings, mode)
}

/// The code store of a project with no git, read from the directory at its root.
pub struct DirectoryHead {
    root: PathBuf,
}

impl DirectoryHead {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl OpenCodeView for DirectoryHead {
    fn head(&self, worktree: &Path) -> Result<Head, String> {
        let source = DirTreeSource::new(&self.root);
        let opened = open_store(&self.root, &source, Mode::Read)?;
        let diff = source.changed_against_worktree(&opened.view.stamp().tip.tree, worktree);
        Ok(Head {
            diff,
            behind: opened.behind.as_ref().map(ToString::to_string),
            view: Box::new(opened.view),
        })
    }
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

/// Fills the definitions, mentions, module declarations and parse-error marker of `facts` from the symbols the
/// extractor found in the file at `path`.
fn code_facts(path: &str, symbols: &FileSymbols, facts: &mut FileFacts) {
    facts.partial = symbols.partial;
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
            def: by_path[d.path.as_str()],
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

/// The workflow definition's path relative to a project root, its ONE spelling: the path whose
/// bytes the code store's extractor reads as the workflow definition.
fn workflow_doc() -> String {
    format!("{RIGGER_DIR}/{WORKFLOW_FILE}")
}

/// The text of a file's `bytes` as an extraction half reads it: none for no bytes and for bytes
/// that are not UTF-8, the two inputs every half maps to no facts.
fn text_of(bytes: Option<&[u8]>) -> Option<&str> {
    bytes.and_then(|bytes| std::str::from_utf8(bytes).ok())
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

    /// ONE PATH IS ONE ENTITY (spec 119 criterion 10): over each file's real extraction, the
    /// resolver settles one entity per path, the first by `CODE_KIND_PRECEDENCE` then the lower
    /// line, with the winner's own kind, line and `CONTAINS` parent: `fn x` over `mod x;`, the
    /// first of two cfg twins and of two impls with one header, the production `fn x` over a
    /// `#[cfg(test)]` twin at a lower line, the function of `const f = function f() {}`, the field
    /// `B` of `class A { B }` over a `namespace A.B` above it, the function of a class field
    /// `f = function f() {}` under the field's class, and of a method's twins in two impls the
    /// lower line's, then the other's once that one is gone.
    #[test]
    fn one_path_is_one_entity_the_precedence_winner_under_its_own_parent() {
        use crate::js_ts_definition_sources::NAMESPACE_A_B_BESIDE_CLASS_A_FIELD_B;
        use crate::rust_definition_sources::{
            METHOD_C_IN_THE_FILE_LEVEL_IMPL, METHOD_C_IN_TWO_IMPLS,
        };
        use rigger_domain::code_facts::{Stamp, Tip};
        use rigger_domain::code_view::FactsView;
        use rigger_domain::contextgraph::REL_CONTAINS;
        use rigger_domain::resolve::Resolver;

        // (file, source, the id fragment the compared entities hold, "kind id@line in parent")
        let cases: [(&str, &str, &str, &[&str]); 9] = [
            (
                "a.rs",
                "mod x;\nfn x() {}\n",
                "",
                &["function a.rs::x@2 in a.rs"],
            ),
            (
                "a.rs",
                "#[cfg(unix)]\nfn x() {}\n#[cfg(not(unix))]\nfn x() {}\n",
                "",
                &["function a.rs::x@2 in a.rs"],
            ),
            (
                "a.rs",
                "struct T;\nimpl T {}\nimpl T {}\n",
                "",
                &["type a.rs::T@1 in a.rs", "impl a.rs::impl T@2 in a.rs"],
            ),
            (
                "a.rs",
                "#[cfg(test)]\nfn x() {}\nfn x() {}\n",
                "",
                &["function a.rs::x@3 in a.rs"],
            ),
            (
                "c.ts",
                "const f = function f() {};\n",
                "",
                &["function c.ts::f@1 in c.ts"],
            ),
            (
                "c.ts",
                NAMESPACE_A_B_BESIDE_CLASS_A_FIELD_B,
                "",
                &["type c.ts::A@2 in c.ts", "field c.ts::A.B@3 in c.ts::A"],
            ),
            (
                "c.ts",
                "class A {\n    f = function f() {};\n}\n",
                "",
                &["type c.ts::A@1 in c.ts", "function c.ts::A::f@2 in c.ts::A"],
            ),
            (
                "a.rs",
                METHOD_C_IN_TWO_IMPLS,
                "::c",
                &["method a.rs::a::B::c@4 in a.rs::a::impl B"],
            ),
            (
                "a.rs",
                METHOD_C_IN_THE_FILE_LEVEL_IMPL,
                "::c",
                &["method a.rs::a::B::c@8 in a.rs::impl a::B"],
            ),
        ];
        let stamp = Stamp {
            tip: Tip {
                head: "main".into(),
                commit: None,
                tree: "fixture-tree".into(),
            },
            fingerprint: 0,
            seq: 1,
        };
        for (file, source, held, winners) in cases {
            let facts = facts_of(file, source.as_bytes());
            let view = FactsView::new(stamp.clone(), vec![facts.clone()]);
            let resolver = Resolver::new(&view);
            let parents: HashMap<String, String> = resolver
                .links(&facts, true)
                .into_iter()
                .filter(|l| l.rel == REL_CONTAINS)
                .map(|l| (l.to, l.from))
                .collect();
            let entities: Vec<String> = resolver
                .entities(&facts, true)
                .into_iter()
                .filter(|e| e.id.contains(held))
                .map(|e| {
                    let parent = parents.get(&e.id).map_or("-", String::as_str);
                    format!("{} {}@{} in {parent}", e.kind, e.id, e.line)
                })
                .collect();
            assert_eq!(entities, winners, "{source:?}: one entity per path");
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
    fn a_file_the_parser_cannot_fully_read_is_partial() {
        let table = [
            (
                "src/lib.rs",
                "fn whole() { helper(); }\nfn helper() {}\n",
                false,
            ),
            (
                "src/lib.rs",
                "fn whole() { helper(); }\nfn broken( {\n",
                true,
            ),
            ("web/app.ts", "function broken( {\n", true),
            ("docs/a-design.md", "# Design\n", false),
        ];
        for (path, source, partial) in table {
            assert_eq!(facts_of(path, source.as_bytes()).partial, partial, "{path}");
        }
    }

    #[test]
    fn scope_follows_the_dev_only_path_rule() {
        // Every DEV-ONLY PATHS pattern at its gates and edges is test scope; every production
        // control, and the nearest path each pattern does not name, is product.
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
            ("tests/foo.rs", Scope::Test),
            ("crate/benches/b.rs", Scope::Test),
            ("examples/demo/main.rs", Scope::Test),
            ("web/__tests__/a.js", Scope::Test),
            ("app/test_views.py", Scope::Test),
            ("app/views_test.py", Scope::Test),
            ("shim/shim.test.mjs", Scope::Test),
            ("web/a.spec.ts", Scope::Test),
            ("Acme.Test/A.cs", Scope::Test),
            ("Acme.Tests/A.cs", Scope::Test),
            ("web/a.test.js", Scope::Test),
            ("web/a.test.cjs", Scope::Test),
            ("web/a.test.jsx", Scope::Test),
            ("web/a.test.tsx", Scope::Test),
            ("web/a.test.d.ts", Scope::Test),
            ("web/.spec.ts", Scope::Test),
            ("pkg/_test.go", Scope::Test),
            ("app/test_.py", Scope::Test),
            ("app/_test.py", Scope::Test),
            ("FooTest/A.cs", Scope::Test),
            ("FooTests/Sub/A.cs", Scope::Test),
            ("Test/A.cs", Scope::Test),
            ("a/b/tests/c/d.rs", Scope::Test),
            ("benches/b.go", Scope::Test),
            ("tests/notes.txt", Scope::Test),
            ("examples/README", Scope::Test),
            ("src/lib.rs", Scope::Product),
            ("src/tests.rs", Scope::Product),
            ("src/testsuite.rs", Scope::Product),
            ("pkg/x_test.rs", Scope::Product),
            ("web/a.test.rs", Scope::Product),
            ("pkg/testing.py", Scope::Product),
            ("contest/Case.cs", Scope::Product),
            ("App/AppTests.cs", Scope::Product),
            ("build.rs", Scope::Product),
            ("Contest/a.cs", Scope::Product),
            ("LoadTest/run.py", Scope::Product),
            ("FooTests/lib.rs", Scope::Product),
            ("src/a.test.rs", Scope::Product),
            ("src/a.spec.rs", Scope::Product),
            ("src/benches.rs", Scope::Product),
            ("src/examples.rs", Scope::Product),
            ("src/__tests__.js", Scope::Product),
            ("src/tests", Scope::Product),
            ("testing/a.rs", Scope::Product),
            ("mytests/a.rs", Scope::Product),
            ("pkg/xtest.go", Scope::Product),
            ("pkg/test_x.go", Scope::Product),
            ("pkg/x.test.go", Scope::Product),
            ("pkg/x_test.go.txt", Scope::Product),
            ("app/test.py", Scope::Product),
            ("app/tests_a.py", Scope::Product),
            ("app/attest.py", Scope::Product),
            ("app/a_test_b.py", Scope::Product),
            ("app/a.spec.py", Scope::Product),
            ("web/test.js", Scope::Product),
            ("web/spec.ts", Scope::Product),
            ("web/a_test.js", Scope::Product),
            ("web/test_a.ts", Scope::Product),
            ("web/atest.spec", Scope::Product),
            ("src/WidgetTests.cs", Scope::Product),
            ("Acme.Tests.Core/A.cs", Scope::Product),
            ("Testing/A.cs", Scope::Product),
            ("Acme.tests/A.cs", Scope::Product),
            ("Acme.Tests/.cs", Scope::Product),
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
        let table: [Row; 10] = [
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
            (
                "src/lib.rs",
                "#![cfg(test)]\nfn first() {}\nmod x;\n",
                &[("x", true, &["src/x.rs", "src/x/mod.rs"])],
            ),
            (
                "src/main.rs",
                "#[cfg(test)]\nuse y;\nfn first() {}\nmod z;\n",
                &[("z", false, &["src/z.rs", "src/z/mod.rs"])],
            ),
            (
                "src/lib.rs",
                "mod outer {\n    #[cfg(test)]\n    mod x;\n}\n",
                &[("x", true, &["src/outer/x.rs", "src/outer/x/mod.rs"])],
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
        let table: [Row; 9] = [
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
                ".rigger/workflow.yml",
                &[0xff, 0xfe, 0x00],
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

    /// A CHANGE TO THE FACTS SHIPS WITH A BUMP: the golden fixtures - the symbol golden's source
    /// for each registered grammar, a design document, a source file's rationale and a workflow
    /// definition - read into facts whose digest is pinned beside the two generations the
    /// fingerprint is made of, so a change to the design extractor, the workflow extractor or the
    /// conversion fails here until the generation is bumped and the triple re-pinned.
    #[test]
    fn a_change_to_the_facts_ships_with_a_generation_bump() {
        use crate::grounder::symbols::registry::tests::GOLDEN_SOURCES;
        const PINNED: (&str, &str, &str) = ("ts-tags-v5", "facts-v2", "ec3d0eb0685882b8");
        let mut fixtures: Vec<(String, &str)> = GOLDEN_SOURCES
            .iter()
            .enumerate()
            .map(|(i, (ext, source))| (format!("golden/{i}.{ext}"), *source))
            .collect();
        fixtures.extend(
            [
                ("tests/it.rs", "fn works() { helper(); }\n"),
                (
                    "docs/widget-design.md",
                    "# Widget design\n\nThe code is `src/widget.rs`, see [notes](docs/notes.md).\n\n## Parts\n",
                ),
                ("scripts/build.sh", "# WHY: keep the cache warm\nmake\n"),
                (
                    ".rigger/workflow.yml",
                    "stages:\n  plan:\n    agent: planner\n  implement:\n    needs: [plan]\n    agent: worker\n    gates: [fmt]\n",
                ),
            ]
            .map(|(path, source)| (path.to_string(), source)),
        );
        let facts: Vec<FileFacts> = fixtures
            .iter()
            .map(|(path, source)| facts_of(path, source.as_bytes()))
            .collect();
        let any = |held: &dyn Fn(&FileFacts) -> bool| facts.iter().any(held);
        for (part, covered) in [
            (
                "every half",
                [Half::Code, Half::Design, Half::Workflow]
                    .iter()
                    .all(|h| any(&|f| f.half == *h)),
            ),
            (
                "every language",
                ["rust", "csharp", "js", "ts", "go", "python"]
                    .iter()
                    .all(|l| any(&|f| f.lang == *l)),
            ),
            ("a test scope", any(&|f| f.scope == Scope::Test)),
            ("a partial file", any(&|f| f.partial)),
            ("a test definition", any(&|f| f.defs.iter().any(|d| d.test))),
            (
                "a definition's parent",
                any(&|f| f.defs.iter().any(|d| d.parent.is_some())),
            ),
            (
                "a test mention",
                any(&|f| f.mentions.iter().any(|m| m.test)),
            ),
            (
                "a tagged mention",
                any(&|f| {
                    f.mentions
                        .iter()
                        .any(|m| matches!(m.kind, MentionKind::Tagged(_)))
                }),
            ),
            (
                "a mention's caller",
                any(&|f| f.mentions.iter().any(|m| m.enclosing.is_some())),
            ),
            ("a module declaration", any(&|f| !f.modules.is_empty())),
            ("a design concept", any(&|f| !f.doc_concepts.is_empty())),
            ("a design link", any(&|f| !f.doc_links.is_empty())),
            ("a workflow node", any(&|f| !f.wf_nodes.is_empty())),
            ("a workflow relation", any(&|f| !f.wf_relations.is_empty())),
        ] {
            assert!(covered, "the golden fixtures hold {part}");
        }
        let digest = format!("{:016x}", fnv1a_64(format!("{facts:#?}").as_bytes()));
        assert_eq!(
            (GRAMMAR_TAGS_VERSION, FACTS_GENERATION, digest.as_str()),
            PINNED,
            "this build's generations and the digest of the golden facts (left) differ from the \
             pinned triple: a change to what the facts hold ships with a bump, so bump \
             `FACTS_GENERATION` (or `GRAMMAR_TAGS_VERSION` for a symbol extraction change) and \
             re-pin the triple to this build's values together"
        );
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
