//! The ranked prose half: the caller's tree's prose files and the graph's records (decisions,
//! lessons, findings), ranked by relevance to the terms in an in-memory tantivy index under its
//! standard tokenizer. Never code. The index lives in this process alone and is written nowhere:
//! each call re-reads the sources, hashes each one's content and re-indexes only the sources
//! added, changed or removed since the call before, so a prose file is always ranked from the
//! bytes the caller's tree holds now.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;
use std::path::Path;

use tantivy::collector::{Count, TopDocs};
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::snippet::SnippetGenerator;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use super::budget::{PROSE_TOP, SNIPPET_CHARS};
use super::text::relative;
use super::words;
use crate::contextgraph::{ContextGraph, Node, KIND_DECISION, KIND_FINDING, KIND_LESSON};
use crate::grounder::walk_guarded;

/// The file extensions read as prose.
const PROSE_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "rst", "txt", "adoc"];

/// The directories of prose the project walk never enters because they are hidden, each walked
/// on its own: rigger's agent definitions, instruction files and skills, and the helper agents
/// `rigger setup` installs.
const HIDDEN_PROSE_DIRS: &[&str] = &[
    ".rigger/agents",
    ".rigger/instructions",
    ".rigger/skills",
    ".claude/agents",
];

/// A prose file's kind by where it lives: the first entry whose directory the path is under.
const KIND_BY_DIR: &[(&str, &str)] = &[
    ("specs", "spec"),
    (".rigger/agents", "persona"),
    (".claude/agents", "persona"),
    ("instructions", "instructions"),
    ("skills", "skill"),
];

/// A prose file anywhere else.
const KIND_DOC: &str = "doc";
/// A spec's `Done when` item.
const KIND_CRITERION: &str = "criterion";
/// The graph's record kinds ranked as prose.
const RECORD_KINDS: &[&str] = &[KIND_DECISION, KIND_LESSON, KIND_FINDING];

/// One ranked result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProseHit {
    /// `spec`, `criterion`, `persona`, `instructions`, `skill`, `doc` or a record kind.
    pub kind: String,
    /// `file:line` for a file's section, the record's id for a record.
    pub place: String,
    /// The section's heading, empty for a record.
    pub title: String,
    /// The best-matching passage.
    pub snippet: String,
}

/// The top results and how many documents matched in all, with the reason the graph's records
/// were left out of the ranking when they were.
#[derive(Debug, Default)]
pub(crate) struct Ranked {
    pub hits: Vec<ProseHit>,
    pub matched: usize,
    pub records_left_out: Option<String>,
}

/// The graph's records ranked as prose.
pub(crate) fn records(graph: &dyn ContextGraph) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    for kind in RECORD_KINDS {
        nodes.extend(graph.nodes_of_kind(kind).map_err(|e| e.to_string())?);
    }
    Ok(nodes)
}

/// One source of prose: a file of the tree or a record of the graph.
struct Source {
    key: String,
    text: String,
    kind: String,
    /// The file the source is, empty for a record.
    file: String,
}

/// The fields of the index's one schema.
#[derive(Clone, Copy)]
struct Fields {
    key: Field,
    kind: Field,
    place: Field,
    title: Field,
    body: Field,
}

/// The open in-memory index.
struct Open {
    fields: Fields,
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
}

/// The in-memory prose index and the content hash of every source it holds.
#[derive(Default)]
pub(crate) struct ProseIndex {
    open: Option<Open>,
    held: BTreeMap<String, u64>,
}

impl ProseIndex {
    /// Bring the index up to date with the prose of the tree at `root` and with `records`, then
    /// rank it for `terms`. When `records` is the reason the graph's records cannot be read, no
    /// record is ranked (any indexed before leave the index) and the reason is stated.
    pub(crate) fn rank(
        &mut self,
        root: &Path,
        terms: &[&str],
        records: Result<Vec<Node>, String>,
    ) -> Result<Ranked, String> {
        let mut sources = files(root);
        let records_left_out = match records {
            Ok(nodes) => {
                sources.extend(nodes.iter().filter_map(record));
                None
            }
            Err(reason) => Some(reason),
        };
        self.refresh(sources)
            .map_err(|e| format!("the prose index could not be brought up to date: {e}"))?;
        let mut ranked = self
            .search(terms)
            .map_err(|e| format!("the prose index could not be searched: {e}"))?;
        ranked.records_left_out = records_left_out;
        Ok(ranked)
    }

    /// Re-index exactly the sources added, changed or removed since the last refresh.
    fn refresh(&mut self, sources: Vec<Source>) -> tantivy::Result<()> {
        let open = match &mut self.open {
            Some(open) => open,
            None => self.open.insert(open_index()?),
        };
        let mut seen = BTreeSet::new();
        let mut dirty = false;
        for source in sources {
            let hash = content_hash(&source.text);
            seen.insert(source.key.clone());
            if self.held.get(&source.key) == Some(&hash) {
                continue;
            }
            open.writer
                .delete_term(Term::from_field_text(open.fields.key, &source.key));
            for section in sections(&source) {
                open.writer.add_document(doc!(
                    open.fields.key => source.key.as_str(),
                    open.fields.kind => section.kind,
                    open.fields.place => section.place,
                    open.fields.title => section.title,
                    open.fields.body => section.body,
                ))?;
            }
            self.held.insert(source.key, hash);
            dirty = true;
        }
        let gone: Vec<String> = self
            .held
            .keys()
            .filter(|key| !seen.contains(*key))
            .cloned()
            .collect();
        for key in gone {
            open.writer
                .delete_term(Term::from_field_text(open.fields.key, &key));
            self.held.remove(&key);
            dirty = true;
        }
        if dirty {
            open.writer.commit()?;
            open.reader.reload()?;
        }
        Ok(())
    }

    /// The top [`PROSE_TOP`] sections for `terms`' words, any of them, a heading match weighing
    /// double.
    fn search(&self, terms: &[&str]) -> tantivy::Result<Ranked> {
        let Some(open) = &self.open else {
            return Ok(Ranked::default());
        };
        let query_words: Vec<String> = terms.iter().flat_map(|t| words(t)).collect();
        if query_words.is_empty() {
            return Ok(Ranked::default());
        }
        let mut parser =
            QueryParser::for_index(&open.index, vec![open.fields.title, open.fields.body]);
        parser.set_field_boost(open.fields.title, 2.0);
        let query = parser
            .parse_query(&query_words.join(" "))
            .map_err(|e| tantivy::TantivyError::InvalidArgument(e.to_string()))?;
        let searcher = open.reader.searcher();
        let (top, matched) = searcher.search(
            &query,
            &(TopDocs::with_limit(PROSE_TOP).order_by_score(), Count),
        )?;
        let mut snippets = SnippetGenerator::create(&searcher, &*query, open.fields.body)?;
        snippets.set_max_num_chars(SNIPPET_CHARS);
        let mut hits = Vec::with_capacity(top.len());
        for (_, address) in top {
            let found: TantivyDocument = searcher.doc(address)?;
            let text = |field: Field| {
                found
                    .get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let fragment = snippets.snippet_from_doc(&found).fragment().to_string();
            let snippet = if fragment.trim().is_empty() {
                text(open.fields.body)
            } else {
                fragment
            };
            hits.push(ProseHit {
                kind: text(open.fields.kind),
                place: text(open.fields.place),
                title: text(open.fields.title),
                snippet: one_line(&snippet),
            });
        }
        Ok(Ranked {
            hits,
            matched,
            records_left_out: None,
        })
    }
}

/// A fresh in-memory index under the one schema.
fn open_index() -> tantivy::Result<Open> {
    let mut schema = Schema::builder();
    let fields = Fields {
        key: schema.add_text_field("key", STRING),
        kind: schema.add_text_field("kind", STRING | STORED),
        place: schema.add_text_field("place", STORED),
        title: schema.add_text_field("title", TEXT | STORED),
        body: schema.add_text_field("body", TEXT | STORED),
    };
    let index = Index::create_in_ram(schema.build());
    let writer = index.writer_with_num_threads(1, 32_000_000)?;
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    Ok(Open {
        fields,
        index,
        writer,
        reader,
    })
}

/// The tree's prose files: every file the project walk visits with a prose extension, and every
/// such file under a hidden prose directory.
fn files(root: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    let mut take = |path: &Path| {
        let is_prose = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| PROSE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
        if is_prose {
            if let Ok(text) = std::fs::read_to_string(path) {
                let file = relative(root, path);
                out.push(Source {
                    key: format!("file:{file}"),
                    kind: kind_of(&file).to_string(),
                    text,
                    file,
                });
            }
        }
        ControlFlow::Continue(())
    };
    let _ = walk_guarded(root, &mut take);
    for dir in HIDDEN_PROSE_DIRS {
        let dir = root.join(dir);
        if dir.is_dir() {
            let _ = walk_guarded(&dir, &mut take);
        }
    }
    out
}

/// The kind of the prose file at `file`.
fn kind_of(file: &str) -> &'static str {
    KIND_BY_DIR
        .iter()
        .find(|(dir, _)| file.starts_with(&format!("{dir}/")) || file.contains(&format!("/{dir}/")))
        .map_or(KIND_DOC, |(_, kind)| kind)
}

/// The source a record node is, when it carries a summary.
fn record(node: &Node) -> Option<Source> {
    let summary = node.attrs.get("summary").filter(|s| !s.trim().is_empty())?;
    Some(Source {
        key: format!("record:{}", node.id),
        text: summary.clone(),
        kind: node.kind.clone(),
        file: String::new(),
    })
}

/// One indexed document.
struct Section {
    kind: String,
    place: String,
    title: String,
    body: String,
}

/// The documents `source` is indexed as. A record is one. A file is one per heading section
/// (Markdown `#` headings outside fenced code; the text before the first heading is a section of
/// its own), and a spec's `Done when` section is one document per top-level list item, each of
/// kind `criterion`.
fn sections(source: &Source) -> Vec<Section> {
    if source.file.is_empty() {
        return vec![Section {
            kind: source.kind.clone(),
            place: source.key.trim_start_matches("record:").to_string(),
            title: String::new(),
            body: source.text.clone(),
        }];
    }
    let mut out = Vec::new();
    let mut fenced = false;
    let mut title = String::new();
    let mut start = 1;
    let mut body: Vec<&str> = Vec::new();
    for (i, line) in source.text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with('#') {
            push_section(&mut out, source, &title, start, &body);
            title = line.trim_start_matches('#').trim().to_string();
            start = i + 1;
            body.clear();
            continue;
        }
        body.push(line);
    }
    push_section(&mut out, source, &title, start, &body);
    out
}

/// Push the section titled `title` at line `start` with lines `body`, split into criteria when it
/// is a spec's `Done when`.
fn push_section(out: &mut Vec<Section>, source: &Source, title: &str, start: usize, body: &[&str]) {
    let place = |line: usize| format!("{}:{line}", source.file);
    if source.kind == "spec" && title.eq_ignore_ascii_case("done when") {
        let mut item: Option<(usize, Vec<&str>)> = None;
        for (offset, line) in body.iter().enumerate() {
            let top = line.starts_with("- ") || line.starts_with("* ");
            if top {
                if let Some((at, lines)) = item.take() {
                    out.push(criterion(place(at), title, &lines));
                }
                item = Some((start + offset + 1, vec![line]));
            } else if let Some((_, lines)) = &mut item {
                lines.push(line);
            }
        }
        if let Some((at, lines)) = item {
            out.push(criterion(place(at), title, &lines));
        }
        return;
    }
    let text = body.join("\n");
    if text.trim().is_empty() && title.is_empty() {
        return;
    }
    out.push(Section {
        kind: source.kind.clone(),
        place: place(start),
        title: title.to_string(),
        body: text,
    });
}

/// One criterion document.
fn criterion(place: String, title: &str, lines: &[&str]) -> Section {
    Section {
        kind: KIND_CRITERION.to_string(),
        place,
        title: title.to_string(),
        body: lines.join("\n"),
    }
}

/// `text`'s content hash, stable within this process.
fn content_hash(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// `text` on one line, its whitespace runs collapsed.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
