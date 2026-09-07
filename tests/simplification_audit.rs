//! Spec 85 criterion 1, THE RESPONSIBILITY MAP: a deterministic, zero-new-dependency scan of
//! every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs`, each assigned a
//! proposed module (ports-and-adapters shape) with its current line span and a reason, written
//! to `docs/audit/responsibility-map.json` and rendered as section 1 of
//! `docs/audit/2026-09-simplification-audit.md`. This unit OWNS the scanner and the
//! responsibility map. It does NOT own: the duplication catalog (criterion 2, `u85c2`), the
//! report's sections 3-5 (criterion 3, `u85c3`), or section 6's prioritized plan (criterion 4,
//! `u85c4`) - this file leaves clearly marked placeholders for each so those units can locate
//! and replace their own section without disturbing this one's.
//!
//! Run plainly (`cargo test --test simplification_audit`), this regenerates the map and section
//! 1 IN MEMORY and asserts they match the committed files byte-for-byte (the drift guard): the
//! catalog can never silently drift from the tree. Run with `RIGGER_AUDIT_WRITE=1` set, it
//! instead REWRITES `docs/audit/responsibility-map.json` and section 1 of the report.
//!
//! THE SCANNER (decision `u85c1-scanner-scope`): one left-to-right character scan per file
//! maintaining a frame stack (`TopLevel` / `Mod` / `Impl` / `Trait` / `Fn` / `Anonymous`).
//! Item keywords (`mod`, `impl`, `trait`, `fn`) are recognized at the top of any NON-Anonymous
//! frame - including inside a `Fn` frame, so a local nested `fn` is still found - but the
//! scanner does NOT recurse into an anonymous block (`if`/`match`/`loop`/closure body, or a
//! bare `{ ... }` expression block) looking for further item keywords inside it; such a block
//! is skipped opaquely via plain brace-depth matching. DISCLOSED LIMIT (mirrors
//! `tests/reap_before_removal_audit.rs`'s own precedent): an `fn`/`mod`/`impl`/`trait` item
//! declared directly inside a non-item block would be missed. Not live against anything today
//! (Rust idiom keeps local items at a block's own top level, and this tree's own style leans on
//! free functions and `#[cfg(test)] mod tests` bodies, both fully covered).
//!
//! A `fn` item counts ONLY if its signature scan reaches an opening `{` (its body) before a
//! bare `;` - see [`scan_fn_signature_end`]. This means a bodyless trait/extern signature
//! (`fn spawn(&self, ...) -> Result<AgentResult, Error>;` in `conductor::AgentDriver`) and a
//! `fn(...)` function-pointer TYPE usage are excluded automatically: neither ever reaches a
//! `{` before its terminating `;` or expression boundary. The signature scan tracks `[`/`]`
//! depth only, so a `;` inside an array-type parameter (`buf: [u8; 32]`) is not mistaken for
//! the bodyless-signature terminator - a shape confirmed present in this tree's own real
//! parameter types.
//!
//! Lexical state tracked while scanning ANY file content (signatures and bodies alike): line
//! comments, NESTED block comments (Rust nests `/* /* */ */`, confirmed present in
//! `src/conductor.rs`), string and raw-string literals (`r"..."`, `r#"..."#`, ... with
//! hash-count matching, confirmed present in all three files) and byte-string variants
//! (`b"..."`, `br#"..."#`), and char literals disambiguated from lifetimes by bounded
//! lookahead: a `'` immediately followed by one char (or a short backslash escape) and a
//! closing `'` is a char literal (confirmed present: `'{'`/`'}'` appear literally in this
//! tree); a `'` not closed that way is a lifetime and consumed as one token. Braces, semicolons
//! and quotes inside any of these lexical states never affect scanning.
//!
//! Zero new dependencies: no `syn`, no external parser - `serde`/`serde_json` (already a
//! workspace dependency) is used only to serialize the committed JSON.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The three files this criterion scans - spec 85's Design and Done-when name them by literal
/// path, in this fixed order (also the order every generated artifact lists them in).
const TARGET_FILES: [&str; 3] = ["src/conductor.rs", "src/main.rs", "src/dash.rs"];

// =========================================================================================
// THE SCANNER
// =========================================================================================

/// One function found by [`scan_file`]: its identity, current location, and the syntactic
/// context (`#[cfg(test)]`-ness, enclosing `impl`/`mod`) the classifier reasons from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScannedFn {
    file: String,
    name: String,
    /// 1-based line of the `fn` keyword itself (not any preceding doc-comment/attribute).
    start_line: usize,
    /// 1-based line of the function body's matching closing brace.
    end_line: usize,
    /// Set when this fn (directly, or via an enclosing `#[cfg(test)]` mod) is test-only.
    is_test: bool,
    /// The nearest enclosing `impl` block's raw header text (e.g. `"GateRatchet"` or
    /// `"AgentDriver for Stub"`), if any - `None` for a free function or a `trait` default
    /// method.
    enclosing_impl: Option<String>,
    /// The dotted path of enclosing `mod` names, outermost first (empty for a function that
    /// sits directly at file scope or only inside an `impl`/`trait`).
    enclosing_mods: Vec<String>,
}

/// A stack frame the scanner pushes on every recognized `{` and pops on its matching `}`.
#[derive(Debug, Clone)]
enum FrameKind {
    TopLevel,
    // The mod's own name and the impl's own header text are tracked separately on
    // `mods_stack`/`impl_stack` (popped in lockstep with these frames) - not duplicated here.
    Mod {
        is_test: bool,
    },
    Impl {
        is_test: bool,
    },
    Trait {
        is_test: bool,
    },
    Fn {
        is_test: bool,
    },
    /// Any other `{` - an if/match/loop/closure body, a bare block expression, a struct
    /// literal, etc. Opaque: the scanner does not look for item keywords inside it.
    Anonymous,
}

struct Frame {
    kind: FrameKind,
}

impl Frame {
    /// Whether item keywords (`fn`/`mod`/`impl`/`trait`) are recognized while this frame is on
    /// top of the stack - every kind except `Anonymous` (decision `u85c1-scanner-scope`).
    fn allows_items(&self) -> bool {
        !matches!(self.kind, FrameKind::Anonymous)
    }

    fn is_test(&self) -> bool {
        match &self.kind {
            FrameKind::Mod { is_test }
            | FrameKind::Trait { is_test }
            | FrameKind::Fn { is_test }
            | FrameKind::Impl { is_test } => *is_test,
            FrameKind::TopLevel | FrameKind::Anonymous => false,
        }
    }
}

/// Whether `c` is a Rust identifier-continuation character (used for word-boundary checks so
/// e.g. `fnv1a_64` is never mistaken for the `fn` keyword).
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Scan `content` (the text of `file`, a repo-relative forward-slash path used only to label
/// findings) for every function with a body, per this module's doc comment.
fn scan_file(file: &str, content: &str) -> Vec<ScannedFn> {
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len();
    let mut line = 1usize;
    let mut i = 0usize;

    let mut stack: Vec<Frame> = vec![Frame {
        kind: FrameKind::TopLevel,
    }];
    let mut mods_stack: Vec<String> = Vec::new();
    let mut impl_stack: Vec<String> = Vec::new();
    let mut out: Vec<ScannedFn> = Vec::new();
    let mut pending_cfg_test = false;
    // (start_line, is_test, name) for the fn currently open, one per Fn frame depth.
    let mut open_fns: Vec<(usize, bool, String)> = Vec::new();

    while i < n {
        let c = chars[i];

        // --- line comments ---
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // --- nested block comments ---
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let mut depth = 1usize;
            i += 2;
            while i < n && depth > 0 {
                if chars[i] == '\n' {
                    line += 1;
                    i += 1;
                    continue;
                }
                if chars[i] == '/' && i + 1 < n && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                    continue;
                }
                if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                    continue;
                }
                i += 1;
            }
            continue;
        }
        // --- attribute: track #[cfg(test)] / #[test] as "pending" for the next item ---
        if c == '#' && i + 1 < n && chars[i + 1] == '[' {
            let start = i;
            let mut depth = 0i32;
            while i < n {
                if chars[i] == '[' {
                    depth += 1;
                } else if chars[i] == ']' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                } else if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            let attr_text: String = chars[start..i].iter().collect();
            if attr_text.contains("cfg(test)") || attr_text.contains("#[test]") {
                pending_cfg_test = true;
            }
            continue;
        }
        // --- string / raw string / byte string literals ---
        if let Some(consumed_lines) = skip_string_literal(&chars, &mut i) {
            line += consumed_lines;
            continue;
        }
        // --- char literal vs lifetime ---
        if c == '\'' {
            if let Some(len) = char_literal_len(&chars, i) {
                i += len;
                continue;
            }
            // A lifetime: consume the tick and its identifier, nothing more.
            i += 1;
            while i < n && is_ident_char(chars[i]) {
                i += 1;
            }
            continue;
        }
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }

        let top_allows_items = stack.last().map(|f| f.allows_items()).unwrap_or(false);

        if top_allows_items && c == 'f' && starts_word(&chars, i, "fn") {
            let kw_line = line;
            let name_start = i + 2;
            let (name, mut j) = read_ident_after_ws(&chars, name_start);
            if !name.is_empty() {
                match scan_fn_signature_end(&chars, &mut j, &mut line) {
                    SignatureEnd::Body => {
                        let is_test =
                            pending_cfg_test || stack.last().map(|f| f.is_test()).unwrap_or(false);
                        pending_cfg_test = false;
                        stack.push(Frame {
                            kind: FrameKind::Fn { is_test },
                        });
                        open_fns.push((kw_line, is_test, name));
                        i = j; // j sits just past the opening '{'
                        continue;
                    }
                    SignatureEnd::NoBody => {
                        pending_cfg_test = false;
                        i = j;
                        continue;
                    }
                }
            }
            // Not actually `fn <ident>` (e.g. a stray "fn" with no following identifier before
            // punctuation) - fall through and treat the byte normally below.
        } else if top_allows_items && c == 'm' && starts_word(&chars, i, "mod") {
            let (name, mut j) = read_ident_after_ws(&chars, i + 3);
            if !name.is_empty() {
                // skip whitespace/comments minimally: only whitespace expected here
                while j < n && (chars[j] == ' ' || chars[j] == '\t' || chars[j] == '\n') {
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    j += 1;
                }
                if j < n && chars[j] == '{' {
                    let is_test =
                        pending_cfg_test || stack.last().map(|f| f.is_test()).unwrap_or(false);
                    pending_cfg_test = false;
                    mods_stack.push(name);
                    stack.push(Frame {
                        kind: FrameKind::Mod { is_test },
                    });
                    i = j + 1;
                    continue;
                } else if j < n && chars[j] == ';' {
                    // `mod foo;` - external file, no body here.
                    pending_cfg_test = false;
                    i = j + 1;
                    continue;
                }
            }
        } else if top_allows_items && c == 't' && starts_word(&chars, i, "trait") {
            let (_name, mut j) = read_ident_after_ws(&chars, i + 5);
            // Skip forward (tracking [ ] depth like a fn signature) to the trait's own `{`.
            let mut bdepth = 0i32;
            let mut found = false;
            while j < n {
                match chars[j] {
                    '[' => bdepth += 1,
                    ']' => bdepth -= 1,
                    '\n' => line += 1,
                    '{' if bdepth <= 0 => {
                        found = true;
                        break;
                    }
                    ';' if bdepth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            if found {
                let is_test =
                    pending_cfg_test || stack.last().map(|f| f.is_test()).unwrap_or(false);
                pending_cfg_test = false;
                stack.push(Frame {
                    kind: FrameKind::Trait { is_test },
                });
                i = j + 1;
                continue;
            }
            pending_cfg_test = false;
            i = j.max(i + 1);
            continue;
        } else if top_allows_items && c == 'i' && starts_word(&chars, i, "impl") {
            let mut j = i + 4;
            let mut bdepth = 0i32;
            let mut found = false;
            let header_start = j;
            while j < n {
                match chars[j] {
                    '[' => bdepth += 1,
                    ']' => bdepth -= 1,
                    '\n' => line += 1,
                    '{' if bdepth <= 0 => {
                        found = true;
                        break;
                    }
                    ';' if bdepth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            if found {
                let header: String = chars[header_start..j].iter().collect();
                let header = header.trim().to_string();
                let is_test =
                    pending_cfg_test || stack.last().map(|f| f.is_test()).unwrap_or(false);
                pending_cfg_test = false;
                impl_stack.push(header);
                stack.push(Frame {
                    kind: FrameKind::Impl { is_test },
                });
                i = j + 1;
                continue;
            }
            pending_cfg_test = false;
            i = j.max(i + 1);
            continue;
        }

        if c == '{' {
            stack.push(Frame {
                kind: FrameKind::Anonymous,
            });
            i += 1;
            continue;
        }
        if c == '}' {
            if let Some(frame) = stack.pop() {
                match frame.kind {
                    FrameKind::Fn { .. } => {
                        if let Some((sl, is_test, name)) = open_fns.pop() {
                            out.push(ScannedFn {
                                file: file.to_string(),
                                name,
                                start_line: sl,
                                end_line: line,
                                is_test,
                                enclosing_impl: impl_stack.last().cloned(),
                                enclosing_mods: mods_stack.clone(),
                            });
                        }
                    }
                    FrameKind::Mod { .. } => {
                        mods_stack.pop();
                    }
                    FrameKind::Impl { .. } => {
                        impl_stack.pop();
                    }
                    FrameKind::Trait { .. } | FrameKind::Anonymous | FrameKind::TopLevel => {}
                }
            }
            i += 1;
            continue;
        }

        // Any other char that isn't whitespace clears a stale pending attribute (attributes
        // always sit immediately - across whitespace/comments/other attributes only - before
        // the item they annotate).
        if !c.is_whitespace() {
            // A bare identifier char not part of a recognized keyword: leave pending_cfg_test
            // as-is only if we are still inside what could be another attribute/whitespace run;
            // since attributes and comments were already special-cased above, reaching here
            // with pending_cfg_test set and no item keyword matched means the attribute was on
            // something this scanner does not model (e.g. a struct/field) - clear it so it
            // cannot leak onto a later, unrelated fn.
            pending_cfg_test = false;
        }
        i += 1;
    }

    out
}

/// Whether `chars[pos..]` starts with keyword `kw` at a word boundary (not preceded by an
/// identifier char, and not followed by one - so `fnv1a` never matches `fn`).
fn starts_word(chars: &[char], pos: usize, kw: &str) -> bool {
    let kw_chars: Vec<char> = kw.chars().collect();
    let end = pos + kw_chars.len();
    if end > chars.len() {
        return false;
    }
    if chars[pos..end] != kw_chars[..] {
        return false;
    }
    if pos > 0 && is_ident_char(chars[pos - 1]) {
        return false;
    }
    if end < chars.len() && is_ident_char(chars[end]) {
        return false;
    }
    true
}

/// Skip whitespace/comments-free gap after a keyword, then read one identifier. Returns the
/// identifier (empty if none found before other punctuation) and the index just past it.
fn read_ident_after_ws(chars: &[char], mut i: usize) -> (String, usize) {
    let n = chars.len();
    while i < n && (chars[i] == ' ' || chars[i] == '\t' || chars[i] == '\n' || chars[i] == '\r') {
        i += 1;
    }
    let start = i;
    while i < n && is_ident_char(chars[i]) {
        i += 1;
    }
    (chars[start..i].iter().collect(), i)
}

enum SignatureEnd {
    Body,
    NoBody,
}

/// Scan a `fn` signature (generics, params, return type, `where` clause) starting at `j`
/// (already past the name) forward to its terminator: an opening `{` (a real function body -
/// `j` is left just past that `{`) or a bare `;`/other terminator at square-bracket depth 0 (a
/// bodyless signature or a `fn(...)` type usage - `j` is left just past it). `[`/`]` depth is
/// tracked so a `;` inside an array-type parameter is never mistaken for the terminator (see
/// this module's doc comment). Newlines crossed are added to `*line`.
fn scan_fn_signature_end(chars: &[char], j: &mut usize, line: &mut usize) -> SignatureEnd {
    let n = chars.len();
    let mut bdepth = 0i32;
    while *j < n {
        let c = chars[*j];
        // Lexical states still apply inside a signature (a default-const-generic string, a
        // doc comment between params, etc.).
        if c == '/' && *j + 1 < n && chars[*j + 1] == '/' {
            while *j < n && chars[*j] != '\n' {
                *j += 1;
            }
            continue;
        }
        if c == '/' && *j + 1 < n && chars[*j + 1] == '*' {
            let mut depth = 1usize;
            *j += 2;
            while *j < n && depth > 0 {
                if chars[*j] == '\n' {
                    *line += 1;
                    *j += 1;
                    continue;
                }
                if chars[*j] == '/' && *j + 1 < n && chars[*j + 1] == '*' {
                    depth += 1;
                    *j += 2;
                    continue;
                }
                if chars[*j] == '*' && *j + 1 < n && chars[*j + 1] == '/' {
                    depth -= 1;
                    *j += 2;
                    continue;
                }
                *j += 1;
            }
            continue;
        }
        if let Some(consumed_lines) = skip_string_literal(chars, j) {
            *line += consumed_lines;
            continue;
        }
        if c == '\'' {
            if let Some(len) = char_literal_len(chars, *j) {
                *j += len;
                continue;
            }
            *j += 1;
            while *j < n && is_ident_char(chars[*j]) {
                *j += 1;
            }
            continue;
        }
        if c == '\n' {
            *line += 1;
            *j += 1;
            continue;
        }
        match c {
            '[' => bdepth += 1,
            ']' => bdepth -= 1,
            '{' if bdepth <= 0 => {
                *j += 1;
                return SignatureEnd::Body;
            }
            ';' if bdepth <= 0 => {
                *j += 1;
                return SignatureEnd::NoBody;
            }
            _ => {}
        }
        *j += 1;
    }
    SignatureEnd::NoBody
}

/// If `chars[i..]` opens a string literal (`"...\"`, `r"..."`, `r#"..."#`, ..., `b"..."`,
/// `br#"..."#`, ...), advance `*i` past its closing delimiter and return how many `\n`s it
/// contained. Returns `None` (and leaves `*i` untouched) if no string literal starts here.
fn skip_string_literal(chars: &[char], i: &mut usize) -> Option<usize> {
    let n = chars.len();
    let start = *i;
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
    let _ = start;
    Some(lines)
}

/// If a char literal starts at `chars[i]` (`i` points at the opening `'`), return its length in
/// chars (including both quotes); else `None` (this `'` is a lifetime marker instead). A char
/// literal is a `'`, one source char OR a bounded backslash escape (`\n`, `\t`, `\r`, `\\`,
/// `\'`, `\0`, `\xNN`, `\u{...}`), then a closing `'` - a lifetime is never followed by a bare
/// closing `'`, so this is unambiguous.
fn char_literal_len(chars: &[char], i: usize) -> Option<usize> {
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

/// Collect and scan the three target files under `root` (a repo checkout), in
/// [`TARGET_FILES`]'s fixed order, deterministically.
fn scan_target_files(root: &Path) -> Vec<ScannedFn> {
    let mut out = Vec::new();
    for rel in TARGET_FILES {
        let path = root.join(rel);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("simplification_audit: cannot read {rel}: {e}"));
        out.extend(scan_file(rel, &content));
    }
    out
}

// =========================================================================================
// THE CLASSIFIER
// =========================================================================================

/// One entry in a file's rule table: if `needle` occurs anywhere in the function's name, the
/// first matching rule (in table order) wins.
struct Rule {
    needle: &'static str,
    module: &'static str,
    concern: &'static str,
}

/// `src/conductor.rs`'s free-function rule table, derived from the concern keywords that
/// actually repeat across its ~600 functions (a frequency pass over the checked-out tree,
/// decision `u85c1-classification-scheme`) - ordered most-specific-concern first so a name
/// matching two rules takes the earlier, narrower one.
const CONDUCTOR_RULES: &[Rule] = &[
    Rule {
        needle: "speculat",
        module: "conductor::spawn",
        concern: "speculative-lane spawning",
    },
    Rule {
        needle: "spawn",
        module: "conductor::spawn",
        concern: "spawn lifecycle",
    },
    Rule {
        needle: "resume",
        module: "conductor::spawn",
        concern: "spawn lifecycle",
    },
    Rule {
        needle: "parked",
        module: "conductor::spawn",
        concern: "spawn lifecycle",
    },
    Rule {
        needle: "wave",
        module: "conductor::spawn",
        concern: "spawn lifecycle",
    },
    Rule {
        needle: "agent",
        module: "conductor::spawn",
        concern: "spawn lifecycle",
    },
    Rule {
        needle: "run_unit",
        module: "conductor::run",
        concern: "unit run loop",
    },
    Rule {
        needle: "gate",
        module: "conductor::gate",
        concern: "gate execution",
    },
    Rule {
        needle: "verdict",
        module: "conductor::gate",
        concern: "gate execution",
    },
    Rule {
        needle: "review",
        module: "conductor::review",
        concern: "review orchestration",
    },
    Rule {
        needle: "critique",
        module: "conductor::review",
        concern: "review orchestration",
    },
    Rule {
        needle: "adjudicat",
        module: "conductor::review",
        concern: "review orchestration",
    },
    Rule {
        needle: "reviewer",
        module: "conductor::review",
        concern: "review orchestration",
    },
    Rule {
        needle: "approv",
        module: "conductor::review",
        concern: "review orchestration",
    },
    Rule {
        needle: "reject",
        module: "conductor::review",
        concern: "review orchestration",
    },
    Rule {
        needle: "budget",
        module: "conductor::budget",
        concern: "budget accounting",
    },
    Rule {
        needle: "mutation",
        module: "conductor::budget",
        concern: "budget accounting",
    },
    Rule {
        needle: "compensat",
        module: "conductor::budget",
        concern: "budget accounting",
    },
    Rule {
        needle: "partition",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "blast",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "dag",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "plan",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "route",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "criterion",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "stage",
        module: "conductor::schedule",
        concern: "unit scheduling",
    },
    Rule {
        needle: "emit",
        module: "conductor::emit",
        concern: "event/decision emission",
    },
    Rule {
        needle: "record",
        module: "conductor::emit",
        concern: "event/decision emission",
    },
    Rule {
        needle: "append",
        module: "conductor::emit",
        concern: "event/decision emission",
    },
    Rule {
        needle: "ground",
        module: "conductor::ground",
        concern: "grounding integration",
    },
    Rule {
        needle: "ingest",
        module: "conductor::ground",
        concern: "grounding integration",
    },
    Rule {
        needle: "graph",
        module: "conductor::ground",
        concern: "grounding integration",
    },
    Rule {
        needle: "gc",
        module: "conductor::gc",
        concern: "reclaim / garbage collection",
    },
    Rule {
        needle: "reclaim",
        module: "conductor::gc",
        concern: "reclaim / garbage collection",
    },
    Rule {
        needle: "harvest",
        module: "conductor::gc",
        concern: "reclaim / garbage collection",
    },
    Rule {
        needle: "drain",
        module: "conductor::gc",
        concern: "reclaim / garbage collection",
    },
    Rule {
        needle: "unit",
        module: "conductor::run",
        concern: "unit run loop",
    },
    Rule {
        needle: "build",
        module: "conductor::support",
        concern: "generic construction helper",
    },
    Rule {
        needle: "write",
        module: "conductor::support",
        concern: "generic write helper",
    },
    Rule {
        needle: "is_",
        module: "conductor::support",
        concern: "predicate helper",
    },
    Rule {
        needle: "has_",
        module: "conductor::support",
        concern: "predicate helper",
    },
    Rule {
        needle: "from_",
        module: "conductor::support",
        concern: "conversion helper",
    },
    Rule {
        needle: "normalize",
        module: "conductor::support",
        concern: "normalization helper",
    },
];

/// `src/main.rs`'s free-function rule table - main.rs is the composition root / CLI, so its
/// dominant repeating prefix (`cmd_`) is the CLI verb dispatch table itself.
const MAIN_RULES: &[Rule] = &[
    Rule {
        needle: "cmd_",
        module: "main::commands",
        concern: "CLI command handler",
    },
    Rule {
        needle: "dash_",
        module: "main::dash_glue",
        concern: "dash registry/marker glue",
    },
    Rule {
        needle: "store_",
        module: "main::store",
        concern: "store hygiene",
    },
    Rule {
        needle: "reset_",
        module: "main::store",
        concern: "store hygiene",
    },
    Rule {
        needle: "reclaim_",
        module: "main::store",
        concern: "store hygiene",
    },
    Rule {
        needle: "migrate",
        module: "main::store",
        concern: "store hygiene",
    },
    Rule {
        needle: "bloat",
        module: "main::store",
        concern: "store hygiene",
    },
    Rule {
        needle: "footprint",
        module: "main::store",
        concern: "store hygiene",
    },
    Rule {
        needle: "git_",
        module: "main::provenance",
        concern: "git/version provenance",
    },
    Rule {
        needle: "workflow",
        module: "main::provenance",
        concern: "workflow/spec loading",
    },
    Rule {
        needle: "spec_",
        module: "main::provenance",
        concern: "workflow/spec loading",
    },
    Rule {
        needle: "docs_",
        module: "main::provenance",
        concern: "docs overlay",
    },
    Rule {
        needle: "install",
        module: "main::setup",
        concern: "project setup",
    },
    Rule {
        needle: "setup",
        module: "main::setup",
        concern: "project setup",
    },
    Rule {
        needle: "scratch",
        module: "main::setup",
        concern: "project setup",
    },
    Rule {
        needle: "skill",
        module: "main::setup",
        concern: "project setup",
    },
    Rule {
        needle: "shim",
        module: "main::setup",
        concern: "project setup",
    },
    Rule {
        needle: "scaffold",
        module: "main::setup",
        concern: "project setup",
    },
    Rule {
        needle: "live",
        module: "main::liveness",
        concern: "liveness/heartbeat reporting",
    },
    Rule {
        needle: "superseded",
        module: "main::liveness",
        concern: "liveness/heartbeat reporting",
    },
    Rule {
        needle: "format_",
        module: "main::render",
        concern: "human-readable output",
    },
    Rule {
        needle: "print_",
        module: "main::render",
        concern: "human-readable output",
    },
    Rule {
        needle: "render",
        module: "main::render",
        concern: "human-readable output",
    },
    Rule {
        needle: "parse_",
        module: "main::support",
        concern: "argument/input parsing",
    },
    Rule {
        needle: "resolve",
        module: "main::support",
        concern: "path/id resolution helper",
    },
    Rule {
        needle: "find_",
        module: "main::support",
        concern: "lookup helper",
    },
    Rule {
        needle: "read_",
        module: "main::support",
        concern: "generic read helper",
    },
    Rule {
        needle: "write_",
        module: "main::support",
        concern: "generic write helper",
    },
    Rule {
        needle: "ensure_",
        module: "main::support",
        concern: "invariant helper",
    },
    Rule {
        needle: "is_",
        module: "main::support",
        concern: "predicate helper",
    },
];

/// `src/dash.rs`'s free-function rule table - dash.rs is the read-only observability adapter,
/// so its concerns split along serve-the-request vs. render-the-page vs. reproject-the-graph.
const DASH_RULES: &[Rule] = &[
    Rule {
        needle: "serve",
        module: "dash::server",
        concern: "HTTP serving",
    },
    Rule {
        needle: "bind",
        module: "dash::server",
        concern: "HTTP serving",
    },
    Rule {
        needle: "tcp",
        module: "dash::server",
        concern: "HTTP serving",
    },
    Rule {
        needle: "route",
        module: "dash::server",
        concern: "HTTP serving",
    },
    Rule {
        needle: "handle",
        module: "dash::server",
        concern: "HTTP serving",
    },
    Rule {
        needle: "process_",
        module: "dash::server",
        concern: "HTTP serving",
    },
    Rule {
        needle: "instance",
        module: "dash::registry",
        concern: "instance registry",
    },
    Rule {
        needle: "pid",
        module: "dash::registry",
        concern: "instance registry",
    },
    Rule {
        needle: "reproject",
        module: "dash::reproject",
        concern: "graph reprojection",
    },
    Rule {
        needle: "underived",
        module: "dash::reproject",
        concern: "graph reprojection",
    },
    Rule {
        needle: "community",
        module: "dash::reproject",
        concern: "graph reprojection",
    },
    Rule {
        needle: "cluster",
        module: "dash::reproject",
        concern: "graph reprojection",
    },
    Rule {
        needle: "bucket",
        module: "dash::reproject",
        concern: "graph reprojection",
    },
    Rule {
        needle: "defs",
        module: "dash::reproject",
        concern: "graph reprojection",
    },
    Rule {
        needle: "html",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "card",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "node",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "graph",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "neighborhood",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "label",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "header",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "field",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "describe",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "displayable",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "rationale",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "role",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "escape",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "format",
        module: "dash::render",
        concern: "HTML rendering",
    },
    Rule {
        needle: "fold",
        module: "dash::render",
        concern: "HTML rendering",
    },
];

/// The three files' rule tables in [`TARGET_FILES`] order.
fn rules_for(file: &str) -> &'static [Rule] {
    match file {
        "src/conductor.rs" => CONDUCTOR_RULES,
        "src/main.rs" => MAIN_RULES,
        "src/dash.rs" => DASH_RULES,
        _ => &[],
    }
}

/// One responsibility-map entry: a scanned function's proposed home and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MapEntry {
    file: String,
    name: String,
    start_line: usize,
    end_line: usize,
    is_test: bool,
    /// `None` for a function no rule could place - present, never omitted (spec 85: "unassignable
    /// functions are named as such, never omitted").
    proposed_module: Option<String>,
    reason: String,
}

/// Assign one scanned function to a proposed module + reason (decision
/// `u85c1-classification-scheme`, mechanical/deterministic per spec 85's Design: "function
/// extraction is a brace-matching scanner... zero new dependencies"):
///
/// 1. Inside a `#[cfg(test)]`-rooted scope (directly or inherited) -> `<file>::tests`, with the
///    enclosing test-mod path named in the reason (or nested further as
///    `<file>::tests::<mod>` when the fn sits inside a named nested test module, so dash.rs's
///    `supervised_lifecycle`/`calls_route_c4`/etc. keep their own identity).
/// 2. Inside a (non-test) `impl <header>` block -> `<file_stem>::<header's Self type,
///    snake_cased>`, reasoned as "grouped with its other `<Self type>` methods".
/// 3. A free (non-test, non-method) function -> the first matching entry in that file's rule
///    table (a substring match on the function name), reasoned by the concern it names.
/// 4. Otherwise -> unassigned, reasoned as "no rule matched".
fn classify(f: &ScannedFn) -> (Option<String>, String) {
    let file_stem = file_stem(&f.file);
    if f.is_test {
        if let Some(inner) = f.enclosing_mods.iter().find(|m| m.as_str() != "tests") {
            return (
                Some(format!("{file_stem}::tests::{inner}")),
                format!(
                    "defined inside `{inner}`, a #[cfg(test)] module; proposed home groups it \
                     with that named test subgroup pending consolidation (spec 85 section 5)."
                ),
            );
        }
        return (
            Some(format!("{file_stem}::tests")),
            "defined inside a #[cfg(test)] test module; proposed home groups it with that \
             file's own test suite pending consolidation (spec 85 section 5)."
                .to_string(),
        );
    }
    if let Some(header) = &f.enclosing_impl {
        let self_type = impl_self_type(header);
        let module = format!("{file_stem}::{}", snake_case(&self_type));
        return (
            Some(module),
            format!("method inside `impl {header}`; grouped with its other `{self_type}` methods."),
        );
    }
    for rule in rules_for(&f.file) {
        if f.name.contains(rule.needle) {
            return (
                Some(rule.module.to_string()),
                format!(
                    "name contains \"{}\" ({}); grouped under `{}`.",
                    rule.needle, rule.concern, rule.module
                ),
            );
        }
    }
    (
        None,
        "no impl-block or naming-convention rule matched this free function; flagged for \
         manual triage in the follow-up refactor spec."
            .to_string(),
    )
}

fn file_stem(file: &str) -> &str {
    match file {
        "src/conductor.rs" => "conductor",
        "src/main.rs" => "main",
        "src/dash.rs" => "dash",
        other => other,
    }
}

/// Strip a LEADING balanced `<...>` generic-parameter list (the `impl`'s own generics, e.g.
/// `impl<'a, T: Foo>`) from `header`, if present - so `"<'a> RunCtx<'a>"` becomes
/// `"RunCtx<'a>"` before the type's own (trailing) generics are stripped separately. Depth is
/// tracked so a bound like `impl<T: Into<String>> Foo<T>` strips only the outer list.
fn strip_leading_impl_generics(header: &str) -> &str {
    let trimmed = header.trim_start();
    if !trimmed.starts_with('<') {
        return trimmed;
    }
    let mut depth = 0i32;
    for (idx, c) in trimmed.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return trimmed[idx + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    trimmed
}

/// The `Self` type name out of an `impl` header (`"GateRatchet"` -> `"GateRatchet"`,
/// `"AgentDriver for Stub"` -> `"Stub"`, `"gate::Runner for FlakyGate"` -> `"FlakyGate"`,
/// `"<'a> RunCtx<'a>"` -> `"RunCtx"`, `"MyGuard where MyGuard: Sized"` -> `"MyGuard"`).
fn impl_self_type(header: &str) -> String {
    let after_for = header.rsplit(" for ").next().unwrap_or(header);
    let no_leading_generics = strip_leading_impl_generics(after_for);
    // Strip a trailing where-clause BEFORE the generic split below: a where-clause bound can
    // itself contain `<...>` (e.g. `where T: Bar<Baz>`), and when the Self type has no
    // generics of its own the `split('<')` step would otherwise find that `<` first and cut
    // in the wrong place, leaving the where-clause text glued onto the self type.
    let no_where_clause = strip_trailing_where_clause(no_leading_generics);
    let generic_stripped = no_where_clause.split('<').next().unwrap_or(no_where_clause);
    generic_stripped
        .rsplit("::")
        .next()
        .unwrap_or(generic_stripped)
        .trim()
        .to_string()
}

/// Strip a trailing ` where ...` clause from an impl header fragment (already past the leading
/// `impl<...>` generics), at a word boundary so a Self type merely CONTAINING "where" as a
/// substring (e.g. a hypothetical `Somewhere` type) is never mistaken for the keyword.
fn strip_trailing_where_clause(header: &str) -> &str {
    let mut search_from = 0usize;
    while let Some(rel) = header[search_from..].find("where") {
        let start = search_from + rel;
        let end = start + "where".len();
        let before_is_boundary = header[..start]
            .chars()
            .next_back()
            .map(|c| !is_ident_char(c))
            .unwrap_or(true);
        let after_is_boundary = header[end..]
            .chars()
            .next()
            .map(|c| !is_ident_char(c))
            .unwrap_or(true);
        if before_is_boundary && after_is_boundary {
            return header[..start].trim_end();
        }
        search_from = end;
    }
    header
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if idx != 0 {
                out.push('_');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Build the full responsibility map for the checked-out tree at `root`, deterministically
/// ordered by (file, in [`TARGET_FILES`] order, then start_line).
fn build_map(root: &Path) -> Vec<MapEntry> {
    let scanned = scan_target_files(root);
    scanned
        .into_iter()
        .map(|f| {
            let (proposed_module, reason) = classify(&f);
            MapEntry {
                file: f.file,
                name: f.name,
                start_line: f.start_line,
                end_line: f.end_line,
                is_test: f.is_test,
                proposed_module,
                reason,
            }
        })
        .collect()
}

/// Deterministic pretty JSON for [`build_map`]'s output - a bare array of [`MapEntry`], each
/// field in declaration order (no `HashMap` anywhere in the shape, so `serde_json` emits the
/// SAME bytes on every run over the same tree - the drift guard's whole premise).
fn map_to_json(entries: &[MapEntry]) -> String {
    let mut s = serde_json::to_string_pretty(entries).expect("MapEntry serializes");
    s.push('\n');
    s
}

// =========================================================================================
// SECTION 1 RENDERING
// =========================================================================================

/// Render section 1 of the report: a proposed module tree (each module heading, its functions
/// sorted as scanned, one line per function with its span and reason) followed by the
/// unassigned functions named individually (never omitted).
fn pluralize_functions(n: usize) -> String {
    if n == 1 {
        "1 function".to_string()
    } else {
        format!("{n} functions")
    }
}

fn render_section_1(entries: &[MapEntry]) -> String {
    let mut modules: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.proposed_module.as_deref())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    modules.sort_unstable();

    let mut out = String::new();
    let _ = writeln!(out, "## 1. Responsibility Map");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs` \
         ({} functions total), assigned to a proposed module by \
         `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier \
         (never by hand). Instrument: the brace-matching scanner over the three named files.",
        entries.len()
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "### Proposed module tree");
    let _ = writeln!(out);
    for module in &modules {
        let mut fns: Vec<&MapEntry> = entries
            .iter()
            .filter(|e| e.proposed_module.as_deref() == Some(*module))
            .collect();
        fns.sort_by_key(|e| (e.file.clone(), e.start_line));
        let _ = writeln!(out, "- `{module}` ({})", pluralize_functions(fns.len()));
        for e in &fns {
            let _ = writeln!(
                out,
                "  - `{}:{}-{}` `{}` - {}",
                e.file, e.start_line, e.end_line, e.name, e.reason
            );
        }
    }
    let _ = writeln!(out);
    let mut unassigned: Vec<&MapEntry> = entries
        .iter()
        .filter(|e| e.proposed_module.is_none())
        .collect();
    unassigned.sort_by_key(|e| (e.file.clone(), e.start_line));
    let _ = writeln!(
        out,
        "### Unassigned ({})",
        pluralize_functions(unassigned.len())
    );
    let _ = writeln!(out);
    if unassigned.is_empty() {
        let _ = writeln!(
            out,
            "None - every scanned function matched an impl-block, test-module, or naming rule."
        );
    } else {
        for e in &unassigned {
            let _ = writeln!(
                out,
                "- `{}:{}-{}` `{}` - {}",
                e.file, e.start_line, e.end_line, e.name, e.reason
            );
        }
    }
    out
}

// =========================================================================================
// REPORT ASSEMBLY (placeholders for the sections this unit does NOT own)
// =========================================================================================

const REPORT_PATH: &str = "docs/audit/2026-09-simplification-audit.md";
const MAP_PATH: &str = "docs/audit/responsibility-map.json";

fn placeholder_section(number: u32, title: &str, owner: &str) -> String {
    format!("## {number}. {title}\n\n_Pending - criterion {owner}._\n")
}

/// Assemble the full report body for a FRESH file (no report exists yet): section 1 populated,
/// sections 2-6 left as placeholders naming the criterion/unit that owns each (`u85c2`'s
/// duplication catalog, `u85c3`'s sections 3-5, `u85c4`'s prioritized plan) - decision
/// `u85c1-report-section-placeholders`. A later unit's own generator is expected to replace
/// its own placeholder in place, leaving this section untouched.
fn assemble_fresh_report(section_1: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Simplification Audit - 2026-09");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "The audit report spec 85 derives the follow-up refactoring specs from. Six sections, \
         each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope."
    );
    let _ = writeln!(out);
    out.push_str(section_1);
    let _ = writeln!(out);
    out.push_str(&placeholder_section(
        2,
        "Duplication Catalog",
        "2 (`u85c2`)",
    ));
    let _ = writeln!(out);
    out.push_str(&placeholder_section(
        3,
        "Boundary Violations",
        "3 (`u85c3`)",
    ));
    let _ = writeln!(out);
    out.push_str(&placeholder_section(
        4,
        "Dead and Vestigial Code",
        "3 (`u85c3`)",
    ));
    let _ = writeln!(out);
    out.push_str(&placeholder_section(5, "Test-Suite Shape", "3 (`u85c3`)"));
    let _ = writeln!(out);
    out.push_str(&placeholder_section(6, "Prioritized Plan", "4 (`u85c4`)"));
    out
}

/// Replace ONLY section 1's span (from its `## 1. ` heading up to, but not including, the next
/// `## ` heading) inside an EXISTING report `existing`, leaving every other section (including
/// placeholders a later unit has since filled in) byte-for-byte untouched. Used when the report
/// file already exists (e.g. this test re-running after `RIGGER_AUDIT_WRITE=1` once).
fn replace_section_1(existing: &str, section_1: &str) -> String {
    let start = match existing.find("## 1. ") {
        Some(p) => p,
        None => return assemble_fresh_report(section_1),
    };
    let rest_after_marker = &existing[start + "## 1. ".len()..];
    let end_offset = rest_after_marker.find("\n## ").map(|p| p + 1); // keep the leading \n boundary out
    let end = match end_offset {
        Some(off) => start + "## 1. ".len() + off,
        None => existing.len(),
    };
    let mut out = String::new();
    out.push_str(&existing[..start]);
    out.push_str(section_1);
    if !section_1.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&existing[end..]);
    out
}

/// The repo root this test's own binary was compiled from - never the process CWD (mirrors
/// `tests/no_os_kill_audit.rs`'s own precedent).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_str(src: &str) -> Vec<ScannedFn> {
        scan_file("t.rs", src)
    }

    // -------------------------------------------------------------------------------------
    // Scanner: basic shapes
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_simple_free_function_is_found_with_its_line_span() {
        let src = "fn foo() {\n    let x = 1;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "foo");
        assert_eq!(fns[0].start_line, 1);
        assert_eq!(fns[0].end_line, 3);
        assert!(!fns[0].is_test);
        assert_eq!(fns[0].enclosing_impl, None);
    }

    #[test]
    fn two_sibling_functions_are_both_found_in_order() {
        let src = "fn a() {}\nfn b() {\n    let _ = 1;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "a");
        assert_eq!(fns[1].name, "b");
        assert_eq!(fns[1].start_line, 2);
        assert_eq!(fns[1].end_line, 4);
    }

    #[test]
    fn pub_async_and_unsafe_modifiers_do_not_block_detection() {
        let src = "pub async fn a() {}\npub(crate) fn b() {}\nunsafe fn c() {}\n";
        let names: Vec<_> = scan_str(src).into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn fnv1a_is_not_mistaken_for_the_fn_keyword() {
        let src = "fn fnv1a_64(bytes: &[u8]) -> u64 {\n    0\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "fnv1a_64");
    }

    // -------------------------------------------------------------------------------------
    // Scanner: lexical states must not corrupt brace matching
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_brace_inside_a_line_comment_is_ignored() {
        let src = "fn a() {\n    // a stray { brace\n    let _ = 1;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 4);
    }

    #[test]
    fn a_brace_inside_a_block_comment_is_ignored() {
        let src = "fn a() {\n    /* a { stray } brace */\n    let _ = 1;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 4);
    }

    #[test]
    fn nested_block_comments_are_handled() {
        let src = "fn a() {\n    /* outer /* inner { */ still comment */\n    let _ = 1;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 4);
    }

    #[test]
    fn a_brace_inside_a_string_literal_is_ignored() {
        let src = "fn a() {\n    let s = \"{ not a brace }\";\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 3);
    }

    #[test]
    fn a_brace_inside_a_raw_string_with_hashes_is_ignored() {
        let src = "fn a() {\n    let s = r#\"{ not \\\"real\\\" }\"#;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 3);
    }

    #[test]
    fn a_brace_inside_a_byte_string_is_ignored() {
        let src = "fn a() {\n    let s = b\"{ not a brace }\";\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 3);
    }

    #[test]
    fn a_brace_char_literal_is_not_mistaken_for_real_braces() {
        let src = "fn a() {\n    let c = '{';\n    let d = '}';\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 4);
    }

    #[test]
    fn a_lifetime_is_not_mistaken_for_a_char_literal() {
        let src = "fn a<'x>(v: &'x str) -> &'x str {\n    v\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 3);
    }

    #[test]
    fn an_escaped_quote_char_literal_does_not_confuse_the_scanner() {
        let src = "fn a() {\n    let c = '\\'';\n    let _ = 1;\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].end_line, 4);
    }

    // -------------------------------------------------------------------------------------
    // Scanner: bodyless signatures and fn-pointer types are excluded
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_trait_method_signature_without_a_body_is_not_recorded() {
        let src = "trait T {\n    fn spawn(&self, x: &str) -> Result<(), ()>;\n}\n";
        let fns = scan_str(src);
        assert!(fns.is_empty(), "{fns:?}");
    }

    #[test]
    fn a_trait_default_method_with_a_body_is_recorded() {
        let src = "trait T {\n    fn spawn(&self) {\n        let _ = 1;\n    }\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "spawn");
    }

    #[test]
    fn a_fn_pointer_type_usage_is_not_recorded() {
        let src = "fn takes_fp(f: fn(usize) -> bool) -> bool {\n    f(1)\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1, "{fns:?}");
        assert_eq!(fns[0].name, "takes_fp");
    }

    #[test]
    fn a_semicolon_inside_an_array_type_param_does_not_end_the_signature_early() {
        let src = "fn a(buf: [u8; 32]) -> bool {\n    buf.len() == 32\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1, "{fns:?}");
        assert_eq!(fns[0].name, "a");
        assert_eq!(fns[0].end_line, 3);
    }

    // -------------------------------------------------------------------------------------
    // Scanner: context (impl blocks, mods, #[cfg(test)] inheritance)
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_method_inside_an_impl_block_carries_its_header() {
        let src = "impl Foo {\n    fn bar(&self) {}\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "bar");
        assert_eq!(fns[0].enclosing_impl.as_deref(), Some("Foo"));
    }

    #[test]
    fn a_trait_impl_header_keeps_the_trait_for_type_text() {
        let src = "impl AgentDriver for Stub {\n    fn spawn(&self) {}\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(
            fns[0].enclosing_impl.as_deref(),
            Some("AgentDriver for Stub")
        );
    }

    #[test]
    fn a_method_inside_an_impl_nested_in_a_cfg_test_mod_is_flagged_test() {
        // Regression (adjudicator u85c1 round 1 REJECT): an impl block sitting inside a
        // #[cfg(test)] mod must propagate that ancestry to its methods - FrameKind::Impl
        // previously had no is_test field at all, so this was always false.
        let src = "#[cfg(test)]\nmod tests {\n    impl AgentDriver for CacheDriver {\n        fn spawn(&self) {}\n    }\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_test, "{:?}", fns[0]);
        assert_eq!(
            fns[0].enclosing_impl.as_deref(),
            Some("AgentDriver for CacheDriver")
        );
    }

    #[test]
    fn a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test() {
        // Regression: a #[cfg(test)] attribute attached DIRECTLY to a standalone impl block
        // (no enclosing cfg-test mod) must also mark its methods test - the impl push site
        // used to drop pending_cfg_test unconditionally instead of reading it like
        // Mod/Trait/Fn already do.
        let src = "#[cfg(test)]\nimpl<'a> RunCtx<'a> {\n    fn for_test() -> Self {\n        todo!()\n    }\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_test, "{:?}", fns[0]);
    }

    #[test]
    fn a_non_cfg_test_impl_block_still_inherits_a_non_test_ancestry() {
        // Sanity: the fix must not make every impl test-only - a plain impl outside any
        // cfg-test context stays production.
        let src = "impl Foo {\n    fn bar(&self) {}\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(!fns[0].is_test, "{:?}", fns[0]);
    }

    #[test]
    fn a_function_directly_in_a_cfg_test_mod_is_flagged_test() {
        let src = "#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_test);
        assert_eq!(fns[0].enclosing_mods, vec!["tests".to_string()]);
    }

    #[test]
    fn a_nested_named_test_submodule_is_still_flagged_test_and_named() {
        let src = "#[cfg(test)]\nmod tests {\n    mod inner_group {\n        fn a() {}\n    }\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_test);
        assert_eq!(
            fns[0].enclosing_mods,
            vec!["tests".to_string(), "inner_group".to_string()]
        );
    }

    #[test]
    fn a_bare_test_attribute_on_a_free_function_marks_it_test_without_a_cfg_test_mod() {
        let src = "#[test]\nfn a_thing_works() {}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_test);
    }

    #[test]
    fn production_functions_before_a_cfg_test_mod_are_not_flagged_test() {
        let src = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 2);
        assert!(!fns[0].is_test);
        assert!(fns[1].is_test);
    }

    #[test]
    fn a_mod_declaration_without_a_body_is_not_pushed_as_a_frame() {
        let src = "mod gitsemver;\nfn a() {}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "a");
        assert_eq!(fns[0].enclosing_mods, Vec::<String>::new());
    }

    #[test]
    fn a_locally_nested_fn_inside_a_function_body_is_still_found() {
        let src = "fn outer() {\n    fn inner() {\n        let _ = 1;\n    }\n    inner();\n}\n";
        let names: Vec<_> = scan_str(src).into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["inner".to_string(), "outer".to_string()]);
    }

    #[test]
    fn braces_from_if_match_and_closures_do_not_break_the_enclosing_fns_span() {
        let src = "fn a(x: i32) -> i32 {\n    if x > 0 {\n        1\n    } else {\n        match x {\n            _ => 0,\n        }\n    }\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "a");
        assert_eq!(fns[0].end_line, 9);
    }

    // -------------------------------------------------------------------------------------
    // Classifier
    // -------------------------------------------------------------------------------------

    fn conductor_fn(name: &str) -> ScannedFn {
        ScannedFn {
            file: "src/conductor.rs".to_string(),
            name: name.to_string(),
            start_line: 1,
            end_line: 2,
            is_test: false,
            enclosing_impl: None,
            enclosing_mods: Vec::new(),
        }
    }

    #[test]
    fn a_test_function_is_classified_under_that_files_tests_module() {
        let mut f = conductor_fn("a_thing_works");
        f.is_test = true;
        f.enclosing_mods = vec!["tests".to_string()];
        let (module, _) = classify(&f);
        assert_eq!(module.as_deref(), Some("conductor::tests"));
    }

    #[test]
    fn a_named_test_submodule_keeps_its_own_identity() {
        let mut f = ScannedFn {
            file: "src/dash.rs".to_string(),
            name: "a_case".to_string(),
            start_line: 1,
            end_line: 2,
            is_test: true,
            enclosing_impl: None,
            enclosing_mods: vec!["tests".to_string(), "supervised_lifecycle".to_string()],
        };
        f.is_test = true;
        let (module, _) = classify(&f);
        assert_eq!(module.as_deref(), Some("dash::tests::supervised_lifecycle"));
    }

    #[test]
    fn a_method_is_classified_under_its_impl_self_type() {
        let mut f = conductor_fn("summary");
        f.enclosing_impl = Some("GateRatchet".to_string());
        let (module, reason) = classify(&f);
        assert_eq!(module.as_deref(), Some("conductor::gate_ratchet"));
        assert!(reason.contains("GateRatchet"), "{reason}");
    }

    #[test]
    fn a_method_on_a_generic_impl_block_strips_the_impls_own_leading_generics() {
        // Regression: `impl<'a> RunCtx<'a> { ... }` must classify under `run_ctx`, not under
        // an empty self-type bucket (the impl's OWN `<'a>` generic list is not the type name).
        let mut f = conductor_fn("for_test");
        f.enclosing_impl = Some("<'a> RunCtx<'a>".to_string());
        let (module, reason) = classify(&f);
        assert_eq!(module.as_deref(), Some("conductor::run_ctx"));
        assert!(reason.contains("RunCtx"), "{reason}");
        assert!(!reason.contains("`` methods"), "{reason}");
    }

    #[test]
    fn impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type() {
        // Regression (sdet-u85c1-where-clause-impl-self-type-untested, currently dormant but
        // unguarded): when the Self type itself has no `<...>` generics, the OLD
        // `split('<')` step never found a `<` to cut at, so a trailing where-clause (which
        // can itself contain `<...>`, e.g. a bound like `T: Bar<Baz>`) leaked into the
        // "self type" text verbatim.
        assert_eq!(
            impl_self_type("Drop for MyGuard where MyGuard: Sized"),
            "MyGuard"
        );
        assert_eq!(impl_self_type("MyGuard where MyGuard: Bar<Baz>"), "MyGuard");
    }

    #[test]
    fn impl_self_type_still_handles_a_generic_self_type_with_a_where_clause() {
        assert_eq!(impl_self_type("Foo<T> where T: Bar<Baz>"), "Foo");
    }

    #[test]
    fn strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match() {
        // A Self type that merely CONTAINS "where" as a substring must survive untouched -
        // only a real `where` keyword at a word boundary is a clause start.
        assert_eq!(strip_trailing_where_clause("Somewhere"), "Somewhere");
        assert_eq!(strip_trailing_where_clause("Foo where T: Bar"), "Foo");
        assert_eq!(strip_trailing_where_clause("Foo"), "Foo");
    }

    #[test]
    fn a_trait_impl_method_is_classified_under_the_implementing_type_not_the_trait() {
        let mut f = conductor_fn("spawn");
        f.enclosing_impl = Some("AgentDriver for Stub".to_string());
        let (module, _) = classify(&f);
        assert_eq!(module.as_deref(), Some("conductor::stub"));
    }

    #[test]
    fn a_free_function_matches_the_first_rule_in_table_order() {
        let f = conductor_fn("run_unit_speculation_lane");
        let (module, reason) = classify(&f);
        // "speculat" is listed before "spawn"/"run_unit" in CONDUCTOR_RULES.
        assert_eq!(module.as_deref(), Some("conductor::spawn"));
        assert!(reason.contains("speculat"), "{reason}");
    }

    #[test]
    fn an_unmatched_free_function_is_explicitly_unassigned_not_omitted() {
        let f = conductor_fn("zzzznomatchzzzz");
        let (module, reason) = classify(&f);
        assert_eq!(module, None);
        assert!(
            reason.contains("no impl-block or naming-convention rule"),
            "{reason}"
        );
    }

    #[test]
    fn every_rule_table_is_internally_deterministic_and_non_empty() {
        for file in TARGET_FILES {
            assert!(!rules_for(file).is_empty(), "{file} has no rules");
        }
    }

    // -------------------------------------------------------------------------------------
    // Map + section 1 rendering
    // -------------------------------------------------------------------------------------

    #[test]
    fn pluralize_functions_uses_singular_only_at_exactly_one() {
        assert_eq!(pluralize_functions(0), "0 functions");
        assert_eq!(pluralize_functions(1), "1 function");
        assert_eq!(pluralize_functions(2), "2 functions");
    }

    #[test]
    fn map_to_json_is_byte_identical_across_two_runs_over_the_same_input() {
        let entries = vec![MapEntry {
            file: "src/conductor.rs".to_string(),
            name: "a".to_string(),
            start_line: 1,
            end_line: 2,
            is_test: false,
            proposed_module: Some("conductor::support".to_string()),
            reason: "x".to_string(),
        }];
        assert_eq!(map_to_json(&entries), map_to_json(&entries));
    }

    #[test]
    fn section_1_names_every_module_and_every_unassigned_function() {
        let entries = vec![
            MapEntry {
                file: "src/conductor.rs".to_string(),
                name: "a".to_string(),
                start_line: 1,
                end_line: 2,
                is_test: false,
                proposed_module: Some("conductor::support".to_string()),
                reason: "reason-a".to_string(),
            },
            MapEntry {
                file: "src/conductor.rs".to_string(),
                name: "b".to_string(),
                start_line: 3,
                end_line: 4,
                is_test: false,
                proposed_module: None,
                reason: "reason-b".to_string(),
            },
        ];
        let rendered = render_section_1(&entries);
        assert!(rendered.contains("## 1. Responsibility Map"));
        assert!(rendered.contains("conductor::support"));
        assert!(rendered.contains("`a`"));
        assert!(rendered.contains("reason-a"));
        assert!(rendered.contains("### Unassigned (1 function)"));
        assert!(rendered.contains("`b`"));
        assert!(rendered.contains("reason-b"));
    }

    #[test]
    fn section_1_reports_none_unassigned_explicitly_when_everything_is_assigned() {
        let entries = vec![MapEntry {
            file: "src/conductor.rs".to_string(),
            name: "a".to_string(),
            start_line: 1,
            end_line: 2,
            is_test: false,
            proposed_module: Some("conductor::support".to_string()),
            reason: "r".to_string(),
        }];
        let rendered = render_section_1(&entries);
        assert!(rendered.contains("None - every scanned function"));
    }

    #[test]
    fn assemble_fresh_report_contains_section_1_and_every_placeholder() {
        let report = assemble_fresh_report("## 1. Responsibility Map\n\nbody\n");
        assert!(report.contains("## 1. Responsibility Map"));
        assert!(report.contains("## 2. Duplication Catalog"));
        assert!(report.contains("_Pending - criterion 2 (`u85c2`)._"));
        assert!(report.contains("## 3. Boundary Violations"));
        assert!(report.contains("## 4. Dead and Vestigial Code"));
        assert!(report.contains("## 5. Test-Suite Shape"));
        assert!(report.contains("## 6. Prioritized Plan"));
        assert!(report.contains("_Pending - criterion 4 (`u85c4`)._"));
    }

    #[test]
    fn replace_section_1_only_touches_section_1_leaving_later_sections_intact() {
        let existing = "# Title\n\n## 1. Responsibility Map\n\nold body\n\n## 2. Duplication Catalog\n\nfilled in by u85c2\n";
        let updated = replace_section_1(existing, "## 1. Responsibility Map\n\nnew body\n");
        assert!(updated.contains("new body"));
        assert!(!updated.contains("old body"));
        assert!(updated.contains("filled in by u85c2"));
    }

    // -------------------------------------------------------------------------------------
    // The Done-when acceptance tests: the REAL checked-out tree, criterion 1's own bar.
    // -------------------------------------------------------------------------------------

    /// THE COVERAGE PROOF (spec 85 THOROUGHNESS: "asserted by the generator - count of scanned
    /// fns in the three files == count of mapped fns"): every function `scan_target_files`
    /// finds over the real tree gets exactly one [`MapEntry`], none dropped.
    #[test]
    fn the_real_tree_map_covers_every_scanned_function() {
        let root = repo_root();
        let scanned = scan_target_files(&root);
        let map = build_map(&root);
        assert_eq!(
            scanned.len(),
            map.len(),
            "scanned {} functions but mapped {} - the responsibility map must cover every one",
            scanned.len(),
            map.len()
        );
        assert!(
            !map.is_empty(),
            "expected to find functions in the three target files"
        );
    }

    /// No unassigned entry is ever silently dropped: every `proposed_module: None` row still
    /// carries a file:line:name identity a reader can act on.
    #[test]
    fn every_unassigned_entry_in_the_real_map_still_names_its_function() {
        let root = repo_root();
        let map = build_map(&root);
        for e in map.iter().filter(|e| e.proposed_module.is_none()) {
            assert!(!e.name.is_empty());
            assert!(!e.file.is_empty());
        }
    }

    /// THE DRIFT GUARD for `docs/audit/responsibility-map.json`: with `RIGGER_AUDIT_WRITE=1`
    /// set, regenerate and overwrite it; otherwise regenerate in memory and assert it matches
    /// the committed file byte-for-byte, so the catalog can never silently drift from the tree
    /// (spec 85 Design).
    #[test]
    fn responsibility_map_json_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let map = build_map(&root);
        let json = map_to_json(&map);
        let path = root.join(MAP_PATH);
        if std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1") {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, &json).unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{MAP_PATH} is missing or unreadable ({e}) - run with RIGGER_AUDIT_WRITE=1 to \
                 generate it"
            )
        });
        assert_eq!(
            committed, json,
            "{MAP_PATH} has drifted from the tree - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    /// THE DRIFT GUARD for section 1 of the report: with `RIGGER_AUDIT_WRITE=1` set,
    /// (re)write it (creating the report fresh with placeholders for the sections this
    /// criterion does not own, or replacing only section 1's span if the report already
    /// exists); otherwise assert the committed report's section 1 matches byte-for-byte.
    #[test]
    fn report_section_1_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let map = build_map(&root);
        let section_1 = render_section_1(&map);
        let path = root.join(REPORT_PATH);
        let write = std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1");
        let existing = fs::read_to_string(&path).ok();
        if write {
            let updated = match &existing {
                Some(text) => replace_section_1(text, &section_1),
                None => assemble_fresh_report(&section_1),
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, updated).unwrap();
            return;
        }
        let committed = existing.unwrap_or_else(|| {
            panic!("{REPORT_PATH} is missing - run with RIGGER_AUDIT_WRITE=1 to generate it")
        });
        let expected = replace_section_1(&committed, &section_1);
        // Comparing the rebuilt-from-committed output to itself would be vacuous; instead
        // assert section 1's own rendered text is present verbatim in the committed file, and
        // that re-applying replace_section_1 is a no-op (proves nothing outside section 1 was
        // touched AND section 1 already matches byte-for-byte).
        assert_eq!(
            expected, committed,
            "{REPORT_PATH} section 1 has drifted from the tree - regenerate with \
             RIGGER_AUDIT_WRITE=1"
        );
        assert!(
            committed.contains(&section_1),
            "{REPORT_PATH} must contain section 1 verbatim"
        );
    }
}
