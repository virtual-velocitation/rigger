//! Spec 85 criterion 1, THE RESPONSIBILITY MAP: a deterministic, zero-new-dependency scan of
//! every function in `crates/rigger-conductor/src/conductor.rs`, `src/cli/mod.rs` and `crates/rigger-dash/src/dash.rs`, each assigned a
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
//! `crates/rigger-conductor/src/conductor.rs`), string and raw-string literals (`r"..."`, `r#"..."#`, ... with
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
//! by their exact normalized stream (an O(n) hashmap pass - this is also the entire `exact`
//! pass) and only one representative per distinct stream is fed through an inverted index over
//! each set's PREFIX-FILTER fingerprints ([`prefix_len`]) for the `near` pass. Prefix filtering
//! is exact: every pair at or above the threshold shares a prefix fingerprint, so no pair is
//! ever dropped and a helper added anywhere cannot change another cluster's membership.
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

mod common;
use common::repo::collect_rs_files;
use common::repo::repo_root;
#[path = "common/source_audit.rs"]
mod source_audit;
use source_audit::{
    char_literal_len, is_ident_char, skip_string_literal, tokenize, RawKind, RawTok,
};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Spec 90 criterion 2's line-free content identity, reused rather than a second open-coded
/// hasher: `content_hash` is the crate's ONE stable content-hash primitive (its own doc calls it
/// "the content-identity primitive"; this report's own DUPLICATION section separately calls it
/// "the crate's ONE stable content-hash primitive"), already depended on for the `symbols`
/// grounder's reindex-freshening gate. See [`span_content_hash`].
use rigger::grounder::symbols::store::content_hash;

/// The three files this criterion scans - spec 85's Design and Done-when name them by literal
/// path, in this fixed order (also the order every generated artifact lists them in).
const TARGET_FILES: [&str; 3] = [
    "crates/rigger-conductor/src/conductor.rs",
    "src/cli/mod.rs",
    "crates/rigger-dash/src/dash.rs",
];

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
            if attr_text.contains("cfg(test)")
                || attr_text.contains("#[test]")
                || cfg_all_contains_bare_test(&attr_text)
            {
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

/// Whether `attr_text` (a full `#[...]` attribute, braces included) is a `cfg` that can ONLY
/// ever be active under a test build - so classifying it as `is_test` is always sound, never a
/// false positive. This scanner stays textual (spec 87 criterion 2's own doc comment: "it never
/// evaluates a `#[cfg(...)]` predicate at all - only `cfg(test)`/`#[test]` are given any
/// semantic meaning"), so this widens that ONE recognized shape by exactly one conservative
/// step rather than growing into a general boolean-cfg evaluator: `cfg(all(test, ANYTHING))` -
/// bare `test` as one of `all`'s own top-level, comma-separated clauses - is a logical SUBSET
/// of plain `cfg(test)` (every condition under which it compiles also satisfies `cfg(test)`),
/// so treating it as test-in-full can never misclassify a real production fn as test. This is
/// spec 93 criterion 1's own fix for `dead-code-json-out-of-line-classifier-missed-compound-
/// cfg-all-test`: widening a store-only whole-module gate from a bare `#[cfg(test)]` to
/// `#[cfg(all(test, any(feature = "store", not(feature = "core"))))]` (blast_radius_eval.rs,
/// eventstore/mod.rs's `pub mod contract;`) made this scanner's literal `contains("cfg(test)")`
/// check miss the module entirely, surfacing its already-test-only functions as spurious
/// production dead-code candidates - the exact misclassification
/// `no_committed_candidate_comes_from_a_known_out_of_line_test_file` exists to catch. Anything
/// OTHER than a bare `test` token at top level inside the `all(...)` - `not(test)`, `test` only
/// nested inside a further `any(...)`/`all(...)`, or no `all(` at all - is deliberately left
/// unrecognized rather than guessed at, the same "disclosed limitation over silent guessing"
/// discipline the rest of this textual scanner already follows.
fn cfg_all_contains_bare_test(attr_text: &str) -> bool {
    let Some(all_start) = attr_text.find("all(") else {
        return false;
    };
    // Require the `all(` to be reached from a `cfg(` (skipping only whitespace in between) so
    // `#[foo(all(test, x))]` on some unrelated attribute is never mistaken for a cfg.
    let before = attr_text[..all_start].trim_end();
    if !before.ends_with("cfg(") {
        return false;
    }
    let inner_start = all_start + "all(".len();
    let chars: Vec<char> = attr_text.chars().collect();
    let byte_to_char = |byte_idx: usize| attr_text[..byte_idx].chars().count();
    let start_ci = byte_to_char(inner_start);
    let mut depth = 1i32;
    let mut ci = start_ci;
    let mut clause_start = start_ci;
    while ci < chars.len() && depth > 0 {
        match chars[ci] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let clause: String = chars[clause_start..ci].iter().collect();
                    if clause.trim() == "test" {
                        return true;
                    }
                }
            }
            ',' if depth == 1 => {
                let clause: String = chars[clause_start..ci].iter().collect();
                if clause.trim() == "test" {
                    return true;
                }
                clause_start = ci + 1;
            }
            _ => {}
        }
        ci += 1;
    }
    false
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

/// `crates/rigger-conductor/src/conductor.rs`'s free-function rule table, derived from the concern keywords that
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

/// `crates/rigger-dash/src/dash.rs`'s free-function rule table - dash.rs is the read-only observability adapter,
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
        "crates/rigger-conductor/src/conductor.rs" => CONDUCTOR_RULES,
        "src/cli/mod.rs" => MAIN_RULES,
        "crates/rigger-dash/src/dash.rs" => DASH_RULES,
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
        "crates/rigger-conductor/src/conductor.rs" => "conductor",
        "src/cli/mod.rs" => "main",
        "crates/rigger-dash/src/dash.rs" => "dash",
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

/// Deterministic pretty JSON for one committed audit ledger: `items` projected through
/// `to_wire` - a LINE-FREE wire shape (spec 90 criterion 2: [`MapEntryWire`],
/// [`DupClusterWire`], [`DeadCodeCandidateWire`]) for a drift-guarded file, or the line-span
/// shape ([`MapEntryLines`], [`DupClusterLines`], [`DeadCodeCandidateLines`]) for its
/// unguarded `.lines` sibling, which is never drift-guarded and is written fresh on every
/// `RIGGER_AUDIT_WRITE=1` run. A bare array, each field in declaration order, with a trailing
/// newline: no `HashMap` anywhere in any of these shapes, so `serde_json` emits the SAME bytes
/// on every run over the same tree - the drift guard's whole premise.
fn ledger_json<T, W: Serialize>(items: &[T], to_wire: impl Fn(&T) -> W) -> String {
    let wire: Vec<W> = items.iter().map(to_wire).collect();
    let mut s = serde_json::to_string_pretty(&wire).expect("a ledger wire shape serializes");
    s.push('\n');
    s
}

// =========================================================================================
// SECTION 1 RENDERING
// =========================================================================================

/// Render section 1 of the report: a proposed module tree (each module heading, its functions
/// sorted as scanned, one line per function with its span and reason) followed by the
/// unassigned functions named individually (never omitted).
fn pluralize(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
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
        "Every function in `crates/rigger-conductor/src/conductor.rs`, `src/cli/mod.rs` and `crates/rigger-dash/src/dash.rs` \
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
        let _ = writeln!(out, "- `{module}` ({})", pluralize(fns.len(), "function"));
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
        pluralize(unassigned.len(), "function")
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

// =========================================================================================
// CRITERION 2 (`u85c2`, THIS UNIT): THE DUPLICATION CATALOG
// =========================================================================================

/// The two directories the duplication catalog scans, recursively, in this fixed order - the
/// strict whole-tree definition (spec 85 Goal: "DRY is STRICT - any logic present in more than
/// one place ANYWHERE in the codebase is a violation"), unlike criterion 1's 3-file scope.
const SCAN_ROOTS: [&str; 2] = ["src", "tests"];

/// The member crates the workspace split carved out of the root package's `src/`: the
/// duplication catalog keeps scanning their `src`/`tests` after [`SCAN_ROOTS`], exactly as it
/// scanned that code before the move.
const SPLIT_CRATES: [&str; 12] = [
    "crates/rigger-domain",
    "crates/rigger-store-sqlite",
    "crates/rigger-graph-sqlite",
    "crates/rigger-process",
    "crates/rigger-worktree-git",
    "crates/rigger-gates-shell",
    "crates/rigger-driver",
    "crates/rigger-grounder",
    "crates/rigger-config-files",
    "crates/rigger-conductor",
    "crates/rigger-console",
    "crates/rigger-dash",
];

/// Shingle window width (spec 85 Design: "Jaccard over 8-token shingles").
const SHINGLE_SIZE: usize = 8;

/// The Jaccard floor for a mechanical `near`/`exact` cluster (spec 85 Design: "normalized
/// similarity is... at or above 0.72").
const SIMILARITY_THRESHOLD: f64 = 0.72;

/// The PREFIX-FILTER length of a shingle set of `n` fingerprints: every set whose Jaccard with
/// it reaches [`SIMILARITY_THRESHOLD`] shares at least one of its first this-many fingerprints,
/// taken in any one global order. Jaccard >= t forces an overlap of at least `ceil(t * n)`, so the sets cannot
/// both miss a prefix of `n - ceil(t * n) + 1`; the overlap is rounded DOWN by a hair so float
/// error can only lengthen the prefix (more candidates), never drop a true pair.
fn prefix_len(n: usize) -> usize {
    let min_overlap = ((SIMILARITY_THRESHOLD * n as f64) - 1e-9).ceil().max(0.0) as usize;
    n - min_overlap.min(n) + 1
}

const CATALOG_PATH: &str = "docs/audit/duplication-catalog.json";

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`CATALOG_PATH`]'s line spans, moved out
/// of the guarded file so a pin bump or a sibling unit's own insertion elsewhere never perturbs
/// this file's guarded bytes. Written only in `RIGGER_AUDIT_WRITE=1` mode; the drift guard never
/// reads it back (see [`ledger_json`]).
const CATALOG_LINES_PATH: &str = "docs/audit/duplication-catalog.lines.json";

/// The committed sidecar of human judgements on catalogued clusters: a list of
/// [`Disposition`]s, each naming a cluster by its content-derived id or a mandatory sweep by its
/// name. The catalog writer copies each one onto its cluster's entry ([`apply_dispositions`]).
const DISPOSITIONS_PATH: &str = "docs/audit/duplication-dispositions.json";

/// The one disposition the audit records today: the detector matched two sites by their
/// normalized token shape, but reading them shows different meaning, not one logic twice.
const NOT_A_DUPLICATE: &str = "not-a-duplicate";

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

/// The date of the latest hand reading pass over the adversarial draw. The draw is seeded on
/// population size, so any change to the scanned function count reshuffles it; the rows the
/// pass read carry their verdicts in [`ADVERSARIAL_SAMPLE_VERDICTS`], and a row drawn since then
/// renders as NOT READ until the next pass records it.
const ADVERSARIAL_SAMPLE_READ_ON: &str = "2026-09-27";

/// One hand verdict on a drawn function, recorded once by the reading pass and rendered on its
/// row: reading is the one thing no generator can do (spec 85 THOROUGHNESS), so the verdict is
/// data the pass writes, never a label the renderer assumes.
enum SampleVerdict {
    /// Read by hand with its host file and siblings: no duplicate beyond what the catalog caught.
    NoDuplicate,
    /// Read by hand: a duplicate was found and closed; the text names it and its one home.
    Closed(&'static str),
}

/// The latest reading pass's verdict per drawn function, keyed `(file, name)`.
const ADVERSARIAL_SAMPLE_VERDICTS: &[(&str, &str, SampleVerdict)] = &[
    (
        "crates/rigger-domain/src/canary.rs",
        "from_event",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-conductor/src/canary_store.rs",
        "any_finding_is_critical",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-conductor/src/canary_store.rs",
        "score_item_reports_no_resolved_model_for_a_tier_whose_driver_leaves_it_empty",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-conductor/src/conductor.rs",
        "land_refused",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-conductor/src/conductor.rs",
        "speculation_escalates_when_every_candidate_is_rejected",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-conductor/src/conductor.rs",
        "replay_step",
        SampleVerdict::Closed(
        "three stepwise budget and attention tests re-rolled its body (and `started_store`'s) as inline closures; all now call `replay_step`",
    ),
    ),
    (
        "crates/rigger-domain/src/contextgraph.rs",
        "a_caller_less_reference_event_serializes_byte_identically_to_the_pre37_wire_form",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-dash/src/dash.rs",
        "get_static",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-dash/src/dash.rs",
        "export_neutralizes_a_script_breakout_in_the_inlined_state",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-driver/src/driver/cli.rs",
        "bridge_emits_propagates_the_first_emit_error",
        SampleVerdict::NoDuplicate,
    ),
    (
        "src/cli/hygiene.rs",
        "runs_menu_line",
        SampleVerdict::NoDuplicate,
    ),
    (
        "src/cli/run.rs",
        "reclaim_spawn_scratch",
        SampleVerdict::NoDuplicate,
    ),
    (
        "src/cli/validate.rs",
        "workflow_drift_advisory",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-domain/src/metrics.rs",
        "artifact_verdict",
        SampleVerdict::NoDuplicate,
    ),
    (
        "crates/rigger-process/src/subprocess.rs",
        "detach_process_group",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/cli.rs",
        "workflow_accepts_a_spec_and_a_base_flag",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/cli.rs",
        "validate_reports_budget_but_no_cache_dir_when_the_wrapper_is_off",
        SampleVerdict::Closed(
        "it re-rolled `assert_validate_reports` inline; it now calls it, which returns the stdout for its extra no-cache-dir check",
    ),
    ),
    (
        "tests/cli.rs",
        "setup_precommit_hook_prefers_a_unit_derived_binary_in_a_real_linked_worktree_over_a_stale_path_rigger",
        SampleVerdict::Closed(
        "it re-rolled `fresh_committed_skill`, `committed_skill` and `commit_a_code_change` inline; it now calls them, with the commit half split out as `commit_staged`",
    ),
    ),
    (
        "tests/cli.rs",
        "guard_write_under_a_root",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/cli.rs",
        "guard_write_exits_the_blocking_code_on_every_transport_failure",
        SampleVerdict::Closed(
        "it and `guard_write_without_a_root_fails_loudly_rather_than_allowing_everything` re-rolled `run_hook_verb`'s piped-stdin spawn; all three now call `pipe_into_rigger`, the blocking checks through `assert_blocks`",
    ),
    ),
    (
        "tests/common/audit_record.rs",
        "read_audit_record",
        SampleVerdict::Closed(
        "`tests/gitsemver_path_inclusion_accounting_periphery.rs` re-rolled it to read the stage1 record; it now includes and calls it",
    ),
    ),
    (
        "tests/compiler_pass_stage1_audit.rs",
        "stage1_record_has_the_shape_every_consumer_relies_on",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/concepts_labels_membership.rs",
        "label_of_the_documentless_hub",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/reset_derived_compaction_periphery.rs",
        "a_prune_with_nothing_to_reclaim_leaves_the_file_unrewritten",
        SampleVerdict::Closed(
        "`the_rewrite_flag_follows_the_file_and_not_this_passs_delete_count` repeated its settled-file fixture and skipped-rewrite assertions; both now call `settled_clean_store` and `assert_prune_skips_the_rewrite`",
    ),
    ),
    (
        "tests/simplification_audit.rs",
        "a_bare_test_attribute_on_a_free_function_marks_it_test_without_a_cfg_test_mod",
        SampleVerdict::Closed(
        "it and five sibling scanner tests re-rolled `scan_single`; all now call it or its name and span assertions",
    ),
    ),
    (
        "tests/spawn_recorded_lenient_periphery.rs",
        "recorded_lenient_collapses_a_re_parked_duplicate_id_to_the_last_recorded_request",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/step_attention_periphery.rs",
        "hung_cursor_functions_are_a_working_public_contract_across_the_crate_boundary",
        SampleVerdict::NoDuplicate,
    ),
    (
        "tests/worker_persona_label_periphery.rs",
        "the_subject_is_the_titles_first_sentence_passed_whole_with_no_truncation",
        SampleVerdict::Closed(
        "it re-rolled `assert_worker_label` inline; it now calls it",
    ),
    ),
];

/// Duplicates the 2026-09-27 reading passes found and closed on functions an earlier draw picked:
/// the draw was then by population index, so each closure reshuffled it until the draw became
/// rank-stable ([`sample_indices`]). Each names the drawn function and its closure, so the finding
/// count stays whole.
const ADVERSARIAL_SAMPLE_CLOSED_BEFORE_REDRAW: &[&str] = &[
    "`tests/kurrentdb_always_available.rs` \
     `architecture_blueprint_has_no_retired_kurrentdb_feature_reference` re-rolled the \
     manifest-relative read `common::repo::repo_text` owns, as did thirteen sibling reads across \
     eleven suites; every one now calls `repo_text`",
    "`tests/replan_episode_identity.rs` `serving` had two inline copies of its scan in its own \
     file; both now call it",
    "`crates/rigger-conductor/src/conductor.rs` `integrate_and_emit` repeated `catch_up_owed_regeneration`'s \
     regenerate/record/clear sequence twice; all three now call `regenerate_and_record`",
    "`tests/dash_run_tree_spine.rs` \
     `an_off_linear_unit_with_no_gate_verdict_renders_no_phantom_gates_passed` repeated \
     `a_crashed_implementer_renders_no_gates_node`'s crash-and-no-Gates assertions; both now call \
     `assert_crash_at_implement_and_no_gates`",
    "`src/main.rs` `dash_read_liveness` repeated `liveness_ages_for_wave`'s marker-age loop, as \
     did `rigger_activity` in `crates/rigger-dash/src/mcpserver.rs`; all three now call `liveness::marker_ages`",
    "`src/main.rs` `merge_hung_attention_defers_to_an_existing_budget_halt` was a value-only \
     copy of `merge_hung_attention_does_nothing_when_not_newly_hung`; both are now cases of \
     `assert_merge_hung_attention_leaves_untouched`",
    "`tests/code_entity_test_exclusion_periphery.rs` \
     `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none` \
     repeated its round-6 and round-7 siblings' legacy fixture; all three now call \
     `legacy_module_def`",
    "`tests/product_binary_location.rs` \
     `no_suite_bakes_the_product_path_at_compile_time_except_the_one_authority` re-rolled the \
     tree walk `common::repo::for_each_rs_file` owns; it now calls it",
    "`tests/simplification_audit.rs` `two_renamed_identical_functions_form_one_exact_cluster` and \
     `build_sweep_clusters_always_returns_exactly_five_named_clusters` re-rolled `on_fixture`, as \
     did eleven sibling tests; all thirteen now call it",
    "`tests/stop_failure_hook_periphery.rs` `hook_refuses_naming` was repeated inline by \
     `hook_stop_failure_rejects_an_unrecognized_class`; it now calls it",
    "`crates/rigger-conductor/src/conductor.rs` `proposal_event` was re-rolled inline by \
     `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes`; it now calls it and \
     `harvest_seeded`",
    "`src/contextgraph/mod.rs` `is_false` re-implemented `std::ops::Not::not`, which the symbol \
     model already uses; every flag now skips through `Not::not`",
    "`src/eventstore/mod.rs` `one` was bypassed by the concurrent contract append's \
     `.last().expect(..)`; it now calls `Appended::one`",
    "`crates/rigger-store-sqlite/src/eventstore/sqlite.rs` `a_rerun_reclaims_the_space_a_failed_reclamation_left_behind` \
     carried its own copies of the periphery suite's `plant_free_pages` and `pragma_i64`; both \
     now live once in the shared store fixtures",
    "`crates/rigger-grounder/src/grounder/workflowdef.rs` `full_reviewers_of` repeated the head of \
     `ReviewPanel::agent_ids`; both now call `ReviewPanel::full_roster`, and every \
     adversary/adjudicator pair goes through `config::push_reviewers`",
    "`tests/common/fixtures/graph.rs` `summarized_node` was re-rolled as an inline closure by \
     `tests/rationale_overlay_data.rs`; it now calls it",
    "`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` `tier_default_matches_the_extracted_const` was repeated inline \
     by `tests/code_ingest_events.rs`; the one test now pins all three tier consts",
    "`src/worktree.rs` `remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside` \
     and `tests/reap_before_removal_periphery.rs` `reaps_before_removing` each re-rolled the \
     spawn/wait/teardown/assert reap proof, as did five siblings in `src/main.rs`, \
     `crates/rigger-process/src/reap.rs`, `src/worktree.rs` and the relocated-scratch periphery suite; all now call \
     `assert_teardown_reaps_what_is_rooted_inside`",
    "`tests/heartbeat_write_read_agree_periphery.rs` \
     `watch_once_suppresses_a_false_dead_driver_when_the_configured_workdir_resolves_from_the_owning_root_with_no_agents_fleet` \
     repeated its two siblings' configured-workdir fixture; all three now call \
     `marker_under_a_configured_workdir`",
];

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

/// Scan every `.rs` file under [`SCAN_ROOTS`] and then each [`SPLIT_CRATES`] member's
/// `src`/`tests`, deterministically ordered ([`collect_rs_files`] sorts within each root; `src`
/// is scanned before `tests`).
fn scan_tree(root: &Path) -> Vec<FileScan> {
    let mut dirs: Vec<String> = SCAN_ROOTS.map(String::from).to_vec();
    for member in SPLIT_CRATES {
        dirs.push(format!("{member}/src"));
        dirs.push(format!("{member}/tests"));
    }
    scan_dirs(root, &dirs)
}

/// The workspace's member crates: every directory under `crates/` that holds a `src/`, as a
/// repo-relative path (`crates/console-core`), sorted. The root package is the implicit first
/// member and is not listed.
fn workspace_member_dirs(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join("crates")) else {
        return Vec::new();
    };
    let mut members: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join("src").is_dir())
        .map(|e| format!("crates/{}", e.file_name().to_string_lossy()))
        .collect();
    members.sort();
    members
}

/// Every source directory of the whole workspace: the root package's `src`/`tests`, then each
/// member crate's own `src`/`tests` - the scope the dead-code sweep reads callers from.
fn workspace_scan_dirs(root: &Path) -> Vec<String> {
    let mut dirs: Vec<String> = SCAN_ROOTS.map(String::from).to_vec();
    for member in workspace_member_dirs(root) {
        dirs.push(format!("{member}/src"));
        dirs.push(format!("{member}/tests"));
    }
    dirs
}

/// `true` when `rel` is production source of some workspace crate: under the root package's
/// `src/` or a member crate's `crates/<name>/src/`.
fn is_production_source(rel: &str) -> bool {
    if rel.starts_with("src/") {
        return true;
    }
    let mut parts = rel.split('/');
    parts.next() == Some("crates") && parts.next().is_some() && parts.next() == Some("src")
}

/// [`scan_tree`] widened to [`workspace_scan_dirs`].
fn scan_workspace(root: &Path) -> Vec<FileScan> {
    scan_dirs(root, &workspace_scan_dirs(root))
}

/// Scan every `.rs` file under each of `dirs` (repo-relative), in the given order.
fn scan_dirs(root: &Path, dirs: &[String]) -> Vec<FileScan> {
    let mut paths = Vec::new();
    for top in dirs {
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
    /// A recorded human judgement on this cluster ([`DISPOSITIONS_PATH`]), `None` while open.
    disposition: Option<String>,
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
    // Phase A: union every function with the same normalized stream (O(n) - this is the entire
    // `exact` pass) and pick one representative per stream.
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

    // Phase B: an inverted index over each (distinct-stream) representative's PREFIX-FILTER
    // fingerprints ([`prefix_len`]), taken in one global order - rarest first, so a boilerplate
    // shingle sits at the back of every set and rarely reaches a prefix. Prefix filtering is
    // exact under ANY fixed global order: every pair at or above the threshold shares a prefix
    // fingerprint, so the candidate set is COMPLETE and a cluster's membership depends only on its
    // members' own similarity - never on how many unrelated functions share a common shingle (the
    // order only decides how many candidates are checked). A pair whose sizes alone rule out the
    // threshold is never a candidate. Candidates are verified by exact Jaccard and unioned when
    // >= SIMILARITY_THRESHOLD.
    let mut frequency: HashMap<u64, usize> = HashMap::new();
    for &fi in &representatives {
        for &sh in &shingle_sets[fi] {
            *frequency.entry(sh).or_default() += 1;
        }
    }
    let mut posting: HashMap<u64, Vec<usize>> = HashMap::new();
    for (ri, &fi) in representatives.iter().enumerate() {
        let mut ordered = shingle_sets[fi].clone();
        ordered.sort_unstable_by_key(|sh| (frequency[sh], *sh));
        ordered.truncate(prefix_len(ordered.len()));
        for sh in ordered {
            posting.entry(sh).or_default().push(ri);
        }
    }
    let size_of = |ri: usize| shingle_sets[representatives[ri]].len() as f64;
    let mut candidates: HashSet<(usize, usize)> = HashSet::new();
    for list in posting.values() {
        for a in 0..list.len() {
            for &b in &list[a + 1..] {
                let (x, y) = (list[a].min(b), list[a].max(b));
                let (small, large) = (size_of(x).min(size_of(y)), size_of(x).max(size_of(y)));
                if small >= SIMILARITY_THRESHOLD * large - 1e-9 {
                    candidates.insert((x, y));
                }
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
            disposition: None,
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

/// The semantic sweep collecting every `/proc/<pid>/stat` or `status` field reader
/// ([`find_proc_stat_or_status_readers`]).
const PROC_STAT_READERS_SWEEP: &str =
    "/proc/<pid>/stat or /proc/<pid>/status field-extraction functions";

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
        disposition: None,
    }
}

/// Whether `c` is the cluster [`sweep_cluster`] built for the sweep called `name` - the ONE
/// by-name lookup, shared by citations and sweep-named dispositions.
fn is_sweep_named(c: &DupCluster, name: &str) -> bool {
    c.note
        .strip_prefix("mandatory sweep: ")
        .and_then(|rest| rest.strip_prefix(name))
        .is_some_and(|rest| rest.starts_with(" - "))
}

/// The cluster [`sweep_cluster`] built for the sweep called `name`, found back by its note.
fn sweep_cluster_named<'a>(clusters: &'a [DupCluster], name: &str) -> Option<&'a DupCluster> {
    clusters.iter().find(|c| is_sweep_named(c, name))
}

/// The process-spawn port: the ONE production module that constructs a `Command`. Every other
/// production spawn routes through it, so the `Command::new` sweep can never reopen in `src/`.
const PROCESS_SPAWN_PORT: &str = "crates/rigger-process/src/subprocess.rs";

/// Every `Command::new` call site in production code outside [`PROCESS_SPAWN_PORT`]: a site in a
/// production source file, not in a wholly-test file (`whole_file_test`) and not inside a test
/// fn or test mod ([`test_line_ranges`]).
fn production_spawns_outside_the_port(
    files: &[FileScan],
    whole_file_test: &BTreeSet<String>,
) -> Vec<DupSite> {
    find_ident_path_call_sites(files, "Command", &["new"])
        .into_iter()
        .filter(|s| {
            let Some(f) = files.iter().find(|f| f.rel == s.file) else {
                return false;
            };
            s.file != PROCESS_SPAWN_PORT
                && is_production_source(&s.file)
                && !whole_file_test.contains(&s.file)
                && !test_line_ranges(f)
                    .iter()
                    .any(|&(a, b)| s.start_line >= a && s.start_line <= b)
        })
        .collect()
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
            "crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
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
/// own named example: "`crates/rigger-dash/src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s `/proc` pid scan, upheld at
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

/// This file's own bespoke source-text scanner (`scan_file`, the frame-stack scanner) and the
/// source audits' shared token-level lexer (`tests/common/source_audit.rs::tokenize`) alongside the
/// codebase's ONE canonical tree-sitter-based
/// extractor, `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract` (its own module doc calls it "the ONE
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
        let is_bespoke_lexer = (sf.file == "tests/simplification_audit.rs"
            && sf.name == "scan_file")
            || (sf.file == "tests/common/source_audit.rs" && sf.name == "tokenize");
        let is_canonical_extractor = sf.file
            == "crates/rigger-grounder/src/grounder/symbols/extract.rs"
            && sf.name == "extract";
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
            PROC_STAT_READERS_SWEEP,
            find_proc_stat_or_status_readers(files, refs),
            "crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split",
        ),
        sweep_cluster(
            "bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor",
            find_bespoke_lexer_vs_canonical_extractor(files, refs),
            "crates/rigger-grounder/src/grounder/symbols/extract.rs::extract as the ONE function that touches source \
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

/// What a [`Disposition`] names: a cluster by its content-derived id, or a mandatory sweep by
/// its name (a sweep's id re-hashes whenever any of its sites changes; its name does not).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum DispositionTarget {
    Cluster { id: String },
    Sweep { sweep: String },
}

impl DispositionTarget {
    fn matches(&self, c: &DupCluster) -> bool {
        match self {
            DispositionTarget::Cluster { id } => c.id == *id,
            DispositionTarget::Sweep { sweep } => is_sweep_named(c, sweep),
        }
    }
}

impl std::fmt::Display for DispositionTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DispositionTarget::Cluster { id } => write!(f, "{id}"),
            DispositionTarget::Sweep { sweep } => write!(f, "the mandatory sweep {sweep}"),
        }
    }
}

/// One [`DISPOSITIONS_PATH`] entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Disposition {
    #[serde(flatten)]
    target: DispositionTarget,
    disposition: String,
    reason: String,
}

/// The committed [`DISPOSITIONS_PATH`] under `root` - the ONE reader of that sidecar.
fn load_dispositions(root: &Path) -> Vec<Disposition> {
    let path = root.join(DISPOSITIONS_PATH);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{DISPOSITIONS_PATH} is missing or unreadable ({e})"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{DISPOSITIONS_PATH} is not a disposition list: {e}"))
}

/// Copies each disposition onto the cluster it names. A disposition naming no cluster is
/// stale - its cluster was closed or its content changed - and is refused rather than kept.
fn apply_dispositions(
    clusters: &mut [DupCluster],
    dispositions: &[Disposition],
) -> Result<(), String> {
    for d in dispositions {
        let mut named = clusters.iter_mut().filter(|c| d.target.matches(c));
        let cluster = named.next().ok_or_else(|| {
            format!(
                "{DISPOSITIONS_PATH} records {} for {}, which names no catalogued cluster",
                d.disposition, d.target
            )
        })?;
        if named.next().is_some() {
            return Err(format!(
                "{DISPOSITIONS_PATH} records {} for {}, which names more than one cluster - \
                 disposition each by its id",
                d.disposition, d.target
            ));
        }
        cluster.disposition = Some(d.disposition.clone());
    }
    Ok(())
}

/// A cluster's id, derived from its OWN content - its classification, its proposed home and
/// its sorted `file#content_hash` site keys, hashed and rendered `dup-<12 hex>` - never from its
/// position in the catalog, so closing one cluster never renumbers another and a citation of
/// an id stays valid until that cluster itself changes. Line numbers never enter the key, so a
/// pure line shift keeps the id too.
fn cluster_id(c: &DupCluster) -> String {
    let mut site_keys: Vec<String> = c
        .sites
        .iter()
        .map(|s| format!("{}#{}", s.file, s.content_hash))
        .collect();
    site_keys.sort_unstable();
    let key = format!(
        "{}\n{}\n{}",
        c.classification,
        c.proposed_home,
        site_keys.join("\n")
    );
    format!("dup-{}", &content_hash(&key)[..12])
}

/// Build the full duplication catalog from an already-scanned tree: the mechanical (Jaccard)
/// clusters, the five mandatory sweeps, and the additional hand-found semantic clusters,
/// combined into ONE deterministically ordered list, each carrying its content-derived
/// [`cluster_id`]. Panics on an id collision rather than writing an ambiguous catalog. Takes
/// `files` (not a root path) so [`real_catalog`] can share [`real_files`]'s ONE scan of the
/// real tree instead of a second unrelated one.
fn build_catalog(files: &[FileScan]) -> Vec<DupCluster> {
    let refs = all_fn_refs(files);
    let mut clusters = build_mechanical_clusters(files, &refs);
    clusters.extend(build_sweep_clusters(files, &refs));
    clusters.extend(build_extra_semantic_clusters(files, &refs));
    clusters.sort_by_key(cluster_sort_key);
    let mut seen = HashSet::new();
    for c in clusters.iter_mut() {
        c.id = cluster_id(c);
        assert!(
            seen.insert(c.id.clone()),
            "two clusters derive the same id {}",
            c.id
        );
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

/// The real checked-out tree's [`build_catalog`] with its committed dispositions applied,
/// memoized alongside [`real_files`] for the same reason.
fn real_catalog() -> &'static [DupCluster] {
    static CACHE: std::sync::OnceLock<Vec<DupCluster>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut clusters = build_catalog(real_files());
        apply_dispositions(&mut clusters, &load_dispositions(&repo_root()))
            .unwrap_or_else(|e| panic!("{e}"));
        clusters
    })
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disposition: Option<String>,
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
        disposition: c.disposition.clone(),
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
        match sweep_cluster_named(clusters, name) {
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
/// live from `files`/`clusters`, so the listing can never drift from the tree; each uncaught
/// row's label and the reading-pass line are the hand verdicts [`ADVERSARIAL_SAMPLE_VERDICTS`]
/// records (spec 85: the recall check is proven by reading, which no generator can do), so a
/// row drawn after the last pass reads NOT READ rather than claiming a reading nobody did.
fn render_adversarial_sample(files: &[FileScan], clusters: &[DupCluster]) -> String {
    let mut out = String::new();
    let refs = adversarial_sample_population(files);
    let picked = sample_indices(
        &sample_keys(files, &refs),
        ADVERSARIAL_SAMPLE_SIZE,
        ADVERSARIAL_SEED,
    );
    let _ = writeln!(out, "### Adversarial sample");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Recall check (spec 85 THOROUGHNESS): {} functions drawn by seeded rank (seed \
         `{ADVERSARIAL_SEED}`, `sample_indices` over all {} functions scanned in `src/` and \
         `tests/`, each ranked by the seeded hash of its own file, name and ordinal so a change \
         elsewhere never reshuffles a drawn row, excluding `{ADVERSARIAL_SAMPLE_EXCLUDED_FILE}` - criterion 4's own citation-\
         guard periphery test, whose function count grows as its citation-drift-guard mechanism \
         hardens round over round; excluding it keeps that unrelated growth out of \
         the draw), each read by hand - together with its host file's \
         surrounding context, since a duplicate can live anywhere in the file or a sibling file - \
         to judge whether a duplicate exists that the mechanical pass and the five sweeps above \
         did not already catch.",
        picked.len(),
        refs.len(),
    );
    let _ = writeln!(out);
    let (mut read, mut closed) = (0, ADVERSARIAL_SAMPLE_CLOSED_BEFORE_REDRAW.len());
    for &i in &picked {
        let sf = refs[i].scanned(files);
        // Keyed on the site's position, not its name: a file can define several functions of
        // one name (trait-impl doubles), and only the one a cluster actually lists is caught.
        let hit = clusters.iter().find(|c| {
            c.sites
                .iter()
                .any(|s| s.file == sf.file && s.start_line == sf.start_line)
        });
        let verdict = ADVERSARIAL_SAMPLE_VERDICTS
            .iter()
            .find(|(file, name, _)| *file == sf.file && *name == sf.name)
            .map(|(_, _, v)| v);
        read += usize::from(verdict.is_some());
        let label = match (hit, verdict) {
            (Some(c), _) => format!("caught: `{}`", c.id),
            (None, Some(SampleVerdict::NoDuplicate)) => "no duplicate found by reading".into(),
            (None, Some(SampleVerdict::Closed(what))) => {
                closed += 1;
                format!("duplicate found by reading and closed: {what}")
            }
            (None, None) => format!(
                "NOT READ - drawn after the {ADVERSARIAL_SAMPLE_READ_ON} reading pass; read it \
                 and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`"
            ),
        };
        let _ = writeln!(
            out,
            "- `{}:{}-{}` `{}` - {label}",
            sf.file, sf.start_line, sf.end_line, sf.name
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Reading pass {ADVERSARIAL_SAMPLE_READ_ON}: {read} of the {} drawn functions read by hand \
         (a caught row too, to judge whether its duplicate reaches past the cluster); {closed} \
         duplicate(s) found by reading, each closed.",
        picked.len(),
    );
    for what in ADVERSARIAL_SAMPLE_CLOSED_BEFORE_REDRAW {
        let _ = writeln!(out, "- closed before the redraw: {what}");
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
         `{}` `process_state` next to `{}` `pid_starttime`, the \
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
         extractor, `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract` (its own module doc's claim, \
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
         (see this subsection's opening paragraph); that exclusion still applies unchanged.",
        cite_fn(real_files(), "crates/rigger-dash/src/dash.rs", "process_state"),
        cite_fn(
            real_files(),
            "crates/rigger-process/src/reap.rs",
            "pid_starttime"
        ),
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
// `u85c3-scope-and-instruments`, `u85c3-dead-code-clean-both-instruments`,
// `u85c3-test-suite-shape-from-committed-catalog`),
// citing `file:line` and naming its instrument per claim, exactly as sections 1 and 2 already
// do for their own mechanically-derived content. Section 3's citations and figures are
// computed at render time (see LIVE CITATIONS FOR THE HAND-WRITTEN PROSE); the static prose of
// sections 4 and 5 is built with `String::push_str`, which needs no `{{`/`}}` escaping for the
// literal braces of its `courier_registry_refresh_{boundary,fence,periphery}` file-glob prose.

// -----------------------------------------------------------------------------------------
// LIVE CITATIONS FOR THE HAND-WRITTEN PROSE
// -----------------------------------------------------------------------------------------
//
// The prose of sections 2 and 3 names real code. Every `file:line` it cites and every figure it
// states is computed here at render time from the checked-out tree, so a line shift or a
// changed count moves the report with the code instead of leaving a stale citation. A helper
// panics, naming what it looked for, when the thing the prose cites is gone - the prompt to
// re-cite or delete that sentence.

/// The lines of the repo file `rel`.
fn repo_lines(rel: &str) -> Vec<String> {
    fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("the report cites {rel}, which is unreadable ({e})"))
        .lines()
        .map(str::to_string)
        .collect()
}

/// The 1-based lines of `rel` containing `needle`.
fn lines_containing(rel: &str, needle: &str) -> Vec<usize> {
    repo_lines(rel)
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains(needle))
        .map(|(i, _)| i + 1)
        .collect()
}

/// The first line of `rel` containing `needle`.
fn line_of(rel: &str, needle: &str) -> usize {
    *lines_containing(rel, needle)
        .first()
        .unwrap_or_else(|| panic!("the report cites {needle:?} in {rel}, which no longer has it"))
}

/// The line of `rel`'s file-level `#[cfg(test)] mod tests`: where its production code ends.
fn test_boundary(rel: &str) -> usize {
    let lines = repo_lines(rel);
    lines
        .windows(2)
        .position(|w| w[0] == "#[cfg(test)]" && w[1].starts_with("mod tests"))
        .map(|i| i + 1)
        .unwrap_or_else(|| panic!("the report cites {rel}'s test module, which it no longer has"))
}

/// How many lines of `rel` contain `needle`, each checked to sit inside its test module - the
/// prose's claim that this reach is test-only.
fn test_only_hits(rel: &str, needle: &str) -> usize {
    let boundary = test_boundary(rel);
    let hits = lines_containing(rel, needle);
    let production: Vec<usize> = hits.iter().copied().filter(|&l| l < boundary).collect();
    assert!(
        production.is_empty(),
        "the report says every {needle:?} in {rel} is test-only, but lines {production:?} sit \
         above its test boundary ({boundary}) - re-cite that sentence"
    );
    hits.len()
}

/// The production lines of `rel` mentioning `needle`, each checked to be a comment - the
/// prose's claim that production only documents it.
fn production_comment_lines(rel: &str, needle: &str) -> Vec<usize> {
    let lines = repo_lines(rel);
    let boundary = test_boundary(rel);
    let hits: Vec<usize> = lines_containing(rel, needle)
        .into_iter()
        .filter(|&l| l < boundary)
        .collect();
    for &l in &hits {
        assert!(
            lines[l - 1].trim_start().starts_with("//"),
            "the report says {rel} only documents {needle:?} in production, but line {l} uses \
             it - re-cite that sentence"
        );
    }
    hits
}

/// `text` with every run of digits replaced by `#`: the prose a live citation leaves when its
/// line numbers and counts are set aside, so a drift guard compares the words, not the figures.
fn without_figures(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_digits = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(ch);
            in_digits = false;
        }
    }
    out
}

/// `rel:start-end` of the scanned function `name` in `rel`.
fn cite_fn(files: &[FileScan], rel: &str, name: &str) -> String {
    let f = files
        .iter()
        .find(|f| f.rel == rel)
        .and_then(|f| f.fns.iter().find(|s| s.name == name))
        .unwrap_or_else(|| panic!("the report cites `{name}` in {rel}, which no longer has it"));
    format!("{rel}:{}-{}", f.start_line, f.end_line)
}

/// `lines` joined as a comma-separated citation list.
fn joined(lines: &[usize]) -> String {
    lines
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Section 3, BOUNDARY VIOLATIONS: one real finding (`crates/rigger-grounder/src/ingest.rs`
/// reaching two concrete grounder modules, for a concern no port covers) plus the
/// checked-and-clean port-concretion sweeps and the use-cases-importing-infrastructure /
/// second-mutation-authority categories. Every citation and figure is computed from `files`
/// and the tree at render time.
fn render_section_3(files: &[FileScan]) -> String {
    const CONDUCTOR: &str = "crates/rigger-conductor/src/conductor.rs";
    const INGEST: &str = "crates/rigger-grounder/src/ingest.rs";
    const MAIN: &str = "src/cli/mod.rs";
    const AGENT_PORT: &str = "crates/rigger-domain/src/agent.rs";
    let boundary = test_boundary(CONDUCTOR);
    let agent_driver = line_of(AGENT_PORT, "pub trait AgentDriver");
    let walk = cite_fn(files, INGEST, "walk_batches");
    let ingest_batches = line_of(CONDUCTOR, "fn ingest_project_batches");
    let ingest_caller = line_of(CONDUCTOR, "self.ingest_project_batches()");
    let paced = line_of(INGEST, "symbols::events::project_batches_paced");
    let design = line_of(INGEST, "design::events::project_batches(");
    let grounder = line_of("crates/rigger-domain/src/grounder.rs", "pub trait Grounder");
    let mut out = String::new();
    out.push_str("## 3. Boundary Violations\n\n");
    out.push_str(
        "Instrument: for each of the five named ports (`eventstore::EventStore`, \
        `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, \
        `grounder::Grounder`), searched every production (pre-`#[cfg(test)]`) call \
        site of that port's known concrete adapter modules from a NON-adapter, \
        NON-composition-root file, and separately searched every domain-ish file's \
        top-level `use` statements for a direct infrastructure-crate import. \
        `src/main.rs` is exempt from the \"reaches a concrete adapter\" check: it is \
        the composition root, and wiring concretions together is its designed job. \
        Every citation below is resolved against the tree when the report is \
        rendered.\n\n",
    );
    out.push_str("FOUND, one violation:\n\n");
    let _ = write!(
        out,
        "Violation 1 (`Grounder`): `{walk}` (`walk_batches`, called from production \
        `conductor::RunCtx::ingest_project_batches` at `{CONDUCTOR}:{ingest_batches}`, \
        itself called from `{CONDUCTOR}:{ingest_caller}` above the `{boundary}` \
        `#[cfg(test)]` boundary) calls \
        `crate::grounder::symbols::events::project_batches_paced` directly by \
        concrete module path at line {paced} to reuse the `symbols` grounder's \
        already-persisted index for a one-time whole-project ingest walk, then at \
        line {design} - same function, same missing-port defect, not a separate third \
        violation - calls `crate::grounder::design::events::project_batches` \
        directly by concrete module path for the design-doc half of the same walk. \
        These two calls are two of the three named sites of section 2's own \
        catalogued duplicate cluster (`{PROJECT_BATCHES}`: `{}`, `{}`, and `{}` - all \
        three named `project_batches`), so this boundary violation and that \
        duplication finding are two symptoms of one root cause - `ingest.rs` naming \
        each concrete grounder submodule because no port exposes either. The \
        `Grounder` port (`crates/rigger-domain/src/grounder.rs:{grounder}`: `ground`, `reindex`, \
        `blast_radius`, `index_stamp`) serves real-time per-query grounding of an \
        agent's prompt; none of its methods exposes \"hand me every indexed file's \
        projected events for a whole-project batch ingest,\" so `ingest.rs` - itself \
        a domain ingest authority, not an adapter and not the composition root - has \
        no port to depend on for either call and reaches the concrete `symbols` \
        module ({paced}) and the concrete `design` module ({design}) directly. Fix \
        direction for a follow-up \
        spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` \
        or a standalone `SymbolProjector` trait) covering both concrete modules, so \
        `ingest.rs` depends on one abstraction instead of either concrete grounder \
        module for its whole-project walk.\n\n",
        cite_fn(
            files,
            "crates/rigger-grounder/src/grounder/design/events.rs",
            "project_batches"
        ),
        cite_fn(
            files,
            "crates/rigger-grounder/src/grounder/symbols/events.rs",
            "project_batches"
        ),
        cite_fn(
            files,
            "crates/rigger-grounder/src/grounder/workflowdef.rs",
            "project_batches"
        ),
    );
    let _ = write!(
        out,
        "Also reaching `grounder::symbols::store::content_hash` from \
        `{INGEST}:{}` and `crates/rigger-conductor/src/canary_store.rs:{}`: DISPOSITIONED as legitimate \
        shared-primitive reuse, not a third violation. `content_hash` (`{}`) is \
        documented at its own definition as the content-identity primitive the \
        `symbols` grounder's reindex freshening gate keys on, and `canary_store.rs`'s \
        own doc comment (`crates/rigger-conductor/src/canary_store.rs:{}`) reuses it by deliberate author \
        intent rather than growing another open-coded FNV-1a copy - a generic hashing \
        utility that happens to live in the `symbols` module, not a grounding \
        operation reached through the port. The broader duplication this primitive \
        is meant to fix (the open-coded FNV-1a copies elsewhere in the crate, per \
        `crates/rigger-domain/src/community.rs:{}`'s own comment) is a separately tracked cross-cutting \
        refactor (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's \
        concern.\n\n",
        line_of(INGEST, "store::content_hash("),
        line_of("crates/rigger-conductor/src/canary_store.rs", "store::content_hash("),
        cite_fn(files, "crates/rigger-grounder/src/grounder/symbols/store.rs", "content_hash"),
        line_of("crates/rigger-conductor/src/canary_store.rs", "parallel"),
        line_of(
            "crates/rigger-domain/src/community.rs",
            "arch-u2i-fnv1a-fourth-parallel-copy",
        ),
    );
    out.push_str(
        "CHECKED AND CLEAN (four of five ports fully clean; the fifth, `Grounder`, is \
        this section's violation above - each search recorded so a clean result is not \
        merely assumed):\n",
    );
    let _ = writeln!(
        out,
        "- `conductor::AgentDriver` concretion reach (`crate::driver::*`): production \
        `conductor.rs` depends only on the port (`{AGENT_PORT}:{agent_driver}`), held as \
        `dyn AgentDriver` (`{CONDUCTOR}:{}`); every one of its {} `crate::driver::` hits \
        sits inside `#[cfg(test)] mod tests`, where the tests construct a concrete driver \
        directly.",
        line_of(CONDUCTOR, "dyn AgentDriver"),
        test_only_hits(CONDUCTOR, "crate::driver::"),
    );
    let main_boundary = test_boundary(MAIN);
    let raw_open = line_of(MAIN, "rusqlite::Connection::open(");
    assert!(
        raw_open > main_boundary,
        "the report says {MAIN}'s raw connection is test-only, but line {raw_open} is above \
         its test boundary ({main_boundary})"
    );
    let _ = writeln!(
        out,
        "- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` \
        outside the SQLite adapters and `crates/rigger-store-sqlite/src/sqlite.rs`, the one opener every store \
        connection goes through): in production, only doc-comment mentions \
        (`{MAIN}:{}`); the one call is a deliberate, explicitly-commented test-only \
        raw-connection bypass (`{MAIN}:{raw_open}`, inside `#[cfg(test)] mod tests` \
        opened at `{MAIN}:{main_boundary}`) that reproduces a pre-append-guard \
        corruption shape `Store::append` itself refuses to construct - a documented \
        test technique, not a boundary violation.",
        joined(&production_comment_lines(MAIN, "Connection::open")),
    );
    let projection_importers = [
        "crates/rigger-graph-sqlite/src/concepts.rs",
        "crates/rigger-dash/src/dash.rs",
        "crates/rigger-grounder/src/grounder/symbols/events.rs",
        "crates/rigger-grounder/src/grounder/design/events.rs",
    ];
    for rel in projection_importers {
        test_only_hits(rel, "contextgraph::sqlite::Projector");
    }
    let _ = writeln!(
        out,
        "- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): \
        checked whole-tree, not only `{CONDUCTOR}` - every one of `conductor.rs`'s {} \
        hits sits inside `#[cfg(test)] mod tests` (production `conductor.rs` only \
        ever depends on `dyn Projection`), and the same is true wherever else \
        `contextgraph::sqlite::Projector` is imported (`{}` - every import sits \
        after that file's own `#[cfg(test)]` boundary).",
        test_only_hits(CONDUCTOR, "contextgraph::sqlite"),
        projection_importers.join("`, `"),
    );
    let mut runner_docs = production_comment_lines(CONDUCTOR, "ExecRunner");
    runner_docs.extend(production_comment_lines(CONDUCTOR, "RecordingRunner"));
    runner_docs.sort_unstable();
    runner_docs.dedup();
    let _ = writeln!(
        out,
        "- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): \
        checked whole-tree, not only `{CONDUCTOR}`. In `conductor.rs`, production \
        depends only on `dyn gate::Runner` (`{CONDUCTOR}:{}`); every production \
        mention of a concrete runner is a doc comment (`{CONDUCTOR}:{}`), and the \
        import of `ExecRunner` (`{CONDUCTOR}:{}`) and every one of its {} uses sit \
        inside `#[cfg(test)] mod tests`. In `crates/rigger-driver/src/driver/replay.rs`, all {} \
        `ExecRunner` mentions sit inside that file's own `#[cfg(test)] mod tests` \
        too.",
        line_of(CONDUCTOR, "dyn gate::Runner"),
        joined(&runner_docs),
        line_of(CONDUCTOR, "use crate::gate::ExecRunner"),
        lines_containing(CONDUCTOR, "ExecRunner")
            .iter()
            .filter(|&&l| l > boundary)
            .count(),
        test_only_hits("crates/rigger-driver/src/driver/replay.rs", "ExecRunner"),
    );
    let use_case_files = [
        CONDUCTOR,
        "crates/rigger-domain/src/blocker.rs",
        "crates/rigger-domain/src/spec.rs",
        "crates/rigger-domain/src/watch.rs",
        "crates/rigger-domain/src/community.rs",
    ];
    let infra = ["rusqlite", "reqwest", "tonic", "tokio", "kurrentdb"];
    for rel in use_case_files {
        for krate in infra {
            assert!(
                lines_containing(rel, &format!("use {krate}")).is_empty(),
                "the report says {rel} imports no {krate}, but it does - re-cite section 3"
            );
        }
    }
    let _ = writeln!(
        out,
        "- Use cases importing infrastructure: searched the `use` statements of every \
        domain-ish file this audit's own code neighborhood names (`{}`) for `{}` - \
        zero hits anywhere. Empty category.\n",
        use_case_files.join("`, `"),
        infra.join("`, `"),
    );
    let _ = writeln!(
        out,
        "A second mutation authority for one domain: the one previously-known \
        instance in this codebase (`crates/rigger-dash/src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s \
        `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) \
        is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it \
        is section 2's finding (`u85c2-proc-stat-worked-example`, \
        `find_proc_stat_or_status_readers`), not re-counted here to avoid \
        double-charging one defect to two sections. Checked process spawning as the \
        one other plausible second-authority candidate: production code constructs \
        every process through the one process-spawn port (`{PROCESS_SPAWN_PORT}`), so \
        production `conductor.rs` never builds a git command of its own. \
        `crates/rigger-worktree-git/src/worktree.rs` is the sole git-worktree-mutation \
        authority OUTSIDE the \
        composition root. Inside it, `{MAIN}` (exempt from the port-concretion-reach \
        check above, not from this one) holds two more git-worktree-mutation sites: \
        `reap_then_remove_worktree` (`{}`), the sanctioned worktree half of the \
        spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a \
        deliberate design choice, not a gap; and `materialize_config_at_rev` (`{}`), \
        a real, already-known, non-blocking gap \
        (`arch-u13-config-checkout-bypasses-worktree-authority` / \
        `arch-u2r-config-checkout-shells-git` / \
        `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the \
        `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so \
        this is a gap in that authority rather than a competing abstraction). No \
        second mutation authority found beyond the already-cited, \
        already-catalogued `/proc` case and this already-dispositioned \
        `materialize_config_at_rev` gap.",
        cite_fn(files, MAIN, "reap_then_remove_worktree"),
        cite_fn(files, MAIN, "materialize_config_at_rev"),
    );
    out
}

/// Section 4.3's per-file distribution table, rendered from `candidates` (file order, already
/// `(file, line)`-sorted) so it can never drift from the committed JSON.
fn render_dead_code_distribution_table(candidates: &[DeadCodeCandidate]) -> String {
    let mut out = String::new();
    out.push_str("| File | Count |\n|---|---|\n");
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

/// Section 4.3's full list, rendered from `candidates` grouped by file - name, line, visibility
/// and ambiguity. `lines` carries each candidate's own line data in the SAME order as
/// `candidates` ([`dead_code_candidate_lines`] - joined by array
/// position, exactly like [`DEAD_CODE_LINES_PATH`] itself), and every `file:line` citation
/// below, `ambiguous_with`'s own included, reads from `lines`, never from a
/// [`DeadCodeCandidate`]'s own `line`/`ambiguous_with`, mirroring [`render_section_2`]'s own
/// `lines` param.
fn render_dead_code_full_list(
    candidates: &[DeadCodeCandidate],
    lines: &[DeadCodeCandidateLines],
) -> String {
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
            "- **{}** (`{}:{}`, `{}`{})\n",
            c.name, cl.file, cl.line, c.visibility, amb,
        ));
    }
    out.push('\n');
    out
}

/// Section 6 item 0's own per-file deletion list: each entry's name AND its own `(line N)`
/// citation, rendered from `candidates` (every entry is to be deleted), paired with `lines`
/// ([`DeadCodeCandidateLines`], the SAME sidecar section 4.3 reads) so this list's own citation
/// is never a second, independent read of the live scan - spec 90 criterion 2, mirroring
/// [`render_dead_code_full_list`] exactly.
fn render_dead_code_deletion_list(
    candidates: &[DeadCodeCandidate],
    lines: &[DeadCodeCandidateLines],
) -> String {
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

/// Section 4, DEAD AND VESTIGIAL CODE. 4.0 is stage 1, the compiler-driven pass; 4.1 is the
/// workspace production-reference sweep; 4.2 states the live-or-deleted rule; 4.3 renders the
/// ledger from [`real_dead_code_candidates`]; 4.4 (retired-feature remnants, stale doc claims)
/// is carried forward verbatim.
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
    out.push_str("### 4.1 Stage 2: the workspace production-reference sweep\n\n");
    out.push_str(
        "A compiler lint never fires on a `pub` item of a crate that is both a library and a \
        binary, so stage 1 cannot see a `pub` function nothing calls. Stage 2 is a reference \
        sweep over the whole workspace instead: the root package's `src/` and every member \
        crate's `crates/<name>/src/`, since a member crate calling into the library is as much a \
        production caller as the library's own binary. Every function defined in that \
        production source is a candidate unless it is test code (a `#[cfg(test)]` span, or a \
        file reached only through a `#[cfg(test)] mod` declaration, transitively), an entry \
        point the language or a foreign caller invokes (a top-level `fn main`, an `extern \
        \"C\"` export), or a trait-impl method (the language calls `Drop`, formatting, \
        operator and iterator methods with no call site in the text).\n\n\
        THE RULE: a candidate is live when an identifier token equal to its name appears in \
        production code outside its own signature - any token, whatever surrounds it, so a \
        function passed by value, named in an attribute string or used as a path segment \
        counts exactly like a call. `tests/` directories and test spans never count. When \
        several candidates share a name, a reference is attributed to the one it names: a free \
        or associated function by its path qualifier, the referencing file's `use` import, or \
        (for a free function) a bare call in its own file; a method by its receiver's type - \
        the `T` of a `T::name` path, the enclosing impl's type for `self.name` or \
        `Self::name`, or the declared type of a local receiver (a typed parameter, a typed \
        `let`, a struct-literal `let`). A method call whose receiver type cannot be read from \
        the text (a field, a call result, a pattern binding) is credited to every method of \
        that name, so the sweep can miss dead code but never reports live code as dead. \
        Output: `docs/audit/dead-code.json` (line-free, drift-guarded like the duplication \
        catalog) and `docs/audit/dead-code.lines.json` (each entry's line numbers).\n\n",
    );
    out.push_str("### 4.2 The rule: live or deleted\n\n");
    out.push_str(
        "A function is either live or deleted; there is no third state. rigger's library has \
        no consumer outside this workspace - its binary, its integration tests and its member \
        crates all sit inside the sweep's scope - so a function with no production caller has \
        no public surface to protect and no reason to stay: it is deleted together with the \
        tests that exercise only it. Code a future change needs is added by that change, \
        together with its caller. The audit enforces the rule: it fails whenever the ledger \
        below holds any entry, and the failure names each one.\n\n",
    );
    out.push_str("### 4.3 The ledger\n\n");
    if real_dead_code_candidates().is_empty() {
        out.push_str("The ledger is empty.\n\n");
    } else {
        out.push_str(&render_dead_code_distribution_table(
            real_dead_code_candidates(),
        ));
        out.push_str(&render_dead_code_full_list(
            real_dead_code_candidates(),
            &real_dead_code_lines(),
        ));
    }
    out.push_str("### 4.4 Retired-feature remnants and stale doc claims\n\n");
    out.push_str(
        "RETIRED-FEATURE REMNANTS. `turbovec` (spec 57, \"Retire turbovec\"): grepped \
        the whole tree (`src/`, `tests/`, `docs/`, `specs/`, `Cargo.toml`) for every \
        mention - found only the deliberate migration-error guard code \
        (`crates/rigger-grounder/src/grounder/mod.rs`'s `is_retired_grounder` / the loud \
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

// -----------------------------------------------------------------------------------------
// CATALOG CITATIONS FOR SECTIONS 5 AND 6
// -----------------------------------------------------------------------------------------
//
// Sections 5 and 6 are hand-written prose, but every cluster id and every cluster-derived
// count in them is resolved from the real catalog at render time: a cited cluster is named by
// its content-derived id below (a mandatory sweep by its sweep name, since its site set moves
// with every call site anywhere), and its site/file counts are read off the cluster itself, so
// the prose can never cite a stale count. Closing or changing a cited cluster makes the render
// panic with the id, which is the prompt to re-cite that sentence.

const PROJECT_BATCHES: &str = "dup-f2eee0c78cd6";

/// The real catalog's cluster `id`, which the report's prose cites.
fn cited(id: &str) -> &'static DupCluster {
    real_catalog()
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| {
            panic!(
                "the report cites {id}, which {CATALOG_PATH} no longer carries - the cluster \
                 was closed or changed, so re-cite that sentence"
            )
        })
}

/// The real catalog's cluster for the sweep called `name`.
fn cited_sweep(name: &str) -> &'static DupCluster {
    sweep_cluster_named(real_catalog(), name)
        .unwrap_or_else(|| panic!("{CATALOG_PATH} carries no cluster for the sweep {name:?}"))
}

/// How many distinct files `c`'s sites span.
fn file_count(c: &DupCluster) -> usize {
    c.sites
        .iter()
        .map(|s| s.file.as_str())
        .collect::<HashSet<_>>()
        .len()
}

/// The real catalog's test-only clusters (every site under `tests/`), and the two shapes
/// sections 5.4 and 5.5 split them into: every site an ordinary helper, or every site a test
/// function.
struct TestOnlyClusters {
    all: Vec<&'static DupCluster>,
    helpers: Vec<&'static DupCluster>,
    tests: Vec<&'static DupCluster>,
}

fn test_only_clusters() -> TestOnlyClusters {
    let test_fns: HashSet<(&str, usize)> = real_files()
        .iter()
        .flat_map(|f| f.fns.iter())
        .filter(|f| f.is_test)
        .map(|f| (f.file.as_str(), f.start_line))
        .collect();
    let is_test_fn = |s: &DupSite| test_fns.contains(&(s.file.as_str(), s.start_line));
    let all: Vec<&'static DupCluster> = real_catalog()
        .iter()
        .filter(|c| c.sites.iter().all(|s| s.file.starts_with("tests/")))
        .collect();
    let helpers = all
        .iter()
        .copied()
        .filter(|c| !c.sites.iter().any(is_test_fn))
        .collect();
    let tests = all
        .iter()
        .copied()
        .filter(|c| c.sites.iter().all(is_test_fn))
        .collect();
    TestOnlyClusters {
        all,
        helpers,
        tests,
    }
}

/// `clusters` ordered widest first - by distinct files, then sites, then id - the order sections
/// 5.2 and 5.4 name the headline helper clusters in.
fn widest_first<'a>(clusters: &[&'a DupCluster]) -> Vec<&'a DupCluster> {
    let mut v = clusters.to_vec();
    v.sort_by(|a, b| {
        (file_count(b), b.sites.len(), &a.id).cmp(&(file_count(a), a.sites.len(), &b.id))
    });
    v
}

/// One bullet naming cluster `c`: its distinct function names, how widely it is redefined, its
/// id and classification.
fn cluster_bullet(c: &DupCluster) -> String {
    let names: BTreeSet<&str> = c.sites.iter().map(|s| s.name.as_str()).collect();
    let names: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    format!(
        "- {} - {} sites across {} files (`{}`, {}).\n",
        names.join(" / "),
        c.sites.len(),
        file_count(c),
        c.id,
        c.classification
    )
}

/// How many headline helper clusters section 5.2 names.
const HEADLINE_HELPER_CLUSTERS: usize = 4;
/// How many further helper clusters section 5.4 names beyond 5.2's headline ones.
const FURTHER_HELPER_CLUSTERS: usize = 6;

/// How many table-driven test families section 5.5 and item 15 name.
const HEADLINE_TEST_FAMILIES: usize = 2;

/// The largest still-open (undispositioned) all-`#[test]` clusters of `tests`, most sites
/// first, at most [`HEADLINE_TEST_FAMILIES`] of them - read off the catalog at render time, so
/// closing a family moves the headline to the next one instead of leaving a stale citation.
fn largest_open_test_families<'a>(tests: &[&'a DupCluster]) -> Vec<&'a DupCluster> {
    let mut open: Vec<&DupCluster> = tests
        .iter()
        .copied()
        .filter(|c| c.disposition.is_none())
        .collect();
    open.sort_by(|a, b| (b.sites.len(), &a.id).cmp(&(a.sites.len(), &b.id)));
    open.truncate(HEADLINE_TEST_FAMILIES);
    open
}

/// The bullets naming `families`, or the one line saying none remain open.
fn test_family_bullets(families: &[&DupCluster]) -> String {
    if families.is_empty() {
        return "- none: every all-`#[test]` cluster is closed or dispositioned.\n".to_string();
    }
    families.iter().map(|c| cluster_bullet(c)).collect()
}

/// How many of `ids` name a cluster in `clusters`.
fn count_cited_in(ids: &[&str], clusters: &[&DupCluster]) -> usize {
    ids.iter()
        .filter(|id| clusters.iter().any(|c| c.id == **id))
        .count()
}

/// How many catalog clusters span at least two distinct files and sit wholly inside `files` - a
/// file group's duplication confined to just itself, never a wider sweep that happens to
/// intersect it.
fn clusters_confined_to(files: &[&str]) -> usize {
    real_catalog()
        .iter()
        .filter(|c| file_count(c) >= 2 && c.sites.iter().all(|s| files.contains(&s.file.as_str())))
        .count()
}

/// How many of `clusters` have a site in every one of `files`.
fn clusters_including(clusters: &[&DupCluster], files: &[&str]) -> usize {
    clusters
        .iter()
        .filter(|c| files.iter().all(|f| c.sites.iter().any(|s| s.file == *f)))
        .count()
}

/// Section 5.1's subsystem table: each hand-derived subsystem, a fixed note, and the file groups
/// whose confined duplication the note counts from the catalog at render time.
const TEST_SUBSYSTEMS: &[(&str, &str, &[&[&str]])] = &[
    (
        "Dashboard: KG lenses & viz (code/concepts/community/files lenses, graph exploration, \
        overlays, viz layout)",
        "",
        &[],
    ),
    (
        "CLI whole-binary integration (`cli.rs`, `watchdog_cli_periphery.rs`, `ci_lanes.rs`)",
        "split plan at 5.3",
        &[],
    ),
    (
        "Knowledge-graph ingestion & context-graph projections",
        "dedup/fold/identity concerns",
        &[],
    ),
    (
        "Conductor orchestration: gates, courier, step/run lifecycle",
        "",
        &[&[
            "tests/courier_registry_refresh_boundary_periphery.rs",
            "tests/courier_registry_refresh_fence_periphery.rs",
            "tests/courier_registry_refresh_periphery.rs",
        ]],
    ),
    ("Reset / log compaction / store hygiene", "", &[]),
    ("Worktree & scratch lifecycle", "", &[]),
    (
        "Simplification-audit generator & its own periphery (this spec)",
        "",
        &[],
    ),
    ("Spec/handbook lint & architecture-doc integrity", "", &[]),
    (
        "Grounding (symbols grounder, turbovec retirement, blast radius)",
        "",
        &[&[
            "tests/kurrentdb_always_available.rs",
            "tests/turbovec_retired.rs",
        ]],
    ),
    ("Process lifecycle: no-os-kill & reap discipline", "", &[]),
    ("Event store & config precedence", "", &[]),
    (
        "Canary (review-panel judge-the-judges evaluation)",
        "",
        &[
            &[
                "tests/canary_false_positives_periphery.rs",
                "tests/canary_unattributed_rejects_periphery.rs",
            ],
            &[
                "tests/canary_item_sharding_jobs_cap_periphery.rs",
                "tests/canary_progress_hook_periphery.rs",
            ],
        ],
    ),
    (
        "Concepts/community lens derivation & fold (non-viz)",
        "",
        &[&[
            "tests/community_detection_cli.rs",
            "tests/concepts_derivation_cli.rs",
        ]],
    ),
    (
        "Residual (no natural larger home)",
        "`build_budget_slots_periphery.rs`, `gitsemver_derivation.rs` - named rather than forced \
        into an ill-fitting bucket",
        &[],
    ),
];

/// One section 5.1 row: the fixed note, then each file group's confined cluster count.
fn subsystem_row(name: &str, note: &str, groups: &[&[&str]]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !note.is_empty() {
        parts.push(note.to_string());
    }
    for group in groups {
        let names: Vec<String> = group
            .iter()
            .map(|f| format!("`{}`", f.trim_start_matches("tests/")))
            .collect();
        parts.push(format!(
            "{} share {} confined to just themselves",
            names.join(" + "),
            pluralize(clusters_confined_to(group), "duplication cluster")
        ));
    }
    format!("| {name} | {} |\n", parts.join("; "))
}

/// Section 5.3's keyword pass: each CLI verb group, matched as a test-name prefix.
const CLI_VERB_PREFIXES: &[&[&str]] = &[
    &["step_"],
    &["run_"],
    &["validate_"],
    &["reset_"],
    &["watch_", "watchdog_"],
    &["canary_"],
    &["dash_", "status_"],
    &["store_", "eventstore_"],
    &["spawn_", "scratch_"],
    &["review_", "gate_"],
    &["setup_", "precommit_", "hook_"],
    &["courier_", "registry_"],
    &["spec_"],
    &["replay_"],
    &["worktree_"],
    &["emit_", "peers_", "decision_"],
    &["stats_"],
    &["heartbeat_", "liveness_"],
    &["prime_", "version_", "init_"],
];

const CLI_SUITE: &str = "tests/cli.rs";

/// `tests/cli.rs`'s `#[test]` function names, from the real tree's scan.
fn cli_test_names() -> Vec<&'static str> {
    real_files()
        .iter()
        .filter(|f| f.rel == CLI_SUITE)
        .flat_map(|f| f.fns.iter())
        .filter(|f| f.is_test)
        .map(|f| f.name.as_str())
        .collect()
}

/// How many of `names` start with one of section 5.3's CLI verb prefixes.
fn verb_covered(names: &[&str]) -> usize {
    names
        .iter()
        .filter(|n| {
            CLI_VERB_PREFIXES
                .iter()
                .flat_map(|g| g.iter())
                .any(|p| n.starts_with(p))
        })
        .count()
}

/// `part` as a whole-number percentage of `whole` (0 when `whole` is 0).
fn percent(part: usize, whole: usize) -> usize {
    (part * 100).checked_div(whole).unwrap_or(0)
}

/// Section 5.3's cross-file test-only clusters with a site in `tests/cli.rs`.
fn cli_cross_file_clusters<'a>(test_only: &[&'a DupCluster]) -> Vec<&'a DupCluster> {
    test_only
        .iter()
        .copied()
        .filter(|c| file_count(c) >= 2 && c.sites.iter().any(|s| s.file == CLI_SUITE))
        .collect()
}

/// Section 5, TEST-SUITE SHAPE: a hand-derived 13-group-plus-residual subsystem breakdown of
/// the `tests/*.rs` files, a `tests/cli.rs` split plan, and shared-fixture /
/// table-driven-test consolidation candidates cross-referenced from the ALREADY-COMMITTED
/// `docs/audit/duplication-catalog.json` filtered to clusters whose every site sits under
/// `tests/` (decision `u85c3-test-suite-shape-from-committed-catalog`).
pub(crate) fn render_section_5() -> String {
    let test_only = test_only_clusters();
    let (test_only_n, helpers_n, tests_n) = (
        test_only.all.len(),
        test_only.helpers.len(),
        test_only.tests.len(),
    );
    let mut out = String::new();
    out.push_str("## 5. Test-Suite Shape\n\n");
    out.push_str(&format!(
        "Instrument: subsystem grouping is a hand-derived, ordered filename-keyword rule table \
        (mirrors criterion 1's own per-file classification convention: first-match-wins, \
        narrowest first, an explicit residual named rather than silently dropped). The \
        consolidation notes below cross-reference the ALREADY-COMMITTED \
        `docs/audit/duplication-catalog.json` (criterion 2's own generator output, not \
        re-scanned here) filtered to the {test_only_n} clusters whose every site sits under \
        `tests/`; every count in them is read from the catalog at render time.\n\n",
    ));
    out.push_str("### 5.1 Subsystem grouping and consolidation map\n\n");
    out.push_str("| Subsystem | Consolidation note |\n");
    out.push_str("|---|---|\n");
    for (name, note, groups) in TEST_SUBSYSTEMS {
        out.push_str(&subsystem_row(name, note, groups));
    }
    out.push('\n');
    out.push_str("### 5.2 Shared fixtures to extract into `tests/common`\n\n");
    let widest_helpers = widest_first(&test_only.helpers);
    let headline = &widest_helpers[..HEADLINE_HELPER_CLUSTERS.min(widest_helpers.len())];
    let headline_sites: usize = headline.iter().map(|c| c.sites.len()).sum();
    out.push_str(&format!(
        "`tests/common/mod.rs` and `tests/common/fixtures/` already hold the shared fixtures - \
        the gap is everything still duplicated OUTSIDE them. The catalog's all-helper-function \
        test-only clusters ({helpers_n} of the {test_only_n} test-only clusters) are the \
        evidence; the {} widest, by distinct files, are the headline case for extraction:\n\n",
        headline.len(),
    ));
    for c in headline {
        out.push_str(&cluster_bullet(c));
    }
    out.push_str(&format!(
        "\nProposed home for each: `tests/common` (the catalog's own `proposed_home` field \
        says so for each). Consolidating just these collapses roughly {headline_sites} \
        duplicate definitions into {} shared ones - the single largest mechanical \
        simplification this audit identifies anywhere in the test suite.\n\n",
        headline.len(),
    ));
    out.push_str("### 5.3 `tests/cli.rs` split plan\n\n");
    let cli_tests = cli_test_names();
    let cli_covered = verb_covered(&cli_tests);
    let cli_cross = cli_cross_file_clusters(&test_only.all);
    let verb_list: Vec<String> = CLI_VERB_PREFIXES
        .iter()
        .map(|g| {
            g.iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    out.push_str(&format!(
        "`tests/cli.rs` holds {cli_n} `#[test]` functions. Its existing internal section markers each name \
        the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - ad \
        hoc organization that falls well short of a deliberate, complete per-surface \
        structure. The split proposed below replaces those by-spec markers with a complete, \
        deliberate BY CLI SUBCOMMAND SURFACE organization. A keyword-on-test-name pass \
        (matching each test's dominant CLI verb: {verbs}) only cleanly covers {cli_covered} of \
        the {cli_n} tests ({cli_pct}%) - disclosed honestly rather than overclaimed, \
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
        `cli.rs` also participates in {cross_n} of the catalog's cross-file \
        test-only duplication clusters, several paired against files that WOULD merge with \
        it under this split (`tests/step_attention_periphery.rs`, paired in {step_n} \
        clusters; `tests/watchdog_cli_periphery.rs`, paired in {watch_n} clusters) - the \
        split is expected to shrink, not grow, the duplication surface.\n\n",
        cli_n = cli_tests.len(),
        verbs = verb_list.join(", "),
        cli_pct = percent(cli_covered, cli_tests.len()),
        cross_n = cli_cross.len(),
        step_n = clusters_including(&cli_cross, &["tests/step_attention_periphery.rs"]),
        watch_n = clusters_including(&cli_cross, &["tests/watchdog_cli_periphery.rs"]),
    ));
    out.push_str("### 5.4 Duplicated helpers across test files (beyond 5.2's headline cases)\n\n");
    let further: Vec<&DupCluster> = widest_helpers
        .iter()
        .skip(headline.len())
        .take(FURTHER_HELPER_CLUSTERS)
        .copied()
        .collect();
    out.push_str(&format!(
        "{helpers_n} test-only clusters in the committed catalog have every site as an \
        ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate \
        class. Beyond the {} in 5.2, the widest are:\n\n",
        headline.len(),
    ));
    for c in &further {
        out.push_str(&cluster_bullet(c));
    }
    out.push_str(&format!(
        "\nEvery one of these {helpers_n} clusters, with its full site list and the catalog's \
        own `proposed_home`, is already machine-readable in the committed \
        `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume \
        directly - not re-enumerated exhaustively here to keep this section a report, not a \
        second copy of the catalog.\n\n",
    ));
    out.push_str("### 5.5 Table-driven test families\n\n");
    out.push_str(&format!(
        "{tests_n} test-only clusters have every site as a `#[test]` function - a \
        literal-differs-only-in-input family, spec 85's own named table-driven-test \
        candidate class. The largest families this audit first found - `tests/spec_lint.rs`'s \
        feed-one-spec-through-`validate` defect tests and `tests/no_os_kill_audit.rs`'s \
        one-termination-pattern-per-test checks - are closed, as are the \
        `tests/reap_before_removal_audit.rs` exemption-coverage family, this generator's own \
        scanner tests and the no-os-kill test helper's pid-refusal tests: their cases run as \
        `test_cases!` rows over shared case helpers. The largest still open:\n\n{}\n\
        As with 5.4, the full {tests_n}-family list lives in the committed catalog by cluster \
        id for a follow-up test-consolidation spec to consume directly.\n",
        test_family_bullets(&largest_open_test_families(&test_only.tests)),
    ));
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
// like criterion 3, there is no new mechanical scanner here (`render_section_6` is
// hand-authored prose). Every duplication-cluster id and count in it is resolved from the
// catalog at render time (see CATALOG CITATIONS FOR SECTIONS 5 AND 6), and every god-file
// function count and span sum from the responsibility map at render time ([`god_file_shape`]) -
// adding no new findings.

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

/// The real checked-out tree's [`build_map`], memoized like [`real_files`].
fn real_map() -> &'static [MapEntry] {
    static CACHE: std::sync::OnceLock<Vec<MapEntry>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| build_map(&repo_root()))
}

/// A map entry's own function span in lines - line-shift-free, unlike its start line.
fn span_lines(e: &MapEntry) -> usize {
    e.end_line - e.start_line + 1
}

/// One proposed production module of a god file: its name, function count and summed span.
struct Bucket {
    module: String,
    fns: usize,
    lines: usize,
}

/// One god file's shape as section 1's map records it, the counts tiers 2 and 3 cite.
struct GodFileShape {
    fns: usize,
    test_fns: usize,
    test_lines: usize,
    /// The named test subgroups (`<stem>::tests::<mod>`), sorted.
    test_groups: Vec<String>,
    /// Production buckets, largest summed span first.
    buckets: Vec<Bucket>,
    unassigned_fns: usize,
    unassigned_lines: usize,
}

fn god_file_shape(file: &str) -> GodFileShape {
    let entries: Vec<&MapEntry> = real_map().iter().filter(|e| e.file == file).collect();
    let tests: Vec<&&MapEntry> = entries.iter().filter(|e| e.is_test).collect();
    let test_groups: BTreeSet<String> = tests
        .iter()
        .filter_map(|e| e.proposed_module.clone())
        .filter(|m| m.matches("::").count() > 1)
        .collect();
    let mut buckets: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let (mut unassigned_fns, mut unassigned_lines) = (0, 0);
    for e in entries.iter().filter(|e| !e.is_test) {
        match e.proposed_module.as_deref() {
            Some(m) => {
                let b = buckets.entry(m).or_default();
                b.0 += 1;
                b.1 += span_lines(e);
            }
            None => {
                unassigned_fns += 1;
                unassigned_lines += span_lines(e);
            }
        }
    }
    let mut buckets: Vec<Bucket> = buckets
        .into_iter()
        .map(|(m, (fns, lines))| Bucket {
            module: m.to_string(),
            fns,
            lines,
        })
        .collect();
    buckets.sort_by(|a, b| (b.lines, &a.module).cmp(&(a.lines, &b.module)));
    GodFileShape {
        fns: entries.len(),
        test_fns: tests.len(),
        test_lines: tests.iter().map(|e| span_lines(e)).sum(),
        test_groups: test_groups.into_iter().collect(),
        buckets,
        unassigned_fns,
        unassigned_lines,
    }
}

/// How many production buckets a tier-2 or tier-3 entry names as its headline.
const HEADLINE_BUCKETS: usize = 5;

/// The last path segment of each of `shape`'s first `n` buckets, backticked.
fn bucket_names(shape: &GodFileShape, n: usize) -> String {
    shape
        .buckets
        .iter()
        .take(n)
        .map(|b| format!("`{}`", b.module.rsplit("::").next().unwrap_or(&b.module)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A tier-2 entry's Scope, Files and line-delta bullets for `file`'s inline test module.
fn test_extraction_scope(file: &str, dest_dir: &str, partition: &str) -> String {
    let s = god_file_shape(file);
    format!(
        "- Scope: the file's `#[cfg(test)] mod tests` holds {} of its {} mapped functions \
        ({}%), roughly {} lines of test-function spans. {partition}\n\
        - Files: `{file}` -> `{file}` (production only) + `{dest_dir}/*.rs`.\n\
        - Expected line delta: 0 net (repo-wide) - roughly {} lines relocated out of \
        `{file}`.\n",
        s.test_fns,
        s.fns,
        percent(s.test_fns, s.fns),
        s.test_lines,
        s.test_lines,
    )
}

/// A tier-3 entry's Scope, Files and line-delta bullets for `file`'s production surface.
fn production_split_scope(file: &str, root_file: &str, dest_dir: &str) -> String {
    let s = god_file_shape(file);
    let bucket_lines: usize = s.buckets.iter().map(|b| b.lines).sum();
    let headline: Vec<String> = s
        .buckets
        .iter()
        .take(HEADLINE_BUCKETS)
        .map(|b| format!("`{}` ({}/{})", b.module, b.fns, b.lines))
        .collect();
    let stems: Vec<&str> = s
        .buckets
        .iter()
        .map(|b| b.module.rsplit("::").next().unwrap_or(&b.module))
        .collect();
    format!(
        "- Scope: {} mapped functions across {} proposed modules (roughly {bucket_lines} lines \
        of function bodies) plus {} unassigned functions ({} lines, each individually named \
        in the committed map for manual placement, per section 1's own \"unassignable \
        functions are named as such, never omitted\" rule). Headline buckets \
        (functions/lines): {}.\n\
        - Files: `{file}` -> {root_file} + `{dest_dir}/{{{}}}.rs`.\n\
        - Expected line delta: 0 net - pure relocation of roughly {} lines of function \
        bodies.\n",
        s.buckets.iter().map(|b| b.fns).sum::<usize>(),
        s.buckets.len(),
        s.unassigned_fns,
        s.unassigned_lines,
        headline.join(", "),
        stems.join(","),
        bucket_lines + s.unassigned_lines,
    )
}

/// `c`'s distinct site files, sorted and backticked.
fn distinct_files(c: &DupCluster) -> String {
    let files: BTreeSet<&str> = c.sites.iter().map(|s| s.file.as_str()).collect();
    files
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Section 6, PRIORITIZED PLAN: nineteen follow-up refactoring-spec stubs across six
/// risk-reduction tiers. See decision `u85c4-section6-plan-structure` for the tier rationale and
/// the cluster accounting (test-only + 7 named + remaining src-touching = every catalogued
/// cluster, each count read from the catalog at render time). Tier 1 item 0, "Delete the dead-code set", cites section 4.3's ledger
/// only, the same "adds no new findings" discipline every other section-6 entry follows.
fn render_section_6() -> String {
    let catalog = real_catalog();
    let test_only = test_only_clusters();
    let command_new = cited_sweep(MANDATORY_SWEEPS[0]);
    let proc_literals = cited_sweep(MANDATORY_SWEEPS[1]);
    let connection_open = cited_sweep(MANDATORY_SWEEPS[2]);
    let rigger_paths = cited_sweep(MANDATORY_SWEEPS[3]);
    let error_shaping = cited_sweep(MANDATORY_SWEEPS[4]);
    let proc_readers = cited_sweep(PROC_STAT_READERS_SWEEP);
    let named = [
        command_new.id.as_str(),
        rigger_paths.id.as_str(),
        connection_open.id.as_str(),
        proc_literals.id.as_str(),
        proc_readers.id.as_str(),
        cited(PROJECT_BATCHES).id.as_str(),
        error_shaping.id.as_str(),
    ];
    let remaining = catalog.len() - test_only.all.len() - named.len();
    let mut out = String::new();
    out.push_str("## 6. Prioritized Plan\n\n");
    out.push_str(
        "Nineteen follow-up refactoring specs, ordered largest risk-reduction first. This \
        section adds no new findings: every citation below points at a claim already \
        recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 \
        (`docs/audit/duplication-catalog.json`), section 4.3 (`docs/audit/dead-code.json`), \
        or sections 3 and 5's own prose. The three committed JSON files ground every count \
        below (queried directly, never re-scanned). Item 0 (Tier 1) \
        deletes the dead-code ledger (section 4.3); six of the remaining \
        eighteen entries split a god file (tiers 2 and 3, two phases times three files); the \
        other twelve retire duplication or close a port gap (tiers 1, 4 and 5) - kept as \
        separate entries throughout, per spec 85's own instruction that \"the god-file \
        splits and the duplication removals are separate entries so each can be its own \
        run.\"\n\n",
    );
    let tier2_shares: Vec<usize> = TARGET_FILES
        .iter()
        .map(|f| {
            let s = god_file_shape(f);
            percent(s.test_fns, s.fns)
        })
        .collect();
    out.push_str("### 6.1 How this plan is ordered\n\n");
    out.push_str(&format!(
        "Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's boundary violation; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. \
        Item 0 (deleting the dead-code ledger, section 4.3) is placed here too, first of \
        all: not a live-gap risk itself, but the cheapest, zero-behavior-change move \
        available, and it shrinks the files tiers 2 and 3 operate on before either touches \
        them.\n\
        2. Tier 2 - god-file test-module extraction: each of the three god files' own \
        inline `#[cfg(test)] mod tests` holds {tier2_lo}-{tier2_hi}% of that file's mapped \
        functions (section 1's map), and moving it is a pure \
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
        6. Tier 6 - remaining catalog sweep: the {remaining} src-touching clusters section 2 \
        found but tiers 1 and 4 did not individually name. Unlike every other tier, none of \
        these {remaining} have been read and risk-assessed one at a time the way tiers 1-4's named \
        clusters have - they are consumed straight from the catalog - so this tier carries \
        production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the \
        follow-up spec must triage each cluster's own production-or-test status before \
        merging it, not assume tier 5's blanket test-only treatment applies here too.\n\n\
        Within a tier, entries are ordered largest-first by the site or line count each \
        retires - the same rule the tiers themselves follow, applied one level down.\n\n",
        tier2_lo = tier2_shares.iter().min().copied().unwrap_or(0),
        tier2_hi = tier2_shares.iter().max().copied().unwrap_or(0),
    ));
    out.push_str("### 6.2 Tier 1: active correctness risk\n\n");
    out.push_str("#### 0. Delete the dead-code set\n\n");
    out.push_str(
        "- Scope: every entry of section 4.3's ledger (`docs/audit/dead-code.json`) and the \
        tests that exercise only it; a test that also exercises live code is trimmed, not \
        deleted. Under section 4.2's rule no entry is kept, and a function whose only caller \
        was a deleted entry is deleted in the same pass.\n",
    );
    if real_dead_code_candidates().is_empty() {
        out.push_str("- Status: complete - the ledger is empty.\n");
    } else {
        out.push_str("- Deletion list:\n\n");
        out.push_str(&render_dead_code_deletion_list(
            real_dead_code_candidates(),
            &real_dead_code_lines(),
        ));
        out.push('\n');
    }
    out.push_str(
        "- Risk: low. A deletion is a pure subtraction: `cargo build` and `clippy -D warnings` \
        on both feature lanes catch any missed reference immediately.\n\
        - Unblocks: shrinks the files tiers 2-4 operate on before they touch them, so it runs \
        first.\n\n",
    );
    out.push_str(&format!(
        "#### 1. Close the `Grounder` port gap for whole-project batch ingest (retires \
        {PROJECT_BATCHES} in the same motion)\n\n",
    ));
    out.push_str(&format!(
        "- Scope: section 3 violation 1 (`crates/rigger-grounder/src/ingest.rs::walk_batches`, reaching \
        `grounder::symbols::events::project_batches_paced` and \
        `grounder::design::events::project_batches` by concrete module path) and \
        duplication cluster `{PROJECT_BATCHES}` ({batch_n} modules' own twin \
        `project_batches` functions, in {batch_files}) are one root cause, not two - fix \
        once. {batch_n} CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): \
        `{PROJECT_BATCHES}`'s own mechanical `proposed_home` suggests relocating into `tests/common`, \
        but every site is production code under `crates/rigger-grounder/src/grounder/` and `src/grounder/`, not a test helper - the \
        mechanical heuristic has no \"add a port method\" category to route a production \
        duplicate to, so it mis-fires here. This plan follows section 3's own reasoned \
        disposition instead: add a `Grounder::project_batches` port method (or a standalone \
        `SymbolProjector` trait) covering all {batch_n} concrete modules, and point `ingest.rs` \
        at it.\n\
        - Files: `crates/rigger-grounder/src/ingest.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, \
        `crates/rigger-grounder/src/grounder/design/events.rs`, `crates/rigger-grounder/src/grounder/workflowdef.rs`.\n\
        - Expected line delta: roughly neutral - one new trait method plus {batch_n} thin impls, \
        minus the {batch_n} duplicate bodies `{PROJECT_BATCHES}` catalogs.\n\
        - Risk: medium. `ingest.rs`'s own module doc calls it \"the ONE walk-and-content-key \
        authority\" - a load-bearing path; needs the existing whole-project-ingest and \
        reindex-freshening coverage to stay green, not just the duplicate sites' own tests.\n\
        - Unblocks: retires the one `Grounder` port violation section 3 found and \
        `{PROJECT_BATCHES}` together, rather than as two separately-tracked fixes.\n\n",
        batch_n = cited(PROJECT_BATCHES).sites.len(),
        batch_files = distinct_files(cited(PROJECT_BATCHES)),
    ));
    let proc_reader_files = distinct_files(proc_readers);
    out.push_str(&format!(
        "#### 2. Retire the duplicate `/proc`-reading authority (`{}` + `{}`)\n\n",
        proc_literals.id, proc_readers.id,
    ));
    out.push_str(&format!(
        "- Scope: the production half is done - `crates/rigger-dash/src/dash.rs::process_state` and \
        `crates/rigger-process/src/reap.rs::pid_starttime` both read their `/proc/<pid>/stat` field through \
        `crates/rigger-process/src/reap.rs::stat_field_after_comm`, the one parser of the kernel's \
        `pid (comm) state ...` layout (`read_ppid` reads `/status`, a different file). What \
        remains is the test-only readers (`{readers_id}`, {readers_sites} sites across \
        {proc_reader_files}, such as the shared `tests/common/fixtures/host.rs::pgid_of` \
        fixture) and {literal_sites} raw `/proc`-path string literals across {literal_files} \
        files (`{literals_id}`), most of them assertion messages and this audit's own sweep \
        names rather than reads.\n\
        - Files: the test-only readers `{readers_id}` names.\n\
        - Expected line delta: small and negative - a test fixture reads its field through one \
        shared helper instead of re-splitting the stat line.\n\
        - Risk: low - test-only; nothing this touches can signal or end a process, so it \
        carries none of the no-os-kill gate's own risk surface.\n\
        - Unblocks: retires the last copies of the \"duplicate implementation reconciled \
        after the fact\" pattern the operator's strict-DRY rule targets - the concrete \
        precedent spec 85's own Goal cites.\n\n",
        readers_id = proc_readers.id,
        readers_sites = proc_readers.sites.len(),
        literals_id = proc_literals.id,
        literal_sites = proc_literals.sites.len(),
        literal_files = file_count(proc_literals),
    ));
    out.push_str("### 6.3 Tier 2: god-file test-module extraction\n\n");
    out.push_str(
        "Each god file's inline test module is the set of its `is_test: true` entries in the \
        committed `docs/audit/responsibility-map.json`. Each entry below moves an \
        already-passing test module with no intended production-behavior change - a \
        `cargo test` pass before and after is the whole verification. The line figures below \
        sum those test functions' own spans, so they exclude the module-level doc comments, \
        `use` statements and blank lines around them.\n\n",
    );
    let conductor = god_file_shape("crates/rigger-conductor/src/conductor.rs");
    out.push_str(
        "#### 3. Extract `crates/rigger-conductor/src/conductor.rs`'s inline test module\n\n",
    );
    out.push_str(&test_extraction_scope(
        "crates/rigger-conductor/src/conductor.rs",
        "src/conductor/tests",
        &format!(
            "Partition into a `src/conductor/tests/` directory, one file per concern, reusing \
            the same names section 1 already assigned the file's own production buckets ({}, \
            ...) so the split needs no new naming scheme.",
            bucket_names(&conductor, HEADLINE_BUCKETS)
        ),
    ));
    out.push_str(
        "- Risk: low - mechanical move of passing tests, zero intended behavior change.\n\
        - Unblocks: shrinks `conductor.rs` to its production code before tier 3 touches a \
        single production line, cutting the odds that an unrelated future unit's blast \
        radius collides with this file.\n\n",
    );
    out.push_str("#### 4. Extract `src/main.rs`'s inline test module\n\n");
    out.push_str(&test_extraction_scope(
        "src/cli/mod.rs",
        "src/main/tests",
        &format!(
            "Same partition approach as item 3, reusing section 1's own production bucket \
            names ({}, ...).",
            bucket_names(&god_file_shape("src/cli/mod.rs"), HEADLINE_BUCKETS)
        ),
    ));
    out.push_str(
        "- Risk: low, same rationale as item 3.\n\
        - Unblocks: shrinks `main.rs` to its production code before tier 3's own main.rs \
        split.\n\n",
    );
    let dash_groups: Vec<String> = god_file_shape("crates/rigger-dash/src/dash.rs")
        .test_groups
        .iter()
        .map(|g| format!("`{g}`"))
        .collect();
    out.push_str("#### 5. Extract `crates/rigger-dash/src/dash.rs`'s inline test module\n\n");
    out.push_str(&test_extraction_scope(
        "crates/rigger-dash/src/dash.rs",
        "src/dash/tests",
        &format!(
            "Lower effort than items 3-4: section 1's own classifier already found {} \
            pre-existing sub-boundaries inside this one test module ({}), so the partition \
            points already exist and need only become their own files.",
            dash_groups.len(),
            dash_groups.join(", ")
        ),
    ));
    out.push_str(
        "- Risk: low - the lowest-effort of the three, for the reason above.\n\
        - Unblocks: shrinks `dash.rs` to its production code before tier 3's own dash.rs \
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
        "#### 6. Split `crates/rigger-conductor/src/conductor.rs`'s production code into `src/conductor/*.rs`\n\n",
    );
    out.push_str(&production_split_scope(
        "crates/rigger-conductor/src/conductor.rs",
        "`src/conductor/mod.rs`",
        "src/conductor",
    ));
    out.push_str(&format!(
        "- The largest bucket, `{}`, may warrant its own second pass if it does not decompose \
        cleanly into one file.\n",
        conductor.buckets.first().map_or("", |b| b.module.as_str()),
    ));
    out.push_str(
        "- Risk: medium-high - conductor.rs is the composition root's own most complex \
        use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and \
        reap audits green, not just the final one.\n\
        - Unblocks: the largest reduction in production-code blast-radius collision risk \
        this audit identifies; makes future duplication-spotting against conductor.rs's own \
        logic tractable by a human reviewer, not only by the mechanical scanner.\n\n",
    );
    out.push_str("#### 7. Split `src/main.rs`'s production code into `src/main/*.rs`\n\n");
    out.push_str(&production_split_scope(
        "src/cli/mod.rs",
        "`src/main.rs` (composition root, thinned)",
        "src/cli",
    ));
    out.push_str(
        "- Risk: medium - `main.rs` is the composition root itself; the split must preserve \
        which concretions get wired where, not merely move text.\n\
        - Unblocks: shrinks `main.rs` to a genuine composition root plus a \
        `cli/` module tree, matching the ports-and-adapters shape this project already \
        mandates everywhere else.\n\n",
    );
    out.push_str(
        "#### 8. Split `crates/rigger-dash/src/dash.rs`'s production code into `src/dash/*.rs`\n\n",
    );
    out.push_str(&production_split_scope(
        "crates/rigger-dash/src/dash.rs",
        "`src/dash/mod.rs`",
        "src/dash",
    ));
    out.push_str(
        "- Risk: low-medium - the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected \
        by a pure module split.\n\
        - Unblocks: completes the god-file split trio; the third program-sized file becomes \
        an ordinary module tree.\n\n",
    );
    out.push_str("### 6.5 Tier 4: named production duplication sweeps\n\n");
    out.push_str(
        "Each entry is one of section 2's five named mandatory sweeps - collected \
        mechanically regardless of the Jaccard pass, per spec 85's own Design.\n\n",
    );
    let (rigger_id, rigger_n) = (&rigger_paths.id, rigger_paths.sites.len());
    let (command_id, command_n) = (&command_new.id, command_new.sites.len());
    let (conn_id, conn_n) = (&connection_open.id, connection_open.sites.len());
    let (error_id, error_n) = (&error_shaping.id, error_shaping.sites.len());
    let error_src = {
        let mut by_file: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for s in error_shaping
            .sites
            .iter()
            .filter(|s| s.file.starts_with("src/"))
        {
            by_file
                .entry(s.file.as_str())
                .or_default()
                .push(format!("`{}`", s.name));
        }
        by_file
            .iter()
            .map(|(f, names)| format!("`{f}` ({})", names.join(", ")))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let error_tests = error_shaping
        .sites
        .iter()
        .filter(|s| s.file.starts_with("tests/"))
        .map(|s| s.file.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    out.push_str(&format!(
        "#### 9. Consolidate the {rigger_n} `.rigger`-path string-literal sites (`{rigger_id}`) - the \
        single largest cluster in the entire catalog by site count\n\n",
    ));
    out.push_str(&format!(
        "- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the {rigger_n} sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-config-files/src/config_store.rs`, \
        `crates/rigger-dash/src/dash.rs`, `crates/rigger-domain/src/docs.rs`, `crates/rigger-gates-shell/src/gate.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, \
        `crates/rigger-grounder/src/grounder/symbols/store.rs`, `crates/rigger-grounder/src/ingest.rs`, `src/main.rs`, `crates/rigger-process/src/reap.rs`, \
        `crates/rigger-store-sqlite/src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `{rigger_id}` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - {rigger_n} literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing {rigger_n} sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n",
    ));
    let port_n = command_new
        .sites
        .iter()
        .filter(|s| s.file == PROCESS_SPAWN_PORT)
        .count();
    let test_n = command_n - port_n;
    out.push_str(&format!(
        "#### 10. The {command_n} `Command::new` call sites (`{command_id}`) - production spawns \
        already route through one process-spawn port\n\n",
    ));
    out.push_str(&format!(
        "- Scope: every production spawn routes through `{PROCESS_SPAWN_PORT}` (the cluster's own \
        `proposed_home`), and the audit's \
        `the_process_spawn_port_is_the_only_production_command_new_caller` gate refuses a new \
        direct construction anywhere else in production code. The {port_n} site(s) in \
        `{PROCESS_SPAWN_PORT}` are the port itself; the other {test_n} are test code spawning \
        git, shells and the product binary.\n\
        - Files: `{PROCESS_SPAWN_PORT}` plus test code in `src/` and `tests/` - full site list in \
        `docs/audit/duplication-catalog.json` under `{command_id}`.\n\
        - Expected line delta: none left in production; a test site that repeats a shared \
        fixture's spawn routes through that fixture instead.\n\
        - Risk: low - no production spawn is left to move, and the gate keeps it that way.\n\
        - Unblocks: the next process-spawning concern added anywhere in the crate reuses the \
        port instead of constructing its own `Command`.\n\n",
    ));
    out.push_str(&format!(
        "#### 11. Consolidate the {conn_n} sqlite `Connection::open` call sites (`{conn_id}`)\n\n",
    ));
    out.push_str(&format!(
        "- Scope: one sqlite-connection-opening adapter function (the cluster's own \
        `proposed_home`) spanning `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, \
        `crates/rigger-store-sqlite/src/eventstore/sqlite.rs` \
        and `src/main.rs`, plus several `tests/` files.\n\
        - Files: full site list in `docs/audit/duplication-catalog.json` under `{conn_id}`.\n\
        - Expected line delta: negative - {conn_n} open calls collapse toward one function.\n\
        - Risk: medium - touches the event store and context graph's own \
        connection-lifecycle code; needs the store-identity and store-resolution contract \
        tests green throughout.\n\
        - Unblocks: one place to change pragma/timeout/journal-mode settings instead of \
        {conn_n}.\n\n",
    ));
    out.push_str(&format!(
        "#### 12. Consolidate the {error_n} error-shaping helper sites (`{error_id}`) - caution, \
        confirm before merging\n\n",
    ));
    out.push_str(&format!(
        "- Scope: the cluster spans {error_src} and {error_tests} unrelated test files - a \
        wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own \
        CONSTRAINTS WALK: \"the threshold is a floor for the mechanical pass; the reading \
        pass owns semantic duplicates\") rather than one real shared concern - the follow-up \
        spec's first job is confirming by reading whether these {error_n} sites share actual \
        logic before proposing one helper, not assuming the cluster label proves it.\n\
        - Files: the `src/` files above, plus the {error_tests} test files named in `docs/audit/duplication-catalog.json` under `{error_id}`.\n\
        - Expected line delta: unknown pending the confirmation read above - potentially \
        zero if the cluster does not survive a human read.\n\
        - Risk: low (the smallest-site-count sweep), but with the stated precondition.\n\
        - Unblocks: either a genuine fifth consolidation, or a documented \"not a real \
        duplicate\" disposition that keeps the catalog honest for whoever reads it next.\n\n",
    ));
    out.push_str("### 6.6 Tier 5: test-suite consolidation\n\n");
    out.push_str(
        "Every entry cites section 5's own already-catalogued test-only duplication; none \
        of it carries production-correctness risk.\n\n",
    );
    out.push_str(
        "#### 13. Extract the headline shared test fixtures into `tests/common` (section 5.2)\n\n",
    );
    let widest_helpers = widest_first(&test_only.helpers);
    let item_13 = &widest_helpers[..HEADLINE_HELPER_CLUSTERS.min(widest_helpers.len())];
    let item_13_sites: usize = item_13.iter().map(|c| c.sites.len()).sum();
    let item_13_scope: Vec<String> = item_13
        .iter()
        .map(|c| format!("`{}` ({} files)", c.id, file_count(c)))
        .collect();
    out.push_str(&format!(
        "- Scope: {} - roughly {item_13_sites} duplicate definitions collapsing into {} shared \
        ones, the single largest mechanical simplification section 5 identifies anywhere in \
        the test suite.\n\
        - Files: per-cluster, from the committed catalog, plus `tests/common/`.\n\
        - Expected line delta: negative - each fixture's small body survives once instead of \
        once per file.\n\
        - Risk: low - test-only, and `tests/common/` already holds the same shape of shared \
        fixture.\n\
        - Unblocks: item 16 below (the remaining test-helper clusters) reuses the same \
        `tests/common` home this item establishes.\n\n",
        item_13_scope.join(", "),
        item_13.len(),
    ));
    out.push_str(
        "#### 14. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)\n\n",
    );
    let cli_tests = cli_test_names();
    out.push_str(&format!(
        "- Scope: {} tests, split into \
        `tests/cli_{{step,run,validate,reset,watch,canary,dash,store,review,setup}}.rs` plus a \
        residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 \
        names, using each test's dominant scenario (a human/AI read, not the {}%-coverage \
        keyword match section 5.3 already disclosed as insufficient alone).\n\
        - Files: `tests/cli.rs` and the eleven new files above.\n\
        - Expected line delta: 0 net - pure relocation into eleven files.\n\
        - Risk: low-medium - a mechanical per-test move with `cargo test`'s full pass count as the verification.\n\
        - Unblocks: splits a file in {} cross-file duplication clusters (section 5.3) and \
        lets item 16's remaining-clusters sweep target smaller, subcommand-scoped files.\n\n",
        cli_tests.len(),
        percent(verb_covered(&cli_tests), cli_tests.len()),
        cli_cross_file_clusters(&test_only.all).len(),
    ));
    let item_15_families = largest_open_test_families(&test_only.tests);
    let item_15: Vec<&str> = item_15_families.iter().map(|c| c.id.as_str()).collect();
    let item_15_sites: usize = item_15_families.iter().map(|c| c.sites.len()).sum();
    out.push_str(
        "#### 15. Convert the largest remaining table-driven test families into \
        parametrized tables (section 5.5)\n\n",
    );
    out.push_str(&format!(
        "- Scope, largest first ({item_15_sites} sites across {} clusters; the spec_lint, \
        no-os-kill, reap-audit exemption, scanner and pid-refusal families are already \
        closed):\n{}\
        - Files: the files named above.\n\
        - Expected line delta: negative - each family's near-identical test bodies collapse \
        into `test_cases!` rows over one case helper.\n\
        - Risk: low - test-only, and each family already shares one body shape (section \
        5.5's own finding).\n\
        - Unblocks: the largest remaining reduction in raw `#[test]` body count.\n\n",
        item_15.len(),
        test_family_bullets(&item_15_families),
    ));
    let (helpers_n, tests_n) = (test_only.helpers.len(), test_only.tests.len());
    let helpers_left = helpers_n - item_13.len();
    let tests_left = tests_n - count_cited_in(&item_15, &test_only.tests);
    out.push_str(&format!(
        "#### 16. Sweep the remaining {helpers_left} test-only helper-duplication clusters \
        (section 5.4, beyond item 13's headline fixtures)\n\n",
    ));
    out.push_str(&format!(
        "- Scope: the {helpers_n} test-only, all-helper-function clusters section 5.4 names, \
        minus the ones item 13 already covers - consumed directly from \
        `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own \
        stated approach).\n\
        - Files: per-cluster, from the committed catalog.\n\
        - Expected line delta: negative, cumulative across {helpers_left} clusters.\n\
        - Risk: low - test-only.\n\
        - Unblocks: closes out the helper-duplication half of the test suite's own \
        strict-DRY exposure.\n\n",
    ));
    out.push_str(&format!(
        "#### 17. Sweep the remaining {tests_left} table-driven test families (section 5.5, \
        beyond item 15's headline families)\n\n",
    ));
    out.push_str(&format!(
        "- Scope: the {tests_n} test-only, all-`#[test]` clusters section 5.5 names, minus \
        the {} cluster ids item 15 already covers - consumed directly from \
        `docs/audit/duplication-catalog.json`.\n\
        - Files: per-cluster, from the committed catalog.\n\
        - Expected line delta: negative, cumulative.\n\
        - Risk: low - test-only.\n\
        - Unblocks: closes out the table-driven-test half of the test suite's own \
        strict-DRY exposure; combined with items 13 and 15-16, retires all {} test-only \
        clusters section 2 found.\n\n",
        item_15.len(),
        test_only.all.len(),
    ));
    out.push_str("### 6.7 Tier 6: remaining catalog sweep\n\n");
    out.push_str(
        "Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one \
        needs its own read before merging (see `### 6.1`'s tier 6 rationale above).\n\n",
    );
    out.push_str(&format!(
        "#### 18. Sweep the remaining {remaining} src-touching duplication clusters \
        (section 2, beyond tiers 1 and 4's {} named clusters)\n\n",
        named.len(),
    ));
    out.push_str(&format!(
        "- Scope: of the catalog's {} clusters, {} are test-only (items 13 and 15-17 \
        above) and {} are the named tier-1/tier-4 items ({}); the remaining {remaining} \
        clusters touching `src/` - mostly small 2-5-site exact/near matches like the two \
        worked examples section 2 itself opens with (`{}`, `{}`) - are swept here, largest \
        exact-duplicate clusters first, consumed directly from \
        `docs/audit/duplication-catalog.json`.\n\
        - Files: per-cluster, from the committed catalog.\n\
        - Expected line delta: negative, cumulative; the largest single contributor is \
        whichever exact cluster has the most sites (read from the catalog at spec-writing \
        time, not fixed here).\n\
        - Risk: low-medium - unlike tier 5, some of these clusters are production code, so \
        each merge needs its own test-coverage check, not a blanket \"test-only\" pass.\n\
        - Unblocks: the last of the catalog's {} clusters; after items 1-2 and 9-18 all \
        land, a future spec can state and check that the duplication catalog's own drift \
        guard finds zero live clusters left unaddressed.\n\n",
        catalog.len(),
        test_only.all.len(),
        named.len(),
        named
            .iter()
            .map(|id| format!("`{id}`"))
            .collect::<Vec<_>>()
            .join(", "),
        catalog[0].id,
        catalog[1].id,
        catalog.len(),
    ));
    out.push_str("### 6.8 Dead and vestigial code beyond item 0: no further follow-up\n\n");
    out.push_str(
        "Section 4.2's rule leaves no dead-code category for a later plan item: a function is \
        live or it is deleted by item 0. Both named retirements (`turbovec`, `kurrentdb`) are \
        still fully clean, and the two stale-looking doc paths found remain confirmed generic \
        illustrative examples, not real dangling references.\n",
    );
    out
}

// -----------------------------------------------------------------------------------------
// THE ADVERSARIAL SAMPLE (spec 85 THOROUGHNESS)
// -----------------------------------------------------------------------------------------

/// Deterministically draw `k` of the functions named by `keys` from `seed` (spec 85 THOROUGHNESS:
/// "the adversary draws 30 functions by seeded random index... The report states the sample
/// seed so the check is reproducible"), returned as indices into `keys`, sorted ascending for a
/// stable, readable listing. Each function is ranked by the seeded hash of its OWN identity (see
/// [`sample_key`]) and the `k` lowest ranks are drawn, so the draw is stable under change: a
/// function added or removed elsewhere never moves any other function's rank, and a cleanup
/// that closes a duplicate replaces only the drawn rows it deleted rather than reshuffling the
/// whole already-read sample (a draw by index into the population reshuffled on every change to
/// the function count, discarding each reading pass).
fn sample_indices(keys: &[String], k: usize, seed: u64) -> Vec<usize> {
    let mut ranked: Vec<(String, usize)> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| (content_hash(&format!("{seed}\u{0}{key}")), i))
        .collect();
    ranked.sort();
    let mut picked: Vec<usize> = ranked.into_iter().take(k).map(|(_, i)| i).collect();
    picked.sort_unstable();
    picked
}

/// The identity [`sample_indices`] ranks a drawn function by: its file, its name, and its
/// ordinal among the same-named functions of that file (in source order) - never its line or
/// its position in the population, both of which move whenever anything above it changes.
fn sample_keys(files: &[FileScan], refs: &[FnRef]) -> Vec<String> {
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    refs.iter()
        .map(|r| {
            let sf = r.scanned(files);
            let ordinal = seen.entry((sf.file.clone(), sf.name.clone())).or_insert(0);
            let key = sample_key(&sf.file, &sf.name, *ordinal);
            *ordinal += 1;
            key
        })
        .collect()
}

/// One function's draw identity - see [`sample_keys`].
fn sample_key(file: &str, name: &str, ordinal: usize) -> String {
    format!("{file}\u{0}{name}\u{0}{ordinal}")
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
// (`render_body: render_x_skill,` in `crates/rigger-domain/src/docs.rs`'s `skill_registry`) and a UFCS value passed
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
    /// Index of this occurrence's own token in its file's token stream - what
    /// [`receiver_type`] reads backwards from to name the type a method call targets.
    tok_idx: usize,
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

/// The declaring file's OWN "file-per-module" directory - mirrors `crates/rigger-grounder/src/grounder/symbols/
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

/// Every production source file of the whole workspace (the root `src/` and each member
/// crate's `src/`), as [`collect_files_with_content`] reads them.
fn collect_workspace_src_files_with_content(root: &Path) -> Vec<(String, String)> {
    let mut dirs = vec!["src".to_string()];
    dirs.extend(
        workspace_member_dirs(root)
            .into_iter()
            .map(|m| format!("{m}/src")),
    );
    collect_files_with_content(root, &dirs)
}

/// Every `.rs` file under each of `dirs` (relative to `root`), read once, as repo-relative
/// `(path, content)` pairs - deterministically ordered ([`collect_rs_files`] sorts within each
/// directory).
fn collect_files_with_content(root: &Path, dirs: &[String]) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for dir in dirs {
        collect_rs_files(&root.join(dir), &mut paths);
    }
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

/// `f`'s test-code line spans: every test fn's span AND every test mod's span, so an item at a
/// `#[cfg(test)] mod`'s own top level (outside every fn body) still reads as test code.
fn test_line_ranges(f: &FileScan) -> Vec<(usize, usize)> {
    f.fns
        .iter()
        .filter(|sf| sf.is_test)
        .map(|sf| (sf.start_line, sf.end_line))
        .chain(
            f.mod_spans
                .iter()
                .filter(|m| m.is_test)
                .map(|m| (m.start_line, m.end_line)),
        )
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
        let is_src = is_production_source(&f.rel);
        let file_wholly_test = whole_file_test.contains(&f.rel);
        // Round 1 (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances` class 1):
        // test regions are FN spans (`is_test`) AND MOD spans (`ModSpan::is_test`) - never fn
        // spans alone, so a `use`/`const`/`static`/`type` item sitting at `#[cfg(test)] mod
        // tests { .. }`'s own top level (outside every fn body) is still test code. A
        // wholly-test FILE (`file_wholly_test`) is handled separately below, directly on
        // `production`, rather than by synthesizing a whole-file span here.
        let test_ranges = test_line_ranges(f);
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
                            tok_idx: i,
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
                                    tok_idx: i,
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
                tok_idx: i,
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

/// One production fn with ZERO production references: a dead-code ledger entry, to be deleted
/// (section 4.2's rule - live or deleted, no third state).
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
    /// Spec 90 criterion 2 addition: this candidate's own line-free identity (see
    /// [`span_content_hash`], over its whole `[start_line, end_line]` span like a catalog
    /// [`dup_site`]) - what [`DEAD_CODE_PATH`]'s guarded wire form carries INSTEAD of `line`.
    content_hash: String,
}

/// `true` for an entry point the language or a foreign caller invokes, never Rust code: a
/// top-level `fn main`, not nested in any `mod`/`impl`, or an `extern "C"` export (a member
/// crate's WebAssembly ABI is called from JavaScript).
fn is_exempt_entry_point(f: &ScannedFn, file_tokens: &HashMap<&str, &[RawTok]>) -> bool {
    if f.name == "main" && f.enclosing_mods.is_empty() && f.enclosing_impl.is_none() {
        return true;
    }
    file_tokens
        .get(f.file.as_str())
        .map(|toks| {
            fn_signature_tokens(toks, f)
                .iter()
                .take_while(|t| !(t.kind == RawKind::Keyword && t.text == "fn"))
                .any(|t| t.kind == RawKind::Keyword && t.text == "extern")
        })
        .unwrap_or(false)
}

/// `f`'s own signature tokens: from the first token on its `fn` line through its body's
/// opening line (visibility, qualifiers, name, generics, parameters, return type).
fn fn_signature_tokens<'a>(file_tokens: &'a [RawTok], f: &ScannedFn) -> &'a [RawTok] {
    body_tokens(file_tokens, f.start_line, f.body_start_line)
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
    // The parameter list is the first `(` after the `fn` name and its own generics - never an
    // earlier one (`pub(crate)`, a `#[cfg_attr(..)]` sharing the line).
    let Some(fn_idx) = sig_toks
        .iter()
        .position(|t| t.kind == RawKind::Keyword && t.text == "fn")
    else {
        return false;
    };
    let mut i = fn_idx + 2;
    if ref_punct(sig_toks.get(i), "<") {
        let mut depth = 0i32;
        while let Some(t) = sig_toks.get(i) {
            if t.kind == RawKind::Punct && t.text == "<" {
                depth += 1;
            } else if t.kind == RawKind::Punct
                && t.text == ">"
                && !ref_punct(sig_toks.get(i - 1), "-")
            {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
    }
    if !ref_punct(sig_toks.get(i), "(") {
        return false;
    }
    i += 1;
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
/// alive through an unrelated same-named `crates/rigger-config-files/src/playbooks.rs` `rebuild`'s real caller) is exactly
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
        .map(|toks| signature_declares_self(fn_signature_tokens(toks, f)))
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
/// `crates/rigger-config-files/src/playbooks.rs`'s unrelated `rebuild`, qualifier `"playbooks"` - `main.rs`'s
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

/// The innermost function whose span contains `line` in `file`, if any.
fn innermost_fn_at(file: &FileScan, line: usize) -> Option<&ScannedFn> {
    file.fns
        .iter()
        .filter(|f| f.start_line <= line && line <= f.end_line)
        .max_by_key(|f| f.start_line)
}

/// The type a type expression starting at `toks[k]` names: references, lifetimes and `mut`
/// skipped, `Self` read as `self_type`, a path reduced to its last segment (`crate::a::B` ->
/// `B`). `None` for a trait object or `impl Trait`, whose concrete type is unknown.
fn type_named_at(toks: &[RawTok], mut k: usize, self_type: Option<&str>) -> Option<String> {
    while let Some(t) = toks.get(k) {
        match (&t.kind, t.text.as_str()) {
            (RawKind::Punct, "&") | (RawKind::Lifetime, _) | (RawKind::Keyword, "mut") => k += 1,
            _ => break,
        }
    }
    let first = toks.get(k)?;
    match (&first.kind, first.text.as_str()) {
        (RawKind::Keyword, "Self") => return self_type.map(str::to_string),
        (RawKind::Ident, _) => {}
        _ => return None,
    }
    let mut last = first.text.clone();
    while ref_punct(toks.get(k + 1), ":") && ref_punct(toks.get(k + 2), ":") {
        match toks.get(k + 3) {
            Some(t) if t.kind == RawKind::Ident => {
                last = t.text.clone();
                k += 3;
            }
            _ => break,
        }
    }
    Some(last)
}

/// The type of the local binding `name` visible at token `use_idx` inside `f`, read from its
/// most recent binding: a typed parameter (`name: T`), a typed `let` (`let name: T`), or a
/// struct-literal `let` (`let name = T { .. }`). Any other most-recent binding (a `for` or
/// closure binding, an untyped `let` from a call) leaves the type unknown.
fn local_binding_type(
    toks: &[RawTok],
    f: &ScannedFn,
    use_idx: usize,
    name: &str,
    self_type: Option<&str>,
) -> Option<String> {
    let lo = toks.partition_point(|t| t.line < f.start_line);
    let is_kw = |j: usize, kw: &str| {
        toks.get(j)
            .map(|t| t.kind == RawKind::Keyword && t.text == kw)
            .unwrap_or(false)
    };
    let mut found: Option<Option<String>> = None;
    for j in lo..use_idx {
        let t = &toks[j];
        if t.kind != RawKind::Ident || t.text != name || j == 0 {
            continue;
        }
        let before = if is_kw(j - 1, "mut") && j >= 2 {
            j - 2
        } else {
            j - 1
        };
        let typed = ref_punct(toks.get(j + 1), ":") && !ref_punct(toks.get(j + 2), ":");
        if is_kw(before, "let") {
            found = Some(if typed {
                type_named_at(toks, j + 2, self_type)
            } else if ref_punct(toks.get(j + 1), "=") {
                let ty = type_named_at(toks, j + 2, self_type);
                let mut k = j + 2;
                while toks
                    .get(k)
                    .map(|t| t.kind == RawKind::Ident || t.text == ":")
                    == Some(true)
                {
                    k += 1;
                }
                ty.filter(|_| ref_punct(toks.get(k), "{"))
            } else {
                None
            });
        } else if t.line <= f.body_start_line
            && typed
            && (ref_punct(toks.get(before), "(") || ref_punct(toks.get(before), ","))
        {
            found = Some(type_named_at(toks, j + 2, self_type));
        } else if is_kw(before, "for") || ref_punct(toks.get(before), "|") {
            found = Some(None);
        }
    }
    found.flatten()
}

/// The type a reference at `file.tokens[i]` is called on, when the scanner can read it: the
/// `T` of a `T::name` path, the enclosing impl's own type for `Self::name` or `self.name`, or
/// the declared type of a plain local receiver (`x.name` - see [`local_binding_type`]).
/// `None` whenever the receiver is anything else (a field, a call result, a chained expression).
fn receiver_type(file: &FileScan, i: usize) -> Option<String> {
    let toks = &file.tokens;
    let enclosing = innermost_fn_at(file, toks[i].line);
    let self_type = enclosing
        .and_then(|f| f.enclosing_impl.as_deref())
        .map(impl_self_type);
    if let Some(q) = qualifier_before(toks, i) {
        return match q.as_str() {
            "Self" => self_type,
            q if q.starts_with(char::is_uppercase) => Some(q.to_string()),
            _ => None,
        };
    }
    if i < 2 || !ref_punct(toks.get(i - 1), ".") {
        return None;
    }
    let recv = &toks[i - 2];
    if recv.kind == RawKind::Keyword && recv.text == "self" {
        return self_type;
    }
    if recv.kind != RawKind::Ident
        || (i >= 3 && (ref_punct(toks.get(i - 3), ".") || ref_punct(toks.get(i - 3), ":")))
    {
        return None;
    }
    local_binding_type(toks, enclosing?, i - 2, &recv.text, self_type.as_deref())
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
        .filter(|fsc| is_production_source(&fsc.rel))
        .flat_map(|fsc| fsc.fns.iter())
        .filter(|f| !(f.is_test || whole_file_test.contains(&f.file)))
        .filter(|f| !is_exempt_entry_point(f, &file_tokens))
        .filter(|f| !is_trait_impl(f))
        .collect();
    candidates.sort_by(|a, b| (&a.file, a.start_line).cmp(&(&b.file, b.start_line)));

    let idx = all_ident_ref_sites(files, whole_file_test);
    let file_scans: HashMap<&str, &FileScan> =
        files.iter().map(|fsc| (fsc.rel.as_str(), fsc)).collect();
    // Every type that defines a method or associated fn of a given name, in any impl block
    // (inherent or trait, production or test) - a call resolved to one of these types is that
    // type's own call, never another type's same-named method.
    let mut definers: HashMap<&str, BTreeSet<String>> = HashMap::new();
    for sf in files.iter().flat_map(|fsc| fsc.fns.iter()) {
        if let Some(header) = sf.enclosing_impl.as_deref() {
            definers
                .entry(sf.name.as_str())
                .or_default()
                .insert(impl_self_type(header));
        }
    }
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

        let my_type = f.enclosing_impl.as_deref().map(impl_self_type);
        let mut production_hit = false;
        let mut test_only: Vec<TestOnlyRef> = Vec::new();
        for s in relevant {
            let counts_for_me = if s.via_attribute {
                true
            } else if cat == DispatchCategory::Method {
                // A method is credited with a call whose receiver resolves to its own type, or
                // whose receiver the scanner cannot read (the conservative direction); a call
                // resolved to ANOTHER type that defines the same name is that type's alone.
                match receiver_type(file_scans[s.file.as_str()], s.tok_idx) {
                    None => true,
                    Some(t) => {
                        Some(&t) == my_type.as_ref()
                            || !definers
                                .get(f.name.as_str())
                                .is_some_and(|types| types.contains(&t))
                    }
                }
            } else if !needs_attribution {
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
        });
    }
    out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    out
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

/// The real checked-out tree's whole-`src`-tree file-aware test-file set, memoized alongside
/// [`real_files`] for the same reason ([`resolve_out_of_line_test_files`] re-scans every `src/`
/// file's out-of-line mods, which is cheap, but no need to repeat it per test).
fn real_whole_file_test_set() -> &'static BTreeSet<String> {
    static CACHE: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        resolve_out_of_line_test_files(&collect_workspace_src_files_with_content(&repo_root()))
    })
}

/// The real checked-out workspace's [`scan_workspace`], memoized like [`real_files`].
fn real_workspace_files() -> &'static [FileScan] {
    static CACHE: std::sync::OnceLock<Vec<FileScan>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| scan_workspace(&repo_root()))
}

/// The real checked-out workspace's [`build_dead_code_candidates`], memoized alongside
/// [`real_files`] for the same reason.
fn real_dead_code_candidates() -> &'static [DeadCodeCandidate] {
    static CACHE: std::sync::OnceLock<Vec<DeadCodeCandidate>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        build_dead_code_candidates(real_workspace_files(), real_whole_file_test_set())
    })
}

/// Section 4.2's gate: the dead-code ledger must be empty. Panics naming every entry as
/// `name (file:line)` so the failure says exactly what to delete.
fn assert_dead_code_ledger_empty(candidates: &[DeadCodeCandidate]) {
    let entries: Vec<String> = candidates
        .iter()
        .map(|c| format!("{} ({}:{})", c.name, c.file, c.line))
        .collect();
    assert!(
        entries.is_empty(),
        "the dead-code ledger must be empty - delete each entry or give it a production \
         caller:\n{}",
        entries.join("\n")
    );
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
        let f = scan_single("fn foo() {\n    let x = 1;\n}\n");
        assert_eq!(f.name, "foo");
        assert_eq!(f.start_line, 1);
        assert_eq!(f.end_line, 3);
        assert!(!f.is_test);
        assert_eq!(f.enclosing_impl, None);
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

    /// Scan `src`, assert it holds exactly ONE function, and return that function.
    fn scan_single(src: &str) -> ScannedFn {
        let fns = scan_str(src);
        assert_eq!(fns.len(), 1, "{fns:?}");
        fns.into_iter().next().unwrap()
    }

    /// `src` scans to exactly one function, named `name`.
    fn assert_single_fn_named(src: &str, name: &str) {
        assert_eq!(scan_single(src).name, name);
    }

    /// `src` scans to exactly one function, whose body closes on line `end_line`.
    fn assert_single_fn_ends_at(src: &str, end_line: usize) {
        assert_eq!(scan_single(src).end_line, end_line);
    }

    /// `src` scans to exactly one function, named `name`, whose body closes on line `end_line`.
    fn assert_single_fn_spans(src: &str, name: &str, end_line: usize) {
        let f = scan_single(src);
        assert_eq!((f.name.as_str(), f.end_line), (name, end_line), "{f:?}");
    }

    rigger::test_cases! {
        fnv1a_is_not_mistaken_for_the_fn_keyword:
            assert_single_fn_named("fn fnv1a_64(bytes: &[u8]) -> u64 {\n    0\n}\n", "fnv1a_64");
    }

    // -------------------------------------------------------------------------------------
    // Scanner: lexical states must not corrupt brace matching
    // -------------------------------------------------------------------------------------

    rigger::test_cases! {
        a_brace_inside_a_line_comment_is_ignored:
            assert_single_fn_ends_at("fn a() {\n    // a stray { brace\n    let _ = 1;\n}\n", 4);
        a_brace_inside_a_block_comment_is_ignored:
            assert_single_fn_ends_at("fn a() {\n    /* a { stray } brace */\n    let _ = 1;\n}\n", 4);
        nested_block_comments_are_handled:
            assert_single_fn_ends_at("fn a() {\n    /* outer /* inner { */ still comment */\n    let _ = 1;\n}\n", 4);
        a_brace_inside_a_string_literal_is_ignored:
            assert_single_fn_ends_at("fn a() {\n    let s = \"{ not a brace }\";\n}\n", 3);
        a_brace_inside_a_raw_string_with_hashes_is_ignored:
            assert_single_fn_ends_at("fn a() {\n    let s = r#\"{ not \\\"real\\\" }\"#;\n}\n", 3);
        a_brace_inside_a_byte_string_is_ignored:
            assert_single_fn_ends_at("fn a() {\n    let s = b\"{ not a brace }\";\n}\n", 3);
        a_brace_char_literal_is_not_mistaken_for_real_braces:
            assert_single_fn_ends_at("fn a() {\n    let c = '{';\n    let d = '}';\n}\n", 4);
        a_lifetime_is_not_mistaken_for_a_char_literal:
            assert_single_fn_ends_at("fn a<'x>(v: &'x str) -> &'x str {\n    v\n}\n", 3);
        an_escaped_quote_char_literal_does_not_confuse_the_scanner:
            assert_single_fn_ends_at("fn a() {\n    let c = '\\'';\n    let _ = 1;\n}\n", 4);
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

    rigger::test_cases! {
        a_trait_default_method_with_a_body_is_recorded:
            assert_single_fn_named("trait T {\n    fn spawn(&self) {\n        let _ = 1;\n    }\n}\n", "spawn");
    }

    #[test]
    fn a_fn_pointer_type_usage_is_not_recorded() {
        assert_single_fn_named(
            "fn takes_fp(f: fn(usize) -> bool) -> bool {\n    f(1)\n}\n",
            "takes_fp",
        );
    }

    // -------------------------------------------------------------------------------------
    // Scanner: context (impl blocks, mods, #[cfg(test)] inheritance)
    // -------------------------------------------------------------------------------------

    /// `src` scans to exactly one function, declared in the impl block headed `impl_header`.
    /// Returns it.
    fn single_fn_in_impl(src: &str, impl_header: &str) -> ScannedFn {
        let f = scan_single(src);
        assert_eq!(f.enclosing_impl.as_deref(), Some(impl_header));
        f
    }

    rigger::test_cases! {
        a_method_inside_an_impl_block_carries_its_header: assert_eq!(
            single_fn_in_impl("impl Foo {\n    fn bar(&self) {}\n}\n", "Foo").name,
            "bar"
        );
        a_trait_impl_header_keeps_the_trait_for_type_text: single_fn_in_impl(
            "impl AgentDriver for Stub {\n    fn spawn(&self) {}\n}\n",
            "AgentDriver for Stub",
        );
        /// Regression (adjudicator u85c1 round 1 REJECT): an impl block sitting inside a
        /// #[cfg(test)] mod must propagate that ancestry to its methods - FrameKind::Impl
        /// previously had no is_test field at all, so this was always false.
        a_method_inside_an_impl_nested_in_a_cfg_test_mod_is_flagged_test: {
            let f = single_fn_in_impl(
                "#[cfg(test)]\nmod tests {\n    impl AgentDriver for CacheDriver {\n        fn spawn(&self) {}\n    }\n}\n",
                "AgentDriver for CacheDriver",
            );
            assert!(f.is_test, "{f:?}");
        };
    }

    #[test]
    fn a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test() {
        // Regression: a #[cfg(test)] attribute attached DIRECTLY to a standalone impl block
        // (no enclosing cfg-test mod) must also mark its methods test - the impl push site
        // used to drop pending_cfg_test unconditionally instead of reading it like
        // Mod/Trait/Fn already do.
        let f = scan_single("#[cfg(test)]\nimpl<'a> RunCtx<'a> {\n    fn for_test() -> Self {\n        todo!()\n    }\n}\n");
        assert!(f.is_test, "{f:?}");
    }

    #[test]
    fn a_non_cfg_test_impl_block_still_inherits_a_non_test_ancestry() {
        // Sanity: the fix must not make every impl test-only - a plain impl outside any
        // cfg-test context stays production.
        let src = "impl Foo {\n    fn bar(&self) {}\n}\n";
        let f = scan_single(src);
        assert!(!f.is_test, "{f:?}");
    }

    /// `src` scans to exactly one function, flagged test, enclosed by the `mods` path.
    fn assert_single_test_fn_in_mods(src: &str, mods: &[&str]) {
        let f = scan_single(src);
        assert!(f.is_test);
        assert_eq!(f.enclosing_mods, mods);
    }

    rigger::test_cases! {
        a_function_directly_in_a_cfg_test_mod_is_flagged_test: assert_single_test_fn_in_mods(
            "#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n",
            &["tests"],
        );
        a_nested_named_test_submodule_is_still_flagged_test_and_named: assert_single_test_fn_in_mods(
            "#[cfg(test)]\nmod tests {\n    mod inner_group {\n        fn a() {}\n    }\n}\n",
            &["tests", "inner_group"],
        );
    }

    #[test]
    fn a_bare_test_attribute_on_a_free_function_marks_it_test_without_a_cfg_test_mod() {
        let src = "#[test]\nfn a_thing_works() {}\n";
        let f = scan_single(src);
        assert!(f.is_test);
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
        let f = scan_single(src);
        assert_eq!(f.name, "a");
        assert_eq!(f.enclosing_mods, Vec::<String>::new());
    }

    #[test]
    fn a_locally_nested_fn_inside_a_function_body_is_still_found() {
        let src = "fn outer() {\n    fn inner() {\n        let _ = 1;\n    }\n    inner();\n}\n";
        let names: Vec<_> = scan_str(src).into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["inner".to_string(), "outer".to_string()]);
    }

    rigger::test_cases! {
        a_semicolon_inside_an_array_type_param_does_not_end_the_signature_early: assert_single_fn_spans(
            "fn a(buf: [u8; 32]) -> bool {\n    buf.len() == 32\n}\n",
            "a",
            3,
        );
        braces_from_if_match_and_closures_do_not_break_the_enclosing_fns_span: assert_single_fn_spans(
            "fn a(x: i32) -> i32 {\n    if x > 0 {\n        1\n    } else {\n        match x {\n            _ => 0,\n        }\n    }\n}\n",
            "a",
            9,
        );
    }

    // -------------------------------------------------------------------------------------
    // Scanner: spec 87 criterion 2 additions - visibility, body_start_line, out-of-line mods
    // -------------------------------------------------------------------------------------

    /// The first function `src` scans to carries visibility `visibility`.
    fn assert_first_fn_visibility(src: &str, visibility: &str) {
        let fns = scan_str(src);
        assert_eq!(fns[0].visibility, visibility);
    }

    rigger::test_cases! {
        a_free_function_with_no_pub_keyword_is_private:
            assert_first_fn_visibility("fn helper() {}\n", "private");
    }

    #[test]
    fn a_bare_pub_function_is_captured_verbatim() {
        let fns = scan_str("pub fn helper() {}\n");
        assert_eq!(fns[0].visibility, "pub");
        assert_eq!(fns[0].name, "helper");
    }

    rigger::test_cases! {
        a_pub_crate_function_keeps_the_qualifier:
            assert_first_fn_visibility("pub(crate) fn helper() {}\n", "pub(crate)");
        a_pub_super_function_keeps_the_qualifier:
            assert_first_fn_visibility("mod m {\n    pub(super) fn helper() {}\n}\n", "pub(super)");
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
        let f = scan_single("#[cfg(test)]\npub fn helper_for_tests() {}\n");
        assert!(f.is_test, "{f:?}");
        assert_eq!(f.visibility, "pub");
    }

    #[test]
    fn an_out_of_line_mod_declaration_is_recorded_not_discarded() {
        let core = scan_str_core("mod helpers;\nfn a() {}\n");
        assert_eq!(core.out_of_line_mods.len(), 1);
        assert_eq!(core.out_of_line_mods[0].name, "helpers");
        assert!(!core.out_of_line_mods[0].is_test);
        assert_eq!(core.out_of_line_mods[0].path_override, None);
    }

    /// `src` declares exactly one out-of-line mod, flagged test; returns it.
    fn single_test_out_of_line_mod(src: &str) -> OutOfLineMod {
        let core = scan_str_core(src);
        assert_eq!(core.out_of_line_mods.len(), 1);
        let m = core.out_of_line_mods.into_iter().next().unwrap();
        assert!(m.is_test, "{m:?}");
        m
    }

    rigger::test_cases! {
        a_cfg_test_out_of_line_mod_is_flagged_test:
            single_test_out_of_line_mod("#[cfg(test)]\nmod contract;\n");
        /// `src/eventstore/mod.rs`'s real declaration: `#[cfg(test)]\npub mod contract;` - the
        /// exact regression this criterion's `pub` fix exists for.
        a_cfg_test_pub_out_of_line_mod_is_flagged_test_the_real_eventstore_mod_rs_shape: assert_eq!(
            single_test_out_of_line_mod("#[cfg(test)]\npub mod contract;\n").name,
            "contract"
        );
        /// Spec 93 criterion 1's real lib.rs shape for a store-only whole-module gate:
        /// `#[cfg(all(test, any(feature = "store", not(feature = "core"))))]` - a strict subset
        /// of `cfg(test)` (it can only ever compile when `cfg(test)` also holds), so it must
        /// classify as test-in-full exactly like the bare `#[cfg(test)]` case above. This is the
        /// regression `cfg_all_contains_bare_test` exists to close.
        a_cfg_all_test_and_feature_compound_out_of_line_mod_is_flagged_test_the_real_blast_radius_eval_shape:
            assert_eq!(
            single_test_out_of_line_mod(
                "#[cfg(all(test, any(feature = \"store\", not(feature = \"core\"))))]\n\
                 mod blast_radius_eval;\n",
            )
            .name,
            "blast_radius_eval"
        );
        /// `src/eventstore/mod.rs`'s real post-spec-93 declaration for `contract`.
        a_cfg_all_test_pub_mod_is_flagged_test_the_real_eventstore_contract_shape:
            single_test_out_of_line_mod(
            "#[cfg(all(test, any(feature = \"store\", not(feature = \"core\"))))]\n\
             pub mod contract;\n",
        );
    }

    rigger::test_cases! {
        /// `test` must be a BARE top-level clause of the `all(...)` - one nested one level
        /// deeper inside an `any(...)` changes the boolean meaning entirely (this predicate can
        /// be true even when NOT a test build, via the `debug_assertions` arm), so it must NOT
        /// be classified as test-in-full. Conservative non-recognition, not a misclassification.
        cfg_all_with_test_nested_inside_a_further_any_is_not_recognized:
            assert!(!cfg_all_contains_bare_test(
            "#[cfg(all(any(test, debug_assertions), feature = \"store\"))]"
        ));
        /// The exact inverse of the real shape - `not(test)` inside an `all(...)` means this
        /// compiles only OUTSIDE a test build, so recognizing it as test-in-full would be a real
        /// false positive, not just an over-broad guess. Must stay unrecognized.
        cfg_all_with_not_test_is_not_recognized: assert!(!cfg_all_contains_bare_test(
            "#[cfg(all(not(test), feature = \"x\"))]"
        ));
        /// `all(` reached from something other than `cfg(` (an arbitrary hypothetical attribute
        /// macro taking its own `all(...)` argument) must not be mistaken for a cfg predicate.
        cfg_all_on_an_unrelated_attribute_is_not_recognized:
            assert!(!cfg_all_contains_bare_test("#[other(all(test))]"));
    }

    rigger::test_cases! {
        an_out_of_line_mod_inherits_test_ness_from_an_enclosing_cfg_test_mod: assert_eq!(
            single_test_out_of_line_mod("#[cfg(test)]\nmod outer {\n    mod inner;\n}\n").name,
            "inner"
        );
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
            file: "crates/rigger-conductor/src/conductor.rs".to_string(),
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
            file: "crates/rigger-dash/src/dash.rs".to_string(),
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

    /// Classify conductor method `name`, declared in the impl block headed `impl_header`: it
    /// must land in `module`. Returns the classification's reason.
    fn classify_method(name: &str, impl_header: &str, module: &str) -> String {
        let mut f = conductor_fn(name);
        f.enclosing_impl = Some(impl_header.to_string());
        let (proposed, reason) = classify(&f);
        assert_eq!(proposed.as_deref(), Some(module));
        reason
    }

    rigger::test_cases! {
        a_method_is_classified_under_its_impl_self_type: {
            let reason = classify_method("summary", "GateRatchet", "conductor::gate_ratchet");
            assert!(reason.contains("GateRatchet"), "{reason}");
        };
        /// Regression: `impl<'a> RunCtx<'a> { ... }` must classify under `run_ctx`, not under an
        /// empty self-type bucket (the impl's OWN `<'a>` generic list is not the type name).
        a_method_on_a_generic_impl_block_strips_the_impls_own_leading_generics: {
            let reason = classify_method("for_test", "<'a> RunCtx<'a>", "conductor::run_ctx");
            assert!(reason.contains("RunCtx"), "{reason}");
            assert!(!reason.contains("`` methods"), "{reason}");
        };
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

    rigger::test_cases! {
        impl_self_type_still_handles_a_generic_self_type_with_a_where_clause:
            assert_eq!(impl_self_type("Foo<T> where T: Bar<Baz>"), "Foo");
        impl_self_type_handles_a_bound_generic_self_type:
            assert_eq!(impl_self_type("<T: Clone> Buckets<T>"), "Buckets");
        impl_self_type_handles_a_trait_impl_on_a_lifetime_generic_self_type:
            assert_eq!(impl_self_type("Trait for Server<'a>"), "Server");
        impl_self_type_handles_a_generic_trait_impl_on_a_generic_self_type: assert_eq!(
            impl_self_type("<'a> Trait<'a> for ReplayDriver<'a>"),
            "ReplayDriver"
        );
        impl_self_type_handles_a_const_generic_self_type: assert_eq!(
            impl_self_type("<const N: usize> Wrapper<[u8; N]>"),
            "Wrapper"
        );
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
    fn strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match() {
        // A Self type that merely CONTAINS "where" as a substring must survive untouched -
        // only a real `where` keyword at a word boundary is a clause start.
        assert_eq!(strip_trailing_where_clause("Somewhere"), "Somewhere");
        assert_eq!(strip_trailing_where_clause("Foo where T: Bar"), "Foo");
        assert_eq!(strip_trailing_where_clause("Foo"), "Foo");
    }

    rigger::test_cases! {
        a_trait_impl_method_is_classified_under_the_implementing_type_not_the_trait:
            classify_method("spawn", "AgentDriver for Stub", "conductor::stub");
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
    fn pluralize_uses_singular_only_at_exactly_one() {
        assert_eq!(pluralize(0, "function"), "0 functions");
        assert_eq!(pluralize(1, "function"), "1 function");
        assert_eq!(pluralize(2, "function"), "2 functions");
    }

    /// A non-test `crates/rigger-conductor/src/conductor.rs` map entry named `name` spanning `lines`, its content hash
    /// `hash-<name>`.
    fn conductor_map_entry(
        name: &str,
        lines: (usize, usize),
        proposed_module: Option<&str>,
        reason: &str,
    ) -> MapEntry {
        MapEntry {
            file: "crates/rigger-conductor/src/conductor.rs".to_string(),
            name: name.to_string(),
            start_line: lines.0,
            end_line: lines.1,
            is_test: false,
            proposed_module: proposed_module.map(str::to_string),
            reason: reason.to_string(),
            content_hash: format!("hash-{name}"),
        }
    }

    /// Section 1 rendered over `entries`, each citing its own line span.
    fn render_map(entries: &[MapEntry]) -> String {
        let lines: Vec<MapEntryLines> = entries.iter().map(map_entry_lines).collect();
        render_section_1(entries, &lines)
    }

    #[test]
    fn map_to_json_is_byte_identical_across_two_runs_over_the_same_input() {
        let entries = vec![conductor_map_entry(
            "a",
            (1, 2),
            Some("conductor::support"),
            "x",
        )];
        assert_eq!(
            ledger_json(&entries, map_entry_wire),
            ledger_json(&entries, map_entry_wire)
        );
    }

    #[test]
    fn section_1_names_every_module_and_every_unassigned_function() {
        let rendered = render_map(&[
            conductor_map_entry("a", (1, 2), Some("conductor::support"), "reason-a"),
            conductor_map_entry("b", (3, 4), None, "reason-b"),
        ]);
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
        let rendered = render_map(&[conductor_map_entry(
            "a",
            (1, 2),
            Some("conductor::support"),
            "r",
        )]);
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

    /// A section writer's output `updated` carries every `new` body, none of the `old` ones,
    /// and every `kept` neighbor byte-for-byte.
    fn assert_span_replaced(updated: &str, new: &[&str], old: &[&str], kept: &[&str]) {
        for body in new {
            assert!(updated.contains(body), "missing new {body:?}");
        }
        for body in old {
            assert!(!updated.contains(body), "kept old {body:?}");
        }
        for body in kept {
            assert!(updated.contains(body), "lost neighbor {body:?}");
        }
    }

    rigger::test_cases! {
        replace_section_1_only_touches_section_1_leaving_later_sections_intact: assert_span_replaced(
            &replace_section_1(
                "# Title\n\n## 1. Responsibility Map\n\nold body\n\n## 2. Duplication Catalog\n\nfilled in by u85c2\n",
                "## 1. Responsibility Map\n\nnew body\n",
            ),
            &["new body"],
            &["old body"],
            &["filled in by u85c2"],
        );
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

    rigger::test_cases! {
        /// THE DRIFT GUARD for `docs/audit/responsibility-map.json`: with `RIGGER_AUDIT_WRITE=1`
        /// set, regenerate and overwrite it; otherwise regenerate in memory and assert it matches
        /// the committed file byte-for-byte, so the catalog can never silently drift from the tree
        /// (spec 85 Design).
        responsibility_map_json_matches_the_tree_or_is_rewritten:
            assert_ledger_matches_the_tree_or_rewrite(
                &build_map(&repo_root()),
                MAP_PATH,
                MAP_LINES_PATH,
                map_entry_wire,
                map_entry_lines,
            );
    }

    /// Spec 90 criterion 2, CLAIM 1 for `docs/audit/responsibility-map.json`: structurally, no
    /// site carries `start_line`/`end_line` and every one carries a non-empty `content_hash`.
    #[test]
    fn the_real_committed_responsibility_map_carries_no_line_number_fields() {
        let map = build_map(&repo_root());
        let json = ledger_json(&map, map_entry_wire);
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

    rigger::test_cases! {
        /// Spec 90 criterion 2, CLAIM 2 for `docs/audit/responsibility-map.json`: a synthetic
        /// fixture tree (never the real checked-out one - [`build_map`] requires all three
        /// [`TARGET_FILES`] to exist, so this writes trivial stand-ins for the two it does not
        /// exercise) proves a pin bump (5 unrelated comment lines prepended to `crates/rigger-dash/src/dash.rs`,
        /// shifting every entry in it) leaves the guarded map byte-identical, because
        /// `content_hash` keys on each function's own raw text, never its line number.
        a_pin_bump_leaves_the_guarded_responsibility_map_byte_identical:
            assert_a_pin_bump_leaves_the_guarded_json_byte_identical(
                &[
                    ("crates/rigger-conductor/src/conductor.rs", "fn one() {}\n"),
                    ("src/cli/mod.rs", "fn two() {}\n"),
                    ("crates/rigger-dash/src/dash.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"),
                ],
                "crates/rigger-dash/src/dash.rs",
                |root| ledger_json(&build_map(root), map_entry_wire),
                "a pin bump that only shifts every entry's OWN line number must leave the guarded \
                 responsibility map byte-identical (spec 90 criterion 2)",
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

    rigger::test_cases! {
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
        two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry:
            assert_two_branches_never_perturb_an_existing_entry(
                &[
                    ("crates/rigger-conductor/src/conductor.rs", "fn one() {}\n"),
                    ("src/cli/mod.rs", "fn two() {}\n"),
                    ("crates/rigger-dash/src/dash.rs", "fn three() {}\n"),
                ],
                (
                    "crates/rigger-conductor/src/conductor.rs",
                    "fn one() {}\n\nfn branch_a_only() {\n    let _ = 1;\n}\n",
                ),
                (
                    "src/cli/mod.rs",
                    "fn two() {}\n\nfn branch_b_only() {\n    let _ = 2;\n}\n",
                ),
                |root| ledger_json(&build_map(root), map_entry_wire),
                ("entry", "entries"),
            );
    }

    /// Spec 90 Design, verbatim: "the report's guard checks structure only (sections present,
    /// counts equal to the catalog) rather than bytes." Mirrors
    /// `assert_section_2_structurally_matches` exactly, one level down (module ~ cluster,
    /// per-entry citation ~ per-site citation): the heading, the aggregate function-total
    /// count, every module's own heading with its declared function count, and the unassigned
    /// heading with its declared count - never the exact citation bytes, which are free to
    /// legitimately move between explicit `RIGGER_AUDIT_WRITE=1` regens (a pin bump anywhere
    /// in `crates/rigger-conductor/src/conductor.rs`, `src/cli/mod.rs` or `crates/rigger-dash/src/dash.rs`).
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
            let heading = format!("- `{module}` ({})", pluralize(count, "function"));
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
        let unassigned_heading = format!(
            "### Unassigned ({})",
            pluralize(unassigned_count, "function")
        );
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
        let section_1 = render_map(&map);
        let path = root.join(REPORT_PATH);
        if audit_write_mode() {
            let _guard = lock_report_write();
            let updated = match fs::read_to_string(&path) {
                Ok(text) => replace_section_1(&text, &section_1),
                Err(_) => assemble_fresh_report(&section_1),
            };
            write_creating_parent(&path, &updated);
            return;
        }
        let committed = committed_report(&path);
        let span = section_span(&committed, "## 1. ");
        assert_section_1_structurally_matches(&committed[span], &map);
    }

    /// The report as it stands at `path`, or - absent one - a fresh report around the tree's
    /// own section 1 (the base every later section's writer patches its own span into).
    fn report_or_fresh(root: &Path, path: &Path) -> String {
        fs::read_to_string(path)
            .unwrap_or_else(|_| assemble_fresh_report(&render_map(&build_map(root))))
    }

    /// The committed report at `path`, which every drift guard's check mode requires.
    fn committed_report(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|_| {
            panic!("{REPORT_PATH} is missing - run with RIGGER_AUDIT_WRITE=1 to generate it")
        })
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
            content_hash: "hash-a".to_string(),
            ..conductor_map_entry("add_one", (1, 3), Some("conductor::support"), "reason-a")
        }];
        let before_rendered = render_map(&before);

        let after = vec![MapEntry {
            start_line: 6,
            end_line: 8,
            ..before[0].clone()
        }];
        let after_rendered = render_map(&after);

        assert_section_1_structurally_matches(&before_rendered, &before);
        assert_section_1_structurally_matches(&after_rendered, &after);
        assert!(before_rendered.contains("`crates/rigger-conductor/src/conductor.rs:1-3`"));
        assert!(after_rendered.contains("`crates/rigger-conductor/src/conductor.rs:6-8`"));
        assert!(!after_rendered.contains("`crates/rigger-conductor/src/conductor.rs:1-3`"));
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

    /// The `Lit` tokens `src` lexes to are exactly `want`, in order.
    fn assert_lit_tokens(src: &str, want: &[&str]) {
        let toks = tok(src);
        let lits: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == RawKind::Lit)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(lits, want);
    }

    rigger::test_cases! {
        string_and_raw_string_literals_are_one_lit_token_each: assert_lit_tokens(
            r####"fn a() { let s = "hi"; let r = r#"raw ) thing"#; }"####,
            &["\"hi\"", "r#\"raw ) thing\"#"],
        );
    }

    #[test]
    fn a_char_literal_is_lit_and_a_bare_tick_ident_is_a_lifetime() {
        let toks = tok("fn a<'x>(c: char) { let z = 'y'; }");
        let lifetime = toks.iter().find(|t| t.text == "'x").unwrap();
        assert_eq!(lifetime.kind, RawKind::Lifetime);
        let ch = toks.iter().find(|t| t.text == "'y'").unwrap();
        assert_eq!(ch.kind, RawKind::Lit);
    }

    rigger::test_cases! {
        number_literals_including_a_fraction_are_lit_tokens:
            assert_lit_tokens("fn a() { let x = 1_000u32; let y = 1.5; }", &["1_000u32", "1.5"]);
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

    /// A fresh scratch fixture tree holding `files` (`(relative path, content)` pairs).
    fn fixture_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        for (rel, content) in files {
            write_fixture(dir.path(), rel, content);
        }
        dir
    }

    /// `read` over a fixture tree holding `files`.
    fn on_fixture<T>(read: impl FnOnce(&Path) -> T, files: &[(&str, &str)]) -> T {
        read(fixture_tree(files).path())
    }

    /// `files` with the entry at `edit.0` replaced by `edit.1`'s content.
    fn with_file<'a>(
        files: &[(&'a str, &'a str)],
        edit: (&'a str, &'a str),
    ) -> Vec<(&'a str, &'a str)> {
        files
            .iter()
            .map(|&(rel, content)| if rel == edit.0 { edit } else { (rel, content) })
            .collect()
    }

    /// `RIGGER_AUDIT_WRITE=1`: every drift guard regenerates its artifact instead of checking it.
    fn audit_write_mode() -> bool {
        std::env::var("RIGGER_AUDIT_WRITE").as_deref() == Ok("1")
    }

    /// Write `contents` to `path`, creating its parent directory first.
    fn write_creating_parent(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    /// One guarded ledger's drift guard: in [`audit_write_mode`], regenerate `guarded` (the
    /// line-free wire JSON of `items`) and its unguarded `lines` sibling; otherwise the committed
    /// `guarded` file must equal the tree's render byte-for-byte. The `lines` sibling is never
    /// read back (spec 90 criterion 2 Design: "the guard NEVER compares").
    fn assert_ledger_matches_the_tree_or_rewrite<T, W: Serialize, L: Serialize>(
        items: &[T],
        guarded: &str,
        lines: &str,
        to_wire: impl Fn(&T) -> W,
        to_lines: impl Fn(&T) -> L,
    ) {
        let root = repo_root();
        let json = ledger_json(items, to_wire);
        let path = root.join(guarded);
        if audit_write_mode() {
            write_creating_parent(&path, &json);
            fs::write(root.join(lines), ledger_json(items, to_lines)).unwrap();
            return;
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{guarded} is missing or unreadable ({e}) - run with RIGGER_AUDIT_WRITE=1 to \
                 generate it"
            )
        });
        assert_eq!(
            committed, json,
            "{guarded} has drifted from the tree - regenerate with RIGGER_AUDIT_WRITE=1"
        );
    }

    /// Five unrelated comment lines a pin bump prepends to a file.
    const PIN_BUMP: &str = "// pin: v1\n// pin: v2\n// pin: v3\n// pin: v4\n// pin: v5\n";

    /// Spec 90 criterion 2, CLAIM 2: over the `base` fixture tree, a pin bump of the file at
    /// `bumped` (shifting every line in it) leaves `render`'s guarded JSON byte-identical.
    fn assert_a_pin_bump_leaves_the_guarded_json_byte_identical(
        base: &[(&str, &str)],
        bumped: &str,
        render: impl Fn(&Path) -> String,
        message: &str,
    ) {
        let dir = fixture_tree(base);
        let base_json = render(dir.path());
        let (_, content) = base.iter().find(|(rel, _)| *rel == bumped).unwrap();
        write_fixture(dir.path(), bumped, &format!("{PIN_BUMP}{content}"));
        assert_eq!(base_json, render(dir.path()), "{message}");
    }

    /// Spec 90 criterion 2, CLAIM 3 for a per-entry artifact: two branches of the `base`
    /// fixture tree, each replacing one DIFFERENT file (`branch_a`, `branch_b`) to add one
    /// unrelated function, keep every pre-existing entry of `render`'s JSON array byte-identical
    /// and each contribute exactly one new, distinct entry - nothing for a real merge to
    /// conflict over. `(noun, nouns)` names an entry in the failure messages.
    fn assert_two_branches_never_perturb_an_existing_entry(
        base: &[(&str, &str)],
        branch_a: (&str, &str),
        branch_b: (&str, &str),
        render: impl Fn(&Path) -> String,
        (noun, nouns): (&str, &str),
    ) {
        let entries_of =
            |files: &[(&str, &str)]| json_array_entries(&render(fixture_tree(files).path()));
        let base_entries = entries_of(base);
        let a_entries = entries_of(&with_file(base, branch_a));
        let b_entries = entries_of(&with_file(base, branch_b));

        assert_eq!(
            a_entries.len(),
            base_entries.len() + 1,
            "branch A must contribute exactly one new {noun}"
        );
        assert_eq!(
            b_entries.len(),
            base_entries.len() + 1,
            "branch B must contribute exactly one new {noun}"
        );
        for entry in &base_entries {
            assert!(
                a_entries.contains(entry),
                "branch A perturbed or dropped a pre-existing {noun} {entry}"
            );
            assert!(
                b_entries.contains(entry),
                "branch B perturbed or dropped a pre-existing {noun} {entry}"
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
        assert_eq!(a_new.len(), 1, "branch A's own new {noun}: {a_new:?}");
        assert_eq!(b_new.len(), 1, "branch B's own new {noun}: {b_new:?}");
        assert_ne!(
            a_new[0], b_new[0],
            "the two branches' new {nouns} must be distinct - nothing for a real merge to \
             conflict over"
        );
    }

    #[test]
    fn scan_tree_finds_functions_under_both_src_and_tests_but_not_elsewhere() {
        let files = on_fixture(
            scan_tree,
            &[
                ("src/a.rs", "fn one() {}\n"),
                ("tests/b.rs", "fn two() {}\n"),
                ("docs/c.rs", "fn three() {}\n"),
            ],
        );
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
        let files = on_fixture(
            scan_tree,
            &[(
                "src/nested.rs",
                "fn outer() {\n    fn inner() {}\n    inner();\n}\n",
            )],
        );
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
        let files = on_fixture(
            scan_tree,
            &[
                ("src/a.rs", "fn included_one() {}\n"),
                (
                    ADVERSARIAL_SAMPLE_EXCLUDED_FILE,
                    "fn excluded_one() {}\nfn excluded_two() {}\n",
                ),
            ],
        );

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
        let files = on_fixture(
            scan_tree,
            &[
                ("src/a.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"),
                ("src/b.rs", "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n"),
            ],
        );
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].classification, "exact");
        assert_eq!(clusters[0].sites.len(), 2);
        let names: HashSet<&str> = clusters[0].sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, HashSet::from(["add_one", "plus_one"]));
    }

    #[test]
    fn two_unrelated_functions_form_no_cluster() {
        let files = on_fixture(scan_tree, &[("src/a.rs", "fn read_config(path: &str) -> String {\n    std::fs::read_to_string(path).unwrap()\n}\n"), ("src/b.rs", "fn sum_all(xs: &[i64]) -> i64 {\n    xs.iter().sum()\n}\n")]);
        assert!(clusters_for(&files).is_empty());
    }

    #[test]
    fn three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home() {
        // Same overall shape (read a /proc file, grab a field after the comm's closing paren)
        // but each extracts a DIFFERENT field - near, not exact (spec 78's dash.rs/reap.rs
        // /proc-stat class this catalog's mandatory sweep also names explicitly).
        let files = on_fixture(scan_tree, &[
            (
                "src/one.rs",
                "fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n",
            ),
            (
                "src/two.rs",
                "fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n",
            ),
            (
                "src/three.rs",
                "fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n",
            ),
        ]);
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].sites.len(), 3);
        assert_eq!(clusters[0].classification, "near");
        assert!(!clusters[0].proposed_home.is_empty());
    }

    /// A near-duplicate pair whose every shared shingle is ALSO carried by many unrelated
    /// functions (the common-boilerplate shape a struct builder's `..Default::default() }`
    /// produces) must still cluster: adding a helper anywhere in the tree can never change
    /// another cluster's membership. `fillers` unrelated functions each open with the pair's
    /// whole shared body and then diverge far enough to fall below the threshold themselves.
    fn near_pair_clusters_beside(fillers: usize) {
        const OPS: [&str; 12] = ["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>", "+", "*"];
        let body: String = OPS
            .iter()
            .map(|op| format!("    let v = v {op} 1;\n"))
            .collect();
        let pair_fn = |name: &str, last: &str| {
            format!("fn {name}(v: u32) -> u32 {{\n{body}    v {last} 1\n}}\n")
        };
        let mut src = pair_fn("alpha", "+") + &pair_fn("beta", "-");
        // Every filler carries each shingle alpha and beta share (the whole body, then the
        // closing `v`), followed by an unrelated tail long enough to keep it below the threshold.
        let tail = "    if v > 3 { return v; }\n    while v < 2 { break; }\n    \
                    for i in 0..v { let _ = i; }\n    match v { 0 => return 1, _ => {} }\n    \
                    loop { break; }\n    let w: Vec<u32> = vec![v];\n    let s = &w[..];\n    \
                    assert!(!s.is_empty());\n    let t = (v, v);\n    let (a, b) = t;\n    \
                    let v = a.max(b);\n";
        for k in 0..fillers {
            let pad = "    let v = v;\n".repeat(k);
            src +=
                &format!("fn filler_{k}(v: u32) -> u32 {{\n{body}    v;\n{tail}{pad}    v\n}}\n");
        }
        let clusters = clusters_for(&on_fixture(scan_tree, &[("src/a.rs", src.as_str())]));
        let names_with_alpha: HashSet<&str> = clusters
            .iter()
            .find(|c| c.sites.iter().any(|s| s.name == "alpha"))
            .map(|c| c.sites.iter().map(|s| s.name.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            names_with_alpha,
            HashSet::from(["alpha", "beta"]),
            "the alpha/beta near pair must cluster alone beside {fillers} unrelated fillers"
        );
    }

    rigger::test_cases! {
        a_near_pair_clusters_beside_a_few_functions_sharing_its_shingles:
            near_pair_clusters_beside(3);
        a_near_pair_still_clusters_when_many_functions_share_every_one_of_its_shingles:
            near_pair_clusters_beside(40);
    }

    #[test]
    fn cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster() {
        let files = on_fixture(
            scan_tree,
            &[
                ("src/z.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"),
                ("src/a.rs", "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n"),
            ],
        );
        let clusters = clusters_for(&files);
        assert_eq!(clusters.len(), 1);
        // src/a.rs sorts before src/z.rs regardless of scan/write order.
        assert_eq!(clusters[0].sites[0].file, "src/a.rs");
        assert_eq!(clusters[0].sites[1].file, "src/z.rs");
    }

    #[test]
    fn same_impl_block_duplicate_methods_propose_that_types_home() {
        let clusters = clusters_for(&on_fixture(scan_tree, &[(
            "src/w.rs",
            "impl Widget {\n    fn add_one(n: u32) -> u32 {\n        n + 1\n    }\n    fn plus_one(m: u32) -> u32 {\n        m + 1\n    }\n}\n",
        )]));
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].proposed_home, "w::widget");
    }

    // -------------------------------------------------------------------------------------
    // The five mandatory sweeps
    // -------------------------------------------------------------------------------------

    #[test]
    fn command_new_sweep_finds_a_call_site_and_names_it() {
        let files = on_fixture(
            scan_tree,
            &[(
                "src/a.rs",
                "fn run() {\n    let _ = std::process::Command::new(\"true\").status();\n}\n",
            )],
        );
        let hits = find_ident_path_call_sites(&files, "Command", &["new"]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Command::new");
        assert_eq!(hits[0].file, "src/a.rs");
    }

    #[test]
    fn command_new_sweep_ignores_an_unrelated_call() {
        let files = on_fixture(
            scan_tree,
            &[("src/a.rs", "fn run() {\n    Other::new();\n}\n")],
        );
        assert!(find_ident_path_call_sites(&files, "Command", &["new"]).is_empty());
    }

    /// The port gate's detection: a production fn's `Command::new` is flagged, while the same
    /// call inside a `#[cfg(test)]` mod, or inside the port module itself, is not.
    #[test]
    fn a_production_spawn_outside_the_port_is_flagged_and_test_or_port_spawns_are_not() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        let spawn = "fn run() {\n    let _ = std::process::Command::new(\"true\").status();\n}\n";
        write_fixture(dir.path(), "src/a.rs", spawn);
        write_fixture(dir.path(), PROCESS_SPAWN_PORT, spawn);
        write_fixture(
            dir.path(),
            "src/b.rs",
            &format!("#[cfg(test)]\nmod tests {{\n    {spawn}}}\n"),
        );
        let files = scan_tree(dir.path());
        let hits = production_spawns_outside_the_port(&files, &BTreeSet::new());
        let files_hit: Vec<&str> = hits.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(files_hit, vec!["src/a.rs"], "{hits:?}");
    }

    /// THE PROCESS-SPAWN PORT GATE: on the real tree, [`PROCESS_SPAWN_PORT`] is the only
    /// production caller of `Command::new` - every production spawn routes through it.
    #[test]
    fn the_process_spawn_port_is_the_only_production_command_new_caller() {
        let outside: Vec<String> =
            production_spawns_outside_the_port(real_files(), real_whole_file_test_set())
                .iter()
                .map(|s| format!("{}:{}", s.file, s.start_line))
                .collect();
        assert!(
            outside.is_empty(),
            "production Command::new sites outside {PROCESS_SPAWN_PORT} - route each through the \
             port: {outside:?}"
        );
    }

    #[test]
    fn sqlite_open_sweep_matches_either_tail() {
        let files = on_fixture(scan_tree, &[("src/a.rs", "fn a() {\n    Connection::open(p)?;\n}\nfn b() {\n    Connection::open_with_flags(p, f)?;\n}\n")]);
        let hits = find_ident_path_call_sites(&files, "Connection", &["open", "open_with_flags"]);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one() {
        let files = on_fixture(scan_tree, &[("src/a.rs", "fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n")]);
        let hits = find_literal_containing(&files, "/proc");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].name.contains("/proc"));
    }

    #[test]
    fn rigger_path_literal_sweep_finds_a_rigger_relative_string() {
        let files = on_fixture(
            scan_tree,
            &[("src/a.rs", "fn a() {\n    let _ = \".rigger/tmp\";\n}\n")],
        );
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
        let files = on_fixture(scan_tree, &[(
            "src/a.rs",
            "fn shape_error(e: &str) -> String {\n    format!(\"error: {}\", e)\n}\nfn is_err_only(x: &Result<(), ()>) -> bool {\n    x.is_err()\n}\n",
        )]);
        let hits = find_error_shaping_fns(&files, &all_fn_refs(&files));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "shape_error");
    }

    #[test]
    fn build_sweep_clusters_always_returns_exactly_five_named_clusters() {
        let files = on_fixture(scan_tree, &[("src/a.rs", "fn a() {}\n")]);
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

    /// Over the `files` fixture tree, `sweep` finds exactly the functions named `expected`.
    fn assert_sweep_finds(
        files: &[(&str, &str)],
        sweep: fn(&[FileScan], &[FnRef]) -> Vec<DupSite>,
        expected: &[&str],
    ) {
        let scanned = on_fixture(scan_tree, files);
        let hits = sweep(&scanned, &all_fn_refs(&scanned));
        let names: HashSet<&str> = hits.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, expected.iter().copied().collect::<HashSet<&str>>());
    }

    rigger::test_cases! {
        proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn:
            assert_sweep_finds(
            &[(
                "src/a.rs",
                "fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n",
            )],
            find_proc_stat_or_status_readers,
            &["state_of", "ppid_of"],
        );
    }

    /// The real catalog's cluster holding a site named `name`.
    fn real_cluster_hosting(name: &str) -> &'static DupCluster {
        real_catalog()
            .iter()
            .find(|c| c.sites.iter().any(|s| s.name == name))
            .unwrap_or_else(|| panic!("{name} is findable in the real catalog"))
    }

    /// THE WORKED EXAMPLE (spec 85 Goal), retired: `dash.rs::process_state` and
    /// `reap.rs::pid_starttime` used to each re-derive the `/proc/<pid>/stat` split and landed in
    /// the reader sweep together. Both now read through `reap::stat_field_after_comm`, so on the
    /// real tree the sweep carries that one parser and neither former re-deriver.
    #[test]
    fn the_dash_reap_proc_stat_pair_reads_through_the_one_stat_parser() {
        let readers = sweep_cluster_named(real_catalog(), PROC_STAT_READERS_SWEEP)
            .expect("the /proc reader sweep is catalogued");
        let names: HashSet<&str> = readers.sites.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains("stat_field_after_comm")
                && !names.contains("process_state")
                && !names.contains("pid_starttime"),
            "the /proc reader sweep {:?} must carry the one stat parser and neither former \
             re-deriver, found: {names:?}",
            readers.id
        );
    }

    /// Each `(src, expected)` case: whether `src` constructs its own `Widget` literal.
    fn assert_constructs_own_widget_literal(cases: &[(&str, bool)]) {
        for (src, expected) in cases {
            assert_eq!(
                constructs_own_type_literal(&tok(src), "Widget"),
                *expected,
                "{src}"
            );
        }
    }

    rigger::test_cases! {
        constructs_own_type_literal_matches_self_and_the_named_type_but_not_an_unrelated_call:
            assert_constructs_own_widget_literal(&[
            ("fn ok() -> Self { Self { a: 1, b: 2 } }", true),
            ("fn ok() -> Widget { Widget { a: 1 } }", true),
            ("fn ok() -> Widget { other_fn(1, 2) }", false),
        ]);
        /// `Self { a, b: 0 }` - the first field is SHORTHAND (no `:`), a real shape
        /// (`SpawnResult`'s own constructors use it) the `:`-only check would miss; and a
        /// single-field shorthand literal (`{ a }`) still closes on `}`, not `:`/`,`.
        constructs_own_type_literal_matches_shorthand_field_init_too:
            assert_constructs_own_widget_literal(&[
            ("fn ok(a: u32) -> Self { Self { a, b: 0 } }", true),
            ("fn ok(a: u32) -> Self { Self { a } }", true),
        ]);
    }

    #[test]
    fn parallel_constructor_sweep_groups_two_constructors_for_one_type_but_not_a_lone_one() {
        let files = on_fixture(scan_tree, &[("src/a.rs", "impl Widget {\n    fn ok(a: u32) -> Self {\n        Self { a, b: 0 }\n    }\n    fn zeroed() -> Self {\n        Self { a: 0, b: 0 }\n    }\n}\nimpl Gadget {\n    fn only() -> Self {\n        Self { x: 1 }\n    }\n}\n")]);
        let refs = all_fn_refs(&files);
        let clusters = find_parallel_constructor_clusters(&files, &refs);
        assert_eq!(clusters.len(), 1);
        let names: HashSet<&str> = clusters[0].sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, HashSet::from(["ok", "zeroed"]));
        assert!(clusters[0].proposed_home.contains("widget"));
    }

    /// The real catalog's cluster hosting `file::name` also contains both `companions`.
    fn assert_real_cluster_of_holds(file: &str, name: &str, companions: [&str; 2]) {
        let clusters = real_catalog();
        let hosting = clusters
            .iter()
            .find(|c| c.sites.iter().any(|s| s.file == file && s.name == name))
            .unwrap_or_else(|| panic!("{name} is findable in the real catalog"));
        let names: HashSet<&str> = hosting.sites.iter().map(|s| s.name.as_str()).collect();
        let [first, second] = companions;
        assert!(
            names.contains(first) && names.contains(second),
            "{name}'s cluster {:?} must also contain {first} and {second}, found: {:?}",
            hosting.id,
            names
        );
    }

    /// A SECOND, DEEPER worked example from the same adversarial sample draw, retired:
    /// `spawn::SpawnResult::liveness_fault` read MISSING from the mechanical catalog (its extra
    /// `class` parameter and meta field pushed its Jaccard similarity to `ok`/`failed` just under
    /// threshold) though it was a third parallel constructor - the recall gap the
    /// parallel-constructor sweep closes. All three now delegate to one canonical constructor,
    /// so on the real tree no parallel-constructor cluster names a `src/spawn.rs` site.
    #[test]
    fn the_spawn_result_constructor_triple_is_one_canonical_constructor() {
        let prefix = "mandatory sweep: parallel constructor functions - ";
        let spawn_constructors: Vec<&str> = real_catalog()
            .iter()
            .filter(|c| c.note.starts_with(prefix))
            .flat_map(|c| c.sites.iter())
            .filter(|s| s.file == "crates/rigger-domain/src/spawn.rs")
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            spawn_constructors.is_empty(),
            "SpawnResult's constructors must all delegate to one canonical constructor, found \
             parallel ones: {spawn_constructors:?}"
        );
    }

    #[test]
    fn same_named_helper_sweep_groups_across_files_but_not_within_one_file_or_below_the_length_floor(
    ) {
        let files = on_fixture(
            scan_tree,
            &[
                (
                    "tests/a.rs",
                    "fn exploration_graph() -> u32 {\n    1\n}\nfn new() -> u32 {\n    2\n}\n",
                ),
                (
                    "tests/b.rs",
                    "fn exploration_graph() -> u32 {\n    3\n}\nfn new() -> u32 {\n    4\n}\n",
                ),
            ],
        );
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
        let hosting = real_cluster_hosting("exploration_graph");
        let files: HashSet<&str> = hosting.sites.iter().map(|s| s.file.as_str()).collect();
        assert!(
            files.contains("tests/dash_exploration_route_client_contract.rs")
                && files.contains("tests/dash_kg_graph_route.rs"),
            "exploration_graph's cluster {:?} must span both files, found: {:?}",
            hosting.id,
            files
        );
    }

    /// The same-named-helper sweep's clusters over the `files` fixture tree.
    fn same_named_helper_clusters(files: &[(&str, &str)]) -> Vec<DupCluster> {
        let scanned = on_fixture(scan_tree, files);
        find_same_named_helper_functions(&scanned, &all_fn_refs(&scanned))
    }

    /// Over the `files` fixture tree, the same-named-helper sweep flags nothing; `why` says
    /// which trait-required shape it must not flag.
    fn assert_no_same_named_helper_cluster(files: &[(&str, &str)], why: &str) {
        let clusters = same_named_helper_clusters(files);
        assert!(
            clusters.is_empty(),
            "{why} must not be flagged as same-named-helper duplication: {clusters:?}"
        );
    }

    rigger::test_cases! {
        /// Three concrete adapters implementing the SAME trait method - required by the trait
        /// contract, not a coincidental duplicate (the adjudicator-upheld precision defect:
        /// `subscribe_all`/`subscribe_stream` across the `EventStore` trait's backend adapters).
        same_named_helper_sweep_excludes_required_trait_impl_methods_across_adapters:
            assert_no_same_named_helper_cluster(
            &[
                (
                    "src/a.rs",
                    "impl MyPort for AdapterOne {\n    fn do_the_shared_thing(&self) -> u32 {\n        1\n    }\n}\n",
                ),
                (
                    "src/b.rs",
                    "impl MyPort for AdapterTwo {\n    fn do_the_shared_thing(&self) -> u32 {\n        2\n    }\n}\n",
                ),
                (
                    "src/c.rs",
                    "impl MyPort for AdapterThree {\n    fn do_the_shared_thing(&self) -> u32 {\n        3\n    }\n}\n",
                ),
            ],
            "required trait-impl methods across 2+ adapters",
        );
        /// A trait's own DEFAULT method (`enclosing_impl` is `None`) next to a concrete override
        /// and a test double - the adjudicator-upheld precision defect's other committed shape
        /// (`blast_radius` across the `Grounder` trait's default, the symbols grounder's
        /// override, and a test double).
        same_named_helper_sweep_excludes_a_trait_default_method_and_its_override:
            assert_no_same_named_helper_cluster(
            &[
                (
                    "src/a.rs",
                    "trait MyPort {\n    fn compute_the_radius(&self) -> u32 {\n        1\n    }\n}\n",
                ),
                (
                    "src/b.rs",
                    "impl MyPort for RealAdapter {\n    fn compute_the_radius(&self) -> u32 {\n        2\n    }\n}\n",
                ),
                (
                    "tests/mock.rs",
                    "impl MyPort for MockAdapter {\n    fn compute_the_radius(&self) -> u32 {\n        3\n    }\n}\n",
                ),
            ],
            "a trait's default method next to its override(s)",
        );
    }

    #[test]
    fn same_named_helper_sweep_still_catches_two_inherent_impls_sharing_a_method_name() {
        // Two UNRELATED inherent impls (no trait, no `" for "` in either header) that happen to
        // share a long method name are still a real coincidental duplicate - the exclusion above
        // must not blanket-suppress every impl-method same-name hit, only the trait-required
        // shape.
        let clusters = same_named_helper_clusters(&[
            (
                "src/a.rs",
                "impl Widget {\n    fn compute_the_layout(&self) -> u32 {\n        1\n    }\n}\n",
            ),
            (
                "src/b.rs",
                "impl Gadget {\n    fn compute_the_layout(&self) -> u32 {\n        2\n    }\n}\n",
            ),
        ]);
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

    rigger::test_cases! {
        bespoke_lexer_sweep_finds_the_named_trio_but_not_an_unrelated_fn: assert_sweep_finds(
            &[
                (
                    "tests/simplification_audit.rs",
                    "fn scan_file() {}\nfn unrelated() {}\n",
                ),
                ("tests/common/source_audit.rs", "pub fn tokenize() {}\n"),
                ("crates/rigger-grounder/src/grounder/symbols/extract.rs", "pub fn extract() {}\n"),
            ],
            find_bespoke_lexer_vs_canonical_extractor,
            &["scan_file", "tokenize", "extract"],
        );
        bespoke_lexer_sweep_finds_neither_lexer_name_in_the_other_lexer_file: assert_sweep_finds(
            &[
                ("tests/simplification_audit.rs", "fn tokenize() {}\n"),
                ("tests/common/source_audit.rs", "pub fn scan_file() {}\n"),
            ],
            find_bespoke_lexer_vs_canonical_extractor,
            &[],
        );
    }

    rigger::test_cases! {
        /// The recall gap u85c1's architecture lens routed to this criterion by name across two
        /// prior review rounds, verified closed on the REAL tree: `scan_file` (this file's own
        /// bespoke scanner), `tokenize` (the source audits' shared lexer) and `extract`
        /// (`crates/rigger-grounder/src/grounder/symbols/extract.rs`, the
        /// codebase's one canonical tree-sitter extractor) land in one cluster.
        the_bespoke_lexer_and_canonical_extractor_the_lens_routed_land_in_one_real_cluster:
            assert_real_cluster_of_holds(
            "crates/rigger-grounder/src/grounder/symbols/extract.rs",
            "extract",
            ["scan_file", "tokenize"],
        );
    }

    // -------------------------------------------------------------------------------------
    // build_catalog / catalog_to_json / render_section_2 / replace_section_2
    // -------------------------------------------------------------------------------------

    /// A cluster's id is derived from its own content, never its position: removing one
    /// cluster from the tree (here the pair sorting FIRST, so a positional scheme would shift
    /// every later id) leaves every surviving cluster's id unchanged, and each id reads
    /// `dup-<12 lowercase hex>`.
    #[test]
    fn removing_a_cluster_keeps_every_other_clusters_id() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        let removed_pair = "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n";
        write_fixture(dir.path(), "src/a.rs", removed_pair);
        write_fixture(dir.path(), "src/b.rs", removed_pair);
        let kept_pair = "fn twice(s: &str) -> String {\n    format!(\"{s}{s}\")\n}\n";
        write_fixture(dir.path(), "src/y.rs", kept_pair);
        write_fixture(dir.path(), "src/z.rs", kept_pair);
        let before = build_catalog(&scan_tree(dir.path()));

        write_fixture(dir.path(), "src/b.rs", "fn unrelated() {}\n");
        let after = build_catalog(&scan_tree(dir.path()));

        assert_eq!(
            after.len() + 1,
            before.len(),
            "exactly the one pair must disappear"
        );
        let id_shape = regex::Regex::new(r"^dup-[0-9a-f]{12}$").expect("a valid pattern");
        for c in &after {
            assert!(
                id_shape.is_match(&c.id),
                "id {:?} is not dup-<12 hex>",
                c.id
            );
            let twin = before
                .iter()
                .find(|b| b.sites == c.sites && b.note == c.note)
                .unwrap_or_else(|| panic!("cluster {} has no counterpart before", c.id));
            assert_eq!(twin.id, c.id, "a surviving cluster's id must not move");
        }
    }

    #[test]
    fn build_catalog_orders_clusters_by_their_first_site() {
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
        let keys: Vec<(String, usize)> = clusters.iter().map(cluster_sort_key).collect();
        let mut sorted_keys = keys.clone();
        sorted_keys.sort();
        assert_eq!(
            keys, sorted_keys,
            "clusters must already be in first-site order"
        );
        // The mechanical exact cluster (src/a.rs, src/z.rs) sorts before every sweep cluster
        // whose sites all live under src/a.rs alone (a fixture with no Command::new etc.), so
        // it is exactly one of the returned clusters and its own site order is (a.rs, z.rs).
        let mech = clusters
            .iter()
            .find(|c| c.classification == "exact")
            .expect("the renamed-identical pair forms an exact cluster");
        assert_eq!(mech.sites[0].file, "src/a.rs");
    }

    fn one_site_cluster(id: &str, file: &str) -> DupCluster {
        DupCluster {
            id: id.to_string(),
            classification: "exact".to_string(),
            sites: vec![DupSite {
                file: file.to_string(),
                start_line: 1,
                end_line: 3,
                name: "a".to_string(),
                content_hash: "deadbeefcafef00d".to_string(),
            }],
            proposed_home: "a::support".to_string(),
            note: "n".to_string(),
            disposition: None,
        }
    }

    /// A recorded disposition marks exactly the cluster it names, and the catalog then carries
    /// it as that entry's `disposition`; an undispositioned entry carries no such key.
    #[test]
    fn a_recorded_disposition_marks_only_the_cluster_it_names() {
        let mut clusters = vec![
            one_site_cluster("dup-aaaaaaaaaaaa", "src/a.rs"),
            one_site_cluster("dup-bbbbbbbbbbbb", "src/b.rs"),
        ];
        let dispositions = vec![Disposition {
            target: DispositionTarget::Cluster {
                id: "dup-bbbbbbbbbbbb".to_string(),
            },
            disposition: NOT_A_DUPLICATE.to_string(),
            reason: "same token shape, different meaning".to_string(),
        }];
        apply_dispositions(&mut clusters, &dispositions).expect("the id names a cluster");
        assert_eq!(clusters[0].disposition, None);
        assert_eq!(clusters[1].disposition.as_deref(), Some(NOT_A_DUPLICATE));
        let json = ledger_json(&clusters, dup_cluster_wire);
        assert_eq!(
            json.matches("\"disposition\": \"not-a-duplicate\"").count(),
            1
        );
    }

    /// A disposition naming no catalogued cluster is stale (its cluster was closed or changed)
    /// and is refused, naming the id, rather than silently kept.
    #[test]
    fn a_disposition_naming_no_cluster_is_refused() {
        let mut clusters = vec![one_site_cluster("dup-aaaaaaaaaaaa", "src/a.rs")];
        let dispositions = vec![Disposition {
            target: DispositionTarget::Cluster {
                id: "dup-cccccccccccc".to_string(),
            },
            disposition: NOT_A_DUPLICATE.to_string(),
            reason: "r".to_string(),
        }];
        let err = apply_dispositions(&mut clusters, &dispositions).unwrap_err();
        assert!(err.contains("dup-cccccccccccc"), "{err}");
    }

    /// A live citation's drift guard compares words, not figures: every digit run - a line
    /// number, a span, a count - reads as `#`, so a moved citation still matches its prose.
    #[test]
    fn without_figures_masks_every_digit_run() {
        assert_eq!(
            without_figures("`src/a.rs:12-340` and 28 hits at line 7"),
            "`src/a.rs:#-#` and # hits at line #"
        );
        let prose = "no figures here";
        assert_eq!(
            without_figures(prose),
            prose,
            "prose without digits is untouched"
        );
    }

    /// A disposition may name a mandatory sweep instead of an id: the sweep's cluster re-hashes
    /// whenever any of its sites changes, so the sweep name is the stable handle. It resolves
    /// the same way citations look a sweep up; an unknown or ambiguous sweep name is refused.
    #[test]
    fn a_disposition_may_name_a_mandatory_sweep() {
        let mut clusters = vec![
            one_site_cluster("dup-aaaaaaaaaaaa", "src/a.rs"),
            sweep_cluster(
                "Connection::open",
                one_site_cluster("x", "src/b.rs").sites,
                "home",
            ),
        ];
        let dispositions: Vec<Disposition> = serde_json::from_str(
            r#"[{"sweep": "Connection::open", "disposition": "not-a-duplicate", "reason": "r"}]"#,
        )
        .expect("a sweep-named disposition parses");
        apply_dispositions(&mut clusters, &dispositions).expect("the sweep is catalogued");
        assert_eq!(clusters[0].disposition, None);
        assert_eq!(clusters[1].disposition.as_deref(), Some(NOT_A_DUPLICATE));

        let unknown: Vec<Disposition> = serde_json::from_str(
            r#"[{"sweep": "no such sweep", "disposition": "not-a-duplicate", "reason": "r"}]"#,
        )
        .unwrap();
        let err = apply_dispositions(&mut clusters, &unknown).unwrap_err();
        assert!(err.contains("no such sweep"), "{err}");

        clusters.push(sweep_cluster("Connection::open", Vec::new(), "home"));
        let err = apply_dispositions(&mut clusters, &dispositions).unwrap_err();
        assert!(err.contains("more than one cluster"), "{err}");
    }

    /// THE CLUSTER GATE: no duplicate of any class is ever left open - every cluster on the real
    /// tree (exact, near or semantic, mechanical or a mandatory sweep) carries a recorded
    /// disposition, so the catalog holds only clusters a reader has judged not to be one logic
    /// written twice.
    #[test]
    fn every_open_cluster_is_dispositioned() {
        let undispositioned: Vec<String> = real_catalog()
            .iter()
            .filter(|c| c.disposition.is_none())
            .map(|c| format!("{} ({})", c.id, c.classification))
            .collect();
        assert!(
            undispositioned.is_empty(),
            "clusters with no disposition in {DISPOSITIONS_PATH} - close each duplicate, or \
             record why it is not one: {undispositioned:?}"
        );
    }

    /// A one-cluster catalog written through `to_wire` ends in a newline, round-trips back to
    /// exactly `to_wire` of that cluster, and never carries any of the `absent` keys.
    fn assert_catalog_ledger_round_trips<W>(to_wire: fn(&DupCluster) -> W, absent: [&str; 2])
    where
        W: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
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
            disposition: None,
        }];
        let json = ledger_json(&clusters, to_wire);
        assert!(json.ends_with('\n'));
        let back: Vec<W> = serde_json::from_str(&json).expect("round trips");
        assert_eq!(back, vec![to_wire(&clusters[0])]);
        for key in absent {
            assert!(!json.contains(key));
        }
    }

    rigger::test_cases! {
        /// Spec 90 criterion 2: the wire shape is LINE-FREE - it round-trips through
        /// `DupClusterWire`, not the full `DupCluster` (whose `start_line`/`end_line` are no
        /// longer present in the json at all).
        catalog_to_json_round_trips_through_deserialize:
            assert_catalog_ledger_round_trips(dup_cluster_wire, ["start_line", "end_line"]);
        /// The unguarded sibling carries the identity (line-free) AND the lines - never the
        /// content_hash, which belongs solely to the guarded file.
        catalog_lines_to_json_round_trips_and_carries_only_the_line_spans:
            assert_catalog_ledger_round_trips(
            dup_cluster_lines,
            ["content_hash", "proposed_home"],
        );
    }

    #[test]
    fn render_section_2_names_every_mandatory_sweep_and_every_cluster_id() {
        let files = on_fixture(
            scan_tree,
            &[
                ("src/a.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"),
                ("src/z.rs", "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n"),
            ],
        );
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

    rigger::test_cases! {
        replace_section_2_only_touches_section_2_leaving_neighbors_intact: assert_span_replaced(
            &replace_section_2(
                "# Title\n\n## 1. Responsibility Map\n\nsection one body\n\n## 2. Duplication Catalog\n\nold placeholder\n\n## 3. Boundary Violations\n\nsection three body\n",
                "## 2. Duplication Catalog\n\nnew body\n",
            ),
            &["new body"],
            &["old placeholder"],
            &["section one body", "section three body"],
        );
    }

    /// `replace` over a document with no section headings at all panics (each caller pins
    /// the loud message with `should_panic`), never silently appending `section`.
    fn replace_into_a_headingless_document(replace: fn(&str, &str) -> String, section: &str) {
        replace("# Title\n\nno sections here\n", section);
    }

    rigger::test_cases! {
        #[should_panic(expected = "missing criterion 1's placeholder contract")]
        replace_section_2_panics_loudly_when_the_heading_is_entirely_absent:
            replace_into_a_headingless_document(
            replace_section_2,
            "## 2. Duplication Catalog\n\nx\n",
        );
    }

    // -------------------------------------------------------------------------------------
    // The adversarial sample
    // -------------------------------------------------------------------------------------

    /// `n` synthetic draw identities, one function each in its own file.
    fn keys(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| sample_key(&format!("src/f{i}.rs"), "f", 0))
            .collect()
    }

    /// Draws of the same population under `seed_a` and `seed_b` agree exactly when the seeds do.
    fn assert_draws_match_iff_seeds_do(seed_a: u64, seed_b: u64) {
        let a = sample_indices(&keys(1000), 30, seed_a);
        let b = sample_indices(&keys(1000), 30, seed_b);
        if seed_a == seed_b {
            assert_eq!(a, b);
        } else {
            assert_ne!(a, b);
        }
    }

    rigger::test_cases! {
        sample_indices_is_deterministic_for_a_fixed_seed: assert_draws_match_iff_seeds_do(42, 42);
        different_seeds_produce_different_draws: assert_draws_match_iff_seeds_do(1, 2);
    }

    #[test]
    fn sample_indices_returns_k_distinct_sorted_in_bounds_indices() {
        let picked = sample_indices(&keys(500), 30, ADVERSARIAL_SEED);
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
        assert_eq!(sample_indices(&keys(5), 30, 7), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn sample_indices_of_an_empty_population_is_empty() {
        assert!(sample_indices(&[], 30, 7).is_empty());
    }

    /// The draw is stable under change: removing a function the draw did NOT pick leaves every
    /// drawn function drawn, and removing a drawn one replaces only that one - the property that
    /// keeps a hand reading pass valid across a cleanup, which a draw by population index lacked.
    #[test]
    fn removing_a_function_replaces_at_most_that_function_in_the_draw() {
        let population = keys(500);
        let drawn = |pop: &[String]| -> BTreeSet<String> {
            sample_indices(pop, 30, ADVERSARIAL_SEED)
                .into_iter()
                .map(|i| pop[i].clone())
                .collect()
        };
        let before = drawn(&population);
        let undrawn = population.iter().position(|k| !before.contains(k)).unwrap();
        let mut without_undrawn = population.clone();
        without_undrawn.remove(undrawn);
        assert_eq!(drawn(&without_undrawn), before);

        let first_drawn = population.iter().position(|k| before.contains(k)).unwrap();
        let mut without_drawn = population.clone();
        let removed = without_drawn.remove(first_drawn);
        let after = drawn(&without_drawn);
        assert_eq!(after.len(), 30);
        assert_eq!(
            before.difference(&after).collect::<Vec<_>>(),
            vec![&removed]
        );
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

    /// Cluster ids are unique on the real tree (the drift guard's own determinism premise,
    /// checked directly against the real tree rather than a fixture).
    #[test]
    fn real_cluster_ids_are_unique() {
        let clusters = real_catalog();
        let ids: Vec<&str> = clusters.iter().map(|c| c.id.as_str()).collect();
        let distinct: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(distinct.len(), ids.len(), "duplicate cluster id");
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
        let keys = sample_keys(real_files(), &refs);
        let a = sample_indices(&keys, ADVERSARIAL_SAMPLE_SIZE, ADVERSARIAL_SEED);
        let b = sample_indices(&keys, ADVERSARIAL_SAMPLE_SIZE, ADVERSARIAL_SEED);
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

    rigger::test_cases! {
        /// THE DRIFT GUARD for `docs/audit/duplication-catalog.json`: with `RIGGER_AUDIT_WRITE=1`
        /// set, regenerate and overwrite it (and, spec 90 criterion 2, its unguarded
        /// `.lines.json` sibling alongside it); otherwise regenerate in memory and assert the
        /// GUARDED file matches the committed bytes byte-for-byte (spec 85 Design) - mirrors
        /// `responsibility_map_json_matches_the_tree_or_is_rewritten` exactly. The `.lines.json`
        /// sibling is deliberately NEVER read back or compared here (spec 90 criterion 2 Design:
        /// "the guard NEVER compares") - it is write-mode-only output.
        duplication_catalog_json_matches_the_tree_or_is_rewritten:
            assert_ledger_matches_the_tree_or_rewrite(
                real_catalog(),
                CATALOG_PATH,
                CATALOG_LINES_PATH,
                dup_cluster_wire,
                dup_cluster_lines,
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
        let json = ledger_json(clusters, dup_cluster_wire);
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

    rigger::test_cases! {
        /// CLAIM 2: "a pin bump that shifts every site in a file leaves it byte-identical." A
        /// synthetic two-file fixture (the same renamed-identical-pair shape
        /// `build_catalog_orders_clusters_by_their_first_site` uses, so the fixture
        /// forms a real 2-site cluster), then a "pin bump" - 5 unrelated comment lines prepended to
        /// ONE file, shifting `add_one`'s own line span by 5 but leaving its text untouched -
        /// regenerates a byte-IDENTICAL guarded catalog, because content_hash keys on the span's own
        /// normalized tokens, never its line number.
        a_pin_bump_that_shifts_every_site_in_a_file_leaves_the_guarded_catalog_byte_identical:
            assert_a_pin_bump_leaves_the_guarded_json_byte_identical(
                &[("src/a.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"), ("src/z.rs", "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n")],
                "src/a.rs",
                |root| ledger_json(&build_catalog(&scan_tree(root)), dup_cluster_wire),
                "a pin bump that only shifts an existing site's OWN line number must leave the \
                 guarded catalog byte-identical (spec 90 criterion 2)",
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
        let base_json = ledger_json(&build_catalog(&scan_tree(base.path())), dup_cluster_wire);

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
        let a_json = ledger_json(
            &build_catalog(&scan_tree(branch_a.path())),
            dup_cluster_wire,
        );

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
        let b_json = ledger_json(
            &build_catalog(&scan_tree(branch_b.path())),
            dup_cluster_wire,
        );

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
            disposition: None,
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
        if audit_write_mode() {
            let _guard = lock_report_write();
            let updated = replace_section_2(&report_or_fresh(&root, &path), &section_2);
            write_creating_parent(&path, &updated);
            return;
        }
        let committed = committed_report(&path);
        let span = section_span(&committed, "## 2. ");
        assert_section_2_structurally_matches(&committed[span], clusters);
    }

    // =====================================================================================
    // Criterion 3 (`u85c3`, THIS UNIT): sections 3-5 (boundary violations, dead and
    // vestigial code, test-suite shape)
    // =====================================================================================

    rigger::test_cases! {
        /// Sections 1, 2 and 6 (this criterion's neighbors) survive byte-for-byte.
        replace_sections_3_to_5_only_touches_that_span_leaving_neighbors_intact: assert_span_replaced(
            &replace_section_3_to_5(
                "# Title\n\n\
                 ## 1. Responsibility Map\n\nsection one body\n\n\
                 ## 2. Duplication Catalog\n\nsection two body\n\n\
                 ## 3. Boundary Violations\n\nold section three\n\n\
                 ## 4. Dead and Vestigial Code\n\nold section four\n\n\
                 ## 5. Test-Suite Shape\n\nold section five\n\n\
                 ## 6. Prioritized Plan\n\n_Pending - criterion 4 (`u85c4`)._\n",
                "## 3. Boundary Violations\n\nnew section three\n",
                "## 4. Dead and Vestigial Code\n\nnew section four\n",
                "## 5. Test-Suite Shape\n\nnew section five\n",
            ),
            &["new section three", "new section four", "new section five"],
            &["old section three", "old section four", "old section five"],
            &[
                "section one body",
                "section two body",
                "_Pending - criterion 4 (`u85c4`)._",
            ],
        );
        /// A report that (hypothetically) ends right after section 5 - no `## 6. ` heading yet
        /// to bound the replacement span against.
        replace_sections_3_to_5_falls_back_to_end_of_string_when_no_section_6_heading_exists:
            assert_span_replaced(
            &replace_section_3_to_5(
                "# Title\n\n\
                 ## 1. Responsibility Map\n\nsection one body\n\n\
                 ## 3. Boundary Violations\n\nold section three\n",
                "## 3. Boundary Violations\n\nnew section three\n",
                "## 4. Dead and Vestigial Code\n\nnew section four\n",
                "## 5. Test-Suite Shape\n\nnew section five\n",
            ),
            &["new section three", "new section four", "new section five"],
            &["old section three"],
            &["section one body"],
        );
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

    /// A report section whose one cited list embeds live line numbers, isolated for STRUCTURAL
    /// treatment by [`assert_cited_section_structurally_matches`].
    struct CitedList {
        /// The section's number, as its failure messages name it.
        section: &'static str,
        /// The section's own top-level heading.
        heading: &'static str,
        /// The cited list's own sub-heading, and the prefix of the next heading that bounds it.
        marker: &'static str,
        next_prefix: &'static str,
        /// How failure messages name the list, and everything in the section outside it.
        label: &'static str,
        outside: &'static str,
        /// The list's own marker for a candidate's file grouping, and for a candidate's name.
        group_marker: fn(&str) -> String,
        name_marker: fn(&str) -> String,
    }

    /// Section 4.3's full list (via [`render_dead_code_full_list`]) is the ONLY content anywhere
    /// in sections 3-5 that embeds a live line number. Everything else in section 4 (4.0-4.2
    /// including the distribution table, and 4.4) is citation-free and fully deterministic from
    /// the tree, so it stays byte-exact - and still catches real content drift (e.g. the
    /// distribution table's own per-file counts).
    const SECTION_4_CITED_LIST: CitedList = CitedList {
        section: "4",
        heading: "## 4. Dead and Vestigial Code",
        marker: "### 4.3 ",
        next_prefix: "### ",
        label: "section 4.3",
        outside: "4.3's own citation list",
        group_marker: |file| format!("**`{file}`**"),
        name_marker: |name| format!("- **{name}**"),
    };

    /// Item 0's own deletion list (via [`render_dead_code_deletion_list`]) is the ONLY content
    /// anywhere in section 6 that embeds a live line number - one level deeper than section 4's
    /// (`#### 0. ` instead of `### 4.3 `). Everything else in section 6 (items 1-18 and the tier
    /// framing prose) is citation-free and fully deterministic, so it stays byte-exact.
    const SECTION_6_CITED_LIST: CitedList = CitedList {
        section: "6",
        heading: "## 6. Prioritized Plan",
        marker: "#### 0. ",
        next_prefix: "#### ",
        label: "section 6 item 0",
        outside: "item 0's own deletion list",
        group_marker: |file| format!("`{file}`: "),
        name_marker: |name| format!("`{name}`"),
    };

    /// Spec 90 Design, verbatim: "the report's guard checks structure only (sections present,
    /// counts equal to the catalog) rather than bytes." The committed section's `list` span
    /// must carry every candidate's own file grouping and name - never the exact citation
    /// bytes, which are free to legitimately move between explicit `RIGGER_AUDIT_WRITE=1`
    /// regens (a pin bump anywhere in `src/`) - while everything outside that span stays
    /// byte-exact against `fresh_section`, exactly as strict as a whole-section comparison.
    fn assert_cited_section_structurally_matches(
        committed_section: &str,
        fresh_section: &str,
        candidates: &[DeadCodeCandidate],
        list: &CitedList,
    ) {
        let CitedList { section, label, .. } = list;
        assert!(
            committed_section.starts_with(list.heading),
            "{REPORT_PATH} section {section} is missing its own heading"
        );
        let committed_span = heading_bounded_span(committed_section, list.marker, list.next_prefix);
        let fresh_span = heading_bounded_span(fresh_section, list.marker, list.next_prefix);
        let committed_list = &committed_section[committed_span.clone()];
        let mut files: Vec<&str> = candidates.iter().map(|c| c.file.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        for file in &files {
            let group_marker = (list.group_marker)(file);
            assert!(
                committed_list.contains(&group_marker),
                "{REPORT_PATH} {label} is missing its own file grouping for `{file}` \
                 (expected {group_marker:?}) - regenerate with RIGGER_AUDIT_WRITE=1"
            );
        }
        for c in candidates {
            let name_marker = (list.name_marker)(&c.name);
            assert!(
                committed_list.contains(&name_marker),
                "{REPORT_PATH} {label} is missing or has a stale entry for `{}` (expected \
                 {name_marker:?}) - regenerate with RIGGER_AUDIT_WRITE=1",
                c.name
            );
        }
        let committed_rest = format!(
            "{}{}",
            &committed_section[..committed_span.start],
            &committed_section[committed_span.end..]
        );
        let fresh_rest = format!(
            "{}{}",
            &fresh_section[..fresh_span.start],
            &fresh_section[fresh_span.end..]
        );
        // Trailing newline COUNT is a formatting artifact of the outer document splice (the
        // top-level boundary this span was cut from keeps the full blank-line separator, one
        // more `\n` than the section's own raw render), never real content - trimmed on both
        // sides before comparing so it cannot produce a false drift report.
        assert_eq!(
            committed_rest.trim_end_matches('\n'),
            fresh_rest.trim_end_matches('\n'),
            "{REPORT_PATH} section {section} (outside {}) has drifted from the tree - \
             regenerate with RIGGER_AUDIT_WRITE=1",
            list.outside
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
        let section_3 = render_section_3(real_files());
        let section_4 = render_section_4();
        let section_5 = render_section_5();
        let path = root.join(REPORT_PATH);
        if audit_write_mode() {
            let _guard = lock_report_write();
            let updated = replace_section_3_to_5(
                &report_or_fresh(&root, &path),
                &section_3,
                &section_4,
                &section_5,
            );
            write_creating_parent(&path, &updated);
            return;
        }
        let committed = committed_report(&path);
        // Section 3's citations are live line numbers and counts, free to move between
        // explicit regens (spec 90 Design: structure, not bytes); its prose must still match.
        assert_eq!(
            without_figures(committed[section_span(&committed, "## 3. ")].trim_end()),
            without_figures(section_3.trim_end()),
            "{REPORT_PATH} section 3 has drifted from the tree - regenerate with \
             RIGGER_AUDIT_WRITE=1"
        );
        let span = section_span(&committed, "## 4. ");
        assert_cited_section_structurally_matches(
            &committed[span],
            &section_4,
            real_dead_code_candidates(),
            &SECTION_4_CITED_LIST,
        );
        assert!(
            committed.contains(&section_5),
            "{REPORT_PATH} section 5 has drifted from the tree - regenerate with \
             RIGGER_AUDIT_WRITE=1"
        );
    }

    /// A synthetic tree's ledger: two dead free functions sharing one name in two files, so
    /// every entry carries an `ambiguous_with` citation as well as its own span.
    fn dead_code_fixture() -> Vec<DeadCodeCandidate> {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/a.rs", "fn twin() {}\n");
        write_fixture(dir.path(), "src/b.rs", "\nfn twin() {}\n");
        let candidates = candidates_for(dir.path());
        assert_eq!(candidates.len(), 2, "{candidates:?}");
        candidates
    }

    /// Shared machinery for the two CLAIM-4-equivalent decoy pin-bump tests below (section 4.3
    /// and section 6 item 0): over [`dead_code_fixture`]'s ledger, override the first entry's
    /// `lines` value with a decoy line, render via `render`, and assert the decoy citation
    /// (built by `citation`) is present while the stale (real) one is absent - proving the
    /// renderer sources its citation from `lines`, never `DeadCodeCandidate`'s own live `line`
    /// field. One helper, not two near-identical test bodies, since this file's own
    /// duplication scanner catches test code too (spec 85 Goal: "no small enough to duplicate
    /// exemption").
    fn assert_dead_code_render_cites_the_unguarded_lines_value(
        render: impl Fn(&[DeadCodeCandidate], &[DeadCodeCandidateLines]) -> String,
        citation: impl Fn(&DeadCodeCandidate, usize) -> String,
    ) {
        let candidates = dead_code_fixture();
        let mut lines: Vec<DeadCodeCandidateLines> =
            candidates.iter().map(dead_code_candidate_lines).collect();
        let real_line = candidates[0].line;
        let decoy_line = real_line + 500_000;
        lines[0].line = decoy_line;
        let rendered = render(&candidates, &lines);
        let decoy_citation = citation(&candidates[0], decoy_line);
        let stale_citation = citation(&candidates[0], real_line);
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

    rigger::test_cases! {
        /// CLAIM-4 equivalent for section 4.3 (mirrors
        /// `report_section_2_cites_file_line_exactly_as_the_unguarded_lines_file_records_them`),
        /// over a synthetic ledger (see `assert_dead_code_render_cites_the_unguarded_lines_value`).
        report_section_4_3_cites_file_line_exactly_as_the_unguarded_lines_file_records_them:
            assert_dead_code_render_cites_the_unguarded_lines_value(
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

    rigger::test_cases! {
        /// Every earlier section (not this criterion's own) survives byte-for-byte.
        replace_section_6_only_touches_that_span_leaving_earlier_sections_intact: assert_span_replaced(
            &replace_section_6(
                "# Title\n\n\
                 ## 1. Responsibility Map\n\nsection one body\n\n\
                 ## 2. Duplication Catalog\n\nsection two body\n\n\
                 ## 3. Boundary Violations\n\nsection three body\n\n\
                 ## 4. Dead and Vestigial Code\n\nsection four body\n\n\
                 ## 5. Test-Suite Shape\n\nsection five body\n\n\
                 ## 6. Prioritized Plan\n\n_Pending - criterion 4 (`u85c4`)._\n",
                "## 6. Prioritized Plan\n\nnew section six\n",
            ),
            &["new section six"],
            &["_Pending - criterion 4"],
            &[
                "section one body",
                "section two body",
                "section three body",
                "section four body",
                "section five body",
            ],
        );
    }

    #[test]
    fn replace_section_6_works_when_it_is_the_very_end_of_the_string() {
        // Section 6 is the LAST section - there is no next heading to bound the
        // replacement span against, unlike sections 1, 2 and 3-5's own combined span.
        let existing = "# Title\n\n## 6. Prioritized Plan\n\nold body\ntrailing line\n";
        let updated = replace_section_6(existing, "## 6. Prioritized Plan\n\nnew body\n");
        assert_eq!(updated, "# Title\n\n## 6. Prioritized Plan\n\nnew body\n");
    }

    rigger::test_cases! {
        #[should_panic(expected = "missing criterion 1's placeholder contract")]
        replace_section_6_panics_loudly_when_the_heading_is_entirely_absent:
            replace_into_a_headingless_document(
            replace_section_6,
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
        // Cites section 3's boundary violation by name.
        assert!(rendered.contains("Grounder"));
        // The many-candidates-one-home resolution for the project_batches cluster (spec 85
        // CONSTRAINTS WALK), its candidate count read from the catalog.
        assert!(rendered.contains(PROJECT_BATCHES));
        assert!(rendered.contains(&format!(
            "{} CANDIDATES, ONE HOME",
            cited(PROJECT_BATCHES).sites.len()
        )));
        // Cites the mandatory-sweep duplication clusters, and the /proc reader cluster, by id.
        for name in MANDATORY_SWEEPS.iter().chain([&PROC_STAT_READERS_SWEEP]) {
            assert!(rendered.contains(&cited_sweep(name).id), "{name}");
        }
        // Cites the god-file test/production split for all three files.
        assert!(rendered.contains("crates/rigger-conductor/src/conductor.rs"));
        assert!(rendered.contains("src/main.rs"));
        assert!(rendered.contains("crates/rigger-dash/src/dash.rs"));
        // Cites section 5's own headline test-suite consolidation items.
        assert!(rendered.contains("tests/common"));
        assert!(rendered.contains("tests/cli.rs"));
        assert!(
            rendered.contains("#### 15. Convert the largest remaining table-driven test families")
        );
        // Item 0: the dead-code deletion, and the explicit no-further-follow-up category.
        assert!(rendered.contains("Delete the dead-code set"));
        assert!(rendered.contains("no further follow-up"));
        assert!(rendered.contains("turbovec"));
        assert!(rendered.contains("kurrentdb"));
        // Adds no new findings: every dollar figure traces back to the committed JSON, not
        // a fresh scan - the section says so explicitly.
        assert!(rendered.contains("adds no new findings"));
        assert!(rendered.contains("#### 0. Delete the dead-code set"));
        assert!(rendered.contains("section 4.2's rule no entry is kept"));
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
        if audit_write_mode() {
            let _guard = lock_report_write();
            let existing = fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!(
                    "{REPORT_PATH} is missing - run criteria 1-3's own writers first \
                     (RIGGER_AUDIT_WRITE=1)"
                )
            });
            write_creating_parent(&path, &replace_section_6(&existing, &section_6));
            return;
        }
        let committed = committed_report(&path);
        let span = section_span(&committed, "## 6. ");
        assert_cited_section_structurally_matches(
            &committed[span],
            &section_6,
            real_dead_code_candidates(),
            &SECTION_6_CITED_LIST,
        );
    }

    rigger::test_cases! {
        /// CLAIM-4 equivalent for section 6 item 0 (mirrors
        /// `report_section_4_3_cites_file_line_exactly_as_the_unguarded_lines_file_records_them`):
        /// same shared machinery, applied to `render_dead_code_deletion_list` instead (see
        /// `assert_dead_code_render_cites_the_unguarded_lines_value`).
        report_section_6_item_0_cites_file_line_exactly_as_the_unguarded_lines_file_records_them:
            assert_dead_code_render_cites_the_unguarded_lines_value(
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

    /// `files` as the owned `(path, content)` pairs the resolvers take.
    fn owned_files(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files
            .iter()
            .map(|(rel, content)| (rel.to_string(), content.to_string()))
            .collect()
    }

    /// Over `files`, the out-of-line resolution pulls in every one of `expected` as test.
    fn assert_resolves_as_test(files: &[(&str, &str)], expected: &[&str]) {
        let test_files = resolve_out_of_line_test_files(&owned_files(files));
        for path in expected {
            assert!(test_files.contains(*path), "{test_files:?}");
        }
    }

    rigger::test_cases! {
        resolves_a_same_name_dot_rs_target: assert_resolves_as_test(
            &[
                ("src/lib.rs", "#[cfg(test)]\nmod probe;\n"),
                ("src/probe.rs", "fn helper() {}\n"),
            ],
            &["src/probe.rs"],
        );
        resolves_a_name_slash_mod_rs_target_when_the_flat_file_does_not_exist:
            assert_resolves_as_test(
            &[
                ("src/lib.rs", "#[cfg(test)]\nmod probe;\n"),
                ("src/probe/mod.rs", "fn helper() {}\n"),
            ],
            &["src/probe/mod.rs"],
        );
        resolves_a_path_override_target: assert_resolves_as_test(
            &[
                (
                    "src/lib.rs",
                    "#[cfg(test)]\n#[path = \"generated/probe.rs\"]\nmod probe;\n",
                ),
                ("src/generated/probe.rs", "fn helper() {}\n"),
            ],
            &["src/generated/probe.rs"],
        );
        /// The exact real-tree case spec 87's Goal names: `pub mod contract;` sits under
        /// `#[cfg(test)]` in `src/eventstore/mod.rs`.
        the_real_eventstore_mod_rs_shape_resolves_contract_rs_as_test: assert_resolves_as_test(
            &[
                ("src/eventstore/mod.rs", "#[cfg(test)]\npub mod contract;\n"),
                ("src/eventstore/contract.rs", "pub fn assert_contract() {}\n"),
            ],
            &["src/eventstore/contract.rs"],
        );
        /// outer.rs is test (declared #[cfg(test)] from lib.rs); outer.rs's OWN `mod inner;` has
        /// no local #[cfg(test)] at all, but the whole file is already test, so inner.rs must be
        /// pulled in too. `outer.rs`'s own children resolve under `src/outer/` (rustc's real
        /// file-per-module convention for a non-`mod.rs` declaring file), never a `src/inner.rs`
        /// sibling - `declaring_file_module_dir`'s own doc names the earlier version of this
        /// resolver that got this wrong.
        transitive_closure_pulls_in_a_second_hop_regardless_of_its_own_local_attribute:
            assert_resolves_as_test(
            &[
                ("src/lib.rs", "#[cfg(test)]\nmod outer;\n"),
                ("src/outer.rs", "mod inner;\n"),
                ("src/outer/inner.rs", "fn helper() {}\n"),
            ],
            &["src/outer.rs", "src/outer/inner.rs"],
        );
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
    // text scan) against `out_of_line_test_module_files` (`crates/rigger-grounder/src/grounder/symbols/events.rs`, spec
    // 86's canonical production resolver), compared directly over the same fixture trees and the
    // real tree.
    // -------------------------------------------------------------------------------------

    /// The production pipeline's own out-of-line-test-file exclusion set
    /// (`events::out_of_line_test_module_files` over a fresh index), scoped to `src/` to match
    /// [`resolve_out_of_line_test_files`]'s own scope.
    #[cfg(feature = "symbols")]
    fn production_out_of_line_exclusion_set(root: &Path) -> BTreeSet<String> {
        let idx = rigger::grounder::symbols::build_index(root.to_str().unwrap(), None);
        rigger::grounder::symbols::events::out_of_line_test_module_files(&idx)
            .into_iter()
            .filter(|path| path.starts_with("src/"))
            .collect()
    }

    #[cfg(feature = "symbols")]
    fn assert_resolvers_agree(files: &[(&str, &str)]) {
        let dir = fixture_tree(files);
        let bespoke = resolve_out_of_line_test_files(&owned_files(files));
        let production = production_out_of_line_exclusion_set(dir.path());
        assert_eq!(
            bespoke, production,
            "the bespoke resolver and the production one disagree on this fixture"
        );
    }

    rigger::test_cases! {
        #[cfg(feature = "symbols")]
        resolvers_agree_on_a_same_name_dot_rs_target: assert_resolvers_agree(&[
            ("src/lib.rs", "#[cfg(test)]\nmod probe;\n"),
            ("src/probe.rs", "pub fn helper() {}\n"),
        ]);
        #[cfg(feature = "symbols")]
        resolvers_agree_on_a_path_override_target: assert_resolvers_agree(&[
            (
                "src/lib.rs",
                "#[cfg(test)]\n#[path = \"generated/probe.rs\"]\nmod probe;\n",
            ),
            ("src/generated/probe.rs", "pub fn helper() {}\n"),
        ]);
        /// `outer.rs`'s own children resolve under `src/outer/` (rustc's real file-per-module
        /// convention for a non-`mod.rs` declaring file) - this fixture caught a real bug in
        /// `resolve_mod_target`'s prior (sibling-directory) resolution, fixed alongside adding
        /// this test; see `declaring_file_module_dir`'s own doc.
        #[cfg(feature = "symbols")]
        resolvers_agree_on_a_transitive_second_hop: assert_resolvers_agree(&[
            ("src/lib.rs", "#[cfg(test)]\nmod outer;\n"),
            ("src/outer.rs", "mod inner;\n"),
            ("src/outer/inner.rs", "pub fn helper() {}\n"),
        ]);
        #[cfg(feature = "symbols")]
        resolvers_agree_on_a_non_test_out_of_line_mod: assert_resolvers_agree(&[
            ("src/lib.rs", "mod normal;\n"),
            ("src/normal.rs", "pub fn helper() {}\n"),
        ]);
    }

    #[cfg(feature = "symbols")]
    #[test]
    fn resolvers_agree_on_the_real_tree() {
        let root = repo_root();
        let bespoke = resolve_out_of_line_test_files(&collect_files_with_content(
            &root,
            &["src".to_string()],
        ));
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
        let files = scan_workspace(root);
        let whole_file_test =
            resolve_out_of_line_test_files(&collect_workspace_src_files_with_content(root));
        build_dead_code_candidates(&files, &whole_file_test)
    }

    /// Over a one-file fixture `(file, src)`, `orphan` is a dead-code candidate; returns its
    /// test-only references.
    fn orphan_test_only_references(file: &str, src: &str) -> Vec<TestOnlyRef> {
        assert_candidates(&[(file, src)], &["orphan"], &[])
            .into_iter()
            .find(|c| c.name == "orphan")
            .unwrap()
            .test_only_references
    }

    rigger::test_cases! {
        /// Spec 87 Done-when criterion 2, fixture 1.
        a_fn_referenced_only_by_its_own_test_is_listed: {
            let refs = orphan_test_only_references(
                "src/lonely.rs",
                "fn orphan() {}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn calls_orphan() {\n        orphan();\n    }\n}\n",
            );
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].file, "src/lonely.rs");
        };
        /// Class 1 ("TEST REGIONS ARE MOD SPANS"): the real `crates/rigger-grounder/src/grounder/symbols/events.rs`
        /// shape (`sdet-u87c2-mod-body-level-test-statements-leak-as-production-refs`) - a named
        /// `use` import sits directly inside `#[cfg(test)] mod tests { .. }`, ABOVE its `#[test]`
        /// fn (never itself a `ScannedFn`), naming `orphan`. Before round 1, `in_test_range` was
        /// built from fn spans alone, so this line misclassified `orphan` as production-
        /// referenced and it never appeared in the JSON at all - the exact false negative that
        /// defeated criterion 2's own Done-when on spec 87's own Goal-cited worked example.
        a_named_use_import_at_mod_test_top_level_does_not_leak_as_a_production_reference: {
            let refs = orphan_test_only_references(
                "src/orphan.rs",
                "pub fn orphan() {}\n\n#[cfg(test)]\nmod tests {\n    use crate::orphan::orphan;\n\n    #[test]\n    fn calls_orphan() {\n        orphan();\n    }\n}\n",
            );
            assert_eq!(refs.len(), 2, "{refs:?}");
        };
    }

    /// Over the `files` fixture tree, every name in `listed` is a dead-code candidate and none
    /// in `unlisted` is. Returns the candidates.
    fn assert_candidates(
        files: &[(&str, &str)],
        listed: &[&str],
        unlisted: &[&str],
    ) -> Vec<DeadCodeCandidate> {
        let candidates = on_fixture(candidates_for, files);
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        for name in listed {
            assert!(names.contains(name), "{name}: {names:?}");
        }
        for name in unlisted {
            assert!(!names.contains(name), "{name}: {names:?}");
        }
        candidates
    }

    /// Over the `files` fixture tree, exactly `count` candidates are named `name`, every one
    /// flagged ambiguous. Returns them.
    fn ambiguous_sharers(
        files: &[(&str, &str)],
        name: &str,
        count: usize,
    ) -> Vec<DeadCodeCandidate> {
        let candidates = on_fixture(candidates_for, files);
        let sharers: Vec<DeadCodeCandidate> = candidates
            .iter()
            .filter(|c| c.name == name)
            .cloned()
            .collect();
        assert_eq!(sharers.len(), count, "{candidates:?}");
        assert!(sharers.iter().all(|c| c.ambiguous), "{sharers:?}");
        sharers
    }

    rigger::test_cases! {
        /// Spec 87 Done-when criterion 2, fixture 2.
        /// `caller` itself has no callers, so it legitimately DOES appear - this fixture's
        /// point is only that `helper`, which IS called from production code, does not.
        a_fn_referenced_from_a_production_caller_does_not_appear: assert_candidates(
            &[
                ("src/used.rs", "fn helper() {}\n\nfn caller() {\n    helper();\n}\n"),
            ],
            &["caller"],
            &["helper"],
        );
    }

    #[test]
    fn a_shared_name_referenced_via_self_colon_colon_from_within_its_own_impl_is_not_a_false_positive(
    ) {
        // Spec 87 criterion 3 regression, found while researching dispositions: the real
        // `crates/rigger-dash/src/dash.rs` `DashMarker::parse` (ambiguous with `gate.rs`/`ledger.rs` x2/`failure.rs`'s
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

    rigger::test_cases! {
        /// "or used as a path segment" (spec 87 Design) - a fn passed by name, e.g. as a
        /// function pointer, with no trailing `(`.
        a_path_qualified_reference_with_no_call_parens_still_counts: assert_candidates(
            &[
                ("src/ptr.rs", "pub fn target() {}\n\nmod user {\n    fn takes_ptr(_f: fn()) {}\n    fn wire() {\n        takes_ptr(super::target);\n    }\n}\n"),
            ],
            &[],
            &["target"],
        );
    }

    rigger::test_cases! {
        a_mention_inside_a_comment_does_not_count_as_a_reference: assert_candidates(
            &[
                ("src/commented.rs", "fn orphan() {}\n\n// this comment happens to say orphan() but never calls it\nfn other() {\n    let _ = 1;\n}\n"),
            ],
            &["orphan"],
            &[],
        );
    }

    #[test]
    fn visibility_is_carried_through_to_the_candidate() {
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(dir.path(), "src/vis.rs", "pub(crate) fn orphan() {}\n");
        let candidates = candidates_for(dir.path());
        let orphan = candidates.iter().find(|c| c.name == "orphan").unwrap();
        assert_eq!(orphan.visibility, "pub(crate)");
    }

    rigger::test_cases! {
        /// Spec 87 Design: the excluded "definition span" is "doc comment, attributes,
        /// signature" - NOT the body, so a self-call inside the body is a real reference.
        recursion_through_the_fns_own_body_still_counts_as_a_reference: assert_candidates(
            &[
                ("src/rec.rs", "fn countdown(n: u32) {\n    if n > 0 {\n        countdown(n - 1);\n    }\n}\n"),
            ],
            &[],
            &["countdown"],
        );
    }

    rigger::test_cases! {
        /// The real `src/eventstore/contract.rs` shape: `assert_contract` has no LOCAL
        /// #[cfg(test)] at all, but the whole file is pulled in as test by `mod.rs`.
        a_whole_file_test_via_out_of_line_resolution_is_never_a_candidate: assert_candidates(
            &[
                ("src/es/mod.rs", "#[cfg(test)]\npub mod contract;\n"),
                ("src/es/contract.rs", "pub fn assert_contract() {}\n"),
            ],
            &[],
            &["assert_contract"],
        );
    }

    rigger::test_cases! {
        main_is_exempted_as_an_entry_point: assert_candidates(
            &[
                ("src/main.rs", "fn main() {}\n"),
            ],
            &[],
            &["main"],
        );
    }

    // -------------------------------------------------------------------------------------
    // Round 1 (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances`): the three
    // classes the round-0 REJECT named, each pinned by its own fixture on the real motivating
    // shape.
    // -------------------------------------------------------------------------------------

    rigger::test_cases! {
        /// Class 2 ("ATTRIBUTE TOKEN TREES ARE REFERENCES"): the real `src/config.rs` shape
        /// (`sdet-u87c2-serde-default-attr-string-ref-is-a-false-positive`) -
        /// `default_build_config` is referenced ONLY through `#[serde(default = "..")]`'s string
        /// literal, a shape no call-site rule (`followed by (`, `::`, `.`, `<`) ever matches.
        /// Before round 1 this was a false positive: a genuinely live fn sat in the JSON as dead.
        a_serde_default_attribute_string_names_a_real_production_reference: assert_candidates(
            &[
                ("src/cfg.rs", "#[derive(serde::Deserialize)]\nstruct Cfg {\n    #[serde(default = \"default_build_config\")]\n    build: String,\n}\n\nfn default_build_config() -> String {\n    String::new()\n}\n"),
            ],
            &[],
            &["default_build_config"],
        );
    }

    rigger::test_cases! {
        /// The `via_attribute` exemption from ambiguity attribution: an attribute mention cannot
        /// be qualifier-resolved (it names no `Type::`/`module::` prefix at all), and the design's
        /// conservative direction says it must still keep BOTH same-named sharers alive rather
        /// than resolve one of them dead just because the attribute could not name which it meant.
        an_attribute_reference_to_an_ambiguous_shared_name_credits_every_sharer: assert_candidates(
            &[
                ("src/attr_amb.rs", "mod a {\n    pub fn make_default() -> u32 {\n        0\n    }\n}\nmod b {\n    pub fn make_default() -> u32 {\n        1\n    }\n}\n#[derive(serde::Deserialize)]\nstruct Cfg {\n    #[serde(default = \"make_default\")]\n    n: u32,\n}\n"),
            ],
            &[],
            &["make_default"],
        );
    }

    /// Over the `files` fixture tree, exactly one `rebuild` is flagged (ambiguous) - the one in
    /// `file`. Returns it.
    fn the_one_flagged_rebuild(files: &[(&str, &str)], file: &str) -> DeadCodeCandidate {
        let sharers = ambiguous_sharers(files, "rebuild", 1);
        assert_eq!(sharers[0].file, file, "{sharers:?}");
        sharers.into_iter().next().unwrap()
    }

    rigger::test_cases! {
        /// Class 4, the adversary's `distiller::rebuild`/`playbooks::rebuild` finding
        /// (`adv-u87c2-r0-free-fn-bare-name-collision-hides-a-genuinely-dead-fn`): two UNRELATED
        /// top-level free fns share the bare name `rebuild`; only one has a real, `::`-qualified
        /// caller. Before round 1, bare-name-only resolution silently counted BOTH alive because
        /// is the aggregate `rebuild(` reference set was non-empty; round 1's per-definition
        /// qualifier attribution now correctly excludes the called one and flags the other.
        a_free_fn_bare_name_collision_where_only_one_sharer_has_a_real_caller_flags_the_other: assert_eq!(
            the_one_flagged_rebuild(
            &[
                ("src/distiller.rs", "pub fn rebuild() -> u32 {\n    1\n}\n"),
                ("src/playbooks.rs", "pub fn rebuild() -> u32 {\n    2\n}\n"),
                ("src/main.rs", "fn main() {\n    let _ = playbooks::rebuild();\n}\n"),
            ],
                "src/distiller.rs",
            )
            .ambiguous_with,
            vec!["src/playbooks.rs:1".to_string()]
        );
        /// Attribution path 3: Rust's own lexical scoping resolves an unqualified sibling call
        /// with no `use` needed at all when the call sits in the SAME file as the definition -
        /// `two.rs`'s own bare `rebuild()` call attributes to `two.rs`'s own `rebuild`, leaving
        /// the unrelated `one.rs` sharer (zero callers of its own) correctly flagged ambiguous.
        a_bare_call_in_the_same_file_as_its_definition_attributes_locally_with_no_import_needed: the_one_flagged_rebuild(
            &[
                ("src/one.rs", "pub fn rebuild() -> u32 {\n    1\n}\n"),
                ("src/two.rs", "pub fn rebuild() -> u32 {\n    2\n}\nfn use_it() -> u32 {\n    rebuild()\n}\n"),
            ],
            "src/one.rs",
        );
        /// The addendum's other attribution path: "a `use module::name;` in the referencing file
        /// resolving to it" - a BARE `rebuild()` call in a file that imports it by qualified path
        /// attributes to that specific definition, same as a `module::rebuild()` call site would.
        a_bare_call_resolved_through_a_use_import_attributes_to_the_imported_definition: the_one_flagged_rebuild(
            &[
                ("src/distiller.rs", "pub fn rebuild() -> u32 {\n    1\n}\n"),
                ("src/playbooks.rs", "pub fn rebuild() -> u32 {\n    2\n}\n"),
                ("src/main.rs", "use crate::playbooks::rebuild;\nfn main() {\n    let _ = rebuild();\n}\n"),
            ],
            "src/distiller.rs",
        );
    }

    rigger::test_cases! {
        /// The addendum's literal "credited to NO definition" case: a BARE `rebuild()` call from
        /// a THIRD file (neither sharer's own, and no `use` import resolving it) cannot be
        /// attributed to either, so BOTH remain zero-attributed and BOTH are flagged ambiguous -
        /// never a false "somebody calls it somewhere" pass for either one.
        a_bare_unqualified_call_from_a_third_unrelated_file_credits_neither_sharer: ambiguous_sharers(
            &[
                ("src/one.rs", "pub fn rebuild() -> u32 {\n    1\n}\n"),
                ("src/two.rs", "pub fn rebuild() -> u32 {\n    2\n}\n"),
                ("src/three.rs", "fn use_it() -> u32 {\n    rebuild()\n}\n"),
            ],
            "rebuild",
            2,
        );
    }

    rigger::test_cases! {
        a_method_name_shared_by_two_impls_with_zero_calls_is_flagged_ambiguous: ambiguous_sharers(
            &[
                ("src/amb.rs", "struct A;\nstruct B;\nimpl A {\n    fn reset(&mut self) {}\n}\nimpl B {\n    fn reset(&mut self) {}\n}\n"),
            ],
            "reset",
            2,
        );
    }

    rigger::test_cases! {
        /// A `.reset()` call whose receiver's type the scanner cannot read (here a `for` loop
        /// binding) could target either `reset`, so it keeps both sharers alive - the
        /// conservative direction: a false negative, never a live method reported dead.
        a_method_name_shared_by_two_impls_with_an_unresolvable_receiver_excludes_both: assert_candidates(
            &[
                ("src/amb2.rs", "struct A;\nstruct B;\nimpl A {\n    fn reset(&mut self) {}\n}\nimpl B {\n    fn reset(&mut self) {}\n}\nfn use_all(xs: Vec<A>) {\n    for mut a in xs {\n        a.reset();\n    }\n}\n"),
            ],
            &[],
            &["reset"],
        );
    }

    #[test]
    fn a_same_name_method_on_another_type_does_not_keep_a_method_alive() {
        // A method is classified on its OWN callers: a call whose receiver resolves to `B`
        // (a typed parameter, a typed `let`, a struct-literal `let`, a `B::reset` path or
        // `self` inside `impl B`) is `B::reset`'s caller only, so `A::reset` stays dead.
        let shapes = [
            "fn caller(b: &mut B) {\n    b.reset();\n}\n",
            "fn caller() {\n    let mut b: B = B {};\n    b.reset();\n}\n",
            "fn caller() {\n    let mut b = B {};\n    b.reset();\n}\n",
            "fn caller(b: &mut B) {\n    B::reset(b);\n}\n",
            "impl B {\n    fn caller(&mut self) {\n        self.reset();\n    }\n}\n",
        ];
        for caller in shapes {
            let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
            write_fixture(
                dir.path(),
                "src/amb3.rs",
                &format!(
                    "struct A;\nstruct B {{}}\nimpl A {{\n    fn reset(&mut self) {{}}\n}}\nimpl B {{\n    fn reset(&mut self) {{}}\n}}\n{caller}"
                ),
            );
            let candidates = candidates_for(dir.path());
            let resets: Vec<(&str, usize)> = candidates
                .iter()
                .filter(|c| c.name == "reset")
                .map(|c| (c.file.as_str(), c.line))
                .collect();
            assert_eq!(
                resets,
                vec![("src/amb3.rs", 4)],
                "only A::reset (line 4) is dead when the one call resolves to B: {caller}"
            );
        }
    }

    #[test]
    fn a_pub_crate_method_behind_an_attribute_is_attributed_by_its_receiver() {
        // A method's `self` receiver is read from its own parameter list, never from the
        // first parenthesis on the `fn` line - `pub(crate)` and a `#[cfg_attr(..)]` both
        // open one earlier.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/ev.rs",
            "struct A;\nstruct B;\nimpl A {\n    #[cfg_attr(x, allow(dead_code))] pub(crate) fn to_event(&self) {}\n}\nimpl B {\n    #[cfg_attr(x, allow(dead_code))] pub(crate) fn to_event(&self) {}\n}\npub fn caller(a: &A) {\n    a.to_event();\n}\n",
        );
        let candidates = candidates_for(dir.path());
        let events: Vec<usize> = candidates
            .iter()
            .filter(|c| c.name == "to_event")
            .map(|c| c.line)
            .collect();
        assert_eq!(events, vec![7], "only B::to_event is dead: {candidates:?}");
    }

    #[test]
    fn a_caller_in_a_workspace_member_crate_counts_as_production() {
        // The workspace is one codebase: a member crate under `crates/<name>/src/` calling
        // into the root package is a production caller, and the member's own functions are
        // swept like any other - except an `extern "C"` export, which is an entry point.
        let dir = tempfile::tempdir().expect("a scratch dir for the fixture tree");
        write_fixture(
            dir.path(),
            "src/map.rs",
            "pub fn hit() {}\n\npub fn never_called() {}\n",
        );
        write_fixture(
            dir.path(),
            "crates/core/src/lib.rs",
            "#[no_mangle]\npub extern \"C\" fn console_call() {\n    rigger::map::hit();\n}\n\nfn orphan_in_member() {}\n",
        );
        let candidates = candidates_for(dir.path());
        let names: Vec<(&str, &str)> = candidates
            .iter()
            .map(|c| (c.file.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("crates/core/src/lib.rs", "orphan_in_member"),
                ("src/map.rs", "never_called"),
            ]
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

    rigger::test_cases! {
        /// The Constraints Walk's "called only through a trait object" case, using an INHERENT
        /// impl so this exercises a plain `.name(` occurrence, not the separate trait-impl
        /// exemption below.
        a_dot_call_through_an_inherent_method_on_any_receiver_counts_receiver_agnostically: assert_candidates(
            &[
                ("src/dyn_dispatch.rs", "struct Real;\nimpl Real {\n    fn spawn(&self) {}\n}\nfn run(r: &Real) {\n    r.spawn();\n}\n"),
            ],
            &[],
            &["spawn"],
        );
    }

    rigger::test_cases! {
        /// Drop::drop shape - invoked by the compiler at scope end, never via an explicit
        /// `.drop(` call site anywhere in real source text. Without the trait-impl exemption
        /// this would be a false-positive dead-code candidate on every real `impl Drop`.
        a_trait_impl_method_is_exempted_even_with_zero_textual_call_sites: assert_candidates(
            &[
                ("src/droppable.rs", "struct Guard;\nimpl Drop for Guard {\n    fn drop(&mut self) {}\n}\n"),
            ],
            &[],
            &["drop"],
        );
    }

    rigger::test_cases! {
        /// `Type::new()` has no preceding `.` - historically a fn taking no `self` had to be
        /// matched via a separate `::`-preceded rule from a method's `.name(` rule; round 3
        /// dropped that distinction for "is this a reference at all" (any occurrence counts
        /// regardless), but `ImplAssoc` still needs its own qualifier-based attribution when a
        /// name is shared - this pins the base case, `Foo::new()` keeping `new` alive at all.
        an_inherent_associated_function_is_matched_via_path_shape_not_dot_shape: assert_candidates(
            &[
                ("src/ctor.rs", "struct Foo;\nimpl Foo {\n    fn new() -> Self {\n        Foo\n    }\n}\nfn make() -> Foo {\n    Foo::new()\n}\n"),
            ],
            &[],
            &["new"],
        );
    }

    rigger::test_cases! {
        an_inherent_associated_fn_name_shared_by_two_types_with_zero_calls_is_ambiguous: ambiguous_sharers(
            &[
                ("src/ctors.rs", "struct A;\nstruct B;\nimpl A {\n    fn new() -> Self {\n        A\n    }\n}\nimpl B {\n    fn new() -> Self {\n        B\n    }\n}\n"),
            ],
            "new",
            2,
        );
    }

    /// Over a one-file fixture `(file, src)` declaring two `new`s (line 4, called through its
    /// own qualifier, and line 9, uncalled), only the uncalled one is flagged - ambiguous with
    /// the called one. `expected` names it in the failure message.
    fn assert_only_the_uncalled_new_is_flagged(file: &str, src: &str, expected: &str) {
        let sharers = ambiguous_sharers(&[(file, src)], "new", 1);
        assert_eq!(sharers[0].file, file);
        assert_eq!(sharers[0].line, 9, "{expected}; {sharers:?}");
        assert_eq!(sharers[0].ambiguous_with, vec![format!("{file}:4")]);
    }

    rigger::test_cases! {
        /// Round 1 (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): a `Type::new()`-qualified
        /// call site is now ATTRIBUTED to the ONE sharer it names, not credited to every sharer
        /// the way an unattributable bare mention would be - `A::new()` proves `A::new` alive
        /// (excluded) while `B::new`, with zero calls of its own, is correctly flagged ambiguous
        /// rather than silently hidden behind `A::new`'s real caller (the same failure shape the
        /// adversary's `distiller::rebuild`/`playbooks::rebuild` finding named for free fns).
        a_qualified_call_site_attributes_only_to_the_sharer_it_names: assert_only_the_uncalled_new_is_flagged(
            "src/ctors2.rs",
            "struct A;\nstruct B;\nimpl A {\n    fn new() -> Self {\n        A\n    }\n}\nimpl B {\n    fn new() -> Self {\n        B\n    }\n}\nfn make() -> A {\n    A::new()\n}\n",
            "expected B::new specifically",
        );
        /// Regression for round-1's own defect (sdet-u87c2-r1-impl-assoc-qualifier-drops-leading-
        /// impl-generics-reintroduces-false-positives, upheld by the round-1 adjudication reject):
        /// when the impl block declares ITS OWN leading generic/lifetime parameters
        /// (`impl<'a> Widget<'a>`), `enclosing_impl`'s header text starts with `<` itself (the
        /// `impl` keyword is never stored). The old naive
        /// `header.split(|c| c == '<' || c.is_whitespace()).next()` therefore returned an EMPTY
        /// qualifier for `Widget::new`, which could never equal the real `Widget::new()` call
        /// site's resolved qualifier `Some("Widget")` - so the genuinely-alive `Widget::new` was
        /// wrongly flagged ambiguous with zero references, exactly the false-positive shape found
        /// in the committed `dead-code.json` for `Namespaced::new`/`ReplayDriver::new`/
        /// `Buckets::new`/`Server::new`. `Other::new`, with zero callers of its own, is the one
        /// that must remain correctly flagged.
        a_qualified_call_site_on_an_impls_own_generic_self_type_attributes_correctly: assert_only_the_uncalled_new_is_flagged(
            "src/generic_ctors.rs",
            "struct Widget<'a>(std::marker::PhantomData<&'a ()>);\nstruct Other;\nimpl<'a> Widget<'a> {\n    fn new() -> Self {\n        Widget(std::marker::PhantomData)\n    }\n}\nimpl Other {\n    fn new() -> Self {\n        Other\n    }\n}\nfn make() -> Widget<'static> {\n    Widget::new()\n}\n",
            "expected Other::new specifically (Widget::new has a real qualified caller)",
        );
    }

    rigger::test_cases! {
        /// The real bug this fixture pins: `.map_err(be)` (found live in
        /// `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`) passes `be` BY NAME with no call syntax, `.`, or `::`
        /// of its own at all. THE RULE (round 3) makes this one case among many value-position
        /// shapes below - no dedicated argument-slot rule is left to name.
        a_fn_passed_by_value_as_a_bare_call_argument_counts_as_a_reference: assert_candidates(
            &[
                ("src/map_err.rs", "struct Error(String);\nfn be<E: std::fmt::Display>(e: E) -> Error {\n    Error(e.to_string())\n}\nfn open() -> Result<(), Error> {\n    std::fs::metadata(\"x\").map(|_| ()).map_err(be)\n}\n"),
            ],
            &[],
            &["be"],
        );
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

    rigger::test_cases! {
        /// Real production shape this pins: `crates/rigger-domain/src/docs.rs`'s `skill_registry()`, e.g.
        /// `SkillEntry { name: "x", render_body: render_x_skill }` - `render_x_skill` is a bare
        /// identifier VALUE in struct-literal field position, no call/dot/`::`/`<` of its own.
        a_fn_pointer_used_as_a_struct_literal_field_value_counts_as_a_reference: assert_candidates(
            &[
                ("src/registry.rs", "struct Entry {\n    name: &'static str,\n    render_body: fn() -> String,\n}\nfn render_a() -> String {\n    String::new()\n}\nfn registry() -> Vec<Entry> {\n    vec![Entry {\n        name: \"a\",\n        render_body: render_a,\n    }]\n}\n"),
            ],
            &[],
            &["render_a"],
        );
    }

    rigger::test_cases! {
        /// Real production shape this pins: `src/config.rs`'s
        /// `.map(FailureRuleDef::to_rule)` - `to_rule` takes `&self` (DispatchCategory::Method,
        /// dispatched receiver-agnostically via `.to_rule(`) but here is referenced by its own
        /// UFCS PATH as a bare value with no call of its own - round 2's `relevant()` filter
        /// checked only `method_shaped` for `Method` and dropped this site entirely.
        a_ufcs_qualified_value_passed_to_a_combinator_counts_as_a_reference_for_a_method: assert_candidates(
            &[
                ("src/ufcs_method.rs", "struct Rule;\nimpl Rule {\n    fn to_rule(&self) -> i32 {\n        0\n    }\n}\nfn apply(rules: Vec<Rule>) -> Vec<i32> {\n    rules.iter().map(Rule::to_rule).collect()\n}\n"),
            ],
            &[],
            &["to_rule"],
        );
    }

    rigger::test_cases! {
        a_fn_named_by_a_let_initializer_counts_as_a_reference: assert_candidates(
            &[
                ("src/let_init.rs", "fn handler() -> i32 {\n    0\n}\nfn wire() -> fn() -> i32 {\n    let f = handler;\n    f\n}\n"),
            ],
            &[],
            &["handler"],
        );
    }

    rigger::test_cases! {
        a_fn_named_as_an_array_element_counts_as_a_reference: assert_candidates(
            &[
                ("src/array_elem.rs", "fn step_one() {}\nfn step_two() {}\nfn pipeline() -> [fn(); 2] {\n    [step_one, step_two]\n}\n"),
            ],
            &[],
            &["step_one", "step_two"],
        );
    }

    rigger::test_cases! {
        a_fn_named_in_a_match_arm_value_counts_as_a_reference: assert_candidates(
            &[
                ("src/match_arm.rs", "fn plan_a() {}\nfn plan_b() {}\nfn choose(n: u8) -> fn() {\n    match n {\n        0 => plan_a,\n        _ => plan_b,\n    }\n}\n"),
            ],
            &[],
            &["plan_a", "plan_b"],
        );
    }

    rigger::test_cases! {
        a_fn_named_in_a_return_expression_counts_as_a_reference: assert_candidates(
            &[
                ("src/return_expr.rs", "fn default_handler() {}\nfn get_handler() -> fn() {\n    return default_handler;\n}\n"),
            ],
            &[],
            &["default_handler"],
        );
    }

    rigger::test_cases! {
        /// `token(&self, i)` never called, never assigned - only NAMED, as another type's own
        /// generic parameter (`Holder<marker_fn>`), a shape no call/dot/`::`/`<`-of-its-own rule
        /// would ever see since `marker_fn` itself is followed by `>`, not `(`/`.`/`::`/`<`.
        a_fn_named_as_a_generic_argument_to_another_type_counts_as_a_reference: assert_candidates(
            &[
                ("src/generic_arg.rs", "fn marker_fn() {}\nstruct Holder<F>(std::marker::PhantomData<F>);\nfn make() -> Holder<marker_fn> {\n    Holder(std::marker::PhantomData)\n}\n"),
            ],
            &[],
            &["marker_fn"],
        );
    }

    #[test]
    fn the_real_tree_no_longer_flags_the_round_2_struct_field_and_ufcs_defects() {
        // Real-tree pin (operator ruling `op-u87c2-round-3-a-reference-is-any-token-not-a-
        // shape`): the exact production fns round 2's adjudication reject named as invisible -
        // every `crates/rigger-domain/src/docs.rs` `skill_registry()` `render_*` entry point, and
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
    // THE EMPTY-LEDGER GATE
    // -------------------------------------------------------------------------------------

    /// Section 4.2's rule made a gate: a fixture ledger with entries fails, and the failure
    /// names every entry by `name` and `file:line`, so the reader knows exactly what to delete.
    #[test]
    #[should_panic(expected = "twin (src/a.rs:1)")]
    fn a_non_empty_ledger_fails_the_gate_naming_each_entry() {
        let candidates = dead_code_fixture();
        let message = std::panic::catch_unwind(|| assert_dead_code_ledger_empty(&candidates))
            .expect_err("a ledger with entries must fail the gate");
        let message = message
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(message.contains("twin (src/b.rs:2)"), "{message}");
        panic!("{message}");
    }

    /// The real workspace's ledger is empty: every production fn has a production caller.
    #[test]
    fn the_real_dead_code_ledger_is_empty() {
        assert_dead_code_ledger_empty(real_dead_code_candidates());
    }

    // -------------------------------------------------------------------------------------
    // THE DRIFT GUARD for `docs/audit/dead-code.json`
    // -------------------------------------------------------------------------------------

    rigger::test_cases! {
        /// THE DRIFT GUARD for `docs/audit/dead-code.json`: with `RIGGER_AUDIT_WRITE=1` set,
        /// regenerate and overwrite it; otherwise regenerate in memory and assert it matches the
        /// committed file byte-for-byte. Mirrors `duplication_catalog_json_matches_the_tree_or_
        /// is_rewritten` exactly.
        dead_code_json_matches_the_tree_or_is_rewritten:
            assert_ledger_matches_the_tree_or_rewrite(
                real_dead_code_candidates(),
                DEAD_CODE_PATH,
                DEAD_CODE_LINES_PATH,
                dead_code_candidate_wire,
                dead_code_candidate_lines,
            );
    }

    /// Spec 90 criterion 2, CLAIM 1 for `docs/audit/dead-code.json`, over a synthetic ledger (the
    /// real one is empty): structurally, no candidate or test-only reference carries `line`, and
    /// every `content_hash` is non-empty. Also checks
    /// any `ambiguous_with` citation is `file#hash`-shaped, never `file:line`.
    #[test]
    fn the_dead_code_json_carries_no_line_number_fields() {
        let json = ledger_json(&dead_code_fixture(), dead_code_candidate_wire);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let arr = value.as_array().expect("a bare array");
        assert!(!arr.is_empty());
        assert!(
            arr.iter().all(|e| e["ambiguous_with"]
                .as_array()
                .is_some_and(|a| !a.is_empty())),
            "the fixture must exercise the ambiguous_with citation shape: {json}"
        );
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

    rigger::test_cases! {
        /// Spec 90 criterion 2, CLAIM 2 for `docs/audit/dead-code.json`: a synthetic fixture tree
        /// proves a pin bump (5 unrelated comment lines prepended to one file, shifting that file's
        /// own candidate's line span) leaves the guarded dead-code JSON byte-identical, because
        /// `content_hash` keys on each candidate's own span text, never its line number. Closes
        /// `sdet-u90c2-surface-accounting`/`sdet-u90c2-deadcode-map-missing-claim2-claim3-tests` -
        /// sdet's own reverted probe already empirically confirmed this property; this test is that
        /// probe made permanent, using the file's own [`candidates_for`] fixture helper.
        a_pin_bump_leaves_the_guarded_dead_code_json_byte_identical:
            assert_a_pin_bump_leaves_the_guarded_json_byte_identical(
                &[("src/a.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"), ("src/z.rs", "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n")],
                "src/a.rs",
                |root| ledger_json(&candidates_for(root), dead_code_candidate_wire),
                "a pin bump that only shifts a candidate's OWN line number must leave the guarded \
                 dead-code JSON byte-identical (spec 90 criterion 2)",
            );
    }

    rigger::test_cases! {
        /// Spec 90 criterion 2, CLAIM 3 for `docs/audit/dead-code.json`, REFRAMED for a per-entry
        /// artifact - see the identical reframing on
        /// `two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry`
        /// for the full rationale (this file's own `json_array_entries` helper is shared with that
        /// test). Two branches, each adding one new, unreferenced (hence dead-code-candidate)
        /// function to a DIFFERENT file, leave every pre-existing candidate byte-identical and each
        /// contribute exactly their own one new entry.
        two_branches_each_adding_an_unrelated_function_to_a_different_file_never_perturb_an_existing_dead_code_entry:
            assert_two_branches_never_perturb_an_existing_entry(
                &[("src/a.rs", "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n"), ("src/z.rs", "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n")],
                (
                    "src/a.rs",
                    "fn add_one(n: u32) -> u32 {\n    n + 1\n}\n\n\
                     fn branch_a_only(x: i64) -> i64 {\n    x * 3 - 7\n}\n",
                ),
                (
                    "src/z.rs",
                    "fn plus_one(m: u32) -> u32 {\n    m + 1\n}\n\n\
                     fn branch_b_only(y: i64) -> i64 {\n    y / 2 + 11\n}\n",
                ),
                |root| ledger_json(&candidates_for(root), dead_code_candidate_wire),
                ("candidate", "candidates"),
            );
    }
}

// =========================================================================================
// CONFIG KEYS WITH NO EFFECT: every configurable key has a production reader
// =========================================================================================

/// A config key is honoured by code or it does not exist: the workflow schema rejects unknown
/// keys (spec 102), so a key nothing reads is a lie the config tells the operator. This gate
/// fails when a field of any `Deserialize` struct in [`CONFIG_SCHEMA_FILE`] (the workflow schema
/// tree and the `AgentDef` frontmatter) has no production `.field` read anywhere in the
/// workspace's production source, outside every test region.
///
/// DISCLOSED LIMIT: the match is by field NAME, not by receiver type, so a field that shares
/// its name with another type's read field (`name`, `kind`, `run`) reads as honoured. That is
/// the conservative direction (a dead key can slip through, a live key is never flagged).
mod config_key_readers {
    use super::*;

    /// The file that declares the configurable schema.
    const CONFIG_SCHEMA_FILE: &str = "crates/rigger-domain/src/config.rs";

    /// `Struct.field` entries exempt from the reader gate. Must stay EMPTY: a key without a
    /// reader is wired or deleted, never allowlisted.
    const UNREAD_KEY_ALLOWLIST: &[&str] = &[];

    /// Every configurable field of every top-level `#[derive(.. Deserialize ..)]` struct in
    /// `content`, as `(struct, field)`. A `#[serde(skip)]` field is runtime state, never a key,
    /// and is left out; an indented (test-module) struct is not part of the schema.
    pub(super) fn schema_fields(content: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut derives_deserialize = false;
        let mut current: Option<String> = None;
        let mut skip_next = false;
        for line in content.lines() {
            if let Some(name) = current.as_ref() {
                if line == "}" {
                    current = None;
                    continue;
                }
                let trimmed = line.trim_start();
                if trimmed.starts_with("#[serde(") && trimmed.contains("skip)") {
                    skip_next = true;
                } else if let Some(rest) = line.strip_prefix("    pub ") {
                    if let Some((field, _)) = rest.split_once(':') {
                        if !skip_next {
                            out.push((name.clone(), field.trim().to_string()));
                        }
                    }
                    skip_next = false;
                }
                continue;
            }
            if line.starts_with("#[derive(") {
                derives_deserialize = line.contains("Deserialize");
            } else if let Some(rest) = line.strip_prefix("pub struct ") {
                if derives_deserialize && line.ends_with('{') {
                    let name: String = rest.chars().take_while(|c| is_ident_char(*c)).collect();
                    current = Some(name);
                }
                derives_deserialize = false;
            } else if !(line.starts_with("#[") || line.starts_with("///")) {
                derives_deserialize = false;
            }
        }
        out
    }

    /// Every `Struct.field` among `fields` with no production `.field` read in `files`.
    pub(super) fn unread_fields(
        files: &[FileScan],
        whole_file_test: &BTreeSet<String>,
        fields: &[(String, String)],
    ) -> Vec<String> {
        let idx = all_ident_ref_sites(files, whole_file_test);
        let tokens: HashMap<&str, &[RawTok]> = files
            .iter()
            .map(|f| (f.rel.as_str(), f.tokens.as_slice()))
            .collect();
        fields
            .iter()
            .filter(|(_, field)| {
                !idx.get(field).is_some_and(|sites| {
                    sites.iter().any(|s| {
                        s.production
                            && !s.via_attribute
                            && s.tok_idx > 0
                            && ref_punct(tokens[s.file.as_str()].get(s.tok_idx - 1), ".")
                    })
                })
            })
            .map(|(st, field)| format!("{st}.{field}"))
            .collect()
    }

    #[test]
    fn schema_fields_skip_runtime_state_and_test_structs() {
        let src = "#[derive(Clone, Deserialize)]\n\
                   #[serde(deny_unknown_fields)]\n\
                   pub struct Keys {\n    \
                   #[serde(default)]\n    \
                   pub budget: u32,\n    \
                   /// Runtime state.\n    \
                   #[serde(skip)]\n    \
                   pub baseline: bool,\n\
                   }\n\
                   #[derive(Clone)]\n\
                   pub struct NotConfig {\n    \
                   pub other: u32,\n\
                   }\n\
                   mod tests {\n    \
                   #[derive(Deserialize)]\n    \
                   pub struct Inner {\n        \
                   pub nested: u32,\n    \
                   }\n\
                   }\n";
        assert_eq!(
            schema_fields(src),
            [("Keys".to_string(), "budget".to_string())]
        );
    }

    #[test]
    fn every_config_key_has_a_production_reader() {
        assert!(
            UNREAD_KEY_ALLOWLIST.is_empty(),
            "the unread-key allowlist must be empty - wire or delete each key: {UNREAD_KEY_ALLOWLIST:?}"
        );
        let content = fs::read_to_string(repo_root().join(CONFIG_SCHEMA_FILE))
            .unwrap_or_else(|e| panic!("cannot read {CONFIG_SCHEMA_FILE}: {e}"));
        let fields = schema_fields(&content);
        assert!(
            fields.iter().any(|(s, f)| s == "Defaults" && f == "budget")
                && fields.iter().any(|(s, f)| s == "AgentDef" && f == "model"),
            "the schema scan must find the known Defaults and AgentDef keys: {fields:?}"
        );
        let unread: Vec<String> =
            unread_fields(real_workspace_files(), real_whole_file_test_set(), &fields)
                .into_iter()
                .filter(|k| !UNREAD_KEY_ALLOWLIST.contains(&k.as_str()))
                .collect();
        assert!(
            unread.is_empty(),
            "every config key must change behaviour through a production reader - wire each of \
             these or delete it from the schema, the scaffold, the workflow and the docs:\n{}",
            unread.join("\n")
        );
    }
}
