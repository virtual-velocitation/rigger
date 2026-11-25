//! Extraction to facts: the [`Extract`] implementations that read one file's path and bytes into
//! its [`FileFacts`]. They re-shape what the per-language symbol extractors, the design-document
//! extractor and the workflow-definition extractor already find, and what an extractor the
//! project's workflow declares answers over THE EXTRACTOR PROTOCOL (spec 125), whose client is
//! [`extractor_protocol`]; this module is no extractor of its own, and its one language knowledge
//! is the doc rules [`FactsExtractor`]'s [`Extract::doc_text`] reads a doc span's text by: Rust's,
//! which strips its lines to their comment text, C#'s, which reads its `///` lines with their XML
//! tags removed, and Python's, which reads its docstring's body, its parts joined, dedented.
//! Every name is kept as written and
//! nothing is resolved across files, so a file's facts are a function of its path and bytes alone
//! under the extractor's fingerprint, a declared extractor being trusted, as a grammar is, to
//! answer the same request the same way. The store's openers compose [`ProjectExtractor`], the
//! bundled extraction under the extractors the project's workflow declares, whose fingerprint
//! changes with the declaration; the show surface asks it for a file's definitions
//! ([`ProjectExtractor::defs`]), so a shown body is bounded as the store extracts it.

mod extractor_protocol;

use std::collections::{btree_map, hash_map, BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, WfNode,
    WfRelation,
};
use rigger_domain::code_view::{trimmed_lines, Entry, Extract, Head, OpenCodeView, TreeSource};
use rigger_domain::contextgraph::definition_precedence;
use rigger_domain::contextgraph::query::qualified_name;
use rigger_domain::groupings::ProductionGroupings;
use rigger_domain::playbooks::fnv1a_64;
use rigger_process::whole_line;
use rigger_store::code::{fingerprint, open_current, open_head, DirTreeSource, Memo, Mode, Opened};

use self::extractor_protocol::{request_line, symbols_answered, Session};
use crate::config::{check_extractors, ExtractorDecl, RIGGER_DIR, WORKFLOW_FILE};
use crate::config_store;
use crate::grounder::design::extract::{extract_concepts, extract_links};
use crate::grounder::symbols::extract::{kind_str, lang_str};
use crate::grounder::symbols::model::{self, Lang, SymRef, GRAMMAR_TAGS_VERSION};
use crate::grounder::symbols::paths::{is_dev_only, module_candidates};
use crate::grounder::symbols::{content_hash, extracted, registry};
use crate::grounder::workflowdef;

/// The generation of everything this module adds to the extractors it re-shapes: the design and
/// workflow extractors, which carry no generation of their own, and the shape of the facts. Bump
/// it whenever any of them can give a file other facts than before, as each symbol extractor's
/// tags generation, joined in [`GRAMMAR_TAGS_VERSION`], is bumped for its own; the two together make
/// the extractor's fingerprint, with the digest of the project's declared extractors when it
/// declares any ([`ProjectExtractor`]), which the store's fingerprint ([`store_fingerprint`]) folds
/// with the grouper's generation. A golden test pins it beside the digest of the facts read over a
/// design, a rationale and a workflow fixture, so a change that is not bumped fails there; a
/// conversion change that moves a language's facts re-pins that language's row of the same golden
/// with the bump.
const FACTS_GENERATION: &str = "facts-v6";

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
/// ([`Memo::process`]). The head keeps the project's extractor, which a doc read takes a span's
/// text from with no session; every declared extractor's session the read started is closed and
/// waited for before the head is given ([`ProjectExtractor::close_sessions`]), so none outlives a
/// read that ends by exiting.
pub fn read_head(
    root: &Path,
    source: &(dyn TreeSource + Sync),
    worktree: &Path,
) -> Result<Head, String> {
    let extract = Arc::new(ProjectExtractor::of(root));
    let head = open_head(
        root,
        source,
        extract.clone(),
        &ProductionGroupings,
        worktree,
        Memo::process(),
    );
    extract.close_sessions();
    head
}

/// The extractors the workflow of the project at `root` declares (spec 125, THE EXTRACTORS KEY):
/// the one read of the map, the code store's and the show surface's through
/// [`ProjectExtractor::of`]. A workflow that cannot be read or fails its check declares none, said
/// on stderr as `rigger: extractors: {error}` in one write ([`whole_line`]), so the bundled
/// grammars proceed alone.
fn declared_extractors(root: &Path) -> BTreeMap<String, ExtractorDecl> {
    read_declared_extractors(root).unwrap_or_else(|error| {
        whole_line::say(format_args!("rigger: extractors: {error}"));
        BTreeMap::new()
    })
}

/// The extractors the workflow at `root` declares, read through
/// [`config_store::load_project_workflow`] and checked by [`check_extractors`]: the empty map when
/// there is no workflow, and why when it cannot be read, does not parse or declares a malformed
/// extractor.
fn read_declared_extractors(root: &Path) -> Result<BTreeMap<String, ExtractorDecl>, String> {
    let Some(workflow) = config_store::load_project_workflow(root).map_err(|e| e.0)? else {
        return Ok(BTreeMap::new());
    };
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

    /// The file at `path` holding `text` read as code, the one routing an advance
    /// ([`Extract::extract`]) and a shown body ([`ProjectExtractor::defs`]) share: a file whose
    /// extension a declaration lists ([`ProjectExtractor::declaration_of`]) in the declared
    /// extractor's name, its symbols the extractor's answer
    /// ([`ProjectExtractor::declared_symbols`]), none for no text or a failed answer, so a declared
    /// extension is never read by a bundled grammar; any other file as the bundled grammars read
    /// it ([`bundled_code`]).
    fn code(&self, path: &str, text: Option<&str>) -> Option<CodeFile> {
        let Some((name, decl)) = self.declaration_of(path) else {
            return bundled_code(path, text);
        };
        Some(CodeFile {
            lang: name.to_string(),
            symbols: text
                .and_then(|text| self.declared_symbols(name, &decl.run, path, text))
                .map(Ok),
        })
    }

    /// The definitions of the file at `path` holding `text`, read as an advance reads them
    /// ([`file_facts`] over [`ProjectExtractor::code`]): none for the workflow definition, which
    /// is read as the workflow before any declaration is consulted, for a file no extraction reads
    /// and for a failed answer; why, when the bundled grammar cannot tag the text.
    pub fn defs(&self, path: &str, text: &str) -> Result<Vec<model::Def>, String> {
        if is_workflow_doc(path) {
            return Ok(Vec::new());
        }
        match self.code(path, Some(text)).and_then(|file| file.symbols) {
            Some(symbols) => symbols.map(|symbols| symbols.defs),
            None => Ok(Vec::new()),
        }
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

    /// Closes, and waits for, every session the extractor holds, so none outlives the read that
    /// started it; a later file of a declared extractor's extensions starts a new one.
    fn close_sessions(&self) {
        let sessions =
            std::mem::take(&mut *self.sessions.lock().unwrap_or_else(PoisonError::into_inner));
        for session in sessions.into_values() {
            session.close();
        }
    }
}

impl Drop for ProjectExtractor {
    /// Closes every session the extractor still holds ([`ProjectExtractor::close_sessions`]).
    fn drop(&mut self) {
        self.close_sessions();
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
    /// symbols the extractor's answer ([`ProjectExtractor::code`]) - none for bytes that are not
    /// UTF-8 or a failed answer - and the workflow definition and every other file read as
    /// [`FactsExtractor`] reads them; a declaration named as a bundled language (`rust`) replaces
    /// its grammar for the extensions it lists.
    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
        file_facts(entry, bytes, |path, text| self.code(path, text))
    }

    /// [`FactsExtractor`]'s: a declared file's language is its declaration's name, so a
    /// declaration named as a bundled language (`rust`) reads that language's doc rule and any
    /// other name the lines trimmed.
    fn doc_text(&self, lang: &str, raw: &str) -> String {
        FactsExtractor.doc_text(lang, raw)
    }
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

    /// The doc rule of the language `lang` names: C#'s ([`csharp_doc_text`]), Rust's
    /// ([`rust_doc_text`]), Python's ([`python_doc_text`]), and the lines trimmed
    /// ([`trimmed_lines`]) for a language with no rule.
    fn doc_text(&self, lang: &str, raw: &str) -> String {
        if lang == lang_str(Lang::CSharp) {
            csharp_doc_text(raw)
        } else if lang == lang_str(Lang::Rust) {
            rust_doc_text(raw)
        } else if lang == lang_str(Lang::Python) {
            python_doc_text(raw)
        } else {
            trimmed_lines(raw)
        }
    }
}

/// The comment text of a Rust doc span's `raw` lines, read as Rust lexes them from the span's start
/// to its end, a trailing newline adding no line. A `//` comment runs to its line's end, and `///`
/// (not `////`) and `//!` give the text after their marker, trimmed. A `/*` comment runs to the
/// `*/` that closes it ([`until_block_closer`]). `/**` (not `/***` nor `/**/`) and `/*!` open a doc
/// block ([`opens_doc_block`]) that gives its lines ([`doc_block_lines`]); any other block comment
/// is dropped whole, a `///` inside it included. A name-value `#[doc = ..]` or `#![doc = ..]` gives
/// each line of its string literal's body, trimmed ([`doc_attribute_text`]). Every other token is
/// dropped ([`after_token`]): another attribute, `#[doc(..)]` included, code and every other
/// literal, read whole so that no comment marker or quote inside a literal is read. Whatever
/// follows a comment or a literal is read on, and the kept lines join by `\n`.
fn rust_doc_text(raw: &str) -> String {
    let mut text = Vec::new();
    let mut rest = raw.strip_suffix('\n').unwrap_or(raw);
    while !rest.is_empty() {
        rest = if let Some(comment) = rest.strip_prefix("//") {
            let (comment, after) = comment.split_once('\n').unwrap_or((comment, ""));
            if let Some(doc) = after_doc_slash(comment).or_else(|| comment.strip_prefix('!')) {
                text.push(doc.trim());
            }
            after
        } else if let Some(comment) = rest.strip_prefix("/*") {
            let (body, after) = until_block_closer(comment);
            if opens_doc_block(comment) {
                // The body past the `*` or `!` that marks a doc block.
                text.extend(doc_block_lines(&body[1..], after.is_some()));
            }
            after.unwrap_or_default()
        } else if let Some((body, after)) = doc_attribute_text(rest) {
            text.extend(body.split('\n').map(str::trim));
            after
        } else {
            after_token(rest)
        };
    }
    text.join("\n")
}

/// The text after the third `/` of a `///` line comment, not `////`, `comment` being the text
/// after its first two: the doc marker a Rust and a C# line comment share.
fn after_doc_slash(comment: &str) -> Option<&str> {
    comment
        .strip_prefix('/')
        .filter(|doc| !doc.starts_with('/'))
}

/// The text of a C# doc span's `raw` lines, split on `\n` with a trailing `\r` dropped: a `///`
/// line (not `////`), leading whitespace aside, gives the text after its marker
/// ([`after_doc_slash`]) with its XML tags removed ([`without_xml_tags`]), trimmed. A line with no
/// such marker, and one left empty, is dropped, and the kept lines join by `\n`.
fn csharp_doc_text(raw: &str) -> String {
    raw.lines()
        .filter_map(|line| after_doc_slash(line.trim_start().strip_prefix("//")?))
        .map(|doc| without_xml_tags(doc).trim().to_string())
        .filter(|doc| !doc.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `line` with every XML tag removed: a `<` that an ASCII letter or a `/` follows, up to the first
/// `>` after it. A `<` that opens no tag, or that no `>` on the line closes, stays as written.
fn without_xml_tags(line: &str) -> String {
    let mut kept = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('<') {
        let tag = &rest[open + 1..];
        let close = tag
            .starts_with(|c: char| c.is_ascii_alphabetic() || c == '/')
            .then(|| tag.find('>'))
            .flatten();
        match close {
            Some(close) => {
                kept.push_str(&rest[..open]);
                rest = &tag[close + 1..];
            }
            None => {
                kept.push_str(&rest[..=open]);
                rest = tag;
            }
        }
    }
    kept.push_str(rest);
    kept
}

/// Whether the Rust block comment whose opening `/*` ends just before `comment` is a doc block:
/// `/**`, but not `/***` nor the empty plain comment `/**/`, or `/*!`.
fn opens_doc_block(comment: &str) -> bool {
    comment.starts_with('!')
        || comment
            .strip_prefix('*')
            .is_some_and(|rest| !rest.starts_with(['*', '/']))
}

/// The text of the Rust block comment whose opening `/*` ends just before `text`, up to the `*/`
/// that closes it, and the text after that closer: block comments nest, each `/*` inside opening
/// a level and each `*/` closing one, so the closer is the `*/` that returns the depth to zero,
/// however many lines on. The comment's text runs to `text`'s end, with none after it, when it is
/// unclosed.
fn until_block_closer(text: &str) -> (&str, Option<&str>) {
    let bytes = text.as_bytes();
    let mut depth = 1_usize;
    let mut at = 0;
    while at + 1 < bytes.len() {
        match (bytes[at], bytes[at + 1]) {
            (b'/', b'*') => depth += 1,
            (b'*', b'/') => depth -= 1,
            _ => {
                at += 1;
                continue;
            }
        }
        if depth == 0 {
            return (&text[..at], Some(&text[at + 2..]));
        }
        at += 2;
    }
    (text, None)
}

/// The lines of a Rust doc block's `body`, its text between the opener and the closer (or the
/// span's end when it is not `closed`), each trimmed. The opener's line is kept when it is not
/// blank. Every later line has one leading `*` stripped and is trimmed again, and is kept even
/// when blank, except the closer's line, which is kept only when it is not blank.
fn doc_block_lines(body: &str, closed: bool) -> impl Iterator<Item = &str> + '_ {
    let last = body.matches('\n').count();
    body.split('\n').enumerate().filter_map(move |(at, line)| {
        let line = line.trim();
        if at == 0 {
            return (!line.is_empty()).then_some(line);
        }
        let line = line.strip_prefix('*').unwrap_or(line).trim();
        (at < last || !closed || !line.is_empty()).then_some(line)
    })
}

/// The text after the Rust token `text` starts with. A string literal is read whole, to its own
/// closer: a plain `".."`, `b".."` or `c".."` ([`plain_literal`]), or a raw `r".."`, `br#".."#` or
/// `cr#".."#` ([`raw_literal`]). So is a char literal: `'x'`, `'"'`, or an escaped `'\''` or
/// `'\u{..}'` up to the next quote. A word is read to its last letter, digit or `_`, and any other
/// character, the quote of a lifetime or a label (`'a`) included, alone.
fn after_token(text: &str) -> &str {
    let word_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut chars = text.chars();
    match chars.next() {
        Some('"') => plain_literal(chars.as_str(), "\"").1,
        Some('\'') => {
            let quoted = chars.as_str();
            match (chars.next(), chars.next()) {
                (Some('\\'), Some(_)) => {
                    let escaped = chars.as_str();
                    escaped.find('\'').map_or("", |end| &escaped[end + 1..])
                }
                (Some(_), Some('\'')) => chars.as_str(),
                _ => quoted,
            }
        }
        Some(c) if word_char(c) => {
            let (word, after) = text.split_at(text.find(|c| !word_char(c)).unwrap_or(text.len()));
            match word {
                "r" | "br" | "cr" => raw_literal(after).map_or(after, |(_, after)| after),
                _ => after,
            }
        }
        _ => chars.as_str(),
    }
}

/// The body of the string literal whose opener ends just before `text`, as written, up to its own
/// `closer` - the first that no `\` escapes, a Rust plain literal's `"` or a Python string's quote
/// or three - and the text after that closer. The body runs to `text`'s end, with nothing after
/// it, when the literal is unclosed.
fn plain_literal<'t>(text: &'t str, closer: &str) -> (&'t str, &'t str) {
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if text[at..].starts_with(closer) {
            return (&text[..at], &text[at + closer.len()..]);
        }
    }
    (text, "")
}

/// The text of a Python docstring span's `raw` lines: the body of the string literal the span's
/// first quote opens - three of that quote when it opens three, else the one - up to its own
/// closer ([`plain_literal`]), joined with the body of each literal that continues it as an
/// implicit concatenation ([`next_python_part`]), as Python joins `"a" "b"` into `ab`, each
/// prefix (`r`, `u`) and whatever follows the last closer dropped and the escapes not decoded;
/// empty when the span holds no quote. The joined body is dedented as PEP 257 trims a docstring:
/// its first line trimmed; every later line stripped of the least indent of the later lines that
/// are not blank, a tab counting as one column, and its end trimmed; the blank lines before the
/// first line of text and after the last dropped; the lines joined by `\n`. The span's first
/// quote is read as the docstring's opener, so a docstring on its header's line after a quote of
/// the header's own (`def f(x="a"): """Doc."""`) is read from that quote.
fn python_doc_text(raw: &str) -> String {
    let Some(open) = raw.find(['"', '\'']) else {
        return String::new();
    };
    let mut body = String::new();
    let mut part = Some(&raw[open..]);
    while let Some(quoted) = part {
        let triple = quoted[..1].repeat(3);
        let closer = if quoted.starts_with(&triple) {
            triple.as_str()
        } else {
            &quoted[..1]
        };
        let (text, after) = plain_literal(&quoted[closer.len()..], closer);
        body.push_str(text);
        part = next_python_part(after);
    }
    let indent = |line: &str| line.chars().take_while(|c| c.is_whitespace()).count();
    let margin = body
        .split('\n')
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(indent)
        .min()
        .unwrap_or(0);
    let lines: Vec<&str> = body
        .split('\n')
        .enumerate()
        .map(|(at, line)| {
            if at == 0 {
                return line.trim();
            }
            let line = line.trim_end();
            line.char_indices()
                .nth(margin)
                .map_or("", |(cut, _)| &line[cut..])
        })
        .collect();
    let text = |line: &&str| !line.is_empty();
    match (lines.iter().position(text), lines.iter().rposition(text)) {
        (Some(first), Some(last)) => lines[first..=last].join("\n"),
        _ => String::new(),
    }
}

/// The text from the opening quote of the Python string literal that continues an implicit
/// concatenation after `text`, the text after a part's closer: the whitespace and line breaks, a
/// `\` that continues a line and each `#` comment to its line's end before it are passed over,
/// then its prefix's letters (`r`, `u`). None when anything else comes first: a `)`, a `;`, other
/// code or the span's end.
fn next_python_part(text: &str) -> Option<&str> {
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if let Some(comment) = rest.strip_prefix('#') {
            rest = comment.split_once('\n').map_or("", |(_, after)| after);
        } else if let Some(continued) = rest.strip_prefix('\\') {
            rest = continued;
        } else {
            break;
        }
    }
    let quoted = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    quoted.starts_with(['"', '\'']).then_some(quoted)
}

/// The body of the raw Rust string literal whose `r` ends just before `text` (`r".."`,
/// `r#".."#`), up to its own closer - the first `"` followed by as many `#` as its opener holds -
/// and the text after that closer; none when `text` opens no raw literal (`r#type`). The body runs
/// to `text`'s end, with nothing after it, when the literal is unclosed.
fn raw_literal(text: &str) -> Option<(&str, &str)> {
    let quoted = text.trim_start_matches('#');
    let hashes = &text[..text.len() - quoted.len()];
    let body = quoted.strip_prefix('"')?;
    let closer = body
        .match_indices('"')
        .map(|(at, _)| at)
        .find(|&at| body[at + 1..].starts_with(hashes));
    Some(closer.map_or((body, ""), |at| {
        (&body[..at], &body[at + 1 + hashes.len()..])
    }))
}

/// The body of the string literal of the Rust name-value doc attribute `text` starts with -
/// `#[doc = ".."]`, `#![doc = r#".."#]`, whitespace allowed between its tokens - as written, up to
/// the literal's own closer ([`plain_literal`], [`raw_literal`]), its escapes not decoded, and the
/// text after that closer; none for another attribute, `#[doc(..)]` included, and for a value that
/// is no string literal (`include_str!(..)`).
fn doc_attribute_text(text: &str) -> Option<(&str, &str)> {
    let attribute = text.strip_prefix('#')?.trim_start();
    let value = attribute
        .strip_prefix('!')
        .unwrap_or(attribute)
        .trim_start()
        .strip_prefix('[')?
        .trim_start()
        .strip_prefix("doc")?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    match value.strip_prefix('"') {
        Some(plain) => Some(plain_literal(plain, "\"")),
        None => raw_literal(value.strip_prefix('r')?),
    }
}

/// The symbols an extraction gives one code file, the bundled grammars' and a declared
/// extractor's alike: its definitions, its references, whether they cover only part of it and
/// the file's doc span (spec 117, DOC FACTS).
#[derive(Debug, PartialEq, Eq)]
struct CodeSymbols {
    defs: Vec<model::Def>,
    refs: Vec<SymRef>,
    partial: bool,
    doc: Option<(u32, u32)>,
}

/// A code file as an extraction reads it: the language it is read as and its symbols, none when
/// there is no text or a declared extractor's answer failed, and why when the bundled grammar
/// cannot tag the text.
struct CodeFile {
    lang: String,
    symbols: Option<Result<CodeSymbols, String>>,
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
    if is_workflow_doc(path) {
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
            if let Some(Ok(symbols)) = file.symbols {
                code_facts(path, &symbols, &mut facts);
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
/// its extension names and the symbols it extracts ([`extracted`]), none when there is no text
/// and why when the grammar cannot tag it; none at all when no grammar names the extension.
fn bundled_code(path: &str, text: Option<&str>) -> Option<CodeFile> {
    let grammar = registry::for_path(path, None)?;
    Some(CodeFile {
        lang: lang_str(grammar.lang).to_string(),
        symbols: extracted(path, || text, None)
            .flatten()
            .map(|(_, symbols)| {
                symbols.map(|symbols| CodeSymbols {
                    defs: symbols.defs,
                    refs: symbols.refs,
                    partial: symbols.partial,
                    doc: symbols.doc,
                })
            }),
    })
}

/// Fills the definitions, mentions, module declarations, parse-error marker and doc span of
/// `facts` from the `symbols` an extraction gave the file at `path`, each definition keeping its
/// own doc span and each reference becoming a mention, a tagged one kept once per site kind
/// ([`one_tagged_mention_per_site_kind`]).
fn code_facts(path: &str, symbols: &CodeSymbols, facts: &mut FileFacts) {
    let CodeSymbols {
        defs,
        refs,
        partial,
        doc,
    } = symbols;
    facts.partial = *partial;
    facts.doc = *doc;
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
            doc: d.doc,
        })
        .collect();
    facts.mentions = one_tagged_mention_per_site_kind(refs.iter().map(|r| Mention {
        enclosing: r.enclosing.as_deref().and_then(index_of),
        // Spec 117 (CAPTURES): the name as written, its qualifier joined to its leaf by `::`
        // for a path and `.` for a member access, the leaf alone with no qualifier.
        name: match &r.qualifier {
            Some(q) => qualified_name(&q.name, q.path, &r.name),
            None => r.name.clone(),
        },
        // The extractor names a reference by its bare leaf, as a definition's leaf is its name
        // whole, so the leaf is what the resolver finds the mention by, qualified or not.
        leaf: r.name.clone(),
        kind: match &r.ref_kind {
            None => MentionKind::Call,
            Some(kind) => MentionKind::Tagged(kind.clone()),
        },
        line: r.line,
        test: r.is_test,
    }));
    facts.modules = defs
        .iter()
        .filter(|d| d.is_out_of_line_module)
        .map(|d| {
            // Spec 117, MODULE ROOTS AT READ: both lists, off the declaring file alone, and the
            // `#[path]` mark the resolver reads to place the file this declaration reaches.
            let (as_root, as_child) = module_candidates(path, d);
            ModuleDecl {
                name: d.name.clone(),
                def: by_path[d.path.as_str()],
                test: d.is_test,
                as_root,
                as_child,
                path_attr: d.path_override.is_some(),
            }
        })
        .collect();
}

/// The `mentions` of one file in their order, every call kept, one per site, since a proof's
/// evidence lists each call's line, and one tagged mention kept per enclosing definition, name,
/// leaf, tag and test flag (spec 117, ONE TAGGED MENTION PER SITE KIND). It sits in the first
/// one's place and takes the lowest of their lines, since an extraction need not give a file's
/// tagged references in line order. The leaf keeps apart the two a declared extractor may write
/// alike: an unqualified `a.b` and `b` under the member `a`.
fn one_tagged_mention_per_site_kind(mentions: impl Iterator<Item = Mention>) -> Vec<Mention> {
    let mut kept: Vec<Mention> = Vec::new();
    let mut at: HashMap<_, usize> = HashMap::new();
    for mention in mentions {
        let MentionKind::Tagged(tag) = &mention.kind else {
            kept.push(mention);
            continue;
        };
        let site_kind = (
            mention.enclosing,
            mention.name.clone(),
            mention.leaf.clone(),
            tag.clone(),
            mention.test,
        );
        match at.entry(site_kind) {
            hash_map::Entry::Occupied(first) => {
                let first = &mut kept[*first.get()];
                first.line = first.line.min(mention.line);
            }
            hash_map::Entry::Vacant(none) => {
                none.insert(kept.len());
                kept.push(mention);
            }
        }
    }
    kept
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

/// The workflow definition's path relative to a project root, as the code store names its files:
/// the path whose bytes the code store's extractor reads as the workflow definition.
fn workflow_doc() -> String {
    format!("{RIGGER_DIR}/{WORKFLOW_FILE}")
}

/// Whether the file at `path` is the workflow definition ([`workflow_doc`]), which every
/// extraction reads as the workflow before any declaration or grammar is consulted.
fn is_workflow_doc(path: &str) -> bool {
    path == workflow_doc()
}

/// The text of a file's `bytes` as an extraction half reads it: none for no bytes and for bytes
/// that are not UTF-8, the two inputs every half maps to no facts.
fn text_of(bytes: Option<&[u8]>) -> Option<&str> {
    bytes.and_then(|bytes| std::str::from_utf8(bytes).ok())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::csharp_definition_sources::CSHARP_DEFINITIONS_SOURCE;
    use crate::extractor_stub::{
        answer_defining, declared_identity, request_for, root_dir_of, StubExtractor,
    };
    use crate::extractors_fixtures::{workflow_declaring_extractors, EXTRACTOR_REFUSALS};
    use crate::go_definition_sources::GO_DEFINITIONS_SOURCE;
    use crate::grounder::symbols::content_hash;
    use crate::grounder::symbols::extract::tests::compiled;
    use crate::grounder::symbols::extract::{extract, CompiledTags};
    use crate::js_ts_definition_sources::{
        JS_DEFINITIONS_SOURCE, TSX_DEFINITIONS_SOURCE, TS_DEFINITIONS_SOURCE,
    };
    use crate::python_definition_sources::PYTHON_DEFINITIONS_SOURCE;
    use crate::rust_definition_sources::RUST_DEFINITIONS_SOURCE;
    use crate::symbol_index::{mention_in, THREE_READS_AND_THREE_CALLS_SOURCE};
    use rigger_domain::code_view::CodeView;
    use rigger_domain::config::ExtractorDecl;
    use rigger_domain::contextgraph::{
        KIND_AGENT, KIND_DESIGN_DOC, KIND_GATE, KIND_RATIONALE, KIND_STAGE, REL_EXPLAINS, REL_RUNS,
        REL_SPECIFIES,
    };
    use rigger_store::code::Store;
    use std::collections::BTreeMap;

    /// The entry of the file at `path`, its blob named for the path.
    fn entry_of(path: &str) -> Entry {
        Entry {
            path: path.to_string(),
            blob: format!("blob-of-{path}"),
        }
    }

    fn facts_of(path: &str, bytes: &[u8]) -> FileFacts {
        FactsExtractor.extract(&entry_of(path), bytes)
    }

    /// The facts of the code file at `path` holding `source`, read as `lang` with its symbols
    /// extracted under the fixture query `tags` in place of the language's bundled one, through
    /// the one skeleton every extraction reads a file through.
    pub(crate) fn facts_under(
        path: &str,
        source: &str,
        lang: Lang,
        tags: &CompiledTags,
    ) -> FileFacts {
        let symbols = extract(source, lang, tags).expect("the fixture source extracts");
        file_facts(&entry_of(path), source.as_bytes(), |_, _| {
            Some(CodeFile {
                lang: lang_str(lang).to_string(),
                symbols: Some(Ok(CodeSymbols {
                    defs: symbols.defs,
                    refs: symbols.refs,
                    partial: symbols.partial,
                    doc: symbols.doc,
                })),
            })
        })
    }

    /// A definition kind's name as the facts spell it, written here rather than taken from the
    /// production mapping the facts are built with.
    fn kind_name(kind: crate::grounder::symbols::model::Kind) -> String {
        format!("{kind:?}").to_lowercase()
    }

    /// Every definition and every call the symbol extractor finds in each language's fixture is
    /// held, and its tagged references one per site kind (spec 117, ONE TAGGED MENTION PER SITE
    /// KIND): a tagged reference repeating an earlier one's enclosing definition, leaf, tag and
    /// test flag is held once, in the earlier one's place at the lower of their lines.
    #[test]
    fn facts_hold_every_def_and_call_and_one_tagged_mention_per_site_kind() {
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
                .and_then(|(_, symbols)| symbols.ok())
                .unwrap_or_else(|| panic!("{path}: the symbol extractor reads the fixture"));
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

            let mut found_refs: Vec<(String, String, u32, bool, Option<String>)> = Vec::new();
            for r in &symbols.refs {
                let kind = match &r.ref_kind {
                    None => "call".to_string(),
                    Some(kind) => format!("tagged:{kind}"),
                };
                let row = (r.name.clone(), kind, r.line, r.is_test, r.enclosing.clone());
                let repeated = r.ref_kind.as_ref().and_then(|_| {
                    found_refs.iter_mut().find(|held| {
                        (&held.0, &held.1, held.3, &held.4) == (&row.0, &row.1, row.3, &row.4)
                    })
                });
                match repeated {
                    Some(held) => held.2 = held.2.min(row.2),
                    None => found_refs.push(row),
                }
            }
            let held_refs: Vec<_> = facts
                .mentions
                .iter()
                .map(|m| {
                    assert_eq!(
                        m.name, m.leaf,
                        "{path}: the fixture writes no reference with a qualifier, so each is \
                         named by its leaf"
                    );
                    let kind = match &m.kind {
                        MentionKind::Call => "call".to_string(),
                        MentionKind::Tagged(kind) => format!("tagged:{kind}"),
                    };
                    (
                        m.leaf.clone(),
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

    /// Spec 117 (CAPTURES): each `@reference.<suffix>` capture of one fixture query keeps its
    /// suffix in the mention it yields: `class`, `type` and `interface` a `type` mention,
    /// `implementation` an `impl`, `constant` a `const`, `field` a `field` and `import` a `use`,
    /// while `call`, `send` and a suffix the design does not list yield a call.
    #[test]
    fn every_capture_keeps_its_suffix() {
        let tagged = |kind: &str| MentionKind::Tagged(kind.to_string());
        // (the capture's suffix, the kind of the mention it yields)
        let table = [
            ("class", tagged("type")),
            ("type", tagged("type")),
            ("interface", tagged("type")),
            ("implementation", tagged("impl")),
            ("constant", tagged("const")),
            ("field", tagged("field")),
            ("import", tagged("use")),
            ("call", MentionKind::Call),
            ("send", MentionKind::Call),
            ("unlisted", MentionKind::Call),
        ];
        // One call per row, `of_<suffix>()` on its own line, and one query pattern per row
        // capturing that call alone under the row's suffix.
        let leaf = |suffix: &str| format!("of_{suffix}");
        let source: String = table
            .iter()
            .map(|(suffix, _)| format!("{}()\n", leaf(suffix)))
            .collect();
        let query: String = table
            .iter()
            .map(|(suffix, _)| {
                format!(
                    "((call function: (identifier) @name) @reference.{suffix} \
                     (#eq? @name \"{}\"))\n",
                    leaf(suffix)
                )
            })
            .collect();
        let fixture = compiled(&tree_sitter_python::LANGUAGE.into(), &query);
        let facts = facts_under("tool/run.py", &source, Lang::Python, &fixture);
        let expected: Vec<Mention> = table
            .into_iter()
            .zip(1..)
            .map(|((suffix, kind), line)| Mention {
                enclosing: None,
                name: leaf(suffix),
                leaf: leaf(suffix),
                kind,
                line,
                test: false,
            })
            .collect();
        assert_eq!(facts.mentions, expected);
    }

    /// A CONSTRUCTOR STAYS A CALL (spec 117, CAPTURES): a `@reference.class` capture on one of
    /// its registry entry's `constructors` node kinds yields a call mention, so a TypeScript, a
    /// TSX, a JavaScript and a C# `new Square()` each credit `Square` as called, while the C#
    /// local's declared type, a `@reference.class` capture on no constructor, stays a `type`
    /// mention.
    #[test]
    fn a_constructor_stays_a_call() {
        let square = |kind: MentionKind| Mention {
            enclosing: None,
            name: "Square".to_string(),
            leaf: "Square".to_string(),
            kind,
            line: 1,
            test: false,
        };
        let call = || square(MentionKind::Call);
        // (path, source, the mentions its facts hold)
        let table = [
            ("web/shape.ts", "new Square();\n", vec![call()]),
            ("web/shape.tsx", "new Square();\n", vec![call()]),
            ("web/shape.js", "new Square();\n", vec![call()]),
            (
                "App/Shape.cs",
                "Square s = new Square();\n",
                vec![square(MentionKind::Tagged("type".to_string())), call()],
            ),
        ];
        let (found, expected): (Vec<_>, Vec<_>) = table
            .into_iter()
            .map(|(path, source, mentions)| {
                (
                    (path, facts_of(path, source.as_bytes()).mentions),
                    (path, mentions),
                )
            })
            .unzip();
        assert_eq!(found, expected);
    }

    /// A QUALIFIER CAPTURE NAMES THE MENTION (spec 117, CAPTURES): a mention is named
    /// `<qualifier>::<leaf>` under a path qualifier and `<qualifier>.<leaf>` under a member one,
    /// its leaf always the bare leaf. A fixture query's `@qualifier.path` and `@qualifier.member`
    /// captures name a Python `Shape.area` call and field read `Shape::area` and `Shape.area`,
    /// the capture deciding the qualifier over the text a `@name` holding it reads. A `@name`
    /// with named children is split: its named leaves outside the type arguments, joined by `.`
    /// or `::`, give the last as the leaf and the text before its separator as the qualifier, so
    /// a C# base type `Ns.IShape` yields the leaf `IShape`, `global::IShape` the leaf `IShape`
    /// under a path, `List<IShape>` the leaf `List`, its type argument `IShape` a mention of its
    /// own, and a JavaScript `new Ns.Kit.Box()` a call of `Box`, its inner `Ns.Kit` a field use
    /// of `Kit`; a `@name` that is no such chain (`new arr[0]()`, `new a[0].b()`, `new (Box)()`)
    /// stages no mention.
    #[test]
    fn a_qualifier_capture_names_the_mention() {
        let mention = |name: &str, leaf: &str, kind: MentionKind, enclosing: Option<u32>| Mention {
            enclosing,
            name: name.to_string(),
            leaf: leaf.to_string(),
            kind,
            line: 1,
            test: false,
        };
        let tagged = |kind: &str| MentionKind::Tagged(kind.to_string());
        // (path, source, the Python fixture query its symbols are extracted under, or none for
        // the bundled grammar, the mentions its facts hold)
        let table = [
            (
                "tool/run.py",
                "Shape.area()\n",
                Some(
                    "(call function: (attribute object: (_) @qualifier.path \
                     attribute: (identifier) @name)) @reference.call",
                ),
                vec![mention("Shape::area", "area", MentionKind::Call, None)],
            ),
            (
                "tool/run.py",
                "Shape.area\n",
                Some(
                    "(expression_statement (attribute object: (_) @qualifier.member \
                     attribute: (identifier) @name)) @reference.field",
                ),
                vec![mention("Shape.area", "area", tagged("field"), None)],
            ),
            (
                "tool/run.py",
                "Shape.area()\n",
                Some(
                    "(call function: (attribute object: (_) @qualifier.path) @name) \
                     @reference.call",
                ),
                vec![mention("Shape::area", "area", MentionKind::Call, None)],
            ),
            (
                "App/Shape.cs",
                "class A : Ns.IShape { }\n",
                None,
                vec![mention("Ns.IShape", "IShape", tagged("impl"), Some(0))],
            ),
            (
                "App/Shape.cs",
                "class A : global::IShape { }\n",
                None,
                vec![mention("global::IShape", "IShape", tagged("impl"), Some(0))],
            ),
            (
                "App/Shape.cs",
                "class A : List<IShape> { }\n",
                None,
                vec![
                    mention("IShape", "IShape", tagged("type"), Some(0)),
                    mention("List", "List", tagged("impl"), Some(0)),
                ],
            ),
            (
                "web/make.js",
                "new Ns.Kit.Box();\n",
                None,
                vec![
                    mention("Ns.Kit", "Kit", tagged("field"), None),
                    mention("Ns.Kit.Box", "Box", MentionKind::Call, None),
                ],
            ),
            ("web/make.js", "new arr[0]();\n", None, vec![]),
            ("web/make.js", "new a[0].b();\n", None, vec![]),
            ("web/make.js", "new (Box)();\n", None, vec![]),
        ];
        let (found, expected): (Vec<_>, Vec<_>) = table
            .into_iter()
            .map(|(path, source, fixture, mentions)| {
                let facts = match fixture {
                    Some(query) => facts_under(
                        path,
                        source,
                        Lang::Python,
                        &compiled(&tree_sitter_python::LANGUAGE.into(), query),
                    ),
                    None => facts_of(path, source.as_bytes()),
                };
                ((source, facts.mentions), (source, mentions))
            })
            .unzip();
        assert_eq!(found, expected);
    }

    /// ONE TAGGED MENTION PER SITE KIND (spec 117): a Rust function reading one constant three
    /// times holds one `const` mention, at the first read's line, while its three calls to one
    /// function stay three call mentions, one per call's line.
    #[test]
    fn one_tagged_mention_per_site_kind() {
        let facts = facts_of("src/lib.rs", THREE_READS_AND_THREE_CALLS_SOURCE.as_bytes());
        assert_eq!(
            facts
                .defs
                .iter()
                .map(|d| d.local.as_str())
                .collect::<Vec<_>>(),
            ["LIMIT", "g", "f"],
            "the definitions, `f` at index 2"
        );
        assert_eq!(
            facts.mentions,
            [
                mention_in(Some(2), "g", "g", None, 5, false),
                mention_in(Some(2), "g", "g", None, 7, false),
                mention_in(Some(2), "g", "g", None, 9, false),
                mention_in(Some(2), "LIMIT", "LIMIT", Some("const"), 4, false),
            ]
        );
    }

    /// ONE TAGGED MENTION PER SITE KIND (spec 117), part by part: of an extraction's tagged
    /// references, one is kept per enclosing definition, name, leaf, tag and test flag, in the
    /// first one's place at the lowest of their lines, and one differing from it in any single
    /// part is kept beside it; every call is kept, one per site, two on one line included.
    #[test]
    fn a_tagged_mention_is_kept_once_per_enclosing_name_leaf_tag_and_test_flag() {
        use crate::symbol_index::{def, qualifier, sym_ref_of_kind};
        let defs = vec![
            def(model::Kind::Function, "f"),
            def(model::Kind::Function, "g"),
        ];
        let qualified = |q: &str, path: bool, r: SymRef| SymRef {
            qualifier: Some(qualifier(q, path)),
            ..r
        };
        // (the case, the references the extraction gives, the mentions the facts hold; `f` is
        // the definition at index 0 and `g` the one at index 1)
        let table = [
            ("no reference", vec![], vec![]),
            (
                "a repeat in one definition",
                vec![
                    sym_ref_of_kind("LIMIT", 3, Some("f"), Some("const"), false),
                    sym_ref_of_kind("LIMIT", 5, Some("f"), Some("const"), false),
                    sym_ref_of_kind("LIMIT", 7, Some("f"), Some("const"), false),
                ],
                vec![mention_in(
                    Some(0),
                    "LIMIT",
                    "LIMIT",
                    Some("const"),
                    3,
                    false,
                )],
            ),
            (
                "a repeat at a lower line, after a call",
                vec![
                    sym_ref_of_kind("LIMIT", 5, Some("f"), Some("const"), false),
                    sym_ref_of_kind("h", 6, Some("f"), None, false),
                    sym_ref_of_kind("LIMIT", 3, Some("f"), Some("const"), false),
                ],
                vec![
                    mention_in(Some(0), "LIMIT", "LIMIT", Some("const"), 3, false),
                    mention_in(Some(0), "h", "h", None, 6, false),
                ],
            ),
            (
                "a repeat at file level",
                vec![
                    sym_ref_of_kind("LIMIT", 1, None, Some("const"), false),
                    sym_ref_of_kind("LIMIT", 2, None, Some("const"), false),
                ],
                vec![mention_in(None, "LIMIT", "LIMIT", Some("const"), 1, false)],
            ),
            (
                "another enclosing definition, or none",
                vec![
                    sym_ref_of_kind("LIMIT", 3, Some("f"), Some("const"), false),
                    sym_ref_of_kind("LIMIT", 5, Some("g"), Some("const"), false),
                    sym_ref_of_kind("LIMIT", 7, None, Some("const"), false),
                ],
                vec![
                    mention_in(Some(0), "LIMIT", "LIMIT", Some("const"), 3, false),
                    mention_in(Some(1), "LIMIT", "LIMIT", Some("const"), 5, false),
                    mention_in(None, "LIMIT", "LIMIT", Some("const"), 7, false),
                ],
            ),
            (
                "another tag",
                vec![
                    sym_ref_of_kind("LIMIT", 3, Some("f"), Some("const"), false),
                    sym_ref_of_kind("LIMIT", 4, Some("f"), Some("type"), false),
                ],
                vec![
                    mention_in(Some(0), "LIMIT", "LIMIT", Some("const"), 3, false),
                    mention_in(Some(0), "LIMIT", "LIMIT", Some("type"), 4, false),
                ],
            ),
            (
                "another test flag",
                vec![
                    sym_ref_of_kind("LIMIT", 3, Some("f"), Some("const"), false),
                    sym_ref_of_kind("LIMIT", 4, Some("f"), Some("const"), true),
                ],
                vec![
                    mention_in(Some(0), "LIMIT", "LIMIT", Some("const"), 3, false),
                    mention_in(Some(0), "LIMIT", "LIMIT", Some("const"), 4, true),
                ],
            ),
            (
                "another name: `x.f` beside `T { f }`",
                vec![
                    qualified(
                        "x",
                        false,
                        sym_ref_of_kind("f", 3, Some("f"), Some("field"), false),
                    ),
                    sym_ref_of_kind("f", 4, Some("f"), Some("field"), false),
                ],
                vec![
                    mention_in(Some(0), "x.f", "f", Some("field"), 3, false),
                    mention_in(Some(0), "f", "f", Some("field"), 4, false),
                ],
            ),
            (
                "another name: `x.f` beside `x::f`",
                vec![
                    qualified(
                        "x",
                        false,
                        sym_ref_of_kind("f", 3, Some("f"), Some("field"), false),
                    ),
                    qualified(
                        "x",
                        true,
                        sym_ref_of_kind("f", 4, Some("f"), Some("field"), false),
                    ),
                ],
                vec![
                    mention_in(Some(0), "x.f", "f", Some("field"), 3, false),
                    mention_in(Some(0), "x::f", "f", Some("field"), 4, false),
                ],
            ),
            (
                "a repeat under one qualifier",
                vec![
                    qualified(
                        "x",
                        false,
                        sym_ref_of_kind("f", 3, Some("f"), Some("field"), false),
                    ),
                    qualified(
                        "x",
                        false,
                        sym_ref_of_kind("f", 5, Some("f"), Some("field"), false),
                    ),
                ],
                vec![mention_in(Some(0), "x.f", "f", Some("field"), 3, false)],
            ),
            (
                "another leaf: an unqualified `a.b` beside `b` under the member `a`",
                vec![
                    sym_ref_of_kind("a.b", 3, Some("f"), Some("field"), false),
                    qualified(
                        "a",
                        false,
                        sym_ref_of_kind("b", 4, Some("f"), Some("field"), false),
                    ),
                ],
                vec![
                    mention_in(Some(0), "a.b", "a.b", Some("field"), 3, false),
                    mention_in(Some(0), "a.b", "b", Some("field"), 4, false),
                ],
            ),
            (
                "three calls to one function",
                vec![
                    sym_ref_of_kind("g", 5, Some("f"), None, false),
                    sym_ref_of_kind("g", 7, Some("f"), None, false),
                    sym_ref_of_kind("g", 9, Some("f"), None, false),
                ],
                vec![
                    mention_in(Some(0), "g", "g", None, 5, false),
                    mention_in(Some(0), "g", "g", None, 7, false),
                    mention_in(Some(0), "g", "g", None, 9, false),
                ],
            ),
            (
                "two calls on one line",
                vec![
                    sym_ref_of_kind("g", 5, Some("f"), None, false),
                    sym_ref_of_kind("g", 5, Some("f"), None, false),
                ],
                vec![
                    mention_in(Some(0), "g", "g", None, 5, false),
                    mention_in(Some(0), "g", "g", None, 5, false),
                ],
            ),
        ];
        let (found, expected): (Vec<_>, Vec<_>) = table
            .into_iter()
            .map(|(case, refs, mentions)| {
                let symbols = CodeSymbols {
                    defs: defs.clone(),
                    refs,
                    partial: false,
                    doc: None,
                };
                let mut facts = FileFacts::default();
                code_facts("src/lib.rs", &symbols, &mut facts);
                ((case, facts.mentions), (case, mentions))
            })
            .unzip();
        assert_eq!(found, expected);
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

    /// Spec 117 (DOC FACTS): the doc spans the bundled extraction gives a code file reach its
    /// facts, the file's as the file's doc span and each definition's as that definition's: a Rust
    /// file's by the Rust rule, a Go file's through its query's `@doc` capture, and a file with no
    /// doc line has none.
    #[test]
    fn a_files_doc_spans_reach_its_facts() {
        // (path, source, the file's doc span, the (leaf, doc span) of each definition)
        type Spans = &'static [(&'static str, Option<(u32, u32)>)];
        type Row = (&'static str, &'static str, Option<(u32, u32)>, Spans);
        let table: [Row; 3] = [
            (
                "src/lib.rs",
                "//! The crate.\n\n/// F.\nfn f() {}\nfn g() {}\n",
                Some((1, 1)),
                &[("f", Some((3, 3))), ("g", None)],
            ),
            ("src/lib.rs", "fn plain() {}\n", None, &[("plain", None)]),
            (
                "p.go",
                "package p\n\n// F reads.\n// It reads.\nfunc F() {}\n",
                None,
                &[("F", Some((3, 4)))],
            ),
        ];
        for (path, source, doc, defs) in table {
            let facts = facts_of(path, source.as_bytes());
            let held: Vec<_> = facts
                .defs
                .iter()
                .map(|d| (d.leaf.as_str(), d.doc))
                .collect();
            assert_eq!((facts.doc, held.as_slice()), (doc, defs), "{path}");
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

    /// Spec 117, MODULE ROOTS AT READ: each out-of-line declaration's module row holds its name,
    /// its test mark, the `as_root` and `as_child` lists `module_candidates` names off the
    /// declaring file alone, and whether it carries a `#[path]` attribute.
    #[test]
    fn module_candidates_come_from_the_declaring_file_alone() {
        // (name, test, as_root, as_child, path_attr) of one declaration
        type Module = (
            &'static str,
            bool,
            &'static [&'static str],
            &'static [&'static str],
            bool,
        );
        // (declaring path, source, each declaration's module row)
        type Row = (&'static str, &'static str, &'static [Module]);
        let table: [Row; 10] = [
            (
                "src/lib.rs",
                "mod alpha;\n",
                &[(
                    "alpha",
                    false,
                    &["src/alpha.rs", "src/alpha/mod.rs"],
                    &["src/alpha.rs", "src/alpha/mod.rs"],
                    false,
                )],
            ),
            (
                "lib/other.rs",
                "mod alpha;\n",
                &[(
                    "alpha",
                    false,
                    &["lib/alpha.rs", "lib/alpha/mod.rs"],
                    &["lib/other/alpha.rs", "lib/other/alpha/mod.rs"],
                    false,
                )],
            ),
            (
                "src/lib.rs",
                "#[cfg(test)]\nmod tests;\n",
                &[(
                    "tests",
                    true,
                    &["src/tests.rs", "src/tests/mod.rs"],
                    &["src/tests.rs", "src/tests/mod.rs"],
                    false,
                )],
            ),
            (
                "src/parent.rs",
                "#[path = \"other/x.rs\"]\nmod renamed;\n",
                &[(
                    "renamed",
                    false,
                    &["src/other/x.rs"],
                    &["src/other/x.rs"],
                    true,
                )],
            ),
            (
                "src/lib.rs",
                "mod outer {\n    mod x;\n}\n",
                &[(
                    "x",
                    false,
                    &["src/outer/x.rs", "src/outer/x/mod.rs"],
                    &["src/outer/x.rs", "src/outer/x/mod.rs"],
                    false,
                )],
            ),
            (
                "src/lib.rs",
                "#![cfg(test)]\nfn first() {}\nmod x;\n",
                &[(
                    "x",
                    true,
                    &["src/x.rs", "src/x/mod.rs"],
                    &["src/x.rs", "src/x/mod.rs"],
                    false,
                )],
            ),
            (
                "src/main.rs",
                "#[cfg(test)]\nuse y;\nfn first() {}\nmod z;\n",
                &[(
                    "z",
                    false,
                    &["src/z.rs", "src/z/mod.rs"],
                    &["src/z.rs", "src/z/mod.rs"],
                    false,
                )],
            ),
            (
                "src/lib.rs",
                "mod outer {\n    #[cfg(test)]\n    mod x;\n}\n",
                &[(
                    "x",
                    true,
                    &["src/outer/x.rs", "src/outer/x/mod.rs"],
                    &["src/outer/x.rs", "src/outer/x/mod.rs"],
                    false,
                )],
            ),
            ("src/lib.rs", "mod inline { fn f() {} }\n", &[]),
            ("web/app.ts", "namespace N { function f() {} }\n", &[]),
        ];
        for (path, source, modules) in table {
            let facts = facts_of(path, source.as_bytes());
            let held = (facts.modules.iter())
                .map(|m| {
                    (
                        m.name.as_str(),
                        m.test,
                        m.as_root.iter().map(String::as_str).collect::<Vec<_>>(),
                        m.as_child.iter().map(String::as_str).collect::<Vec<_>>(),
                        m.path_attr,
                    )
                })
                .collect::<Vec<_>>();
            let expected = (modules.iter())
                .map(|&(name, test, as_root, as_child, path_attr)| {
                    (name, test, as_root.to_vec(), as_child.to_vec(), path_attr)
                })
                .collect::<Vec<_>>();
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

    /// A CHANGE TO THE FACTS SHIPS WITH A BUMP, and EACH LANGUAGE PINS ITS OWN ROWS (spec 117):
    /// the golden fixtures read into facts. Each registry entry's code fixtures - the symbol
    /// golden's sources keyed by that entry's extension, and for Rust a test file - are digested
    /// in one row beside that entry's tags generation; a design document, a source file's
    /// rationale and a workflow definition are digested in one row beside `FACTS_GENERATION`. So
    /// a change to one language's extraction fails that language's row until its generation is
    /// bumped, and a change to the design extractor, the workflow extractor or the conversion
    /// fails until `FACTS_GENERATION` is bumped and every row it moves re-pinned with it.
    #[test]
    fn a_change_to_the_facts_ships_with_a_generation_bump() {
        use crate::grounder::symbols::registry::tests::{
            generation_rows, pinned_rows, GOLDEN_SOURCES,
        };
        // One row per line with a comment line between rows, so units re-pinning different
        // languages' rows merge clean.
        const PINNED: [(&str, &str, &str); 7] = [
            // Rust
            ("rs", "rust-tags-v5", "d2d32a872e13880d"),
            // C#
            ("cs", "csharp-tags-v5", "1686734362d08137"),
            // TypeScript
            ("ts", "typescript-tags-v6", "59a7286ef00e8791"),
            // TSX
            ("tsx", "tsx-tags-v6", "f99cfdbc67a9dbe4"),
            // JavaScript
            ("js", "javascript-tags-v6", "b0392ae368f5f278"),
            // Go
            ("go", "go-tags-v4", "dc89c1ebc294684f"),
            // Python
            ("py", "python-tags-v2", "ca3c7d0d1a679ab9"),
        ];
        const PINNED_FACTS: (&str, &str) = ("facts-v6", "006133eb2aae6c76");
        fn digest(facts: &impl std::fmt::Debug) -> String {
            format!("{:016x}", fnv1a_64(format!("{facts:#?}").as_bytes()))
        }
        // A code fixture's path counts its index among its own extension's fixtures, so a fixture
        // added for one language moves no other language's paths.
        let mut code: Vec<(&str, FileFacts)> = Vec::new();
        for (ext, source) in GOLDEN_SOURCES {
            let i = code.iter().filter(|(keyed, _)| *keyed == ext).count();
            code.push((
                ext,
                facts_of(&format!("golden/{i}.{ext}"), source.as_bytes()),
            ));
        }
        code.push(("rs", facts_of("tests/it.rs", b"fn works() { helper(); }\n")));
        let rest: Vec<FileFacts> = [
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
        .iter()
        .map(|(path, source)| facts_of(path, source.as_bytes()))
        .collect();
        let facts: Vec<&FileFacts> = code.iter().map(|(_, facts)| facts).chain(&rest).collect();
        let any = |held: &dyn Fn(&FileFacts) -> bool| facts.iter().any(|f| held(f));
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
            (
                "a qualified mention",
                any(&|f| f.mentions.iter().any(|m| m.name != m.leaf)),
            ),
            ("a module declaration", any(&|f| !f.modules.is_empty())),
            (
                "a module's two lists apart",
                any(&|f| f.modules.iter().any(|m| m.as_root != m.as_child)),
            ),
            (
                "a module's `#[path]` mark",
                any(&|f| f.modules.iter().any(|m| m.path_attr)),
            ),
            (
                "a definition's doc span",
                any(&|f| f.defs.iter().any(|d| d.doc.is_some())),
            ),
            ("a file's doc span", any(&|f| f.doc.is_some())),
            ("a design concept", any(&|f| !f.doc_concepts.is_empty())),
            ("a design link", any(&|f| !f.doc_links.is_empty())),
            ("a workflow node", any(&|f| !f.wf_nodes.is_empty())),
            ("a workflow relation", any(&|f| !f.wf_relations.is_empty())),
        ] {
            assert!(covered, "the golden fixtures hold {part}");
        }
        assert_eq!(
            generation_rows(&code, |keyed| digest(&keyed)),
            pinned_rows(&PINNED),
            "each entry's tags generation and the digest of its code fixtures' facts (left) differ \
             from the pinned rows: a change to one language's extraction ships with a bump of \
             that language's generation macro in model.rs, re-pinning its row here and in the \
             registry golden; a change to the conversion ships with a bump of \
             `FACTS_GENERATION`, re-pinning its row below and each language row it moves; either \
             re-pins to this build's values together"
        );
        assert_eq!(
            (FACTS_GENERATION, digest(&rest).as_str()),
            PINNED_FACTS,
            "this build's `FACTS_GENERATION` and the digest of the design, rationale and workflow \
             facts (left) differ from the pinned row: a change to the design extractor, the \
             workflow extractor or the conversion ships with a bump of `FACTS_GENERATION`, so \
             bump it and re-pin this row, and each language row it moves, to this build's values \
             together"
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
    /// declaration is in force under it for `store_fingerprint` and for a reader of the head, in
    /// the root or in a worktree whose own workflow declares another (a reader reads the root's
    /// declaration), is not in force under another declaration at the root, and is again once the
    /// first returns.
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
        let worktree = project_declaring(Some(&b));
        let head = DirectoryHead::new(root)
            .head(worktree.path())
            .expect("read the head from a worktree declaring B");
        assert_eq!(
            head.view.stamp(),
            &stamp,
            "a reader in a worktree declaring B reads the root's declaration, A's store"
        );
        write_workflow(root, &b);
        assert_eq!(in_force(), None, "not in force under B");
        write_workflow(root, &a);
        assert_eq!(in_force(), Some(stamp), "in force again under A");
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
                        doc: None,
                    },
                    Def {
                        local: "T::new".into(),
                        leaf: "new".into(),
                        kind: "method".into(),
                        line: 3,
                        end_line: 4,
                        parent: Some(0),
                        test: true,
                        doc: None,
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
                        doc: None,
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
                    .map(|name| request_for(&format!("{name}.zig"), "x\n"))
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

    /// A SHOWN BODY IS BOUNDED AS THE STORE EXTRACTS IT: under a declaration named `rust` over
    /// `rs` and `yml`, `defs` gives a listed file the leaves of its answer - none when the answer
    /// fails - a file of a bundled language its grammar's, and the workflow definition and a
    /// document none; only the `.rs` files were asked of the extractor, so the workflow
    /// definition is read as the workflow before any declaration is consulted.
    #[cfg(unix)]
    #[test]
    fn a_files_definitions_are_read_as_the_store_reads_them() {
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let project = stub_project(&stub, "rust", "[rs, yml]", &[]);
        stub.answers("src/lib.rs", &answer_defining("src/lib.rs", "declared"));
        let workflow = std::fs::read_to_string(project.path().join(workflow_doc()))
            .expect("read the workflow");
        let extractor = ProjectExtractor::of(project.path());
        // (the file, its text, the leaves of its definitions)
        let rows: [(&str, &str, &[&str]); 5] = [
            ("src/lib.rs", "fn held() {}\n", &["declared"]),
            ("src/gone.rs", "fn held() {}\n", &[]),
            ("tool/run.py", "def held():\n    pass\n", &["held"]),
            (".rigger/workflow.yml", &workflow, &[]),
            ("docs/a-design.md", "# A design\n\n## Parts\n", &[]),
        ];
        for (path, text, leaves) in rows {
            let held = extractor
                .defs(path, text)
                .map(|defs| defs.into_iter().map(|d| d.name).collect::<Vec<_>>());
            assert_eq!(
                held,
                Ok(leaves.iter().map(|leaf| leaf.to_string()).collect()),
                "{path}"
            );
        }
        assert_eq!(
            (stub.starts(), stub.requests().len()),
            (vec![root_dir_of(project.path())], 2),
            "the `.rs` files were asked of one session, no other file of any"
        );
    }

    /// A READ HEAD HOLDS NO SESSION: over a project declaring the stub for `zig`, a reader that
    /// reads the store's head - from the root, which advances the store, and from a worktree
    /// differing in the declared file, which extracts that file again - holds a head whose every
    /// session of the stub has already ended, so a read that ends by exiting leaves no extractor
    /// running, and the head still carries the project's extractor, its doc text read with none.
    #[cfg(unix)]
    #[test]
    fn a_read_head_holds_the_extractor_with_every_session_closed() {
        let kept = tempfile::tempdir().expect("tempdir");
        let stub = StubExtractor::new(kept.path());
        let project = stub_project(&stub, "zig", "[zig]", &[("src/a.zig", "a\n")]);
        stub.answers("src/a.zig", &answer_defining("src/a.zig", "a"));
        let root = project.path();
        let workflow =
            std::fs::read_to_string(root.join(workflow_doc())).expect("read the workflow");
        let worktree = project_declaring(Some(&workflow));
        std::fs::create_dir_all(worktree.path().join("src")).expect("create the source directory");
        std::fs::write(worktree.path().join("src/a.zig"), "b\n").expect("change the file");
        // (the case, the reader's worktree, the stub's starts and ends while the head is held)
        let rows: [(&str, &Path, usize); 2] = [
            ("the root", root, 1),
            (
                "a worktree differing in the declared file",
                worktree.path(),
                2,
            ),
        ];
        for (case, reader, sessions) in rows {
            let head = DirectoryHead::new(root)
                .head(reader)
                .expect("read the head");
            let doc = (head.extract.as_deref()).map(|extract| extract.doc_text("zig", " a \n b"));
            assert_eq!(
                (stub.starts().len(), stub.ends().len(), doc.as_deref()),
                (sessions, sessions, Some("a\nb")),
                "{case}"
            );
        }
    }

    /// The doc text of a span, each row (the language, the span's raw lines, the doc text): Rust
    /// spans, `\r\n` endings, `#[doc(hidden)]` lines, nested block comments and literals holding
    /// a comment marker or a quote among them, C# `///` spans holding XML tags and a `<` that
    /// opens none, Python docstrings, one joined from parts and one in parentheses among them, and
    /// the lines of a language with no doc rule.
    const DOC_TEXT_ROWS: &[(&str, &str, &str)] = &[
        (
            "rust",
            "/// Reads the file.\r\n#[doc(hidden)]\r\n///\r\n/// Then answers it.\r\n",
            "Reads the file.\n\nThen answers it.",
        ),
        (
            "rust",
            "    /// First.\n    // a plain comment\n    //// four slashes\n\n    #[must_use]\n    ///tight\n",
            "First.\ntight",
        ),
        (
            "rust",
            "//! The module.\n//!\n//! Its parts.",
            "The module.\n\nIts parts.",
        ),
        (
            "rust",
            "/**\r\n * Block first.\r\n *\r\n * Block second.\r\n */\r\n",
            "Block first.\n\nBlock second.",
        ),
        ("rust", "/** One line. */", "One line."),
        (
            "rust",
            "/*! Opens here\n   no star\n ** two stars\n closes here */",
            "Opens here\nno star\n* two stars\ncloses here",
        ),
        (
            "rust",
            "/*** not a doc ***/\n/**/\n/** Kept. */ fn f() {}\n/// After.",
            "Kept.\nAfter.",
        ),
        ("rust", "/** Open\n * still open", "Open\nstill open"),
        (
            "rust",
            "/**\n * Block.\n */\n// plain after\n/// After.",
            "Block.\nAfter.",
        ),
        (
            "rust",
            "#[doc = \"Value doc.\"]\n#![doc = \"Inner doc.\"]\n# [ doc = \"Spaced.\" ]\n#[doc=\"  Tight.  \"]",
            "Value doc.\nInner doc.\nSpaced.\nTight.",
        ),
        (
            "rust",
            r##"#[doc = r#"Raw "quoted"."#]
#[doc = r"Raw plain."]
#[doc = "Escaped \"as written\"."]
#[doc = "Unclosed"##,
            r#"Raw "quoted".
Raw plain.
Escaped \"as written\".
Unclosed"#,
        ),
        (
            "rust",
            "#[doc = include_str!(\"README.md\")]\n#[doc = raw_text!()]\n#[doc(alias = \"x\")]\n#[docs = \"no\"]\n#[cfg_attr(test, doc = \"conditional\")]\n/// Only this.",
            "Only this.",
        ),
        ("rust", "#doc = \"No bracket.\"\n/// Only this.", "Only this."),
        ("rust", "/// Last.\r", "Last."),
        ("rust", "#[doc(hidden)]\n// plain\n", ""),
        ("rust", "", ""),
        (
            "go",
            "  // A Go doc.\r\n\t// Its second line.  \r\n\n",
            "// A Go doc.\n// Its second line.\n",
        ),
        (
            "python",
            "    \"\"\"Area of the shape.\n\n    Computed in units.\n      Indented more.   \n    \"\"\"\n",
            "Area of the shape.\n\nComputed in units.\n  Indented more.",
        ),
        (
            "python",
            "    \"\"\"\r\n    Starts below.\r\n\r\n    Then more.\r\n    \"\"\"\r\n",
            "Starts below.\n\nThen more.",
        ),
        (
            "python",
            "\t\"\"\"Mixed.\n\tTab in.\n    Four in.\n\t\"\"\"",
            "Mixed.\nTab in.\n   Four in.",
        ),
        ("python", "    \"\"\"One line.\"\"\"\n", "One line."),
        ("python", "\"\"\"  Spaced.  \"\"\"", "Spaced."),
        ("python", "'''Single.'''", "Single."),
        ("python", "    'Plain.'\n", "Plain."),
        ("python", "r\"\"\"Raw \\d.\"\"\"", "Raw \\d."),
        ("python", "'It\\'s here.'", "It\\'s here."),
        (
            "python",
            "\"\"\"Say 'hi', \"bye\" and \\\"\"\" too.\"\"\"",
            "Say 'hi', \"bye\" and \\\"\"\" too.",
        ),
        ("python", "'a\"\"\"b'", "a\"\"\"b"),
        ("python", "    \"\"\"Doc.\"\"\"  # a \"note\"\n", "Doc."),
        ("python", "\"\"\"\"\"\"", ""),
        ("python", "''", ""),
        (
            "python",
            "    \"\"\"Unclosed\n    still open\n",
            "Unclosed\nstill open",
        ),
        ("python", "    pass\n", ""),
        ("python", "def one(): r\"\"\"Raw \\d.\"\"\"\n", "Raw \\d."),
        ("python", "    \"a\" 'b'\n", "ab"),
        ("python", "    \"a\" U'b' R\"c\"\n", "abc"),
        ("python", "    \"a\" \\\n    r\"\\d\"\n", "a\\d"),
        ("python", "    (\"P.\")\n", "P."),
        (
            "python",
            "    (\"long \"  # it's \"x\"\n     \"doc\")\n",
            "long doc",
        ),
        (
            "python",
            "    (\"\"\"First.\n\n        Indented.\n    \"\"\" \"\"\"    Tail.\"\"\")\n",
            "First.\n\nIndented.\nTail.",
        ),
        ("python", "    \"a\"; x = \"q\"\n", "a"),
        (
            "zig",
            "/// Not read as Rust.\n #[doc(hidden)] ",
            "/// Not read as Rust.\n#[doc(hidden)]",
        ),
        (
            "rust",
            "/**\n * ```\n * /* set up */\n * let x = 1;\n * ```\n */",
            "```\n/* set up */\nlet x = 1;\n```",
        ),
        ("rust", "/** a /* b */ c */", "a /* b */ c"),
        (
            "rust",
            "/// a\n/* commented out\n/// not doc\n*/\n/// b",
            "a\nb",
        ),
        ("rust", "#[doc = \"Doc.\"] // see \"x\"", "Doc."),
        ("rust", "#[doc = \"a\"] #[cfg(feature = \"y\")]", "a"),
        (
            "rust",
            r###"#[doc = r##"Holds "# inside."##]"###,
            r##"Holds "# inside."##,
        ),
        ("rust", "#[doc = \"a\"] /* x\n/// y\n*/ /// b", "a\nb"),
        ("rust", "/* a /* b */ /// c */\n/// d", "d"),
        ("rust", "/// a // b /* c\n/// d", "a // b /* c\nd"),
        ("rust", "#[cfg(feature = \"/*\")]\n/// After.", "After."),
        ("rust", "#[attr('\"', '\\\"')]\n/// After.", "After."),
        ("rust", "#[attr(&'a str)]\n/// After.", "After."),
        (
            "rust",
            "#[attr(r#type, r#\"a\" /*\"#, br#\"b\" /*\"#, cr#\"c\" /*\"#)]\n/// After.",
            "After.",
        ),
        ("rust", "#[doc = r#\"Raw \"open", "Raw \"open"),
        ("rust", "#[doc = \"a\n  b\"]\n/// c", "a\nb\nc"),
        ("rust", "/** Unclosed\n", "Unclosed"),
        ("rust", "/** Open\n *", "Open\n"),
        (
            "csharp",
            "/// <summary>\r\n/// Reads the <c>file</c>.\r\n/// </summary>\r\n",
            "Reads the file.",
        ),
        (
            "csharp",
            "    /// Uses <see cref=\"Shape\"/> here.\n\t/// <param name=\"s\">The shape.</param>",
            "Uses  here.\nThe shape.",
        ),
        (
            "csharp",
            "/// a < b, 1 <2> 3, a<b\n/// <!-- kept -->",
            "a < b, 1 <2> 3, a<b\n<!-- kept -->",
        ),
        (
            "csharp",
            "//// four\n// plain\n/* block */\n///tight\n///\n/// After.",
            "tight\nAfter.",
        ),
        ("csharp", "", ""),
    ];

    /// `extractor`'s doc text of each [`DOC_TEXT_ROWS`] span is the row's, `name` naming the
    /// extractor in a failure.
    fn assert_doc_texts(name: &str, extractor: &dyn Extract) {
        for &(lang, raw, text) in DOC_TEXT_ROWS {
            assert_eq!(
                extractor.doc_text(lang, raw),
                text,
                "{name} {lang}: {raw:?}"
            );
        }
    }

    /// THE DOC TEXT IS STRIPPED: `doc_text` turns a Rust span's raw lines, `\r\n` endings and a
    /// `#[doc(hidden)]` line included, into the comment text alone - the text of each doc comment
    /// and each doc attribute's literal, every comment and literal bounded as Rust lexes it,
    /// everything else dropped, a doc block's opener, closer and one leading `*` stripped; turns
    /// a C# span's raw lines into the text of each `///` line with its XML tags removed, a line
    /// left empty and every other line dropped; gives a Python span its docstring's body, its
    /// parts joined, dedented; and gives a language with no rule its lines trimmed. The project's
    /// extractor with no declaration answers as the bundled one.
    #[test]
    fn the_doc_text_is_the_comment_text_alone() {
        let project = project_declaring(None);
        let extractors: [(&str, &dyn Extract); 2] = [
            ("bundled", &FactsExtractor),
            ("project", &ProjectExtractor::of(project.path())),
        ];
        for (name, extractor) in extractors {
            assert_doc_texts(name, extractor);
        }
    }

    /// THE DOC TEXT IS STRIPPED, under a declaration: a declared file's language is its
    /// declaration's name, and the project's extractor under a declaration in force, named `rust`
    /// or `zig`, gives each row's span the rule of the row's language as the bundled extractor
    /// does - a `rust` span Rust's doc rule, a `zig` span its lines trimmed.
    #[test]
    fn a_declared_language_reads_the_doc_rule_of_its_name() {
        // (the declaration's name, the `extractors:` mapping declaring it)
        let rows = [
            ("rust", "{ rust: { extensions: [rs], run: rust-tags } }"),
            ("zig", "{ zig: { extensions: [zig], run: zig-tags } }"),
        ];
        for (name, mapping) in rows {
            let project = project_declaring(Some(&workflow_declaring_extractors(mapping)));
            assert_eq!(
                declared_extractors(project.path())
                    .into_keys()
                    .collect::<Vec<_>>(),
                [name],
                "{mapping} is in force"
            );
            assert_doc_texts(
                &format!("declaring {name}"),
                &ProjectExtractor::of(project.path()),
            );
        }
    }
}
