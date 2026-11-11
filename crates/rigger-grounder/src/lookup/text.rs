//! The live text half: every term searched for literally, with ripgrep's smart case, in every
//! file of the caller's working tree that the project walk visits ([`walk_guarded`]: the tree's
//! own `.gitignore` rules, hidden entries skipped, no symlink followed). Nothing is indexed, so the
//! answer is always the tree's bytes as they stand.

use std::ops::ControlFlow;
use std::path::Path;

use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};

use super::budget::{TEXT_PER_FILE, TEXT_TOTAL};
use crate::grounder::walk_guarded;

/// One matching line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextHit {
    /// The file, relative to the tree's root, `/`-separated.
    pub file: String,
    /// The 1-based line number.
    pub line: u64,
    /// The line, its line ending removed.
    pub text: String,
}

/// One term's matches: the lines kept, at most [`TEXT_PER_FILE`] per file and [`TEXT_TOTAL`] in
/// all, and how many lines matched in all.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TermText {
    pub hits: Vec<TextHit>,
    pub matched: usize,
}

/// Scan the tree at `root` once for every term of `terms`, answering each term's matches in the
/// order of `terms`. Fails only when a term cannot be turned into a literal matcher.
pub(crate) fn scan(root: &Path, terms: &[&str]) -> Result<Vec<TermText>, String> {
    let matchers = terms
        .iter()
        .map(|term| literal(term))
        .collect::<Result<Vec<_>, _>>()?;
    let mut found: Vec<TermText> = terms.iter().map(|_| TermText::default()).collect();
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .build();
    let _ = walk_guarded(root, &mut |path| {
        let Ok(bytes) = std::fs::read(path) else {
            return ControlFlow::Continue(());
        };
        let file = relative(root, path);
        for (matcher, term) in matchers.iter().zip(found.iter_mut()) {
            search_file(&mut searcher, matcher, &bytes, &file, term);
        }
        ControlFlow::Continue(())
    });
    Ok(found)
}

/// The literal matcher for `term`, case-insensitive unless the term holds an uppercase letter
/// (ripgrep's smart case).
fn literal(term: &str) -> Result<RegexMatcher, String> {
    RegexMatcherBuilder::new()
        .case_smart(true)
        .fixed_strings(true)
        .build(term)
        .map_err(|e| format!("the term {term:?} is no literal pattern: {e}"))
}

/// Record `bytes`'s matches of `matcher` into `term`: every match counted, at most
/// [`TEXT_PER_FILE`] of this file's kept while the term keeps fewer than [`TEXT_TOTAL`].
fn search_file(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    bytes: &[u8],
    file: &str,
    term: &mut TermText,
) {
    let mut kept_here = 0;
    let _ = searcher.search_slice(
        matcher,
        bytes,
        Lossy(|line, text| {
            term.matched += 1;
            if kept_here < TEXT_PER_FILE && term.hits.len() < TEXT_TOTAL {
                kept_here += 1;
                term.hits.push(TextHit {
                    file: file.to_string(),
                    line,
                    text: text.trim_end_matches(['\n', '\r']).to_string(),
                });
            }
            Ok(true)
        }),
    );
}

/// `path` relative to `root`, `/`-separated on every platform.
pub(crate) fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
