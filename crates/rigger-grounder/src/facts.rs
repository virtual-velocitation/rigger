//! Extraction to facts: the [`Extract`] implementations that read one file's path and bytes into
//! its [`FileFacts`]. They re-shape what the per-language symbol extractors, the design-document
//! extractor and the workflow-definition extractor already find, and what an extractor the
//! project's workflow declares answers over THE EXTRACTOR PROTOCOL (spec 125); this module is no
//! extractor of its own, and holds no language knowledge. Every name is kept as written and
//! nothing is resolved across files, so a file's facts are a function of its path and bytes alone
//! under the extractor's fingerprint, a declared extractor being trusted, as a grammar is, to
//! answer the same request the same way. The store's openers compose [`ProjectExtractor`], the
//! bundled extraction under the extractors the project's workflow declares, whose fingerprint
//! changes with the declaration.

use std::collections::{btree_map, BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::{Mutex, PoisonError};

use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, WfNode,
    WfRelation,
};
use rigger_domain::code_view::{Entry, Extract, Head, OpenCodeView, TreeSource};
use rigger_domain::contextgraph::{
    definition_precedence, REF_KIND_CONST, REF_KIND_TYPE, REF_KIND_USE,
};
use rigger_domain::groupings::ProductionGroupings;
use rigger_domain::playbooks::fnv1a_64;
use rigger_process::{subprocess, whole_line};
use rigger_store::code::{fingerprint, open_current, open_head, DirTreeSource, Memo, Mode, Opened};
use serde::{Deserialize, Serialize};

use crate::config::{check_extractors, ExtractorDecl, RIGGER_DIR, WORKFLOW_FILE};
use crate::config_store;
use crate::grounder::design::extract::{extract_concepts, extract_links};
use crate::grounder::symbols::extract::{kind_str, lang_str};
use crate::grounder::symbols::model::{self, SymRef, GRAMMAR_TAGS_VERSION};
use crate::grounder::symbols::paths::{is_dev_only, module_candidates};
use crate::grounder::symbols::{content_hash, extracted, registry};
use crate::grounder::workflowdef;

/// The generation of everything this module adds to the extractors it re-shapes: the design and
/// workflow extractors, which carry no generation of their own, and the shape of the facts. Bump
/// it whenever any of them can give a file other facts than before, as the symbol extractors'
/// [`GRAMMAR_TAGS_VERSION`] is bumped for theirs; the two together make the extractor's fingerprint,
/// with the digest of the project's declared extractors when it declares any ([`ProjectExtractor`]),
/// which the store's fingerprint ([`store_fingerprint`]) folds with the grouper's generation. A golden
/// test pins it beside the digest of the facts read over a fixture set, so a change that is not
/// bumped fails there.
const FACTS_GENERATION: &str = "facts-v2";

/// The fingerprint of an extractor built from these generations: a stable hash, since a snapshot
/// stores it and a later process compares against it.
fn fingerprint_of(grammar_generation: &str, facts_generation: &str) -> u64 {
    fnv1a_64(format!("{grammar_generation}\n{facts_generation}").as_bytes())
}

/// The fingerprint every snapshot of the store [`open_store`] opens at `root` is stamped with: the
/// project's extractor's ([`ProjectExtractor::of`]) and the grouper's folded, as the advance folds
/// them.
pub fn store_fingerprint(root: &Path) -> u64 {
    fingerprint(&ProjectExtractor::of(root), &ProductionGroupings)
}

/// The code store at `root` opened as `mode` says over `source`: its facts read by the project's
/// extractor ([`ProjectExtractor::of`] its root) and grouped by [`ProductionGroupings`]. Every
/// opener of the store composes it here.
pub fn open_store(root: &Path, source: &dyn TreeSource, mode: Mode) -> Result<Opened, String> {
    open_current(
        root,
        source,
        &ProjectExtractor::of(root),
        &ProductionGroupings,
        mode,
    )
}

/// The head of the code store at `root` read for a reader in `worktree`, overlaid with the files
/// the worktree differs in ([`open_head`]), composed as [`open_store`] composes it, its
/// differing files' facts memoised for the life of the process and on disk under the store
/// ([`Memo::process`]).
pub fn read_head(
    root: &Path,
    source: &(dyn TreeSource + Sync),
    worktree: &Path,
) -> Result<Head, String> {
    open_head(
        root,
        source,
        &ProjectExtractor::of(root),
        &ProductionGroupings,
        worktree,
        Memo::process(),
    )
}

/// The extractors the workflow of the project at `root` declares (spec 125, THE EXTRACTORS KEY):
/// the code store's one read of the map. A workflow that cannot be read or fails its check
/// declares none, said on stderr as `rigger: extractors: {error}` in one write ([`whole_line`]),
/// so the bundled grammars proceed alone.
pub fn declared_extractors(root: &Path) -> BTreeMap<String, ExtractorDecl> {
    read_declared_extractors(root).unwrap_or_else(|error| {
        whole_line::say(format_args!("rigger: extractors: {error}"));
        BTreeMap::new()
    })
}

/// The extractors the workflow at `root` declares, read through [`config_store::load_workflow`]
/// and checked by [`check_extractors`]: the empty map when there is no workflow, and why when it
/// cannot be read, does not parse or declares a malformed extractor.
fn read_declared_extractors(root: &Path) -> Result<BTreeMap<String, ExtractorDecl>, String> {
    let path = root.join(workflow_doc());
    if !path
        .try_exists()
        .map_err(|e| format!("read workflow: {e}"))?
    {
        return Ok(BTreeMap::new());
    }
    let workflow = config_store::load_workflow(&path).map_err(|e| e.0)?;
    check_extractors(&workflow.extractors)?;
    Ok(workflow.extractors)
}

/// The extractor of one project: the bundled grammars ([`FactsExtractor`]) and the extractors its
/// workflow declares ([`declared_extractors`]), each declared one asked through a session of its
/// own (THE EXTRACTOR PROTOCOL, spec 125).
#[derive(Debug)]
pub struct ProjectExtractor {
    root: PathBuf,
    declared: BTreeMap<String, ExtractorDecl>,
    /// The open session of each declared extractor, by its name, behind one lock: the store reads
    /// files on several threads, and a session serves one file at a time.
    sessions: Mutex<BTreeMap<String, Session>>,
}

impl ProjectExtractor {
    /// The extractor of the project at `root`, holding the extractors its workflow declares and no
    /// session yet: a declared extractor starts, in `root`, at the first file of its extensions.
    pub fn of(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            declared: declared_extractors(root),
            sessions: Mutex::default(),
        }
    }

    /// The name and declaration of the extractor whose extensions list the extension of the file
    /// at `path`, as written; the check lets no two extractors list one extension.
    fn declaration_of(&self, path: &str) -> Option<(&str, &ExtractorDecl)> {
        let extension = registry::extension(path)?;
        self.declared
            .iter()
            .find(|(_, decl)| decl.extensions.iter().any(|listed| listed == extension))
            .map(|(name, decl)| (name.as_str(), decl))
    }

    /// The symbols the declared extractor `name`, run as `run`, answers for the file at `path`
    /// holding `text`, asked through its session, started first when it has none. A failure -
    /// the session cannot start, the request cannot be written, the answer cannot be read or is
    /// refused ([`symbols_answered`]) - says `rigger: extractor {name}: {path}: {why}` on stderr in
    /// one write ([`whole_line`]), closes the session, so the next file of its extensions starts a
    /// new one, and gives none.
    fn declared_symbols(
        &self,
        name: &str,
        run: &str,
        path: &str,
        text: &str,
    ) -> Option<CodeSymbols> {
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let answered = match sessions.entry(name.to_string()) {
            btree_map::Entry::Occupied(open) => Ok(open.into_mut()),
            btree_map::Entry::Vacant(none) => {
                Session::start(run, &self.root).map(|session| none.insert(session))
            }
        }
        .and_then(|session| session.ask(&request_line(path, text)))
        .and_then(|line| symbols_answered(path, text, &line));
        match answered {
            Ok(symbols) => Some(symbols),
            Err(why) => {
                whole_line::say(format_args!("rigger: extractor {name}: {path}: {why}"));
                if let Some(session) = sessions.remove(name) {
                    session.close();
                }
                None
            }
        }
    }
}

impl Drop for ProjectExtractor {
    /// Closes every session the extractor still holds.
    fn drop(&mut self) {
        let sessions = self
            .sessions
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        for session in std::mem::take(sessions).into_values() {
            session.close();
        }
    }
}

impl Extract for ProjectExtractor {
    /// [`FactsExtractor`]'s with no declaration, so a store built before any is declared stays in
    /// force; with any, the grammar generation carries the digest of the declared map's JSON in
    /// name order, so a changed declaration is a changed extractor.
    fn fingerprint(&self) -> u64 {
        if self.declared.is_empty() {
            return FactsExtractor.fingerprint();
        }
        let json = serde_json::to_string(&self.declared)
            .expect("a map of strings to strings and lists of strings serializes");
        let digest = content_hash(&json);
        fingerprint_of(
            &format!("{GRAMMAR_TAGS_VERSION}+{digest}"),
            FACTS_GENERATION,
        )
    }

    /// A file whose extension a declaration lists is code in the declared extractor's name, its
    /// symbols the extractor's answer ([`ProjectExtractor::declared_symbols`]) - none for bytes
    /// that are not UTF-8 or a failed answer - and the workflow definition and every other file
    /// read as [`FactsExtractor`] reads them; a declaration named as a bundled language
    /// (`rust`) replaces its grammar for the extensions it lists.
    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
        file_facts(entry, bytes, |path, text| match self.declaration_of(path) {
            Some((name, decl)) => Some(CodeFile {
                lang: name.to_string(),
                symbols: text.and_then(|text| self.declared_symbols(name, &decl.run, path, text)),
            }),
            None => bundled_code(path, text),
        })
    }
}

/// One running declared extractor: the child and the ends of its input and output rigger holds.
#[derive(Debug)]
struct Session {
    child: Child,
    requests: ChildStdin,
    answers: BufReader<ChildStdout>,
}

impl Session {
    /// The extractor `run` started in `dir` through the host's shell
    /// ([`subprocess::shell_command`]), its input and output piped to rigger and its stderr
    /// rigger's own; why not when it cannot start.
    fn start(run: &str, dir: &Path) -> Result<Self, String> {
        let mut child = subprocess::shell_command(run, dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("start: {e}"))?;
        let requests = child.stdin.take().expect("the extractor's input is piped");
        let answers = child
            .stdout
            .take()
            .expect("the extractor's output is piped");
        Ok(Self {
            child,
            requests,
            answers: BufReader::new(answers),
        })
    }

    /// The extractor's answer to the request line `request`, written and flushed: the next line
    /// of its output, its line ending dropped; why not when the request cannot be written, its
    /// output ends first or the answer is not text.
    fn ask(&mut self, request: &str) -> Result<String, String> {
        self.requests
            .write_all(format!("{request}\n").as_bytes())
            .and_then(|()| self.requests.flush())
            .map_err(|e| format!("write the request: {e}"))?;
        let mut answer = String::new();
        match self.answers.read_line(&mut answer) {
            Ok(0) => Err("end of output".to_string()),
            Ok(_) => Ok(answer.trim_end_matches(['\n', '\r']).to_string()),
            Err(e) => Err(format!("read the answer: {e}")),
        }
    }

    /// Ends the session: closes the extractor's input, so one reading it sees its input end, and
    /// its output, so one still writing fails its next write, then waits for it to exit. Its exit
    /// status is no part of the protocol, so a wait that fails leaves nothing more to do.
    fn close(self) {
        let Self {
            mut child,
            requests,
            answers,
        } = self;
        drop(requests);
        drop(answers);
        let _ = child.wait();
    }
}

/// One request of THE EXTRACTOR PROTOCOL: the file's root-relative path, `/`-separated, and its
/// text.
#[derive(Serialize)]
struct Request<'a> {
    path: &'a str,
    text: &'a str,
}

/// The request line for the file at `path` holding `text`: one line of JSON, every line break of
/// the text escaped.
fn request_line(path: &str, text: &str) -> String {
    serde_json::to_string(&Request { path, text }).expect("two strings serialize")
}

/// One answer of THE EXTRACTOR PROTOCOL as it parses: the path it is for, and the error the
/// extractor met or the file's definitions and references in the bundled extraction's own forms,
/// with whether they cover only part of the file.
#[derive(Deserialize)]
struct Answer {
    path: String,
    error: Option<String>,
    defs: Option<Vec<model::Def>>,
    refs: Option<Vec<SymRef>>,
    #[serde(default)]
    partial: bool,
}

/// The symbols the answer line `line` gives the request for the file at `path` holding `text`,
/// or why THE EXTRACTOR PROTOCOL refuses it. Checked in order: the line parses as an answer; it is
/// for `path`; it holds no error; it holds both lists; each def has a name and a path, a line from
/// 1 to the text's last - one more than the `\n` the text holds - and an end line from its line to
/// the last, and none of the out-of-line module fields only the bundled Rust grammar sets; each
/// ref has a name, a line from 1 to the last and, when it has a kind, one of `const`, `type` and
/// `use`; and no def path leads back to itself through the parents ([`parent_cycle`]).
fn symbols_answered(path: &str, text: &str, line: &str) -> Result<CodeSymbols, String> {
    let answer: Answer =
        serde_json::from_str(line).map_err(|e| format!("the answer does not parse: {e}"))?;
    if answer.path != path {
        return Err(format!("the answer is for {:?}", answer.path));
    }
    if let Some(error) = answer.error {
        return Err(format!("the answer is an error: {error}"));
    }
    let (Some(defs), Some(refs)) = (answer.defs, answer.refs) else {
        return Err("the answer holds no error and lacks defs or refs".to_string());
    };
    let last = text.matches('\n').count() as u32 + 1;
    for (i, def) in defs.iter().enumerate() {
        if def.name.is_empty() {
            return Err(format!("defs[{i}]: the name is empty"));
        }
        if def.path.is_empty() {
            return Err(format!("defs[{i}]: the path is empty"));
        }
        if !(1..=last).contains(&def.line) {
            return Err(format!(
                "defs[{i}]: line {} is outside 1..={last}",
                def.line
            ));
        }
        if !(def.line..=last).contains(&def.end_line) {
            return Err(format!(
                "defs[{i}]: end_line {} is outside {}..={last}",
                def.end_line, def.line
            ));
        }
        if def.is_out_of_line_module
            || def.path_override.is_some()
            || def.enclosing_inline_module_path.is_some()
        {
            return Err(format!(
                "defs[{i}]: an out-of-line module is not in the protocol"
            ));
        }
    }
    for (i, r) in refs.iter().enumerate() {
        if r.name.is_empty() {
            return Err(format!("refs[{i}]: the name is empty"));
        }
        if !(1..=last).contains(&r.line) {
            return Err(format!("refs[{i}]: line {} is outside 1..={last}", r.line));
        }
        if let Some(kind) = &r.ref_kind {
            if ![REF_KIND_CONST, REF_KIND_TYPE, REF_KIND_USE].contains(&kind.as_str()) {
                return Err(format!(
                    "refs[{i}]: ref_kind {kind:?} is not {REF_KIND_CONST}, {REF_KIND_TYPE} or \
                     {REF_KIND_USE}"
                ));
            }
        }
    }
    if let Some(through) = parent_cycle(&defs) {
        return Err(format!(
            "the defs' parents form a cycle through {through:?}"
        ));
    }
    Ok(CodeSymbols {
        defs,
        refs,
        partial: answer.partial,
    })
}

/// The def path a walk of `defs` from def path to parent returns to, when one does: the paths are
/// walked in the order the defs give them, each path stepping to the parent each def of that path
/// names, and a parent no def is at ends that step. None when the parents form no cycle.
fn parent_cycle(defs: &[model::Def]) -> Option<&str> {
    let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
    for def in defs {
        let named = parents.entry(def.path.as_str()).or_default();
        named.extend(def.parent.as_deref());
    }
    // A path is `false` while the walk is inside it and `true` once every step from it is done.
    let mut walked: HashMap<&str, bool> = HashMap::new();
    for start in defs.iter().map(|def| def.path.as_str()) {
        if walked.contains_key(start) {
            continue;
        }
        walked.insert(start, false);
        let mut walk = vec![(start, 0)];
        while let Some(step) = walk.last_mut() {
            let (at, next) = *step;
            let Some(&parent) = parents[at].get(next) else {
                walked.insert(at, true);
                walk.pop();
                continue;
            };
            step.1 += 1;
            match walked.get(parent) {
                Some(false) => return Some(parent),
                Some(true) => {}
                None if parents.contains_key(parent) => {
                    walked.insert(parent, false);
                    walk.push((parent, 0));
                }
                None => {}
            }
        }
    }
    None
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
        read_head(&self.root, &DirTreeSource::new(&self.root), worktree)
    }
}

/// Reads a file's facts over the existing symbol, design and workflow extractors.
#[derive(Clone, Copy, Debug, Default)]
pub struct FactsExtractor;

impl Extract for FactsExtractor {
    fn fingerprint(&self) -> u64 {
        fingerprint_of(GRAMMAR_TAGS_VERSION, FACTS_GENERATION)
    }

    /// The bundled grammars' read of the file ([`bundled_code`]) through the one skeleton
    /// ([`file_facts`]).
    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
        file_facts(entry, bytes, bundled_code)
    }
}

/// The symbols an extraction gives one code file, the bundled grammars' and a declared
/// extractor's alike: its definitions, its references and whether they cover only part of it.
#[derive(Debug, PartialEq, Eq)]
struct CodeSymbols {
    defs: Vec<model::Def>,
    refs: Vec<SymRef>,
    partial: bool,
}

/// A code file as an extraction reads it: the language it is read as and its symbols, none when
/// the extraction failed.
struct CodeFile {
    lang: String,
    symbols: Option<CodeSymbols>,
}

/// One file's facts, the one skeleton every extraction reads a file through: its identity and its
/// scope by the dev-only rule; for the workflow definition, the workflow half and its nodes and
/// relations, before `code` is asked of it; for any other file, the code half, the language and
/// the symbols `code` reads it as when it reads it at all, else the design half, then the design
/// facts of its text. Total over its input: bytes that are not UTF-8, a failed extraction and a
/// workflow definition that does not parse give a file's identity and no lists, as a file that
/// extracts to nothing.
fn file_facts(
    entry: &Entry,
    bytes: &[u8],
    code: impl FnOnce(&str, Option<&str>) -> Option<CodeFile>,
) -> FileFacts {
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
    match code(path, text) {
        Some(file) => {
            facts.half = Half::Code;
            facts.lang = file.lang;
            if let Some(symbols) = file.symbols {
                code_facts(
                    path,
                    &symbols.defs,
                    &symbols.refs,
                    symbols.partial,
                    &mut facts,
                );
            }
        }
        None => facts.half = Half::Design,
    }
    if let Some(text) = text {
        design_facts(path, text, &mut facts);
    }
    facts
}

/// The bundled grammars' read of the file at `path` holding `text`: the language of the grammar
/// its extension names and the symbols it extracts, none when there is no text or the extraction
/// fails; none at all when no grammar names the extension.
fn bundled_code(path: &str, text: Option<&str>) -> Option<CodeFile> {
    let grammar = registry::for_path(path, None)?;
    Some(CodeFile {
        lang: lang_str(grammar.lang).to_string(),
        symbols: extracted(path, || text, None)
            .flatten()
            .map(|(_, symbols)| CodeSymbols {
                defs: symbols.defs,
                refs: symbols.refs,
                partial: symbols.partial,
            }),
    })
}

/// Fills the definitions, mentions, module declarations and parse-error marker of `facts` from
/// the definitions `defs`, the references `refs` and the `partial` flag an extraction gave the
/// file at `path`.
fn code_facts(
    path: &str,
    defs: &[model::Def],
    refs: &[SymRef],
    partial: bool,
    facts: &mut FileFacts,
) {
    facts.partial = partial;
    let by_path = def_indices_by_path(defs);
    let index_of = |def_path: &str| by_path.get(def_path).copied();
    facts.defs = defs
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
    facts.mentions = refs
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
    facts.modules = defs
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

/// The index in `defs` of the definition a path names, for the paths an extraction gives a parent
/// or a caller by. Definitions can share a path (cfg twins, a type and a function of one name);
/// the one the path names is the one that becomes the entity, the first by
/// [`definition_precedence`], the earlier in the file's order on a tie.
fn def_indices_by_path(defs: &[model::Def]) -> HashMap<&str, u32> {
    let mut held: HashMap<&str, (u32, (usize, u32))> = HashMap::new();
    for (i, d) in defs.iter().enumerate() {
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
/// bytes the code store's extractor reads as the workflow definition, and the workflow
/// [`declared_extractors`] reads the project's extractors from.
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
    use crate::extractor_stub::{answer_defining, declared_identity, StubExtractor};
    use crate::extractors_fixtures::{workflow_declaring_extractors, EXTRACTOR_REFUSALS};
    use crate::go_definition_sources::GO_DEFINITIONS_SOURCE;
    use crate::grounder::symbols::content_hash;
    use crate::grounder::symbols::model::Kind;
    use crate::js_ts_definition_sources::{
        JS_DEFINITIONS_SOURCE, TSX_DEFINITIONS_SOURCE, TS_DEFINITIONS_SOURCE,
    };
    use crate::python_definition_sources::PYTHON_DEFINITIONS_SOURCE;
    use crate::rust_definition_sources::RUST_DEFINITIONS_SOURCE;
    use rigger_domain::code_view::CodeView;
    use rigger_domain::config::ExtractorDecl;
    use rigger_domain::contextgraph::{
        KIND_AGENT, KIND_DESIGN_DOC, KIND_GATE, KIND_RATIONALE, KIND_STAGE, REL_EXPLAINS, REL_RUNS,
        REL_SPECIFIES,
    };
    use rigger_store::code::Store;
    use std::collections::BTreeMap;

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

    /// Writes `workflow` as the workflow definition of the project at `root`.
    fn write_workflow(root: &Path, workflow: &str) {
        let path = root.join(workflow_doc());
        let dir = path
            .parent()
            .expect("the workflow lies under the rigger directory");
        std::fs::create_dir_all(dir).expect("create the rigger directory");
        std::fs::write(&path, workflow).expect("write the workflow");
    }

    /// A project in a fresh directory, its workflow definition `workflow` when there is one.
    fn project_declaring(workflow: Option<&str>) -> tempfile::TempDir {
        let project = tempfile::tempdir().expect("tempdir");
        if let Some(workflow) = workflow {
            write_workflow(project.path(), workflow);
        }
        project
    }

    /// The project at `root` declares no extractor in force: its read answers `read`, the map
    /// declared is empty, and the project's extractor is the bundled one, its fingerprint
    /// `FactsExtractor`'s.
    fn assert_bundled(root: &Path, read: Result<BTreeMap<String, ExtractorDecl>, String>) {
        let label = root.display();
        assert_eq!(read_declared_extractors(root), read, "{label}");
        assert_eq!(declared_extractors(root), BTreeMap::new(), "{label}");
        assert_eq!(
            ProjectExtractor::of(root).fingerprint(),
            FactsExtractor.fingerprint(),
            "{label}"
        );
    }

    /// THE WORKFLOW DECLARES EXTRACTORS, none or failing: with no workflow, a workflow without the
    /// key, an empty declaration, a workflow that fails to parse at any key, one that cannot be
    /// read, and each declaration the check refuses, the declared map is empty and the project's
    /// extractor is the bundled one; the read answers why a declaration failed.
    #[test]
    fn with_no_declaration_or_a_failing_one_the_project_extractor_is_the_bundled_one() {
        let refused = EXTRACTOR_REFUSALS.map(|(mapping, refusal)| {
            (
                Some(workflow_declaring_extractors(mapping)),
                Err(refusal.to_string()),
            )
        });
        let rows = [
            (None, Ok(BTreeMap::new())),
            (Some("stages: {}\n".to_string()), Ok(BTreeMap::new())),
            (
                Some(workflow_declaring_extractors("{}")),
                Ok(BTreeMap::new()),
            ),
            (
                Some("nonsense: 1\n".to_string()),
                Err("parse workflow: nonsense: unknown key".to_string()),
            ),
            (
                Some(workflow_declaring_extractors(
                    "{ zig: { extensions: [zig], run: zig-tags, args: [] } }",
                )),
                Err("parse workflow: extractors.zig.args: unknown key".to_string()),
            ),
        ];
        for (workflow, read) in rows.into_iter().chain(refused) {
            let project = project_declaring(workflow.as_deref());
            assert_bundled(project.path(), read);
        }
        let project = project_declaring(None);
        let unreadable = project.path().join(workflow_doc());
        std::fs::create_dir_all(&unreadable).expect("a directory where the workflow lies");
        let why = std::fs::read_to_string(&unreadable).expect_err("a directory reads as no text");
        assert_bundled(project.path(), Err(format!("read workflow: {why}")));
    }

    /// THE WORKFLOW DECLARES EXTRACTORS, declared: a declaration folds its digest, the content
    /// hash of the declared map's JSON in name order, into the fingerprint's grammar generation,
    /// so declarations differing in a name, an extension or a run alone give fingerprints that
    /// differ from each other and from the bundled one, and one declaration read twice gives one.
    #[test]
    fn each_declaration_folds_its_digest_into_the_fingerprint() {
        // (the `extractors:` mapping, the declared map's JSON in name order)
        let rows = [
            (
                "{ zig: { extensions: [zig], run: zig-tags } }",
                r#"{"zig":{"extensions":["zig"],"run":"zig-tags"}}"#,
            ),
            (
                "{ zag: { extensions: [zig], run: zig-tags } }",
                r#"{"zag":{"extensions":["zig"],"run":"zig-tags"}}"#,
            ),
            (
                "{ zig: { extensions: [zog], run: zig-tags } }",
                r#"{"zig":{"extensions":["zog"],"run":"zig-tags"}}"#,
            ),
            (
                "{ zig: { extensions: [zig], run: zig-facts } }",
                r#"{"zig":{"extensions":["zig"],"run":"zig-facts"}}"#,
            ),
            (
                "{ zig: { extensions: [zig], run: zig-tags }, ada: { extensions: [adb], run: gnat } }",
                r#"{"ada":{"extensions":["adb"],"run":"gnat"},"zig":{"extensions":["zig"],"run":"zig-tags"}}"#,
            ),
        ];
        let mut held = vec![FactsExtractor.fingerprint()];
        for (mapping, json) in rows {
            let project = project_declaring(Some(&workflow_declaring_extractors(mapping)));
            let fingerprint = ProjectExtractor::of(project.path()).fingerprint();
            let digest = content_hash(json);
            assert_eq!(
                fingerprint,
                fingerprint_of(
                    &format!("{GRAMMAR_TAGS_VERSION}+{digest}"),
                    FACTS_GENERATION
                ),
                "{mapping}"
            );
            assert_eq!(
                ProjectExtractor::of(project.path()).fingerprint(),
                fingerprint,
                "{mapping} read twice"
            );
            held.push(fingerprint);
        }
        let distinct: std::collections::BTreeSet<u64> = held.iter().copied().collect();
        assert_eq!(distinct.len(), rows.len() + 1, "{held:x?}");
    }

    /// THE WORKFLOW DECLARES EXTRACTORS, in force: the store `open_store` opens under one
    /// declaration is in force under it for `store_fingerprint` and for a reader of the head, is
    /// not in force under another declaration, and is again once the first returns.
    #[test]
    fn a_store_opened_under_one_declaration_is_not_in_force_under_another() {
        let a = workflow_declaring_extractors("{ zig: { extensions: [zig], run: zig-tags } }");
        let b = workflow_declaring_extractors("{ zig: { extensions: [zig], run: zig-facts } }");
        let project = project_declaring(Some(&a));
        let root = project.path();
        std::fs::write(root.join("lib.rs"), "fn held() {}\n").expect("write a source file");
        let opened = open_store(root, &DirTreeSource::new(root), Mode::Write)
            .expect("open the store under A");
        let stamp = opened.view.stamp().clone();
        assert_eq!(
            stamp.fingerprint,
            fingerprint(&ProjectExtractor::of(root), &ProductionGroupings),
            "opened under A's extractor"
        );
        assert_ne!(
            stamp.fingerprint,
            fingerprint(&FactsExtractor, &ProductionGroupings),
            "A's store is not the bundled one"
        );
        let in_force = || {
            let current = Store::new(root).current(store_fingerprint(root));
            current
                .expect("read the store")
                .map(|view| view.stamp().clone())
        };
        assert_eq!(in_force(), Some(stamp.clone()), "in force under A");
        let head = DirectoryHead::new(root)
            .head(root)
            .expect("read the head under A");
        assert_eq!(
            head.view.stamp(),
            &stamp,
            "a reader under A reads A's store"
        );
        write_workflow(root, &b);
        assert_eq!(in_force(), None, "not in force under B");
        write_workflow(root, &a);
        assert_eq!(in_force(), Some(stamp), "in force again under A");
    }

    /// A def an answer carries: a function named `name` at `path` over `line..=end_line`, its
    /// container `parent`.
    fn answered_def(
        name: &str,
        path: &str,
        (line, end_line): (u32, u32),
        parent: Option<&str>,
        is_test: bool,
    ) -> model::Def {
        model::Def {
            kind: Kind::Function,
            name: name.to_string(),
            line,
            end_line,
            path: path.to_string(),
            parent: parent.map(str::to_string),
            is_test,
            is_out_of_line_module: false,
            path_override: None,
            enclosing_inline_module_path: None,
        }
    }

    /// A ref an answer carries: `name` at `line`, inside the def at `enclosing`, of `ref_kind`.
    fn answered_ref(
        name: &str,
        line: u32,
        enclosing: Option<&str>,
        ref_kind: Option<&str>,
        is_test: bool,
    ) -> SymRef {
        SymRef {
            name: name.to_string(),
            line,
            enclosing: enclosing.map(str::to_string),
            is_test,
            ref_kind: ref_kind.map(str::to_string),
        }
    }

    /// An answer for `a.zig` carrying the defs `defs` (JSON objects, comma-separated) and no refs.
    fn answer_with_defs(defs: &str) -> String {
        format!(r#"{{"path":"a.zig","defs":[{defs}],"refs":[]}}"#)
    }

    /// An answer for `a.zig` carrying no defs and the refs `refs` (JSON objects, comma-separated).
    fn answer_with_refs(refs: &str) -> String {
        format!(r#"{{"path":"a.zig","defs":[],"refs":[{refs}]}}"#)
    }

    /// THE EXTRACTOR PROTOCOL, the answer: over a request for `a.zig`, an answer holding defs and
    /// refs within the text's lines - the last one more than the `\n` the text holds - gives them
    /// and its partial flag as written; every answer the protocol refuses gives the exact why:
    /// one that does not parse, that is for another path, that is an error, that lacks a list, a
    /// def or ref outside the lines or with an empty name, a def with an empty path, an end line
    /// outside its line and the last, or an out-of-line module field, a ref kind other than
    /// `const`, `type` and `use`, and a cycle among the def paths through their parents.
    #[test]
    fn an_answer_gives_its_symbols_or_why_the_protocol_refuses_it() {
        const TEXT: &str = "a\nb\nc\n";
        let def_on = |line: u32, end_line: u32| {
            format!(
                r#"{{"kind":"Function","name":"f","line":{line},"end_line":{end_line},"path":"f"}}"#
            )
        };
        let parented = |path: &str, parent: &str| {
            format!(
                r#"{{"kind":"Function","name":"f","line":1,"end_line":1,"path":"{path}","parent":"{parent}"}}"#
            )
        };
        let unparented = |path: &str| {
            format!(r#"{{"kind":"Type","name":"t","line":1,"end_line":1,"path":"{path}"}}"#)
        };
        let empty = || CodeSymbols {
            defs: Vec::new(),
            refs: Vec::new(),
            partial: false,
        };
        let refused = |why: &str| Err(why.to_string());
        // (the request's text, the answer line, its symbols or why it is refused)
        let rows: Vec<(&str, String, Result<CodeSymbols, String>)> = vec![
            (
                TEXT,
                r#"{"path":"a.zig","defs":[],"refs":[]}"#.into(),
                Ok(empty()),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","defs":[],"refs":[],"partial":true}"#.into(),
                Ok(CodeSymbols {
                    partial: true,
                    ..empty()
                }),
            ),
            (
                TEXT,
                concat!(
                    r#"{"path":"a.zig","defs":["#,
                    r#"{"kind":"Function","name":"a","line":1,"end_line":1,"path":"a","parent":"c"},"#,
                    r#"{"kind":"Function","name":"b","line":4,"end_line":4,"path":"b","parent":"c"},"#,
                    r#"{"kind":"Function","name":"c","line":1,"end_line":4,"path":"c","parent":"gone"},"#,
                    r#"{"kind":"Function","name":"a","line":2,"end_line":3,"path":"a","parent":"c","is_test":true}"#,
                    r#"],"refs":["#,
                    r#"{"name":"x","line":1},"#,
                    r#"{"name":"y","line":4,"enclosing":"a","is_test":true},"#,
                    r#"{"name":"C","line":2,"ref_kind":"const"},"#,
                    r#"{"name":"T","line":3,"ref_kind":"type"},"#,
                    r#"{"name":"u","line":4,"ref_kind":"use"}"#,
                    r#"]}"#
                )
                .into(),
                Ok(CodeSymbols {
                    defs: vec![
                        answered_def("a", "a", (1, 1), Some("c"), false),
                        answered_def("b", "b", (4, 4), Some("c"), false),
                        answered_def("c", "c", (1, 4), Some("gone"), false),
                        answered_def("a", "a", (2, 3), Some("c"), true),
                    ],
                    refs: vec![
                        answered_ref("x", 1, None, None, false),
                        answered_ref("y", 4, Some("a"), None, true),
                        answered_ref("C", 2, None, Some("const"), false),
                        answered_ref("T", 3, None, Some("type"), false),
                        answered_ref("u", 4, None, Some("use"), false),
                    ],
                    partial: false,
                }),
            ),
            ("", answer_with_defs(&def_on(1, 1)), {
                Ok(CodeSymbols {
                    defs: vec![answered_def("f", "f", (1, 1), None, false)],
                    ..empty()
                })
            }),
            (
                "",
                answer_with_defs(&def_on(2, 2)),
                refused("defs[0]: line 2 is outside 1..=1"),
            ),
            ("a\nb", answer_with_defs(&def_on(1, 2)), {
                Ok(CodeSymbols {
                    defs: vec![answered_def("f", "f", (1, 2), None, false)],
                    ..empty()
                })
            }),
            (
                "a\nb",
                answer_with_defs(&def_on(1, 3)),
                refused("defs[0]: end_line 3 is outside 1..=2"),
            ),
            (
                TEXT,
                "not json".into(),
                refused("the answer does not parse: expected ident at line 1 column 2"),
            ),
            (
                TEXT,
                r#"{"defs":[],"refs":[]}"#.into(),
                refused("the answer does not parse: missing field `path` at line 1 column 21"),
            ),
            (
                TEXT,
                r#"{"path":"c.zig","defs":[],"refs":[]}"#.into(),
                refused(r#"the answer is for "c.zig""#),
            ),
            (
                TEXT,
                r#"{"path":"c.zig","error":"no parser"}"#.into(),
                refused(r#"the answer is for "c.zig""#),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","error":"no parser"}"#.into(),
                refused("the answer is an error: no parser"),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","error":"no parser","defs":[],"refs":[]}"#.into(),
                refused("the answer is an error: no parser"),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","refs":[]}"#.into(),
                refused("the answer holds no error and lacks defs or refs"),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","defs":[]}"#.into(),
                refused("the answer holds no error and lacks defs or refs"),
            ),
            (
                TEXT,
                r#"{"path":"a.zig"}"#.into(),
                refused("the answer holds no error and lacks defs or refs"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Function","name":"","line":1,"end_line":1,"path":"f"}"#,
                ),
                refused("defs[0]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_defs(&format!(
                    r#"{},{{"kind":"Function","name":"","line":1,"end_line":1,"path":"g"}}"#,
                    def_on(1, 1)
                )),
                refused("defs[1]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Function","name":"f","line":1,"end_line":1,"path":""}"#,
                ),
                refused("defs[0]: the path is empty"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(0, 1)),
                refused("defs[0]: line 0 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(5, 5)),
                refused("defs[0]: line 5 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(2, 1)),
                refused("defs[0]: end_line 1 is outside 2..=4"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(2, 5)),
                refused("defs[0]: end_line 5 is outside 2..=4"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Module","name":"m","line":1,"end_line":1,"path":"m","is_out_of_line_module":true}"#,
                ),
                refused("defs[0]: an out-of-line module is not in the protocol"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Module","name":"m","line":1,"end_line":1,"path":"m","path_override":"x.zig"}"#,
                ),
                refused("defs[0]: an out-of-line module is not in the protocol"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Module","name":"m","line":1,"end_line":1,"path":"m","enclosing_inline_module_path":"n"}"#,
                ),
                refused("defs[0]: an out-of-line module is not in the protocol"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"","line":1}"#),
                refused("refs[0]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1},{"name":"","line":1}"#),
                refused("refs[1]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":0}"#),
                refused("refs[0]: line 0 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":5}"#),
                refused("refs[0]: line 5 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":"call"}"#),
                refused(r#"refs[0]: ref_kind "call" is not const, type or use"#),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":""}"#),
                refused(r#"refs[0]: ref_kind "" is not const, type or use"#),
            ),
            (
                TEXT,
                answer_with_defs(&parented("a", "a")),
                refused(r#"the defs' parents form a cycle through "a""#),
            ),
            (
                TEXT,
                answer_with_defs(&format!("{},{}", parented("a", "b"), parented("b", "a"))),
                refused(r#"the defs' parents form a cycle through "a""#),
            ),
            (
                TEXT,
                answer_with_defs(&format!(
                    "{},{},{}",
                    parented("a", "b"),
                    parented("b", "c"),
                    parented("c", "b")
                )),
                refused(r#"the defs' parents form a cycle through "b""#),
            ),
            (
                TEXT,
                answer_with_defs(&format!(
                    "{},{},{}",
                    unparented("a"),
                    parented("a", "b"),
                    parented("b", "a")
                )),
                refused(r#"the defs' parents form a cycle through "a""#),
            ),
        ];
        for (text, answer, symbols) in rows {
            assert_eq!(
                symbols_answered("a.zig", text, &answer),
                symbols,
                "{text:?}: {answer}"
            );
        }
    }

    /// THE EXTRACTOR PROTOCOL, a session: a started extractor answers a request with its line, its
    /// line ending dropped; one whose output ends, whose answer is not text, or that is gone before
    /// the request is written says why; and one that cannot start in its directory says why.
    #[cfg(unix)]
    #[test]
    fn a_session_answers_a_request_with_its_line_or_says_why_it_cannot() {
        let kept = tempfile::tempdir().expect("tempdir");
        // (the extractor's run, whether it has ended before the request, its answer or why not)
        let rows = [
            (r#"read -r line; printf '%s\n' "$line""#, false, Ok("ping")),
            (r#"read -r line; printf 'pong\r\n'"#, false, Ok("pong")),
            (r#"read -r line; printf 'pong'"#, false, Ok("pong")),
            ("read -r line", false, Err("end of output".to_string())),
            (
                r#"read -r line; printf '\377\n'"#,
                false,
                Err("read the answer: stream did not contain valid UTF-8".to_string()),
            ),
            (
                "exit 0",
                true,
                // EPIPE: the extractor's end of the pipe closed with it.
                Err(format!(
                    "write the request: {}",
                    std::io::Error::from_raw_os_error(32)
                )),
            ),
        ];
        for (run, ended, answer) in rows {
            let mut session = Session::start(run, kept.path()).expect("start the session");
            if ended {
                session.child.wait().expect("wait for the extractor");
            }
            assert_eq!(session.ask("ping"), answer.map(str::to_string), "{run}");
            session.close();
        }
        let gone = kept.path().join("gone");
        assert_eq!(
            Session::start("true", &gone).map(|session| session.close()),
            Err(format!("start: {}", std::io::Error::from_raw_os_error(2))),
            "a session cannot start in a directory that is not there"
        );
    }

    /// A project in a fresh directory whose workflow declares `stub` as the extractor `name` of
    /// the flow sequence `extensions`, holding each of `files` at its path with its text.
    fn stub_project(
        stub: &StubExtractor,
        name: &str,
        extensions: &str,
        files: &[(&str, &str)],
    ) -> tempfile::TempDir {
        let project = project_declaring(Some(&workflow_declaring_extractors(
            &stub.declaring(name, extensions),
        )));
        for (path, text) in files {
            let file = project.path().join(path);
            std::fs::create_dir_all(file.parent().expect("a file lies in a directory"))
                .expect("create the file's directory");
            std::fs::write(file, text).expect("write the file");
        }
        project
    }

    /// The physical directory of the project at `root`, as the stub records where it started.
    fn root_dir_of(root: &Path) -> String {
        root.canonicalize()
            .expect("the root exists")
            .display()
            .to_string()
    }

    /// A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE: over a project declaring the stub for `zig` and
    /// `zag`, the store `open_store` advances holds each such file's answer as its facts under the
    /// declared name - its definitions with their parents, its mentions with their callers and
    /// kinds, its partial flag, its scope by the dev-only rule and the design facts of its text -
    /// beside a bundled file's own facts; the stub started once, in the root, read each file's
    /// request once and was closed when the advance was done.
    #[cfg(unix)]
    #[test]
    fn an_advance_holds_each_declared_files_answer_read_in_one_session() {
        const A_TEXT: &str = "// WHY: the stub answers it\ntype T\n  fn new\n    call helper\n";
        const A_ANSWER: &str = concat!(
            r#"{"path":"src/a.zig","defs":["#,
            r#"{"kind":"Type","name":"T","line":2,"end_line":4,"path":"T"},"#,
            r#"{"kind":"Method","name":"new","line":3,"end_line":4,"path":"T::new","parent":"T","is_test":true}"#,
            r#"],"refs":["#,
            r#"{"name":"helper","line":4,"enclosing":"T::new"},"#,
            r#"{"name":"T","line":3,"enclosing":"T","ref_kind":"type","is_test":true},"#,
            r#"{"name":"top","line":1}"#,
            r#"],"partial":true}"#
        );
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let project = stub_project(
            &stub,
            "zig",
            "[zig, zag]",
            &[
                ("src/a.zig", A_TEXT),
                ("tests/b.zag", "only\n"),
                ("lib.rs", "fn held() {}\n"),
            ],
        );
        stub.answers("src/a.zig", A_ANSWER);
        stub.answers(
            "tests/b.zag",
            r#"{"path":"tests/b.zag","defs":[],"refs":[]}"#,
        );
        let root = project.path();
        let opened =
            open_store(root, &DirTreeSource::new(root), Mode::Write).expect("open the store");
        let held = |path: &str| {
            opened
                .view
                .file(path)
                .unwrap_or_else(|| panic!("{path} is in the store"))
                .into_owned()
        };
        let a = held("src/a.zig");
        let mention = |enclosing, name: &str, kind, line, test| Mention {
            enclosing,
            name: name.to_string(),
            leaf: name.to_string(),
            kind,
            line,
            test,
        };
        assert_eq!(
            a,
            FileFacts {
                partial: true,
                defs: vec![
                    Def {
                        local: "T".into(),
                        leaf: "T".into(),
                        kind: "type".into(),
                        line: 2,
                        end_line: 4,
                        parent: None,
                        test: false,
                    },
                    Def {
                        local: "T::new".into(),
                        leaf: "new".into(),
                        kind: "method".into(),
                        line: 3,
                        end_line: 4,
                        parent: Some(0),
                        test: true,
                    },
                ],
                mentions: vec![
                    mention(Some(1), "helper", MentionKind::Call, 4, false),
                    mention(Some(0), "T", MentionKind::Tagged("type".into()), 3, true),
                    mention(None, "top", MentionKind::Call, 1, false),
                ],
                doc_concepts: vec![DocConcept {
                    kind: KIND_RATIONALE.into(),
                    id: "src/a.zig#L1".into(),
                    title: "WHY: the stub answers it".into(),
                }],
                doc_links: vec![DocLink {
                    from: "src/a.zig#L1".into(),
                    rel: REL_EXPLAINS.into(),
                    to: "src/a.zig".into(),
                }],
                ..declared_identity("src/a.zig", &a.blob, "zig")
            },
            "a declared file holds its answer"
        );
        let b = held("tests/b.zag");
        assert_eq!(
            b,
            FileFacts {
                scope: Scope::Test,
                ..declared_identity("tests/b.zag", &b.blob, "zig")
            },
            "a declared file's scope is the dev-only rule's"
        );
        let lib = held("lib.rs");
        let entry = Entry {
            path: "lib.rs".into(),
            blob: lib.blob.clone(),
        };
        assert_eq!(
            lib,
            FactsExtractor.extract(&entry, b"fn held() {}\n"),
            "an undeclared file holds the bundled grammars' facts"
        );
        let mut requests = stub.requests();
        requests.sort();
        assert_eq!(
            (stub.starts(), requests, stub.ends()),
            (
                vec![root_dir_of(root)],
                vec![
                    r#"{"path":"src/a.zig","text":"// WHY: the stub answers it\ntype T\n  fn new\n    call helper\n"}"#
                        .to_string(),
                    r#"{"path":"tests/b.zag","text":"only\n"}"#.to_string(),
                ],
                vec!["ended".to_string()],
            ),
            "one session, in the root, one request a file, closed after the advance"
        );
    }

    /// A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE, failed: in the store `open_store` advances, a
    /// file the stub answers with an error, with another file's path, with two defs each naming
    /// the other its parent, or not at all holds its identity facts and no definitions, and a file
    /// beside them still holds its answer.
    #[cfg(unix)]
    #[test]
    fn a_file_whose_answer_fails_holds_its_identity_and_the_next_file_extracts() {
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let names = ["a", "b", "c", "d", "e"];
        let files: Vec<(String, &str)> = names
            .iter()
            .map(|name| (format!("{name}.zig"), "x\ny\n"))
            .collect();
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), *text))
            .collect();
        let project = stub_project(&stub, "zig", "[zig]", &files);
        stub.answers("a.zig", r#"{"path":"a.zig","error":"no parser"}"#);
        stub.answers("b.zig", r#"{"path":"c.zig","defs":[],"refs":[]}"#);
        stub.answers(
            "c.zig",
            concat!(
                r#"{"path":"c.zig","defs":["#,
                r#"{"kind":"Function","name":"f","line":1,"end_line":2,"path":"f","parent":"g"},"#,
                r#"{"kind":"Function","name":"g","line":2,"end_line":2,"path":"g","parent":"f"}"#,
                r#"],"refs":[]}"#
            ),
        );
        stub.answers(
            "d.zig",
            r#"{"path":"d.zig","defs":[{"kind":"Function","name":"d","line":1,"end_line":2,"path":"d"}],"refs":[{"name":"d","line":2,"enclosing":"d"}]}"#,
        );
        let root = project.path();
        let opened =
            open_store(root, &DirTreeSource::new(root), Mode::Write).expect("open the store");
        for name in names {
            let path = format!("{name}.zig");
            let held = opened
                .view
                .file(&path)
                .unwrap_or_else(|| panic!("{path} is in the store"))
                .into_owned();
            let identity = declared_identity(&path, &held.blob, "zig");
            let expected = if name == "d" {
                FileFacts {
                    defs: vec![Def {
                        local: "d".into(),
                        leaf: "d".into(),
                        kind: "function".into(),
                        line: 1,
                        end_line: 2,
                        parent: None,
                        test: false,
                    }],
                    mentions: vec![Mention {
                        enclosing: Some(0),
                        name: "d".into(),
                        leaf: "d".into(),
                        kind: MentionKind::Call,
                        line: 2,
                        test: false,
                    }],
                    ..identity
                }
            } else {
                identity
            };
            assert_eq!(held, expected, "{path}");
        }
    }

    /// A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE, restarted: one project extractor reading files
    /// one at a time closes its session at each failed file - an error, the end of the stub's
    /// output, another file's path - so the next file starts a new one in the root; a closed
    /// session's input ends, the stub's own exit aside, and dropping the extractor closes the
    /// session it still holds.
    #[cfg(unix)]
    #[test]
    fn after_a_failed_file_the_next_file_starts_a_new_session() {
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let project = stub_project(&stub, "zig", "[zig]", &[]);
        let good = |name: &str| answer_defining(&format!("{name}.zig"), name);
        // (the file's name, its answer, the leaves it holds, the stub's starts and ends after it)
        type Row<'a> = (&'a str, Option<String>, &'a [&'a str], usize, usize);
        let rows: [Row; 6] = [
            ("a", Some(good("a")), &["a"], 1, 0),
            (
                "b",
                Some(r#"{"path":"b.zig","error":"no parser"}"#.into()),
                &[],
                1,
                1,
            ),
            ("c", Some(good("c")), &["c"], 2, 1),
            ("d", None, &[], 2, 1),
            ("e", Some(good("a")), &[], 3, 2),
            ("f", Some(good("f")), &["f"], 4, 2),
        ];
        let extractor = ProjectExtractor::of(project.path());
        for (name, answer, leaves, starts, ends) in rows {
            let path = format!("{name}.zig");
            if let Some(answer) = answer {
                stub.answers(&path, &answer);
            }
            let entry = Entry {
                path: path.clone(),
                blob: format!("blob-of-{path}"),
            };
            let facts = extractor.extract(&entry, b"x\n");
            let held: Vec<&str> = facts.defs.iter().map(|d| d.leaf.as_str()).collect();
            assert_eq!(
                (
                    facts.half,
                    facts.lang.as_str(),
                    held,
                    stub.starts().len(),
                    stub.ends().len()
                ),
                (Half::Code, "zig", leaves.to_vec(), starts, ends),
                "{path}"
            );
        }
        drop(extractor);
        let root_dir = root_dir_of(project.path());
        assert_eq!(
            (stub.starts(), stub.ends().len(), stub.requests()),
            (
                vec![root_dir; 4],
                3,
                ["a", "b", "c", "d", "e", "f"]
                    .map(|name| format!(r#"{{"path":"{name}.zig","text":"x\n"}}"#))
                    .to_vec(),
            ),
            "dropping the extractor closes the session it holds"
        );
    }

    /// A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE, over a bundled one: a declaration named `rust`
    /// over `rs` gives each `.rs` file its answer in place of the Rust grammar's - none when the
    /// answer fails - while a bundled language it does not list keeps its grammar.
    #[cfg(unix)]
    #[test]
    fn a_declaration_named_rust_over_rs_replaces_the_bundled_grammar() {
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let project = stub_project(&stub, "rust", "[rs]", &[]);
        stub.answers("src/lib.rs", &answer_defining("src/lib.rs", "declared"));
        let extractor = ProjectExtractor::of(project.path());
        // (the file, its text, the language it holds, the leaves of its definitions)
        let rows: [(&str, &str, &str, &[&str]); 3] = [
            ("src/lib.rs", "fn held() {}\n", "rust", &["declared"]),
            ("src/gone.rs", "fn held() {}\n", "rust", &[]),
            (
                "tool/run.py",
                "def held():\n    pass\n",
                "python",
                &["held"],
            ),
        ];
        for (path, text, lang, leaves) in rows {
            let entry = Entry {
                path: path.into(),
                blob: format!("blob-of-{path}"),
            };
            let facts = extractor.extract(&entry, text.as_bytes());
            let held: Vec<&str> = facts.defs.iter().map(|d| d.leaf.as_str()).collect();
            assert_eq!(
                (facts.half, facts.lang.as_str(), held),
                (Half::Code, lang, leaves.to_vec()),
                "{path}"
            );
        }
        assert_eq!(
            (stub.starts(), stub.requests().len()),
            (vec![root_dir_of(project.path())], 2),
            "the `.rs` files were asked of one session, the `.py` file of none"
        );
    }

    /// A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE, unconsulted: under a declaration listing `zig`
    /// and `yml`, the workflow definition, a file of a bundled language, a document, a file whose
    /// name only ends in `.zig` and one whose extension differs in case extract exactly as the
    /// bundled grammars extract them, and a declared file that is not text holds the declared
    /// identity; none of them starts the extractor.
    #[cfg(unix)]
    #[test]
    fn a_file_no_declaration_reaches_extracts_as_the_bundled_grammars_do() {
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let project = stub_project(&stub, "zig", "[zig, yml]", &[]);
        let workflow =
            std::fs::read(project.path().join(workflow_doc())).expect("read the workflow");
        let extractor = ProjectExtractor::of(project.path());
        // (the file, its bytes, the facts it holds when they are not the bundled grammars')
        let rows: [(&str, &[u8], Option<FileFacts>); 6] = [
            (".rigger/workflow.yml", &workflow, None),
            ("src/lib.rs", b"fn held() {}\n", None),
            ("docs/a-design.md", b"# A design\n\n## Parts\n", None),
            ("a/.zig", b"x\n", None),
            ("a.ZIG", b"x\n", None),
            (
                "bad.zig",
                &[0xff, 0xfe],
                Some(declared_identity("bad.zig", "blob-of-bad.zig", "zig")),
            ),
        ];
        for (path, bytes, declared) in rows {
            let entry = Entry {
                path: path.into(),
                blob: format!("blob-of-{path}"),
            };
            let expected = declared.unwrap_or_else(|| FactsExtractor.extract(&entry, bytes));
            assert_eq!(extractor.extract(&entry, bytes), expected, "{path}");
        }
        assert_eq!(
            (stub.starts(), stub.requests()),
            (Vec::<String>::new(), Vec::<String>::new()),
            "no file started the extractor"
        );
    }
}
