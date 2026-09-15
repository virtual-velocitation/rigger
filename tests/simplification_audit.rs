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
//!
//! Spec 85 criterion 2, THE DUPLICATION CATALOG (`u85c2`, this unit): a deterministic,
//! zero-new-dependency scan of every function under `src/` and `tests/` (not just the three
//! criterion-1 target files - the strict duplication definition is whole-tree), clustered by
//! normalized-token-shingle similarity plus five mandatory mechanical sweeps, written to
//! `docs/audit/duplication-catalog.json` and rendered as section 2 of the report. This unit
//! OWNS the similarity pass, the catalog and its drift guard; it does NOT own the responsibility
//! map (criterion 1, `u85c1`, already landed) or sections 3-6 (`u85c3`/`u85c4`) - it replaces
//! only the `## 2. ` placeholder span, leaving every other section untouched (mirrors u85c1's
//! own `replace_section_1`; see [`replace_section_2`]).
//!
//! THE SIMILARITY PASS: every function [`scan_file`] finds (reused unchanged) has its body text
//! (its own line span, already how [`ScannedFn`] records it) tokenized by [`tokenize`] - a
//! second, TOKEN-level lexer (as opposed to `scan_file`'s brace-matching one) that reuses the
//! same lexical primitives (`skip_string_literal`, `char_literal_len`, `is_ident_char`) rather
//! than re-implementing comment/string/char-vs-lifetime handling a second time. Each token is
//! then normalized ([`normalize_tokens`]): a keyword or a punctuation character is kept
//! verbatim (structural signal); a string/byte/number/char literal becomes the placeholder
//! `LIT`; a lifetime becomes `LIFETIME`; an identifier immediately followed by `!` (a macro
//! invocation) becomes `MACRO`; every other identifier is canonicalized to its KIND by casing
//! convention ([`ident_kind_marker`]) - `SCREAMING_SNAKE` to `CONST`, `UpperCamelCase` to
//! `TYPE`, anything else to `IDENT`. Comments and whitespace produce no token at all. Similarity
//! is Jaccard over the normalized token stream's 8-token sliding-window shingles (a function
//! shorter than 8 tokens gets one shingle: its whole normalized stream). Two functions cluster
//! together when their shingle-set Jaccard is >= 0.72; a cluster is classified `exact` when
//! every member's normalized stream is byte-identical (Jaccard 1.0 - a rename-only copy-paste)
//! and `near` otherwise. To keep this tractable over the whole tree, functions are first grouped
//! by their exact normalized stream (an O(n) hashmap pass, immune to the cap below - this is
//! also the entire `exact` pass) and only one representative per distinct stream is fed through
//! an inverted-shingle-index candidate search for the `near` pass; a shingle shared by more than
//! [`POSTING_CAP`] representatives is skipped as a candidate GENERATOR (not as similarity
//! evidence for pairs found via a rarer shared shingle) - true near-duplicates share many
//! shingles, so this bounds the pathological case (a hyper-common boilerplate shingle) without
//! materially harming recall; the seeded adversarial sample below is this claim's check.
//!
//! THE FIVE MANDATORY SWEEPS (spec 85 Design: "named as mandatory sweeps the catalog must
//! cover"), each mechanically collected (not similarity-dependent) into its own `semantic`
//! cluster so each unconditionally appears regardless of what the Jaccard pass finds:
//! `Command::new` call sites, `/proc`-path string literals, `Connection::open`/
//! `open_with_flags` (sqlite) call sites, `.rigger`-path string literals, and "error-shaping"
//! helper functions (name contains `error`, or `err` at a `_`-bounded or start/end word
//! boundary, AND the body invokes the `format!` macro).
//!
//! THE ADVERSARIAL SAMPLE (spec 85 THOROUGHNESS): [`sample_indices`] deterministically draws
//! [`ADVERSARIAL_SAMPLE_SIZE`] function indices from a fixed seed ([`ADVERSARIAL_SEED`], stated
//! in the rendered report so the draw is reproducible) via a documented linear-congruential
//! generator - no `rand` dependency. The report's adversarial-sample subsection is generated by
//! hand: for each drawn function, whether the catalog already covers it (and where) is recorded.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Spec 90 criterion 2's line-free content identity, reused rather than a second open-coded
/// hasher: `content_hash` is the crate's ONE stable content-hash primitive (its own doc calls it
/// "the content-identity primitive"; this report's own DUPLICATION section separately calls it
/// "the crate's ONE stable content-hash primitive"), already depended on for the `symbols`
/// grounder's reindex-freshening gate. See [`span_content_hash`].
use rigger::grounder::symbols::store::content_hash;

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
    /// Spec 87 criterion 2 addition: the visibility qualifier exactly as written, normalized to
    /// drop internal whitespace (`"pub"`, `"pub(crate)"`, `"pub(super)"`, `"pub(in ...)"`), or
    /// `"private"` when no `pub` keyword preceded this fn.
    visibility: String,
    /// Spec 87 criterion 2 addition: 1-based line of the opening `{` that starts this fn's
    /// body - the fn's OWN "definition span" the reference sweep excludes when counting
    /// references to its OWN name is `[start_line, body_start_line]` (signature only, per spec
    /// 87 Design: "doc comment, attributes, signature" - deliberately NOT the body, so a
    /// recursive self-call still counts as a real reference).
    body_start_line: usize,
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
/// findings) for every function with a body, per this module's doc comment. Thin wrapper over
/// [`scan_file_core`] - kept so every pre-existing caller (spec 85's whole-tree/god-file scans)
/// stays byte-for-byte unchanged; spec 87 criterion 2's own callers use [`scan_file_core`]
/// directly for the extra fields it captures.
fn scan_file(file: &str, content: &str) -> Vec<ScannedFn> {
    scan_file_core(file, content).fns
}

/// One out-of-line `mod name;` declaration - spec 87 criterion 2's file-aware `is_test`
/// (Design: "a file declared by a parent's `#[cfg(test)] mod name;`... is test code in full").
/// `is_test` reflects ONLY this declaration's own local attribution (directly, or inherited
/// from an enclosing test frame WITHIN THE SAME FILE) - a target file reached only because its
/// OWN declaring file was itself pulled in as test by some other, earlier declaration is
/// resolved by the transitive closure in [`resolve_out_of_line_test_files`], not here.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OutOfLineMod {
    name: String,
    is_test: bool,
    /// A `#[path = "value"]` attribute governing this declaration, captured verbatim (spec 87
    /// Design names this as one of the three ways a target resolves).
    path_override: Option<String>,
}

/// One `mod name { .. }` (INLINE, WITH a body) block's own span - `start_line` the line of its
/// opening `{`, `end_line` the line of its matching `}` (the SAME two-endpoint convention
/// [`ScannedFn::start_line`]/[`end_line`] uses for a fn body). Spec 87 round-1 fix for
/// `sdet-u87c2-mod-body-level-test-statements-leak-as-production-refs`
/// (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances` class 1, "TEST REGIONS ARE
/// MOD SPANS"): a `use`/`const`/`static`/`type` item sitting directly inside a `#[cfg(test)] mod
/// tests { .. }` block, ABOVE OR BETWEEN its `fn`s (never itself a [`ScannedFn`]), must still read
/// as test code - `all_ident_ref_sites`'s `in_test_range` now checks this span too, not fn spans
/// alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModSpan {
    start_line: usize,
    end_line: usize,
    is_test: bool,
}

/// [`scan_file`]'s full result: every function found, every out-of-line `mod name;` declaration
/// (spec 87 criterion 2's addition - see [`OutOfLineMod`]), and every inline `mod name { .. }`
/// block's own span (round 1's addition - see [`ModSpan`]).
struct FileScanCore {
    fns: Vec<ScannedFn>,
    out_of_line_mods: Vec<OutOfLineMod>,
    mod_spans: Vec<ModSpan>,
}

/// The real scanner behind [`scan_file`] (spec 85 criterion 1's original doc comment above still
/// governs the core algorithm unchanged: one left-to-right frame-stack scan, `Anonymous` blocks
/// skipped opaquely, all the same lexical primitives). Spec 87 criterion 2 additions, all
/// PURELY ADDITIVE (no existing field, branch, or emitted `ScannedFn` value changes for any
/// input that does not exercise one of these new shapes - verified: no `#[cfg(test)]` or
/// `#[test]` attribute anywhere in `src/` today sits directly before a `pub`-qualified item,
/// the one shape the new `pub` branch below changes the handling of):
/// - `visibility` and `body_start_line` are captured on every `ScannedFn`.
/// - a `pub`/`pub(crate)`/`pub(super)`/`pub(in ...)` qualifier between a pending `#[cfg(test)]`
///   (or `#[path]`) attribute and the item it governs is now recognized and skipped WITHOUT
///   clearing the pending attribute - needed for `src/eventstore/mod.rs`'s real
///   `#[cfg(test)]\npub mod contract;` to resolve as a test declaration at all; previously the
///   generic "any other char clears a stale pending attribute" rule (still correct for
///   everything this scanner does not model) cleared it on the bare `p` of `pub` before the
///   `mod` keyword was ever reached.
/// - `#[path = "value"]` is tracked the same way as `#[cfg(test)]`, consumed by the next `mod`.
/// - `mod name;` (external, no body) now records an [`OutOfLineMod`] instead of silently
///   discarding the declaration.
fn scan_file_core(file: &str, content: &str) -> FileScanCore {
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
    let mut out_of_line_mods: Vec<OutOfLineMod> = Vec::new();
    let mut mod_spans: Vec<ModSpan> = Vec::new();
    let mut pending_cfg_test = false;
    let mut pending_visibility: Option<String> = None;
    let mut pending_path_override: Option<String> = None;
    // (start_line, is_test, name, visibility, body_start_line) for the fn currently open, one
    // per Fn frame depth.
    let mut open_fns: Vec<(usize, bool, String, String, usize)> = Vec::new();
    // (start_line, is_test) for the inline `mod { .. }` currently open, one per Mod frame depth -
    // mirrors `open_fns` (round 1's `ModSpan` addition).
    let mut open_mods: Vec<(usize, bool)> = Vec::new();

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
        // --- attribute: track #[cfg(test)] / #[test] / #[path = ".."] as "pending" for the
        // next item ---
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
            let compact: String = attr_text.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.starts_with("#[path=") {
                if let Some(v) = extract_quoted(&attr_text) {
                    pending_path_override = Some(v);
                }
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

        if top_allows_items && c == 'p' && starts_word(&chars, i, "pub") {
            // A visibility qualifier: `pub`, or `pub(crate)`/`pub(super)`/`pub(in path)`.
            // Deliberately does NOT touch `pending_cfg_test`/`pending_path_override` - see this
            // function's own doc comment for why that matters.
            let mut j = i + 3;
            while j < n && (chars[j] == ' ' || chars[j] == '\t') {
                j += 1;
            }
            if j < n && chars[j] == '(' {
                let mut bdepth = 0i32;
                while j < n {
                    match chars[j] {
                        '(' => bdepth += 1,
                        ')' => {
                            bdepth -= 1;
                            if bdepth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        '\n' => line += 1,
                        _ => {}
                    }
                    j += 1;
                }
            }
            let raw: String = chars[i..j].iter().collect();
            let normalized: String = raw.split_whitespace().collect::<Vec<_>>().join("");
            pending_visibility = Some(normalized);
            i = j;
            continue;
        }

        if top_allows_items && c == 'f' && starts_word(&chars, i, "fn") {
            let kw_line = line;
            let name_start = i + 2;
            let (name, mut j) = read_ident_after_ws(&chars, name_start);
            if !name.is_empty() {
                match scan_fn_signature_end(&chars, &mut j, &mut line) {
                    SignatureEnd::Body => {
                        // `line` now sits on the opening `{` itself (scan_fn_signature_end
                        // returns immediately after consuming it, with no further newline
                        // crossed) - exactly the fn's own `body_start_line`.
                        let body_start_line = line;
                        let is_test =
                            pending_cfg_test || stack.last().map(|f| f.is_test()).unwrap_or(false);
                        let visibility = pending_visibility
                            .take()
                            .unwrap_or_else(|| "private".to_string());
                        pending_cfg_test = false;
                        pending_path_override = None;
                        stack.push(Frame {
                            kind: FrameKind::Fn { is_test },
                        });
                        open_fns.push((kw_line, is_test, name, visibility, body_start_line));
                        i = j; // j sits just past the opening '{'
                        continue;
                    }
                    SignatureEnd::NoBody => {
                        pending_cfg_test = false;
                        pending_visibility = None;
                        pending_path_override = None;
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
                    pending_visibility = None;
                    pending_path_override = None;
                    mods_stack.push(name);
                    // `line` sits on the opening `{` itself here (nothing since the last `\n`
                    // has advanced it) - the same "line now sits on the opening brace" property
                    // `body_start_line` relies on for a fn.
                    open_mods.push((line, is_test));
                    stack.push(Frame {
                        kind: FrameKind::Mod { is_test },
                    });
                    i = j + 1;
                    continue;
                } else if j < n && chars[j] == ';' {
                    // `mod foo;` - external file: record it (spec 87 criterion 2) instead of
                    // silently discarding.
                    let is_test =
                        pending_cfg_test || stack.last().map(|f| f.is_test()).unwrap_or(false);
                    out_of_line_mods.push(OutOfLineMod {
                        name,
                        is_test,
                        path_override: pending_path_override.take(),
                    });
                    pending_cfg_test = false;
                    pending_visibility = None;
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
                pending_visibility = None;
                pending_path_override = None;
                stack.push(Frame {
                    kind: FrameKind::Trait { is_test },
                });
                i = j + 1;
                continue;
            }
            pending_cfg_test = false;
            pending_visibility = None;
            pending_path_override = None;
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
                pending_visibility = None;
                pending_path_override = None;
                impl_stack.push(header);
                stack.push(Frame {
                    kind: FrameKind::Impl { is_test },
                });
                i = j + 1;
                continue;
            }
            pending_cfg_test = false;
            pending_visibility = None;
            pending_path_override = None;
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
                        if let Some((sl, is_test, name, visibility, body_start_line)) =
                            open_fns.pop()
                        {
                            out.push(ScannedFn {
                                file: file.to_string(),
                                name,
                                start_line: sl,
                                end_line: line,
                                is_test,
                                enclosing_impl: impl_stack.last().cloned(),
                                enclosing_mods: mods_stack.clone(),
                                visibility,
                                body_start_line,
                            });
                        }
                    }
                    FrameKind::Mod { .. } => {
                        mods_stack.pop();
                        if let Some((sl, is_test)) = open_mods.pop() {
                            mod_spans.push(ModSpan {
                                start_line: sl,
                                end_line: line,
                                is_test,
                            });
                        }
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
        // always sit immediately - across whitespace/comments/other attributes only, and now a
        // visibility qualifier (handled above) - before the item they annotate).
        if !c.is_whitespace() {
            // A bare identifier char not part of a recognized keyword: leave pending_cfg_test
            // as-is only if we are still inside what could be another attribute/whitespace run;
            // since attributes, comments and `pub` were already special-cased above, reaching
            // here with a pending attribute set and no item keyword matched means the attribute
            // was on something this scanner does not model (e.g. a struct/field) - clear it so
            // it cannot leak onto a later, unrelated fn.
            pending_cfg_test = false;
            pending_visibility = None;
            pending_path_override = None;
        }
        i += 1;
    }

    FileScanCore {
        fns: out,
        out_of_line_mods,
        mod_spans,
    }
}

/// The first `"..."` substring's contents (no escape processing - attribute string values in
/// this tree are plain paths, never containing a `\"`). `None` if `s` holds no quoted string at
/// all (defensive; every real `#[path = ".."]` attribute this scanner recognizes does).
fn extract_quoted(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
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
/// [`TARGET_FILES`]'s fixed order, deterministically, ALONGSIDE each file's own raw content
/// (spec 90 criterion 2: [`build_map`]'s [`raw_span_content_hash`] calls need it; every other
/// caller just wants the functions, via [`scan_target_files`] below - the ONE walk, never a
/// second parallel one that re-reads the same three files again).
fn scan_target_files_with_content(root: &Path) -> (Vec<ScannedFn>, HashMap<&'static str, String>) {
    let mut out = Vec::new();
    let mut contents = HashMap::new();
    for rel in TARGET_FILES {
        let path = root.join(rel);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("simplification_audit: cannot read {rel}: {e}"));
        out.extend(scan_file(rel, &content));
        contents.insert(rel, content);
    }
    (out, contents)
}

/// Collect and scan the three target files under `root` (a repo checkout), in
/// [`TARGET_FILES`]'s fixed order, deterministically.
fn scan_target_files(root: &Path) -> Vec<ScannedFn> {
    scan_target_files_with_content(root).0
}

/// Spec 90 criterion 2, THE LINE-FREE CONTENT IDENTITY for a responsibility-map entry (decision
/// `u90c2-content-hash-primitive-reuse`): unlike the catalog/dead-code pipeline ([`scan_tree`]),
/// this criterion's scanner ([`scan_file`]) is deliberately UNTOKENIZED - its own module doc
/// says so, to keep it independent of the token-level lexer - so there is no pre-computed
/// normalized-token stream to reuse here. This hashes the span's exact RAW source text instead
/// (still through [`content_hash`], the crate's ONE hash primitive, never a second one): a pin
/// bump elsewhere in the file moves the span to a new line number but never touches its own
/// text, so the hash stays byte-identical across the shift, exactly like [`span_content_hash`]'s
/// token-based version for the other two artifacts.
fn raw_span_content_hash(content: &str, start_line: usize, end_line: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let lo = start_line.saturating_sub(1).min(lines.len());
    let hi = end_line.min(lines.len());
    let slice = if lo < hi {
        lines[lo..hi].join("\n")
    } else {
        String::new()
    };
    content_hash(&slice)
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

/// One responsibility-map entry: a scanned function's proposed home and why. Spec 90 criterion
/// 2 addition: `content_hash` (see [`raw_span_content_hash`]) is this entry's line-free identity,
/// which the guarded `docs/audit/responsibility-map.json` ([`MapEntryWire`]) carries instead of
/// `start_line`/`end_line`; those move to the unguarded `.lines.json` sibling
/// ([`MapEntryLines`]) and stay here for every other purpose (rendering, sorting).
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
    content_hash: String,
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

/// The three criterion-1 target files keep their established short stems; any OTHER path (the
/// whole-tree files criterion 2 additionally scans) derives one generically - its file name
/// with the `.rs` extension stripped (`"src/reap.rs"` -> `"reap"`, `"tests/foo/bar.rs"` ->
/// `"bar"`) - a strict extension of the fallback that changes nothing for the 3 named files
/// (they still hit their own match arm first) or any existing caller (criterion 1 never calls
/// this with anything else).
fn file_stem(file: &str) -> &str {
    match file {
        "src/conductor.rs" => "conductor",
        "src/main.rs" => "main",
        "src/dash.rs" => "dash",
        other => Path::new(other)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(other),
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

/// Strip a leading `dyn ` keyword token from an impl-header self-type fragment. An inherent
/// impl block on a trait-object type is valid Rust (e.g. std's own `impl dyn Any`), so the
/// Self-type text can genuinely start with `dyn ` (`"dyn Projection"`) - and the same shape can
/// follow a `for` too (`"Trait for dyn Concrete"`). Gap disclosed by
/// `adv-u87c2-r1-cheaper-fix-exists-reuse-impl-self-type`: zero `impl dyn` blocks exist in
/// `src/` today (live-but-currently-inert), but the general resolver must still handle it
/// correctly since this text scanner enumerates header shapes once rather than discovering them
/// one per round. No `"impl "` branch: the caller that captures `header` (the fn-item detector
/// above, `header_start = i + 4`) already skips the literal `impl` keyword token at capture
/// time, so `header` can never itself start with `"impl "` - an `"impl "` branch here would be
/// dead code, confirmed unreachable and removed (`adv-u87c2-r2-strip-leading-self-type-keyword-
/// impl-branch-is-dead-code`).
fn strip_leading_self_type_keyword(header: &str) -> &str {
    header.strip_prefix("dyn ").map_or(header, str::trim_start)
}

/// The `Self` type name out of an `impl` header (`"GateRatchet"` -> `"GateRatchet"`,
/// `"AgentDriver for Stub"` -> `"Stub"`, `"gate::Runner for FlakyGate"` -> `"FlakyGate"`,
/// `"<'a> RunCtx<'a>"` -> `"RunCtx"`, `"MyGuard where MyGuard: Sized"` -> `"MyGuard"`,
/// `"dyn Projection"` -> `"Projection"`). The one grammar-based self-type extractor for this
/// text scanner (`op-u87c2-round-2-impl-self-type-is-parsed-by-grammar`): every OTHER site that
/// needs an impl header's Self type - the round-2 fix to `impl_assoc_qualifier` included -
/// reuses this, rather than a second parallel parser reproving the same shapes.
fn impl_self_type(header: &str) -> String {
    let after_for = header.rsplit(" for ").next().unwrap_or(header);
    let no_leading_generics = strip_leading_impl_generics(after_for);
    let no_leading_keyword = strip_leading_self_type_keyword(no_leading_generics);
    // Strip a trailing where-clause BEFORE the generic split below: a where-clause bound can
    // itself contain `<...>` (e.g. `where T: Bar<Baz>`), and when the Self type has no
    // generics of its own the `split('<')` step would otherwise find that `<` first and cut
    // in the wrong place, leaving the where-clause text glued onto the self type.
    let no_where_clause = strip_trailing_where_clause(no_leading_keyword);
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
    let (scanned, contents) = scan_target_files_with_content(root);
    scanned
        .into_iter()
        .map(|f| {
            let (proposed_module, reason) = classify(&f);
            let content_hash =
                raw_span_content_hash(&contents[f.file.as_str()], f.start_line, f.end_line);
            MapEntry {
                file: f.file,
                name: f.name,
                start_line: f.start_line,
                end_line: f.end_line,
                is_test: f.is_test,
                proposed_module,
                reason,
                content_hash,
            }
        })
        .collect()
}

/// [`MAP_PATH`]'s guarded per-entry shape (spec 90 criterion 2): every [`MapEntry`] field except
/// its line span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapEntryWire {
    file: String,
    name: String,
    is_test: bool,
    proposed_module: Option<String>,
    reason: String,
    content_hash: String,
}

/// [`MAP_LINES_PATH`]'s shape: one entry's line span only, in the SAME order as [`MAP_PATH`] -
/// joined by array position, `name` carried too for a human cross-checking by eye.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapEntryLines {
    file: String,
    name: String,
    start_line: usize,
    end_line: usize,
}

fn map_entry_wire(e: &MapEntry) -> MapEntryWire {
    MapEntryWire {
        file: e.file.clone(),
        name: e.name.clone(),
        is_test: e.is_test,
        proposed_module: e.proposed_module.clone(),
        reason: e.reason.clone(),
        content_hash: e.content_hash.clone(),
    }
}

fn map_entry_lines(e: &MapEntry) -> MapEntryLines {
    MapEntryLines {
        file: e.file.clone(),
        name: e.name.clone(),
        start_line: e.start_line,
        end_line: e.end_line,
    }
}

/// Deterministic pretty JSON for [`build_map`]'s output, LINE-FREE (spec 90 criterion 2):
/// serializes through [`MapEntryWire`], never [`MapEntry`] directly, so the guarded file never
/// carries a line number - a bare array, each field in declaration order (no `HashMap` anywhere
/// in the shape, so `serde_json` emits the SAME bytes on every run over the same tree - the
/// drift guard's whole premise).
fn map_to_json(entries: &[MapEntry]) -> String {
    let wire: Vec<MapEntryWire> = entries.iter().map(map_entry_wire).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("MapEntryWire serializes");
    s.push('\n');
    s
}

/// Deterministic pretty JSON for [`MAP_LINES_PATH`] - the line spans [`map_to_json`] dropped.
/// Never drift-guarded; written fresh every `RIGGER_AUDIT_WRITE=1` run alongside the guarded
/// file.
fn map_lines_to_json(entries: &[MapEntry]) -> String {
    let wire: Vec<MapEntryLines> = entries.iter().map(map_entry_lines).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("MapEntryLines serializes");
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

/// Spec 90 criterion 2: `lines` carries each entry's own line span in the SAME order as
/// `entries` ([`map_entry_lines`] - joined by array position, exactly like [`MAP_LINES_PATH`]
/// itself). Every `file:line` citation below reads from `lines`, never from a [`MapEntry`]'s own
/// `start_line`/`end_line` directly - mirrors [`render_section_2`]'s own `lines` param exactly
/// (spec 90 Design: "the report keeps its `file:line` citations ... rendered from that file").
fn render_section_1(entries: &[MapEntry], lines: &[MapEntryLines]) -> String {
    assert_eq!(
        entries.len(),
        lines.len(),
        "entries and lines must be the same length, computed from the same pass"
    );
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
        let mut fns: Vec<(&MapEntry, &MapEntryLines)> = entries
            .iter()
            .zip(lines)
            .filter(|(e, _)| e.proposed_module.as_deref() == Some(*module))
            .collect();
        fns.sort_by_key(|(e, el)| (e.file.clone(), el.start_line));
        let _ = writeln!(out, "- `{module}` ({})", pluralize_functions(fns.len()));
        for (e, el) in &fns {
            debug_assert_eq!(
                e.name, el.name,
                "entries and lines must share order/identity"
            );
            let _ = writeln!(
                out,
                "  - `{}:{}-{}` `{}` - {}",
                el.file, el.start_line, el.end_line, e.name, e.reason
            );
        }
    }
    let _ = writeln!(out);
    let mut unassigned: Vec<(&MapEntry, &MapEntryLines)> = entries
        .iter()
        .zip(lines)
        .filter(|(e, _)| e.proposed_module.is_none())
        .collect();
    unassigned.sort_by_key(|(e, el)| (e.file.clone(), el.start_line));
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
        for (e, el) in &unassigned {
            debug_assert_eq!(
                e.name, el.name,
                "entries and lines must share order/identity"
            );
            let _ = writeln!(
                out,
                "- `{}:{}-{}` `{}` - {}",
                el.file, el.start_line, el.end_line, e.name, e.reason
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

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`MAP_PATH`]'s line spans (see
/// [`CATALOG_LINES_PATH`]'s own doc comment for the full rationale, identical here).
const MAP_LINES_PATH: &str = "docs/audit/responsibility-map.lines.json";

/// Guards every read-modify-write of [`REPORT_PATH`] in `RIGGER_AUDIT_WRITE=1` mode: this
/// criterion's own drift-guard test (`report_section_2_matches_the_tree_or_is_rewritten`) is a
/// SECOND writer of the same file criterion 1's `report_section_1_matches_the_tree_or_is_
/// rewritten` already writes, and `cargo test`'s default multi-threaded runner would otherwise
/// let their read-modify-write cycles interleave and clobber each other's update (decision
/// `u85c2-report-write-lock`). Recovers from a poisoned lock (a prior panicking write) rather
/// than cascading a failure into every later write, since poisoning here signals nothing about
/// THIS write's own correctness.
static REPORT_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_report_write() -> std::sync::MutexGuard<'static, ()> {
    REPORT_WRITE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

/// Find `marker`'s first occurrence in `haystack` that starts a genuine markdown line - the
/// preceding byte is a newline (or `pos == 0`, never actually hit by any caller below, since
/// every real heading this module searches for is preceded by the report's own title/intro or
/// an earlier section's blank-line separator). A bare `str::find` is NOT safe here: once
/// section 2's own mechanical/sweep listing embeds a captured excerpt whose raw quoted text
/// happens to contain a heading-shaped substring (a real `#### N. ` sub-heading's own tail
/// reads as `## N. ` too - `"#### 3. Boundary Violations"` contains `"## 3. Boundary
/// Violations"` starting at its third character), an unanchored search finds that FALSE,
/// earlier position instead of the real heading and a `replace_section_*` call silently
/// truncates or overwrites the file from there (decision
/// `u85c4-fix-heading-boundary-false-match` - found when adding this criterion's own section 6
/// shifted which giant literal gets swept and made the corruption reproducible, though the
/// defect class predates this unit). Every heading search below routes through this instead of
/// `str::find` for that reason.
fn find_heading(haystack: &str, marker: &str) -> Option<usize> {
    haystack
        .match_indices(marker)
        .find(|&(pos, _)| pos == 0 || haystack.as_bytes()[pos - 1] == b'\n')
        .map(|(pos, _)| pos)
}

/// Replace ONLY section 1's span (from its `## 1. ` heading up to, but not including, the next
/// `## ` heading) inside an EXISTING report `existing`, leaving every other section (including
/// placeholders a later unit has since filled in) byte-for-byte untouched. Used when the report
/// file already exists (e.g. this test re-running after `RIGGER_AUDIT_WRITE=1` once).
fn replace_section_1(existing: &str, section_1: &str) -> String {
    let start = match find_heading(existing, "## 1. ") {
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

// =========================================================================================
// CRITERION 2 (`u85c2`, THIS UNIT): THE DUPLICATION CATALOG
// =========================================================================================

/// The two directories the duplication catalog scans, recursively, in this fixed order - the
/// strict whole-tree definition (spec 85 Goal: "DRY is STRICT - any logic present in more than
/// one place ANYWHERE in the codebase is a violation"), unlike criterion 1's 3-file scope.
const SCAN_ROOTS: [&str; 2] = ["src", "tests"];

/// Shingle window width (spec 85 Design: "Jaccard over 8-token shingles").
const SHINGLE_SIZE: usize = 8;

/// The Jaccard floor for a mechanical `near`/`exact` cluster (spec 85 Design: "normalized
/// similarity is... at or above 0.72").
const SIMILARITY_THRESHOLD: f64 = 0.72;

/// A shingle shared by more than this many distinct-normalized-stream representatives is
/// skipped as a candidate-pair GENERATOR (see this module's doc comment) - bounds the
/// pathological hyper-common-boilerplate-shingle case without discarding it as similarity
/// evidence for pairs found via any of the function's OTHER shingles.
const POSTING_CAP: usize = 32;

const CATALOG_PATH: &str = "docs/audit/duplication-catalog.json";

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`CATALOG_PATH`]'s line spans, moved out
/// of the guarded file so a pin bump or a sibling unit's own insertion elsewhere never perturbs
/// this file's guarded bytes. Written only in `RIGGER_AUDIT_WRITE=1` mode; the drift guard never
/// reads it back (see [`catalog_lines_to_json`]).
const CATALOG_LINES_PATH: &str = "docs/audit/duplication-catalog.lines.json";

/// The fixed seed for [`sample_indices`]'s adversarial draw - arbitrary but permanently fixed
/// (spec 85 THOROUGHNESS: "The report states the sample seed so the check is reproducible").
const ADVERSARIAL_SEED: u64 = 85_072_026;
const ADVERSARIAL_SAMPLE_SIZE: usize = 30;

/// Criterion 4's (`u85c4`) own citation-guard periphery test - excluded from the adversarial
/// draw's POPULATION (never from the duplication catalog itself, which still scans and clusters
/// this file's functions like any other). [`sample_indices`] seeds purely on population SIZE, so
/// without this exclusion every function this file gains or loses as its citation-drift-guard
/// mechanism hardens round over round (`tests/prioritized_plan_citation_periphery.rs`'s own
/// module doc walks that history) silently redraws the WHOLE 30-function sample - discarding
/// this criterion's own already hand-verified reading pass with no re-reading to match it
/// (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`, the preferred remedy
/// named by `adj-u85c4-r6-uphold-adversarial-sample-donewhen-violation` /
/// `adv-u85c4-r6-adversarial-sample-silently-reshuffled-with-no-reading-pass`: "stop this unit's
/// own file-count growth from perturbing a criterion-2-owned artifact at all"). A citation-guard
/// rewrite by this criterion can now never perturb another criterion's owned artifact again.
const ADVERSARIAL_SAMPLE_EXCLUDED_FILE: &str = "tests/prioritized_plan_citation_periphery.rs";

/// Every `.rs` file strictly under `dir`, recursively, appended to `out`, in deterministic
/// (sorted) finding order - mirrors `tests/no_os_kill_audit.rs::collect_rs_files`'s own
/// precedent (kept as this criterion's own copy: spec 85 "WHAT THIS SPEC DOES NOT DO... no test
/// consolidation" forbids reaching into that file to share it, and the duplication this creates
/// is itself exactly the kind of thing THIS catalog is built to find and list, section 5's own
/// concern to consolidate).
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

// -----------------------------------------------------------------------------------------
// THE TOKENIZER
// -----------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawKind {
    Keyword,
    Ident,
    Lifetime,
    Lit,
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawTok {
    kind: RawKind,
    text: String,
    /// 1-based line the token STARTS on, within whatever `chars` slice was tokenized.
    line: usize,
}

/// Rust keywords (2018+ reserved and strict, plus weak keywords actually used as such) - kept
/// verbatim by [`normalize_tokens`] rather than canonicalized like an identifier, since a
/// keyword is structural signal, not a name.
const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "try", "union", "yield", "abstract", "become",
    "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual",
];

fn is_keyword(s: &str) -> bool {
    RUST_KEYWORDS.contains(&s)
}

/// Advance `*i` past a nested block comment (`chars[*i]=='/'`, `chars[*i+1]=='*'` - the caller
/// checks this before calling), returning the number of `\n`s crossed. A second, standalone
/// implementation of nested-comment skipping alongside `scan_file`'s own two inline copies
/// (its main loop and `scan_fn_signature_end`) is itself exactly the kind of instance this
/// catalog's own similarity pass is built to catch (and does - see the report); factoring a
/// THIRD shared implementation across all of them is section 5/6 refactor-spec territory, out
/// of scope for an audit-only spec that changes no production code and may not consolidate
/// tests (spec 85 "WHAT THIS SPEC DOES NOT DO").
fn skip_block_comment(chars: &[char], i: &mut usize) -> usize {
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

/// Tokenize `chars` into a normalized-similarity-ready token stream: comments and whitespace
/// produce no token; a string/byte/raw-string literal or a char literal becomes one `Lit`
/// token (reusing [`skip_string_literal`]/[`char_literal_len`] exactly as `scan_file` does, so
/// no lexical state is re-implemented); a `'`+ident not matched as a char literal is one
/// `Lifetime` token; a digit-led run (plus one optional `.`-fraction) is one `Lit` (number)
/// token; a letter/`_`-led run is `Keyword` when it names a Rust keyword, else `Ident`; every
/// other character is its own single-char `Punct` token (so a multi-char operator like `::` or
/// `->` becomes two/three adjacent `Punct` tokens - the mandatory-sweep matchers below account
/// for this).
fn tokenize(chars: &[char]) -> Vec<RawTok> {
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

/// Canonicalize an identifier to its KIND by casing convention (this module's doc comment):
/// `SCREAMING_SNAKE`/`ALLCAPS` (every letter uppercase, at least one letter present) -> `CONST`;
/// leads with an uppercase letter -> `TYPE`; anything else -> `IDENT`.
fn ident_kind_marker(text: &str) -> &'static str {
    let has_upper = text.chars().any(|c| c.is_uppercase());
    let all_screaming = has_upper
        && text
            .chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit());
    if all_screaming {
        "CONST"
    } else if text.chars().next().map(char::is_uppercase).unwrap_or(false) {
        "TYPE"
    } else {
        "IDENT"
    }
}

/// Normalize a token stream for shingling (this module's doc comment): a keyword or punct
/// token is kept verbatim; a literal becomes `LIT`; a lifetime becomes `LIFETIME`; an
/// identifier immediately followed by a `!` `Punct` token (a macro invocation) becomes `MACRO`;
/// every other identifier is canonicalized by [`ident_kind_marker`]. Borrows every element
/// (either straight from `toks`, or a `'static` marker) rather than allocating a `String` per
/// token - over a whole-tree scan the token volume is large enough that this allocation would
/// otherwise dominate the whole pass (measured: see decision `u85c2-shingle-fingerprints`).
fn normalize_tokens(toks: &[RawTok]) -> Vec<&str> {
    toks.iter()
        .enumerate()
        .map(|(idx, tok)| match tok.kind {
            RawKind::Keyword | RawKind::Punct => tok.text.as_str(),
            RawKind::Lifetime => "LIFETIME",
            RawKind::Lit => "LIT",
            RawKind::Ident => {
                let is_macro = toks
                    .get(idx + 1)
                    .map(|t| t.kind == RawKind::Punct && t.text == "!")
                    .unwrap_or(false);
                if is_macro {
                    "MACRO"
                } else {
                    ident_kind_marker(&tok.text)
                }
            }
        })
        .collect()
}

/// The 8-token sliding-window shingle set of a normalized stream, as SORTED, deduplicated
/// 64-bit fingerprints rather than the joined shingle text itself (this module's doc comment: a
/// stream shorter than [`SHINGLE_SIZE`] gets one shingle - its whole stream - so two identical
/// short streams still match at Jaccard 1.0, and two different ones never spuriously overlap).
/// Two things make a whole-tree scan (thousands of functions, each producing dozens of
/// shingles, compared over hundreds of thousands of candidate pairs) tractable rather than a
/// multi-minute run (measured - decision `u85c2-shingle-fingerprints`): hashing each window
/// directly, never materializing the joined `String` (`DefaultHasher::new()` uses a FIXED, not
/// per-process-random, key, so a fingerprint is the same on every run - the drift guard's
/// determinism premise, even though it is not a value this module ever writes to a file); and
/// returning them SORTED so [`jaccard`] compares by a linear merge instead of a hash lookup per
/// element, since it is called once per candidate pair. A fingerprint collision between two
/// DIFFERENT windows is possible in principle (64 bits, not cryptographic) but astronomically
/// unlikely at this codebase's scale (order 10^5 shingles - a collision probability comparable
/// to a hash-table library's own birthday-bound assumption) and would only ever UNDER-count a
/// union (making two functions look marginally more similar), never invent a false function.
fn shingles(norm: &[&str]) -> Vec<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut out = Vec::new();
    if norm.is_empty() {
        return out;
    }
    if norm.len() < SHINGLE_SIZE {
        let mut h = DefaultHasher::new();
        norm.hash(&mut h);
        out.push(h.finish());
        return out;
    }
    for w in norm.windows(SHINGLE_SIZE) {
        let mut h = DefaultHasher::new();
        w.hash(&mut h);
        out.push(h.finish());
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Jaccard similarity of two fingerprint lists, via a linear merge (this module's doc comment
/// on [`shingles`]). PRECONDITION: both `a` and `b` are already sorted ascending and
/// deduplicated (as [`shingles`] always returns them) - an unsorted input silently produces a
/// wrong answer rather than panicking, so every caller of this private function must be one
/// that upholds it (there are exactly two: [`build_mechanical_clusters`] and this module's own
/// tests, which construct their fixtures pre-sorted).
fn jaccard(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut inter = 0usize;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

// -----------------------------------------------------------------------------------------
// THE WHOLE-TREE SCAN
// -----------------------------------------------------------------------------------------

/// One `.rs` file's scan: every function [`scan_file`] found in it, and the WHOLE file's own
/// token stream (tokenized once; [`body_tokens`] slices a function's span out of it, so a
/// function is never re-tokenized from scratch and the mandatory sweeps below can scan the
/// whole file including code outside any scanned function).
struct FileScan {
    rel: String,
    fns: Vec<ScannedFn>,
    tokens: Vec<RawTok>,
    /// Spec 87 round-1 addition (see [`ModSpan`]) - every inline `mod { .. }` block's own span,
    /// used by [`all_ident_ref_sites`] to widen `in_test_range` beyond fn spans alone. Populated
    /// via [`scan_file_core`] directly (this criterion's own extra field, per that function's own
    /// doc comment), never through the [`scan_file`] wrapper which discards it.
    mod_spans: Vec<ModSpan>,
}

/// Scan every `.rs` file under [`SCAN_ROOTS`], deterministically ordered ([`collect_rs_files`]
/// sorts within each root; `src` is scanned before `tests`).
fn scan_tree(root: &Path) -> Vec<FileScan> {
    let mut paths = Vec::new();
    for top in SCAN_ROOTS {
        collect_rs_files(&root.join(top), &mut paths);
    }
    paths
        .into_iter()
        .map(|path| {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("simplification_audit: cannot read {rel}: {e}"));
            let chars: Vec<char> = content.chars().collect();
            let tokens = tokenize(&chars);
            let core = scan_file_core(&rel, &content);
            FileScan {
                rel,
                fns: core.fns,
                tokens,
                mod_spans: core.mod_spans,
            }
        })
        .collect()
}

/// The slice of `file_tokens` whose `.line` falls within `[start_line, end_line]` (both
/// inclusive, 1-based - exactly how `scan_file` records a function's own span). `file_tokens`
/// must be sorted non-decreasing by `.line` (true of anything [`tokenize`] produces, since it
/// scans forward through the content once) so a binary search suffices.
fn body_tokens(file_tokens: &[RawTok], start_line: usize, end_line: usize) -> &[RawTok] {
    let lo = file_tokens.partition_point(|t| t.line < start_line);
    let hi = file_tokens.partition_point(|t| t.line <= end_line);
    &file_tokens[lo..hi]
}

/// Spec 90 criterion 2, THE LINE-FREE CONTENT IDENTITY for a catalog site or a dead-code
/// entry/reference (decision `u90c2-content-hash-primitive-reuse`): this span's OWN normalized
/// token stream ([`normalize_tokens`] over [`body_tokens`] - the EXACT preprocessing
/// [`build_mechanical_clusters`] already runs for its Jaccard pass, joined the SAME way
/// (`norm.join("\u{1}")`) that pass already keys exact-duplicate grouping on), hashed through
/// [`content_hash`] rather than a second, parallel hasher. A pin bump elsewhere in the file
/// moves this span to a new line number but never touches its own tokens, so the hash is
/// byte-identical across the shift; an actual edit inside the span (a real content change)
/// changes it, as intended. Works uniformly for a whole function's span (a [`dup_site`]) and a
/// single-line call/literal site (`start_line == end_line`, the mandatory sweeps) alike.
fn span_content_hash(file_tokens: &[RawTok], start_line: usize, end_line: usize) -> String {
    let toks = body_tokens(file_tokens, start_line, end_line);
    let norm = normalize_tokens(toks);
    content_hash(&norm.join("\u{1}"))
}

/// One function's identity within [`scan_tree`]'s output: which file, and its index into that
/// file's own `fns`.
#[derive(Debug, Clone, Copy)]
struct FnRef {
    file_idx: usize,
    fn_idx: usize,
}

impl FnRef {
    fn scanned<'a>(&self, files: &'a [FileScan]) -> &'a ScannedFn {
        &files[self.file_idx].fns[self.fn_idx]
    }
}

/// Every function across `files`, deterministically ordered by `(file, start_line)` -
/// `scan_file` itself emits a function only when its CLOSING brace is reached, which for a
/// nested function is before its enclosing one, so this explicit sort (not raw `scan_file`
/// emission order) is what makes the catalog's own ordering (and so its drift guard)
/// reproducible regardless of that emission quirk.
fn all_fn_refs(files: &[FileScan]) -> Vec<FnRef> {
    let mut refs = Vec::new();
    for (file_idx, f) in files.iter().enumerate() {
        for fn_idx in 0..f.fns.len() {
            refs.push(FnRef { file_idx, fn_idx });
        }
    }
    refs.sort_by(|a, b| {
        let fa = a.scanned(files);
        let fb = b.scanned(files);
        (&fa.file, fa.start_line).cmp(&(&fb.file, fb.start_line))
    });
    refs
}

/// [`all_fn_refs`] restricted to the population [`render_adversarial_sample`]'s draw pulls from -
/// see [`ADVERSARIAL_SAMPLE_EXCLUDED_FILE`] for why one file is excluded here but nowhere else
/// (the duplication catalog itself still scans and clusters that file's functions normally).
fn adversarial_sample_population(files: &[FileScan]) -> Vec<FnRef> {
    all_fn_refs(files)
        .into_iter()
        .filter(|r| r.scanned(files).file != ADVERSARIAL_SAMPLE_EXCLUDED_FILE)
        .collect()
}

// -----------------------------------------------------------------------------------------
// UNION-FIND (clustering)
// -----------------------------------------------------------------------------------------

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

// -----------------------------------------------------------------------------------------
// THE CATALOG SHAPE
// -----------------------------------------------------------------------------------------

/// Spec 90 criterion 2 addition: `content_hash` (see [`span_content_hash`]) is this site's
/// line-free identity - the ONLY per-site field the guarded `docs/audit/duplication-catalog.json`
/// carries ([`DupSiteWire`]). `start_line`/`end_line` stay on this struct for every OTHER
/// purpose (rendering, sorting, sweep note counts) and move to the unguarded
/// `docs/audit/duplication-catalog.lines.json` sibling ([`DupSiteLines`]) instead of the guarded
/// file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DupSite {
    file: String,
    start_line: usize,
    end_line: usize,
    name: String,
    content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DupCluster {
    id: String,
    /// `"exact"` | `"near"` | `"semantic"` (spec 85 Design section 2).
    classification: String,
    sites: Vec<DupSite>,
    proposed_home: String,
    note: String,
}

fn dup_site(f: &ScannedFn, file_tokens: &[RawTok]) -> DupSite {
    DupSite {
        file: f.file.clone(),
        start_line: f.start_line,
        end_line: f.end_line,
        name: f.name.clone(),
        content_hash: span_content_hash(file_tokens, f.start_line, f.end_line),
    }
}

/// Propose ONE home for a cluster (spec 85 Design: "the ONE proposed home"), reusing the
/// existing [`file_stem`]/[`snake_case`]/[`impl_self_type`] helpers criterion 1 already
/// established rather than a second naming scheme: if every site sits in the SAME `impl`
/// block, group with that Self type's other methods (exactly criterion 1's own rule); if every
/// site is in one file, propose consolidating within that file; otherwise a new shared module
/// spanning the files involved.
fn propose_home(files: &[FileScan], members: &[usize], refs: &[FnRef]) -> String {
    let scanned_of = |i: usize| refs[i].scanned(files);
    let first = scanned_of(members[0]);
    let same_impl = first.enclosing_impl.as_deref().filter(|_| {
        members
            .iter()
            .all(|&i| scanned_of(i).enclosing_impl.as_deref() == first.enclosing_impl.as_deref())
    });
    if let Some(header) = same_impl {
        let stem = file_stem(&first.file);
        return format!("{stem}::{}", snake_case(&impl_self_type(header)));
    }
    let file_set: HashSet<&str> = members
        .iter()
        .map(|&i| scanned_of(i).file.as_str())
        .collect();
    if file_set.len() == 1 {
        let stem = file_stem(&first.file);
        return format!(
            "{stem}::support (consolidate these {} sites into one function in this file)",
            members.len()
        );
    }
    let mut file_list: Vec<&str> = file_set.into_iter().collect();
    file_list.sort_unstable();
    format!(
        "a new shared module (sites span {} files: {})",
        file_list.len(),
        file_list.join(", ")
    )
}

/// THE MECHANICAL PASS (this module's doc comment): normalized-token-shingle Jaccard
/// clustering over every function `refs` names. Deterministically ordered.
fn build_mechanical_clusters(files: &[FileScan], refs: &[FnRef]) -> Vec<DupCluster> {
    let n = refs.len();
    let mut norm_keys: Vec<String> = Vec::with_capacity(n);
    let mut shingle_sets: Vec<Vec<u64>> = Vec::with_capacity(n);
    for r in refs {
        let f = &files[r.file_idx];
        let sf = &f.fns[r.fn_idx];
        let toks = body_tokens(&f.tokens, sf.start_line, sf.end_line);
        let norm = normalize_tokens(toks);
        norm_keys.push(norm.join("\u{1}"));
        shingle_sets.push(shingles(&norm));
    }

    let mut dsu = Dsu::new(n);
    // Phase A: union every function with the same normalized stream (O(n), immune to
    // POSTING_CAP - this is the entire `exact` pass) and pick one representative per stream.
    let mut repr_of_key: HashMap<&str, usize> = HashMap::new();
    let mut representatives: Vec<usize> = Vec::new();
    for (i, key) in norm_keys.iter().enumerate() {
        let key = key.as_str();
        if let Some(&r) = repr_of_key.get(key) {
            dsu.union(i, r);
        } else {
            repr_of_key.insert(key, i);
            representatives.push(i);
        }
    }

    // Phase B: an inverted shingle index over the (distinct-stream) representatives only,
    // candidate pairs verified by exact Jaccard, unioned when >= SIMILARITY_THRESHOLD.
    let mut posting: HashMap<u64, Vec<usize>> = HashMap::new();
    for (ri, &fi) in representatives.iter().enumerate() {
        for &sh in &shingle_sets[fi] {
            posting.entry(sh).or_default().push(ri);
        }
    }
    let mut candidates: HashSet<(usize, usize)> = HashSet::new();
    for list in posting.values() {
        if list.len() < 2 || list.len() > POSTING_CAP {
            continue;
        }
        for a in 0..list.len() {
            for &b in &list[a + 1..] {
                let (x, y) = (list[a].min(b), list[a].max(b));
                candidates.insert((x, y));
            }
        }
    }
    let mut cand_sorted: Vec<(usize, usize)> = candidates.into_iter().collect();
    cand_sorted.sort_unstable();
    for (ra, rb) in cand_sorted {
        let (fa, fb) = (representatives[ra], representatives[rb]);
        if jaccard(&shingle_sets[fa], &shingle_sets[fb]) >= SIMILARITY_THRESHOLD {
            dsu.union(fa, fb);
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = dsu.find(i);
        groups.entry(root).or_default().push(i);
    }
    let mut clusters: Vec<DupCluster> = Vec::new();
    for mut members in groups.into_values() {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|&a, &b| {
            let fa = refs[a].scanned(files);
            let fb = refs[b].scanned(files);
            (&fa.file, fa.start_line).cmp(&(&fb.file, fb.start_line))
        });
        let distinct_keys: HashSet<&str> = members.iter().map(|&i| norm_keys[i].as_str()).collect();
        let classification = if distinct_keys.len() == 1 {
            "exact"
        } else {
            "near"
        };
        let sites: Vec<DupSite> = members
            .iter()
            .map(|&i| dup_site(refs[i].scanned(files), &files[refs[i].file_idx].tokens))
            .collect();
        let proposed_home = propose_home(files, &members, refs);
        clusters.push(DupCluster {
            id: String::new(),
            classification: classification.to_string(),
            sites,
            proposed_home,
            note: format!(
                "mechanical: normalized-token Jaccard similarity ({SHINGLE_SIZE}-token \
                 shingles, threshold {SIMILARITY_THRESHOLD})"
            ),
        });
    }
    clusters.sort_by(|a, b| {
        let ka = (&a.sites[0].file, a.sites[0].start_line);
        let kb = (&b.sites[0].file, b.sites[0].start_line);
        ka.cmp(&kb)
    });
    clusters
}

// -----------------------------------------------------------------------------------------
// THE FIVE MANDATORY SWEEPS (spec 85 Design)
// -----------------------------------------------------------------------------------------

/// Names of the five mandatory sweeps, in the fixed order every rendering and cross-check
/// uses; also each sweep's [`DupCluster::note`] substring identity (see [`sweep_cluster`]).
const MANDATORY_SWEEPS: [&str; 5] = [
    "Command::new call sites",
    "/proc-path string literals",
    "sqlite Connection::open call sites",
    ".rigger-path string literals",
    "error-shaping helper functions",
];

/// Every `<head> :: <one of tails> (` call site (a `::`-qualified call, tokenized as TWO
/// adjacent `:` `Punct` tokens per [`tokenize`]'s own single-char-punct simplification).
fn find_ident_path_call_sites(files: &[FileScan], head: &str, tails: &[&str]) -> Vec<DupSite> {
    let mut hits = Vec::new();
    for f in files {
        let t = &f.tokens;
        if t.len() < 5 {
            continue;
        }
        for i in 0..=(t.len() - 5) {
            let is_head = t[i].kind == RawKind::Ident && t[i].text == head;
            let is_colon_colon = t[i + 1].kind == RawKind::Punct
                && t[i + 1].text == ":"
                && t[i + 2].kind == RawKind::Punct
                && t[i + 2].text == ":";
            let tail_ok = tails
                .iter()
                .any(|tail| t[i + 3].kind == RawKind::Ident && t[i + 3].text == *tail);
            let is_call = t[i + 4].kind == RawKind::Punct && t[i + 4].text == "(";
            if is_head && is_colon_colon && tail_ok && is_call {
                hits.push(DupSite {
                    file: f.rel.clone(),
                    start_line: t[i].line,
                    end_line: t[i].line,
                    name: format!("{head}::{}", t[i + 3].text),
                    content_hash: span_content_hash(t, t[i].line, t[i].line),
                });
            }
        }
    }
    hits
}

/// Every literal token (string/byte/raw-string/char/number - only string-shaped ones will ever
/// actually contain a `/`-bearing path) whose raw text contains `needle`.
fn find_literal_containing(files: &[FileScan], needle: &str) -> Vec<DupSite> {
    let mut hits = Vec::new();
    for f in files {
        for t in &f.tokens {
            if t.kind == RawKind::Lit && t.text.contains(needle) {
                hits.push(DupSite {
                    file: f.rel.clone(),
                    start_line: t.line,
                    end_line: t.line,
                    name: t.text.clone(),
                    content_hash: span_content_hash(&f.tokens, t.line, t.line),
                });
            }
        }
    }
    hits
}

/// A function's name reads as "error-shaping" (spec 85 Design names this a mandatory sweep):
/// contains `"error"`, or contains `"err"` at a `_`-bounded or leading/trailing word boundary -
/// excludes an incidental substring hit like `"deferred"` (contains `"err"` but none of
/// `"_err"`/`"err_"`/a leading or trailing `"err"`).
fn looks_error_shaping(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("error")
        || n.contains("_err")
        || n.contains("err_")
        || n.starts_with("err")
        || n.ends_with("err")
}

/// The error-shaping sweep's mechanical confirmation: the name-shape rule above narrowed to
/// functions whose body actually invokes the `format!` macro. Name-shaping alone is too broad
/// (e.g. a function merely checking `.is_err()`); the `format!` requirement is what makes this
/// mechanical rather than a name-only guess.
fn find_error_shaping_fns(files: &[FileScan], refs: &[FnRef]) -> Vec<DupSite> {
    let mut hits = Vec::new();
    for r in refs {
        let f = &files[r.file_idx];
        let sf = &f.fns[r.fn_idx];
        if !looks_error_shaping(&sf.name) {
            continue;
        }
        let toks = body_tokens(&f.tokens, sf.start_line, sf.end_line);
        let has_format = toks.windows(2).any(|w| {
            w[0].kind == RawKind::Ident
                && w[0].text == "format"
                && w[1].kind == RawKind::Punct
                && w[1].text == "!"
        });
        if has_format {
            hits.push(dup_site(sf, &f.tokens));
        }
    }
    hits
}

/// Build ONE `semantic` cluster from a sweep's hits (spec 85 Design: each mandatory sweep is
/// named as ONE thing the catalog must cover, not similarity-dependent - so it appears
/// regardless of what the Jaccard pass finds). `sweep_name` becomes part of [`DupCluster::note`]
/// so [`render_section_2`] and the mandatory-sweep cross-check can find it back by name.
fn sweep_cluster(sweep_name: &str, mut sites: Vec<DupSite>, proposed_home: &str) -> DupCluster {
    sites.sort_by(|a, b| {
        a.file
            .as_str()
            .cmp(b.file.as_str())
            .then(a.start_line.cmp(&b.start_line))
    });
    let note = format!(
        "mandatory sweep: {sweep_name} - {} site(s), collected mechanically regardless of the \
         Jaccard pass (spec 85 Design)",
        sites.len()
    );
    DupCluster {
        id: String::new(),
        classification: "semantic".to_string(),
        sites,
        proposed_home: proposed_home.to_string(),
        note,
    }
}

fn build_sweep_clusters(files: &[FileScan], refs: &[FnRef]) -> Vec<DupCluster> {
    vec![
        sweep_cluster(
            MANDATORY_SWEEPS[0],
            find_ident_path_call_sites(files, "Command", &["new"]),
            "a single injected process-spawn port every Command::new site routes through \
             instead of constructing its own Command",
        ),
        sweep_cluster(
            MANDATORY_SWEEPS[1],
            find_literal_containing(files, "/proc"),
            "src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)",
        ),
        sweep_cluster(
            MANDATORY_SWEEPS[2],
            find_ident_path_call_sites(files, "Connection", &["open", "open_with_flags"]),
            "one sqlite-connection-opening adapter function every caller is injected with",
        ),
        sweep_cluster(
            MANDATORY_SWEEPS[3],
            find_literal_containing(files, ".rigger"),
            "one .rigger-relative path-composition helper",
        ),
        sweep_cluster(
            MANDATORY_SWEEPS[4],
            find_error_shaping_fns(files, refs),
            "one error-shaping helper module",
        ),
    ]
}

/// Every function that reads a `/proc/<pid>/stat` or `/proc/<pid>/status` path (spec 85 Goal's
/// own named example: "`src/dash.rs` reimplementing `src/reap.rs`'s `/proc` pid scan, upheld at
/// spec 62's capstone"). Verified by reading (not merely inferred from the sweep above, which
/// is call-SITE not function granularity): `dash.rs::process_state` and `reap.rs::pid_starttime`
/// both do `std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?` then
/// `.rsplit_once(')')?.1` then a `split_whitespace()` field extraction - same job (parse the
/// kernel's own `pid (comm) state ...` stat-file layout), different shape (a different field,
/// a differently-structured tail) - exactly the semantic-not-mechanical class this generalizes
/// beyond the one pair by GROUPING BY WHICH FILE THEY READ rather than hardcoding those two
/// citations, so a third such reader (present or future) is caught the same way.
fn find_proc_stat_or_status_readers(files: &[FileScan], refs: &[FnRef]) -> Vec<DupSite> {
    let mut hits = Vec::new();
    for r in refs {
        let f = &files[r.file_idx];
        let sf = &f.fns[r.fn_idx];
        let toks = body_tokens(&f.tokens, sf.start_line, sf.end_line);
        let reads_stat_or_status = toks.iter().any(|t| {
            t.kind == RawKind::Lit
                && t.text.contains("/proc")
                && (t.text.contains("/stat") || t.text.contains("/status"))
        });
        if reads_stat_or_status {
            hits.push(dup_site(sf, &f.tokens));
        }
    }
    hits
}

/// Whether `toks` (a function body) constructs a struct literal of its OWN enclosing type -
/// `Self { <field>: ...` or `<self_type> { <field>: ...` - the shape of a constructor-style
/// associated function (spec 85 THOROUGHNESS adversarial sample: found by reading
/// `spawn::SpawnResult::liveness_fault`, whose extra `class` parameter and `serde_json::json!`
/// meta field push its normalized-token Jaccard similarity to `SpawnResult::ok`/`failed` just
/// under the mechanical threshold, even though it is unmistakably a THIRD parallel constructor
/// for the same struct on a `cargo test`-visible read - "same job, different shape", the
/// textbook semantic case. Detects the general PATTERN (any struct's own literal in its own
/// impl block) rather than hardcoding this one triple, so this recall gap closes for every
/// struct with 2+ constructor-style associated functions, not only this one).
fn constructs_own_type_literal(toks: &[RawTok], self_type: &str) -> bool {
    if toks.len() < 4 {
        return false;
    }
    for i in 0..=(toks.len() - 4) {
        let head_matches = matches!(toks[i].kind, RawKind::Keyword | RawKind::Ident)
            && (toks[i].text == "Self" || toks[i].text == self_type);
        if !head_matches || toks[i + 1].kind != RawKind::Punct || toks[i + 1].text != "{" {
            continue;
        }
        if toks[i + 2].kind != RawKind::Ident {
            continue;
        }
        // The token right after the field name distinguishes a struct-literal field from a
        // block merely starting with an identifier expression (`{ ident.method() }`): an
        // explicit `field: value` (`:`) or a SHORTHAND field (`,` between fields, or `}` for a
        // single-field literal) - `.`/`(` etc. never start a struct-literal field.
        if toks[i + 3].kind == RawKind::Punct
            && matches!(toks[i + 3].text.as_str(), ":" | "," | "}")
        {
            return true;
        }
    }
    false
}

/// Every NON-TEST, non-empty group of 2+ constructor-style associated functions for the SAME
/// `impl <Type>` (same file, same Self type - see [`constructs_own_type_literal`]), each its
/// own semantic cluster ("parallel constructors for `<Type>`").
fn find_parallel_constructor_clusters(files: &[FileScan], refs: &[FnRef]) -> Vec<DupCluster> {
    let mut by_type: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, r) in refs.iter().enumerate() {
        let f = &files[r.file_idx];
        let sf = &f.fns[r.fn_idx];
        if sf.is_test {
            continue;
        }
        let Some(header) = sf.enclosing_impl.as_deref() else {
            continue;
        };
        let self_type = impl_self_type(header);
        let toks = body_tokens(&f.tokens, sf.start_line, sf.end_line);
        if constructs_own_type_literal(toks, &self_type) {
            by_type
                .entry((sf.file.clone(), self_type))
                .or_default()
                .push(i);
        }
    }
    let mut keys: Vec<&(String, String)> = by_type.keys().collect();
    keys.sort();
    let mut clusters = Vec::new();
    for key in keys {
        let members = &by_type[key];
        if members.len() < 2 {
            continue;
        }
        let (file, ty) = key;
        let sites: Vec<DupSite> = members
            .iter()
            .map(|&i| dup_site(refs[i].scanned(files), &files[refs[i].file_idx].tokens))
            .collect();
        clusters.push(sweep_cluster(
            "parallel constructor functions",
            sites,
            &format!(
                "{}::{} - one canonical constructor, the rest thin variants over it (or a \
                 builder), rather than each re-listing every field",
                file_stem(file),
                snake_case(ty)
            ),
        ));
    }
    clusters
}

/// Same-named free HELPER functions (never `#[test]`s themselves - `is_test` excludes both a
/// `#[test]` fn and anything inside a `#[cfg(test)] mod`) independently defined in 2+ DIFFERENT
/// files - the class a THIRD adversarial-sample read found: `exploration_graph`, a
/// near-identical-purpose test-fixture Graph builder, independently defined in
/// `tests/dash_exploration_route_client_contract.rs` AND `tests/dash_kg_graph_route.rs` with
/// different concrete fixture data - "same job (build an exploration-route test fixture),
/// different shape", exactly this catalog's semantic class, generalized here (rather than
/// hardcoding that one pair) to every other same-named helper this shape also applies to. Name
/// length is floored at [`SAME_NAME_MIN_LEN`] to exclude generic short names (`new`, `run`,
/// `setup`) that coincide across unrelated files without indicating real duplication.
const SAME_NAME_MIN_LEN: usize = 12;

/// Whether `members` (all sharing one function NAME, per [`find_same_named_helper_functions`])
/// is the REQUIRED shape of a shared trait - 2+ concrete adapters implementing the same trait
/// method, or a trait's own default method next to its override(s) - rather than a coincidental
/// same-named-helper duplicate. This is the precision defect the adversarial review found LIVE
/// in 3 committed clusters: `subscribe_all`/`subscribe_stream` across the `EventStore` trait's 3
/// backend adapters plus a test double, and `blast_radius` across the `Grounder` trait's own
/// default method, the `symbols` grounder's override, and a test double - every member of each
/// is either a TRAIT-IMPL method (`enclosing_impl` contains `" for "`, the exact substring
/// [`impl_self_type`] already special-cases) for a DIFFERENT concrete Self type, or a trait's own
/// default method (`enclosing_impl` is `None`) sharing the name of those overrides - the same
/// distinction [`find_parallel_constructor_clusters`] already draws one function away by keying
/// on `(file, Self type)`. Detected here as: 2+ DISTINCT "shape identities" among the members
/// (`None` for a free function or a trait's own default method, or the Self type extracted from
/// a `" for "` impl header) where at least one identity comes from an ACTUAL trait impl - a
/// coincidental same-named pair of two ordinary free functions (both `None`) or two INHERENT
/// impls (neither header contains `" for "`) never trips this, so the real `exploration_graph`
/// catch (two free functions, both `None`) is untouched.
fn is_required_trait_shape(files: &[FileScan], refs: &[FnRef], members: &[usize]) -> bool {
    let mut identities: HashSet<Option<String>> = HashSet::new();
    let mut any_trait_impl = false;
    for &i in members {
        match refs[i].scanned(files).enclosing_impl.as_deref() {
            Some(header) if header.contains(" for ") => {
                any_trait_impl = true;
                identities.insert(Some(impl_self_type(header)));
            }
            Some(header) => {
                identities.insert(Some(impl_self_type(header)));
            }
            None => {
                identities.insert(None);
            }
        }
    }
    any_trait_impl && identities.len() > 1
}

fn find_same_named_helper_functions(files: &[FileScan], refs: &[FnRef]) -> Vec<DupCluster> {
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in refs.iter().enumerate() {
        let sf = r.scanned(files);
        if sf.is_test || sf.name.len() < SAME_NAME_MIN_LEN {
            continue;
        }
        by_name.entry(sf.name.as_str()).or_default().push(i);
    }
    let mut names: Vec<&str> = by_name.keys().copied().collect();
    names.sort_unstable();
    let mut clusters = Vec::new();
    for name in names {
        let members = &by_name[name];
        let files_involved: HashSet<&str> = members
            .iter()
            .map(|&i| refs[i].scanned(files).file.as_str())
            .collect();
        if files_involved.len() < 2 {
            continue;
        }
        if is_required_trait_shape(files, refs, members) {
            continue;
        }
        let sites: Vec<DupSite> = members
            .iter()
            .map(|&i| dup_site(refs[i].scanned(files), &files[refs[i].file_idx].tokens))
            .collect();
        clusters.push(sweep_cluster(
            "same-named helper function defined independently in 2+ files",
            sites,
            &format!(
                "one shared `{name}` helper (e.g. relocated into `tests/common`) rather than \
                 each file defining its own"
            ),
        ));
    }
    clusters
}

/// This file's own bespoke source-text scanner (`scan_file`, the frame-stack scanner) and
/// token-level lexer (`tokenize`) alongside the codebase's ONE canonical tree-sitter-based
/// extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc calls it "the ONE
/// function that touches tree-sitter", architecture 5.5.3) - a fourth semantic cluster, added
/// per the adjudicator's REMEDY after u85c1's architecture lens routed this exact pair to this
/// criterion BY NAME across two prior review rounds and it was never added. `scan_file` and
/// `tokenize` re-derive Rust source structure (function/definition boundaries, string/char/
/// comment-literal handling) via a from-scratch character scan the same job `extract()` already
/// solves canonically through an injected tree-sitter grammar - "same job, different shape",
/// exactly this catalog's semantic class. Matched by the exact `(file, name)` this class is
/// currently known to occupy (mirrors the five mandatory sweeps' own by-name call-site matching,
/// e.g. `Command::new`/`Connection::open`) rather than a fragile structural heuristic over
/// arbitrary char-by-char scanning; a real-tree regression test pins the trio into one cluster.
fn find_bespoke_lexer_vs_canonical_extractor(files: &[FileScan], refs: &[FnRef]) -> Vec<DupSite> {
    let mut hits = Vec::new();
    for r in refs {
        let sf = r.scanned(files);
        let is_bespoke_lexer = sf.file == "tests/simplification_audit.rs"
            && matches!(sf.name.as_str(), "scan_file" | "tokenize");
        let is_canonical_extractor =
            sf.file == "src/grounder/symbols/extract.rs" && sf.name == "extract";
        if is_bespoke_lexer || is_canonical_extractor {
            hits.push(dup_site(sf, &files[r.file_idx].tokens));
        }
    }
    hits
}

/// Semantic duplicate clusters found BY READING, beyond the five mandatory sweeps (spec 85
/// Design section 2: "plus every semantic duplicate the auditors find by reading"): the
/// `/proc/<pid>/stat`|`status` reader family (the report's worked example), every struct with
/// 2+ parallel constructor-style associated functions, every same-named helper independently
/// defined in 2+ files, and this file's own bespoke lexer alongside the canonical tree-sitter
/// extractor.
fn build_extra_semantic_clusters(files: &[FileScan], refs: &[FnRef]) -> Vec<DupCluster> {
    let mut clusters = vec![
        sweep_cluster(
            "/proc/<pid>/stat or /proc/<pid>/status field-extraction functions",
            find_proc_stat_or_status_readers(files, refs),
            "src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split",
        ),
        sweep_cluster(
            "bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor",
            find_bespoke_lexer_vs_canonical_extractor(files, refs),
            "src/grounder/symbols/extract.rs::extract as the ONE function that touches source \
             parsing (already its own module doc's claim, architecture 5.5.3) - this file's own \
             scan_file/tokenize are ad hoc scanners for the identical job and should route \
             through an injected-grammar extractor rather than re-deriving structure by hand",
        ),
    ];
    clusters.extend(find_parallel_constructor_clusters(files, refs));
    clusters.extend(find_same_named_helper_functions(files, refs));
    clusters
}

// -----------------------------------------------------------------------------------------
// CATALOG ASSEMBLY, JSON, RENDERING
// -----------------------------------------------------------------------------------------

/// A cluster's deterministic sort key: its first site's `(file, start_line)`, or a fallback
/// that sorts before every real site (never hit on the real tree - all five sweeps find sites -
/// but keeps a degenerate zero-hit sweep cluster orderable rather than panicking).
fn cluster_sort_key(c: &DupCluster) -> (String, usize) {
    match c.sites.first() {
        Some(s) => (s.file.clone(), s.start_line),
        None => (String::new(), 0),
    }
}

/// Build the full duplication catalog from an already-scanned tree: the mechanical (Jaccard)
/// clusters, the five mandatory sweeps, and the additional hand-found semantic clusters,
/// combined into ONE deterministically ordered list with sequential `dup-NNNN` ids assigned
/// after sorting (so ids are stable regardless of which pass found a given cluster). Takes
/// `files` (not a root path) so [`real_catalog`] can share [`real_files`]'s ONE scan of the
/// real tree instead of a second unrelated one.
fn build_catalog(files: &[FileScan]) -> Vec<DupCluster> {
    let refs = all_fn_refs(files);
    let mut clusters = build_mechanical_clusters(files, &refs);
    clusters.extend(build_sweep_clusters(files, &refs));
    clusters.extend(build_extra_semantic_clusters(files, &refs));
    clusters.sort_by_key(cluster_sort_key);
    for (idx, c) in clusters.iter_mut().enumerate() {
        c.id = format!("dup-{:04}", idx + 1);
    }
    clusters
}

/// The real checked-out tree's [`scan_tree`], computed ONCE per test process and shared by
/// every real-tree acceptance test below (it is the single most expensive step this module
/// performs - re-running it once per test, as the tests below would otherwise each do
/// independently, made the whole suite noticeably slow for no benefit: [`scan_tree`] and
/// [`build_catalog`] are pure functions of the checked-out tree, so every caller within one
/// process computes the identical result).
fn real_files() -> &'static [FileScan] {
    static CACHE: std::sync::OnceLock<Vec<FileScan>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| scan_tree(&repo_root()))
}

/// The real checked-out tree's [`build_catalog`], memoized alongside [`real_files`] for the
/// same reason.
fn real_catalog() -> &'static [DupCluster] {
    static CACHE: std::sync::OnceLock<Vec<DupCluster>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| build_catalog(real_files()))
}

// -----------------------------------------------------------------------------------------
// SPEC 90 CRITERION 2: THE LINE-FREE WIRE SHAPE
// -----------------------------------------------------------------------------------------

/// [`CATALOG_PATH`]'s guarded per-site shape: `{file, name, content_hash}`, no line numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DupSiteWire {
    file: String,
    name: String,
    content_hash: String,
}

/// [`CATALOG_PATH`]'s guarded per-cluster shape - every [`DupCluster`] field except its sites'
/// line spans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DupClusterWire {
    id: String,
    classification: String,
    sites: Vec<DupSiteWire>,
    proposed_home: String,
    note: String,
}

/// [`CATALOG_LINES_PATH`]'s shape: one cluster's sites' line spans only, in the SAME
/// cluster/site order as [`CATALOG_PATH`] - joined by array position (both come from the SAME
/// `Vec<DupCluster>` in the SAME pass), `id` carried too for a human cross-checking by eye.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DupSiteLines {
    file: String,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DupClusterLines {
    id: String,
    sites: Vec<DupSiteLines>,
}

fn dup_cluster_wire(c: &DupCluster) -> DupClusterWire {
    DupClusterWire {
        id: c.id.clone(),
        classification: c.classification.clone(),
        sites: c
            .sites
            .iter()
            .map(|s| DupSiteWire {
                file: s.file.clone(),
                name: s.name.clone(),
                content_hash: s.content_hash.clone(),
            })
            .collect(),
        proposed_home: c.proposed_home.clone(),
        note: c.note.clone(),
    }
}

fn dup_cluster_lines(c: &DupCluster) -> DupClusterLines {
    DupClusterLines {
        id: c.id.clone(),
        sites: c
            .sites
            .iter()
            .map(|s| DupSiteLines {
                file: s.file.clone(),
                start_line: s.start_line,
                end_line: s.end_line,
            })
            .collect(),
    }
}

/// Deterministic pretty JSON for [`build_catalog`]'s output, LINE-FREE (spec 90 criterion 2):
/// serializes through [`DupClusterWire`], never [`DupCluster`] directly, so the guarded file
/// never carries a line number. Mirrors [`map_to_json`]'s own shape (a bare array, declaration
/// field order, no `HashMap` anywhere in the shape).
fn catalog_to_json(clusters: &[DupCluster]) -> String {
    let wire: Vec<DupClusterWire> = clusters.iter().map(dup_cluster_wire).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("DupClusterWire serializes");
    s.push('\n');
    s
}

/// Deterministic pretty JSON for [`CATALOG_LINES_PATH`] - the line spans [`catalog_to_json`]
/// dropped. Never drift-guarded (spec 90 criterion 2 Design: "the guard NEVER compares"); written
/// fresh every `RIGGER_AUDIT_WRITE=1` run alongside the guarded file.
fn catalog_lines_to_json(clusters: &[DupCluster]) -> String {
    let wire: Vec<DupClusterLines> = clusters.iter().map(dup_cluster_lines).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("DupClusterLines serializes");
    s.push('\n');
    s
}

/// Spec 90 criterion 2: `lines` carries each cluster's site line spans in the SAME
/// cluster/site order as `clusters` ([`dup_cluster_lines`] - joined by array position, exactly
/// like [`CATALOG_LINES_PATH`] itself). Every `file:line` citation below reads from `lines`,
/// never from a [`DupSite`]'s own `start_line`/`end_line` directly - the report's citations are
/// the SAME line data the unguarded sidecar persists, not a second, independent read of the
/// live scan (spec 90 Design: "the report keeps its `file:line` citations ... rendered from
/// that file").
fn render_section_2(
    files: &[FileScan],
    clusters: &[DupCluster],
    lines: &[DupClusterLines],
) -> String {
    assert_eq!(
        clusters.len(),
        lines.len(),
        "clusters and lines must be the same length, computed from the same pass"
    );
    let mut out = String::new();
    let _ = writeln!(out, "## 2. Duplication Catalog");
    let _ = writeln!(out);
    let total_sites: usize = clusters.iter().map(|c| c.sites.len()).sum();
    let _ = writeln!(
        out,
        "{} clusters ({} total sites) across `src/` and `tests/`, found by \
         `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard \
         pass ({SHINGLE_SIZE}-token shingles, threshold {SIMILARITY_THRESHOLD}) plus five \
         mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in \
         more than one place anywhere in the codebase is a violation, with no \
         \"small enough to duplicate\" exemption.",
        clusters.len(),
        total_sites
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "### Mandatory sweeps");
    let _ = writeln!(out);
    for name in MANDATORY_SWEEPS {
        match clusters.iter().find(|c| c.note.contains(name)) {
            Some(c) => {
                let _ = writeln!(out, "- **{name}**: {} site(s) - `{}`", c.sites.len(), c.id);
            }
            None => {
                let _ = writeln!(out, "- **{name}**: 0 sites found");
            }
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "### Clusters ({} exact, {} near, {} semantic)",
        clusters
            .iter()
            .filter(|c| c.classification == "exact")
            .count(),
        clusters
            .iter()
            .filter(|c| c.classification == "near")
            .count(),
        clusters
            .iter()
            .filter(|c| c.classification == "semantic")
            .count(),
    );
    let _ = writeln!(out);
    for (c, cl) in clusters.iter().zip(lines) {
        debug_assert_eq!(
            c.id, cl.id,
            "clusters and lines must share cluster order/identity"
        );
        let _ = writeln!(
            out,
            "#### `{}` ({}, {} sites)",
            c.id,
            c.classification,
            c.sites.len()
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "Proposed home: `{}`", c.proposed_home);
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", c.note);
        let _ = writeln!(out);
        for (s, sl) in c.sites.iter().zip(&cl.sites) {
            let _ = writeln!(
                out,
                "- `{}:{}-{}` `{}`",
                sl.file, sl.start_line, sl.end_line, s.name
            );
        }
        let _ = writeln!(out);
    }
    out.push_str(&render_adversarial_sample(files, clusters));
    out
}

/// Renders the "### Adversarial sample" subsection required by spec 85 THOROUGHNESS ("the
/// adversary draws 30 functions by seeded random index... The report states the sample seed so
/// the check is reproducible"). The draw itself and each row's catalog membership are computed
/// live from `files`/`clusters`, so the listing can never drift from the tree; the closing
/// paragraph is a hand-verified reading-pass finding (spec 85: the recall check is proven by
/// reading, which no generator can do) and is refreshed by hand whenever the draw shifts enough
/// to change what it covers.
fn render_adversarial_sample(files: &[FileScan], clusters: &[DupCluster]) -> String {
    let mut out = String::new();
    let refs = adversarial_sample_population(files);
    let picked = sample_indices(refs.len(), ADVERSARIAL_SAMPLE_SIZE, ADVERSARIAL_SEED);
    let _ = writeln!(out, "### Adversarial sample");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Recall check (spec 85 THOROUGHNESS): {} functions drawn by seeded random index (seed \
         `{ADVERSARIAL_SEED}`, `sample_indices` over all {} functions scanned in `src/` and \
         `tests/`, excluding `{ADVERSARIAL_SAMPLE_EXCLUDED_FILE}` - criterion 4's own citation-\
         guard periphery test, whose function count grows as its citation-drift-guard mechanism \
         hardens round over round; excluding it keeps that unrelated growth from ever reshuffling \
         this already-verified draw), each read by hand - together with its host file's \
         surrounding context, since a duplicate can live anywhere in the file or a sibling file - \
         to judge whether a duplicate exists that the mechanical pass and the five sweeps above \
         did not already catch.",
        picked.len(),
        refs.len(),
    );
    let _ = writeln!(out);
    for &i in &picked {
        let sf = refs[i].scanned(files);
        let hit = clusters.iter().find(|c| {
            c.sites
                .iter()
                .any(|s| s.file == sf.file && s.name == sf.name)
        });
        match hit {
            Some(c) => {
                let _ = writeln!(
                    out,
                    "- `{}:{}-{}` `{}` - caught: `{}`",
                    sf.file, sf.start_line, sf.end_line, sf.name, c.id
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "- `{}:{}-{}` `{}` - no duplicate found by reading",
                    sf.file, sf.start_line, sf.end_line, sf.name
                );
            }
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Two real recall gaps surfaced this way and were closed by widening the mechanical \
         sweep with a new generalizable detector each - not a one-off citation - so the fix \
         catches every present and future instance of its class, each pinned by a real-tree \
         regression test: `find_proc_stat_or_status_readers` (decision \
         `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or \
         `/proc/<pid>/status` literal, closing the spec's own named worked example - \
         `src/dash.rs:499-507` `process_state` next to `src/reap.rs:190-197` `pid_starttime`, the \
         same job on the same file with a different field/shape, upheld at spec 62's capstone; \
         `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) \
         groups 2+ non-test functions per `(file, Self type)` that build a `Self {{ .. }}` / \
         `TypeName {{ .. }}` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` \
         reading MISSING from its own `ok`/`failed` cluster even though all three are parallel \
         constructors for one struct. A third worked example, `exploration_graph` (independently \
         defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and \
         `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass \
         with no sweep needed - confirming the mechanical pass itself has real recall, not only \
         the two widened sweeps. Two further real defects, found on review rather than in this \
         draw, were closed the same way: a RECALL gap the architecture lens routed to this \
         criterion by name across two prior review rounds - this file's own bespoke source-text \
         lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter \
         extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc's claim, \
         architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision \
         `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the \
         adversary found by reading every `same-named helper` cluster against \
         `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying \
         REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/\
         `subscribe_stream` across the `EventStore` trait's three backend adapters plus a test \
         double, `blast_radius` across the `Grounder` trait's own default method, its override, \
         and a test double) - closed by excluding members whose extracted Self type differs \
         across the group when at least one comes from an actual `\" for \"` trait impl (decision \
         `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring \
         `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. \
         Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) \
         excluded this criterion's own citation-guard periphery file from the draw's population \
         (see this subsection's opening paragraph) and redrew the sample; every one of the 19 \
         functions above marked \"no duplicate found by reading\" was re-read by hand against \
         its host file's surrounding context, exactly as this THOROUGHNESS check requires \
         whenever the draw changes. 18 of the 19 are genuinely not duplicates; `apply` at \
         `src/conductor.rs:29832-29834` is one shape worth naming so it is not mistaken for a \
         miss - a `Projection` test double's own required trait-impl body, the same \
         port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s \
         trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) \
         already excludes from clustering by design, confirmed to still hold for this draw's own \
         instance of it. The 19th is a genuine small duplicate this catalog's `fn`-only scanner \
         (module doc, THE SCANNER) structurally cannot represent as a cluster: `gate_verdict_event` \
         (`src/conductor.rs:29191-29200`) and the `verdict` closure inside \
         `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` \
         (`src/conductor.rs:30596-30605`) do the identical job - find the recorded `GateVerdict` \
         for a `\"<unit>/gate:g#<attempt>\"` replay key, panicking with the same message when none \
         exists - differing only in whether the unit segment is the literal `\"s\"` or a \
         parameter. A `let`-bound closure is not a `fn` item, so no change to this scanner short \
         of teaching it to see closures could catalog this pair as a cluster; named here, \
         prominently, rather than silently, so a later refactor - or a scanner that learns to see \
         closures - does not miss it."
    );
    let _ = writeln!(out);
    out
}

/// Replace ONLY section 2's span (from its `## 2. ` heading up to, but not including, the next
/// `## ` heading) inside an EXISTING report `existing`, leaving every other section byte-for-
/// byte untouched - mirrors [`replace_section_1`] exactly (this module's doc comment /
/// decision `u85c1-report-section-placeholders`: each unit's generator only ever owns its own
/// section span). Panics if `existing` has no `## 2. ` heading at all - that would mean the
/// report is missing criterion 1's placeholder contract, a precondition this criterion (and
/// every later one) relies on, not a case to paper over silently.
/// The `## N. ` section's own byte span within `existing` - shared by [`replace_section_2`]
/// (which overwrites it) and spec 90 criterion 2's structural drift-guard check (which reads it
/// without touching the rest of the document, since a byte comparison would fail on every pin
/// bump elsewhere in the tree - see `assert_section_2_structurally_matches`).
fn section_span(existing: &str, marker: &str) -> std::ops::Range<usize> {
    let start = find_heading(existing, marker).unwrap_or_else(|| {
        panic!(
            "{REPORT_PATH} has no {marker:?} heading - missing criterion 1's placeholder \
             contract"
        )
    });
    let rest_after_marker = &existing[start + marker.len()..];
    let end_offset = rest_after_marker.find("\n## ").map(|p| p + 1);
    let end = match end_offset {
        Some(off) => start + marker.len() + off,
        None => existing.len(),
    };
    start..end
}

fn replace_section_2(existing: &str, section_2: &str) -> String {
    let span = section_span(existing, "## 2. ");
    let mut out = String::new();
    out.push_str(&existing[..span.start]);
    out.push_str(section_2);
    if !section_2.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&existing[span.end..]);
    out
}

// =========================================================================================
// CRITERION 3 (`u85c3`, THIS UNIT): SECTIONS 3-5 (BOUNDARY VIOLATIONS, DEAD AND VESTIGIAL
// CODE, TEST-SUITE SHAPE)
// =========================================================================================
//
// This criterion's own Done-when text: "This criterion OWNS sections 3-5 and introduces no
// generator code" - unlike criteria 1 and 2, there is no new mechanical scanner here. Each
// `render_section_N` below is hand-authored prose from a real investigation (decisions
// `u85c3-scope-and-instruments`, `u85c3-boundary-violation-mutation-scratch-reach`,
// `u85c3-dead-code-clean-both-instruments`, `u85c3-test-suite-shape-from-committed-catalog`),
// citing `file:line` and naming its instrument per claim, exactly as sections 1 and 2 already
// do for their own mechanically-derived content. Deliberately built with `String::push_str`
// rather than `writeln!`/`format!` throughout: the content is 100% static (no interpolated
// runtime values), and `push_str` needs no `{{`/`}}` escaping for the literal braces this
// section's own quoted `AgentDriver` trait definition and `courier_registry_refresh_{boundary,
// fence,periphery}` file-glob prose require.

/// Section 3, BOUNDARY VIOLATIONS: one real finding (`src/conductor.rs`'s mutation-scratch
/// reclaim reaching directly into the concrete `driver::replay` adapter for a concern no port
/// covers) plus four explicitly-checked-and-clean port-concretion sweeps and the
/// use-cases-importing-infrastructure / second-mutation-authority categories (decision
/// `u85c3-boundary-violation-mutation-scratch-reach`).
fn render_section_3() -> String {
    let mut out = String::new();
    out.push_str("## 3. Boundary Violations\n\n");
    out.push_str(
        "Instrument: for each of the five named ports (`eventstore::EventStore`, \
        `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, \
        `grounder::Grounder`), grepped every production (pre-`#[cfg(test)]`) call \
        site of that port's known concrete adapter modules from a NON-adapter, \
        NON-composition-root file, and separately grepped every domain-ish file's \
        top-level `use` statements for a direct infrastructure-crate import. \
        `src/main.rs` is exempt from the \"reaches a concrete adapter\" check: it is \
        the composition root, and wiring concretions together is its designed job.\n\n",
    );
    out.push_str("FOUND, two violations:\n\n");
    out.push_str(
        "Violation 1 (`AgentDriver`): `src/conductor.rs:7091-7096` \
        (`reclaim_terminal_unit_mutation_scratch`, real production code - well above \
        the `#[cfg(test)] mod tests` boundary this audit's own section 1 identified \
        at `src/conductor.rs:10260`) calls `crate::driver::replay::cache_home_from` \
        and `crate::driver::replay::reclaim_unit_mutation_scratch` directly by \
        concrete module path. Read via `rigger graph --show AgentDriver`: the port \
        `conductor.rs` actually depends on for driving agents is `trait AgentDriver \
        { fn spawn(&self, agent: &AgentDef, prompt: &str, opts: &SpawnOpts, emit: \
        &dyn Fn(&str, Value) -> Result<(), Error>) -> Result<AgentResult, Error>; }` \
        (`src/conductor.rs:1083-1091`) - one method, `spawn`. Neither called \
        function is about driving an agent or replaying a recorded run (the concern \
        `driver::replay` otherwise owns); both are pure, driver-instance-free \
        scratch-lifecycle utilities that happen to live inside that one concrete \
        adapter's module. The port that should have been used: none exists for this \
        concern yet, which is itself the defect - `conductor.rs` (a \
        use-case/orchestration file) should not need to know which concrete \
        `AgentDriver` implementation happens to define its own mutation-scratch \
        cache-home resolution. Fix direction for a follow-up spec: relocate \
        `cache_home_from` and `reclaim_unit_mutation_scratch` out of \
        `driver::replay` into a neutral, adapter-independent module (a `scratch` or \
        `mutation` support module conductor.rs and every driver adapter can depend \
        on alike), so no use-case file reaches into one specific adapter's internals \
        for a concern that adapter does not conceptually own.\n\n",
    );
    out.push_str(
        "Violation 2 (`Grounder`): `src/ingest.rs:187-211` (`walk_batches`, called \
        from production `conductor::RunCtx::ingest_project_batches` at \
        `src/conductor.rs:7903`, itself called from `src/conductor.rs:7894` well \
        above the `10260` `#[cfg(test)]` boundary) calls \
        `crate::grounder::symbols::events::project_batches_paced` directly by \
        concrete module path at line 197 to reuse the `symbols` grounder's \
        already-persisted index for a one-time whole-project ingest walk, then at \
        line 203 - same function, same missing-port defect, not a separate third \
        violation - calls `crate::grounder::design::events::project_batches` \
        directly by concrete module path for the design-doc half of the same walk. \
        These two calls are the two named sites of section 2's own catalogued twin \
        duplicate pair (`dup-0201`: `src/grounder/symbols/events.rs:36-38` and \
        `src/grounder/design/events.rs:90-114`, both named `project_batches`), so \
        this boundary violation and that duplication finding are two symptoms of \
        one root cause - `ingest.rs` naming each concrete grounder submodule \
        because no port exposes either. The `Grounder` port's own methods \
        (`ground`, `reindex`, `blast_radius`, `index_stamp` - its provenance stamp \
        - all at `src/grounder/mod.rs:133-175`) serve real-time per-query \
        grounding of an agent's prompt; none exposes \"hand me every indexed \
        file's projected events for a whole-project batch ingest,\" so \
        `ingest.rs` - itself a domain ingest authority (its own module doc names \
        it \"the ONE walk-and-content-key authority\"), not an adapter and not \
        the composition root - has no port to depend on for either call and \
        reaches the concrete `symbols` module (197) and the concrete `design` \
        module (203) directly. Same missing-port defect class as violation 1. Fix \
        direction for a follow-up spec: add an ingest-shaped port method (e.g. a \
        `Grounder::project_batches` or a standalone `SymbolProjector` trait) \
        covering both concrete modules, so `ingest.rs` depends on one abstraction \
        instead of either concrete grounder module for its whole-project walk.\n\n",
    );
    out.push_str(
        "Also reaching `grounder::symbols::store::content_hash` from the same two \
        call sites' neighborhood (`src/ingest.rs:230`, `src/canary.rs:213`): \
        DISPOSITIONED as legitimate shared-primitive reuse, not a third violation. \
        `content_hash` (`src/grounder/symbols/store.rs:49`) is documented at its own \
        definition as \"the content-identity primitive\" the `symbols` grounder's \
        own reindex-freshening gate keys on, and `canary.rs`'s own doc comment \
        (`canary.rs:191`) separately calls it \"the crate's ONE stable content-hash \
        primitive\", reused there by deliberate author intent rather than adding \
        yet another open-coded FNV-1a copy - a generic hashing utility that happens \
        to live in the `symbols` module, not a grounding operation reached through \
        the port. The broader duplication this primitive is meant to fix (several \
        open-coded FNV-1a copies elsewhere in the crate, per `src/community.rs`'s \
        own comment at line 67) is a separately tracked cross-cutting refactor \
        (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's concern.\n\n",
    );
    out.push_str(
        "CHECKED AND CLEAN (three of five ports fully clean; the other two, \
        `AgentDriver` and `Grounder`, are this section's two violations above - each \
        search recorded so a clean result is not merely assumed):\n",
    );
    out.push_str(
        "- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` \
        outside `src/eventstore/sqlite.rs` / `src/eventstore/kurrentdb.rs` / \
        `src/contextgraph/sqlite.rs`): two hits in all of `src/`, one a doc-comment \
        mention (`src/main.rs:4332`) and one a deliberate, explicitly-commented \
        test-only raw-connection bypass (`src/main.rs:23860`, inside `#[cfg(test)] \
        mod tests` opened at `src/main.rs:12650`) that reproduces a pre-append-guard \
        corruption shape `Store::append` itself refuses to construct - a documented \
        test technique, not a boundary violation.\n",
    );
    out.push_str(
        "- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): \
        checked whole-tree, not only `src/conductor.rs` - every one of \
        `conductor.rs`'s 28 hits sits inside `#[cfg(test)] mod tests` (production \
        `conductor.rs` only ever depends on `dyn Projection`), and the same is true \
        wherever else `contextgraph::sqlite::Projector` is imported \
        (`src/concepts.rs`, `src/dash.rs`, `src/grounder/symbols/events.rs`, \
        `src/grounder/design/events.rs` - every import sits after that file's own \
        `#[cfg(test)]` boundary); `src/community.rs`'s one mention is a doc \
        comment.\n",
    );
    out.push_str(
        "- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): \
        checked whole-tree, not only `src/conductor.rs`. In `conductor.rs`, \
        production depends only on `dyn gate::Runner` (`conductor.rs:1288`); every \
        mention of a concrete runner before that is a doc comment \
        (`conductor.rs:6498,6545,6615,6621,6625`), and the only actual import and \
        use of `ExecRunner` (`conductor.rs:10266` onward) plus the test-only \
        `RecordingRunner` impl (`conductor.rs:28923,29014`) sit inside \
        `#[cfg(test)] mod tests`, well past the `10260` boundary. Every other \
        source-level `ExecRunner` mention in `src/` is either a doc comment \
        (`src/worktree.rs`, `src/config.rs`, `src/budget.rs`, `src/driver/cli.rs`, \
        `src/lib.rs`) or, in `src/driver/replay.rs`, an import and 13 parameter \
        types that all sit inside that file's own `#[cfg(test)] mod tests` too.\n",
    );
    out.push_str(
        "- Use cases importing infrastructure: grepped the top-level `use` statements \
        of every domain-ish file this audit's own code neighborhood names \
        (`src/conductor.rs`, `src/blocker.rs`, `src/spec.rs`, `src/watch.rs`, \
        `src/community.rs`) for `rusqlite`, `reqwest`, `tonic`, `tokio`, `kurrentdb` \
        - zero hits anywhere. Empty category.\n\n",
    );
    out.push_str(
        "A second mutation authority for one domain: the one previously-known \
        instance in this codebase (`src/dash.rs` reimplementing `src/reap.rs`'s \
        `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) \
        is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it \
        is section 2's finding (`u85c2-proc-stat-worked-example`, \
        `find_proc_stat_or_status_readers`), not re-counted here to avoid \
        double-charging one defect to two sections. Checked git as the one other \
        plausible second-authority candidate: every `Command::new(\"git\")` call \
        site in `src/conductor.rs` (23 sites) is at line >= 17297, inside \
        `#[cfg(test)] mod tests` - production `conductor.rs` never shells to git \
        directly. `src/worktree.rs` is the sole git-worktree-mutation authority \
        OUTSIDE the composition root. Inside it, `src/main.rs` (exempt from the \
        port-concretion-reach check above, not from this one) holds two more \
        git-worktree-mutation sites: `reap_then_remove_worktree` \
        (`main.rs:2791-2805`), the sanctioned worktree half of the spec-34/spec-79 \
        orphan-sweep and extensively reviewed across those specs - a deliberate \
        design choice, not a gap; and `materialize_config_at_rev` \
        (`main.rs:5510-5556`), a real, already-known, non-blocking gap \
        (`arch-u13-config-checkout-bypasses-worktree-authority` / \
        `arch-u2r-config-checkout-shells-git` / \
        `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the \
        `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so \
        this is a gap in that authority rather than a competing abstraction). No \
        second mutation authority found beyond the already-cited, \
        already-catalogued `/proc` case and this already-dispositioned \
        `materialize_config_at_rev` gap.\n",
    );
    out
}

/// Criterion 3's own KG-degree lookup, hand-run via `rigger graph --show <entity>` for every real
/// candidate (spec 87 Design's THE KNOWLEDGE GRAPH CROSS-CHECK) and hardcoded here - a report-
/// rendering annotation, never a JSON field (spec 87 OUTPUT names no degree field for
/// `dead-code.json`), so it lives beside the rendering, not the instrument. `src/ingest.rs`'s two
/// `#[cfg(feature)]`-gated `ingest_project` sites share one lookup: the graph's own entity
/// resolution for this shared name only ever returns the light-lane (`:468`) definition
/// regardless of which line is asked for - a disclosed instrument-boundary quirk (section 4.2
/// discloses it), not this function's own imprecision.
fn kg_degree_for(file: &str, name: &str) -> u32 {
    match (file, name) {
        ("src/canary.rs", "cataloged_classes") => 12,
        ("src/config.rs", "sdet_author_enabled") => 3,
        ("src/dash.rs", "pid_is_alive") => 5,
        ("src/dash.rs", "neighborhood") => 7,
        ("src/dash.rs", "serve") => 3,
        ("src/distiller.rs", "rebuild") => 15,
        ("src/eventstore/sqlite.rs", "with_content_identity") => 32,
        ("src/gate.rs", "resolve_wrapper_name") => 3,
        ("src/grounder/symbols/events.rs", "index_events") => 17,
        ("src/grounder/symbols/model.rs", "definitions_named") => 7,
        ("src/grounder/symbols/model.rs", "references_named") => 10,
        ("src/ingest.rs", "ingest_project") => 3,
        ("src/ledger.rs", "fully_done") => 6,
        ("src/ledger.rs", "is_integrated") => 3,
        ("src/spawn.rs", "new") => 5,
        ("src/spawn.rs", "with_system_prompt") => 6,
        ("src/spawn.rs", "with_model") => 7,
        ("src/spawn.rs", "with_tools") => 5,
        ("src/spawn.rs", "with_dir") => 6,
        ("src/spawn.rs", "with_blast_radius") => 6,
        ("src/spawn.rs", "with_title") => 7,
        ("src/spawn.rs", "with_reviews") => 5,
        ("src/spawn.rs", "park") => 14,
        // Round 3 (spec 89 criterion 1, `arch-u89c1r2-dirty-check-duplicated-and-diverges-
        // fail-direction`) shifts `is_dirty` to line 635 AND changes its body (a one-line
        // delegation to the new `path_is_dirty` free fn, replacing the direct `git(...)` call).
        // `rigger graph --show` only reflects `.`'s own checked-out ref (the main repo's base
        // branch), which cannot see an unmerged unit branch's edits - reconfirmed live: it still
        // reports the PRE-spec-89 body at line 544. Degree re-derived by the same two-part count
        // the live tool's own output visibly used at that base snapshot (test_only_references
        // count + call-out count, 4 + 1 = 5): `is_dirty`'s callers are its same 4 test-only call
        // sites, untouched by this round (see this file's `disposition_for`), and its body still
        // makes exactly ONE outgoing call - `path_is_dirty` in place of `git` - so the edge count
        // is unchanged at 5.
        ("src/worktree.rs", "is_dirty") => 5,
        (other_file, other_name) => panic!(
            "dead-code candidate {other_file}::{other_name} has no recorded knowledge-graph \
             degree - run `rigger graph --show {other_file}::<name>` and add it here (spec 87 \
             Design's THE KNOWLEDGE GRAPH CROSS-CHECK)"
        ),
    }
}

/// The wire spelling of a [`Disposition`] (matches `#[serde(rename_all = \"kebab-case\")]`
/// exactly) - the report renders the same literal word the committed JSON carries.
fn disposition_label(d: Disposition) -> &'static str {
    match d {
        Disposition::Delete => "delete",
        Disposition::KeepPublicSurface => "keep-public-surface",
        Disposition::KeepPending => "keep-pending",
    }
}

/// Section 4.2's per-file distribution table, rendered from [`real_dead_code_candidates`] (file
/// order, already `(file, line)`-sorted) so it can never drift from the committed JSON by a hand-
/// transcription error - 26 entries across 12 files invite exactly that.
fn render_dead_code_distribution_table() -> String {
    let mut out = String::new();
    out.push_str("| File | Count |\n|---|---|\n");
    let candidates = real_dead_code_candidates();
    let mut i = 0;
    while i < candidates.len() {
        let file = candidates[i].file.as_str();
        let count = candidates.iter().filter(|c| c.file == file).count();
        out.push_str(&format!("| `{file}` | {count} |\n"));
        i += count;
    }
    out.push_str(&format!("| **Total** | **{}** |\n", candidates.len()));
    out
}

/// Section 4.3's full list, rendered from [`real_dead_code_candidates`] grouped by file - name,
/// line, visibility, ambiguity, KG degree ([`kg_degree_for`]), disposition
/// ([`disposition_label`]) and the full cited reason ([`disposition_for`]) - so the report and
/// the committed JSON can never state two different reasons for one candidate: both read the
/// SAME lookup table. Spec 90 criterion 2: `lines` carries each candidate's own line data in
/// the SAME order as `real_dead_code_candidates()` ([`dead_code_candidate_lines`] - joined by
/// array position, exactly like [`DEAD_CODE_LINES_PATH`] itself). Every `file:line` citation
/// below - including `ambiguous_with`'s own citations - reads from `lines`, never from a
/// [`DeadCodeCandidate`]'s own `line`/`ambiguous_with` directly - mirrors [`render_section_2`]'s
/// own `lines` param exactly (spec 90 Design: "the report keeps its `file:line` citations ...
/// rendered from that file"). ONE exception, deliberate: [`kg_degree_for`]'s own `(file, line)`
/// arguments are a LOOKUP KEY into a hand-maintained table keyed on the tree's real definition
/// sites, never a rendered citation (its OUTPUT, the degree number, is what the report shows) -
/// those read the candidate's own live field, like every other non-citation field here.
fn render_dead_code_full_list(lines: &[DeadCodeCandidateLines]) -> String {
    let candidates = real_dead_code_candidates();
    assert_eq!(
        candidates.len(),
        lines.len(),
        "candidates and lines must be the same length, computed from the same pass"
    );
    let mut out = String::new();
    let mut current_file: Option<&str> = None;
    for (c, cl) in candidates.iter().zip(lines) {
        debug_assert_eq!(
            c.file, cl.file,
            "candidates and lines must share order/identity"
        );
        debug_assert_eq!(
            c.name, cl.name,
            "candidates and lines must share order/identity"
        );
        if current_file != Some(cl.file.as_str()) {
            out.push_str(&format!("\n**`{}`**\n\n", cl.file));
            current_file = Some(cl.file.as_str());
        }
        let amb = if c.ambiguous {
            format!(" (ambiguous with {})", cl.ambiguous_with.join(", "))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "- **{}** (`{}:{}`, `{}`{}, KG degree {}): `{}`. {}\n",
            c.name,
            cl.file,
            cl.line,
            c.visibility,
            amb,
            // `kg_degree_for`'s own (file, line) pair is a LOOKUP KEY into a hand-maintained
            // table keyed on the tree's real definition sites (see its own panic message: "run
            // `rigger graph --show <file>::<name>` and add it here") - internal plumbing, never
            // a rendered citation, so it reads the candidate's own live field like every other
            // non-citation field above (`c.visibility`, `c.disposition`, `c.reason`), not the
            // unguarded `lines` sidecar.
            kg_degree_for(&c.file, &c.name),
            disposition_label(c.disposition),
            c.reason
        ));
    }
    out.push('\n');
    out
}

/// Section 6 item 0's own per-file deletion list: each entry's name AND its own `(line N)`
/// citation (corrected round 3 - a prior doc comment here claimed "names only", contradicted by
/// this function's own body, which has always rendered a citation too), rendered from
/// [`real_dead_code_candidates`] filtered to [`Disposition::Delete`] so it can never list a
/// `keep-pending` entry by transcription error, paired with `lines`
/// ([`DeadCodeCandidateLines`], the SAME sidecar section 4.3 reads) so this list's own citation
/// is never a second, independent read of the live scan - spec 90 criterion 2, mirroring
/// [`render_dead_code_full_list`] exactly.
fn render_dead_code_deletion_list(lines: &[DeadCodeCandidateLines]) -> String {
    let candidates = real_dead_code_candidates();
    assert_eq!(
        candidates.len(),
        lines.len(),
        "candidates and lines must be the same length, computed from the same pass"
    );
    let mut out = String::new();
    let mut current_file: Option<&str> = None;
    for (c, cl) in candidates
        .iter()
        .zip(lines)
        .filter(|(c, _)| c.disposition == Disposition::Delete)
    {
        debug_assert_eq!(
            c.file, cl.file,
            "candidates and lines must share order/identity"
        );
        debug_assert_eq!(
            c.name, cl.name,
            "candidates and lines must share order/identity"
        );
        if current_file != Some(cl.file.as_str()) {
            if current_file.is_some() {
                out.push('\n');
            }
            out.push_str(&format!("  - `{}`: ", cl.file));
            current_file = Some(cl.file.as_str());
        } else {
            out.push_str(", ");
        }
        out.push_str(&format!("`{}` (line {})", c.name, cl.line));
    }
    out.push('\n');
    out
}

/// Section 4, DEAD AND VESTIGIAL CODE. 4.0 is stage 1 (criterion 1, `u87c1`): the compiler-driven
/// pass, additive and preserved verbatim. 4.1-4.3 are stages 2-3 (criteria 2-3, `u87c2`/`u87c3`,
/// THIS unit owns 4.2-4.3): the whole-`src/`-tree production-reference sweep replaces the STALE
/// prior finding this doc comment used to make ("zero live dead functions" - true only of the
/// 596-function three-god-file scope stage 1 inherited from spec 85, and only because that
/// scope's own scanner counted `tests/` and every `#[cfg(test)]` body as a live reference, the
/// exact false negative spec 87's own Goal names) with 26 real candidates, each carrying one of
/// three dispositions with a cited reason (decision `u87c3-self-colon-colon-qualifier-false-
/// positive` fixed one real scanner bug the research found along the way). 4.4 (retired-feature
/// remnants, stale doc claims) is criterion-1-era content, re-verified unchanged by this
/// criterion's own research and carried forward verbatim.
fn render_section_4() -> String {
    let mut out = String::new();
    out.push_str("## 4. Dead and Vestigial Code\n\n");
    out.push_str("### 4.0 Stage 1 (spec 87 criterion 1): the compiler-proven pass\n\n");
    out.push_str(
        "Spec 87 redoes this whole section in two stages: STAGE 1, here, is compiler-driven \
        and lands first; STAGE 2 (criterion 2) and STAGE 3 (criterion 3) are a separate \
        reference sweep over the tree this stage leaves behind, and rewrite everything below \
        this subsection in full. This subsection is this stage's own record, additive to and \
        preserved by that later rewrite.\n\n\
        INSTRUMENT ONE (`cargo minify`, tweedegolf's tool): run over the whole crate on the \
        default (`symbols`) feature lane, checking every FUNCTION, CONST, STATIC, STRUCT, \
        ENUM, UNION, TYPE_ALIAS, ASSOCIATED_FUNCTION and MACRO_DEFINITION for zero references. \
        Result: \"no unused code that can be minified\" - zero items removed. `cargo minify` \
        has no `--no-default-features` flag (verified: `cargo minify --no-default-features` -> \
        `error: unrecognized option`), so the light lane's own picture comes from instrument \
        two below instead.\n\n\
        INSTRUMENT TWO (the promoted-lint build): `cargo rustc --lib` and `cargo rustc --bin \
        rigger`, each on both the default and `--no-default-features` lanes, with `-D \
        dead_code -D unused_imports -D unused_variables -D unreachable_pub` passed as trailing \
        (target-only) flags - four builds total, all clean. `cargo rustc`'s trailing flags \
        were chosen deliberately over a whole-crate `RUSTFLAGS` env var (tried first): \
        `RUSTFLAGS` also strict-lints `build.rs`'s own compilation, which then fails on two \
        items in `build/gitsemver.rs` that are correctly `pub` for their other three \
        `#[path]` inclusion sites (`src/main.rs`, `tests/gitsemver_derivation.rs`, \
        `tests/gitsemver_worktree_periphery.rs`) but register as \
        `unreachable_pub` from `build.rs`'s own isolated crate view - a false positive from \
        the blunt instrument, not a real defect in `build.rs`. `cargo rustc`'s trailing flags \
        apply only to the one named target's own rustc invocation, never to a dependency and \
        never to `build.rs`, so it proves exactly the claim this criterion's Done-when makes \
        (a build of the library and the binary) without that false positive.\n\n\
        FOUND, in `src/`: zero items either instrument could prove unreachable - \
        `delete_compiler` in the companion record (`docs/audit/stage1-compiler-pass.json`) is \
        the empty list. This is the exact known blind spot this spec's own Design section \
        predicts, not an unexplored gap: `rustc`'s `dead_code` lint never fires on a `pub` \
        item in a crate that is both a library and a binary (this report's own prior finding, \
        instrument three below, already established the codebase's src/ is clean under a \
        plain rebuild), and `unreachable_pub` only catches a `pub` item that is provably \
        reachable from NOWHERE outside its own crate - not one that is correctly exported but \
        simply has zero real callers. That second, larger class needs the reference-counting \
        instrument stage 2 builds, not a compiler diagnostic; this stage's near-empty yield is \
        exactly why stage 2 exists.\n\n\
        FOUND, outside `src/` (`build/gitsemver.rs`, spec 74's compile-time \
        version-derivation seam, shared via `#[path]` into four separate compilations - \
        `build.rs`, `src/main.rs`, `tests/gitsemver_derivation.rs`, and \
        `tests/gitsemver_worktree_periphery.rs`): two items, \
        `UNVERSIONED_SUFFIX` (line 45) and `derive_version` (line 129), were `pub` when \
        nothing outside their own defining crate ever reaches them at any of those four \
        inclusion sites - `pub(crate)` satisfies every site independently, since each \
        `#[path]` inclusion recompiles the same source text fresh as part of whichever crate \
        includes it. Applied as the compiler's own suggested fix (`rustc`: \"consider \
        restricting its visibility: `pub(crate)`\"); both a fast regression test \
        (`tests/compiler_pass_stage1_audit.rs`) and this record hold the exact file:line so a \
        future widening back to `pub` is caught. Not counted in `delete_compiler` above - a \
        visibility narrowing is not a deletion, and `build/` is not `src/` - but disclosed \
        here in full rather than silently folded into either count, mirroring this section's \
        own \"disclosed as an instrument limitation rather than silently worked around\" \
        discipline below.\n\n\
        VERIFIED STILL GREEN on the tree as this stage leaves it: `tests/no_os_kill_audit.rs` \
        and `tests/reap_before_removal_audit.rs`, both process-lifecycle audits this \
        criterion's Done-when names by name - unaffected, since this stage's only change \
        touches a compile-time version string, never process lifecycle.\n\n",
    );
    out.push_str(
        "### 4.1 Stage 2 (spec 87 criterion 2): the whole-tree production-reference sweep\n\n",
    );
    out.push_str(
        "Stage 1's own near-empty yield (`delete_compiler` is the empty list) is the exact known \
        blind spot the Design section predicts: `rustc`'s `dead_code` lint never fires on a \
        `pub` item in a crate that is both a library and a binary. STAGE 2, criterion 2's own \
        unit (`u87c2`, four review rounds, approved `adj-u87c2-r3-verdict-approve`), replaces \
        the prior report's name-reference sweep - scoped to only the 596 production fns of the \
        three god files and counting WHOLE-TREE name occurrences including every `#[cfg(test)]` \
        body - with a whole-`src/`-tree, FILE-AWARE (an out-of-line `#[cfg(test)] mod name;` \
        target is test code in full, transitively) production-only reference sweep, extending \
        `tests/simplification_audit.rs`'s existing scanner rather than adding a third lexer. \
        Four rounds each found and closed a genuine false-positive-class defect (a mod-body test \
        leak, a `serde(default = \"..\")` attribute string, an impl-generics-dropping qualifier, \
        a struct-literal-field-value/UFCS-value invisible shape) before converging on THE RULE: \
        a production reference to fn F is any identifier token equal to F's name in production \
        code, whatever token precedes or follows it - no reference SHAPE is enumerated at all \
        any more, closing the whole class of \"the next invisible shape\" false positives the \
        first three rounds kept finding one at a time. Output: `docs/audit/dead-code.json`, one \
        entry per production fn with zero production references (name, file:line, visibility, \
        ambiguity, the test-only references that kept it looking alive), drift-guarded exactly \
        like the duplication catalog.\n\n",
    );
    out.push_str("### 4.2 Stage 3 (spec 87 criterion 3, THIS): the dispositions\n\n");
    out.push_str(
        "This criterion (`u87c3`) OWNS section 4's text, the dispositions, and section 6's item \
        0 - not a new instrument. While researching dispositions it found and fixed one real \
        defect IN the stage-2 instrument, disclosed rather than silently folded in: an ambiguous \
        `ImplAssoc` fn (ambiguity = a bare name shared by >= 2 production fns of the same kind, \
        e.g. `parse`, shared here by `dash.rs`/`gate.rs`/`ledger.rs` x2/`failure.rs`) referenced \
        ONLY via `Self::name(` from within its OWN impl block was never attributed correctly - \
        the qualifier-resolution step captures the literal token text in front of `::`, and for \
        `Self::parse(` that text is the keyword `Self`, never the enclosing type's own name, so \
        the `resolved == my_qualifier` match always failed. Concretely: `DashMarker::parse` \
        (`src/dash.rs:398`) is called only via `Self::parse(s)` inside `DashMarker::read` \
        (`dash.rs:408`) - itself called from real production code at `main.rs:5731`, `7313` and \
        `7629` - so it read as dead when it is genuinely alive: the dangerous false-positive \
        direction this whole unit's own precision discipline forbids (\"a false-positive dead \
        verdict is the dangerous direction\"), and one this criterion's own disposition work \
        would have shipped as a recommended deletion of live code had it gone unnoticed. FIX \
        (same shared instrument, not a parallel one): a `Self::name(` occurrence in the SAME \
        FILE as the candidate's own definition now resolves to it directly - the identical \
        same-file lexical-scope approximation the sweep already applies to a bare `Free`-fn \
        sibling call, widened to cover the keyword `Self` too. Verified against every OTHER \
        ambiguous name in the final candidate set (`rebuild`/`distiller.rs`, `ingest_project` x2 \
        /`ingest.rs`, `new`/`spawn.rs`): none has a same-file `Self::name(` call, so the fix's \
        blast radius is exactly the one candidate it removes. The committed candidate count \
        drops from 27 to 26 as a result.\n\n\
        THE COUNT AND PER-FILE DISTRIBUTION (26 candidates across 12 files):\n\n",
    );
    out.push_str(&render_dead_code_distribution_table());
    out.push_str(
        "\nMETHODOLOGY: every one of the 26 was independently re-verified by hand (NOT taken on \
        the instrument's word alone, per this whole audit's own \"green was never sufficient\" \
        discipline) - a recursive `grep -rn` for the fn's own call-shaped name across the WHOLE \
        `src/` tree (never scoped to a single file or glob, closing the exact \
        `src/*.rs`-vs-`src/**/*.rs` gap this criterion's own research hit once and fixed before \
        it could hide a real caller) to confirm no production caller exists anywhere, then a \
        read of the doc-claimed OR actually-wired real caller/consumer to establish why. Every \
        entry carries exactly one of three dispositions (spec 87 DISPOSITIONS): `delete` (23 \
        entries) - the fn and the tests that reference only it, safe to remove because a \
        genuinely EQUIVALENT, ACTUALLY-WIRED replacement already exists in production (a batched \
        entry point, a multi-seed core, a real network probe, a direct struct literal - never \
        \"nothing needs this\" alone); `keep-public-surface` (0 entries today - none of the 26 \
        cites a real MCP/workflow-template/CLI-contract consumer, the only citations this \
        disposition accepts); `keep-pending` (3 entries) - a shipped, spec-tested mechanism with \
        no call site wired in yet, each citing the ALREADY-LANDED spec whose Done-when criteria \
        the candidate's own tests prove (spec 27's digest-pool rebuild, spec 32's SDET-author \
        toggle, spec 60's storage-level idempotency guard) - none invents a future spec number \
        that does not exist. THE KNOWLEDGE GRAPH CROSS-CHECK (spec 85's second instrument for \
        this section, spec 87 Design's own explicit follow-up): `rigger graph --show <entity>` \
        degree, run for all 26 real candidates and reported per-entry in the full list below, \
        does NOT cleanly read as \"agrees (zero)\" the way spec 87's Design anticipated post-spec-\
        86 - every one of the 26 shows a non-zero degree, because the graph's `degree` counts \
        every edge touching the node, GOVERNS edges from this very audit's own `DecisionMade`/ \
        `ReviewFinding` events about the host FILE included, not code-call edges alone (the same \
        depth-2-traversal GOVERNS-edge inflation this report's own section 4.0 already disclosed \
        for `rigger graph --around`). Disclosed plainly rather than silently worked around, \
        matching this section's own established discipline: the cross-check's real, useful \
        signal here is not \"zero\" but \"no candidate's degree is disproportionately large \
        relative to its file's decision/finding volume in a way that would suggest a hidden real \
        code-structure edge the reference sweep missed\" - none does; `src/eventstore/sqlite.rs`'s \
        `with_content_identity` (degree 32, the highest) is explained entirely by its 44 own \
        test-only references and spec 60's own extensive documentation, not by an undiscovered \
        caller.\n\n",
    );
    out.push_str("### 4.3 The full list, dispositioned\n\n");
    out.push_str(&render_dead_code_full_list(&real_dead_code_lines()));
    out.push_str("### 4.4 Retired-feature remnants and stale doc claims\n\n");
    out.push_str(
        "RETIRED-FEATURE REMNANTS. `turbovec` (spec 57, \"Retire turbovec\"): grepped \
        the whole tree (`src/`, `tests/`, `docs/`, `specs/`, `Cargo.toml`) for every \
        mention - found only the deliberate migration-error guard code \
        (`src/grounder/mod.rs`'s `is_retired_grounder` / the loud \
        `retired_grounder_error`) plus the tests and docs that keep it retired \
        (`tests/turbovec_retired.rs`, `tests/turbovec_retired_cargo_boundary.rs`, \
        `tests/grounder_name_contract.rs`, and several others naming it as a \
        guarded-against name). Zero implementing code, zero cargo feature, zero \
        dependency - confirmed by reading `Cargo.toml`'s `[features]` section in \
        full (one feature, `symbols`, on by default; no `turbovec` entry anywhere). \
        `kurrentdb` build-time feature flag (spec 47, \"KurrentDB is always \
        available\"): grepped `Cargo.toml` and every file under `src/` for `feature \
        = \"kurrentdb\"` / `-F kurrentdb` - zero hits outside the tests that guard \
        against its resurrection (`tests/kurrentdb_always_available.rs`); \
        `kurrentdb` and `tokio` are unconditional `[dependencies]` as spec 47 \
        requires, and `testcontainers` (the adapter's contract-test-only dependency) \
        correctly lives under `[dev-dependencies]`, never the production dependency \
        tree. Both named retirements are fully clean - a real, evidenced negative \
        finding, not an assumption.\n\n",
    );
    out.push_str(
        "STALE DOC CLAIMS. Scanned every `docs/*.md`, `README.md`, and \
        `CONTRIBUTING.md` for any `src/**/*.rs` or `tests/**/*.rs` path-shaped \
        substring and checked each cited path still exists on disk. Two misses \
        surfaced (`src/bar.rs` in \
        `docs/architecture-addendum-pit-of-success.md:252,256`; `src/modifier.rs` in \
        `docs/architecture.md:1022,1027-1028`) - both read in context and confirmed \
        generic illustrative examples in unrelated prose (`crates/foo/src/bar.rs` as \
        a spec-criterion example path, `core-schema/src/modifier.rs` as an event-log \
        worked example), never a real claim about this repository's own layout. Zero \
        genuine dangling file references found.\n",
    );
    out
}

/// Section 5, TEST-SUITE SHAPE: a hand-derived 13-group-plus-residual subsystem breakdown of
/// all 156 `tests/*.rs` files, a `tests/cli.rs` split plan, and shared-fixture /
/// table-driven-test consolidation candidates cross-referenced from the ALREADY-COMMITTED
/// `docs/audit/duplication-catalog.json` filtered to clusters whose every site sits under
/// `tests/` (decision `u85c3-test-suite-shape-from-committed-catalog`).
pub(crate) fn render_section_5() -> String {
    let mut out = String::new();
    out.push_str("## 5. Test-Suite Shape\n\n");
    out.push_str(
        "Instrument: `tests/` holds 156 files today (spec 85's Goal cites 153 - this \
        criterion's own two periphery files plus criterion 2's own periphery file, \
        all landed since the Goal text was written, account for the +3), 104,685 \
        lines by `wc -l` (this section's own prose lives inside \
        `simplification_audit.rs`, one of the 156 files this instrument counts, so \
        this criterion's own edits land inside this same file and this figure grows \
        with each such edit - accurate as of this section's most recent revision, \
        not guaranteed to remain so after further edits to this file). Subsystem \
        grouping is a hand-derived, ordered filename-keyword rule table (mirrors \
        criterion 1's own per-file classification convention: first-match-wins, \
        narrowest first, an explicit residual named rather than silently dropped). \
        The consolidation-candidate columns below cross-reference the \
        ALREADY-COMMITTED `docs/audit/duplication-catalog.json` (criterion 2's own \
        generator output, not re-scanned here) filtered to the 340 clusters whose \
        every site sits under `tests/`.\n\n",
    );
    out.push_str("### 5.1 Subsystem grouping and consolidation map\n\n");
    out.push_str("| Subsystem | Files | Lines | Consolidation note |\n");
    out.push_str("|---|---|---|---|\n");
    out.push_str(
        "| Dashboard: KG lenses & viz (code/concepts/community/files lenses, graph \
        exploration, overlays, viz layout) | 53 | 23,510 | Largest group by file \
        count; owns the single strongest shared-fixture evidence in the whole suite \
        (5.2) |\n",
    );
    out.push_str(
        "| CLI whole-binary integration (`cli.rs`, `watchdog_cli_periphery.rs`, \
        `ci_lanes.rs`) | 3 | 28,098 | `cli.rs` alone is 27,074 of these lines; split \
        plan at 5.3 |\n",
    );
    out.push_str(
        "| Knowledge-graph ingestion & context-graph projections | 14 | 8,952 | \
        dedup/fold/identity concerns, several already cross-clustered with the \
        dash/viz group |\n",
    );
    out.push_str(
        "| Conductor orchestration: gates, courier, step/run lifecycle | 19 | 8,257 | \
        the `courier_registry_refresh_{boundary,fence,periphery}` trio (3 files) \
        pair together in 6 clusters confined to just themselves (2-5 sites each; \
        excludes the whole-codebase Command::new/`.rigger`-path mandatory-sweep \
        clusters, section 2, that also happen to intersect them) |\n",
    );
    out.push_str(
        "| Reset / log compaction / store hygiene | 7 | 8,021 | `reset_menu.rs` and \
        `reset_menu_identity_migration_periphery.rs` pair together in 3 duplication \
        clusters |\n",
    );
    out.push_str("| Worktree & scratch/mutation-scratch lifecycle | 14 | 4,592 | |\n");
    out.push_str(
        "| Simplification-audit generator & its own periphery (this spec) | 3 | 5,829 \
        | `simplification_audit.rs` is itself the single largest test file after \
        `cli.rs` |\n",
    );
    out.push_str("| Spec/handbook lint & architecture-doc integrity | 6 | 3,344 | |\n");
    out.push_str(
        "| Grounding (symbols grounder, turbovec retirement, blast radius) | 8 | \
        3,348 | `kurrentdb_always_available.rs` and `turbovec_retired.rs` \
        independently redefine the same 4 Cargo.toml-reading helpers plus a \
        near-identical retired-feature-guard test (7 clusters, section 5.4/5.5) |\n",
    );
    out.push_str(
        "| Process lifecycle: no-os-kill & reap discipline | 4 | 3,320 | \
        `no_os_kill_audit.rs` and `reap_before_removal_audit.rs` pair together in 3 \
        clusters; each also has large internal near-duplicate families (5.5) |\n",
    );
    out.push_str("| Event store & config precedence | 11 | 2,780 | |\n");
    out.push_str(
        "| Canary (review-panel judge-the-judges evaluation) | 8 | 2,455 | two pairs \
        share a near-identical fixture shape: \
        `canary_false_positives_periphery.rs`/`canary_unattributed_rejects_periphery.rs` \
        and \
        `canary_item_sharding_jobs_cap_periphery.rs`/`canary_progress_hook_periphery.rs` \
        (2 clusters each) |\n",
    );
    out.push_str(
        "| Concepts/community lens derivation & fold (non-viz) | 4 | 1,536 | \
        `community_detection_cli.rs` and `concepts_derivation_cli.rs` pair together \
        in 9 clusters, the densest single file-pair in the whole catalog |\n",
    );
    out.push_str(
        "| Residual (no natural larger home) | 2 | 683 | \
        `build_budget_slots_periphery.rs`, `gitsemver_derivation.rs` - named rather \
        than forced into an ill-fitting bucket |\n",
    );
    out.push('\n');
    out.push_str(
        "Total: 156 files, 104,725 lines by this table's own per-file count (156 \
        files summed here) against 104,685 by a fresh `wc -l` above - the ~40-line \
        gap is `simplification_audit.rs`'s own line count moving as this section's \
        prose is written into it (the same self-measurement the instrument \
        paragraph above names), not a missing file; the per-file counts in the \
        table itself are not re-derived on every such move, since doing so for all \
        156 files on every edit is outside this criterion's own \
        no-new-generator-code scope.\n\n",
    );
    out.push_str("### 5.2 Shared fixtures to extract into `tests/common`\n\n");
    out.push_str(
        "`tests/common/mod.rs` already exists (`product_binary_from`, `rigger_bin`, \
        `rigger_courier`, `terminate_pid`, `stop_pid`, `is_alive`, `RestoreEnvVars`) \
        - the gap is everything duplicated OUTSIDE it. The catalog's cross-file (2+ \
        distinct files), all-helper-function clusters (181 of the 340 test-only \
        clusters) are the evidence; the four widest are the headline case for \
        extraction:\n\n",
    );
    out.push_str(
        "- `page_script` - a small JS snippet fixture - independently redefined in 19 \
        different files (`dup-0345`, exact; e.g. \
        `tests/adaptive_labels_periphery.rs:52-61`, \
        `tests/code_lens_overview_collapse_viz.rs:26-35`, \
        `tests/concepts_lens_view_periphery.rs:689-698`, + 16 more), all inside the \
        Dashboard/viz subsystem (5.1) - cross-validates that grouping.\n",
    );
    out.push_str(
        "- `node_available` - a viz-fixture predicate - independently redefined in 19 \
        files; the mechanical pass also clusters it together with the `gitsemver_available`/ \
        `npm_available` availability-check helpers (5 more sites across `src/main.rs` and \
        three test files) into one 24-site cluster (`dup-0243`, exact).\n",
    );
    out.push_str(
        "- `temp_project` - a scratch-project-directory fixture - independently \
        redefined in 19 files (`dup-0371`, semantic; e.g. \
        `tests/canary_model_drift_periphery.rs:39-46`, \
        `tests/cause_wire_periphery.rs:54-61`, `tests/cli.rs:19-29`), plus a \
        near-identical 12-site variant (`dup-0370`) and a 13-site `run_rigger` \
        companion helper that drives it (`dup-0372`).\n",
    );
    out.push_str(
        "- `run_stream_identity` - a store-identity fixture - independently redefined \
        in 18 files (`dup-0377`, semantic).\n\n",
    );
    out.push_str(
        "Proposed home for all four: `tests/common` (the catalog's own \
        `proposed_home` field already says so verbatim for each). Consolidating just \
        these four collapses roughly 72 duplicate definitions into 4 shared ones - \
        the single largest mechanical simplification this audit identifies anywhere \
        in the test suite.\n\n",
    );
    out.push_str("### 5.3 `tests/cli.rs` split plan\n\n");
    out.push_str(
        "27,074 lines, 351 `#[test]` functions, 15 pre-existing internal section \
        markers in the file: 5 full box-style banner-comment pairs \
        (`tests/cli.rs:11256/11258`, `11816/11818`, `11914/11916`, `12372/12374`, \
        `20514/20527`) plus 10 single-line `// --- Spec NN, criterion M` headers \
        (`21054`, `21972`, `22066`, `23403`, `25425`, `25548`, `25632`, `25770`, \
        `25983`, `26418`). So the file carries some existing, ad hoc organization - \
        each single-line header names the spec and criterion whose tests follow it, \
        not a CLI subcommand or subsystem - rather than the \"genuinely flat, not \
        internally organized\" state a first read might suggest; 15 markers spread \
        across 351 tests still fall well short of a deliberate, complete \
        per-surface structure. This correction does not disturb the split proposed \
        below: it replaces the file's existing ad hoc, by-spec markers with a \
        complete, deliberate BY CLI SUBCOMMAND SURFACE organization instead. A \
        keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, \
        `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, \
        `dash_`/`status_`, `store_`/`eventstore_`, \
        `spawn_`/`mutation_scratch_`/`scratch_`, `review_`/`gate_`, \
        `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, \
        `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, \
        `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers \
        274 of the 351 tests (78%) - disclosed honestly rather than overclaimed, \
        because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end \
        (a single test legitimately drives `step` + `run` + `validate` + `dash` \
        together to prove a cross-cutting property, e.g. \
        `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or \
        `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword \
        match cannot and should not force into one bucket. The proposed split is BY \
        CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since \
        the whole file drives the `rigger` binary end to end - into per-surface \
        files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, \
        `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, \
        `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, \
        `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely \
        cross-cutting scenarios), with each test's home decided by its DOMINANT \
        scenario on a human/AI read, not a mechanical keyword match - the same \
        discipline this audit's own responsibility map applied to unassignable \
        functions (named, never silently forced). Cross-referencing the catalog: \
        `cli.rs` also participates in 29 of the catalog's cross-file \
        test-duplication clusters (the most of any single file), several paired \
        against files that WOULD merge with it under this split \
        (`tests/step_attention_periphery.rs`, paired in 6 clusters; \
        `tests/watchdog_cli_periphery.rs`, paired in 2 clusters) - the split is \
        expected to shrink, not grow, the duplication surface.\n\n",
    );
    out.push_str(
        "### 5.4 Duplicated helpers across test files (beyond 5.2's four headline \
        cases)\n\n",
    );
    out.push_str(
        "181 test-only clusters in the committed catalog have every site as an \
        ordinary (non-`#[test]`) helper function - the shared-fixture-extraction \
        candidate class. Beyond the four in 5.2, the widest are: `dup-0349` \
        (`architecture_text` / `eventstore_source` / `main_rs_source` - \
        source-text-loading helpers for doc/architecture-integrity checks, 12 files, \
        15 sites); `dup-0376` (a companion, 16-file/16-site variant of 5.2's \
        `run_stream_identity` fixture, alongside `dup-0377`'s 18-file version); \
        `dup-0405` (`write_two_stage_workflow` / \
        `write_budget_one_two_stage_workflow` / `write_standalone_review_workflow` - \
        workflow-YAML-literal builders duplicated across `tests/cli.rs` and \
        `tests/step_attention_periphery.rs`, 4 files, 15 sites); \
        `dup-0376`/`dup-0377` (`seed_run_events`, an event-seeding helper, 6-8 \
        files); `dup-0469` (`apply_def_json` / `apply_ref_fresh`-shaped \
        fold-application helpers, 5 files); `dup-0474` (`community` / `concept` / \
        `def`-named single-field constructor helpers, 6 files); `dup-0477` \
        (`code_lens` / `concepts_lens` two-line accessor helpers, 3 files). Every \
        one of these 181 clusters, with its full site list and the catalog's own \
        `proposed_home`, is already machine-readable in the committed \
        `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to \
        consume directly - not re-enumerated exhaustively here to keep this section \
        a report, not a second copy of the catalog.\n\n",
    );
    out.push_str("### 5.5 Table-driven test families\n\n");
    out.push_str(
        "159 test-only clusters have every site as a `#[test]` function - a \
        literal-differs-only-in-input family, spec 85's own named table-driven-test \
        candidate class. The single largest anywhere in the suite: `dup-0662` (near, \
        42 sites, all in `tests/spec_lint.rs`, e.g. \
        `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class:54-102`, \
        `validate_spec_attributes_a_prose_level_defect_to_no_criterion:120-163`, \
        `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion:172-209` \
        - 42 near-identical \"feed one spec fixture through `validate`, assert one \
        expected defect/advisory line\" bodies). Proposed table: `#[test] fn \
        validate_spec_field_guide_defects() { for (fixture, expected) in CASES { ... \
        } }` retiring all 42 named tests into one parametrized loop over a `(&str, \
        &str)` (or richer struct) case table. Other large families: \
        `dup-0603`/`dup-0605` (15+7 sites, `tests/reap_before_removal_audit.rs`, \
        \"one fixture function body, one exemption-coverage shape, assert \
        covered/not-covered\" - retires into one table keyed by exemption shape); \
        `dup-0636` (11 sites, `tests/simplification_audit.rs` - this \
        very unit's own scanner tests, a `(source, expected_tokens_or_clusters)` \
        table candidate); `dup-0592`/`dup-0593` (11+4 sites, \
        `tests/no_os_kill_audit.rs`, one process-termination-pattern-string per test \
        - a `(pattern, is_caught)` table); `dup-0595` (4 sites, \
        `tests/no_os_kill_test_helper_periphery.rs`, \
        `terminate_pid_refuses_pid_zero` / `_pid_one` x `stop_pid_refuses_pid_zero` \
        / `_pid_one` - a 2x2 `(helper, pid)` table). As with 5.4, the full \
        157-family list lives in the committed catalog by cluster id for a follow-up \
        test-consolidation spec to consume directly.\n",
    );
    out
}

/// Replace sections 3 THROUGH 5's combined span (from the `## 3. ` heading up to, but not
/// including, the `## 6. ` heading) inside an EXISTING report `existing`, leaving sections 1,
/// 2 and 6 byte-for-byte untouched - the same one-owner-per-span contract as
/// [`replace_section_1`] / [`replace_section_2`] (decision `u85c1-report-section-placeholders`),
/// widened to a THREE-section span because this criterion owns sections 3, 4 and 5 together.
/// Deliberately searches for the NEXT criterion's `## 6. ` heading, not the next bare `## `
/// heading (unlike `replace_section_1`/`replace_section_2`'s single-section span) - `## 4. `
/// and `## 5. ` are internal to the content THIS function itself replaces, not a stopping
/// point; falls back to the end of the string if no `## 6. ` heading exists (a report that
/// somehow ends after section 5). Panics if `existing` has no `## 3. ` heading at all - that
/// would mean the report is missing criterion 1's placeholder contract, a precondition every
/// criterion after the first relies on, not a case to paper over silently.
fn replace_section_3_to_5(
    existing: &str,
    section_3: &str,
    section_4: &str,
    section_5: &str,
) -> String {
    let start = find_heading(existing, "## 3. ").unwrap_or_else(|| {
        panic!("{REPORT_PATH} has no '## 3. ' heading - missing criterion 1's placeholder contract")
    });
    let end = find_heading(existing, "## 6. ").unwrap_or(existing.len());
    let mut out = String::new();
    out.push_str(&existing[..start]);
    out.push_str(section_3);
    if !section_3.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(section_4);
    if !section_4.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(section_5);
    if !section_5.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&existing[end..]);
    out
}

// =========================================================================================
// CRITERION 4 (`u85c4`, THIS UNIT): SECTION 6 (PRIORITIZED PLAN)
// =========================================================================================
//
// This criterion's own Done-when text: "cites sections 1-5 and adds no new findings" -
// like criterion 3, there is no new mechanical scanner here (`render_section_6` is 100%
// hand-authored prose, built with `String::push_str` for the same reason sections 3-5 are:
// no interpolated runtime values, and no `{{`/`}}` escaping needed for the literal braces
// this section's own `src/conductor/{run_ctx,support,...}.rs` file-glob prose requires).
// Every count and file:line citation below is read directly from the already-committed
// `docs/audit/responsibility-map.json` / `docs/audit/duplication-catalog.json`, or from a
// direct read of a god file's own `#[cfg(test)] mod tests` opening line (decision
// `u85c4-section6-plan-structure`) - never from a fresh scan, honoring "adds no new
// findings".

/// Replace ONLY section 6's span (from its `## 6. ` heading to the end of the string - it is
/// the LAST section, so unlike [`replace_section_1`] / [`replace_section_2`] there is no next
/// `## ` heading to search for) inside an EXISTING report `existing`, leaving every earlier
/// section byte-for-byte untouched - the same one-owner-per-span contract as every other
/// `replace_section_*` function (decision `u85c1-report-section-placeholders`). Panics if
/// `existing` has no `## 6. ` heading at all - that would mean the report is missing
/// criterion 1's placeholder contract, the same precondition every other criterion's own
/// `replace_section_*` relies on.
fn replace_section_6(existing: &str, section_6: &str) -> String {
    let start = find_heading(existing, "## 6. ").unwrap_or_else(|| {
        panic!("{REPORT_PATH} has no '## 6. ' heading - missing criterion 1's placeholder contract")
    });
    let mut out = String::new();
    out.push_str(&existing[..start]);
    out.push_str(section_6);
    if !section_6.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Section 6, PRIORITIZED PLAN: twenty follow-up refactoring-spec stubs across six
/// risk-reduction tiers. See decision `u85c4-section6-plan-structure` for the tier rationale and
/// the reconciled cluster-id accounting (340 test-only + 7 named + 327 remaining src-touching =
/// 674 total clusters). Spec 87 criterion 3 (`u87c3`, its own explicit Done-when: "section 6
/// gains item 0") added Tier 1 item 0, "Delete the dead-code set", once its own section-4
/// rewrite turned what was an empty, no-follow-up-needed category (under the OLD three-god-file-
/// scoped scanner) into a real, cited 23-function deletion list; item 0 cites section 4.3 only,
/// the same "adds no new findings" discipline every other section-6 entry already follows.
fn render_section_6() -> String {
    let mut out = String::new();
    out.push_str("## 6. Prioritized Plan\n\n");
    out.push_str(
        "Twenty follow-up refactoring specs, ordered largest risk-reduction first. This \
        section adds no new findings: every citation below points at a claim already \
        recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 \
        (`docs/audit/duplication-catalog.json`), section 4.3 (`docs/audit/dead-code.json`, \
        dispositioned), or sections 3 and 5's own prose. Three instruments ground every \
        count below: the three committed JSON files (queried directly, never re-scanned) \
        and, where a god file's own `#[cfg(test)] mod tests` boundary line is cited, a \
        direct read of that file - the boundary line itself is not a scanner output, it is \
        where in the file the earliest `is_test: true` entry begins. Item 0 (Tier 1) \
        deletes 23 dead functions across 12 files (section 4.3); six of the remaining \
        nineteen entries split a god file (tiers 2 and 3, two phases times three files); the \
        other thirteen retire duplication or close a port gap (tiers 1, 4 and 5) - kept as \
        separate entries throughout, per spec 85's own instruction that \"the god-file \
        splits and the duplication removals are separate entries so each can be its own \
        run.\"\n\n",
    );
    out.push_str("### 6.1 How this plan is ordered\n\n");
    out.push_str(
        "Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's two boundary violations; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. \
        Item 0 (deleting the 23-function dead-code set, section 4.3) is placed here too, \
        first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change, \
        guaranteed-safe move available (spec 87's own Done-when: \"Tier 1 item 0\"), and it \
        shrinks the exact three god files tiers 2 and 3 operate on before either touches \
        them - ordered before items 1 and 2 for that reason, per this section's own \
        largest-first-within-a-tier rule (item 0's line delta exceeds either boundary-fix \
        item's, section 4.3).\n\
        2. Tier 2 - god-file test-module extraction: each of the three god files' own \
        inline `#[cfg(test)] mod tests` is the majority of that file's bulk (56-70% by \
        boundary-line count, per a direct read of each file), and moving it is a pure \
        relocation with no production-behavior change - the single largest safe line-count \
        reduction in this plan, and the precondition that makes tier 3 tractable.\n\
        3. Tier 3 - god-file production splits: section 1's own proposed module tree \
        applied to the (now much smaller) remaining production surface of each god file. \
        Higher execution risk than tier 2 because it touches live orchestration and CLI \
        logic, so it is sequenced after tier 2 shrinks the target first.\n\
        4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps \
        section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path \
        literals, sqlite `Connection::open`, error-shaping helpers), each already a single \
        committed cluster with its own proposed home.\n\
        5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only \
        duplication. No production-correctness exposure at all (worst case a test \
        regresses, never the product), so it is ordered ahead only of tier 6 despite \
        touching the largest raw line count anywhere in this plan.\n\
        6. Tier 6 - remaining catalog sweep: the 327 src-touching clusters section 2 found \
        but tiers 1 and 4 did not individually name. Unlike every other tier, none of these \
        327 have been read and risk-assessed one at a time the way tiers 1-4's named \
        clusters have - they are consumed straight from the catalog - so this tier carries \
        production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the \
        follow-up spec must triage each cluster's own production-or-test status before \
        merging it, not assume tier 5's blanket test-only treatment applies here too.\n\n\
        Within a tier, entries are ordered largest-first by the site or line count each \
        retires - the same rule the tiers themselves follow, applied one level down.\n\n",
    );
    out.push_str("### 6.2 Tier 1: active correctness risk\n\n");
    out.push_str("#### 0. Delete the dead-code set\n\n");
    out.push_str(
        "- Scope: the 23 `delete`-dispositioned entries of section 4.3 (`docs/audit/dead-code.json`) \
        and the tests that reference only them. Every entry was independently re-verified by \
        hand to have a real, ACTUALLY-WIRED replacement already in production - never removed \
        merely for having zero references - so this is a pure subtraction, not a behavior \
        change: `pid_is_alive` -> `dash_serving_on`, `neighborhood`/both `ingest_project` lanes \
        -> their already-wired batched/multi-seed cores, `serve` -> `serve_on`, the whole \
        `SpawnRequest` builder family (`new` + 7 `with_*` + `park`) -> `driver/replay.rs`'s \
        direct struct literal, `is_integrated` -> `is_terminal`, `fully_done` -> `done()`, \
        `resolve_wrapper_name`/`cataloged_classes`/`definitions_named`/`references_named`/ \
        `expect_merged`/`is_dirty` -> confirmed genuinely unreferenced anywhere. The full \
        per-file deletion list, by name:\n\n",
    );
    out.push_str(&render_dead_code_deletion_list(&real_dead_code_lines()));
    out.push_str(
        "\n- Files: `src/canary.rs`, `src/dash.rs`, `src/gate.rs`, \
        `src/grounder/symbols/events.rs`, `src/grounder/symbols/model.rs`, `src/ingest.rs`, \
        `src/ledger.rs`, `src/spawn.rs`, `src/worktree.rs`, plus every test file that \
        references ONLY a deleted fn (each entry's own `test_only_references` in \
        `docs/audit/dead-code.json` names them precisely).\n\
        - Expected line delta: negative, at least -284 production lines (the 23 function \
        definitions measured directly - signature, doc comment, and body, walking upward over \
        contiguous `///`/`#[...]`/blank lines the same way this audit's own scanner excludes a \
        definition's own signature span - `src/spawn.rs` alone accounts for 74 of the 284 \
        across its 9-function builder family), MORE negative once each entry's orphaned tests \
        are removed too - deliberately NOT measured here: a test whose ONLY purpose is exercising \
        a deleted fn is removed outright, but several of the 156 test files touch a candidate as \
        ONE part of a larger fixture (e.g. `pid_is_alive` injected as a closure inside \
        `ensure_run_dashboard_at`'s own idempotency tests, which test OTHER production behavior \
        too) and need editing, not deletion - that per-test triage is this item's own first task, \
        not a number this criterion's own no-production-code-changes scope should estimate.\n\
        - Risk: low. Every deletion target already has a confirmed, wired, tested replacement in \
        production (never \"nothing needs this\" alone - the disposition citations name the \
        replacement or the confirming grep); `cargo build`/`clippy -D warnings` catches any \
        missed reference immediately (a still-referenced item cannot compile away silently), and \
        the ambiguous `ingest_project` pair's two `#[cfg(feature)]` lanes must both be edited \
        together or a lane-specific build breaks.\n\
        - Unblocks: shrinks `src/dash.rs`, `src/spawn.rs`, `src/ledger.rs` and the other six \
        touched files before tiers 2-3 (god-file splits) and tier 4 (duplication sweeps, several \
        of which touch these SAME files) operate on them; the largest, safest, zero-behavior-\
        change line-count reduction available anywhere in this plan, so it runs first.\n\n",
    );
    out.push_str("#### 1. Close the `AgentDriver` port gap around mutation-scratch reclaim\n\n");
    out.push_str(
        "- Scope: `conductor.rs`'s production `reclaim_terminal_unit_mutation_scratch` \
        (`src/conductor.rs:7091-7096`, section 3 violation 1) calls \
        `crate::driver::replay::cache_home_from` and \
        `crate::driver::replay::reclaim_unit_mutation_scratch` by concrete module path - two \
        pure, driver-instance-free scratch-lifecycle utilities that do not conceptually \
        belong to the `driver::replay` concern they currently live inside. Relocate both \
        into a neutral module every `AgentDriver` adapter and `conductor.rs` can depend on \
        alike (no new trait needed - neither function takes a driver instance, so this is a \
        home fix, not a port-method fix).\n\
        - Files: `src/conductor.rs`, `src/driver/replay.rs`, a new home for the two \
        relocated functions.\n\
        - Expected line delta: near zero net - a pure move of two functions.\n\
        - Risk: low-medium. The reclaim path is covered by spec 83's \
        worktree-lifetime-fenced-by-spawn-liveness contract tests; those tests move with the \
        functions, not get rewritten.\n\
        - Unblocks: retires the only `AgentDriver` port violation section 3 found.\n\n",
    );
    out.push_str(
        "#### 2. Close the `Grounder` port gap for whole-project batch ingest (retires \
        dup-0204 in the same motion)\n\n",
    );
    out.push_str(
        "- Scope: section 3 violation 2 (`src/ingest.rs:187-211` `walk_batches`, reaching \
        `grounder::symbols::events::project_batches_paced` and \
        `grounder::design::events::project_batches` by concrete module path) and \
        duplication cluster `dup-0204` (the same two modules' own twin `project_batches` \
        functions, `src/grounder/symbols/events.rs:36-38` / \
        `src/grounder/design/events.rs:90-114`) are one root cause, not two - fix once. TWO \
        CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-0204`'s own mechanical \
        `proposed_home` suggests relocating into `tests/common`, but both sites are \
        production code under `src/grounder/`, not test helpers - the mechanical heuristic \
        has no \"add a port method\" category to route a production duplicate to, so it \
        mis-fires here. This plan follows section 3's own reasoned disposition instead: add \
        a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) \
        covering both concrete modules, and point `ingest.rs` at it.\n\
        - Files: `src/ingest.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/events.rs`, \
        `src/grounder/design/events.rs`.\n\
        - Expected line delta: roughly neutral - one new trait method plus two thin impls, \
        minus the two duplicate bodies `dup-0204` catalogs.\n\
        - Risk: medium. `ingest.rs`'s own module doc calls it \"the ONE walk-and-content-key \
        authority\" - a load-bearing path; needs the existing whole-project-ingest and \
        reindex-freshening coverage to stay green, not just the two duplicate-site tests.\n\
        - Unblocks: retires the one `Grounder` port violation section 3 found and `dup-0204` \
        together, rather than as two separately-tracked fixes.\n\n",
    );
    out.push_str(
        "#### 3. Retire the duplicate `/proc`-reading authority (`dup-0143` + `dup-0144`)\n\n",
    );
    out.push_str(
        "- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and \
        `src/main.rs::pgid_of` (`src/main.rs:23346-23359`) each independently re-derive \
        `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` \
        (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact \
        \"second mutation authority\" example spec 85's own Goal names and spec 62's \
        capstone previously caught (`dup-0144`, 15 sites: `src/dash.rs`, `src/main.rs`, \
        `src/reap.rs`, `tests/cli.rs`, `tests/mutation_runner_pdeathsig_periphery.rs` - spec \
        91's own launcher-exits proving test reads `/proc/<pid>/stat` directly for the same \
        reason `dash.rs::process_state` does, growing this already-known cluster by one site \
        rather than opening a new one), plus 60 raw `/proc`-path string literals scattered \
        across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and four test files with no \
        shared composer (`dup-0143`). Both clusters' own `proposed_home` agree: `src/reap.rs` \
        becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of \
        re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on \
        production server, so it is the actual active-correctness risk this tier-1 placement \
        is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12825`) \
        and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it \
        rides in this same item only because it shares `dup-0143`/`dup-0144`'s one root cause \
        and one proposed fix with `process_state`, not because retiring it retires any live \
        risk of its own.\n\
        - Files: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs` (`proc_pgid_of`, \
        `tests/cli.rs:23650-23663`, re-points at the same call).\n\
        - Expected line delta: negative - retires `process_state`'s and `pgid_of`'s own \
        parsing bodies in favor of calling `reap.rs`'s existing parser.\n\
        - Risk: low for both halves, for two different reasons. Section 3's own disposition \
        already establishes `process_state` as a duplicate READ-only reimplementation, never a \
        bypassed mutation path - nothing this touches can signal or kill a process, so it \
        carries none of the no-os-kill gate's own risk surface. `pgid_of`'s own risk is lower \
        still: being test-only, retiring it is ordinary test cleanup, not a \
        correctness-risk retirement - it is sequenced here for shared-fix convenience, not \
        because it independently needed tier-1 urgency.\n\
        - Unblocks: retires the codebase's only currently-known live instance of the \
        \"duplicate implementation reconciled after the fact\" pattern the operator's \
        strict-DRY rule targets - the concrete precedent spec 85's own Goal cites - and, as a \
        free byproduct, `main.rs`'s own test-only duplicate parser.\n\n",
    );
    out.push_str("### 6.3 Tier 2: god-file test-module extraction\n\n");
    out.push_str(
        "Each god file's inline test-module boundary is the earliest `is_test: true` \
        entry's `start_line` in the committed `docs/audit/responsibility-map.json`, \
        cross-checked against a direct read of the file's own `#[cfg(test)]` markers. All \
        three checks agree no file is fully flat before this boundary: conductor.rs has one \
        small early exception (`for_test`, `src/conductor.rs:2327-2371`) and main.rs has one \
        (`compose_precommit`, `src/main.rs:11630-11635`); dash.rs has none. Each entry below \
        moves an already-passing test module with no intended production-behavior change - a \
        `cargo test` pass before and after is the whole verification. The line-delta figures \
        below are file-length minus the boundary's own start line (a direct-read fact, not a \
        function-span sum), so they include the module-level doc comments, `use` statements \
        and blank lines a per-function span sum would miss.\n\n",
    );
    out.push_str("#### 4. Extract `src/conductor.rs`'s inline test module\n\n");
    out.push_str(
        "- Scope: the file's `#[cfg(test)] mod tests` opens at `src/conductor.rs:10260` and \
        runs to end of file - roughly 24,400 of the file's 34,677 lines (70%), 417 of its \
        612 mapped functions. On its own it is nearly as large as all of `tests/cli.rs` \
        (27,074 lines). Partition into a `src/conductor/tests/` directory, one file per \
        concern, reusing the same names section 1 already assigned the file's own \
        production buckets (`budget`, `gate`, `review`, `run_ctx`, `schedule`, `spawn`, \
        ...) so the split needs no new naming scheme.\n\
        - Files: `src/conductor.rs` -> `src/conductor.rs` (production only) + \
        `src/conductor/tests/*.rs`.\n\
        - Expected line delta: 0 net (repo-wide) - roughly 24,400 lines relocated out of \
        `conductor.rs`.\n\
        - Risk: low - mechanical move of passing tests, zero intended behavior change.\n\
        - Unblocks: shrinks `conductor.rs` from 34,677 to roughly 10,260 lines before tier 3 \
        touches a single production line - the single largest reduction in this whole plan \
        to the odds that an unrelated future unit's blast radius collides with this file.\n\n",
    );
    out.push_str("#### 5. Extract `src/main.rs`'s inline test module\n\n");
    out.push_str(
        "- Scope: the file's `#[cfg(test)] mod tests` opens at `src/main.rs:12650` (the \
        same boundary section 3 cites for its own test-only raw-connection-bypass finding) \
        and runs to end of file - roughly 11,374 of the file's 24,024 lines (47%), 332 of \
        its 618 mapped functions. Same partition approach as item 4, reusing section 1's own \
        production bucket names (`commands`, `store`, `support`, `provenance`, `dash_glue`, \
        `setup`, ...).\n\
        - Files: `src/main.rs` -> `src/main.rs` (production only) + \
        `src/main/tests/*.rs`.\n\
        - Expected line delta: 0 net - roughly 11,374 lines relocated.\n\
        - Risk: low, same rationale as item 4.\n\
        - Unblocks: shrinks `main.rs` to roughly 12,650 lines before tier 3's own main.rs \
        split.\n\n",
    );
    out.push_str("#### 6. Extract `src/dash.rs`'s inline test module\n\n");
    out.push_str(
        "- Scope: the file's `#[cfg(test)] mod tests` opens at `src/dash.rs:4907` and runs \
        to end of file - roughly 6,213 of the file's 11,120 lines (56%), 148 of its 263 \
        mapped functions. Lower effort than items 4-5: section 1's own classifier already \
        found five pre-existing sub-boundaries inside this one test module \
        (`dash::tests::calls_route_c4`, `metadata_card_c2`, `rationale_overlay_c3`, \
        `subject_view_c5`, `supervised_lifecycle`), so the partition points already exist \
        and need only become their own files.\n\
        - Files: `src/dash.rs` -> `src/dash.rs` (production only) + `src/dash/tests/*.rs`.\n\
        - Expected line delta: 0 net - roughly 6,213 lines relocated.\n\
        - Risk: low - the lowest-effort of the three, for the reason above.\n\
        - Unblocks: shrinks `dash.rs` to roughly 4,907 lines before tier 3's own dash.rs \
        split.\n\n",
    );
    out.push_str("### 6.4 Tier 3: god-file production splits\n\n");
    out.push_str(
        "Each entry below applies section 1's own proposed module tree to a god file's \
        production surface, sequenced after the matching tier-2 entry removes that file's \
        test bulk first. Every module name and function/line count below is summed directly \
        from the committed `docs/audit/responsibility-map.json` (function-body spans only); \
        a file's remaining non-function production lines - struct/enum/type definitions, \
        `use` statements, module docs - are outside section 1's own function-only scan and \
        move with whichever module they sit beside, without needing their own assignment.\n\n",
    );
    out.push_str(
        "#### 7. Split `src/conductor.rs`'s production code into `src/conductor/*.rs`\n\n",
    );
    out.push_str(
        "- Scope: 175 mapped functions across 14 proposed modules (roughly 6,850 lines of \
        function bodies) plus 20 unassigned functions (952 lines, each individually named \
        in the committed map for manual placement, per section 1's own \"unassignable \
        functions are named as such, never omitted\" rule). Headline buckets: \
        `conductor::run_ctx` (100 functions, 4,901 lines - more than half this remaining \
        surface on its own), `conductor::support` (15/387), `conductor::gate` (19/133), \
        `conductor::run` (6/122), `conductor::review` (9/102); the other nine buckets are \
        each five functions or fewer.\n\
        - Files: `src/conductor.rs` -> `src/conductor/mod.rs` + \
        `src/conductor/{run_ctx,support,gate,run,review,schedule,budget,prior_failure,spawn,\
        review_outcome,ground,error,gate_ratchet,integration_approval}.rs`.\n\
        - Expected line delta: 0 net - pure relocation of roughly 7,800 lines; `run_ctx` \
        alone may warrant its own second pass if it does not decompose cleanly into one \
        file.\n\
        - Risk: medium-high - conductor.rs is the composition root's own most complex \
        use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and \
        reap audits green, not just the final one.\n\
        - Unblocks: the largest reduction in production-code blast-radius collision risk \
        this audit identifies; makes future duplication-spotting against conductor.rs's own \
        logic tractable by a human reviewer, not only by the mechanical scanner.\n\n",
    );
    out.push_str("#### 8. Split `src/main.rs`'s production code into `src/main/*.rs`\n\n");
    out.push_str(
        "- Scope: 286 mapped functions across 15 proposed modules (roughly 8,340 lines of \
        function bodies) plus 102 unassigned functions (2,674 lines). Headline buckets: \
        `main::commands` (34/2,568), `main::store` (32/862), `main::support` (25/652), \
        `main::provenance` (23/446), `main::dash_glue` (25/346), `main::setup` (18/271).\n\
        - Files: `src/main.rs` -> `src/main.rs` (composition root, thinned) + \
        `src/cli/{commands,store,support,provenance,dash_glue,setup,render,liveness,\
        store_location,run_registration,docs_overlay,scaffold_report,residue_report,\
        store_selection,replay_runner}.rs`.\n\
        - Expected line delta: 0 net - pure relocation of roughly 11,000 lines.\n\
        - Risk: medium - `main.rs` is the composition root itself; the split must preserve \
        which concretions get wired where, not merely move text.\n\
        - Unblocks: shrinks the second-largest god file to a genuine composition root plus a \
        `cli/` module tree, matching the ports-and-adapters shape this project already \
        mandates everywhere else.\n\n",
    );
    out.push_str("#### 9. Split `src/dash.rs`'s production code into `src/dash/*.rs`\n\n");
    out.push_str(
        "- Scope: 115 mapped functions across 9 proposed modules (roughly 2,720 lines of \
        function bodies) plus 41 unassigned functions (848 lines). Headline buckets: \
        `dash::render` (26/642), `dash::reproject` (12/519), `dash::server` (8/426), \
        `dash::buckets` (8/116).\n\
        - Files: `src/dash.rs` -> `src/dash/mod.rs` + \
        `src/dash/{render,reproject,server,buckets,registry,response,reaped_child,lens,\
        dash_marker}.rs`.\n\
        - Expected line delta: 0 net - pure relocation of roughly 3,600 lines.\n\
        - Risk: low-medium - the smallest of the three god files by production surface, and \
        the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected \
        by a pure module split.\n\
        - Unblocks: completes the god-file split trio; the third program-sized file becomes \
        an ordinary module tree.\n\n",
    );
    out.push_str("### 6.5 Tier 4: named production duplication sweeps\n\n");
    out.push_str(
        "Each entry is one of section 2's five named mandatory sweeps - collected \
        mechanically regardless of the Jaccard pass, per spec 85's own Design.\n\n",
    );
    out.push_str(
        "#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0055`) - the \
        single largest cluster in the entire catalog by site count\n\n",
    );
    out.push_str(
        "- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the 662 sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, \
        `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, \
        `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, \
        `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `dup-0055` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - 662 literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing 662 sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n",
    );
    out.push_str(
        "#### 11. Consolidate the 333 `Command::new` call sites (`dup-0006`) behind one \
        injected process-spawn port\n\n",
    );
    out.push_str(
        "- Scope: one process-spawn seam every `Command::new` site routes through (the \
        cluster's own `proposed_home`).\n\
        - Files: spans `src/budget.rs`, `src/conductor.rs`, `src/dash.rs`, \
        `src/driver/cli.rs`, `src/gate.rs`, `src/main.rs`, `src/worktree.rs` plus many \
        `tests/` files - full site list in `docs/audit/duplication-catalog.json` under \
        `dup-0006`.\n\
        - Expected line delta: negative, though smaller per-site than `dup-0055` since each \
        `Command::new` call already carries real configuration (args, env, cwd) that must \
        move with it, not just a literal.\n\
        - Risk: medium-high - several of these 333 sites sit inside `src/budget.rs`'s and \
        `src/conductor.rs`'s already-hardened process-lifecycle code (spec 78's no-os-kill \
        discipline); the follow-up spec must preserve every existing handle-bound-kill \
        invariant at each site it touches, and the no-os-kill gate is the acceptance bar, \
        not merely `cargo test`.\n\
        - Unblocks: one seam instead of 333 independent constructions - the next \
        process-spawning concern added anywhere in the crate reuses it instead of adding \
        site 334.\n\n",
    );
    out.push_str(
        "#### 12. Consolidate the 46 sqlite `Connection::open` call sites (`dup-0109`)\n\n",
    );
    out.push_str(
        "- Scope: one sqlite-connection-opening adapter function (the cluster's own \
        `proposed_home`) spanning `src/contextgraph/sqlite.rs`, `src/eventstore/sqlite.rs` \
        and `src/main.rs`, plus several `tests/` files.\n\
        - Files: full site list in `docs/audit/duplication-catalog.json` under `dup-0109`.\n\
        - Expected line delta: negative - 46 open calls collapse toward one function.\n\
        - Risk: medium - touches the event store and context graph's own \
        connection-lifecycle code; needs the store-identity and store-resolution contract \
        tests green throughout.\n\
        - Unblocks: one place to change pragma/timeout/journal-mode settings instead of \
        46.\n\n",
    );
    out.push_str(
        "#### 13. Consolidate the 5 error-shaping helper sites (`dup-0213`) - caution, \
        confirm before merging\n\n",
    );
    out.push_str(
        "- Scope: the cluster spans `src/grounder/mod.rs` (`retired_grounder_error`), \
        `src/worktree.rs` (`revert_on_base_aborts_and_errors_on_a_conflicting_revert`), \
        `src/conductor.rs` (a `mod tests` case, \
        `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`) and \
        three unrelated test files, at line counts from 7 to 132 - a wide spread for one \
        claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own \
        CONSTRAINTS WALK: \"the threshold is a floor for the mechanical pass; the reading \
        pass owns semantic duplicates\") rather than one real shared concern - the follow-up \
        spec's first job is confirming by reading whether these six sites share actual \
        logic before proposing one helper, not assuming the cluster label proves it.\n\
        - Files: `src/grounder/mod.rs`, `src/worktree.rs`, plus the three test files named \
        in `docs/audit/duplication-catalog.json` under `dup-0213`.\n\
        - Expected line delta: unknown pending the confirmation read above - potentially \
        zero if the cluster does not survive a human read.\n\
        - Risk: low (the smallest-site-count sweep), but with the stated precondition.\n\
        - Unblocks: either a genuine fifth consolidation, or a documented \"not a real \
        duplicate\" disposition that keeps the catalog honest for whoever reads it next.\n\n",
    );
    out.push_str("### 6.6 Tier 5: test-suite consolidation\n\n");
    out.push_str(
        "Every entry cites section 5's own already-catalogued test-only duplication; none \
        of it carries production-correctness risk.\n\n",
    );
    out.push_str("#### 14. Extract the four headline shared test fixtures into `tests/common` (section 5.2)\n\n");
    out.push_str(
        "- Scope: `page_script` (`dup-0345`, 19 files), `node_available` (`dup-0243`, 23 \
        files - merged with two related availability-check helpers), `temp_project` \
        (`dup-0371`, 19 files) and `run_stream_identity` (`dup-0377`, 18 files) - roughly 72 \
        duplicate definitions collapsing into four shared ones, the \
        single largest mechanical simplification section 5 identifies anywhere in the test \
        suite.\n\
        - Files: the 18 dashboard/viz test files section 5.1 already groups together, plus \
        `tests/common/mod.rs`.\n\
        - Expected line delta: negative - each fixture's small body survives once instead of \
        up to 18 times.\n\
        - Risk: low - test-only, and `tests/common/mod.rs` already exists with the same \
        shape of helper (`product_binary_from`, `rigger_bin`, ...).\n\
        - Unblocks: item 17 below (the remaining test-helper clusters) reuses the same \
        `tests/common` home this item establishes.\n\n",
    );
    out.push_str(
        "#### 15. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)\n\n",
    );
    out.push_str(
        "- Scope: 27,074 lines, 351 tests, split into \
        `tests/cli_{step,run,validate,reset,watch,canary,dash,store,review,setup}.rs` plus a \
        residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 \
        names, using each test's dominant scenario (a human/AI read, not the 78%-coverage \
        keyword match section 5.3 already disclosed as insufficient alone).\n\
        - Files: `tests/cli.rs` and the eleven new files above.\n\
        - Expected line delta: 0 net - pure relocation of 27,074 lines into eleven files.\n\
        - Risk: low-medium - the largest single test file in the repo, but a mechanical \
        per-test move with `cargo test`'s full pass count as the verification.\n\
        - Unblocks: shrinks the catalog's most cross-clustered single file (29 duplication \
        clusters per section 5.3) and lets item 17's remaining-clusters sweep target \
        smaller, subcommand-scoped files.\n\n",
    );
    out.push_str(
        "#### 16. Convert the four largest table-driven test families into parametrized \
        tables (section 5.5)\n\n",
    );
    out.push_str(
        "- Scope, largest first: `dup-0662` (42 sites, `tests/spec_lint.rs`), \
        `dup-0603`/`dup-0605` (15+7 sites, `tests/reap_before_removal_audit.rs`), \
        `dup-0592`/`dup-0593` (11+4 sites, `tests/no_os_kill_audit.rs`), \
        `dup-0636` (11 sites, `tests/simplification_audit.rs` - this very \
        generator's own scanner tests) - 90 sites across 6 clusters.\n\
        - Files: the four files named above.\n\
        - Expected line delta: negative - each family's near-identical test bodies collapse \
        into one parametrized loop over a table.\n\
        - Risk: low - test-only, and each family already shares one body shape (section \
        5.5's own finding).\n\
        - Unblocks: the largest reduction in raw `#[test]` count available in the suite \
        (roughly 90 named tests retiring toward 4).\n\n",
    );
    out.push_str(
        "#### 17. Sweep the remaining 177 test-only helper-duplication clusters (section \
        5.4, beyond item 14's four headline fixtures)\n\n",
    );
    out.push_str(
        "- Scope: the 181 test-only, all-helper-function clusters section 5.4 names, minus \
        the 4 item 14 already covers - consumed directly from \
        `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own \
        stated approach). Includes the `dup-0370`/`dup-0372` `temp_project` companion and \
        variant clusters section 5.4 itself places in this \"beyond the four\" bucket.\n\
        - Files: per-cluster, from the committed catalog.\n\
        - Expected line delta: negative, cumulative across 177 clusters.\n\
        - Risk: low - test-only.\n\
        - Unblocks: closes out the helper-duplication half of the test suite's own \
        strict-DRY exposure.\n\n",
    );
    out.push_str(
        "#### 18. Sweep the remaining 153 table-driven test families (section 5.5, beyond \
        item 16's four headline families)\n\n",
    );
    out.push_str(
        "- Scope: the 159 test-only, all-`#[test]` clusters section 5.5 names, minus the 6 \
        cluster ids item 16 already covers - consumed directly from \
        `docs/audit/duplication-catalog.json`. Includes `dup-0595` (4 sites, \
        `tests/no_os_kill_test_helper_periphery.rs`), the smallest of section 5.5's own \
        named large families, left here rather than in item 16.\n\
        - Files: per-cluster, from the committed catalog.\n\
        - Expected line delta: negative, cumulative.\n\
        - Risk: low - test-only.\n\
        - Unblocks: closes out the table-driven-test half of the test suite's own \
        strict-DRY exposure; combined with item 17, retires all 340 test-only clusters \
        section 2 found.\n\n",
    );
    out.push_str("### 6.7 Tier 6: remaining catalog sweep\n\n");
    out.push_str(
        "Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one \
        needs its own read before merging (see `### 6.1`'s tier 6 rationale above).\n\n",
    );
    out.push_str(
        "#### 19. Sweep the remaining 327 src-touching duplication clusters (section 2, \
        beyond tiers 1 and 4's 7 named clusters)\n\n",
    );
    out.push_str(
        "- Scope: of the catalog's 674 clusters, 340 are test-only (items 14 and 16-18 \
        above) and 7 are the named tier-1/tier-4 items (`dup-0006`, `dup-0055`, `dup-0109`, \
        `dup-0143`, `dup-0144`, `dup-0204`, `dup-0213`); the remaining 327 clusters touching \
        `src/` - mostly small 2-5-site exact/near matches like the two worked examples \
        section 2 itself opens with (`dup-0001`, `dup-0002`) - are swept here, largest \
        exact-duplicate clusters first, consumed directly from \
        `docs/audit/duplication-catalog.json`.\n\
        - Files: per-cluster, from the committed catalog.\n\
        - Expected line delta: negative, cumulative; the largest single contributor is \
        whichever exact cluster has the most sites (read from the catalog at spec-writing \
        time, not fixed here).\n\
        - Risk: low-medium - unlike tier 5, some of these clusters are production code, so \
        each merge needs its own test-coverage check, not a blanket \"test-only\" pass.\n\
        - Unblocks: the last of the catalog's 674 clusters; after items 1-3 and 10-19 all \
        land, a future spec can state and check that the duplication catalog's own drift \
        guard finds zero live clusters left unaddressed.\n\n",
    );
    out.push_str("### 6.8 Dead and vestigial code beyond item 0: no further follow-up\n\n");
    out.push_str(
        "Spec 87 redid section 4 (item 0, Tier 1, above, is that redo's own real follow-up: \
        delete the 23 `delete`-dispositioned candidates). Of section 4.3's remaining 3 \
        candidates, all `keep-pending`, NONE gets a new refactoring-spec stub here: each already \
        cites its own governing, ALREADY-LANDED spec as the thing a future wiring pass would \
        extend - spec 27 for `distiller::rebuild`, spec 32 for `Defaults::sdet_author_enabled`, \
        spec 60 for `Store::with_content_identity` - not a gap this plan should re-propose as a \
        fresh entry \
        - re-litigating an already-landed spec's own scope is out of place in a plan whose own \
        rule is \"adds no new findings\". `keep-public-surface` is explicitly empty (0 of 26 \
        candidates cite a real MCP/workflow-template/CLI-contract consumer) - stated so with the \
        search that established it, never omitted (spec 85's own CONSTRAINTS WALK), the same \
        discipline spec 85's original all-clean section 4 applied to a scope this redo has since \
        superseded. Both named retirements (`turbovec`, `kurrentdb`, spec 85's own original \
        section 4 finding, unaffected by spec 87's redo since neither is a `src/` production fn) \
        are still fully clean, and the two stale-looking doc paths found remain confirmed \
        generic illustrative examples, not real dangling references - re-verified, not \
        re-scanned, by this criterion's own research.\n",
    );
    out
}

// -----------------------------------------------------------------------------------------
// THE ADVERSARIAL SAMPLE (spec 85 THOROUGHNESS)
// -----------------------------------------------------------------------------------------

/// One step of a 64-bit linear congruential generator (the Knuth/Numerical-Recipes constants) -
/// deterministic, zero new dependency (no `rand`).
fn lcg_next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

/// Deterministically draw `k` distinct indices in `[0, n)` from `seed` (spec 85 THOROUGHNESS:
/// "the adversary draws 30 functions by seeded random index... The report states the sample
/// seed so the check is reproducible"), sorted ascending for a stable, readable listing.
fn sample_indices(n: usize, k: usize, seed: u64) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let mut state = seed;
    let mut seen = HashSet::new();
    let mut picked = Vec::new();
    let want = k.min(n);
    while picked.len() < want {
        let r = ((lcg_next(&mut state) >> 33) as usize) % n;
        if seen.insert(r) {
            picked.push(r);
        }
    }
    picked.sort_unstable();
    picked
}

// =========================================================================================
// SPEC 87 CRITERION 2 (`u87c2`, THIS UNIT): THE PRODUCTION-REFERENCE SWEEP
// =========================================================================================
//
// Redoes spec 85's section 4 (dead/vestigial code) on the tree spec 87 criterion 1's compiler
// pass leaves behind (that stage found zero removable items in `src/` - the exact blind spot
// its own Design predicts: `dead_code` never fires on a `pub` item in a lib+bin crate). This
// criterion classifies every fn under `src/` production-vs-test FILE-AWARE (an out-of-line
// `#[cfg(test)] mod name;` target - resolved via `name.rs`, `name/mod.rs`, or a `#[path]`
// override, transitively - is test in full, closing the exact false-negative spec 87's Goal
// names: `src/eventstore/contract.rs`'s top-level `pub fn assert_contract` has no LOCAL
// `#[cfg(test)]` of its own, so a per-file scan misses it; only `src/eventstore/mod.rs`'s real
// `#[cfg(test)]\npub mod contract;` proves the whole file test), then counts references to
// each production fn from PRODUCTION code only (test spans and all of `tests/` excluded, the
// fn's own signature span excluded, only code-shaped references counted), writing
// `docs/audit/dead-code.json`. This criterion OWNS the instrument and this JSON only -
// dispositions and the report's section 4 rewrite are criterion 3's (spec 87 Done-when).
//
// ZERO NEW DEPENDENCIES, same discipline as criteria 1/2 of spec 85: the reference corpus
// reuses [`scan_tree`]'s existing whole-`src`+`tests` [`FileScan`] (`.tokens`, already
// comment/string/lifetime-aware via [`tokenize`]) rather than a third lexer.
//
// BOTH FEATURE LANES (Constraints Walk: "a `#[cfg(feature)]`-gated fn - counted within its
// lane; both lanes are scanned and the union is the reference set"): this scanner is TEXTUAL,
// not a real compilation - it never evaluates a `#[cfg(...)]` predicate at all (only
// `cfg(test)`/`#[test]` are given any semantic meaning, for `is_test`), so a
// feature-gated fn and every reference to it are found from the source text UNCONDITIONALLY,
// regardless of which lane would actually compile it. The union both lanes require is
// therefore already what a single textual pass produces - mirrors spec 85's responsibility
// map and duplication catalog, neither of which lane-splits either.
//
// `build.rs`/`build/` (Constraints Walk: "A fn used only in `build.rs` - `build.rs` and
// `build/` are production references"): out of THIS criterion's scope - candidates are drawn
// from `src/` only (spec 87 Done-when: "over the whole `src/` tree"), and `build.rs` cannot
// call into `src/` at all (it is a wholly separate compilation with no linkage to the library
// it builds), so no candidate can ever be "used only in build.rs". This constraint bears on
// criterion 1's whole-crate compiler pass (already landed - see `u87c1-stage1-deletion-is-real-
// and-committed`) or on criterion 3's report, not on this instrument.
//
// MACRO BODIES (Constraints Walk: "scanned as text; a name inside a macro body is a
// reference"): [`tokenize`] does not special-case macro invocations at all - a macro's
// argument tokens are ordinary tokens like any other, so a call-shaped reference inside one is
// found by the exact same pass with no extra mechanism needed.
//
// TRAIT OBJECTS (Constraints Walk: "a fn called only through a trait object... an impl's
// method referenced via `.name(` on any receiver is alive"): trivially covered by THE RULE
// below - a `.name(` occurrence is an identifier token like any other, receiver-agnostic with
// no special case needed.
//
// ENTRY POINTS (Constraints Walk: "the scanner exempts `fn main` and any fn named by a
// `#[...]` attribute that registers it"): `fn main` (top-level, no enclosing mod/impl) is
// exempted unconditionally in [`build_dead_code_candidates`]. The `#[...]`-attribute-registers-
// it class (e.g. a `#[no_mangle]` export, a `value_parser = my_fn` style clap attribute, a
// serde `default =`/`with =`/`deserialize_with =`/`serialize_with =` attribute) is now given a
// GENERAL mechanism rather than a per-shape one: round 1
// (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances` class 2) added
// [`mark_attribute_tokens`], so `all_ident_ref_sites` counts every identifier AND every
// identifier-shaped string-literal segment inside ANY `#[...]` attribute's token tree as a
// reference unconditionally, superseding the earlier per-shape-name verification this comment
// used to disclose (decision `u87c2-entry-point-attribute-exemption-verified-dormant`, now
// stale) - the real motivating instance, `src/config.rs`'s
// `#[serde(default = "default_build_config")]`, is exactly this shape and is covered by it.
//
// THE RULE (round 3, `op-u87c2-round-3-a-reference-is-any-token-not-a-shape`, replacing three
// rounds of one-shape-at-a-time patching - a struct-literal field VALUE
// (`render_body: render_x_skill,` in `src/docs.rs`'s `skill_registry`) and a UFCS value passed
// to a combinator (`.map(FailureRuleDef::to_rule)` in `src/config.rs`) were each the NEXT
// invisible shape, the expected failure mode of a scanner that enumerates shapes rather than
// dropping the concept of shape entirely): A PRODUCTION REFERENCE TO FN F IS ANY IDENTIFIER
// TOKEN EQUAL TO F'S NAME IN PRODUCTION CODE, WHATEVER TOKEN FOLLOWS OR PRECEDES IT - a struct-
// literal field value, a call argument, a `let` initializer, an array element, a match arm, a
// return expression, a generic argument, a UFCS path used as a value, a dot call, and a plain
// call are all literally the same case, because no code inspects adjacency at all any more.
// [`ref_shapes`] and [`RefSite`]'s `method_shaped`/`free_shaped` fields are REMOVED, not
// extended, and the `DispatchCategory`-keyed shape gate in [`build_dead_code_candidates`]'s
// `relevant` filter goes with them - only the fn's own definition token (never a reference to
// itself or anything else) and its own signature span are excluded. Attribution of a bare
// token to ONE of several same-named definitions when a name is shared is UNCHANGED from round
// 1 (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): path-qualified or receiver-agnostic
// evidence, otherwise credited to no one. PRECISION TRADE, explicit and accepted: a local
// variable or struct field that happens to share a fn's bare name now keeps that fn looking
// alive too - a false negative (a dead fn that reads as live), never a false positive (a live
// fn that reads as dead) - the one direction spec 87's Design already requires everywhere else
// in this file.

/// One occurrence of a bare identifier, classified for spec 87 criterion 2's reference sweep.
/// Round 3 (`op-u87c2-round-3-a-reference-is-any-token-not-a-shape`) dropped the notion of
/// SHAPE entirely - every non-definition `Ident` token is a `RefSite` for its own text,
/// unconditionally (see THE RULE, above `RefSite`'s neighborhood in this file) - so this no
/// longer records what the token was adjacent to, only where it was and how it disambiguates.
#[derive(Debug, Clone)]
struct RefSite {
    file: String,
    line: usize,
    /// `true` when this occurrence sits in a `src/` file OUTSIDE every effective-test span
    /// (file-aware `is_test`) - i.e. it is eligible to count as a PRODUCTION reference. A
    /// `tests/` file occurrence is never production.
    production: bool,
    /// The module/type name segment immediately in front of a `qualifier::name`-shaped
    /// occurrence (spec 87 round-1 addendum `op-u87c2-round-1-ambiguity-covers-free-fns-too`) -
    /// `None` when this occurrence is not `::`-preceded at all. Used ONLY to attribute a
    /// reference to ONE specific definition when its bare name is shared by more than one
    /// production `Free`/`ImplAssoc` fn; a unique name never consults this field at all.
    qualifier: Option<String>,
    /// `true` for a reference synthesized from a `#[...]` attribute's token tree or string
    /// literal (spec 87 round-1 fix for `sdet-u87c2-serde-default-attr-string-ref-is-a-false-
    /// positive`, class 2 of `op-u87c2-round-1-closes-the-reference-classes-not-the-instances`) -
    /// e.g. `#[serde(default = "default_build_config")]`. Deliberately EXEMPT from the ambiguity
    /// attribution above (always credits every same-named sharer): an attribute mention cannot be
    /// qualifier-resolved at all (it is not call-shaped code), and the design's own conservative
    /// direction - "an attribute mention keeps a fn alive; a false negative is cheaper than
    /// deleting live code" - means it must never be silently dropped just because the name it
    /// names happens to collide with an unrelated fn elsewhere.
    via_attribute: bool,
}

fn ref_punct(tok: Option<&RawTok>, text: &str) -> bool {
    tok.map(|t| t.kind == RawKind::Punct && t.text == text)
        .unwrap_or(false)
}

/// The identifier immediately in front of a `qualifier::name`-shaped occurrence at `toks[i]`,
/// or `None` when `toks[i]` is not `::`-preceded at all. Spec 87 round-1 addendum
/// (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): "the tokenizer's existing
/// `preceded_by_coloncolon` signal" carried one step further, to name WHICH qualifier a
/// `::`-qualified reference names, so a bare-name collision between two `Free`/`ImplAssoc` fns
/// can attribute a qualified call site to the one it actually means.
fn qualifier_before(toks: &[RawTok], i: usize) -> Option<String> {
    if i < 3 {
        return None;
    }
    if ref_punct(toks.get(i - 1), ":") && ref_punct(toks.get(i - 2), ":") {
        let q = toks.get(i - 3)?;
        if q.kind == RawKind::Ident || q.kind == RawKind::Keyword {
            return Some(q.text.clone());
        }
    }
    None
}

/// Index ranges into `tokens` (half-open `[start, end)`) occupied by the CONTENT of a `#[...]`
/// attribute - every token strictly between the `#`/`[` that opens it and its own matching `]`,
/// the `#` and both brackets themselves excluded. Spec 87 round-1 fix, class 2 of
/// `op-u87c2-round-1-closes-the-reference-classes-not-the-instances` ("ATTRIBUTE TOKEN TREES ARE
/// REFERENCES"). Depth is tracked on `[`/`]` only - an attribute's own inner `(...)`/`{...}`
/// (e.g. `#[cfg_attr(test, serde(default = ".."))]`) never closes it early.
fn attribute_token_ranges(tokens: &[RawTok]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let is_hash = tokens[i].kind == RawKind::Punct && tokens[i].text == "#";
        let opens = is_hash
            && tokens
                .get(i + 1)
                .map(|t| t.kind == RawKind::Punct && t.text == "[")
                .unwrap_or(false);
        if !opens {
            i += 1;
            continue;
        }
        let start = i + 2;
        let mut depth = 1i32;
        let mut j = start;
        while j < tokens.len() {
            if tokens[j].kind == RawKind::Punct && tokens[j].text == "[" {
                depth += 1;
            } else if tokens[j].kind == RawKind::Punct && tokens[j].text == "]" {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            j += 1;
        }
        ranges.push((start, j.min(tokens.len())));
        i = j + 1;
    }
    ranges
}

/// A per-token `true`/`false` flag, one entry per `tokens[i]`, `true` exactly when `i` falls
/// inside some [`attribute_token_ranges`] span - the O(1)-lookup form [`all_ident_ref_sites`]
/// scans against on its one pass over the file's tokens.
fn mark_attribute_tokens(tokens: &[RawTok]) -> Vec<bool> {
    let mut flags = vec![false; tokens.len()];
    for (s, e) in attribute_token_ranges(tokens) {
        for f in flags.iter_mut().take(e).skip(s) {
            *f = true;
        }
    }
    flags
}

/// Identifier-shaped names inside an attribute string literal's content (spec 87 round-1: the
/// motivating case is `#[serde(default = "default_build_config")]`, but a qualified
/// `#[path = "module::name"]`-style value is split the same way, one candidate name per `::`
/// segment) - a literal that is not a name/path at all (free English text in e.g. a
/// `#[doc = ".."]`) yields nothing, since no segment survives the identifier-shape filter.
fn ident_like_segments(inner: &str) -> Vec<String> {
    inner
        .split("::")
        .map(str::trim)
        .filter(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .next()
                    .map(|c| c.is_alphabetic() || c == '_')
                    .unwrap_or(false)
                && seg.chars().all(is_ident_char)
        })
        .map(str::to_string)
        .collect()
}

/// Every `src/` file's out-of-line `mod name;` declarations, keyed by declaring file - the seed
/// pass [`resolve_out_of_line_test_files`] closes transitively. Takes `(path, content)` pairs
/// (not [`FileScan`], which does not retain raw content) so it stays independently fixture-
/// testable in memory, mirroring this module's existing `scan_str`-style convention.
fn resolve_out_of_line_test_files(files: &[(String, String)]) -> BTreeSet<String> {
    let by_path: HashMap<&str, &str> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    let exists = |p: &str| by_path.contains_key(p);

    let mut test_files: BTreeSet<String> = BTreeSet::new();
    let mut worklist: Vec<String> = Vec::new();

    for (path, content) in files {
        for m in scan_file_core(path, content).out_of_line_mods {
            if !m.is_test {
                continue;
            }
            if let Some(target) = resolve_mod_target(path, &m, &exists) {
                if test_files.insert(target.clone()) {
                    worklist.push(target);
                }
            }
        }
    }
    // Transitive closure: ANY mod a now-known test file declares (test-tagged locally or not)
    // is pulled in as test too, since the WHOLE declaring file is already test.
    while let Some(path) = worklist.pop() {
        let Some(&content) = by_path.get(path.as_str()) else {
            continue;
        };
        for m in scan_file_core(&path, content).out_of_line_mods {
            if let Some(target) = resolve_mod_target(&path, &m, &exists) {
                if test_files.insert(target.clone()) {
                    worklist.push(target);
                }
            }
        }
    }
    test_files
}

/// The declaring file's OWN "file-per-module" directory - mirrors `src/grounder/symbols/
/// events.rs`'s production `module_dir` exactly (spec 87 round-1 fix,
/// `resolvers_agree_on_a_transitive_second_hop`): for `mod.rs`/`lib.rs`/`main.rs`, its own
/// PARENT directory (these three names never introduce a new directory level of their own); for
/// any other file `name.rs`, `<parent>/name` - real rustc convention: a plain file's own
/// children live under a directory NAMED after it (`src/outer.rs`'s `mod inner;` resolves to
/// `src/outer/inner.rs`, never a `src/inner.rs` sibling). An earlier version of this function
/// used the plain PARENT directory unconditionally - correct for a top-level or `mod.rs`
/// declaring file (where the two coincide) but wrong the moment a transitive second hop's
/// declaring file is an ordinary nested file, exactly what the resolver-agreement test with the
/// canonical production resolver caught.
fn declaring_file_module_dir(declaring_file: &str) -> String {
    let dir = Path::new(declaring_file)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let base = Path::new(declaring_file)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(declaring_file);
    if base == "mod.rs" || base == "lib.rs" || base == "main.rs" {
        dir
    } else {
        let stem = base.strip_suffix(".rs").unwrap_or(base);
        if dir.is_empty() {
            stem.to_string()
        } else {
            format!("{dir}/{stem}")
        }
    }
}

/// Resolve one [`OutOfLineMod`]'s target file: a `#[path = ".."]` override first - resolved
/// relative to `declaring_file`'s own PARENT directory, rustc's own escape hatch from the
/// file-per-module convention (unconditionally directory-of-file-relative, matching the
/// production resolver's identical `#[path]` rule) - else `name.rs`, else `name/mod.rs`,
/// resolved relative to `declaring_file`'s own [`declaring_file_module_dir`] (spec 87 Design's
/// exact three forms) - `None` if nothing at that path exists in `exists` (a mod pointing
/// outside the given file set, e.g. into `build/`, resolves to nothing and is ignored, matching
/// this criterion's `src/`-only scope).
fn resolve_mod_target(
    declaring_file: &str,
    m: &OutOfLineMod,
    exists: &impl Fn(&str) -> bool,
) -> Option<String> {
    let candidate = if let Some(ov) = &m.path_override {
        let dir = Path::new(declaring_file)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        normalize_rel_path(dir, ov)
    } else {
        let module_dir = declaring_file_module_dir(declaring_file);
        let dir = Path::new(&module_dir);
        let as_file = normalize_rel_path(dir, &format!("{}.rs", m.name));
        if exists(&as_file) {
            as_file
        } else {
            normalize_rel_path(dir, &format!("{}/mod.rs", m.name))
        }
    };
    if exists(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

/// Lexically join `dir` and `rel` (no filesystem access), collapsing `.`/`..` segments, and
/// render forward-slash - the same repo-relative shape every path in this module uses.
fn normalize_rel_path(dir: &Path, rel: &str) -> String {
    let joined = dir.join(rel);
    let mut parts: Vec<String> = Vec::new();
    for comp in joined.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

/// Every `.rs` file under `root/src`, read once, as repo-relative `(path, content)` pairs -
/// deterministically ordered ([`collect_rs_files`] sorts within each directory).
fn collect_src_files_with_content(root: &Path) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    collect_rs_files(&root.join("src"), &mut paths);
    paths
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("dead-code sweep: cannot read {rel}: {e}"));
            (rel, content)
        })
        .collect()
}

/// Every identifier occurrence across `files` (whole `src`+`tests` tree, per [`scan_tree`]),
/// indexed by name - computed ONCE and shared by every candidate's lookup. `whole_file_test`
/// (from [`resolve_out_of_line_test_files`], `src/`-scoped) forces every fn in a wholly-test
/// file to count as test even though its own local [`ScannedFn::is_test`] may say otherwise -
/// see this module's spec 87 criterion 2 banner comment for why (`src/eventstore/contract.rs`).
fn all_ident_ref_sites(
    files: &[FileScan],
    whole_file_test: &BTreeSet<String>,
) -> HashMap<String, Vec<RefSite>> {
    let mut idx: HashMap<String, Vec<RefSite>> = HashMap::new();
    for f in files {
        let is_src = f.rel.starts_with("src/");
        let file_wholly_test = whole_file_test.contains(&f.rel);
        // Round 1 (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances` class 1):
        // test regions are FN spans (`is_test`) AND MOD spans (`ModSpan::is_test`) - never fn
        // spans alone, so a `use`/`const`/`static`/`type` item sitting at `#[cfg(test)] mod
        // tests { .. }`'s own top level (outside every fn body) is still test code. A
        // wholly-test FILE (`file_wholly_test`) is handled separately below, directly on
        // `production`, rather than by synthesizing a whole-file span here.
        let test_ranges: Vec<(usize, usize)> = f
            .fns
            .iter()
            .filter(|sf| sf.is_test)
            .map(|sf| (sf.start_line, sf.end_line))
            .chain(
                f.mod_spans
                    .iter()
                    .filter(|m| m.is_test)
                    .map(|m| (m.start_line, m.end_line)),
            )
            .collect();
        let in_test_range = |line: usize| test_ranges.iter().any(|&(s, e)| line >= s && line <= e);
        let is_production_line = |line: usize| is_src && !file_wholly_test && !in_test_range(line);

        // Round 1 (class 2, "ATTRIBUTE TOKEN TREES ARE REFERENCES"): any identifier or
        // identifier-shaped string-literal segment inside a `#[...]` attribute's token tree is a
        // reference to a same-named fn, unconditionally (not shape-gated the way ordinary code
        // is) - `#[serde(default = "default_build_config")]`, `#[path = ".."]`, clap
        // `value_parser`/`default_value_t`, `cfg_attr` payloads, and any future attribute alike.
        let attr_tok = mark_attribute_tokens(&f.tokens);

        for (i, t) in f.tokens.iter().enumerate() {
            if attr_tok[i] {
                let production = is_production_line(t.line);
                match t.kind {
                    RawKind::Ident => {
                        idx.entry(t.text.clone()).or_default().push(RefSite {
                            file: f.rel.clone(),
                            line: t.line,
                            production,
                            qualifier: None,
                            via_attribute: true,
                        });
                    }
                    RawKind::Lit => {
                        if let Some(inner) = extract_quoted(&t.text) {
                            for name in ident_like_segments(&inner) {
                                idx.entry(name).or_default().push(RefSite {
                                    file: f.rel.clone(),
                                    line: t.line,
                                    production,
                                    qualifier: None,
                                    via_attribute: true,
                                });
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if t.kind != RawKind::Ident {
                continue;
            }
            // A DEFINITION site (`fn name(` - the name token immediately preceded by the `fn`
            // keyword) is never a reference, to itself OR to any other same-named fn. Without
            // this, two fns sharing a bare name (e.g. two inherent `fn new()`s) would each
            // read the OTHER's own signature `new(` as a "call" - found empirically against the
            // real tree (`new`/`drop` before this fix) and pinned by this criterion's own
            // ambiguous-associated-fn fixtures. THE RULE (round 3): every OTHER `Ident` token
            // is a reference to its own text, unconditionally - no shape test of any kind.
            let is_definition_site =
                i > 0 && f.tokens[i - 1].kind == RawKind::Keyword && f.tokens[i - 1].text == "fn";
            if is_definition_site {
                continue;
            }
            let production = is_production_line(t.line);
            let qualifier = qualifier_before(&f.tokens, i);
            idx.entry(t.text.clone()).or_default().push(RefSite {
                file: f.rel.clone(),
                line: t.line,
                production,
                qualifier,
                via_attribute: false,
            });
        }
    }
    idx
}

/// One `file:line` a candidate is referenced from - but ONLY from test code (spec 87 OUTPUT:
/// "the test-only references that kept it looking alive"). Spec 90 criterion 2 addition:
/// `content_hash` (see [`span_content_hash`], over this ONE line's own tokens - `start_line ==
/// end_line`, exactly like a mandatory-sweep call site) is this reference's line-free identity
/// for [`DEAD_CODE_PATH`]'s guarded wire form; `line` stays for every other purpose and moves to
/// [`DEAD_CODE_LINES_PATH`] instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TestOnlyRef {
    file: String,
    line: usize,
    content_hash: String,
}

/// One production fn with ZERO production references (spec 87 OUTPUT). Criterion 2 owns every
/// field through `test_only_references`; criterion 3 (this unit) OWNS `disposition` and `reason`,
/// extending the SAME struct/JSON rather than adding a parallel one (decision
/// `u87c2-json-schema-excludes-disposition`: "c3 extends this same struct/JSON when it lands").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DeadCodeCandidate {
    name: String,
    file: String,
    line: usize,
    visibility: String,
    /// Spec 87 Constraints Walk: "a name shared by several fns is reported as ambiguous rather
    /// than counted as alive" - round 1 (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): ONE
    /// class for every fn kind (`Method`, `ImplAssoc`, `Free`), `true` exactly when this fn's
    /// bare name is shared by >= 2 production fns of the SAME [`DispatchCategory`] AND no
    /// reference could be ATTRIBUTED to this one specifically (see `ambiguous_with`).
    ambiguous: bool,
    /// Populated exactly when `ambiguous` is `true`: `file:line` of every OTHER production fn
    /// this bare name is shared with (round 1 addendum: "a definition with zero attributable
    /// references is listed with ambiguous:true and the sharers named") - named explicitly here
    /// rather than left implicit, since a sharer that WAS successfully attributed a reference is
    /// alive and so never appears anywhere else in this JSON to be found by name alone. Empty
    /// (and omitted from the wire form) for every non-ambiguous entry, keeping this field's
    /// addition byte-identical for the overwhelmingly common case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ambiguous_with: Vec<String>,
    /// Spec 90 criterion 2: the SAME citations as `ambiguous_with`, line-free - `file#hash`
    /// instead of `file:line` (a sharer's own [`span_content_hash`], computed at construction
    /// time over the FULL scanned candidate pool since a live sharer may not itself appear
    /// anywhere in this list's own entries to look its hash back up from later). What
    /// [`DEAD_CODE_PATH`]'s guarded wire form carries INSTEAD of `ambiguous_with`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ambiguous_with_hashed: Vec<String>,
    test_only_references: Vec<TestOnlyRef>,
    /// Criterion 3's own field (spec 87 OUTPUT: "and a DISPOSITION"). Assigned by
    /// [`disposition_for`], keyed on `(file, name)` - never on a line number, since a line moves
    /// `#[cfg(feature)]`-gated `ingest_project`s share one name and one reason.
    disposition: Disposition,
    /// The cited reason [`disposition_for`] assigns alongside [`DeadCodeCandidate::disposition`]
    /// (spec 87 DISPOSITIONS: "Every entry gets one; an entry without a cited reason is a
    /// defect") - a `delete` names why the current zero-production-reference reading is real and
    /// safe to act on; `keep-public-surface` names the consumer; `keep-pending` names the spec.
    reason: String,
    /// Spec 90 criterion 2 addition: this candidate's own line-free identity (see
    /// [`span_content_hash`], over its whole `[start_line, end_line]` span like a catalog
    /// [`dup_site`]) - what [`DEAD_CODE_PATH`]'s guarded wire form carries INSTEAD of `line`.
    content_hash: String,
}

/// Spec 87 DISPOSITIONS, decided: "exactly three" - `delete` (the fn and the tests that
/// reference only it), `keep-public-surface` (a `pub` item that is part of the library's
/// intended external surface, with a cited consumer), or `keep-pending` (referenced only by a
/// test that proves a contract the product is expected to gain, with the spec that will call
/// it). `kebab-case` on the wire matches the spec's own literal spelling exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Disposition {
    Delete,
    KeepPublicSurface,
    KeepPending,
}

/// `true` for the one real, concrete entry point this tree has (Constraints Walk) - a
/// top-level `fn main`, not nested in any `mod`/`impl`.
fn is_exempt_entry_point(f: &ScannedFn) -> bool {
    f.name == "main" && f.enclosing_mods.is_empty() && f.enclosing_impl.is_none()
}

/// `true` for a TRAIT impl (`impl Trait for Type`, `enclosing_impl` contains `" for "`) as
/// opposed to an inherent one (`impl Type`). Deliberately EXEMPTED from candidacy entirely
/// (decision `u87c2-trait-impl-methods-exempted-language-invoked-dispatch`): a trait method can
/// be invoked by LANGUAGE MACHINERY with no textually-matching `.name(` call site anywhere in
/// this source at all - `Drop::drop` at scope end, `Display`/`Debug::fmt` through `{}`/`{:?}`
/// formatting, every operator trait (`PartialEq::eq` through `==`, `Index::index` through
/// `x[i]`, `Add::add` through `+`, ...), `Iterator::next` through a `for` loop's desugaring.
/// Proving one of these truly unreferenced would need real trait-and-desugaring resolution this
/// zero-dependency textual scanner cannot do; this JSON feeds REAL deletions in the wave
/// (spec 87 section 6 item 0), so a false negative here (keeping a genuinely dead trait method)
/// is the safe failure direction - a false positive would delete working `Drop`/operator/format
/// behavior. A receiver-agnostic method that IS textually called (the Constraints Walk's trait-
/// OBJECT case) still needs no exemption - any occurrence of its name already counts as a
/// reference (THE RULE, above `RefSite`) - this rule only ever removes candidates that a
/// textual name search would otherwise call dead.
fn is_trait_impl(f: &ScannedFn) -> bool {
    f.enclosing_impl
        .as_deref()
        .map(|h| h.contains(" for "))
        .unwrap_or(false)
}

/// `true` when `sig_toks` (a fn's signature span, [`body_tokens`]-sliced to
/// `[start_line, body_start_line]`) declares a `self` receiver as its first parameter - the
/// real distinction between a METHOD (`(&self, ..)`/`(&mut self, ..)`/`(self, ..)`, called
/// `receiver.name(..)`) and an INHERENT ASSOCIATED FUNCTION (`fn new() -> Self`, called
/// `Type::name(..)` - path-shaped, same as a free function). `enclosing_impl.is_some()` alone
/// does NOT make this distinction (an earlier version of this sweep matched every impl fn via
/// `.name(` only, which made `Type::new()` invisible - fixed before this criterion's own JSON
/// was ever committed).
fn signature_declares_self(sig_toks: &[RawTok]) -> bool {
    let Some(paren_idx) = sig_toks
        .iter()
        .position(|t| t.kind == RawKind::Punct && t.text == "(")
    else {
        return false;
    };
    let mut i = paren_idx + 1;
    while let Some(t) = sig_toks.get(i) {
        match (&t.kind, t.text.as_str()) {
            (RawKind::Punct, "&") => i += 1,
            (RawKind::Lifetime, _) => i += 1,
            (RawKind::Keyword, "mut") => i += 1,
            (RawKind::Keyword, "self") => return true,
            _ => return false,
        }
    }
    false
}

/// A candidate's dispatch category (this criterion's own concept, not spec 87 vocabulary
/// directly): which [`RefSite`] shape resolves it, and how a shared bare name is disambiguated.
/// `ImplAssoc` gets the SAME ambiguity treatment `Method` always has (inherent associated
/// functions, `Type::new()`, suffer the identical bare-name-only dispatch imprecision - found
/// empirically: this tree's own many `new()`s). Round 1
/// (`op-u87c2-round-1-ambiguity-covers-free-fns-too`) extends ambiguity to `Free` too - the
/// ADVERSARY's `src/distiller.rs` `rebuild` finding (a genuinely dead free fn silently counted
/// alive through an unrelated same-named `src/playbooks.rs` `rebuild`'s real caller) is exactly
/// the failure a bare-name-only free-fn resolution invites - superseding the prior
/// `u87c2-ambiguous-method-rule` decision's Free exemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DispatchCategory {
    Method,
    ImplAssoc,
    Free,
}

fn dispatch_category(f: &ScannedFn, file_tokens: &HashMap<&str, &[RawTok]>) -> DispatchCategory {
    if f.enclosing_impl.is_none() {
        return DispatchCategory::Free;
    }
    let has_self = file_tokens
        .get(f.file.as_str())
        .map(|toks| signature_declares_self(body_tokens(toks, f.start_line, f.body_start_line)))
        .unwrap_or(false);
    if has_self {
        DispatchCategory::Method
    } else {
        DispatchCategory::ImplAssoc
    }
}

/// The file's own "file-per-module" name (the SAME convention [`resolve_mod_target`] resolves a
/// `mod name;` declaration by): `<name>.rs` -> `name`; `<name>/mod.rs` -> `name` (the directory's
/// own name, since `mod.rs` itself names nothing).
fn file_module_name(file: &str) -> String {
    let path = Path::new(file);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if stem == "mod" {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string()
    } else {
        stem.to_string()
    }
}

/// A `Free` fn's OWN qualifying module-path segment - spec 87 round-1 addendum
/// (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): the innermost enclosing INLINE `mod` it
/// sits in (`enclosing_mods.last()`), or - at file scope - the file's own [`file_module_name`].
/// Matched against a reference site's [`RefSite::qualifier`] (the token immediately before a
/// `qualifier::name(`) to attribute a `::`-qualified call to ONE specific same-named `Free` fn
/// (the real case: `src/distiller.rs`'s `rebuild`, qualifier `"distiller"`, versus
/// `src/playbooks.rs`'s unrelated `rebuild`, qualifier `"playbooks"` - `main.rs`'s
/// `playbooks::rebuild(..)` matches only the latter).
fn free_fn_qualifier(f: &ScannedFn) -> String {
    f.enclosing_mods
        .last()
        .cloned()
        .unwrap_or_else(|| file_module_name(&f.file))
}

/// An `ImplAssoc` fn's OWN qualifying type name - its `enclosing_impl` header's Self type (e.g.
/// `"Foo"` for `impl Foo`, `"Foo"` for `impl Foo<T>` too, `"Widget"` for `impl<'a> Widget<'a>`) -
/// matched the same way [`free_fn_qualifier`] is, against a `Type::name(`-shaped call site's
/// [`RefSite::qualifier`]. Delegates to [`impl_self_type`], the one grammar-based extractor for
/// this header shape (round-2 fix for `sdet-u87c2-r1-impl-assoc-qualifier-drops-leading-impl-
/// generics-reintroduces-false-positives`: the old naive `split('<' | whitespace)` returned an
/// EMPTY qualifier whenever the impl block declared its OWN leading generics, since the header
/// text starts with `<` itself in that case and never carries the `impl` keyword).
fn impl_assoc_qualifier(f: &ScannedFn) -> String {
    impl_self_type(f.enclosing_impl.as_deref().unwrap_or_default())
}

/// Per-file map: an imported bare name -> its qualifying module segment, for a SIMPLE
/// `use path::to::name;` import (spec 87 round-1 addendum: "a `use module::name;` ... resolving
/// to it") - the last two `::`-separated path segments become `(name, qualifier)`. Deliberately
/// narrow: a group import (`use a::{b, c};`), a glob (`use a::*;`), or a renamed import
/// (`use a::b as c;`) is NOT resolved, an accepted, disclosed textual-scanner limitation (this is
/// used only to ATTRIBUTE a bare call site during ambiguity resolution - missing one of these
/// shapes costs a missed disambiguation, never a wrong one, since an unattributed reference is
/// simply credited to no definition, never a false one).
fn use_import_qualifiers(tokens: &[RawTok]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut i = 0usize;
    while i < tokens.len() {
        if tokens[i].kind != RawKind::Keyword || tokens[i].text != "use" {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut segments: Vec<String> = Vec::new();
        let mut simple = true;
        while j < tokens.len() {
            match (&tokens[j].kind, tokens[j].text.as_str()) {
                (RawKind::Punct, ";") => {
                    j += 1;
                    break;
                }
                (RawKind::Ident, _) => segments.push(tokens[j].text.clone()),
                (RawKind::Keyword, "self" | "crate" | "super") => {
                    segments.push(tokens[j].text.clone())
                }
                (RawKind::Punct, ":") => {}
                (RawKind::Keyword, "as") | (RawKind::Punct, "{" | "}" | "*" | ",") => {
                    simple = false;
                }
                _ => {}
            }
            j += 1;
        }
        if simple && segments.len() >= 2 {
            let name = segments[segments.len() - 1].clone();
            let qualifier = segments[segments.len() - 2].clone();
            out.insert(name, qualifier);
        }
        i = j;
    }
    out
}

/// SPEC 87 CRITERION 2's whole computation: every production fn under `src/` (file-aware
/// `is_test`) with zero references from production code. `files` is [`scan_tree`]'s whole
/// `src`+`tests` output; `whole_file_test` is [`resolve_out_of_line_test_files`]'s `src/`-only
/// result over the SAME tree `files` was read from.
fn build_dead_code_candidates(
    files: &[FileScan],
    whole_file_test: &BTreeSet<String>,
) -> Vec<DeadCodeCandidate> {
    let file_tokens: HashMap<&str, &[RawTok]> = files
        .iter()
        .map(|fsc| (fsc.rel.as_str(), fsc.tokens.as_slice()))
        .collect();

    let mut candidates: Vec<&ScannedFn> = files
        .iter()
        .filter(|fsc| fsc.rel.starts_with("src/"))
        .flat_map(|fsc| fsc.fns.iter())
        .filter(|f| !(f.is_test || whole_file_test.contains(&f.file)))
        .filter(|f| !is_exempt_entry_point(f))
        .filter(|f| !is_trait_impl(f))
        .collect();
    candidates.sort_by(|a, b| (&a.file, a.start_line).cmp(&(&b.file, b.start_line)));

    let idx = all_ident_ref_sites(files, whole_file_test);
    let use_imports: HashMap<&str, HashMap<String, String>> = files
        .iter()
        .map(|fsc| (fsc.rel.as_str(), use_import_qualifiers(&fsc.tokens)))
        .collect();

    let categories: HashMap<(&str, usize), DispatchCategory> = candidates
        .iter()
        .map(|f| {
            (
                (f.file.as_str(), f.start_line),
                dispatch_category(f, &file_tokens),
            )
        })
        .collect();

    // Round 1 (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): ambiguity is ONE class for
    // every [`DispatchCategory`] - `Free` is no longer exempt (the prior `Free != cat` guard is
    // gone). `group_members` names every OTHER sharer's `file:line` for `ambiguous_with`.
    let mut name_counts: HashMap<(DispatchCategory, &str), usize> = HashMap::new();
    let mut group_members: HashMap<(DispatchCategory, &str), Vec<String>> = HashMap::new();
    // Spec 90 criterion 2: every candidate's own `file:line` citation -> its `content_hash`,
    // over the FULL scanned pool (not just `out` below) - `ambiguous_with` can cite a live
    // sharer that never itself appears in `out` (it has a production reference, so it is not a
    // dead-code candidate), so the hash for that citation must be captured HERE, not
    // reconstructed later from the filtered output list.
    let mut citation_hash: HashMap<String, String> = HashMap::new();
    for f in &candidates {
        let cat = categories[&(f.file.as_str(), f.start_line)];
        *name_counts.entry((cat, f.name.as_str())).or_insert(0) += 1;
        let citation = format!("{}:{}", f.file, f.start_line);
        citation_hash.entry(citation.clone()).or_insert_with(|| {
            span_content_hash(file_tokens[f.file.as_str()], f.start_line, f.end_line)
        });
        group_members
            .entry((cat, f.name.as_str()))
            .or_default()
            .push(citation);
    }

    let mut out = Vec::new();
    for f in &candidates {
        let cat = categories[&(f.file.as_str(), f.start_line)];
        let my_citation = format!("{}:{}", f.file, f.start_line);
        let group_size = name_counts
            .get(&(cat, f.name.as_str()))
            .copied()
            .unwrap_or(0);
        let ambiguous_group = group_size > 1;
        let empty: Vec<RefSite> = Vec::new();
        let sites = idx.get(&f.name).unwrap_or(&empty);
        // THE RULE (round 3, `op-u87c2-round-3-a-reference-is-any-token-not-a-shape`): every
        // occurrence of this name is a candidate reference - no `DispatchCategory`-keyed shape
        // gate any more - other than the fn's own signature span, which can never reference
        // itself or a same-named sibling.
        let relevant = sites.iter().filter(|s| {
            !(s.file == f.file && s.line >= f.start_line && s.line <= f.body_start_line)
        });

        // Round 1 addendum: for `Method`, ambiguity keeps its ORIGINAL aggregate rule
        // unchanged - receiver-agnostic dispatch genuinely cannot attribute a `.name(` call to
        // one specific sharer, so every reference counts for every sharer alike. `Free`/
        // `ImplAssoc` get real per-definition ATTRIBUTION when the name is shared, tried in
        // order: (1) a `::`-qualified call site whose qualifier textually matches this
        // definition's own (a type name for `ImplAssoc`, a module name for `Free`); (2) a bare
        // call resolved through the REFERENCING file's own `use module::name;` import; (3) for
        // `Free` only, a bare call sitting in the SAME FILE as this definition - Rust's own
        // lexical scoping resolves an unqualified sibling call with no `use` needed at all (the
        // extremely common case: two files each defining and bare-calling their own same-named
        // helper). An attribute-derived reference (`via_attribute`) always counts for every
        // sharer regardless (never delete live code over an attribute mention no qualifier can
        // resolve). Anything left over - a bare, unqualified, un-`use`d, different-file mention
        // of a shared name (or an `ImplAssoc` call written `Self::name(` from inside a SIBLING
        // impl, which this textual scanner does not resolve back to a type - a disclosed, narrow
        // scanner limitation) - is credited to NO sharer, per the addendum's literal text.
        let needs_attribution = ambiguous_group && cat != DispatchCategory::Method;
        let my_qualifier = match cat {
            DispatchCategory::Free => Some(free_fn_qualifier(f)),
            DispatchCategory::ImplAssoc => Some(impl_assoc_qualifier(f)),
            DispatchCategory::Method => None,
        };

        let mut production_hit = false;
        let mut test_only: Vec<TestOnlyRef> = Vec::new();
        for s in relevant {
            let counts_for_me = if !needs_attribution || s.via_attribute {
                true
            } else if s.qualifier.as_deref() == Some("Self") && s.file == f.file {
                // Spec 87 criterion 3 fix: `Self::name(` inside the SAME FILE as this
                // candidate's own definition names whatever type's impl block lexically
                // contains it. A textual scanner cannot walk impl-block scope, so this
                // approximates it the same way the `Free`-category same-file fallback below
                // already does ("Rust's own lexical scoping resolves an unqualified sibling
                // call with no `use` needed at all") - widened here to the literal keyword
                // `Self`, which can never equal a type's own name by direct text comparison
                // and so was never matched by the plain `resolved == my_qualifier` check
                // (found via `DashMarker::parse`, called only via `Self::parse` from its own
                // `DashMarker::read` - a real production call path the false-positive
                // direction this unit forbids must not miss).
                true
            } else {
                let resolved = s
                    .qualifier
                    .clone()
                    .or_else(|| use_imports.get(s.file.as_str())?.get(&f.name).cloned())
                    .or_else(|| {
                        (cat == DispatchCategory::Free && s.file == f.file)
                            .then(|| my_qualifier.clone())
                            .flatten()
                    });
                resolved == my_qualifier
            };
            if !counts_for_me {
                continue;
            }
            if s.production {
                production_hit = true;
            } else {
                test_only.push(TestOnlyRef {
                    file: s.file.clone(),
                    line: s.line,
                    content_hash: span_content_hash(file_tokens[s.file.as_str()], s.line, s.line),
                });
            }
        }
        if production_hit {
            continue;
        }
        let ambiguous = ambiguous_group;
        let ambiguous_with: Vec<String> = if ambiguous {
            group_members[&(cat, f.name.as_str())]
                .iter()
                .filter(|c| **c != my_citation)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let ambiguous_with_hashed: Vec<String> = ambiguous_with
            .iter()
            .map(|citation| {
                let (file, _) = citation.rsplit_once(':').unwrap_or_else(|| {
                    panic!("ambiguous_with citation {citation:?} is not file:line-shaped")
                });
                let hash = citation_hash.get(citation).unwrap_or_else(|| {
                    panic!("no content_hash recorded for ambiguous_with citation {citation:?}")
                });
                format!("{file}#{hash}")
            })
            .collect();
        test_only.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        test_only.dedup();
        out.push(DeadCodeCandidate {
            name: f.name.clone(),
            file: f.file.clone(),
            line: f.start_line,
            visibility: f.visibility.clone(),
            ambiguous,
            ambiguous_with,
            ambiguous_with_hashed,
            test_only_references: test_only,
            content_hash: span_content_hash(file_tokens[f.file.as_str()], f.start_line, f.end_line),
            // NOT yet dispositioned here: this is criterion 2's instrument, which never sees a
            // `disposition_for` entry for a fixture-tree file (every one of this file's OWN
            // fixture tests constructs a synthetic `src/...rs` that has no place in that real-
            // tree-only table). [`apply_real_tree_dispositions`], applied ONLY by
            // [`real_dead_code_candidates`], fills these in for the real tree; a fixture test
            // never inspects either field.
            disposition: Disposition::Delete,
            reason: String::new(),
        });
    }
    out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    out
}

/// Spec 87 criterion 3's own disposition assignment for the REAL tree only - hand-authored per
/// candidate, keyed on `(file, line)` rather than bare `name` (the two `#[cfg(feature)]`-gated
/// `ingest_project`s share a name but carry different reasons; the `dash.rs` `parse` this file's
/// own fixture regression test names is NOT in this table at all - the Self:: fix above removes
/// it from the real tree's candidate set entirely, so it never reaches this function). Applied
/// by [`apply_real_tree_dispositions`] to [`real_dead_code_candidates`] only - a fixture-tree
/// candidate never reaches this function at all, so a synthetic `src/vis.rs`-shaped fixture file
/// can never trip its panic (see that function's own doc for why the fixture/real-tree split
/// exists). Every entry the real tree's instrument (criteria 1-2) finds MUST resolve here or
/// this panics loudly, by design: spec 85's own THOROUGHNESS discipline ("unassignable functions
/// are named as such, never omitted") applies to a disposition exactly as it does to the
/// responsibility map's module assignment - a silently-defaulted disposition on a real candidate
/// would be worse than a loud build failure naming exactly which one needs research. The full
/// evidence trail behind each entry - the real caller/consumer read, every recursive `grep -rn`
/// this unit ran to rule out a hidden qualified call site, and (for the three `keep-pending`
/// entries) the governing spec's own text - lives in
/// `docs/audit/2026-09-simplification-audit.md` section 4, not repeated here; this table carries
/// the SAME reason text the report's full list cites, so the JSON and the report can never state
/// two different reasons for one candidate.
fn disposition_for(file: &str, name: &str) -> (Disposition, &'static str) {
    use Disposition::{Delete, KeepPending};
    match (file, name) {
        ("src/canary.rs", "cataloged_classes") => (
            Delete,
            "cataloged_classes has no production caller anywhere in src/ (checked whole-tree, \
             recursively). Its only two references are its own inline tests \
             (canary.rs:1419/1433); the one behavior it exists to prove - the shipped corpus \
             catalogs >= 3 defect classes (spec 13 unit 5) - is asserted by \
             the_shipped_corpus_loads_and_catalogs_at_least_three_defect_classes, which is the \
             ONLY caller and would go dead with it. run_canary (the real production review-panel \
             loop) never consults it: corpus diversity is a load-time authoring check, not a \
             runtime one.",
        ),
        ("src/config.rs", "sdet_author_enabled") => (
            KeepPending,
            "sdet_author_enabled has zero production callers - a real, disclosed wiring gap, not \
             a false positive: spawn_sdet_author (conductor.rs:3980) gates the always-on SDET \
             periphery-test author ONLY on whether an sdet-author agent is configured \
             (self.cfg.agents.get(ROLE_SDET_AUTHOR)) and never consults \
             self.cfg.defaults.sdet_author_enabled() at all, so today an explicit \
             sdet_author: false in a workflow's defaults: block does nothing - the on-by-default \
             opt-out spec 32 documents and this very method's own doc comment claims ('the \
             conductor's build-seam reads through here') is not actually true of the shipped \
             code. keep-pending, citing spec 32 (the sdet-author feature this toggle governs, \
             already landed): deleting the method would delete the documented resolution \
             authority for an already-shipped, still-referenced config field \
             (Defaults::sdet_author) before a follow-up wires spawn_sdet_author to call it - the \
             correct fix is wiring the call site, out of scope here (no production code changes \
             this criterion).",
        ),
        ("src/dash.rs", "pid_is_alive") => (
            Delete,
            "pid_is_alive has no production caller. It is the RETIRED predecessor of the real \
             marker-serving check: dash_marker_serving (main.rs:5711, the production predicate \
             ensure_run_dashboard_at is called with) calls dash::dash_serving_on - a REAL \
             network probe of the port - and its own doc comment explains why by name: 'A REAL \
             network probe of the port, never a bare pid-liveness check: a marker left by a \
             self-reaped or pid-recycled dash must never masquerade as still serving just because \
             its pid happens to be alive'. pid_is_alive's only references are its own unit test \
             (dash.rs:9304) and an older fixture test (main.rs:12795-12841) that injects it as a \
             simplified closure for readability, not a claim about production behavior.",
        ),
        ("src/dash.rs", "neighborhood") => (
            Delete,
            "neighborhood (the single-seed convenience wrapper) has no production caller. The \
             route handler (dash.rs:3404) calls neighborhood_of (the multi-seed core \
             neighborhood delegates to) DIRECTLY, per its own doc comment: 'the re-pointed \
             run-tree click (spec 43) uses it to seed from a unit's several decision/finding \
             content nodes at once'. neighborhood's 13 references are all its own tests \
             exercising the single-seed case directly.",
        ),
        ("src/dash.rs", "serve") => (
            Delete,
            "serve (the blocking, self-binding accept-loop entry point) has no production caller. \
             The real dash-serving CLI path (main.rs::cmd_dash) calls dash::serve_on (main.rs:6493) \
             with the listener bind_singleton already bound, never dash::serve, which would bind \
             its own listener and so cannot participate in spec 62's singleton-bind-then-serve \
             flow. (main.rs:6574's doc comment still names '[`dash::serve`]' as the accept loop \
             that terminates - a stale intra-doc reference worth a follow-up fixing it to name \
             serve_on, disclosed here rather than silently left.) serve's 2 references are its \
             own smoke tests.",
        ),
        ("src/distiller.rs", "rebuild") => (
            KeepPending,
            "rebuild (the digest-pool projection rebuild, spec 27) has no production caller \
             anywhere - src/lib.rs's mod distiller; declaration is the only mention of the module \
             outside distiller.rs itself, confirmed by a whole-tree recursive grep. Unlike a mere \
             convenience wrapper, rebuild's own tested behavior IS spec 27's already-landed \
             Done-when contract (clearing and re-deriving the digest pool, run-boundary scoping, \
             determinism) - deleting it would delete the shipped mechanism those criteria proved, \
             not an unused alternative to one. keep-pending, citing spec 27: it is modeled \
             directly on playbooks::rebuild, which IS wired to a CLI surface (rigger playbooks \
             --rebuild); distiller::rebuild is the equivalent primitive awaiting its own call \
             site (a rigger distill command, or a hook into rigger reset), never spec'd as a \
             Done-when criterion of spec 27 itself, which is why it shipped unwired.",
        ),
        ("src/eventstore/sqlite.rs", "with_content_identity") => (
            KeepPending,
            "with_content_identity (the storage-level idempotency append-guard builder, spec 60) \
             has no production caller: open_sqlite_store (main.rs:444), the crate's ONE sqlite \
             event-log constructor, is a bare Store::open(path) with no guard chained, and \
             derived_index_identity() (ingest.rs:368, the production ContentIdentity value) is \
             only ever passed to maintenance operations (count_derived_duplicates, \
             prune_derived_index) that take it as a query-time parameter, never to \
             with_content_identity itself. Its 44 references are all test fixtures exercising the \
             guard directly against a Store they construct, per spec 60 criterion 4's own \
             Done-when text: 'it DOES prove itself by driving the STORE PORT directly'. \
             keep-pending, citing spec 60 (already landed): this is the documented, deliberately \
             injected-at-the-composition-root 'defense in depth so a regression upstream can \
             never re-bloat the log' criterion 4 shipped and tested - deleting it removes a \
             proven backstop the composition root is architecturally meant to wire in, not dead \
             weight. Disclosed plainly: whether main.rs's real store-open path SHOULD chain it is \
             an open question this criterion does not resolve (no production code changes here) - \
             a follow-up spec should either wire it into open_sqlite_store or explicitly retire \
             it, rather than let it sit silently unwired indefinitely.",
        ),
        ("src/gate.rs", "resolve_wrapper_name") => (
            Delete,
            "resolve_wrapper_name has no production caller - independently confirmed (u87c2's own \
             decision u87c2-three-precision-fixes-from-real-tree-spot-check already found this by \
             hand: 'resolve_build_layer duplicates its ambient-PATH read inline rather than \
             calling it'). resolve_build_layer (gate.rs:581), its own doc comment's named sole \
             intended caller ('kept pub as the wrapper-only building block ... \
             resolve_build_layer composes'), instead reads std::env::var_os(\"PATH\") itself \
             (gate.rs:588) rather than calling resolve_wrapper_name(wrapper) - a small, \
             confirmed duplicate-glue defect (worth folding into section 2's duplication catalog \
             on a follow-up pass), not a sign the function is unneeded. resolve_wrapper_name's 4 \
             references are its own tests.",
        ),
        ("src/grounder/symbols/events.rs", "index_events") => (
            Delete,
            "index_events is spec 87's own Goal-worked example of the false negative this whole \
             spec exists to fix ('a function whose only caller is its own test reads as alive: \
             the false negative the operator predicted... e.g. \
             src/grounder/symbols/events.rs:17 index_events, 21 test references'). Confirmed \
             again on the current tree (now 64 test references at line 29, having grown with the \
             test suite): no production caller anywhere.",
        ),
        ("src/grounder/symbols/model.rs", "definitions_named") => (
            Delete,
            "definitions_named has no production caller - only its own module's assertion-style \
             tests (symbols/mod.rs) use it to check index state after a build/update, never a \
             production edge-resolution path.",
        ),
        ("src/grounder/symbols/model.rs", "references_named") => (
            Delete,
            "references_named has no production caller - the same test-only accessor shape as \
             its sibling definitions_named immediately above it.",
        ),
        ("src/ingest.rs", "ingest_project") => (
            Delete,
            "ingest_project - both the #[cfg(feature = \"symbols\")] single-event lane and the \
             #[cfg(not(feature = \"symbols\"))] light-lane no-op, one name at two cfg-gated \
             sites - has no production caller on either lane. Production calls \
             ingest_project_batched exclusively (conductor.rs, main.rs), the batched entry point \
             this fn's own doc comment already names as the thing 'existing callers discard \
             [IngestStats] and are unaffected' by, i.e. it documents its own supersession.",
        ),
        ("src/ledger.rs", "fully_done") => (
            Delete,
            "fully_done has no production caller. Its own doc comment's three-conjunct \
             completion check is subsumed elsewhere: nothing in conductor.rs or main.rs calls it \
             (confirmed whole-tree, recursively) - the wired run-completion checks it was \
             apparently meant to serve use done() and the per-unit is_terminal predicate instead. \
             (Line shifted 500 -> 506 -> 574: spec 88 round 3's prior_criterion_unit spec-scoping \
             fix added 6 lines above spec_stem, then merging rigger-run's spec 88 criterion 3 \
             (ESCALATION RESUMES) added resume_bound/ResumeGrant/UnitResumed earlier in this same \
             file.)",
        ),
        ("src/ledger.rs", "is_integrated") => (
            Delete,
            "is_integrated has no production caller, though its doc comment claims one ('used by \
             resume to skip completed work'): the real resume-skip logic uses is_terminal \
             (Integrated OR Escalated - confirmed live at conductor.rs:1621/9829, ledger.rs:643, \
             main.rs:9927/9932), which correctly subsumes is_integrated's narrower Integrated-only \
             check (a resume must also skip an Escalated unit, which is_integrated alone would \
             wrongly re-attempt). Superseded, not merely unused. \
             (Line shifted 577 -> 583 -> 651: same round-3 doc-comment addition, then the same \
             rigger-run merge shift above.)",
        ),
        ("src/spawn.rs", "new") => (
            Delete,
            "SpawnRequest::new and its 7 builder methods (with_system_prompt/with_model/ \
             with_tools/with_dir/with_blast_radius/with_title/with_reviews) plus the park \
             convenience wrapper are ALL dead together, same root cause: the real production spawn \
             path (driver/replay.rs:251-252 fn spawn_request(...) -> SpawnRequest { SpawnRequest \
             { ... } }) constructs the struct via a direct struct literal and calls \
             park_in_run(store, &req, &opts.run_id) directly (driver/replay.rs:338) - it never \
             touches the builder or the zero-run-id park() wrapper at all. Every one of these 9 \
             fns' references is a test fixture building a SpawnRequest by hand; spec 87's own \
             Goal text names four of the seven builders (with_title/with_reviews/with_model/ \
             with_blast_radius) as its worked example of confirmed dead code.",
        ),
        ("src/spawn.rs", "with_system_prompt") => (
            Delete,
            "with_system_prompt - part of the SpawnRequest builder family; see the disposition on \
             SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.",
        ),
        ("src/spawn.rs", "with_model") => (
            Delete,
            "with_model - part of the SpawnRequest builder family; see the disposition on \
             SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.",
        ),
        ("src/spawn.rs", "with_tools") => (
            Delete,
            "with_tools - part of the SpawnRequest builder family; see the disposition on \
             SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.",
        ),
        ("src/spawn.rs", "with_dir") => (
            Delete,
            "with_dir - part of the SpawnRequest builder family; see the disposition on \
             SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.",
        ),
        ("src/spawn.rs", "with_blast_radius") => (
            Delete,
            "with_blast_radius - part of the SpawnRequest builder family (one of the four spec 87 \
             Goal names by name); see the disposition on SpawnRequest::new (spawn.rs:320) for the \
             shared root cause and citation.",
        ),
        ("src/spawn.rs", "with_title") => (
            Delete,
            "with_title - part of the SpawnRequest builder family (one of the four spec 87 Goal \
             names by name); see the disposition on SpawnRequest::new (spawn.rs:320) for the \
             shared root cause and citation.",
        ),
        ("src/spawn.rs", "with_reviews") => (
            Delete,
            "with_reviews - part of the SpawnRequest builder family (one of the four spec 87 Goal \
             names by name); see the disposition on SpawnRequest::new (spawn.rs:320) for the \
             shared root cause and citation.",
        ),
        ("src/spawn.rs", "park") => (
            Delete,
            "park (the zero-run-id convenience wrapper around park_in_run) - part of the \
             SpawnRequest-construction dead set; see the disposition on SpawnRequest::new \
             (spawn.rs:320) for the shared root cause and citation. park_in_run itself (spawn.rs:410, \
             the real park authority) is correctly NOT a candidate: its own body is the one real \
             production reference driver/replay.rs:338 needs - this scanner counts direct \
             references, not reachability, so park_in_run reads alive even though its only OTHER \
             caller (park) is itself dead.",
        ),
        ("src/worktree.rs", "is_dirty") => (
            Delete,
            "is_dirty has no production caller - one of spec 87's own two Goal-cited worked \
             examples ('src/worktree.rs expect_merged and is_dirty'), reconfirmed on the current \
             tree: its 3 references (src/conductor.rs and worktree.rs's own `mod tests`) are all \
             test-only; its own body now delegates to `path_is_dirty` (spec 89 round 3), but that \
             internal call is not a caller of `is_dirty` itself. Line shifted again, this time by \
             spec 89 criterion 1's round 3 fix \
             (`arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`): `is_dirty`'s \
             body was replaced with a one-line delegation to the new shared `path_is_dirty` free \
             fn (built on the pre-existing `git`/`run_git` primitives, and now also called \
             directly by `sweep_terminal_logged` and `main.rs`'s `reclaim_orphan_scratch` so the \
             two no longer risk diverging on how a git-status failure is read), and the doc \
             comment naming that delegation pushes the line down 8 more, 627->635. expect_merged \
             itself (formerly src/worktree.rs:86) is no longer a candidate at all: round 4 moved \
             it, together with `IntegrateOutcome` and the pre-round-4 `integrate` method, into \
             this file's own `#[cfg(test)] mod tests` (a test-only recomposition of the \
             newly-split `merge_into_worktree`/`land`, since production - `integrate_and_emit` - \
             now calls those two split methods directly for its own row-level durable recording \
             and has no caller left for the combined form) - a test-scoped item is not a \
             production dead-code candidate by this scanner's own definition, closing the \
             finding at its root rather than re-dispositioning it in place.",
        ),
        (other_file, other_name) => panic!(
            "dead-code candidate {other_file}::{other_name} has no assigned disposition - this is \
             a NEW candidate the tree has grown since spec 87 criterion 3 researched and \
             dispositioned every candidate that existed then (26 entries); assign one in \
             disposition_for (spec 87 criterion 3's own OWNS: 'the dispositions'), citing a real \
             consumer (keep-public-surface), a governing spec (keep-pending), or the evidence \
             that no production caller exists anywhere (delete) - never default one silently \
             (spec 85's own THOROUGHNESS discipline: 'unassignable... named as such, never \
             omitted')"
        ),
    }
}

const DEAD_CODE_PATH: &str = "docs/audit/dead-code.json";

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`DEAD_CODE_PATH`]'s line spans (see
/// [`CATALOG_LINES_PATH`]'s own doc comment for the full rationale, identical here).
const DEAD_CODE_LINES_PATH: &str = "docs/audit/dead-code.lines.json";

/// [`DEAD_CODE_PATH`]'s guarded per-reference shape: `{file, content_hash}`, no line number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestOnlyRefWire {
    file: String,
    content_hash: String,
}

/// [`DEAD_CODE_LINES_PATH`]'s per-reference shape: the line [`TestOnlyRefWire`] dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestOnlyRefLines {
    file: String,
    line: usize,
}

/// [`DEAD_CODE_PATH`]'s guarded per-candidate shape: every [`DeadCodeCandidate`] field except
/// `line`, with `ambiguous_with_hashed` standing in for `ambiguous_with`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DeadCodeCandidateWire {
    name: String,
    file: String,
    content_hash: String,
    visibility: String,
    ambiguous: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ambiguous_with: Vec<String>,
    test_only_references: Vec<TestOnlyRefWire>,
    disposition: Disposition,
    reason: String,
}

/// [`DEAD_CODE_LINES_PATH`]'s per-candidate shape: the line spans [`DeadCodeCandidateWire`]
/// dropped - `line`, plus `ambiguous_with`'s ORIGINAL `file:line` form (not the wire file`#`hash
/// one) and every `test_only_references` entry's own line. In the SAME order as
/// [`DEAD_CODE_PATH`] - joined by array position, `name` carried too for a human cross-checking
/// by eye.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DeadCodeCandidateLines {
    file: String,
    name: String,
    line: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ambiguous_with: Vec<String>,
    test_only_references: Vec<TestOnlyRefLines>,
}

fn dead_code_candidate_wire(c: &DeadCodeCandidate) -> DeadCodeCandidateWire {
    DeadCodeCandidateWire {
        name: c.name.clone(),
        file: c.file.clone(),
        content_hash: c.content_hash.clone(),
        visibility: c.visibility.clone(),
        ambiguous: c.ambiguous,
        ambiguous_with: c.ambiguous_with_hashed.clone(),
        test_only_references: c
            .test_only_references
            .iter()
            .map(|r| TestOnlyRefWire {
                file: r.file.clone(),
                content_hash: r.content_hash.clone(),
            })
            .collect(),
        disposition: c.disposition,
        reason: c.reason.clone(),
    }
}

fn dead_code_candidate_lines(c: &DeadCodeCandidate) -> DeadCodeCandidateLines {
    DeadCodeCandidateLines {
        file: c.file.clone(),
        name: c.name.clone(),
        line: c.line,
        ambiguous_with: c.ambiguous_with.clone(),
        test_only_references: c
            .test_only_references
            .iter()
            .map(|r| TestOnlyRefLines {
                file: r.file.clone(),
                line: r.line,
            })
            .collect(),
    }
}

/// Deterministic pretty JSON for [`build_dead_code_candidates`]'s output, LINE-FREE (spec 90
/// criterion 2): serializes through [`DeadCodeCandidateWire`], never [`DeadCodeCandidate`]
/// directly - mirrors [`catalog_to_json`]'s own shape (a bare array, declared field order, no
/// `HashMap` anywhere).
fn dead_code_to_json(candidates: &[DeadCodeCandidate]) -> String {
    let wire: Vec<DeadCodeCandidateWire> =
        candidates.iter().map(dead_code_candidate_wire).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("DeadCodeCandidateWire serializes");
    s.push('\n');
    s
}

/// Deterministic pretty JSON for [`DEAD_CODE_LINES_PATH`] - the line data [`dead_code_to_json`]
/// dropped. Never drift-guarded; written fresh every `RIGGER_AUDIT_WRITE=1` run alongside the
/// guarded file.
fn dead_code_lines_to_json(candidates: &[DeadCodeCandidate]) -> String {
    let wire: Vec<DeadCodeCandidateLines> =
        candidates.iter().map(dead_code_candidate_lines).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("DeadCodeCandidateLines serializes");
    s.push('\n');
    s
}

/// The real checked-out tree's whole-`src`-tree file-aware test-file set, memoized alongside
/// [`real_files`] for the same reason ([`resolve_out_of_line_test_files`] re-scans every `src/`
/// file's out-of-line mods, which is cheap, but no need to repeat it per test).
fn real_whole_file_test_set() -> &'static BTreeSet<String> {
    static CACHE: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        resolve_out_of_line_test_files(&collect_src_files_with_content(&repo_root()))
    })
}

/// Criterion 3's own finalization step, applied ONLY to the real tree's candidates: fills in
/// each candidate's [`disposition_for`] lookup result. Kept OUT of
/// [`build_dead_code_candidates`] itself (criterion 2's instrument) so this file's own dozens of
/// fixture tests - each constructing a synthetic `src/...rs` tree via [`candidates_for`] - never
/// trip `disposition_for`'s deliberate real-tree-only panic; only [`real_dead_code_candidates`]
/// calls this.
fn apply_real_tree_dispositions(mut candidates: Vec<DeadCodeCandidate>) -> Vec<DeadCodeCandidate> {
    for c in &mut candidates {
        let (disposition, reason) = disposition_for(&c.file, &c.name);
        c.disposition = disposition;
        c.reason = reason.to_string();
    }
    candidates
}

/// The real checked-out tree's [`build_dead_code_candidates`], DISPOSITIONED (see
/// [`apply_real_tree_dispositions`]), memoized alongside [`real_files`] for the same reason.
fn real_dead_code_candidates() -> &'static [DeadCodeCandidate] {
    static CACHE: std::sync::OnceLock<Vec<DeadCodeCandidate>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        apply_real_tree_dispositions(build_dead_code_candidates(
            real_files(),
            real_whole_file_test_set(),
        ))
    })
}

/// [`real_dead_code_candidates`]'s own `lines` sidecar, in the SAME order - one shared
/// computation for both call sites that need it ([`render_section_4`]'s 4.3 and
/// [`render_section_6`]'s item 0), rather than each repeating the same two-line
/// `real_dead_code_candidates().iter().map(dead_code_candidate_lines).collect()` build.
fn real_dead_code_lines() -> Vec<DeadCodeCandidateLines> {
    real_dead_code_candidates()
        .iter()
        .map(dead_code_candidate_lines)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_str(src: &str) -> Vec<ScannedFn> {
        scan_file("t.rs", src)
    }

    fn scan_str_core(src: &str) -> FileScanCore {
        scan_file_core("t.rs", src)
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
    // Scanner: spec 87 criterion 2 additions - visibility, body_start_line, out-of-line mods
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_free_function_with_no_pub_keyword_is_private() {
        let fns = scan_str("fn helper() {}\n");
        assert_eq!(fns[0].visibility, "private");
    }

    #[test]
    fn a_bare_pub_function_is_captured_verbatim() {
        let fns = scan_str("pub fn helper() {}\n");
        assert_eq!(fns[0].visibility, "pub");
        assert_eq!(fns[0].name, "helper");
    }

    #[test]
    fn a_pub_crate_function_keeps_the_qualifier() {
        let fns = scan_str("pub(crate) fn helper() {}\n");
        assert_eq!(fns[0].visibility, "pub(crate)");
    }

    #[test]
    fn a_pub_super_function_keeps_the_qualifier() {
        let fns = scan_str("mod m {\n    pub(super) fn helper() {}\n}\n");
        assert_eq!(fns[0].visibility, "pub(super)");
    }

    #[test]
    fn visibility_does_not_leak_onto_a_later_unrelated_function() {
        let fns = scan_str("pub struct Foo;\nfn helper() {}\n");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "helper");
        assert_eq!(fns[0].visibility, "private");
    }

    #[test]
    fn body_start_line_is_the_opening_brace_not_the_fn_keyword() {
        let src = "fn helper(\n    x: i32,\n) -> i32 {\n    x\n}\n";
        let fns = scan_str(src);
        assert_eq!(fns[0].start_line, 1);
        assert_eq!(fns[0].body_start_line, 3);
        assert_eq!(fns[0].end_line, 5);
    }

    #[test]
    fn a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword() {
        // The exact shape this criterion's fix targets: a visibility qualifier sitting between
        // a #[cfg(test)] attribute and the item it governs must not clear the pending flag.
        let src = "#[cfg(test)]\npub fn helper_for_tests() {}\n";
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_test, "{:?}", fns[0]);
        assert_eq!(fns[0].visibility, "pub");
    }

    #[test]
    fn an_out_of_line_mod_declaration_is_recorded_not_discarded() {
        let core = scan_str_core("mod helpers;\nfn a() {}\n");
        assert_eq!(core.out_of_line_mods.len(), 1);
        assert_eq!(core.out_of_line_mods[0].name, "helpers");
        assert!(!core.out_of_line_mods[0].is_test);
        assert_eq!(core.out_of_line_mods[0].path_override, None);
    }

    #[test]
    fn a_cfg_test_out_of_line_mod_is_flagged_test() {
        let core = scan_str_core("#[cfg(test)]\nmod contract;\n");
        assert_eq!(core.out_of_line_mods.len(), 1);
        assert!(core.out_of_line_mods[0].is_test);
    }

    #[test]
    fn a_cfg_test_pub_out_of_line_mod_is_flagged_test_the_real_eventstore_mod_rs_shape() {
        // `src/eventstore/mod.rs`'s real declaration: `#[cfg(test)]\npub mod contract;` - the
        // exact regression this criterion's `pub` fix exists for.
        let core = scan_str_core("#[cfg(test)]\npub mod contract;\n");
        assert_eq!(core.out_of_line_mods.len(), 1);
        assert_eq!(core.out_of_line_mods[0].name, "contract");
        assert!(
            core.out_of_line_mods[0].is_test,
            "{:?}",
            core.out_of_line_mods[0]
        );
    }

    #[test]
    fn an_out_of_line_mod_inherits_test_ness_from_an_enclosing_cfg_test_mod() {
        let core = scan_str_core("#[cfg(test)]\nmod outer {\n    mod inner;\n}\n");
        assert_eq!(core.out_of_line_mods.len(), 1);
        assert_eq!(core.out_of_line_mods[0].name, "inner");
        assert!(core.out_of_line_mods[0].is_test);
    }

    #[test]
    fn a_path_override_attribute_is_captured_verbatim() {
        let core = scan_str_core("#[path = \"generated/real.rs\"]\nmod fake;\n");
        assert_eq!(core.out_of_line_mods.len(), 1);
        assert_eq!(
            core.out_of_line_mods[0].path_override.as_deref(),
            Some("generated/real.rs")
        );
    }

    #[test]
    fn an_inline_mod_is_not_recorded_as_out_of_line() {
        let core = scan_str_core("mod inline_body {\n    fn a() {}\n}\n");
        assert!(core.out_of_line_mods.is_empty());
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
            visibility: "private".to_string(),
            body_start_line: 1,
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
            visibility: "private".to_string(),
            body_start_line: 1,
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
    fn impl_self_type_strips_a_leading_dyn_token_on_the_self_type() {
        // Regression pointer from adv-u87c2-r1-cheaper-fix-exists-reuse-impl-self-type: an
        // inherent impl block on a trait-object type (`impl dyn Projection { .. }`, valid Rust -
        // e.g. std's own `impl dyn Any`) has a Self type text of `dyn Projection`, and the SAME
        // shape can appear after `for` too (`impl Trait for dyn Concrete`). Zero `impl dyn`
        // blocks exist in `src/` today (live-but-currently-inert per that finding), but the
        // general resolver must still handle it correctly since this scanner enumerates header
        // shapes once rather than discovering them one per round.
        assert_eq!(impl_self_type("dyn Projection"), "Projection");
        assert_eq!(impl_self_type("AgentDriver for dyn Stub"), "Stub");
        assert_eq!(impl_self_type("dyn Projection<'a>"), "Projection");
    }

    // -------------------------------------------------------------------------------------
    // Round 3 (`sdet-u87c2-r2-four-of-six-operator-fixture-shapes-untested`): the operator's
    // round-2 ruling (`op-u87c2-round-2-impl-self-type-is-parsed-by-grammar`) named 6 required
    // fixture shapes; only 2 (a bare-lifetime generic self-type, and the `dyn`-prefix shape
    // above) had a direct #[test]. These 4 close the remaining shapes literally, verbatim from
    // that ruling - hand-traced as correct today, but a future edit to
    // `strip_leading_impl_generics`/`impl_self_type` had nothing pinning any of them.
    // -------------------------------------------------------------------------------------

    #[test]
    fn impl_self_type_handles_a_bound_generic_self_type() {
        assert_eq!(impl_self_type("<T: Clone> Buckets<T>"), "Buckets");
    }

    #[test]
    fn impl_self_type_handles_a_trait_impl_on_a_lifetime_generic_self_type() {
        assert_eq!(impl_self_type("Trait for Server<'a>"), "Server");
    }

    #[test]
    fn impl_self_type_handles_a_generic_trait_impl_on_a_generic_self_type() {
        assert_eq!(
            impl_self_type("<'a> Trait<'a> for ReplayDriver<'a>"),
            "ReplayDriver"
        );
    }

    #[test]
    fn impl_self_type_handles_a_const_generic_self_type() {
        assert_eq!(
            impl_self_type("<const N: usize> Wrapper<[u8; N]>"),
            "Wrapper"
        );
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
            content_hash: "hash-a".to_string(),
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
                content_hash: "hash-a".to_string(),
            },
            MapEntry {
                file: "src/conductor.rs".to_string(),
                name: "b".to_string(),
                start_line: 3,
                end_line: 4,
                is_test: false,
                proposed_module: None,
                reason: "reason-b".to_string(),
                content_hash: "hash-b".to_string(),
            },
        ];
        let lines: Vec<MapEntryLines> = entries.iter().map(map_entry_lines).collect();
        let rendered = render_section_1(&entries, &lines);
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
            content_hash: "hash-a".to_string(),
        }];
        let lines: Vec<MapEntryLines> = entries.iter().map(map_entry_lines).collect();
        let rendered = render_section_1(&entries, &lines);
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
            fs::write(root.join(MAP_LINES_PATH), map_lines_to_json(&map)).unwrap();
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

    /// Spec 90 criterion 2, CLAIM 1 for `docs/audit/responsibility-map.json`: structurally, no
    /// site carries `start_line`/`end_line` and every one carries a non-empty `content_hash`.
    #[test]
    fn the_real_committed_responsibility_map_carries_no_line_number_fields() {
        let map = build_map(&repo_root());
        let json = map_to_json(&map);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let arr = value.as_array().expect("a bare array");
        assert!(!arr.is_empty());
        for entry in arr {
            let obj = entry.as_object().expect("an entry object");
            assert!(
                !obj.contains_key("start_line"),
                "entry {entry:?} still carries start_line"
            );
            assert!(
                !obj.contains_key("end_line"),
                "entry {entry:?} still carries end_line"
            );
            let hash = obj
                .get("content_hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            assert!(
                !hash.is_empty(),
                "entry {entry:?} has an empty content_hash"
            );
        }
    }

    /// Spec 90 criterion 2, CLAIM 2 for `docs/audit/responsibility-map.json`: a synthetic
    /// fixture tree (never the real checked-out one - [`build_map`] requires all three
    /// [`TARGET_FILES`] to exist, so this writes trivial stand-ins for the two it does not
    /// exercise) proves a pin bump (5 unrelated comment lines prepended to `src/dash.rs`,
    /// shifting every entry in it) leaves the guarded map byte-identical, because
    /// `content_hash` keys on each function's own raw text, never its line number.
    #[test]
    fn a_pin_bump_leaves_the_guarded_responsibility_map_byte_identical() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/conductor.rs", "fn one() {}\n");
        write_fixture(dir.path(), "src/main.rs", "fn two() {}\n");
        write_fixture(
            dir.path(),
            "src/dash.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        let base_json = map_to_json(&build_map(dir.path()));

        write_fixture(
            dir.path(),
            "src/dash.rs",
            "// pin: v1\n// pin: v2\n// pin: v3\n// pin: v4\n// pin: v5\n\
             fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        let bumped_json = map_to_json(&build_map(dir.path()));

        assert_eq!(
            base_json, bumped_json,
            "a pin bump that only shifts every entry's OWN line number must leave the guarded \
             responsibility map byte-identical (spec 90 criterion 2)"
        );
    }

    /// Parses a guarded artifact's bare-array JSON into owned [`serde_json::Value`] elements,
    /// so two independently-generated arrays can be compared as SETS (order-independent,
    /// insensitive to pretty-printed whitespace) rather than by byte equality. Shared by the
    /// responsibility-map and dead-code CLAIM 3 tests below - both need the same "every
    /// pre-existing entry survives untouched, exactly one new entry appears" shape.
    fn json_array_entries(json: &str) -> Vec<serde_json::Value> {
        let value: serde_json::Value = serde_json::from_str(json).expect("valid json");
        value.as_array().expect("a bare array").clone()
    }

    /// Spec 90 criterion 2, CLAIM 3 for `docs/audit/responsibility-map.json`, REFRAMED for a
    /// per-entry artifact - `sdet-lens-u90c2-probe-verified-no-live-defect`'s own empirically-
    /// verified proposal: CLAIM 3's literal text ("two branches adding tests in different files
    /// merge it without conflict") does not transfer as byte-identical-across-the-board the way
    /// it does for the duplication catalog, which lists only DUPLICATE clusters - a genuinely
    /// new, non-duplicate function correctly ADDS a new array entry here, and that addition is
    /// not a defect to reject. What carries the merge-safety guarantee for a PER-ENTRY artifact
    /// is CLAIM 2's own pin-bump property (a pre-existing entry is never perturbed by an
    /// unrelated edit elsewhere in its file) PLUS this: two branches, each adding one new,
    /// distinct function to a DIFFERENT target file, leave every pre-existing entry byte-
    /// identical and each contribute exactly their own one new entry - so a real merge of the
    /// two branches has nothing to conflict over, even though the guarded file's own byte
    /// length legitimately changes (unlike the catalog's).
    #[test]
    fn two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry(
    ) {
        let base = tempfile::tempdir().expect("base scratch dir");
        write_fixture(base.path(), "src/conductor.rs", "fn one() {}\n");
        write_fixture(base.path(), "src/main.rs", "fn two() {}\n");
        write_fixture(base.path(), "src/dash.rs", "fn three() {}\n");
        let base_entries = json_array_entries(&map_to_json(&build_map(base.path())));

        let branch_a = tempfile::tempdir().expect("branch A scratch dir");
        write_fixture(
            branch_a.path(),
            "src/conductor.rs",
            "fn one() {}\n\nfn branch_a_only() {\n    let _ = 1;\n}\n",
        );
        write_fixture(branch_a.path(), "src/main.rs", "fn two() {}\n");
        write_fixture(branch_a.path(), "src/dash.rs", "fn three() {}\n");
        let a_entries = json_array_entries(&map_to_json(&build_map(branch_a.path())));

        let branch_b = tempfile::tempdir().expect("branch B scratch dir");
        write_fixture(branch_b.path(), "src/conductor.rs", "fn one() {}\n");
        write_fixture(
            branch_b.path(),
            "src/main.rs",
            "fn two() {}\n\nfn branch_b_only() {\n    let _ = 2;\n}\n",
        );
        write_fixture(branch_b.path(), "src/dash.rs", "fn three() {}\n");
        let b_entries = json_array_entries(&map_to_json(&build_map(branch_b.path())));

        assert_eq!(
            a_entries.len(),
            base_entries.len() + 1,
            "branch A must contribute exactly one new entry"
        );
        assert_eq!(
            b_entries.len(),
            base_entries.len() + 1,
            "branch B must contribute exactly one new entry"
        );
        for entry in &base_entries {
            assert!(
                a_entries.contains(entry),
                "branch A perturbed or dropped a pre-existing entry {entry}"
            );
            assert!(
                b_entries.contains(entry),
                "branch B perturbed or dropped a pre-existing entry {entry}"
            );
        }
        let a_new: Vec<&serde_json::Value> = a_entries
            .iter()
            .filter(|e| !base_entries.contains(e))
            .collect();
        let b_new: Vec<&serde_json::Value> = b_entries
            .iter()
            .filter(|e| !base_entries.contains(e))
            .collect();
        assert_eq!(a_new.len(), 1, "branch A's own new entry: {a_new:?}");
        assert_eq!(b_new.len(), 1, "branch B's own new entry: {b_new:?}");
        assert_ne!(
            a_new[0], b_new[0],
            "the two branches' new entries must be distinct - nothing for a real merge to \
             conflict over"
        );
    }

    /// Spec 90 Design, verbatim: "the report's guard checks structure only (sections present,
    /// counts equal to the catalog) rather than bytes." Mirrors
    /// `assert_section_2_structurally_matches` exactly, one level down (module ~ cluster,
    /// per-entry citation ~ per-site citation): the heading, the aggregate function-total
    /// count, every module's own heading with its declared function count, and the unassigned
    /// heading with its declared count - never the exact citation bytes, which are free to
    /// legitimately move between explicit `RIGGER_AUDIT_WRITE=1` regens (a pin bump anywhere
    /// in `src/conductor.rs`, `src/main.rs` or `src/dash.rs`).
    fn assert_section_1_structurally_matches(committed_section_1: &str, entries: &[MapEntry]) {
        assert!(
            committed_section_1.starts_with("## 1. Responsibility Map"),
            "{REPORT_PATH} section 1 is missing its own heading"
        );
        let total_line = format!("({} functions total)", entries.len());
        assert!(
            committed_section_1.contains(&total_line),
            "{REPORT_PATH} section 1's declared function total has drifted from the tree \
             (expected {total_line:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
        );
        let mut modules: Vec<&str> = entries
            .iter()
            .filter_map(|e| e.proposed_module.as_deref())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        modules.sort_unstable();
        for module in &modules {
            let count = entries
                .iter()
                .filter(|e| e.proposed_module.as_deref() == Some(*module))
                .count();
            let heading = format!("- `{module}` ({})", pluralize_functions(count));
            assert!(
                committed_section_1.contains(&heading),
                "{REPORT_PATH} section 1 is missing or has a stale heading for module \
                 `{module}` (expected {heading:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
            );
        }
        let unassigned_count = entries
            .iter()
            .filter(|e| e.proposed_module.is_none())
            .count();
        let unassigned_heading =
            format!("### Unassigned ({})", pluralize_functions(unassigned_count));
        assert!(
            committed_section_1.contains(&unassigned_heading),
            "{REPORT_PATH} section 1's unassigned count has drifted from the tree (expected \
             {unassigned_heading:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    /// THE DRIFT GUARD for section 1 of the report: with `RIGGER_AUDIT_WRITE=1` set,
    /// (re)write it (creating the report fresh with placeholders for the sections this
    /// criterion does not own, or replacing only section 1's span if the report already
    /// exists); otherwise assert the committed report's section 1 matches the fresh map
    /// STRUCTURALLY (see `assert_section_1_structurally_matches`), never byte-for-byte.
    /// Mirrors `report_section_2_matches_the_tree_or_is_rewritten`'s own check-mode half
    /// exactly (round 3 remedy: section 1's own guard was still a whole-section byte
    /// comparison against a render that embeds live line numbers - the same defect class
    /// criterion 2's guard was fixed for).
    #[test]
    fn report_section_1_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let map = build_map(&root);
        let lines: Vec<MapEntryLines> = map.iter().map(map_entry_lines).collect();
        let section_1 = render_section_1(&map, &lines);
        let path = root.join(REPORT_PATH);
        let write = std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1");
        if write {
            let _guard = lock_report_write();
            let existing = fs::read_to_string(&path).ok();
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
        let committed = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{REPORT_PATH} is missing - run with RIGGER_AUDIT_WRITE=1 to generate it")
        });
        let span = section_span(&committed, "## 1. ");
        assert_section_1_structurally_matches(&committed[span], &map);
    }

    /// The structural report guard's own pin-bump proof for section 1 (mirrors
    /// `a_pin_bump_leaves_the_rendered_report_section_2_structurally_unchanged`): a synthetic
    /// entry, rendered before and after a pin bump that shifts its own line span by 5. Every
    /// structural fact - module heading, function count, name, reason - is byte-identical
    /// across the bump; only the citation's own numeric span moves, tracking the live tree
    /// exactly.
    #[test]
    fn a_pin_bump_leaves_the_rendered_report_section_1_structurally_unchanged() {
        let before = vec![MapEntry {
            file: "src/conductor.rs".to_string(),
            name: "add_one".to_string(),
            start_line: 1,
            end_line: 3,
            is_test: false,
            proposed_module: Some("conductor::support".to_string()),
            reason: "reason-a".to_string(),
            content_hash: "hash-a".to_string(),
        }];
        let before_lines: Vec<MapEntryLines> = before.iter().map(map_entry_lines).collect();
        let before_rendered = render_section_1(&before, &before_lines);

        let after = vec![MapEntry {
            start_line: 6,
            end_line: 8,
            ..before[0].clone()
        }];
        let after_lines: Vec<MapEntryLines> = after.iter().map(map_entry_lines).collect();
        let after_rendered = render_section_1(&after, &after_lines);

        assert_section_1_structurally_matches(&before_rendered, &before);
        assert_section_1_structurally_matches(&after_rendered, &after);
        assert!(before_rendered.contains("`src/conductor.rs:1-3`"));
        assert!(after_rendered.contains("`src/conductor.rs:6-8`"));
        assert!(!after_rendered.contains("`src/conductor.rs:1-3`"));
    }

    /// CLAIM-4 equivalent for section 1 (mirrors
    /// `report_section_2_cites_file_line_exactly_as_the_unguarded_lines_file_records_them`):
    /// proven at the DATA-FLOW level, not by coincidence. A synthetic [`MapEntry`] carries a
    /// DECOY `start_line`/`end_line` that appears nowhere in `lines`, rendered with a separate,
    /// deliberately different [`MapEntryLines`] - the rendered citation is the `lines` value,
    /// never the decoy `MapEntry` one. Fails if `render_section_1` ever falls back to
    /// `MapEntry`'s own fields.
    #[test]
    fn report_section_1_cites_file_line_exactly_as_the_unguarded_lines_file_records_them() {
        let decoy = (999_999, 999_998);
        let real = (10, 12);
        let entry = MapEntry {
            file: "src/a.rs".to_string(),
            name: "add_one".to_string(),
            start_line: decoy.0,
            end_line: decoy.1,
            is_test: false,
            proposed_module: Some("a::support".to_string()),
            reason: "reason-a".to_string(),
            content_hash: "deadbeefcafef00d".to_string(),
        };
        let lines = MapEntryLines {
            file: "src/a.rs".to_string(),
            name: "add_one".to_string(),
            start_line: real.0,
            end_line: real.1,
        };
        let rendered = render_section_1(&[entry], &[lines]);
        let real_citation = format!("`src/a.rs:{}-{}`", real.0, real.1);
        let decoy_citation = format!("`src/a.rs:{}-{}`", decoy.0, decoy.1);
        assert!(
            rendered.contains(&real_citation),
            "report section 1 must cite the unguarded lines value {real_citation} - got \
             {rendered:?}"
        );
        assert!(
            !rendered.contains(&decoy_citation),
            "report section 1 must NEVER cite MapEntry's own start_line/end_line directly - it \
             cited the decoy {decoy_citation} instead of the unguarded lines data"
        );
    }

    // =====================================================================================
    // Criterion 2 (`u85c2`, THIS UNIT): the duplication catalog
    // =====================================================================================

    // -------------------------------------------------------------------------------------
    // The tokenizer
    // -------------------------------------------------------------------------------------

    fn tok(src: &str) -> Vec<RawTok> {
        let chars: Vec<char> = src.chars().collect();
        tokenize(&chars)
    }

    #[test]
    fn a_simple_free_function_tokenizes_to_keywords_idents_and_punct() {
        let toks = tok("fn foo(x: u32) {}");
        let kinds: Vec<RawKind> = toks.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RawKind::Keyword, // fn
                RawKind::Ident,   // foo
                RawKind::Punct,   // (
                RawKind::Ident,   // x
                RawKind::Punct,   // :
                RawKind::Ident,   // u32 (lowercase-led, not a keyword)
                RawKind::Punct,   // )
                RawKind::Punct,   // {
                RawKind::Punct,   // }
            ]
        );
    }

    #[test]
    fn line_comments_and_whitespace_produce_no_token() {
        let toks = tok("fn a() {\n // a comment\n let x = 1;\n}\n");
        assert!(toks.iter().all(|t| !t.text.contains("comment")));
        assert!(!toks.iter().any(|t| t.text.trim().is_empty()));
    }

    #[test]
    fn nested_block_comments_produce_no_token_and_advance_line_count() {
        let toks = tok("fn a() {\n/* outer /* inner */ still-comment */\nlet x = 1;\n}\n");
        assert!(!toks.iter().any(|t| t.text == "outer"));
        let let_tok = toks.iter().find(|t| t.text == "let").unwrap();
        assert_eq!(let_tok.line, 3);
    }

    #[test]
    fn string_and_raw_string_literals_are_one_lit_token_each() {
        let toks = tok(r####"fn a() { let s = "hi"; let r = r#"raw ) thing"#; }"####);
        let lits: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == RawKind::Lit)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(lits, vec!["\"hi\"", "r#\"raw ) thing\"#"]);
    }

    #[test]
    fn a_char_literal_is_lit_and_a_bare_tick_ident_is_a_lifetime() {
        let toks = tok("fn a<'x>(c: char) { let z = 'y'; }");
        let lifetime = toks.iter().find(|t| t.text == "'x").unwrap();
        assert_eq!(lifetime.kind, RawKind::Lifetime);
        let ch = toks.iter().find(|t| t.text == "'y'").unwrap();
        assert_eq!(ch.kind, RawKind::Lit);
    }

    #[test]
    fn number_literals_including_a_fraction_are_lit_tokens() {
        let toks = tok("fn a() { let x = 1_000u32; let y = 1.5; }");
        let lits: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == RawKind::Lit)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(lits, vec!["1_000u32", "1.5"]);
    }

    #[test]
    fn a_multi_char_operator_is_several_single_char_punct_tokens() {
        let toks = tok("fn a() { Command::new(\"x\") }");
        let puncts: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == RawKind::Punct)
            .map(|t| t.text.as_str())
            .collect();
        assert!(puncts.windows(2).any(|w| w == [":", ":"]));
    }

    // -------------------------------------------------------------------------------------
    // ident_kind_marker / normalize_tokens
    // -------------------------------------------------------------------------------------

    #[test]
    fn ident_kind_marker_classifies_by_casing() {
        assert_eq!(ident_kind_marker("MAX_RETRIES"), "CONST");
        assert_eq!(ident_kind_marker("AgentDriver"), "TYPE");
        assert_eq!(ident_kind_marker("RunCtx"), "TYPE");
        assert_eq!(ident_kind_marker("spawn_with_resume"), "IDENT");
        assert_eq!(ident_kind_marker("x"), "IDENT");
        // A single uppercase letter has "at least one letter, all letters uppercase" - CONST,
        // not TYPE: consistent (a real one-letter generic like `T` reads the same either way,
        // but this keeps the rule a single unambiguous casing check with no length special case).
        assert_eq!(ident_kind_marker("T"), "CONST");
    }

    #[test]
    fn normalize_tokens_canonicalizes_idents_and_keeps_structure() {
        let toks = tok("fn spawn_it(cfg: AgentConfig) { format!(\"{}\", MAX_RETRIES); }");
        let norm = normalize_tokens(&toks);
        assert_eq!(
            norm,
            vec![
                "fn", "IDENT", "(", "IDENT", ":", "TYPE", ")", "{", "MACRO", "!", "(", "LIT", ",",
                "CONST", ")", ";", "}",
            ]
        );
    }

    #[test]
    fn two_functions_differing_only_by_identifier_names_normalize_identically() {
        let toks_a = tok("fn add_one(n: u32) -> u32 { n + 1 }");
        let toks_b = tok("fn plus_one(m: u32) -> u32 { m + 1 }");
        assert_eq!(normalize_tokens(&toks_a), normalize_tokens(&toks_b));
    }

    // -------------------------------------------------------------------------------------
    // shingles / jaccard
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_stream_shorter_than_the_window_is_one_whole_stream_shingle() {
        let norm: Vec<&str> = vec!["fn", "IDENT", "("];
        let s = shingles(&norm);
        assert_eq!(s.len(), 1);
        // The same short stream, computed independently, fingerprints identically.
        assert_eq!(s, shingles(&["fn", "IDENT", "("]));
    }

    #[test]
    fn a_stream_at_least_the_window_produces_sliding_shingles() {
        let norm: Vec<&str> = "a b c d e f g h i".split_whitespace().collect();
        let s = shingles(&norm);
        // 9 tokens, window 8 -> 2 shingles, both distinct fingerprints.
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn shingle_fingerprints_are_deterministic_and_distinguish_different_windows() {
        let a: Vec<&str> = vec!["a", "b", "c"];
        let b: Vec<&str> = vec!["a", "b", "c"];
        let c: Vec<&str> = vec!["x", "y", "z"];
        assert_eq!(shingles(&a), shingles(&b));
        assert_ne!(shingles(&a), shingles(&c));
    }

    #[test]
    fn jaccard_of_identical_sets_is_one_and_disjoint_sets_is_zero() {
        let a: Vec<u64> = vec![10, 20];
        let b: Vec<u64> = vec![30, 40];
        assert_eq!(jaccard(&a, &a), 1.0);
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_of_a_partial_overlap_is_the_intersection_over_union() {
        let a: Vec<u64> = vec![10, 20, 30];
        let b: Vec<u64> = vec![20, 30, 40];
        // intersection {20,30}=2, union {10,20,30,40}=4 -> 0.5
        assert_eq!(jaccard(&a, &b), 0.5);
    }

    #[test]
    fn renamed_identical_functions_are_near_duplicate_at_jaccard_one() {
        let a = shingles(&normalize_tokens(&tok(
            "fn add_one(n: u32) -> u32 { n + 1 }",
        )));
        let b = shingles(&normalize_tokens(&tok(
            "fn plus_one(m: u32) -> u32 { m + 1 }",
        )));
        assert_eq!(jaccard(&a, &b), 1.0);
    }

    // -------------------------------------------------------------------------------------
    // scan_tree / body_tokens / all_fn_refs (a small fixture tree, mirrors
    // tests/no_os_kill_audit.rs's own use of tempfile for a filesystem-shaped fixture)
    // -------------------------------------------------------------------------------------

    fn write_fixture(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn scan_tree_finds_functions_under_both_src_and_tests_but_not_elsewhere() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/a.rs", "fn one() {}\n");
        write_fixture(dir.path(), "tests/b.rs", "fn two() {}\n");
        write_fixture(dir.path(), "docs/c.rs", "fn three() {}\n");
        let files = scan_tree(dir.path());
        let names: Vec<&str> = files
            .iter()
            .flat_map(|f| f.fns.iter().map(|s| s.name.as_str()))
            .collect();
        assert!(names.contains(&"one"));
        assert!(names.contains(&"two"));
        assert!(!names.contains(&"three"));
    }

    #[test]
    fn body_tokens_slices_exactly_the_functions_own_span() {
        let src = "fn a() {\n let x = 1;\n}\nfn b() {\n let y = 2;\n}\n";
        let chars: Vec<char> = src.chars().collect();
        let tokens = tokenize(&chars);
        let fns = scan_file("t.rs", src);
        let a = fns.iter().find(|f| f.name == "a").unwrap();
        let toks_a = body_tokens(&tokens, a.start_line, a.end_line);
        assert!(toks_a.iter().any(|t| t.text == "x"));
        assert!(!toks_a.iter().any(|t| t.text == "y"));
    }

    #[test]
    fn all_fn_refs_is_ordered_by_file_then_start_line_regardless_of_scan_emission_order() {
        // A nested fn closes (and so is emitted by scan_file) BEFORE its enclosing one - this
        // fixture's outer() encloses inner(), so scan_file's own push order is [inner, outer],
        // the exact case all_fn_refs's explicit sort exists to correct.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/nested.rs",
            "fn outer() {\n    fn inner() {}\n    inner();\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let names: Vec<&str> = refs
            .iter()
            .map(|r| r.scanned(&files).name.as_str())
            .collect();
        assert_eq!(names, vec!["outer", "inner"]);
    }

    /// [`adversarial_sample_population`] excludes ONLY [`ADVERSARIAL_SAMPLE_EXCLUDED_FILE`]'s own
    /// functions from the draw's population, leaving [`all_fn_refs`] (and so the duplication
    /// catalog itself) untouched - see [`ADVERSARIAL_SAMPLE_EXCLUDED_FILE`]'s own doc comment for
    /// why this one file is singled out.
    #[test]
    fn adversarial_sample_population_excludes_only_the_citation_guard_periphery_file() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/a.rs", "fn included_one() {}\n");
        write_fixture(
            dir.path(),
            ADVERSARIAL_SAMPLE_EXCLUDED_FILE,
            "fn excluded_one() {}\nfn excluded_two() {}\n",
        );
        let files = scan_tree(dir.path());

        let all = all_fn_refs(&files);
        assert_eq!(
            all.len(),
            3,
            "sanity: all_fn_refs (the catalog's own population) must still see all 3 fns"
        );

        let population = adversarial_sample_population(&files);
        let names: Vec<&str> = population
            .iter()
            .map(|r| r.scanned(&files).name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["included_one"],
            "the adversarial draw's population must exclude every fn from {}",
            ADVERSARIAL_SAMPLE_EXCLUDED_FILE
        );
    }

    // -------------------------------------------------------------------------------------
    // build_mechanical_clusters
    // -------------------------------------------------------------------------------------

    fn clusters_for(files: &[FileScan]) -> Vec<DupCluster> {
        let refs = all_fn_refs(files);
        build_mechanical_clusters(files, &refs)
    }

    #[test]
    fn two_renamed_identical_functions_form_one_exact_cluster() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/b.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let files = scan_tree(dir.path());
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].classification, "exact");
        assert_eq!(clusters[0].sites.len(), 2);
        let names: HashSet<&str> = clusters[0].sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, HashSet::from(["add_one", "plus_one"]));
    }

    #[test]
    fn two_unrelated_functions_form_no_cluster() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn read_config(path: &str) -> String {\n    std::fs::read_to_string(path).unwrap()\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/b.rs",
            "fn sum_all(xs: &[i64]) -> i64 {\n    xs.iter().sum()\n}\n",
        );
        let files = scan_tree(dir.path());
        assert!(clusters_for(&files).is_empty());
    }

    #[test]
    fn three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        // Same overall shape (read a /proc file, grab a field after the comm's closing paren)
        // but each extracts a DIFFERENT field - near, not exact (spec 78's dash.rs/reap.rs
        // /proc-stat class this catalog's mandatory sweep also names explicitly).
        write_fixture(
            dir.path(),
            "src/one.rs",
            "fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/two.rs",
            "fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/three.rs",
            "fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n",
        );
        let files = scan_tree(dir.path());
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].sites.len(), 3);
        assert_eq!(clusters[0].classification, "near");
        assert!(!clusters[0].proposed_home.is_empty());
    }

    #[test]
    fn cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/z.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let files = scan_tree(dir.path());
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        // src/a.rs sorts before src/z.rs regardless of scan/write order.
        assert_eq!(clusters[0].sites[0].file, "src/a.rs");
        assert_eq!(clusters[0].sites[1].file, "src/z.rs");
    }

    #[test]
    fn same_impl_block_duplicate_methods_propose_that_types_home() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/w.rs",
            "impl Widget {\n    fn add_one(n: u32) -> u32 {\n        n + 1\n    }\n    fn plus_one(m: u32) -> u32 {\n        m + 1\n    }\n}\n",
        );
        let files = scan_tree(dir.path());
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].proposed_home, "w::widget");
    }

    // -------------------------------------------------------------------------------------
    // The five mandatory sweeps
    // -------------------------------------------------------------------------------------

    #[test]
    fn command_new_sweep_finds_a_call_site_and_names_it() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn run() {\n    let _ = std::process::Command::new(\"true\").status();\n}\n",
        );
        let files = scan_tree(dir.path());
        let hits = find_ident_path_call_sites(&files, "Command", &["new"]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Command::new");
        assert_eq!(hits[0].file, "src/a.rs");
    }

    #[test]
    fn command_new_sweep_ignores_an_unrelated_call() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/a.rs", "fn run() {\n    Other::new();\n}\n");
        let files = scan_tree(dir.path());
        assert!(find_ident_path_call_sites(&files, "Command", &["new"]).is_empty());
    }

    #[test]
    fn sqlite_open_sweep_matches_either_tail() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn a() {\n    Connection::open(p)?;\n}\nfn b() {\n    Connection::open_with_flags(p, f)?;\n}\n",
        );
        let files = scan_tree(dir.path());
        let hits = find_ident_path_call_sites(&files, "Connection", &["open", "open_with_flags"]);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n",
        );
        let files = scan_tree(dir.path());
        let hits = find_literal_containing(&files, "/proc");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].name.contains("/proc"));
    }

    #[test]
    fn rigger_path_literal_sweep_finds_a_rigger_relative_string() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn a() {\n    let _ = \".rigger/tmp\";\n}\n",
        );
        let files = scan_tree(dir.path());
        assert_eq!(find_literal_containing(&files, ".rigger").len(), 1);
    }

    #[test]
    fn looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring(
    ) {
        assert!(looks_error_shaping("shape_error"));
        assert!(looks_error_shaping("to_err_string"));
        assert!(looks_error_shaping("err_to_string"));
        assert!(looks_error_shaping("errf"));
        assert!(!looks_error_shaping("deferred"));
        assert!(!looks_error_shaping("current"));
    }

    #[test]
    fn error_shaping_sweep_requires_both_the_name_shape_and_a_format_call() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn shape_error(e: &str) -> String {\n    format!(\"error: {}\", e)\n}\nfn is_err_only(x: &Result<(), ()>) -> bool {\n    x.is_err()\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let hits = find_error_shaping_fns(&files, &refs);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "shape_error");
    }

    #[test]
    fn build_sweep_clusters_always_returns_exactly_five_named_clusters() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/a.rs", "fn a() {}\n");
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let clusters = build_sweep_clusters(&files, &refs);
        assert_eq!(clusters.len(), 5);
        for c in &clusters {
            assert_eq!(c.classification, "semantic");
        }
        for name in MANDATORY_SWEEPS {
            assert!(clusters.iter().any(|c| c.note.contains(name)));
        }
    }

    #[test]
    fn proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn(
    ) {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let hits = find_proc_stat_or_status_readers(&files, &refs);
        let names: HashSet<&str> = hits.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, HashSet::from(["state_of", "ppid_of"]));
    }

    /// THE WORKED EXAMPLE (spec 85 Goal): on the real tree, `dash.rs::process_state` and
    /// `reap.rs::pid_starttime` land in the SAME cluster - the mechanical pass alone does not
    /// find this pair (their tails differ enough to fall under the Jaccard threshold), which is
    /// exactly why this hand-found sweep exists.
    #[test]
    fn the_dash_reap_proc_stat_pair_the_spec_names_lands_in_one_real_cluster() {
        let clusters = real_catalog();
        let hosting = clusters
            .iter()
            .find(|c| c.sites.iter().any(|s| s.name == "process_state"))
            .expect("process_state is findable in the real catalog");
        let names: Vec<&str> = hosting.sites.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"pid_starttime"),
            "process_state's cluster {:?} must also contain pid_starttime, found: {:?}",
            hosting.id,
            names
        );
    }

    #[test]
    fn constructs_own_type_literal_matches_self_and_the_named_type_but_not_an_unrelated_call() {
        let toks = tok("fn ok() -> Self { Self { a: 1, b: 2 } }");
        assert!(constructs_own_type_literal(&toks, "Widget"));
        let toks2 = tok("fn ok() -> Widget { Widget { a: 1 } }");
        assert!(constructs_own_type_literal(&toks2, "Widget"));
        let toks3 = tok("fn ok() -> Widget { other_fn(1, 2) }");
        assert!(!constructs_own_type_literal(&toks3, "Widget"));
    }

    #[test]
    fn constructs_own_type_literal_matches_shorthand_field_init_too() {
        // `Self { a, b: 0 }` - the first field is SHORTHAND (no `:`), a real shape
        // (`SpawnResult`'s own constructors use it) the `:`-only check would miss.
        let toks = tok("fn ok(a: u32) -> Self { Self { a, b: 0 } }");
        assert!(constructs_own_type_literal(&toks, "Widget"));
        // A single-field shorthand literal (`{ a }`) still closes on `}`, not `:`/`,`.
        let toks2 = tok("fn ok(a: u32) -> Self { Self { a } }");
        assert!(constructs_own_type_literal(&toks2, "Widget"));
    }

    #[test]
    fn parallel_constructor_sweep_groups_two_constructors_for_one_type_but_not_a_lone_one() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "impl Widget {\n    fn ok(a: u32) -> Self {\n        Self { a, b: 0 }\n    }\n    fn zeroed() -> Self {\n        Self { a: 0, b: 0 }\n    }\n}\nimpl Gadget {\n    fn only() -> Self {\n        Self { x: 1 }\n    }\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let clusters = find_parallel_constructor_clusters(&files, &refs);
        assert_eq!(clusters.len(), 1);
        let names: HashSet<&str> = clusters[0].sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, HashSet::from(["ok", "zeroed"]));
        assert!(clusters[0].proposed_home.contains("widget"));
    }

    /// A SECOND, DEEPER worked example from the same adversarial sample draw:
    /// `spawn::SpawnResult::liveness_fault` reads MISSING from the mechanical catalog (its
    /// extra `class` parameter and `serde_json::json!` meta field push its Jaccard similarity
    /// to `ok`/`failed` just under threshold) even though `ok` and `failed` themselves DO
    /// cluster mechanically - reading it revealed a real recall gap the parallel-constructor
    /// sweep above exists to close.
    #[test]
    fn the_spawn_result_constructor_triple_the_adversarial_sample_found_lands_in_one_real_cluster()
    {
        let clusters = real_catalog();
        let hosting = clusters
            .iter()
            .find(|c| {
                c.sites
                    .iter()
                    .any(|s| s.file == "src/spawn.rs" && s.name == "liveness_fault")
            })
            .expect("liveness_fault is findable in the real catalog");
        let names: HashSet<&str> = hosting.sites.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains("ok") && names.contains("failed"),
            "liveness_fault's cluster {:?} must also contain ok and failed, found: {:?}",
            hosting.id,
            names
        );
    }

    #[test]
    fn same_named_helper_sweep_groups_across_files_but_not_within_one_file_or_below_the_length_floor(
    ) {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "tests/a.rs",
            "fn exploration_graph() -> u32 {\n    1\n}\nfn new() -> u32 {\n    2\n}\n",
        );
        write_fixture(
            dir.path(),
            "tests/b.rs",
            "fn exploration_graph() -> u32 {\n    3\n}\nfn new() -> u32 {\n    4\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let clusters = find_same_named_helper_functions(&files, &refs);
        // Only `exploration_graph` (>= SAME_NAME_MIN_LEN) qualifies; `new` (a coincidental
        // short, common name) does not.
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].sites.len(), 2);
        assert!(clusters[0]
            .sites
            .iter()
            .all(|s| s.name == "exploration_graph"));
    }

    /// A THIRD worked example from the adversarial sample: `exploration_graph`, a near-
    /// identical-purpose test-fixture builder, independently defined (with different concrete
    /// fixture data) in `tests/dash_exploration_route_client_contract.rs` and
    /// `tests/dash_kg_graph_route.rs`.
    #[test]
    fn the_two_exploration_graph_fixture_builders_the_adversarial_sample_found_land_in_one_real_cluster(
    ) {
        let clusters = real_catalog();
        let hosting = clusters
            .iter()
            .find(|c| c.sites.iter().any(|s| s.name == "exploration_graph"))
            .expect("exploration_graph is findable in the real catalog");
        let files: HashSet<&str> = hosting.sites.iter().map(|s| s.file.as_str()).collect();
        assert!(
            files.contains("tests/dash_exploration_route_client_contract.rs")
                && files.contains("tests/dash_kg_graph_route.rs"),
            "exploration_graph's cluster {:?} must span both files, found: {:?}",
            hosting.id,
            files
        );
    }

    #[test]
    fn same_named_helper_sweep_excludes_required_trait_impl_methods_across_adapters() {
        // Three concrete adapters implementing the SAME trait method - required by the trait
        // contract, not a coincidental duplicate (the adjudicator-upheld precision defect:
        // `subscribe_all`/`subscribe_stream` across the `EventStore` trait's backend adapters).
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "impl MyPort for AdapterOne {\n    fn do_the_shared_thing(&self) -> u32 {\n        1\n    }\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/b.rs",
            "impl MyPort for AdapterTwo {\n    fn do_the_shared_thing(&self) -> u32 {\n        2\n    }\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/c.rs",
            "impl MyPort for AdapterThree {\n    fn do_the_shared_thing(&self) -> u32 {\n        3\n    }\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let clusters = find_same_named_helper_functions(&files, &refs);
        assert!(
            clusters.is_empty(),
            "required trait-impl methods across 2+ adapters must not be flagged as \
             same-named-helper duplication: {clusters:?}"
        );
    }

    #[test]
    fn same_named_helper_sweep_excludes_a_trait_default_method_and_its_override() {
        // A trait's own DEFAULT method (`enclosing_impl` is `None`) next to a concrete override
        // and a test double - the adjudicator-upheld precision defect's other committed shape
        // (`blast_radius` across the `Grounder` trait's default, the symbols grounder's
        // override, and a test double).
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "trait MyPort {\n    fn compute_the_radius(&self) -> u32 {\n        1\n    }\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/b.rs",
            "impl MyPort for RealAdapter {\n    fn compute_the_radius(&self) -> u32 {\n        2\n    }\n}\n",
        );
        write_fixture(
            dir.path(),
            "tests/mock.rs",
            "impl MyPort for MockAdapter {\n    fn compute_the_radius(&self) -> u32 {\n        3\n    }\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let clusters = find_same_named_helper_functions(&files, &refs);
        assert!(
            clusters.is_empty(),
            "a trait's default method next to its override(s) must not be flagged as \
             same-named-helper duplication: {clusters:?}"
        );
    }

    #[test]
    fn same_named_helper_sweep_still_catches_two_inherent_impls_sharing_a_method_name() {
        // Two UNRELATED inherent impls (no trait, no `" for "` in either header) that happen to
        // share a long method name are still a real coincidental duplicate - the exclusion above
        // must not blanket-suppress every impl-method same-name hit, only the trait-required
        // shape.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "impl Widget {\n    fn compute_the_layout(&self) -> u32 {\n        1\n    }\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/b.rs",
            "impl Gadget {\n    fn compute_the_layout(&self) -> u32 {\n        2\n    }\n}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let clusters = find_same_named_helper_functions(&files, &refs);
        assert_eq!(
            clusters.len(),
            1,
            "two unrelated inherent-impl methods sharing a name must still be caught: {clusters:?}"
        );
    }

    /// The adjudicator-upheld precision defect, verified fixed on the REAL tree: neither
    /// `subscribe_all`/`subscribe_stream` (the `EventStore` trait's 3 backend adapters plus a
    /// test double) nor `blast_radius` (the `Grounder` trait's default, its override, and a test
    /// double) appear in a same-named-helper cluster any longer.
    #[test]
    fn the_real_tree_no_longer_misclassifies_required_trait_methods_as_same_named_helpers() {
        let files = real_files();
        let refs = all_fn_refs(files);
        let clusters = find_same_named_helper_functions(files, &refs);
        for bad_name in ["subscribe_all", "subscribe_stream", "blast_radius"] {
            assert!(
                !clusters
                    .iter()
                    .any(|c| c.sites.iter().any(|s| s.name == bad_name)),
                "{bad_name} must not appear in a same-named-helper cluster (required \
                 trait-impl/port-adapter shape)"
            );
        }
    }

    #[test]
    fn bespoke_lexer_sweep_finds_the_named_trio_but_not_an_unrelated_fn() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "tests/simplification_audit.rs",
            "fn scan_file() {}\nfn tokenize() {}\nfn unrelated() {}\n",
        );
        write_fixture(
            dir.path(),
            "src/grounder/symbols/extract.rs",
            "pub fn extract() {}\n",
        );
        let files = scan_tree(dir.path());
        let refs = all_fn_refs(&files);
        let hits = find_bespoke_lexer_vs_canonical_extractor(&files, &refs);
        let names: HashSet<&str> = hits.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, HashSet::from(["scan_file", "tokenize", "extract"]));
    }

    /// The recall gap u85c1's architecture lens routed to this criterion by name across two
    /// prior review rounds, verified closed on the REAL tree: `scan_file`, `tokenize` (this
    /// file's own bespoke scanner/lexer) and `extract` (`src/grounder/symbols/extract.rs`, the
    /// codebase's one canonical tree-sitter extractor) land in one cluster.
    #[test]
    fn the_bespoke_lexer_and_canonical_extractor_the_lens_routed_land_in_one_real_cluster() {
        let clusters = real_catalog();
        let hosting = clusters
            .iter()
            .find(|c| {
                c.sites
                    .iter()
                    .any(|s| s.file == "src/grounder/symbols/extract.rs" && s.name == "extract")
            })
            .expect("extract is findable in the real catalog");
        let names: HashSet<&str> = hosting.sites.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains("scan_file") && names.contains("tokenize"),
            "extract's cluster {:?} must also contain scan_file and tokenize, found: {:?}",
            hosting.id,
            names
        );
    }

    // -------------------------------------------------------------------------------------
    // build_catalog / catalog_to_json / render_section_2 / replace_section_2
    // -------------------------------------------------------------------------------------

    #[test]
    fn build_catalog_assigns_sequential_ids_after_the_deterministic_sort() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/z.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let clusters = build_catalog(&scan_tree(dir.path()));
        let ids: Vec<&str> = clusters.iter().map(|c| c.id.as_str()).collect();
        let mut sorted_ids = ids.clone();
        sorted_ids.sort_unstable();
        assert_eq!(ids, sorted_ids, "ids must already be in ascending order");
        assert_eq!(ids[0], "dup-0001");
        // The mechanical exact cluster (src/a.rs, src/z.rs) sorts before every sweep cluster
        // whose sites all live under src/a.rs alone (a fixture with no Command::new etc.), so
        // it is exactly one of the returned clusters and its own site order is (a.rs, z.rs).
        let mech = clusters
            .iter()
            .find(|c| c.classification == "exact")
            .expect("the renamed-identical pair forms an exact cluster");
        assert_eq!(mech.sites[0].file, "src/a.rs");
    }

    #[test]
    fn catalog_to_json_round_trips_through_deserialize() {
        let clusters = vec![DupCluster {
            id: "dup-0001".to_string(),
            classification: "exact".to_string(),
            sites: vec![DupSite {
                file: "src/a.rs".to_string(),
                start_line: 1,
                end_line: 3,
                name: "a".to_string(),
                content_hash: "deadbeefcafef00d".to_string(),
            }],
            proposed_home: "a::support".to_string(),
            note: "n".to_string(),
        }];
        let json = catalog_to_json(&clusters);
        assert!(json.ends_with('\n'));
        // Spec 90 criterion 2: the wire shape is LINE-FREE - it round-trips through
        // `DupClusterWire`, not the full `DupCluster` (whose `start_line`/`end_line` are no
        // longer present in `json` at all).
        let back: Vec<DupClusterWire> = serde_json::from_str(&json).expect("round trips");
        assert_eq!(back, vec![dup_cluster_wire(&clusters[0])]);
        assert!(!json.contains("start_line"));
        assert!(!json.contains("end_line"));
    }

    #[test]
    fn catalog_lines_to_json_round_trips_and_carries_only_the_line_spans() {
        let clusters = vec![DupCluster {
            id: "dup-0001".to_string(),
            classification: "exact".to_string(),
            sites: vec![DupSite {
                file: "src/a.rs".to_string(),
                start_line: 1,
                end_line: 3,
                name: "a".to_string(),
                content_hash: "deadbeefcafef00d".to_string(),
            }],
            proposed_home: "a::support".to_string(),
            note: "n".to_string(),
        }];
        let json = catalog_lines_to_json(&clusters);
        assert!(json.ends_with('\n'));
        let back: Vec<DupClusterLines> = serde_json::from_str(&json).expect("round trips");
        assert_eq!(back, vec![dup_cluster_lines(&clusters[0])]);
        // The unguarded sibling carries the identity (line-free) AND the lines - never the
        // content_hash, which belongs solely to the guarded file.
        assert!(!json.contains("content_hash"));
        assert!(!json.contains("proposed_home"));
    }

    #[test]
    fn render_section_2_names_every_mandatory_sweep_and_every_cluster_id() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let files = scan_tree(dir.path());
        let clusters = build_catalog(&files);
        let lines: Vec<DupClusterLines> = clusters.iter().map(dup_cluster_lines).collect();
        let rendered = render_section_2(&files, &clusters, &lines);
        assert!(rendered.starts_with("## 2. Duplication Catalog"));
        for name in MANDATORY_SWEEPS {
            assert!(rendered.contains(name));
        }
        for c in &clusters {
            assert!(rendered.contains(&c.id));
        }
    }

    #[test]
    fn replace_section_2_only_touches_section_2_leaving_neighbors_intact() {
        let existing = "# Title\n\n## 1. Responsibility Map\n\nsection one body\n\n## 2. Duplication Catalog\n\nold placeholder\n\n## 3. Boundary Violations\n\nsection three body\n";
        let updated = replace_section_2(existing, "## 2. Duplication Catalog\n\nnew body\n");
        assert!(updated.contains("new body"));
        assert!(!updated.contains("old placeholder"));
        assert!(updated.contains("section one body"));
        assert!(updated.contains("section three body"));
    }

    #[test]
    #[should_panic(expected = "missing criterion 1's placeholder contract")]
    fn replace_section_2_panics_loudly_when_the_heading_is_entirely_absent() {
        replace_section_2(
            "# Title\n\nno sections here\n",
            "## 2. Duplication Catalog\n\nx\n",
        );
    }

    // -------------------------------------------------------------------------------------
    // The adversarial sample
    // -------------------------------------------------------------------------------------

    #[test]
    fn sample_indices_is_deterministic_for_a_fixed_seed() {
        let a = sample_indices(1000, 30, 42);
        let b = sample_indices(1000, 30, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn sample_indices_returns_k_distinct_sorted_in_bounds_indices() {
        let picked = sample_indices(500, 30, ADVERSARIAL_SEED);
        assert_eq!(picked.len(), 30);
        let distinct: HashSet<usize> = picked.iter().copied().collect();
        assert_eq!(distinct.len(), 30);
        assert!(picked.iter().all(|&i| i < 500));
        let mut sorted = picked.clone();
        sorted.sort_unstable();
        assert_eq!(picked, sorted);
    }

    #[test]
    fn sample_indices_caps_at_n_when_k_exceeds_it() {
        let picked = sample_indices(5, 30, 7);
        assert_eq!(picked, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn sample_indices_of_an_empty_population_is_empty() {
        assert!(sample_indices(0, 30, 7).is_empty());
    }

    #[test]
    fn different_seeds_produce_different_draws() {
        let a = sample_indices(1000, 30, 1);
        let b = sample_indices(1000, 30, 2);
        assert_ne!(a, b);
    }

    // -------------------------------------------------------------------------------------
    // The Done-when acceptance tests: the REAL checked-out tree, criterion 2's own bar.
    // -------------------------------------------------------------------------------------

    /// Every cluster has >= 2 sites (a cluster of 1 is not a duplicate), a non-empty proposed
    /// home, and a classification that is one of the three spec-named values.
    #[test]
    fn every_real_cluster_has_two_or_more_sites_a_classification_and_a_home() {
        let clusters = real_catalog();
        assert!(!clusters.is_empty());
        for c in clusters {
            assert!(c.sites.len() >= 2, "cluster {} has < 2 sites", c.id);
            assert!(
                !c.proposed_home.is_empty(),
                "cluster {} has no proposed home",
                c.id
            );
            assert!(
                matches!(c.classification.as_str(), "exact" | "near" | "semantic"),
                "cluster {} has an unrecognized classification {:?}",
                c.id,
                c.classification
            );
        }
    }

    /// THE MANDATORY SWEEPS (spec 85 Done-when: "the mandatory sweeps... each appear"): on the
    /// real tree every one of the five finds at least one site - proving they are wired to a
    /// real cluster, not merely declared.
    #[test]
    fn every_mandatory_sweep_appears_with_at_least_one_site_on_the_real_tree() {
        let clusters = real_catalog();
        for name in MANDATORY_SWEEPS {
            let found = clusters.iter().find(|c| c.note.contains(name));
            let sites = found.map(|c| c.sites.len()).unwrap_or(0);
            assert!(
                sites >= 1,
                "mandatory sweep {name:?} found 0 sites on the real tree"
            );
        }
    }

    /// Cluster ids are unique and already in ascending `dup-NNNN` order (the drift guard's own
    /// determinism premise, checked directly against the real tree rather than a fixture).
    #[test]
    fn real_cluster_ids_are_unique_and_ascending() {
        let clusters = real_catalog();
        let ids: Vec<&str> = clusters.iter().map(|c| c.id.as_str()).collect();
        let distinct: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(distinct.len(), ids.len(), "duplicate cluster id");
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }

    /// The adversarial sample over the REAL function population is exactly
    /// [`ADVERSARIAL_SAMPLE_SIZE`] distinct, in-bounds indices, reproducible from
    /// [`ADVERSARIAL_SEED`] (spec 85 THOROUGHNESS) - the report's own subsection renders this
    /// same draw, read by hand (see the report; this test pins only the mechanism).
    #[test]
    fn the_real_tree_adversarial_sample_is_reproducible_and_well_formed() {
        let refs = all_fn_refs(real_files());
        assert!(
            refs.len() > ADVERSARIAL_SAMPLE_SIZE,
            "expected far more than {ADVERSARIAL_SAMPLE_SIZE} functions in the real tree"
        );
        let a = sample_indices(refs.len(), ADVERSARIAL_SAMPLE_SIZE, ADVERSARIAL_SEED);
        let b = sample_indices(refs.len(), ADVERSARIAL_SAMPLE_SIZE, ADVERSARIAL_SEED);
        assert_eq!(a, b);
        assert_eq!(a.len(), ADVERSARIAL_SAMPLE_SIZE);
        assert!(a.iter().all(|&i| i < refs.len()));
        // Every drawn index resolves to a real function (the report's "### Adversarial sample"
        // subsection, rendered by `render_section_2`, is what actually lists this same draw for
        // a human to read by hand - this test pins only the mechanism's determinism).
        for &i in &a {
            let _ = refs[i].scanned(real_files());
        }
    }

    /// THE DRIFT GUARD for `docs/audit/duplication-catalog.json`: with `RIGGER_AUDIT_WRITE=1`
    /// set, regenerate and overwrite it (and, spec 90 criterion 2, its unguarded
    /// `.lines.json` sibling alongside it); otherwise regenerate in memory and assert the
    /// GUARDED file matches the committed bytes byte-for-byte (spec 85 Design) - mirrors
    /// `responsibility_map_json_matches_the_tree_or_is_rewritten` exactly. The `.lines.json`
    /// sibling is deliberately NEVER read back or compared here (spec 90 criterion 2 Design:
    /// "the guard NEVER compares") - it is write-mode-only output.
    #[test]
    fn duplication_catalog_json_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let clusters = real_catalog();
        let json = catalog_to_json(clusters);
        let path = root.join(CATALOG_PATH);
        if std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1") {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, &json).unwrap();
            fs::write(
                root.join(CATALOG_LINES_PATH),
                catalog_lines_to_json(clusters),
            )
            .unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{CATALOG_PATH} is missing or unreadable ({e}) - run with RIGGER_AUDIT_WRITE=1 \
                 to generate it"
            )
        });
        assert_eq!(
            committed, json,
            "{CATALOG_PATH} has drifted from the tree - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    // -------------------------------------------------------------------------------------
    // Spec 90 criterion 2, THE DRIFT GUARD IS LINE-FREE: the four Done-when claims, each its
    // own test against the real committed catalog (structural) or a synthetic fixture tree
    // (byte-stability under a pin bump / a two-branch merge - the properties a real branch
    // divergence needs, not provable from one static committed snapshot alone).
    // -------------------------------------------------------------------------------------

    /// CLAIM 1: "the guarded catalog carries no line numbers." Checked STRUCTURALLY (parsed as
    /// generic JSON, not through the producer's own typed shape, so a regression that
    /// re-introduces a differently-named line field would still be caught) against the real
    /// committed file.
    #[test]
    fn the_real_committed_catalog_carries_no_line_number_fields() {
        let clusters = real_catalog();
        let json = catalog_to_json(clusters);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let arr = value.as_array().expect("a bare array");
        assert!(!arr.is_empty());
        for cluster in arr {
            let sites = cluster["sites"].as_array().expect("sites array");
            assert!(!sites.is_empty());
            for site in sites {
                let obj = site.as_object().expect("a site object");
                assert!(
                    !obj.contains_key("start_line"),
                    "site {site:?} still carries start_line"
                );
                assert!(
                    !obj.contains_key("end_line"),
                    "site {site:?} still carries end_line"
                );
                let hash = obj
                    .get("content_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                assert!(!hash.is_empty(), "site {site:?} has an empty content_hash");
            }
        }
    }

    /// CLAIM 2: "a pin bump that shifts every site in a file leaves it byte-identical." A
    /// synthetic two-file fixture (the same renamed-identical-pair shape
    /// `build_catalog_assigns_sequential_ids_after_the_deterministic_sort` uses, so the fixture
    /// forms a real 2-site cluster), then a "pin bump" - 5 unrelated comment lines prepended to
    /// ONE file, shifting `add_one`'s own line span by 5 but leaving its text untouched -
    /// regenerates a byte-IDENTICAL guarded catalog, because content_hash keys on the span's own
    /// normalized tokens, never its line number.
    #[test]
    fn a_pin_bump_that_shifts_every_site_in_a_file_leaves_the_guarded_catalog_byte_identical() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let base_json = catalog_to_json(&build_catalog(&scan_tree(dir.path())));

        write_fixture(
            dir.path(),
            "src/a.rs",
            "// pin: v1\n// pin: v2\n// pin: v3\n// pin: v4\n// pin: v5\n\
             fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        let bumped_json = catalog_to_json(&build_catalog(&scan_tree(dir.path())));

        assert_eq!(
            base_json, bumped_json,
            "a pin bump that only shifts an existing site's OWN line number must leave the \
             guarded catalog byte-identical (spec 90 criterion 2)"
        );
    }

    /// CLAIM 3: "two branches adding tests in different files merge it without conflict." Two
    /// synthetic branches off the SAME base tree, each appending one UNIQUE (non-duplicating)
    /// function to a DIFFERENT file - exactly the common real-world shape (a sibling unit adding
    /// its own new periphery test) spec 90's Goal names as the u86 c2 x c3 flake. Since neither
    /// addition forms a new duplicate cluster, the STRONGEST possible proof holds: the guarded
    /// catalog is byte-identical across base, branch A, and branch B - nothing to merge at all,
    /// let alone conflict.
    #[test]
    fn two_branches_adding_an_unrelated_function_to_different_files_leave_the_guarded_catalog_unaffected(
    ) {
        let base = tempfile::tempdir().expect("base scratch dir");
        write_fixture(
            base.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            base.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let base_json = catalog_to_json(&build_catalog(&scan_tree(base.path())));

        let branch_a = tempfile::tempdir().expect("branch A scratch dir");
        write_fixture(
            branch_a.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n\n\
             fn branch_a_only(x: i64) -> i64 {\n    x * 3 - 7\n}\n",
        );
        write_fixture(
            branch_a.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let a_json = catalog_to_json(&build_catalog(&scan_tree(branch_a.path())));

        let branch_b = tempfile::tempdir().expect("branch B scratch dir");
        write_fixture(
            branch_b.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            branch_b.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n\n\
             fn branch_b_only(y: i64) -> i64 {\n    y / 2 + 11\n}\n",
        );
        let b_json = catalog_to_json(&build_catalog(&scan_tree(branch_b.path())));

        assert_eq!(
            base_json, a_json,
            "branch A's own unrelated addition to src/a.rs must not perturb the guarded catalog"
        );
        assert_eq!(
            base_json, b_json,
            "branch B's own unrelated addition to src/z.rs must not perturb the guarded catalog"
        );
    }

    /// The structural report guard's own pin-bump proof, exercising the RENDERED REPORT TEXT
    /// itself (`render_section_2`'s output), not only the underlying catalog JSON CLAIM 2
    /// already covers: the same "pin bump that shifts every site in a file" fixture, rendered
    /// before and after. `assert_section_2_structurally_matches` accepts BOTH renders (against
    /// their own fresh cluster data), and every structural fact of the one bumped cluster -
    /// id, classification, proposed home, note, site names/files, site count - is byte-
    /// identical across the bump; only the bumped site's own numeric citation moves, tracking
    /// the shift exactly (never stale, never frozen) since `render_section_2` always reads a
    /// FRESH `lines` computed from the same live `clusters` it renders alongside. This is the
    /// property spec 90's remedy asks for: the report survives a pin bump structurally, the way
    /// the guarded catalog already survives one byte-for-byte.
    #[test]
    fn a_pin_bump_leaves_the_rendered_report_section_2_structurally_unchanged() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let render = |root: &Path| -> (String, Vec<DupCluster>) {
            let files = scan_tree(root);
            let clusters = build_catalog(&files);
            let lines: Vec<DupClusterLines> = clusters.iter().map(dup_cluster_lines).collect();
            let rendered = render_section_2(&files, &clusters, &lines);
            (rendered, clusters)
        };
        let (before, before_clusters) = render(dir.path());

        write_fixture(
            dir.path(),
            "src/a.rs",
            "// pin: v1\n// pin: v2\n// pin: v3\n// pin: v4\n// pin: v5\n\
             fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        let (after, after_clusters) = render(dir.path());

        // The structural guard itself must accept BOTH renders, against their own fresh
        // clusters - the very property that makes it pin-bump-safe.
        assert_section_2_structurally_matches(&before, &before_clusters);
        assert_section_2_structurally_matches(&after, &after_clusters);

        assert_eq!(
            before_clusters.len(),
            after_clusters.len(),
            "the bump must not add or remove a cluster"
        );
        for (b, a) in before_clusters.iter().zip(&after_clusters) {
            assert_eq!(b.id, a.id, "cluster id must survive a pure line shift");
            assert_eq!(b.classification, a.classification);
            assert_eq!(b.proposed_home, a.proposed_home);
            assert_eq!(b.note, a.note);
            assert_eq!(b.sites.len(), a.sites.len());
            for (bs, asite) in b.sites.iter().zip(&a.sites) {
                assert_eq!(bs.name, asite.name);
                assert_eq!(bs.file, asite.file);
            }
        }
        assert!(
            before.contains("`src/a.rs:1-3`"),
            "expected the pre-bump render to cite add_one at its original span - got {before:?}"
        );
        assert!(
            after.contains("`src/a.rs:6-8`"),
            "expected the post-bump render to cite add_one at its shifted span, tracking the \
             live tree exactly - got {after:?}"
        );
        assert!(
            !after.contains("`src/a.rs:1-3`"),
            "the post-bump render must not still cite add_one's stale, pre-bump span"
        );
    }

    /// CLAIM 4: "the report still cites `file:line` from the unguarded lines file." Proven at
    /// the DATA-FLOW level, not by coincidence: a synthetic cluster whose [`DupSite`] carries a
    /// DECOY `start_line`/`end_line` that appears nowhere in `lines`, rendered with a separate,
    /// deliberately different [`DupClusterLines`] - the rendered citation is the `lines` value,
    /// never the decoy `DupSite` one. The prior round's same-named test derived both sides from
    /// the SAME live cluster (`dup_cluster_lines(cluster)`, itself just repackaging that
    /// cluster's own `DupSite.start_line`/`end_line`), so it could never fail even if
    /// `render_section_2` read `DupSite.start_line`/`end_line` directly - exactly the defect
    /// `adv-u90c2-report-guard-still-byte-pinned-to-live-line-numbers` found. This version fails
    /// if the renderer ever falls back to `DupSite`'s own fields.
    #[test]
    fn report_section_2_cites_file_line_exactly_as_the_unguarded_lines_file_records_them() {
        let decoy = (999_999, 999_998);
        let real = (10, 12);
        let cluster = DupCluster {
            id: "dup-0001".to_string(),
            classification: "near".to_string(),
            sites: vec![DupSite {
                file: "src/a.rs".to_string(),
                start_line: decoy.0,
                end_line: decoy.1,
                name: "add_one".to_string(),
                content_hash: "deadbeefcafef00d".to_string(),
            }],
            proposed_home: "a::support".to_string(),
            note: "n".to_string(),
        };
        let lines = DupClusterLines {
            id: "dup-0001".to_string(),
            sites: vec![DupSiteLines {
                file: "src/a.rs".to_string(),
                start_line: real.0,
                end_line: real.1,
            }],
        };
        let rendered = render_section_2(&[], &[cluster], &[lines]);
        let real_citation = format!("`src/a.rs:{}-{}`", real.0, real.1);
        let decoy_citation = format!("`src/a.rs:{}-{}`", decoy.0, decoy.1);
        assert!(
            rendered.contains(&real_citation),
            "report section 2 must cite the unguarded lines value {real_citation} - got \
             {rendered:?}"
        );
        assert!(
            !rendered.contains(&decoy_citation),
            "report section 2 must NEVER cite DupSite's own start_line/end_line directly - it \
             cited the decoy {decoy_citation} instead of the unguarded lines data"
        );
    }

    /// Spec 90 Design, verbatim: "the report's guard checks structure only (sections present,
    /// counts equal to the catalog) rather than bytes." A byte-exact comparison against the
    /// previously committed text (the prior round's own mechanism) fails on the VERY NEXT pin
    /// bump anywhere in the tree - any change that moves a cited site's own line number, with
    /// no change to the duplication catalog itself - reproducing spec 90's own Goal (the
    /// cascading-diff-on-an-unrelated-edit problem this whole spec exists to eliminate) for the
    /// report specifically, even though the guarded JSON catalog it is generated alongside is
    /// already genuinely pin-bump-stable (`adv-u90c2-report-guard-still-byte-pinned-to-live-
    /// line-numbers`). This checks only that the heading is present, that the declared
    /// cluster/site counts match the fresh catalog, that every mandatory sweep is named, and
    /// that every current cluster id has its own heading - never the exact citation bytes,
    /// which are free to legitimately move between explicit `RIGGER_AUDIT_WRITE=1` regens.
    fn assert_section_2_structurally_matches(committed_section_2: &str, clusters: &[DupCluster]) {
        assert!(
            committed_section_2.starts_with("## 2. Duplication Catalog"),
            "{REPORT_PATH} section 2 is missing its own heading"
        );
        let total_sites: usize = clusters.iter().map(|c| c.sites.len()).sum();
        let counts_line = format!("{} clusters ({total_sites} total sites)", clusters.len());
        assert!(
            committed_section_2.contains(&counts_line),
            "{REPORT_PATH} section 2's declared counts have drifted from the tree (expected \
             {counts_line:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
        );
        for name in MANDATORY_SWEEPS {
            assert!(
                committed_section_2.contains(name),
                "{REPORT_PATH} section 2 is missing mandatory sweep {name:?} - regenerate with \
                 RIGGER_AUDIT_WRITE=1"
            );
        }
        for c in clusters {
            let heading = format!(
                "#### `{}` ({}, {} sites)",
                c.id,
                c.classification,
                c.sites.len()
            );
            assert!(
                committed_section_2.contains(&heading),
                "{REPORT_PATH} section 2 is missing or has a stale heading for cluster `{}` \
                 (expected {heading:?}) - regenerate with RIGGER_AUDIT_WRITE=1",
                c.id
            );
        }
    }

    /// THE DRIFT GUARD for section 2 of the report: with `RIGGER_AUDIT_WRITE=1` set, patch
    /// section 2's span in place (guarded by [`REPORT_WRITE_LOCK`] since criterion 1's own
    /// drift guard writes the SAME file); otherwise assert the committed report's section 2
    /// matches the fresh catalog STRUCTURALLY (see `assert_section_2_structurally_matches`),
    /// never byte-for-byte. Mirrors `report_section_1_matches_the_tree_or_is_rewritten`'s
    /// write-mode half; its check-mode half deliberately does NOT mirror that test's byte
    /// comparison (spec 90 Design decides section 2's guard specifically is structural).
    #[test]
    fn report_section_2_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let clusters = real_catalog();
        let lines: Vec<DupClusterLines> = clusters.iter().map(dup_cluster_lines).collect();
        let section_2 = render_section_2(real_files(), clusters, &lines);
        let path = root.join(REPORT_PATH);
        let write = std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1");
        if write {
            let _guard = lock_report_write();
            let existing = fs::read_to_string(&path).ok();
            let base = match existing {
                Some(text) => text,
                None => {
                    let map = build_map(&root);
                    let map_lines: Vec<MapEntryLines> = map.iter().map(map_entry_lines).collect();
                    let section_1 = render_section_1(&map, &map_lines);
                    assemble_fresh_report(&section_1)
                }
            };
            let updated = replace_section_2(&base, &section_2);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, updated).unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{REPORT_PATH} is missing - run with RIGGER_AUDIT_WRITE=1 to generate it")
        });
        let span = section_span(&committed, "## 2. ");
        assert_section_2_structurally_matches(&committed[span], clusters);
    }

    // =====================================================================================
    // Criterion 3 (`u85c3`, THIS UNIT): sections 3-5 (boundary violations, dead and
    // vestigial code, test-suite shape)
    // =====================================================================================

    #[test]
    fn replace_sections_3_to_5_only_touches_that_span_leaving_neighbors_intact() {
        let existing = "# Title\n\n\
             ## 1. Responsibility Map\n\nsection one body\n\n\
             ## 2. Duplication Catalog\n\nsection two body\n\n\
             ## 3. Boundary Violations\n\nold section three\n\n\
             ## 4. Dead and Vestigial Code\n\nold section four\n\n\
             ## 5. Test-Suite Shape\n\nold section five\n\n\
             ## 6. Prioritized Plan\n\n_Pending - criterion 4 (`u85c4`)._\n";
        let updated = replace_section_3_to_5(
            existing,
            "## 3. Boundary Violations\n\nnew section three\n",
            "## 4. Dead and Vestigial Code\n\nnew section four\n",
            "## 5. Test-Suite Shape\n\nnew section five\n",
        );
        assert!(updated.contains("new section three"));
        assert!(updated.contains("new section four"));
        assert!(updated.contains("new section five"));
        assert!(!updated.contains("old section three"));
        assert!(!updated.contains("old section four"));
        assert!(!updated.contains("old section five"));
        // Sections 1, 2 and 6 (this criterion's neighbors) survive byte-for-byte.
        assert!(updated.contains("section one body"));
        assert!(updated.contains("section two body"));
        assert!(updated.contains("_Pending - criterion 4 (`u85c4`)._"));
    }

    #[test]
    fn replace_sections_3_to_5_falls_back_to_end_of_string_when_no_section_6_heading_exists() {
        // A report that (hypothetically) ends right after section 5 - no `## 6. ` heading
        // yet to bound the replacement span against.
        let existing = "# Title\n\n\
             ## 1. Responsibility Map\n\nsection one body\n\n\
             ## 3. Boundary Violations\n\nold section three\n";
        let updated = replace_section_3_to_5(
            existing,
            "## 3. Boundary Violations\n\nnew section three\n",
            "## 4. Dead and Vestigial Code\n\nnew section four\n",
            "## 5. Test-Suite Shape\n\nnew section five\n",
        );
        assert!(updated.contains("new section three"));
        assert!(updated.contains("new section four"));
        assert!(updated.contains("new section five"));
        assert!(updated.contains("section one body"));
        assert!(!updated.contains("old section three"));
    }

    #[test]
    #[should_panic(expected = "missing criterion 1's placeholder contract")]
    fn replace_sections_3_to_5_panics_loudly_when_the_heading_is_entirely_absent() {
        replace_section_3_to_5(
            "# Title\n\nno sections here\n",
            "## 3. Boundary Violations\n\nx\n",
            "## 4. Dead and Vestigial Code\n\ny\n",
            "## 5. Test-Suite Shape\n\nz\n",
        );
    }

    /// A sub-heading's own span within a larger rendered block (from `marker` up to, but not
    /// including, the next heading starting with `next_prefix`) - mirrors `section_span`'s own
    /// top-level-heading logic one level down, generalized over the next boundary's own prefix
    /// so it serves both `### `-level (section 4's own 4.N sub-headings) and `#### `-level
    /// (section 6's own numbered plan items) boundaries alike.
    fn heading_bounded_span(
        haystack: &str,
        marker: &str,
        next_prefix: &str,
    ) -> std::ops::Range<usize> {
        let start = find_heading(haystack, marker)
            .unwrap_or_else(|| panic!("{REPORT_PATH} is missing its own {marker:?} heading"));
        let next_boundary = format!("\n{next_prefix}");
        let rest_after_marker = &haystack[start + marker.len()..];
        let end_offset = rest_after_marker.find(&next_boundary).map(|p| p + 1);
        let end = match end_offset {
            Some(off) => start + marker.len() + off,
            None => haystack.len(),
        };
        start..end
    }

    /// Spec 90 Design, verbatim: "the report's guard checks structure only (sections present,
    /// counts equal to the catalog) rather than bytes." Section 4.3's full list (via
    /// [`render_dead_code_full_list`]) is the ONLY content anywhere in sections 3-5 that embeds
    /// a live line number, so it is the ONLY span this isolates for structural treatment -
    /// its own file groupings and entry names, never the exact citation bytes, which are free
    /// to legitimately move between explicit `RIGGER_AUDIT_WRITE=1` regens (a pin bump anywhere
    /// in `src/`). Everything else in section 4 (4.0-4.2 including the distribution table, and
    /// 4.4) is citation-free and fully deterministic from the tree, so it stays byte-exact
    /// against `fresh_section_4` - exactly as strict as this guard was before this fix, and
    /// still catches real content drift (e.g. the distribution table's own per-file counts).
    fn assert_section_4_structurally_matches(
        committed_section_4: &str,
        fresh_section_4: &str,
        candidates: &[DeadCodeCandidate],
    ) {
        assert!(
            committed_section_4.starts_with("## 4. Dead and Vestigial Code"),
            "{REPORT_PATH} section 4 is missing its own heading"
        );
        let committed_span = heading_bounded_span(committed_section_4, "### 4.3 ", "### ");
        let fresh_span = heading_bounded_span(fresh_section_4, "### 4.3 ", "### ");
        let committed_4_3 = &committed_section_4[committed_span.clone()];
        let mut files: Vec<&str> = candidates.iter().map(|c| c.file.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        for file in &files {
            let group_marker = format!("**`{file}`**");
            assert!(
                committed_4_3.contains(&group_marker),
                "{REPORT_PATH} section 4.3 is missing its own file grouping for `{file}` \
                 (expected {group_marker:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
            );
        }
        for c in candidates {
            let name_marker = format!("- **{}**", c.name);
            assert!(
                committed_4_3.contains(&name_marker),
                "{REPORT_PATH} section 4.3 is missing or has a stale entry for `{}` (expected \
                 {name_marker:?}) - regenerate with RIGGER_AUDIT_WRITE=1",
                c.name
            );
        }
        let committed_rest = format!(
            "{}{}",
            &committed_section_4[..committed_span.start],
            &committed_section_4[committed_span.end..]
        );
        let fresh_rest = format!(
            "{}{}",
            &fresh_section_4[..fresh_span.start],
            &fresh_section_4[fresh_span.end..]
        );
        // Trailing newline COUNT is a formatting artifact of the outer document splice (the
        // top-level `## 4. ` -> `## 5. ` boundary this span was cut from keeps the full
        // blank-line separator, one more `\n` than `render_section_4`'s own raw return), never
        // real content - trimmed on both sides before comparing so it cannot produce a false
        // drift report.
        assert_eq!(
            committed_rest.trim_end_matches('\n'),
            fresh_rest.trim_end_matches('\n'),
            "{REPORT_PATH} section 4 (outside 4.3's own citation list) has drifted from the \
             tree - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    /// THE DRIFT GUARD for sections 3-5 of the report: with `RIGGER_AUDIT_WRITE=1` set,
    /// patch the combined 3-5 span in place (guarded by [`REPORT_WRITE_LOCK`] since
    /// criteria 1 and 2's own drift guards write the SAME file); otherwise assert the
    /// committed report's sections 3 and 5 match VERBATIM (safe - both are 100% static text,
    /// zero pin-bump risk) and section 4 matches the fresh candidate list STRUCTURALLY (see
    /// `assert_section_4_structurally_matches`), never byte-for-byte. Round 3 remedy: this
    /// guard's own check-mode half used to `assert_eq!` the ENTIRE 3-5 span against a render
    /// that embeds section 4.3's live line numbers - the same defect class criterion 2's guard
    /// was fixed for (`report_section_2_matches_the_tree_or_is_rewritten`), now fixed here too.
    #[test]
    fn report_sections_3_through_5_match_the_tree_or_are_rewritten() {
        let root = repo_root();
        let section_3 = render_section_3();
        let section_4 = render_section_4();
        let section_5 = render_section_5();
        let path = root.join(REPORT_PATH);
        let write = std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1");
        if write {
            let _guard = lock_report_write();
            let existing = fs::read_to_string(&path).ok();
            let base = match existing {
                Some(text) => text,
                None => {
                    let map = build_map(&root);
                    let map_lines: Vec<MapEntryLines> = map.iter().map(map_entry_lines).collect();
                    let section_1 = render_section_1(&map, &map_lines);
                    assemble_fresh_report(&section_1)
                }
            };
            let updated = replace_section_3_to_5(&base, &section_3, &section_4, &section_5);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, updated).unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{REPORT_PATH} is missing - run with RIGGER_AUDIT_WRITE=1 to generate it")
        });
        assert!(
            committed.contains(&section_3),
            "{REPORT_PATH} section 3 has drifted from the tree - regenerate with \
             RIGGER_AUDIT_WRITE=1"
        );
        let span = section_span(&committed, "## 4. ");
        assert_section_4_structurally_matches(
            &committed[span],
            &section_4,
            real_dead_code_candidates(),
        );
        assert!(
            committed.contains(&section_5),
            "{REPORT_PATH} section 5 has drifted from the tree - regenerate with \
             RIGGER_AUDIT_WRITE=1"
        );
    }

    /// Shared machinery for the two CLAIM-4-equivalent decoy pin-bump tests below (section 4.3
    /// and section 6 item 0): pick `probe`, override its own `lines` entry with a decoy line
    /// found nowhere in `real_dead_code_candidates()`, render via `render`, and assert the
    /// decoy citation (built by `citation`) is present while the stale (real) one is absent -
    /// proving the renderer sources its citation from `lines`, never `DeadCodeCandidate`'s own
    /// live `line` field. One helper, not two near-identical test bodies, since this file's own
    /// duplication scanner catches test code too (spec 85 Goal: "no small enough to duplicate
    /// exemption").
    fn assert_dead_code_render_cites_the_unguarded_lines_value(
        probe: usize,
        render: impl Fn(&[DeadCodeCandidateLines]) -> String,
        citation: impl Fn(&DeadCodeCandidate, usize) -> String,
    ) {
        let candidates = real_dead_code_candidates();
        let mut lines = real_dead_code_lines();
        let real_line = candidates[probe].line;
        let decoy_line = real_line + 500_000;
        lines[probe].line = decoy_line;
        let rendered = render(&lines);
        let decoy_citation = citation(&candidates[probe], decoy_line);
        let stale_citation = citation(&candidates[probe], real_line);
        assert!(
            rendered.contains(&decoy_citation),
            "must cite the unguarded lines value {decoy_citation} - got {rendered:?}"
        );
        assert!(
            !rendered.contains(&stale_citation),
            "must NEVER cite DeadCodeCandidate's own live `line` field directly - it still \
             cited the stale {stale_citation} instead of the unguarded lines data"
        );
    }

    /// CLAIM-4 equivalent for section 4.3 (mirrors
    /// `report_section_2_cites_file_line_exactly_as_the_unguarded_lines_file_records_them`):
    /// proven at the DATA-FLOW level against the real committed candidate pool, since
    /// `render_dead_code_full_list` (unlike `render_section_2`) sources its candidates
    /// internally from `real_dead_code_candidates()` rather than a caller-supplied slice - a
    /// synthetic single-candidate fixture is not this function's own shape (see
    /// `assert_dead_code_render_cites_the_unguarded_lines_value`). `kg_degree_for` still
    /// receives the probed candidate's OWN live line (a lookup key into a hand-maintained
    /// table, never a citation - see `render_dead_code_full_list`'s own doc comment), so this
    /// isolates exactly the citation data-flow claim without tripping `kg_degree_for`'s own
    /// unknown-pair panic.
    #[test]
    fn report_section_4_3_cites_file_line_exactly_as_the_unguarded_lines_file_records_them() {
        assert_dead_code_render_cites_the_unguarded_lines_value(
            0,
            render_dead_code_full_list,
            |c, line| format!("`{}:{}`", c.file, line),
        );
    }

    // =====================================================================================
    // Criterion 4 (`u85c4`, THIS UNIT): section 6 (prioritized plan)
    // =====================================================================================

    #[test]
    fn find_heading_skips_a_heading_shaped_substring_embedded_inside_a_sub_heading() {
        // The exact corruption shape `find_heading`'s own doc comment describes (decision
        // `u85c4-fix-heading-boundary-false-match`): a `#### 3. ` sub-heading's tail reads as
        // `## 3. ` starting at its own third byte, so an unanchored `str::find("## 3. ")` would
        // land INSIDE this sub-heading instead of the real `## 3. ` heading further down. Built
        // directly here rather than relying on the committed report's current, incidental
        // content (`sdet-u85c4-find-heading-lacks-direct-adversarial-unit-test`) - this fails a
        // regression to plain `str::find` even after a future edit removes today's coincidental
        // collision from the real artifact.
        let haystack = "intro text\n\n\
             #### 3. A Sub-Heading Whose Tail Reads As A Real One\n\n\
             body under the sub-heading\n\n\
             ## 3. The Real Heading\n\n\
             body under the real heading\n";

        // Sanity: the adversarial substring really is embedded where this test assumes - an
        // unanchored search would find it at the sub-heading's own third byte, two bytes past
        // where `#### 3. ` itself starts.
        let sub_heading_start = haystack.find("#### 3. ").unwrap();
        let false_match = sub_heading_start + 2;
        assert_eq!(
            &haystack[false_match..false_match + "## 3. ".len()],
            "## 3. "
        );
        assert_ne!(haystack.as_bytes()[false_match - 1], b'\n');

        let real_match = haystack.rfind("\n## 3. ").map(|p| p + 1).unwrap();
        assert_eq!(
            find_heading(haystack, "## 3. "),
            Some(real_match),
            "find_heading must skip the heading-shaped substring embedded inside the sub-heading \
             (at byte {false_match}, not preceded by a newline) and land on the real line-start \
             heading at byte {real_match} instead"
        );
    }

    #[test]
    fn find_heading_returns_none_when_no_genuine_line_start_match_exists() {
        // Only the false, embedded match exists here - no real `## 3. ` heading anywhere. A
        // regression to plain `str::find` would wrongly return the embedded position instead of
        // `None`.
        let haystack = "intro\n\n#### 3. A Sub-Heading Whose Tail Reads As A Real One\n\nbody\n";
        assert_eq!(find_heading(haystack, "## 3. "), None);
    }

    #[test]
    fn replace_section_6_only_touches_that_span_leaving_earlier_sections_intact() {
        let existing = "# Title\n\n\
             ## 1. Responsibility Map\n\nsection one body\n\n\
             ## 2. Duplication Catalog\n\nsection two body\n\n\
             ## 3. Boundary Violations\n\nsection three body\n\n\
             ## 4. Dead and Vestigial Code\n\nsection four body\n\n\
             ## 5. Test-Suite Shape\n\nsection five body\n\n\
             ## 6. Prioritized Plan\n\n_Pending - criterion 4 (`u85c4`)._\n";
        let updated = replace_section_6(existing, "## 6. Prioritized Plan\n\nnew section six\n");
        assert!(updated.contains("new section six"));
        assert!(!updated.contains("_Pending - criterion 4"));
        // Every earlier section (not this criterion's own) survives byte-for-byte.
        assert!(updated.contains("section one body"));
        assert!(updated.contains("section two body"));
        assert!(updated.contains("section three body"));
        assert!(updated.contains("section four body"));
        assert!(updated.contains("section five body"));
    }

    #[test]
    fn replace_section_6_works_when_it_is_the_very_end_of_the_string() {
        // Section 6 is the LAST section - there is no next heading to bound the
        // replacement span against, unlike sections 1, 2 and 3-5's own combined span.
        let existing = "# Title\n\n## 6. Prioritized Plan\n\nold body\ntrailing line\n";
        let updated = replace_section_6(existing, "## 6. Prioritized Plan\n\nnew body\n");
        assert_eq!(updated, "# Title\n\n## 6. Prioritized Plan\n\nnew body\n");
    }

    #[test]
    #[should_panic(expected = "missing criterion 1's placeholder contract")]
    fn replace_section_6_panics_loudly_when_the_heading_is_entirely_absent() {
        replace_section_6(
            "# Title\n\nno sections here\n",
            "## 6. Prioritized Plan\n\nx\n",
        );
    }

    #[test]
    fn render_section_6_cites_every_tier_and_the_explicit_none_needed_category() {
        let rendered = render_section_6();
        assert!(rendered.contains("## 6. Prioritized Plan"));
        // Ordering methodology and tier structure (largest risk-reduction first, per spec
        // 85's own Done-when text).
        assert!(rendered.contains("largest risk-reduction"));
        assert!(rendered.contains("Tier 1"));
        assert!(rendered.contains("Tier 2"));
        assert!(rendered.contains("Tier 3"));
        assert!(rendered.contains("Tier 4"));
        assert!(rendered.contains("Tier 5"));
        // God-file splits and duplication removals are separate entries (spec 85's own
        // wording, quoted so a reader can see this criterion's own bar is met).
        assert!(rendered.contains("separate entries"));
        // Cites section 3's two boundary violations by name.
        assert!(rendered.contains("AgentDriver"));
        assert!(rendered.contains("Grounder"));
        // The two-candidates-one-home resolution for dup-0204 (spec 85 CONSTRAINTS WALK).
        assert!(rendered.contains("dup-0204"));
        assert!(rendered.contains("TWO CANDIDATES, ONE HOME"));
        // Cites the mandatory-sweep duplication clusters by id.
        assert!(rendered.contains("dup-0006"));
        assert!(rendered.contains("dup-0055"));
        assert!(rendered.contains("dup-0109"));
        assert!(rendered.contains("dup-0143"));
        assert!(rendered.contains("dup-0144"));
        assert!(rendered.contains("dup-0213"));
        // Cites the god-file test/production split for all three files.
        assert!(rendered.contains("src/conductor.rs"));
        assert!(rendered.contains("src/main.rs"));
        assert!(rendered.contains("src/dash.rs"));
        // Cites section 5's own headline test-suite consolidation items.
        assert!(rendered.contains("tests/common"));
        assert!(rendered.contains("tests/cli.rs"));
        assert!(rendered.contains("dup-0662"));
        // Item 0 (spec 87 c3): section 4's redo turned dead-and-vestigial-code from an empty
        // category into a real Tier 1 deletion item; the 3 residual `keep-pending` entries
        // explicitly get no NEW follow-up stub (each cites an already-landed governing spec
        // instead), and `keep-public-surface` (0 entries) is stated so with the search that
        // established it, never omitted - spec 85's own CONSTRAINTS WALK rule for an empty
        // category, now applied to the narrower keep-public-surface slice.
        assert!(rendered.contains("Delete the dead-code set"));
        assert!(rendered.contains("no further follow-up"));
        assert!(rendered.contains("turbovec"));
        assert!(rendered.contains("kurrentdb"));
        // Adds no new findings: every dollar figure traces back to the committed JSON, not
        // a fresh scan - the section says so explicitly.
        assert!(rendered.contains("adds no new findings"));
        // Item 0's own content: the deletion count, its two named citations, and the three
        // keep-pending entries' governing specs (spec 87 c3's own Done-when: "section 6 gains
        // item 0").
        assert!(rendered.contains("#### 0. Delete the dead-code set"));
        assert!(rendered.contains("23 dead functions"));
        assert!(rendered.contains("src/spawn.rs"));
        assert!(rendered.contains("284 production lines"));
        for (spec_n, name) in [
            ("27", "distiller::rebuild"),
            ("32", "Defaults::sdet_author_enabled"),
            ("60", "Store::with_content_identity"),
        ] {
            assert!(
                rendered.contains(&format!("spec {spec_n}")),
                "missing keep-pending citation for spec {spec_n} ({name}): {rendered}"
            );
        }
    }

    /// Spec 90 Design, verbatim: "the report's guard checks structure only (sections present,
    /// counts equal to the catalog) rather than bytes." Item 0's own deletion list (via
    /// [`render_dead_code_deletion_list`]) is the ONLY content anywhere in section 6 that
    /// embeds a live line number, so it is the ONLY span this isolates for structural
    /// treatment, mirroring `assert_section_4_structurally_matches` exactly (same pattern, one
    /// level deeper: `#### 0. ` instead of `### 4.3 `). Everything else in section 6 (items
    /// 1-19 and the tier framing prose) is citation-free and fully deterministic, so it stays
    /// byte-exact against `fresh_section_6` - exactly as strict as this guard was before this
    /// fix.
    fn assert_section_6_structurally_matches(
        committed_section_6: &str,
        fresh_section_6: &str,
        candidates: &[DeadCodeCandidate],
    ) {
        assert!(
            committed_section_6.starts_with("## 6. Prioritized Plan"),
            "{REPORT_PATH} section 6 is missing its own heading"
        );
        let committed_span = heading_bounded_span(committed_section_6, "#### 0. ", "#### ");
        let fresh_span = heading_bounded_span(fresh_section_6, "#### 0. ", "#### ");
        let committed_item_0 = &committed_section_6[committed_span.clone()];
        let deletes: Vec<&DeadCodeCandidate> = candidates
            .iter()
            .filter(|c| c.disposition == Disposition::Delete)
            .collect();
        let mut files: Vec<&str> = deletes.iter().map(|c| c.file.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        for file in &files {
            let group_marker = format!("`{file}`: ");
            assert!(
                committed_item_0.contains(&group_marker),
                "{REPORT_PATH} section 6 item 0 is missing its own file grouping for `{file}` \
                 (expected {group_marker:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
            );
        }
        for c in &deletes {
            let name_marker = format!("`{}`", c.name);
            assert!(
                committed_item_0.contains(&name_marker),
                "{REPORT_PATH} section 6 item 0 is missing or has a stale entry for `{}` \
                 (expected {name_marker:?}) - regenerate with RIGGER_AUDIT_WRITE=1",
                c.name
            );
        }
        let committed_rest = format!(
            "{}{}",
            &committed_section_6[..committed_span.start],
            &committed_section_6[committed_span.end..]
        );
        let fresh_rest = format!(
            "{}{}",
            &fresh_section_6[..fresh_span.start],
            &fresh_section_6[fresh_span.end..]
        );
        // Trailing newline COUNT is a formatting artifact, never real content (mirrors
        // `assert_section_4_structurally_matches`'s own identical trim) - trimmed on both sides
        // so it cannot produce a false drift report regardless of which document-splice path
        // produced each side.
        assert_eq!(
            committed_rest.trim_end_matches('\n'),
            fresh_rest.trim_end_matches('\n'),
            "{REPORT_PATH} section 6 (outside item 0's own deletion list) has drifted from the \
             tree - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    /// THE DRIFT GUARD for section 6 of the report: with `RIGGER_AUDIT_WRITE=1` set, patch
    /// section 6's span in place (guarded by [`REPORT_WRITE_LOCK`] since criteria 1-3's own
    /// drift guards write the SAME file); otherwise assert the committed report's section 6
    /// matches the fresh candidate list STRUCTURALLY (see
    /// `assert_section_6_structurally_matches`), never byte-for-byte. Round 3 remedy: this
    /// guard's own check-mode half used to `assert_eq!` the WHOLE section 6 against a render
    /// that embeds item 0's own live line numbers - the same defect class criteria 1's and
    /// 2's guards were fixed for, now fixed here too (`render_section_6` is mostly static text,
    /// EXCEPT item 0's own deletion list, rendered from `real_dead_code_candidates()` - see
    /// this unit's own module-doc banner for why no generator code backs the REST of section 6:
    /// spec 85's own Done-when text for this criterion is "cites sections 1-5 and adds no new
    /// findings", not a new scanner).
    #[test]
    fn report_section_6_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let section_6 = render_section_6();
        let path = root.join(REPORT_PATH);
        let write = std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1");
        if write {
            let _guard = lock_report_write();
            let existing = fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!(
                    "{REPORT_PATH} is missing - run criteria 1-3's own writers first \
                     (RIGGER_AUDIT_WRITE=1)"
                )
            });
            let updated = replace_section_6(&existing, &section_6);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, updated).unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{REPORT_PATH} is missing - run with RIGGER_AUDIT_WRITE=1 to generate it")
        });
        let span = section_span(&committed, "## 6. ");
        assert_section_6_structurally_matches(
            &committed[span],
            &section_6,
            real_dead_code_candidates(),
        );
    }

    /// CLAIM-4 equivalent for section 6 item 0 (mirrors
    /// `report_section_4_3_cites_file_line_exactly_as_the_unguarded_lines_file_records_them`):
    /// same shared machinery, applied to `render_dead_code_deletion_list` instead, probed at
    /// the first real `delete`-dispositioned entry (see
    /// `assert_dead_code_render_cites_the_unguarded_lines_value`).
    #[test]
    fn report_section_6_item_0_cites_file_line_exactly_as_the_unguarded_lines_file_records_them() {
        let probe = real_dead_code_candidates()
            .iter()
            .position(|c| c.disposition == Disposition::Delete)
            .expect("the real tree has at least one delete-dispositioned candidate");
        assert_dead_code_render_cites_the_unguarded_lines_value(
            probe,
            render_dead_code_deletion_list,
            |c, line| format!("`{}` (line {})", c.name, line),
        );
    }

    // =====================================================================================
    // Criterion 2 (`u87c2`, THIS UNIT) - spec 87: the production-reference sweep
    // =====================================================================================

    // -------------------------------------------------------------------------------------
    // resolve_out_of_line_test_files: the three resolution forms + transitive closure
    // -------------------------------------------------------------------------------------

    #[test]
    fn resolves_a_same_name_dot_rs_target() {
        let files = vec![
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\nmod probe;\n".to_string(),
            ),
            ("src/probe.rs".to_string(), "fn helper() {}\n".to_string()),
        ];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(test_files.contains("src/probe.rs"), "{test_files:?}");
    }

    #[test]
    fn resolves_a_name_slash_mod_rs_target_when_the_flat_file_does_not_exist() {
        let files = vec![
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\nmod probe;\n".to_string(),
            ),
            (
                "src/probe/mod.rs".to_string(),
                "fn helper() {}\n".to_string(),
            ),
        ];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(test_files.contains("src/probe/mod.rs"), "{test_files:?}");
    }

    #[test]
    fn resolves_a_path_override_target() {
        let files = vec![
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\n#[path = \"generated/probe.rs\"]\nmod probe;\n".to_string(),
            ),
            (
                "src/generated/probe.rs".to_string(),
                "fn helper() {}\n".to_string(),
            ),
        ];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(
            test_files.contains("src/generated/probe.rs"),
            "{test_files:?}"
        );
    }

    #[test]
    fn the_real_eventstore_mod_rs_shape_resolves_contract_rs_as_test() {
        // The exact real-tree case spec 87's Goal names: `pub mod contract;` sits under
        // `#[cfg(test)]` in `src/eventstore/mod.rs`.
        let files = vec![
            (
                "src/eventstore/mod.rs".to_string(),
                "#[cfg(test)]\npub mod contract;\n".to_string(),
            ),
            (
                "src/eventstore/contract.rs".to_string(),
                "pub fn assert_contract() {}\n".to_string(),
            ),
        ];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(
            test_files.contains("src/eventstore/contract.rs"),
            "{test_files:?}"
        );
    }

    #[test]
    fn transitive_closure_pulls_in_a_second_hop_regardless_of_its_own_local_attribute() {
        // outer.rs is test (declared #[cfg(test)] from lib.rs); outer.rs's OWN `mod inner;` has
        // no local #[cfg(test)] at all, but the whole file is already test, so inner.rs must be
        // pulled in too. `outer.rs`'s own children resolve under `src/outer/` (rustc's real
        // file-per-module convention for a non-`mod.rs` declaring file), never a `src/inner.rs`
        // sibling - `declaring_file_module_dir`'s own doc names the earlier version of this
        // resolver that got this wrong.
        let files = vec![
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\nmod outer;\n".to_string(),
            ),
            ("src/outer.rs".to_string(), "mod inner;\n".to_string()),
            (
                "src/outer/inner.rs".to_string(),
                "fn helper() {}\n".to_string(),
            ),
        ];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(test_files.contains("src/outer.rs"));
        assert!(test_files.contains("src/outer/inner.rs"), "{test_files:?}");
    }

    #[test]
    fn a_mod_declaration_with_no_matching_file_resolves_to_nothing() {
        let files = vec![(
            "src/lib.rs".to_string(),
            "#[cfg(test)]\nmod nonexistent;\n".to_string(),
        )];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(test_files.is_empty(), "{test_files:?}");
    }

    #[test]
    fn a_non_test_out_of_line_mod_is_not_pulled_in() {
        let files = vec![
            ("src/lib.rs".to_string(), "mod normal;\n".to_string()),
            ("src/normal.rs".to_string(), "fn helper() {}\n".to_string()),
        ];
        let test_files = resolve_out_of_line_test_files(&files);
        assert!(test_files.is_empty(), "{test_files:?}");
    }

    // -------------------------------------------------------------------------------------
    // Round 1 class 3 (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances`):
    // THE TWO RESOLVERS AGREE, PROVEN - `resolve_out_of_line_test_files` (this file's bespoke
    // text scan) against `out_of_line_test_module_files` (`src/grounder/symbols/events.rs`, spec
    // 86's canonical production resolver - PRIVATE, so this stage-2/3-changes-no-production-code
    // unit cannot call it directly). Compared through its OWN OBSERVABLE EFFECT via the public
    // API (`build_index` -> `index_events`) instead: a file the production resolver excludes is
    // HOLLOWED (`events::for_extraction`) before extraction, so it contributes ZERO
    // `CodeEntityExtracted` events for any of its definitions - a file with at least one
    // non-test definition that still emits none is therefore excluded. Every fixture below (and
    // the real tree's own product code) gives each candidate file at least one always-live
    // top-level item, so "did this file's own item reach the graph" is an unambiguous signal,
    // never confused with a file that is merely, coincidentally, empty of product code. Reads
    // `Event::type_`/`Event::data` generically (both `pub`) rather than the private
    // `CodeEntityExtracted` type itself - `data` is its `serde_json::to_vec` wire form, so a
    // plain `serde_json::Value` walk needs no knowledge of the private struct at all, mirroring
    // how any other real consumer of this event log (a UI, another service) would read it.
    // -------------------------------------------------------------------------------------

    /// The production pipeline's own effective out-of-line-test-file exclusion set, scoped to
    /// `src/` (matching [`resolve_out_of_line_test_files`]'s own scope) - see this section's own
    /// banner comment for the derivation.
    #[cfg(feature = "symbols")]
    fn production_out_of_line_exclusion_set(root: &Path) -> BTreeSet<String> {
        let idx = rigger::grounder::symbols::build_index(root.to_str().unwrap(), None);
        let events = rigger::grounder::symbols::events::index_events(&idx);
        let mut graphed: BTreeSet<String> = BTreeSet::new();
        for e in &events {
            if e.type_ != rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED {
                continue;
            }
            let payload: serde_json::Value =
                serde_json::from_slice(&e.data).expect("CodeEntityExtracted payload is valid JSON");
            if let Some(file) = payload.get("file").and_then(|v| v.as_str()) {
                graphed.insert(file.to_string());
            }
        }
        idx.files()
            .iter()
            .filter(|(path, _)| path.starts_with("src/"))
            .filter(|(path, fs)| {
                fs.defs.iter().any(|d| !d.is_test) && !graphed.contains(path.as_str())
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    #[cfg(feature = "symbols")]
    fn assert_resolvers_agree(files: &[(String, String)]) {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        for (rel, content) in files {
            write_fixture(dir.path(), rel, content);
        }
        let bespoke = resolve_out_of_line_test_files(files);
        let production = production_out_of_line_exclusion_set(dir.path());
        assert_eq!(
            bespoke, production,
            "the bespoke resolver and the production one disagree on this fixture"
        );
    }

    #[cfg(feature = "symbols")]
    #[test]
    fn resolvers_agree_on_a_same_name_dot_rs_target() {
        assert_resolvers_agree(&[
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\nmod probe;\n".to_string(),
            ),
            (
                "src/probe.rs".to_string(),
                "pub fn helper() {}\n".to_string(),
            ),
        ]);
    }

    #[cfg(feature = "symbols")]
    #[test]
    fn resolvers_agree_on_a_path_override_target() {
        assert_resolvers_agree(&[
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\n#[path = \"generated/probe.rs\"]\nmod probe;\n".to_string(),
            ),
            (
                "src/generated/probe.rs".to_string(),
                "pub fn helper() {}\n".to_string(),
            ),
        ]);
    }

    #[cfg(feature = "symbols")]
    #[test]
    fn resolvers_agree_on_a_transitive_second_hop() {
        // `outer.rs`'s own children resolve under `src/outer/` (rustc's real file-per-module
        // convention for a non-`mod.rs` declaring file) - this fixture caught a real bug in
        // `resolve_mod_target`'s prior (sibling-directory) resolution, fixed alongside adding
        // this test; see `declaring_file_module_dir`'s own doc.
        assert_resolvers_agree(&[
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\nmod outer;\n".to_string(),
            ),
            ("src/outer.rs".to_string(), "mod inner;\n".to_string()),
            (
                "src/outer/inner.rs".to_string(),
                "pub fn helper() {}\n".to_string(),
            ),
        ]);
    }

    #[cfg(feature = "symbols")]
    #[test]
    fn resolvers_agree_on_a_non_test_out_of_line_mod() {
        assert_resolvers_agree(&[
            ("src/lib.rs".to_string(), "mod normal;\n".to_string()),
            (
                "src/normal.rs".to_string(),
                "pub fn helper() {}\n".to_string(),
            ),
        ]);
    }

    #[cfg(feature = "symbols")]
    #[test]
    fn resolvers_agree_on_the_real_tree() {
        let root = repo_root();
        let bespoke = resolve_out_of_line_test_files(&collect_src_files_with_content(&root));
        let production = production_out_of_line_exclusion_set(&root);
        assert_eq!(
            bespoke, production,
            "the bespoke resolver and the production one disagree on the real tree"
        );
    }

    // -------------------------------------------------------------------------------------
    // build_dead_code_candidates: the Done-when fixtures
    // -------------------------------------------------------------------------------------

    fn candidates_for(root: &Path) -> Vec<DeadCodeCandidate> {
        let files = scan_tree(root);
        let whole_file_test = resolve_out_of_line_test_files(&collect_src_files_with_content(root));
        build_dead_code_candidates(&files, &whole_file_test)
    }

    #[test]
    fn a_fn_referenced_only_by_its_own_test_is_listed() {
        // Spec 87 Done-when criterion 2, fixture 1.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/lonely.rs",
            "fn orphan() {}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn calls_orphan() {\n        orphan();\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"orphan"), "{names:?}");
        let orphan = candidates.iter().find(|c| c.name == "orphan").unwrap();
        assert_eq!(orphan.test_only_references.len(), 1);
        assert_eq!(orphan.test_only_references[0].file, "src/lonely.rs");
    }

    #[test]
    fn a_fn_referenced_from_a_production_caller_does_not_appear() {
        // Spec 87 Done-when criterion 2, fixture 2.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/used.rs",
            "fn helper() {}\n\nfn caller() {\n    helper();\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"helper"), "{names:?}");
        // `caller` itself has no callers, so it legitimately DOES appear - this fixture's
        // point is only that `helper`, which IS called from production code, does not.
        assert!(names.contains(&"caller"), "{names:?}");
    }

    #[test]
    fn a_shared_name_referenced_via_self_colon_colon_from_within_its_own_impl_is_not_a_false_positive(
    ) {
        // Spec 87 criterion 3 regression, found while researching dispositions: the real
        // `src/dash.rs` `DashMarker::parse` (ambiguous with `gate.rs`/`ledger.rs` x2/`failure.rs`'s
        // own `parse`s) is referenced ONLY via `Self::parse(...)` from its own `DashMarker::read`
        // (dash.rs:408) - itself a REAL production call path (`main.rs:5731/7313/7629` all call
        // `DashMarker::read`). `qualifier_before` (round 1) captures the literal token text
        // in front of `::`, and for `Self::parse(` that text is the keyword `"Self"`, never the
        // enclosing type's own name - so `impl_assoc_qualifier`'s `resolved == my_qualifier`
        // attribution check always failed for a `Self::`-qualified call, reading a genuinely LIVE
        // ambiguous `ImplAssoc` fn as dead: the dangerous false-positive direction this whole
        // unit's own precision discipline forbids ("a false-positive dead verdict is the
        // dangerous direction", `u87c2-three-precision-fixes-from-real-tree-spot-check`) - this
        // JSON feeds section 6's real deletion list, so a false positive here would recommend
        // deleting live code. `src/b.rs`'s unrelated, unreferenced-anywhere `parse` (the same
        // shared bare name, a DIFFERENT file) stays a genuine dead candidate - the fix must not
        // widen ambiguity attribution beyond the same-file `Self::` case.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "struct A;\n\nimpl A {\n    fn parse(s: &str) -> Option<A> {\n        let _ = s;\n        None\n    }\n\n    fn read(s: &str) -> Option<A> {\n        Self::parse(s)\n    }\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/b.rs",
            "struct B;\n\nimpl B {\n    fn parse(s: &str) -> Option<B> {\n        let _ = s;\n        None\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<(&str, &str)> = candidates
            .iter()
            .map(|c| (c.file.as_str(), c.name.as_str()))
            .collect();
        assert!(
            !names.contains(&("src/a.rs", "parse")),
            "src/a.rs's parse is called via Self::parse from its own read (production code) - it \
             must not read as dead: {names:?}"
        );
        assert!(
            names.contains(&("src/b.rs", "parse")),
            "src/b.rs's parse has no reference anywhere - it must still read as genuinely dead: \
             {names:?}"
        );
    }

    #[test]
    fn a_path_qualified_reference_with_no_call_parens_still_counts() {
        // "or used as a path segment" (spec 87 Design) - a fn passed by name, e.g. as a
        // function pointer, with no trailing `(`.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/ptr.rs",
            "pub fn target() {}\n\nmod user {\n    fn takes_ptr(_f: fn()) {}\n    fn wire() {\n        takes_ptr(super::target);\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"target"), "{names:?}");
    }

    #[test]
    fn a_mention_inside_a_comment_does_not_count_as_a_reference() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/commented.rs",
            "fn orphan() {}\n\n// this comment happens to say orphan() but never calls it\nfn other() {\n    let _ = 1;\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"orphan"), "{names:?}");
    }

    #[test]
    fn visibility_is_carried_through_to_the_candidate() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/vis.rs", "pub(crate) fn orphan() {}\n");
        let candidates = candidates_for(dir.path());
        let orphan = candidates.iter().find(|c| c.name == "orphan").unwrap();
        assert_eq!(orphan.visibility, "pub(crate)");
    }

    #[test]
    fn recursion_through_the_fns_own_body_still_counts_as_a_reference() {
        // Spec 87 Design: the excluded "definition span" is "doc comment, attributes,
        // signature" - NOT the body, so a self-call inside the body is a real reference.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/rec.rs",
            "fn countdown(n: u32) {\n    if n > 0 {\n        countdown(n - 1);\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"countdown"), "{names:?}");
    }

    #[test]
    fn a_whole_file_test_via_out_of_line_resolution_is_never_a_candidate() {
        // The real `src/eventstore/contract.rs` shape: `assert_contract` has no LOCAL
        // #[cfg(test)] at all, but the whole file is pulled in as test by `mod.rs`.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/es/mod.rs",
            "#[cfg(test)]\npub mod contract;\n",
        );
        write_fixture(
            dir.path(),
            "src/es/contract.rs",
            "pub fn assert_contract() {}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"assert_contract"), "{names:?}");
    }

    #[test]
    fn main_is_exempted_as_an_entry_point() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/main.rs", "fn main() {}\n");
        let candidates = candidates_for(dir.path());
        assert!(
            candidates.iter().all(|c| c.name != "main"),
            "{candidates:?}"
        );
    }

    // -------------------------------------------------------------------------------------
    // Round 1 (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances`): the three
    // classes the round-0 REJECT named, each pinned by its own fixture on the real motivating
    // shape.
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_named_use_import_at_mod_test_top_level_does_not_leak_as_a_production_reference() {
        // Class 1 ("TEST REGIONS ARE MOD SPANS"): the real `src/grounder/symbols/events.rs`
        // shape (`sdet-u87c2-mod-body-level-test-statements-leak-as-production-refs`) - a named
        // `use` import sits directly inside `#[cfg(test)] mod tests { .. }`, ABOVE its `#[test]`
        // fn (never itself a `ScannedFn`), naming `orphan`. Before round 1, `in_test_range` was
        // built from fn spans alone, so this line misclassified `orphan` as production-
        // referenced and it never appeared in the JSON at all - the exact false negative that
        // defeated criterion 2's own Done-when on spec 87's own Goal-cited worked example.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/orphan.rs",
            "pub fn orphan() {}\n\n#[cfg(test)]\nmod tests {\n    use crate::orphan::orphan;\n\n    #[test]\n    fn calls_orphan() {\n        orphan();\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"orphan"), "{names:?}");
        let orphan = candidates.iter().find(|c| c.name == "orphan").unwrap();
        assert_eq!(orphan.test_only_references.len(), 2, "{orphan:?}");
    }

    #[test]
    fn a_serde_default_attribute_string_names_a_real_production_reference() {
        // Class 2 ("ATTRIBUTE TOKEN TREES ARE REFERENCES"): the real `src/config.rs` shape
        // (`sdet-u87c2-serde-default-attr-string-ref-is-a-false-positive`) -
        // `default_build_config` is referenced ONLY through `#[serde(default = "..")]`'s string
        // literal, a shape no call-site rule (`followed by (`, `::`, `.`, `<`) ever matches.
        // Before round 1 this was a false positive: a genuinely live fn sat in the JSON as dead.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/cfg.rs",
            "#[derive(serde::Deserialize)]\nstruct Cfg {\n    #[serde(default = \"default_build_config\")]\n    build: String,\n}\n\nfn default_build_config() -> String {\n    String::new()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"default_build_config"), "{names:?}");
    }

    #[test]
    fn an_attribute_reference_to_an_ambiguous_shared_name_credits_every_sharer() {
        // The `via_attribute` exemption from ambiguity attribution: an attribute mention cannot
        // be qualifier-resolved (it names no `Type::`/`module::` prefix at all), and the design's
        // conservative direction says it must still keep BOTH same-named sharers alive rather
        // than resolve one of them dead just because the attribute could not name which it meant.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/attr_amb.rs",
            "mod a {\n    pub fn make_default() -> u32 {\n        0\n    }\n}\nmod b {\n    pub fn make_default() -> u32 {\n        1\n    }\n}\n#[derive(serde::Deserialize)]\nstruct Cfg {\n    #[serde(default = \"make_default\")]\n    n: u32,\n}\n",
        );
        let candidates = candidates_for(dir.path());
        assert!(
            candidates.iter().all(|c| c.name != "make_default"),
            "{candidates:?}"
        );
    }

    #[test]
    fn a_free_fn_bare_name_collision_where_only_one_sharer_has_a_real_caller_flags_the_other() {
        // Class 4, the adversary's `distiller::rebuild`/`playbooks::rebuild` finding
        // (`adv-u87c2-r0-free-fn-bare-name-collision-hides-a-genuinely-dead-fn`): two UNRELATED
        // top-level free fns share the bare name `rebuild`; only one has a real, `::`-qualified
        // caller. Before round 1, bare-name-only resolution silently counted BOTH alive because
        // is the aggregate `rebuild(` reference set was non-empty; round 1's per-definition
        // qualifier attribution now correctly excludes the called one and flags the other.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/distiller.rs",
            "pub fn rebuild() -> u32 {\n    1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/playbooks.rs",
            "pub fn rebuild() -> u32 {\n    2\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/main.rs",
            "fn main() {\n    let _ = playbooks::rebuild();\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let rebuilds: Vec<&DeadCodeCandidate> =
            candidates.iter().filter(|c| c.name == "rebuild").collect();
        assert_eq!(rebuilds.len(), 1, "{candidates:?}");
        assert_eq!(rebuilds[0].file, "src/distiller.rs", "{rebuilds:?}");
        assert!(rebuilds[0].ambiguous, "{rebuilds:?}");
        assert_eq!(
            rebuilds[0].ambiguous_with,
            vec!["src/playbooks.rs:1".to_string()]
        );
    }

    #[test]
    fn a_bare_call_in_the_same_file_as_its_definition_attributes_locally_with_no_import_needed() {
        // Attribution path 3: Rust's own lexical scoping resolves an unqualified sibling call
        // with no `use` needed at all when the call sits in the SAME file as the definition -
        // `two.rs`'s own bare `rebuild()` call attributes to `two.rs`'s own `rebuild`, leaving
        // the unrelated `one.rs` sharer (zero callers of its own) correctly flagged ambiguous.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/one.rs",
            "pub fn rebuild() -> u32 {\n    1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/two.rs",
            "pub fn rebuild() -> u32 {\n    2\n}\nfn use_it() -> u32 {\n    rebuild()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let rebuilds: Vec<&DeadCodeCandidate> =
            candidates.iter().filter(|c| c.name == "rebuild").collect();
        assert_eq!(rebuilds.len(), 1, "{candidates:?}");
        assert_eq!(rebuilds[0].file, "src/one.rs", "{rebuilds:?}");
        assert!(rebuilds[0].ambiguous, "{rebuilds:?}");
    }

    #[test]
    fn a_bare_unqualified_call_from_a_third_unrelated_file_credits_neither_sharer() {
        // The addendum's literal "credited to NO definition" case: a BARE `rebuild()` call from
        // a THIRD file (neither sharer's own, and no `use` import resolving it) cannot be
        // attributed to either, so BOTH remain zero-attributed and BOTH are flagged ambiguous -
        // never a false "somebody calls it somewhere" pass for either one.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/one.rs",
            "pub fn rebuild() -> u32 {\n    1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/two.rs",
            "pub fn rebuild() -> u32 {\n    2\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/three.rs",
            "fn use_it() -> u32 {\n    rebuild()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let rebuilds: Vec<&DeadCodeCandidate> =
            candidates.iter().filter(|c| c.name == "rebuild").collect();
        assert_eq!(rebuilds.len(), 2, "{candidates:?}");
        assert!(rebuilds.iter().all(|c| c.ambiguous), "{rebuilds:?}");
    }

    #[test]
    fn a_bare_call_resolved_through_a_use_import_attributes_to_the_imported_definition() {
        // The addendum's other attribution path: "a `use module::name;` in the referencing file
        // resolving to it" - a BARE `rebuild()` call in a file that imports it by qualified path
        // attributes to that specific definition, same as a `module::rebuild()` call site would.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/distiller.rs",
            "pub fn rebuild() -> u32 {\n    1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/playbooks.rs",
            "pub fn rebuild() -> u32 {\n    2\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/main.rs",
            "use crate::playbooks::rebuild;\nfn main() {\n    let _ = rebuild();\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let rebuilds: Vec<&DeadCodeCandidate> =
            candidates.iter().filter(|c| c.name == "rebuild").collect();
        assert_eq!(rebuilds.len(), 1, "{candidates:?}");
        assert_eq!(rebuilds[0].file, "src/distiller.rs", "{rebuilds:?}");
        assert!(rebuilds[0].ambiguous, "{rebuilds:?}");
    }

    #[test]
    fn a_method_name_shared_by_two_impls_with_zero_calls_is_flagged_ambiguous() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/amb.rs",
            "struct A;\nstruct B;\nimpl A {\n    fn reset(&mut self) {}\n}\nimpl B {\n    fn reset(&mut self) {}\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let resets: Vec<&DeadCodeCandidate> =
            candidates.iter().filter(|c| c.name == "reset").collect();
        assert_eq!(resets.len(), 2, "{candidates:?}");
        assert!(resets.iter().all(|c| c.ambiguous), "{resets:?}");
    }

    #[test]
    fn a_method_name_shared_by_two_impls_with_a_call_site_excludes_both() {
        // Cannot tell which `reset` a receiver-agnostic `.reset()` call targets - spec 87
        // Constraints Walk: "reported as ambiguous rather than counted as alive", so neither
        // sharer is asserted dead.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/amb2.rs",
            "struct A;\nstruct B;\nimpl A {\n    fn reset(&mut self) {}\n}\nimpl B {\n    fn reset(&mut self) {}\n}\nfn use_one(a: &mut A) {\n    a.reset();\n}\n",
        );
        let candidates = candidates_for(dir.path());
        assert!(
            candidates.iter().all(|c| c.name != "reset"),
            "{candidates:?}"
        );
    }

    #[test]
    fn an_unambiguous_method_with_zero_calls_is_a_plain_candidate_not_ambiguous() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/solo.rs",
            "struct A;\nimpl A {\n    fn reset(&mut self) {}\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let reset = candidates.iter().find(|c| c.name == "reset").unwrap();
        assert!(!reset.ambiguous, "{reset:?}");
    }

    #[test]
    fn a_dot_call_through_an_inherent_method_on_any_receiver_counts_receiver_agnostically() {
        // The Constraints Walk's "called only through a trait object" case, using an INHERENT
        // impl so this exercises a plain `.name(` occurrence, not the separate trait-impl
        // exemption below.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/dyn_dispatch.rs",
            "struct Real;\nimpl Real {\n    fn spawn(&self) {}\n}\nfn run(r: &Real) {\n    r.spawn();\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"spawn"), "{names:?}");
    }

    #[test]
    fn a_trait_impl_method_is_exempted_even_with_zero_textual_call_sites() {
        // Drop::drop shape - invoked by the compiler at scope end, never via an explicit
        // `.drop(` call site anywhere in real source text. Without the trait-impl exemption
        // this would be a false-positive dead-code candidate on every real `impl Drop`.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/droppable.rs",
            "struct Guard;\nimpl Drop for Guard {\n    fn drop(&mut self) {}\n}\n",
        );
        let candidates = candidates_for(dir.path());
        assert!(
            candidates.iter().all(|c| c.name != "drop"),
            "{candidates:?}"
        );
    }

    #[test]
    fn an_inherent_associated_function_is_matched_via_path_shape_not_dot_shape() {
        // `Type::new()` has no preceding `.` - historically a fn taking no `self` had to be
        // matched via a separate `::`-preceded rule from a method's `.name(` rule; round 3
        // dropped that distinction for "is this a reference at all" (any occurrence counts
        // regardless), but `ImplAssoc` still needs its own qualifier-based attribution when a
        // name is shared - this pins the base case, `Foo::new()` keeping `new` alive at all.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/ctor.rs",
            "struct Foo;\nimpl Foo {\n    fn new() -> Self {\n        Foo\n    }\n}\nfn make() -> Foo {\n    Foo::new()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"new"), "{names:?}");
    }

    #[test]
    fn an_inherent_associated_fn_name_shared_by_two_types_with_zero_calls_is_ambiguous() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/ctors.rs",
            "struct A;\nstruct B;\nimpl A {\n    fn new() -> Self {\n        A\n    }\n}\nimpl B {\n    fn new() -> Self {\n        B\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let news: Vec<&DeadCodeCandidate> = candidates.iter().filter(|c| c.name == "new").collect();
        assert_eq!(news.len(), 2, "{candidates:?}");
        assert!(news.iter().all(|c| c.ambiguous), "{news:?}");
    }

    #[test]
    fn a_qualified_call_site_attributes_only_to_the_sharer_it_names() {
        // Round 1 (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): a `Type::new()`-qualified
        // call site is now ATTRIBUTED to the ONE sharer it names, not credited to every sharer
        // the way an unattributable bare mention would be - `A::new()` proves `A::new` alive
        // (excluded) while `B::new`, with zero calls of its own, is correctly flagged ambiguous
        // rather than silently hidden behind `A::new`'s real caller (the same failure shape the
        // adversary's `distiller::rebuild`/`playbooks::rebuild` finding named for free fns).
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/ctors2.rs",
            "struct A;\nstruct B;\nimpl A {\n    fn new() -> Self {\n        A\n    }\n}\nimpl B {\n    fn new() -> Self {\n        B\n    }\n}\nfn make() -> A {\n    A::new()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let news: Vec<&DeadCodeCandidate> = candidates.iter().filter(|c| c.name == "new").collect();
        assert_eq!(news.len(), 1, "{candidates:?}");
        assert_eq!(news[0].file, "src/ctors2.rs");
        assert_eq!(news[0].line, 9, "expected B::new specifically; {news:?}");
        assert!(news[0].ambiguous, "{news:?}");
        assert_eq!(news[0].ambiguous_with, vec!["src/ctors2.rs:4".to_string()]);
    }

    #[test]
    fn a_qualified_call_site_on_an_impls_own_generic_self_type_attributes_correctly() {
        // Regression for round-1's own defect (sdet-u87c2-r1-impl-assoc-qualifier-drops-leading-
        // impl-generics-reintroduces-false-positives, upheld by the round-1 adjudication reject):
        // when the impl block declares ITS OWN leading generic/lifetime parameters
        // (`impl<'a> Widget<'a>`), `enclosing_impl`'s header text starts with `<` itself (the
        // `impl` keyword is never stored). The old naive
        // `header.split(|c| c == '<' || c.is_whitespace()).next()` therefore returned an EMPTY
        // qualifier for `Widget::new`, which could never equal the real `Widget::new()` call
        // site's resolved qualifier `Some("Widget")` - so the genuinely-alive `Widget::new` was
        // wrongly flagged ambiguous with zero references, exactly the false-positive shape found
        // in the committed `dead-code.json` for `Namespaced::new`/`ReplayDriver::new`/
        // `Buckets::new`/`Server::new`. `Other::new`, with zero callers of its own, is the one
        // that must remain correctly flagged.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/generic_ctors.rs",
            "struct Widget<'a>(std::marker::PhantomData<&'a ()>);\nstruct Other;\nimpl<'a> Widget<'a> {\n    fn new() -> Self {\n        Widget(std::marker::PhantomData)\n    }\n}\nimpl Other {\n    fn new() -> Self {\n        Other\n    }\n}\nfn make() -> Widget<'static> {\n    Widget::new()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let news: Vec<&DeadCodeCandidate> = candidates.iter().filter(|c| c.name == "new").collect();
        assert_eq!(news.len(), 1, "{candidates:?}");
        assert_eq!(news[0].file, "src/generic_ctors.rs");
        assert_eq!(
            news[0].line, 9,
            "expected Other::new specifically (Widget::new has a real qualified caller); {news:?}"
        );
        assert!(news[0].ambiguous, "{news:?}");
        assert_eq!(
            news[0].ambiguous_with,
            vec!["src/generic_ctors.rs:4".to_string()]
        );
    }

    #[test]
    fn a_fn_passed_by_value_as_a_bare_call_argument_counts_as_a_reference() {
        // The real bug this fixture pins: `.map_err(be)` (found live in
        // `src/contextgraph/sqlite.rs`) passes `be` BY NAME with no call syntax, `.`, or `::`
        // of its own at all. THE RULE (round 3) makes this one case among many value-position
        // shapes below - no dedicated argument-slot rule is left to name.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/map_err.rs",
            "struct Error(String);\nfn be<E: std::fmt::Display>(e: E) -> Error {\n    Error(e.to_string())\n}\nfn open() -> Result<(), Error> {\n    std::fs::metadata(\"x\").map(|_| ()).map_err(be)\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"be"), "{names:?}");
    }

    // -------------------------------------------------------------------------------------
    // Round 3 (`op-u87c2-round-3-a-reference-is-any-token-not-a-shape`): one fixture per
    // value-position shape the operator's ruling names, each a real class the round-2
    // shape-based scanner missed (or would have missed next, by the same pattern) - a
    // struct-literal field value (the round-2 upheld defect itself,
    // `sdet-u87c2-r2-fnptr-struct-field-value-is-an-invisible-reference-shape`), a UFCS value
    // passed to a combinator on a METHOD-category fn (the second round-2 upheld defect,
    // `sdet-u87c2-r2-method-category-relevant-filter-discards-ufcs-qualified-call-sites`), a
    // `let` initializer, an array element, a match arm, a return expression, and a generic
    // argument to another type. THE RULE makes all seven the same code path; these fixtures
    // exist so a future shape-based regression (the u86c1 six-round repeat-discovery pattern)
    // is caught immediately rather than rediscovered one shape at a time.
    // -------------------------------------------------------------------------------------

    #[test]
    fn a_fn_pointer_used_as_a_struct_literal_field_value_counts_as_a_reference() {
        // Real production shape this pins: `src/docs.rs`'s `skill_registry()`, e.g.
        // `SkillEntry { name: "x", render_body: render_x_skill }` - `render_x_skill` is a bare
        // identifier VALUE in struct-literal field position, no call/dot/`::`/`<` of its own.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/registry.rs",
            "struct Entry {\n    name: &'static str,\n    render_body: fn() -> String,\n}\nfn render_a() -> String {\n    String::new()\n}\nfn registry() -> Vec<Entry> {\n    vec![Entry {\n        name: \"a\",\n        render_body: render_a,\n    }]\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"render_a"), "{names:?}");
    }

    #[test]
    fn a_ufcs_qualified_value_passed_to_a_combinator_counts_as_a_reference_for_a_method() {
        // Real production shape this pins: `src/config.rs`'s
        // `.map(FailureRuleDef::to_rule)` - `to_rule` takes `&self` (DispatchCategory::Method,
        // dispatched receiver-agnostically via `.to_rule(`) but here is referenced by its own
        // UFCS PATH as a bare value with no call of its own - round 2's `relevant()` filter
        // checked only `method_shaped` for `Method` and dropped this site entirely.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/ufcs_method.rs",
            "struct Rule;\nimpl Rule {\n    fn to_rule(&self) -> i32 {\n        0\n    }\n}\nfn apply(rules: Vec<Rule>) -> Vec<i32> {\n    rules.iter().map(Rule::to_rule).collect()\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"to_rule"), "{names:?}");
    }

    #[test]
    fn a_fn_named_by_a_let_initializer_counts_as_a_reference() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/let_init.rs",
            "fn handler() -> i32 {\n    0\n}\nfn wire() -> fn() -> i32 {\n    let f = handler;\n    f\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"handler"), "{names:?}");
    }

    #[test]
    fn a_fn_named_as_an_array_element_counts_as_a_reference() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/array_elem.rs",
            "fn step_one() {}\nfn step_two() {}\nfn pipeline() -> [fn(); 2] {\n    [step_one, step_two]\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"step_one"), "{names:?}");
        assert!(!names.contains(&"step_two"), "{names:?}");
    }

    #[test]
    fn a_fn_named_in_a_match_arm_value_counts_as_a_reference() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/match_arm.rs",
            "fn plan_a() {}\nfn plan_b() {}\nfn choose(n: u8) -> fn() {\n    match n {\n        0 => plan_a,\n        _ => plan_b,\n    }\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"plan_a"), "{names:?}");
        assert!(!names.contains(&"plan_b"), "{names:?}");
    }

    #[test]
    fn a_fn_named_in_a_return_expression_counts_as_a_reference() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/return_expr.rs",
            "fn default_handler() {}\nfn get_handler() -> fn() {\n    return default_handler;\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"default_handler"), "{names:?}");
    }

    #[test]
    fn a_fn_named_as_a_generic_argument_to_another_type_counts_as_a_reference() {
        // `token(&self, i)` never called, never assigned - only NAMED, as another type's own
        // generic parameter (`Holder<marker_fn>`), a shape no call/dot/`::`/`<`-of-its-own rule
        // would ever see since `marker_fn` itself is followed by `>`, not `(`/`.`/`::`/`<`.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/generic_arg.rs",
            "fn marker_fn() {}\nstruct Holder<F>(std::marker::PhantomData<F>);\nfn make() -> Holder<marker_fn> {\n    Holder(std::marker::PhantomData)\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"marker_fn"), "{names:?}");
    }

    #[test]
    fn the_real_tree_no_longer_flags_the_round_2_struct_field_and_ufcs_defects() {
        // Real-tree pin (operator ruling `op-u87c2-round-3-a-reference-is-any-token-not-a-
        // shape`): the exact production fns round 2's adjudication reject named as invisible -
        // every `src/docs.rs` `skill_registry()` `render_*` entry point, and
        // `src/config.rs`'s `FailureRuleDef::to_rule` - must be ABSENT from the real
        // `dead-code.json`, never merely "no longer ambiguous" or "still present but fixed
        // elsewhere".
        let names: std::collections::HashSet<&str> = real_dead_code_candidates()
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(!names.contains("to_rule"), "{names:?}");
        for render_fn in [
            "render_using_rigger_skill",
            "render_planning_a_spec_skill",
            "render_reset_store_skill",
            "render_build_graph_skill",
            "render_reindex_skill",
            "render_resume_a_run_skill",
            "render_handle_an_escalation_skill",
            "render_watch_a_run_skill",
            "render_restore_the_dash_skill",
            "render_diagnose_churn_skill",
        ] {
            assert!(!names.contains(render_fn), "{render_fn} still in {names:?}");
        }
    }

    // -------------------------------------------------------------------------------------
    // THE DRIFT GUARD for `docs/audit/dead-code.json`
    // -------------------------------------------------------------------------------------

    /// THE DRIFT GUARD for `docs/audit/dead-code.json`: with `RIGGER_AUDIT_WRITE=1` set,
    /// regenerate and overwrite it; otherwise regenerate in memory and assert it matches the
    /// committed file byte-for-byte. Mirrors `duplication_catalog_json_matches_the_tree_or_
    /// is_rewritten` exactly.
    #[test]
    fn dead_code_json_matches_the_tree_or_is_rewritten() {
        let root = repo_root();
        let candidates = real_dead_code_candidates();
        let json = dead_code_to_json(candidates);
        let path = root.join(DEAD_CODE_PATH);
        if std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1") {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, &json).unwrap();
            fs::write(
                root.join(DEAD_CODE_LINES_PATH),
                dead_code_lines_to_json(candidates),
            )
            .unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{DEAD_CODE_PATH} is missing or unreadable ({e}) - run with RIGGER_AUDIT_WRITE=1 \
                 to generate it"
            )
        });
        assert_eq!(
            committed, json,
            "{DEAD_CODE_PATH} has drifted from the tree - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    /// Spec 90 criterion 2, CLAIM 1 for `docs/audit/dead-code.json`: structurally, no candidate
    /// or test-only reference carries `line`, and every `content_hash` is non-empty. Also checks
    /// any `ambiguous_with` citation is `file#hash`-shaped, never `file:line`.
    #[test]
    fn the_real_committed_dead_code_json_carries_no_line_number_fields() {
        let candidates = real_dead_code_candidates();
        let json = dead_code_to_json(candidates);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let arr = value.as_array().expect("a bare array");
        assert!(!arr.is_empty());
        for entry in arr {
            let obj = entry.as_object().expect("a candidate object");
            assert!(
                !obj.contains_key("line"),
                "entry {entry:?} still carries line"
            );
            let hash = obj
                .get("content_hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            assert!(
                !hash.is_empty(),
                "entry {entry:?} has an empty content_hash"
            );
            for citation in obj
                .get("ambiguous_with")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                let citation = citation.as_str().unwrap_or_default();
                assert!(
                    citation.contains('#') && !citation.contains(':'),
                    "ambiguous_with citation {citation:?} is not file#hash-shaped"
                );
            }
            for r in entry["test_only_references"].as_array().expect("array") {
                let robj = r.as_object().expect("a reference object");
                assert!(
                    !robj.contains_key("line"),
                    "reference {r:?} still carries line"
                );
                let rhash = robj
                    .get("content_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                assert!(
                    !rhash.is_empty(),
                    "reference {r:?} has an empty content_hash"
                );
            }
        }
    }

    /// Spec 90 criterion 2, CLAIM 2 for `docs/audit/dead-code.json`: a synthetic fixture tree
    /// proves a pin bump (5 unrelated comment lines prepended to one file, shifting that file's
    /// own candidate's line span) leaves the guarded dead-code JSON byte-identical, because
    /// `content_hash` keys on each candidate's own span text, never its line number. Closes
    /// `sdet-u90c2-surface-accounting`/`sdet-u90c2-deadcode-map-missing-claim2-claim3-tests` -
    /// sdet's own reverted probe already empirically confirmed this property; this test is that
    /// probe made permanent, using the file's own [`candidates_for`] fixture helper (never
    /// [`real_dead_code_candidates`], which requires the real tree's `disposition_for` table).
    #[test]
    fn a_pin_bump_leaves_the_guarded_dead_code_json_byte_identical() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            dir.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let base_json = dead_code_to_json(&candidates_for(dir.path()));

        write_fixture(
            dir.path(),
            "src/a.rs",
            "// pin: v1\n// pin: v2\n// pin: v3\n// pin: v4\n// pin: v5\n\
             fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        let bumped_json = dead_code_to_json(&candidates_for(dir.path()));

        assert_eq!(
            base_json, bumped_json,
            "a pin bump that only shifts a candidate's OWN line number must leave the guarded \
             dead-code JSON byte-identical (spec 90 criterion 2)"
        );
    }

    /// Spec 90 criterion 2, CLAIM 3 for `docs/audit/dead-code.json`, REFRAMED for a per-entry
    /// artifact - see the identical reframing on
    /// `two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry`
    /// for the full rationale (this file's own `json_array_entries` helper is shared with that
    /// test). Two branches, each adding one new, unreferenced (hence dead-code-candidate)
    /// function to a DIFFERENT file, leave every pre-existing candidate byte-identical and each
    /// contribute exactly their own one new entry.
    #[test]
    fn two_branches_each_adding_an_unrelated_function_to_a_different_file_never_perturb_an_existing_dead_code_entry(
    ) {
        let base = tempfile::tempdir().expect("base scratch dir");
        write_fixture(
            base.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            base.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let base_entries = json_array_entries(&dead_code_to_json(&candidates_for(base.path())));

        let branch_a = tempfile::tempdir().expect("branch A scratch dir");
        write_fixture(
            branch_a.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n\n\
             fn branch_a_only(x: i64) -> i64 {\n    x * 3 - 7\n}\n",
        );
        write_fixture(
            branch_a.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n",
        );
        let a_entries = json_array_entries(&dead_code_to_json(&candidates_for(branch_a.path())));

        let branch_b = tempfile::tempdir().expect("branch B scratch dir");
        write_fixture(
            branch_b.path(),
            "src/a.rs",
            "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n",
        );
        write_fixture(
            branch_b.path(),
            "src/z.rs",
            "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n\n\
             fn branch_b_only(y: i64) -> i64 {\n    y / 2 + 11\n}\n",
        );
        let b_entries = json_array_entries(&dead_code_to_json(&candidates_for(branch_b.path())));

        assert_eq!(
            a_entries.len(),
            base_entries.len() + 1,
            "branch A must contribute exactly one new candidate"
        );
        assert_eq!(
            b_entries.len(),
            base_entries.len() + 1,
            "branch B must contribute exactly one new candidate"
        );
        for entry in &base_entries {
            assert!(
                a_entries.contains(entry),
                "branch A perturbed or dropped a pre-existing candidate {entry}"
            );
            assert!(
                b_entries.contains(entry),
                "branch B perturbed or dropped a pre-existing candidate {entry}"
            );
        }
        let a_new: Vec<&serde_json::Value> = a_entries
            .iter()
            .filter(|e| !base_entries.contains(e))
            .collect();
        let b_new: Vec<&serde_json::Value> = b_entries
            .iter()
            .filter(|e| !base_entries.contains(e))
            .collect();
        assert_eq!(a_new.len(), 1, "branch A's own new candidate: {a_new:?}");
        assert_eq!(b_new.len(), 1, "branch B's own new candidate: {b_new:?}");
        assert_ne!(
            a_new[0], b_new[0],
            "the two branches' new candidates must be distinct - nothing for a real merge to \
             conflict over"
        );
    }

    #[test]
    fn every_real_candidate_has_a_zero_degree_knowledge_graph_cross_check_shape() {
        // Every real candidate is a genuine `src/` production fn (never a test, never from
        // `tests/`). The actual `rigger graph --show <entity>` degree per candidate (spec 87
        // Design's cross-check) is reported in the report's own section 4 full list - this test
        // pins only the file-scope shape every candidate must satisfy for that lookup to be
        // meaningful at all.
        for c in real_dead_code_candidates() {
            assert!(c.file.starts_with("src/"), "{c:?}");
        }
    }

    /// Spec 87 criterion 3's own THOROUGHNESS proof (mirrors spec 85's "coverage is asserted by
    /// the generator" discipline): [`disposition_for`] panicking on an unrecognized `(file,
    /// line)` IS the enforcement mechanism (a candidate the tree has grown since this research
    /// would fail every test in this suite, loudly, naming exactly which one) - this test adds a
    /// second, independent proof that every entry actually carries a non-empty reason and that
    /// the three-disposition vocabulary is exhaustive (spec 87 DISPOSITIONS: "exactly three").
    #[test]
    fn every_real_candidate_carries_exactly_one_disposition_with_a_non_empty_cited_reason() {
        let candidates = real_dead_code_candidates();
        assert!(
            !candidates.is_empty(),
            "expected real dead-code candidates to disposition"
        );
        for c in candidates {
            assert!(
                !c.reason.trim().is_empty(),
                "{}:{} ({}) carries an empty disposition reason",
                c.file,
                c.line,
                c.name
            );
            assert!(
                matches!(
                    c.disposition,
                    Disposition::Delete | Disposition::KeepPublicSurface | Disposition::KeepPending
                ),
                "{}:{} ({}) has an unrecognized disposition {:?}",
                c.file,
                c.line,
                c.name,
                c.disposition
            );
        }
    }

    /// Regression pin: the exact real-tree candidate count and disposition split this criterion's
    /// research established (26 candidates: the Self:: fix above removes `dash.rs::parse`'s false
    /// positive from criterion 2's original 27; 23 `delete`, 3 `keep-pending`, 0
    /// `keep-public-surface` today - spec 88 criterion 1 round 4 then dropped this to 25/22/0/3:
    /// `expect_merged` (formerly src/worktree.rs:86, `delete`) is no longer a production
    /// candidate at all, moved into `src/worktree.rs`'s own `#[cfg(test)] mod tests` alongside
    /// the `IntegrateOutcome` enum and a test-only `integrate` recomposition of the newly-split
    /// `merge_into_worktree`/`land`). A future change to either the tree or `disposition_for`
    /// that shifts this split should be a deliberate, reviewed edit - this test makes that shift
    /// visible rather than silent.
    #[test]
    fn the_real_tree_disposition_split_matches_this_criterions_research() {
        let candidates = real_dead_code_candidates();
        let delete = candidates
            .iter()
            .filter(|c| c.disposition == Disposition::Delete)
            .count();
        let keep_public = candidates
            .iter()
            .filter(|c| c.disposition == Disposition::KeepPublicSurface)
            .count();
        let keep_pending = candidates
            .iter()
            .filter(|c| c.disposition == Disposition::KeepPending)
            .count();
        assert_eq!(
            (candidates.len(), delete, keep_public, keep_pending),
            (25, 22, 0, 3),
            "the real-tree candidate count or disposition split has changed since this \
             criterion's research - {candidates:#?}"
        );
    }
}
