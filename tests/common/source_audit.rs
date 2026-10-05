//! Shared support for the whole-tree source audits (`tests/no_os_kill_audit.rs`,
//! `tests/reap_before_removal_audit.rs`): the one finding shape both report, the fixture-file
//! writer both prove detection with, and the real-tree assertion both close on. Included by
//! each suite through `#[path]`, never through `tests/common/mod.rs`. Also the line-level
//! source reading those audits and `tests/boundary_audit.rs` share: which lines are test code,
//! and which function encloses a line. And the one Rust lexer, [`tokenize`], that
//! `tests/simplification_audit.rs` scans by and rule 4 of `tests/boundary_audit.rs` finds string
//! literals with. Each suite uses the subset it needs (hence the module-wide `dead_code`
//! allowance, as in `tests/common/mod.rs`).

#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// One audit hit: which file, which 1-based line, which shape, and the offending line's own
/// text (for the failure message only - never re-scanned).
#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line_no: usize,
    pub shape: &'static str,
    pub line_text: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {} - `{}`",
            self.file,
            self.line_no,
            self.shape,
            self.line_text.trim()
        )
    }
}

/// Write `content` to `root/rel`, creating its parent directories - one fixture file.
pub fn write_file(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

/// The acceptance check both audits close on: `scan` over the REAL, currently checked-out tree
/// (resolved from `CARGO_MANIFEST_DIR`, never the process CWD) finds nothing. `audit` names the
/// audit and `what` the kind of site it counts, for the failure message.
pub fn assert_real_tree_clean(scan: fn(&Path) -> Vec<Finding>, audit: &str, what: &str) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let findings = scan(&root);
    assert!(
        findings.is_empty(),
        "{audit} found {} {what} in the real tree:\n{}",
        findings.len(),
        findings
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The number of leading ASCII space characters on `line` - this audit's sole proxy for
/// "indentation level", since every file it scans is rustfmt-clean (a build-gate
/// precondition) and rustfmt never indents with tabs.
pub fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
}

/// True for a line that DECLARES a function: `fn ` appearing as its own word (never part of a
/// longer identifier or embedded in prose - "function" has no `fn` substring at all, since
/// `f` is always followed by `u`, never `n`, so no comment-line guard is needed here). Matches
/// `fn `, `pub fn `, `pub(crate) fn `, `async fn `, `unsafe fn `, and any other modifier
/// combination, since it only requires the character immediately before `fn ` to be a
/// non-identifier character (or the start of the line).
pub fn is_fn_sig_line(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let marker: Vec<char> = "fn ".chars().collect();
    if chars.len() < marker.len() {
        return false;
    }
    for start in 0..=(chars.len() - marker.len()) {
        if chars[start..start + marker.len()] != marker[..] {
            continue;
        }
        let before_ok =
            start == 0 || !(chars[start - 1].is_alphanumeric() || chars[start - 1] == '_');
        if before_ok {
            return true;
        }
    }
    false
}

/// The `[start, end]` line range (0-based, inclusive) of the brace- or semicolon-delimited
/// item beginning at `start`: the first line at/after `start` whose trimmed-end text ends in
/// `;` (a single-statement item - e.g. `lib.rs`'s `mod blast_radius_eval;`) closes the item on
/// that same line; the first line ending in `{` opens a block, closed by the first LATER line
/// at `start`'s OWN indentation whose trimmed text is exactly `}`. A multi-line signature
/// (wrapped params, a `where` clause) is handled the same way either form is: this only cares
/// about which line eventually ends in `{` or `;`, never how many lines came before it.
pub fn block_span(lines: &[&str], start: usize) -> (usize, usize) {
    let indent = leading_spaces(lines[start]);
    let mut k = start;
    while k < lines.len() {
        let t = lines[k].trim_end();
        if t.ends_with(';') {
            return (start, k);
        }
        if t.ends_with('{') {
            let mut m = k + 1;
            while m < lines.len() {
                if lines[m].trim() == "}" && leading_spaces(lines[m]) == indent {
                    return (start, m);
                }
                m += 1;
            }
            return (start, lines.len() - 1);
        }
        k += 1;
    }
    (start, lines.len() - 1)
}

/// The `[start, end]` ranges (0-based, inclusive) of every `#[cfg(test)]`-attributed item in
/// `lines`: a whole `mod { ... }` block (the common shape - `tests`, `pure_metric_tests`,
/// `corpus_gates`, ...), a single standalone item (`main.rs`'s `#[cfg(test)] fn
/// compose_precommit`), or a semicolon-terminated module declaration (`lib.rs`'s `#[cfg(test)]
/// mod blast_radius_eval;`). Attributes may stack (`blast_radius_eval.rs`'s `#[cfg(test)]`
/// directly above a further `#[cfg(feature = "symbols")]` before the actual `mod`), so this
/// skips every contiguous attribute/blank line before locating the attributed item itself.
/// Requires the marker line's TRIMMED text to BE the attribute - exactly `#[cfg(test)]`, or
/// starting `#[cfg(all(test,` (`namespace.rs`'s test module also needs the `store` lane) -
/// never a substring match, so a doc comment merely mentioning the phrase in prose (as
/// `blast_radius_eval.rs`'s own module doc does) is never mistaken for the attribute (a `//` or
/// `///` line never starts with `#` after trimming).
pub fn cfg_test_ranges(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let marker = lines[i].trim();
        if marker != "#[cfg(test)]" && !marker.starts_with("#[cfg(all(test,") {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < lines.len() {
            let t = lines[j].trim();
            if t.is_empty() || t.starts_with('#') {
                j += 1;
            } else {
                break;
            }
        }
        if j >= lines.len() {
            break;
        }
        let (_, end) = block_span(lines, j);
        ranges.push((i, end));
        i = end + 1;
    }
    ranges
}

pub fn in_ranges(line: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(s, e)| line >= s && line <= e)
}

/// Every `fn`-signature line (0-based, ascending order) that is NOT inside `excluded` and is
/// not itself a comment line (a further guard against a doc comment that happens to embed a
/// literal `fn ` code sample, on top of [`is_fn_sig_line`]'s own word-boundary check).
pub fn fn_sig_lines(lines: &[&str], excluded: &[(usize, usize)]) -> Vec<usize> {
    (0..lines.len())
        .filter(|&i| {
            !in_ranges(i, excluded)
                && !lines[i].trim_start().starts_with("//")
                && is_fn_sig_line(lines[i])
        })
        .collect()
}

/// The signature line (0-based) of the function enclosing line `at`, if any: the nearest
/// fn-signature line AT OR BEFORE `at` whose own [`block_span`] actually reaches `at` -
/// searched innermost-candidate-first so a later, more deeply nested `fn` is preferred over an
/// outer one that has already closed by `at`.
pub fn enclosing_fn_line(lines: &[&str], sigs: &[usize], at: usize) -> Option<usize> {
    sigs.iter()
        .rev()
        .copied()
        .find(|&s| s <= at && at <= block_span(lines, s).1)
}

/// Whether `c` is a Rust identifier-continuation character (used for word-boundary checks so
/// e.g. `fnv1a_64` is never mistaken for the `fn` keyword).
pub fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// If `chars[i..]` opens a string literal (`"...\"`, `r"..."`, `r#"..."#`, ..., `b"..."`,
/// `br#"..."#`, ...), advance `*i` past its closing delimiter and return how many `\n`s it
/// contained. Returns `None` (and leaves `*i` untouched) if no string literal starts here.
pub fn skip_string_literal(chars: &[char], i: &mut usize) -> Option<usize> {
    let n = chars.len();
    let mut p = *i;
    if p < n && chars[p] == 'b' {
        p += 1;
    }
    let mut hashes = 0usize;
    let mut raw = false;
    if p < n && chars[p] == 'r' {
        let mut q = p + 1;
        let mut h = 0usize;
        while q < n && chars[q] == '#' {
            h += 1;
            q += 1;
        }
        if q < n && chars[q] == '"' {
            raw = true;
            hashes = h;
            p = q + 1;
        }
    }
    if !raw {
        if p < n && chars[p] == '"' {
            p += 1;
        } else {
            return None;
        }
        // Plain (possibly byte-) string: scan for unescaped closing quote.
        let mut lines = 0usize;
        while p < n {
            match chars[p] {
                '\\' if p + 1 < n => {
                    if chars[p + 1] == '\n' {
                        lines += 1;
                    }
                    p += 2;
                }
                '\n' => {
                    lines += 1;
                    p += 1;
                }
                '"' => {
                    p += 1;
                    *i = p;
                    return Some(lines);
                }
                _ => p += 1,
            }
        }
        *i = p;
        return Some(lines);
    }
    // Raw (possibly byte-) string: scan for `"` followed by exactly `hashes` `#`s.
    let mut lines = 0usize;
    while p < n {
        if chars[p] == '"' {
            let mut q = p + 1;
            let mut h = 0usize;
            while q < n && h < hashes && chars[q] == '#' {
                h += 1;
                q += 1;
            }
            if h == hashes {
                *i = q;
                return Some(lines);
            }
        }
        if chars[p] == '\n' {
            lines += 1;
        }
        p += 1;
    }
    *i = p;
    Some(lines)
}

/// If a char literal starts at `chars[i]` (`i` points at the opening `'`), return its length in
/// chars (including both quotes); else `None` (this `'` is a lifetime marker instead). A char
/// literal is a `'`, one source char OR a bounded backslash escape (`\n`, `\t`, `\r`, `\\`,
/// `\'`, `\0`, `\xNN`, `\u{...}`), then a closing `'` - a lifetime is never followed by a bare
/// closing `'`, so this is unambiguous.
pub fn char_literal_len(chars: &[char], i: usize) -> Option<usize> {
    let n = chars.len();
    if i >= n || chars[i] != '\'' {
        return None;
    }
    let mut p = i + 1;
    if p >= n {
        return None;
    }
    if chars[p] == '\\' {
        p += 1;
        if p >= n {
            return None;
        }
        match chars[p] {
            'x' => {
                p += 1;
                let mut hex = 0;
                while p < n && hex < 2 && chars[p].is_ascii_hexdigit() {
                    p += 1;
                    hex += 1;
                }
            }
            'u' => {
                p += 1;
                if p < n && chars[p] == '{' {
                    p += 1;
                    while p < n && chars[p] != '}' {
                        p += 1;
                    }
                    if p < n {
                        p += 1;
                    }
                }
            }
            _ => p += 1, // \n \t \r \\ \' \" \0 etc: one escaped char
        }
    } else {
        p += 1;
    }
    if p < n && chars[p] == '\'' {
        Some(p + 1 - i)
    } else {
        None
    }
}

/// Advance `*i` past a nested block comment (`chars[*i]=='/'`, `chars[*i+1]=='*'` - the caller
/// checks this before calling), returning the number of `\n`s crossed.
pub fn skip_block_comment(chars: &[char], i: &mut usize) -> usize {
    let n = chars.len();
    let mut depth = 1usize;
    let mut lines = 0usize;
    *i += 2;
    while *i < n && depth > 0 {
        if chars[*i] == '\n' {
            lines += 1;
            *i += 1;
            continue;
        }
        if chars[*i] == '/' && *i + 1 < n && chars[*i + 1] == '*' {
            depth += 1;
            *i += 2;
            continue;
        }
        if chars[*i] == '*' && *i + 1 < n && chars[*i + 1] == '/' {
            depth -= 1;
            *i += 2;
            continue;
        }
        *i += 1;
    }
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    Keyword,
    Ident,
    Lifetime,
    Lit,
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTok {
    pub kind: RawKind,
    pub text: String,
    /// 1-based line the token STARTS on, within whatever `chars` slice was tokenized.
    pub line: usize,
}

/// What no line of any file under `src/` or `crates/` may hold, case-sensitively (rule 4 of
/// `tests/boundary_audit.rs`): the one gate id the core once knew, spelled as a string literal or
/// a concept id, its script, its scratch root and its tool. A token split across literals or
/// built at run time to pass is not a removal.
pub const GATE_TOKENS: &[&str] = &[
    "cargo-mutants",
    "cargo mutants",
    "mutation.sh",
    "rigger-mutants",
    "MUTATION_GATE_ID",
    "\"mutation\"",
    "gate:mutation",
];

/// The tool's environment word, banned only as a whole word: `$MUTANTS` is a hit,
/// `UNIT_MUTANTS_PREFIX` is not.
pub const GATE_WORD: &str = "MUTANTS";

/// Whether `line` holds `word` with no identifier character (`[A-Za-z0-9_]`) on either side.
pub fn holds_whole_word(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + word.len()..].chars().next();
        !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char)
    })
}

/// The gate id as a YAML key, banned on a line that begins inside a string literal.
pub const GATE_YAML_KEY: &str = "mutation:";

/// The rule-4 gate or tool token or form `line` holds, the first one when it holds several:
/// a [`GATE_TOKENS`] entry, [`GATE_WORD`] as a whole word, or - only when the line begins inside
/// a string literal (`begins_in_literal`), where a YAML document's text lives - a line leading
/// with [`GATE_YAML_KEY`].
pub fn gate_name_in(line: &str, begins_in_literal: bool) -> Option<&'static str> {
    GATE_TOKENS
        .iter()
        .copied()
        .find(|token| line.contains(token))
        .or_else(|| holds_whole_word(line, GATE_WORD).then_some(GATE_WORD))
        .or_else(|| {
            (begins_in_literal && line.trim_start().starts_with(GATE_YAML_KEY))
                .then_some(GATE_YAML_KEY)
        })
}

/// Rust keywords (2018+ reserved and strict, plus weak keywords actually used as such) - kept
/// verbatim by the simplification audit's normalizer rather than canonicalized like an
/// identifier, since a keyword is structural signal, not a name.
pub const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "try", "union", "yield", "abstract", "become",
    "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual",
];

pub fn is_keyword(s: &str) -> bool {
    RUST_KEYWORDS.contains(&s)
}

/// Tokenize `chars` into a normalized-similarity-ready token stream: comments and whitespace
/// produce no token; a string/byte/raw-string literal or a char literal becomes one `Lit`
/// token (through [`skip_string_literal`]/[`char_literal_len`], so no lexical state is
/// re-implemented); a `'`+ident not matched as a char literal is one `Lifetime` token; a digit-led
/// run (plus one optional `.`-fraction) is one `Lit` (number)
/// token; a letter/`_`-led run is `Keyword` when it names a Rust keyword, else `Ident`; every
/// other character is its own single-char `Punct` token (so a multi-char operator like `::` or
/// `->` becomes two/three adjacent `Punct` tokens - the simplification audit's mandatory-sweep
/// matchers account for this).
pub fn tokenize(chars: &[char]) -> Vec<RawTok> {
    let n = chars.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut out = Vec::new();
    while i < n {
        let c = chars[i];
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            line += skip_block_comment(chars, &mut i);
            continue;
        }
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let start_line = line;
        if let Some(consumed_lines) = skip_string_literal(chars, &mut i) {
            let text: String = chars[start..i].iter().collect();
            out.push(RawTok {
                kind: RawKind::Lit,
                text,
                line: start_line,
            });
            line += consumed_lines;
            continue;
        }
        if c == '\'' {
            if let Some(len) = char_literal_len(chars, i) {
                i += len;
                let text: String = chars[start..i].iter().collect();
                out.push(RawTok {
                    kind: RawKind::Lit,
                    text,
                    line: start_line,
                });
                continue;
            }
            i += 1;
            while i < n && is_ident_char(chars[i]) {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(RawTok {
                kind: RawKind::Lifetime,
                text,
                line: start_line,
            });
            continue;
        }
        if c.is_ascii_digit() {
            i += 1;
            while i < n && is_ident_char(chars[i]) {
                i += 1;
            }
            if i < n && chars[i] == '.' && i + 1 < n && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < n && is_ident_char(chars[i]) {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            out.push(RawTok {
                kind: RawKind::Lit,
                text,
                line: start_line,
            });
            continue;
        }
        if is_ident_char(c) {
            i += 1;
            while i < n && is_ident_char(chars[i]) {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let kind = if is_keyword(&text) {
                RawKind::Keyword
            } else {
                RawKind::Ident
            };
            out.push(RawTok {
                kind,
                text,
                line: start_line,
            });
            continue;
        }
        out.push(RawTok {
            kind: RawKind::Punct,
            text: c.to_string(),
            line: start_line,
        });
        i += 1;
    }
    out
}
