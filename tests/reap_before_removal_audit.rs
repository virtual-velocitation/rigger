//! Spec 79 criterion 2, THE BARE-REMOVAL AUDIT: the whole-tree companion to criterion 1's
//! reap-before-removal rewiring (`sweep_terminal`, `clear_worktree_dir`, `reclaim_cache_sibling`,
//! `reclaim_worktree_on_branch`, `Worktree::discard`, `Worktree::remove` - src/worktree.rs, and
//! `reclaim_unit_mutation_scratch` - src/driver/replay.rs). This test walks every `.rs` file
//! under `src/` and fails, naming file and line, on any `fs::remove_dir_all(...)` or
//! `git worktree remove` call site that is NEITHER routed through a reap-then-remove path NOR
//! carries a claimed-exemption comment this test independently verifies is present (spec 79's
//! Design: "a directory that can have processes rooted inside it is removed ONLY through a
//! reap-then-remove path... a removal path whose dir provably cannot host a rooted process...
//! may stay bare, but the exemption is claimed in a code comment at the site").
//!
//! THE TWO FORMS A COVERED SITE TAKES (both already present in the real tree, u79c1's
//! delivery - decision `u79c1-rewiring-complete-and-exemption-marker`):
//!
//! 1. ROUTED: the removal's own enclosing function calls one of the reap authorities
//!    (`reap::reap_authorized`, `reap::reap_processes_rooted_under`,
//!    `worktree::reap_dir_before_removal`) - or, for a call SITE that merely delegates to one
//!    of the two sanctioned wrapper helpers (`reap_then_remove_dir`/`reap_then_remove_worktree`
//!    in `src/main.rs`) rather than removing directly, that wrapper call itself - on a line
//!    STRICTLY BEFORE the removal's own line (never merely "anywhere in the same function",
//!    ROUND-2 FIX: a prior round of this file accepted unordered co-occurrence, which marked
//!    the exact inverted remove-then-reap anti-pattern spec 79's own Design text forbids as
//!    covered - `adj-u79c2-verdict-reject-detection-blind-spots`, upholding
//!    `arch-u79c2-coverage-check-is-unordered-co-occurrence-not-reap-then-remove`). For
//!    `reap_dir_before_removal` specifically - the one authority with a DOCUMENTED
//!    literal-empty-string no-op convention (its own doc comment: "Pass `""` when the caller
//!    has no such root... the reap becomes a no-op") - the matched call's own second argument
//!    is additionally required not to be that literal `""` (ROUND-2 FIX, upholding
//!    `adv-u79c2-authorized-root-value-blind-textual-match`: `reap_dir_before_removal(dir,
//!    "")` is a GUARANTEED runtime no-op, textually indistinguishable from a genuinely
//!    effective call under a name-only match). This is still a TEXTUAL "does the enclosing
//!    function's source contain one of these names, in the right order, with an effective
//!    argument where that is checked" scan, not real control-flow or data-flow analysis
//!    (matching `tests/no_os_kill_audit.rs`'s own precedent for the sibling spec-78 audit) -
//!    it does NOT correlate the reap call's own directory argument against the removal's
//!    (every routed site in this tree calls its reap authority unconditionally or on every
//!    branch that reaches the removal, but e.g. `Worktree::remove` reaps a locally
//!    canonicalized `base` derived from `self.dir` while the removal itself uses `self.dir`
//!    directly - two textually different expressions for the same directory, so a strict
//!    same-variable-text requirement would misclassify that real, correct site as
//!    uncovered; a text scan cannot safely bridge that without a real data-flow analyzer).
//! 2. EXEMPTED: the enclosing function carries the literal marker substring `reap-exempt`
//!    (u79c1's own convention, always followed by `(spec 79, criterion 2): <reason>` in the
//!    real tree, though this test only requires the marker itself - the reason text is a
//!    human-reviewed prose claim, not something a text scan can validate) in a doc or line
//!    comment - proof the exemption was DELIBERATELY claimed at this exact site, not merely
//!    that some comment happens to sit nearby.
//!
//! SCOPE: `src/` only, recursively (`src/driver/replay.rs` included) - never `tests/`. Spec
//! 79's Done-when line is literally "walks `src/`", and its Notes name why: "the pid-namespace
//! test runner already contains TEST-spawned orphans; this spec is about the OPERATOR-side
//! runtime paths, which run in no namespace." A test fixture's own tempdir teardown (e.g.
//! `tests/reap_before_removal_periphery.rs`'s fixture processes, or `tests/cli.rs`'s scratch
//! cleanup) is therefore never scanned - not because it is safe by inspection, but because it
//! is a different problem this spec does not own.
//!
//! Within `src/`, anything textually inside a `#[cfg(test)]`-attributed item (a `mod { ... }`
//! block, OR a single standalone item like `main.rs`'s `#[cfg(test)] fn compose_precommit`, OR
//! `lib.rs`'s semicolon-terminated `#[cfg(test)] mod blast_radius_eval;`) is excluded for the
//! SAME reason - it is `#[cfg(test)]` code precisely because it only ever runs inside the
//! pid-namespaced test runner, never on an operator's real run.
//!
//! WHY A REGEX/AST LIBRARY WAS NOT REACHED FOR: spec 79's Global Constraints forbid a new
//! dependency, and rustfmt (a build-gate precondition on every unit) makes two structural
//! properties reliable enough for a plain-text scan: (a) a brace-delimited item's own closing
//! `}` always sits at EXACTLY the item's own indentation, however many lines its signature
//! spans, and (b) an item's leading attributes/doc comments are always contiguous immediately
//! above it. [`block_span`] and [`cfg_test_ranges`] below lean on exactly these two properties
//! and nothing else about Rust's grammar.

use std::fs;
use std::path::{Path, PathBuf};

/// The reap authorities a covered removal site's enclosing function calls (spec 79's Design
/// and `u79c1-rewiring-complete-and-exemption-marker`): the two direct calls every routed
/// worktree.rs site makes, plus the two `src/main.rs` wrapper helpers spec 79's Design section
/// names by name as the sanctioned reap-then-remove path for a call site that delegates to
/// them instead of removing directly (`plan-u79c2-scope-and-gate`'s own allow-list wording).
const REAP_AUTHORITIES: [&str; 5] = [
    "reap_processes_rooted_under(",
    "reap_authorized(",
    "reap_dir_before_removal(",
    "reap_then_remove_dir(",
    "reap_then_remove_worktree(",
];

/// The claimed-exemption marker this audit verifies (u79c1's own convention): every real
/// exemption comment in the tree reads `reap-exempt (spec 79, criterion 2): <reason>`, but
/// this check requires only the marker substring itself - a text scan cannot judge whether the
/// human-authored reason is actually sound, only that an exemption was deliberately claimed at
/// this site rather than merely inferred from an unrelated nearby comment.
const EXEMPTION_MARKER: &str = "reap-exempt";

/// One bare-removal finding: which file, which 1-based line, which shape, and the offending
/// line's own text (for the failure message only - never re-scanned).
#[derive(Debug, Clone)]
struct Finding {
    file: String,
    line_no: usize,
    shape: &'static str,
    line_text: String,
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

/// The number of leading ASCII space characters on `line` - this audit's sole proxy for
/// "indentation level", since every file it scans is rustfmt-clean (a build-gate
/// precondition) and rustfmt never indents with tabs.
fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
}

/// True for a line that DECLARES a function: `fn ` appearing as its own word (never part of a
/// longer identifier or embedded in prose - "function" has no `fn` substring at all, since
/// `f` is always followed by `u`, never `n`, so no comment-line guard is needed here). Matches
/// `fn `, `pub fn `, `pub(crate) fn `, `async fn `, `unsafe fn `, and any other modifier
/// combination, since it only requires the character immediately before `fn ` to be a
/// non-identifier character (or the start of the line).
fn is_fn_sig_line(line: &str) -> bool {
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
fn block_span(lines: &[&str], start: usize) -> (usize, usize) {
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
/// Requires the marker line's TRIMMED text to be EXACTLY `#[cfg(test)]` - never a substring
/// match - so a doc comment merely mentioning the phrase in prose (as `blast_radius_eval.rs`'s
/// own module doc does) is never mistaken for the attribute (a `//` or `///` line can never
/// equal `#[cfg(test)]` after trimming).
fn cfg_test_ranges(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "#[cfg(test)]" {
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

fn in_ranges(line: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(s, e)| line >= s && line <= e)
}

/// Every `fn`-signature line (0-based, ascending order) that is NOT inside `excluded` and is
/// not itself a comment line (a further guard against a doc comment that happens to embed a
/// literal `fn ` code sample, on top of [`is_fn_sig_line`]'s own word-boundary check).
fn fn_sig_lines(lines: &[&str], excluded: &[(usize, usize)]) -> Vec<usize> {
    (0..lines.len())
        .filter(|&i| {
            !in_ranges(i, excluded)
                && !lines[i].trim_start().starts_with("//")
                && is_fn_sig_line(lines[i])
        })
        .collect()
}

/// The earliest line (0-based) of the contiguous doc-comment/attribute block sitting
/// immediately above `fn_line` (an `fn`-signature line), if any - so a `reap-exempt` marker
/// placed in the function's OWN doc comment (as `heal_corrupt_worktree_admin` does, rather
/// than as a body-level line comment the way `materialize_config_at_rev` does) is still found
/// by [`is_covered`], which only ever searches an [`enclosing_fn_span`]. Stops at the first
/// line above that is not a `///`/`//!`/`//` comment or a `#[...]` attribute - a blank line,
/// or real code (the end of a DIFFERENT, preceding item).
fn doc_comment_start(lines: &[&str], fn_line: usize) -> usize {
    let mut start = fn_line;
    while start > 0 {
        let t = lines[start - 1].trim();
        if t.starts_with("///") || t.starts_with("//!") || t.starts_with("//") || t.starts_with('#')
        {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

/// The enclosing function's `[start, end]` span for line `at`, if any: the nearest
/// fn-signature line AT OR BEFORE `at` whose own [`block_span`] actually reaches `at` -
/// searched innermost-candidate-first (`sigs` descending) so a later, more deeply nested `fn`
/// is preferred over an outer one that has already closed by `at`. `start` is widened backward
/// over the function's own leading doc-comment/attribute block via [`doc_comment_start`], so a
/// marker placed there (not just in the body) still counts.
fn enclosing_fn_span(lines: &[&str], sigs: &[usize], at: usize) -> Option<(usize, usize)> {
    for &s in sigs.iter().rev() {
        if s > at {
            continue;
        }
        let (_, end) = block_span(lines, s);
        if at <= end {
            return Some((doc_comment_start(lines, s), end));
        }
    }
    None
}

/// Whether `authority` is a reap call whose second argument is checked for a literal
/// empty-string no-op (spec 79 c2 round-2 fix,
/// `adv-u79c2-authorized-root-value-blind-textual-match`). Only [`reap_dir_before_removal`]
/// qualifies: it is the ONE authority with a documented literal `""` no-op convention (its
/// own doc comment, mirrored by `Worktree::create`'s: "Pass `""` when the caller has no such
/// root... the reap becomes a no-op") - a bare `&str` argument, so the no-op form is the
/// literal token `""` itself. The other root-taking authorities
/// (`reap_processes_rooted_under`, `reap_then_remove_dir`, `reap_then_remove_worktree`) take
/// a `&Path`, so their real-tree no-op equivalent would be a WRAPPED expression like
/// `Path::new("")`, never the bare literal this defect was reproduced against; generalizing
/// the check to a wrapped form was not reproduced against any real call site in this tree
/// and would risk a brittle partial parse for no demonstrated defect. `reap_authorized`
/// takes no root argument at all (its caller already authorized `base` before calling in).
fn takes_checked_root_arg(authority: &str) -> bool {
    authority == "reap_dir_before_removal("
}

/// Every double-quoted string literal's inner text on `line`, in order, naive (no escape
/// handling - every string this audit scans is a short ASCII CLI-arg or path literal, never
/// containing an escaped quote).
fn quoted_tokens(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        out.push(&after[..end]);
        rest = &after[end + 1..];
    }
    out
}

/// The raw text of the arguments to the call whose name-plus-opening-paren is `prefix`
/// (e.g. `"reap_dir_before_removal("`), starting the search at `lines[start]` - everything
/// from just after that opening paren through the matching closing paren, joined across
/// however many lines the call wraps onto (rustfmt may put each argument on its own line).
/// Paren- and string-aware, so a `)` or `,` inside a quoted string argument is never
/// mistaken for the call's own delimiter, and a nested call in an argument
/// (`std::path::Path::new(dir)`) does not prematurely close it. Bounded to a small forward
/// window - generous for any call this audit inspects (a handful of short arguments) but
/// never scanning arbitrarily far into the file; returns `None` if `prefix` is not found on
/// `lines[start]` or the call does not close within the window (a shape this audit cannot
/// confidently verify is never credited as covering).
fn call_args_text(lines: &[&str], start: usize, prefix: &str) -> Option<String> {
    const WINDOW: usize = 12;
    let mut text = String::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut opened = false;
    for line in lines.iter().skip(start).take(WINDOW) {
        let chunk = if !opened {
            let at = line.find(prefix)?;
            opened = true;
            &line[at + prefix.len()..]
        } else {
            line
        };
        for c in chunk.chars() {
            if in_str {
                text.push(c);
                if c == '"' {
                    in_str = false;
                }
                continue;
            }
            match c {
                '"' => {
                    in_str = true;
                    text.push(c);
                }
                '(' => {
                    depth += 1;
                    text.push(c);
                }
                ')' => {
                    if depth == 0 {
                        return Some(text);
                    }
                    depth -= 1;
                    text.push(c);
                }
                _ => text.push(c),
            }
        }
        text.push('\n');
    }
    None
}

/// Split `text` (a call's own raw argument text from [`call_args_text`]) into its top-level
/// arguments on comma, string- and paren-aware so a comma inside a nested call or a quoted
/// string is never mistaken for an argument separator. A trailing comma before the closing
/// paren (rustfmt's usual style for a wrapped multi-line call) yields no spurious empty
/// trailing argument.
fn split_top_level_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut current = String::new();
    for c in text.chars() {
        if in_str {
            current.push(c);
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                current.push(c);
            }
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    let last = current.trim();
    if !last.is_empty() {
        args.push(last.to_string());
    }
    args
}

/// Whether the call to `prefix` starting at `lines[start]` has an effective (not
/// literal-empty-string) authorized-root argument - only meaningful when
/// [`takes_checked_root_arg`] is true for `prefix`. A call whose own arguments this audit
/// cannot confidently extract (did not close within [`call_args_text`]'s window, or has
/// fewer than 2 arguments - both shapes this audit has never seen in the real tree) is
/// treated as NOT effective: a coverage claim this scan cannot itself verify is never
/// credited.
fn authorized_root_arg_is_effective(lines: &[&str], start: usize, prefix: &str) -> bool {
    let Some(text) = call_args_text(lines, start, prefix) else {
        return false;
    };
    let args = split_top_level_args(&text);
    match args.last() {
        Some(last) => last != "\"\"",
        None => false,
    }
}

/// The `[window_start, window_end]` bound (0-based, inclusive) that the exemption-marker
/// check in [`is_covered`] searches for `removal_line`, given the OTHER removal-shaped
/// lines (if any) found in `[start, end]` (spec 79 c2 round-3 fix,
/// `arch-u79c2r2-exemption-marker-function-wide-not-site-scoped`): the nearest earlier
/// removal-shaped line strictly before `removal_line` (exclusive - its own comment belongs
/// to IT, never to this later removal) or `start` if none, through the nearest later
/// removal-shaped line strictly after `removal_line` (exclusive) or `end` if none. In a
/// function with exactly one removal - every real site in this tree today - this degenerates
/// to the whole `[start, end]` span (preserving order-independence: a claim in the function's
/// own leading doc comment, as `heal_corrupt_worktree_admin` carries it, or trailing the
/// removal in the body, both still count). In a function with more than one removal, this
/// keeps a marker attached to ONE of them from silently also covering an unrelated,
/// unexempted other one. Skips comment lines while locating the neighboring removals
/// (mirroring [`scan_tree`]'s own guard), since a removal pattern merely mentioned in prose
/// is not a real site to bound against.
fn exemption_window(
    lines: &[&str],
    start: usize,
    end: usize,
    removal_line: usize,
) -> (usize, usize) {
    let mut window_start = start;
    for i in start..removal_line {
        let line = lines[i];
        if line.trim_start().starts_with("//") {
            continue;
        }
        if remove_dir_all_shape(line).is_some() || worktree_remove_shape(lines, i).is_some() {
            window_start = i + 1;
        }
    }
    let mut window_end = end;
    for i in (removal_line + 1)..=end {
        let line = lines[i];
        if line.trim_start().starts_with("//") {
            continue;
        }
        if remove_dir_all_shape(line).is_some() || worktree_remove_shape(lines, i).is_some() {
            window_end = i - 1;
            break;
        }
    }
    (window_start, window_end)
}

/// `line` with every quoted-string literal's content (and its delimiting quotes) dropped, and
/// everything from the first unquoted `//` onward dropped too - string-aware (naive, no escape
/// handling, matching [`quoted_tokens`]'s own established precedent: every string this audit
/// scans is a short ASCII CLI-arg or path literal, never an escaped quote) so that BOTH a
/// reap-authority name living only inside a string literal (a log message - spec 79 c2
/// round-4 fix, `adv-u79c2r3-authority-match-inside-noncomment-string-literal-uncaught-by-
/// either-fix`) AND one living only after a trailing `//` comment on an otherwise-real code
/// line (round-4 fix, `arch-u79c2r3-comment-guard-is-whole-line-only-trailing-comment-still-
/// falsely-covers`) are excluded from a "does this line contain a real call" check - while a
/// real call's own name, sitting in actual code before either a trailing comment or a string
/// argument, is left untouched and still matches. A `//` that itself lives inside a string
/// literal argument (never seen in this tree's own reap-authority calls) is correctly not
/// mistaken for a comment start, since string content is tracked and skipped first.
fn effective_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_str = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            break;
        }
        out.push(c);
    }
    out
}

/// The comment portion of `line`, if it carries one: the whole line when its trimmed text
/// already starts with `//` (a whole-line comment, including `///`/`//!` doc comments - both
/// start with `//`), or the substring from the first unquoted `//` onward for a TRAILING
/// comment following real code on the same line (string-aware, naive - matching
/// [`effective_code`]'s own precedent, so a `//` inside a string literal argument is never
/// mistaken for a comment start). `None` when `line` carries no comment at all - so a marker
/// substring living only in real, non-comment code (a log string, say) is never mistaken for a
/// deliberately claimed exemption (spec 79 c2 round-4 fix,
/// `sdet-u79c2r3-exemption-marker-check-has-no-comment-guard-at-all`: the round-3
/// EXEMPTION_MARKER check had no comment requirement of any kind).
fn comment_text(line: &str) -> Option<&str> {
    if line.trim_start().starts_with("//") {
        return Some(line);
    }
    let mut in_str = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if in_str {
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            continue;
        }
        if c == '/' && chars.peek().is_some_and(|&(_, next)| next == '/') {
            return Some(&line[idx..]);
        }
    }
    None
}

/// Whether `span` (the removal's enclosing function, or `None` if it has none) is covered:
/// either the claimed-exemption marker appears, ON AN ACTUAL COMMENT per [`comment_text`]
/// (round-4 fix, `sdet-u79c2r3-exemption-marker-check-has-no-comment-guard-at-all`), within
/// this removal's own [`exemption_window`] (order-independent within that window - a
/// documented human claim about this site, not a call with a happens-before relationship to
/// the removal, but no longer credited to an unrelated OTHER removal elsewhere in the same
/// function - spec 79 c2 round-3 fix, `arch-u79c2r2-exemption-marker-function-wide-not-site-
/// scoped`), or a reap-authority call appears, in [`effective_code`] (round-4 fix, closing
/// both the trailing-comment gap `arch-u79c2r3-comment-guard-is-whole-line-only-trailing-
/// comment-still-falsely-covers` and the string-literal gap `adv-u79c2r3-authority-match-
/// inside-noncomment-string-literal-uncaught-by-either-fix` together - a bare substring match
/// against the raw line, as round 3 did, credits an authority NAME sitting in a trailing
/// comment or inside a log-message string, never a real call), on a line STRICTLY BEFORE
/// `removal_line` with - for the one authority [`takes_checked_root_arg`] flags - an effective
/// authorized-root argument (spec 79 c2 round-2 fix: see the module doc's ROUTED entry for why
/// order and this one argument check exist, and why full directory-argument correlation does
/// not).
fn is_covered(lines: &[&str], span: Option<(usize, usize)>, removal_line: usize) -> bool {
    let Some((start, end)) = span else {
        return false;
    };
    let (window_start, window_end) = exemption_window(lines, start, end, removal_line);
    if lines[window_start..=window_end]
        .iter()
        .any(|line| comment_text(line).is_some_and(|c| c.contains(EXEMPTION_MARKER)))
    {
        return true;
    }
    for i in start..removal_line {
        let code = effective_code(lines[i]);
        for authority in REAP_AUTHORITIES.iter() {
            if !code.contains(*authority) {
                continue;
            }
            if !takes_checked_root_arg(authority)
                || authorized_root_arg_is_effective(lines, i, authority)
            {
                return true;
            }
        }
    }
    false
}

/// How many lines ahead of a `"worktree"` token this audit looks for the paired
/// `"remove"` token (spec 79 c2 round-2 fix,
/// `sdet-u79c2-multiline-worktree-remove-args-array-evades-detection`): generous enough for
/// rustfmt's own longest observed wrap of a `.args([...])` array (one element per line) with
/// room to spare, but bounded so an unrelated, later `"remove"` string literal elsewhere in
/// the function is never mistaken for this pair.
const WORKTREE_REMOVE_WINDOW: usize = 6;

/// True at the line where a `"worktree"` quoted-string token appears, if a `"remove"`
/// quoted-string token appears on the SAME line or within
/// [`WORKTREE_REMOVE_WINDOW`] lines after it - a bare, unreaped `git worktree remove` call
/// site, whether rustfmt kept its `.args([...])` array on one line or wrapped it one element
/// per line (round-2 fix; the prior round's same-line-only substring match missed the
/// wrapped form). Anchored at the `"worktree"` line, so a finding's reported line and the
/// coverage check's order test both use the EARLIEST evidence of the call, not wherever
/// `"remove"` happens to land. A `"worktree"` token with no `"remove"` token anywhere in the
/// window (e.g. `git worktree list`/`prune`) is not this shape.
fn worktree_remove_shape(lines: &[&str], i: usize) -> Option<&'static str> {
    if !quoted_tokens(lines[i]).contains(&"worktree") {
        return None;
    }
    let upper = (i + 1 + WORKTREE_REMOVE_WINDOW).min(lines.len());
    if lines[i..upper]
        .iter()
        .any(|line| quoted_tokens(line).contains(&"remove"))
    {
        return Some("bare git worktree remove with no reap coverage or claimed exemption");
    }
    None
}

/// The forbidden `fs::remove_dir_all` shape on `line`, if any - a real call. The caller
/// ([`scan_tree`]) already skips a whole-line comment before reaching either shape check
/// (mirroring [`fn_sig_lines`]'s own comment guard, shared here rather than duplicated per
/// shape), so a comment merely mentioning the pattern in prose is never mistaken for it.
fn remove_dir_all_shape(line: &str) -> Option<&'static str> {
    if line.contains("remove_dir_all(") {
        return Some("bare fs::remove_dir_all with no reap coverage or claimed exemption");
    }
    None
}

/// Every `.rs` file strictly under `dir`, recursively, appended to `out`, deterministically
/// ordered.
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

/// Scan every `.rs` file under `root/src`, recursively, for a bare-removal finding (spec 79
/// criterion 2's audit) - deterministically ordered by (file, line).
fn scan_tree(root: &Path) -> Vec<Finding> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    let mut findings = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        let excluded = cfg_test_ranges(&lines);
        let sigs = fn_sig_lines(&lines, &excluded);
        for (i, line) in lines.iter().enumerate() {
            if in_ranges(i, &excluded) || line.trim_start().starts_with("//") {
                continue;
            }
            let shape = remove_dir_all_shape(line).or_else(|| worktree_remove_shape(&lines, i));
            let Some(shape) = shape else {
                continue;
            };
            let span = enclosing_fn_span(&lines, &sigs, i);
            if !is_covered(&lines, span, i) {
                findings.push(Finding {
                    file: rel.clone(),
                    line_no: i + 1,
                    shape,
                    line_text: (*line).to_string(),
                });
            }
        }
    }
    findings
}

/// Every `(file, 1-based line)` in `root/src` that carries the claimed-exemption marker (spec
/// 79's Design: "the exemption is claimed in a code comment at the site" - this is the audit
/// LISTING those claims, not merely accepting them silently), deterministically ordered.
fn find_exemption_markers(root: &Path) -> Vec<(String, usize)> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    let mut hits = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            if line.contains(EXEMPTION_MARKER) {
                hits.push((rel.clone(), i + 1));
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }

    #[test]
    fn a_clean_fixture_tree_yields_no_findings() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/worktree.rs",
            "\
fn clear_worktree_dir(dir: &str, authorized_root: &str) {
    reap_dir_before_removal(dir, authorized_root);
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    #[test]
    fn bare_remove_dir_all_with_no_coverage_is_caught() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file, "src/somewhere.rs");
        assert_eq!(findings[0].line_no, 2);
        assert!(findings[0].shape.contains("remove_dir_all"), "{findings:?}");
    }

    #[test]
    fn bare_git_worktree_remove_with_no_coverage_is_caught() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(repo: &str, dir: &str) {
    let _ = std::process::Command::new(\"git\")
        .args([\"worktree\", \"remove\", \"--force\", dir])
        .output();
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].shape.contains("git worktree remove"),
            "{findings:?}"
        );
    }

    #[test]
    fn a_reap_call_anywhere_earlier_in_the_enclosing_function_covers_the_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str, root: &str) {
    let other = 1;
    reap_processes_rooted_under(std::path::Path::new(dir), std::path::Path::new(root));
    let also = other + 1;
    let _ = std::fs::remove_dir_all(dir);
    let _ = also;
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a reap call earlier in the SAME function must cover the removal; {findings:?}"
        );
    }

    /// ROUND-3 FIX (upholding
    /// `sdet-u79c2r2-authority-name-in-prose-comment-still-falsely-covers`): a reap-authority
    /// NAME appearing only in a prose comment - never a real call - must not be mistaken for
    /// coverage, mirroring `scan_tree`'s own removal-line comment guard.
    #[test]
    fn a_reap_authority_name_in_a_prose_comment_never_covers_the_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    // TODO: call reap_processes_rooted_under( here eventually, not done yet
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "an authority name in prose, with no real call, must never cover; {findings:?}"
        );
        assert_eq!(findings[0].line_no, 3, "{findings:?}");
    }

    /// ROUND-4 FIX (upholding
    /// `arch-u79c2r3-comment-guard-is-whole-line-only-trailing-comment-still-falsely-covers`):
    /// the round-3 comment guard only skips a line whose TRIMMED text starts with `//` - a
    /// TRAILING comment on a real code line that happens to name a reap authority must still
    /// never be mistaken for a real call.
    #[test]
    fn a_reap_authority_name_in_a_trailing_comment_never_covers_the_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    let x = 1; // reap_processes_rooted_under(
    let _ = std::fs::remove_dir_all(dir);
    let _ = x;
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "an authority name in a TRAILING comment, with no real call, must never cover; \
             {findings:?}"
        );
    }

    /// ROUND-4 FIX: a real reap call is unaffected by the trailing-comment fix above when a
    /// harmless comment follows it on the SAME line - the call itself sits before the `//`,
    /// so it must still cover.
    #[test]
    fn a_real_reap_call_with_a_trailing_comment_on_the_same_line_still_covers() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    reap_authorized(std::path::PathBuf::from(dir)); // reaps everything rooted here first
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a real call followed by a harmless trailing comment must still cover; {findings:?}"
        );
    }

    /// ROUND-4 FIX (upholding
    /// `sdet-u79c2r3-exemption-marker-check-has-no-comment-guard-at-all`): the round-3
    /// EXEMPTION_MARKER check has no comment requirement at all - the marker substring living
    /// inside a non-comment string literal (a log message) must never be mistaken for a
    /// deliberately claimed exemption.
    #[test]
    fn an_exemption_marker_inside_a_non_comment_string_literal_never_covers_the_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    log::warn!(\"reap-exempt: not a real claim, just a string\");
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "reap-exempt inside a non-comment string literal must never be mistaken for a \
             claimed exemption; {findings:?}"
        );
    }

    /// ROUND-4 FIX: the exemption marker must still cover when it lives in a genuine TRAILING
    /// comment on the same line as real code (not just a whole-line comment), proving the
    /// comment-guard fix above does not over-narrow the marker check to whole-line-only.
    #[test]
    fn an_exemption_marker_in_a_trailing_comment_after_real_code_still_covers() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    let _ = std::fs::remove_dir_all(dir); // reap-exempt (spec 79, criterion 2): trailing form
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a marker in a genuine trailing comment after real code must still cover; \
             {findings:?}"
        );
    }

    /// ROUND-4 FIX (upholding
    /// `adv-u79c2r3-authority-match-inside-noncomment-string-literal-uncaught-by-either-fix`,
    /// reproducing the adjudicator's own probe byte-for-byte): a reap-authority NAME living
    /// inside a non-comment string literal (a log message, never a real call) must never be
    /// mistaken for coverage - neither the comment-guard fix nor the exemption-marker fix above
    /// catches this shape on its own.
    #[test]
    fn a_reap_authority_name_inside_a_non_comment_string_literal_never_covers_the_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    log::debug!(\"about to reap_authorized(dir) then remove\");
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "a reap-authority name inside a non-comment string literal must never be mistaken \
             for a real call; {findings:?}"
        );
    }

    #[test]
    fn the_exemption_marker_covers_a_removal_with_no_reap_call() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    // reap-exempt (spec 79, criterion 2): dir is created and removed entirely within this
    // function and nothing is ever spawned with a cwd inside it.
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "the recognized exemption marker must cover the removal; {findings:?}"
        );
    }

    #[test]
    fn the_exemption_marker_in_the_functions_own_doc_comment_above_the_signature_also_covers_it() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
/// reap-exempt (spec 79, criterion 2): mirrors heal_corrupt_worktree_admin - the marker sits
/// in the doc comment ABOVE the fn signature, never in the body next to the removal itself.
fn f(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a marker in the fn's own leading doc comment must cover the removal; {findings:?}"
        );
    }

    #[test]
    fn an_arbitrary_comment_is_never_mistaken_for_the_exemption_marker() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    // trust me, this one is fine
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "an unrecognized comment must not be accepted as a claimed exemption; {findings:?}"
        );
    }

    #[test]
    fn a_reap_call_only_in_a_sibling_function_never_covers_this_one() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn helper_with_reap(dir: &str, root: &str) {
    reap_processes_rooted_under(std::path::Path::new(dir), std::path::Path::new(root));
}

fn bare_sibling(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "a reap call in a DIFFERENT function must not cover this one's removal; {findings:?}"
        );
        assert_eq!(
            findings[0].line_no, 6,
            "attributed to bare_sibling; {findings:?}"
        );
    }

    #[test]
    fn a_removal_inside_a_cfg_test_mod_block_is_never_scanned() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
#[cfg(test)]
mod tests {
    fn t(dir: &str) {
        let _ = std::fs::remove_dir_all(dir);
    }
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a #[cfg(test)] mod block is out of this spec's scope; {findings:?}"
        );
    }

    #[test]
    fn a_removal_inside_a_standalone_cfg_test_fn_is_never_scanned_and_a_later_real_fn_still_is() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
#[cfg(test)]
fn helper(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}

fn production(dir2: &str) {
    let _ = std::fs::remove_dir_all(dir2);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].line_no, 7,
            "only the production fn's removal is flagged; {findings:?}"
        );
    }

    #[test]
    fn stacked_cfg_attributes_before_a_test_mod_still_exclude_it() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
#[cfg(test)]
#[cfg(feature = \"symbols\")]
mod corpus_gates {
    fn t(dir: &str) {
        let _ = std::fs::remove_dir_all(dir);
    }
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a stacked second attribute before the mod must not defeat the exclusion; {findings:?}"
        );
    }

    #[test]
    fn a_doc_comment_mentioning_the_cfg_test_attribute_in_prose_is_never_mistaken_for_it() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
//! This module's tests live under a #[cfg(test)] mod declared below.
fn production(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "prose merely mentioning the attribute must not exclude real code; {findings:?}"
        );
    }

    #[test]
    fn a_semicolon_terminated_cfg_test_item_excludes_only_itself() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/lib.rs",
            "\
#[cfg(test)]
mod blast_radius_eval;

fn production(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "the semicolon-terminated declaration excludes only itself; {findings:?}"
        );
        assert_eq!(findings[0].line_no, 5, "{findings:?}");
    }

    #[test]
    fn a_bare_removal_under_tests_is_never_scanned() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "tests/somewhere.rs",
            "\
fn f(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "spec 79's audit is src/ only, never tests/; {findings:?}"
        );
    }

    #[test]
    fn a_finding_names_its_exact_file_and_line_number() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/multi_line.rs",
            "\
fn a() {}
fn b(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
fn c() {}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file, "src/multi_line.rs");
        assert_eq!(findings[0].line_no, 3, "{findings:?}");
    }

    #[test]
    fn a_multi_line_fn_signature_still_resolves_its_own_closing_brace() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(
    dir: &str,
    root: &str,
) {
    reap_authorized(std::path::PathBuf::from(dir));
    let _ = std::fs::remove_dir_all(dir);
    let _ = root;
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a multi-line signature must still resolve the same enclosing span; {findings:?}"
        );
    }

    /// ROUND-2 FIX (spec 79 c2, adjudication `adj-u79c2-verdict-reject-detection-blind-
    /// spots`, upholding `arch-u79c2-coverage-check-is-unordered-co-occurrence-not-reap-
    /// then-remove`): a reap call sitting AFTER the removal it supposedly authorizes is
    /// the exact inverted remove-then-reap anti-pattern spec 79's own Design text exists
    /// to forbid ("removed ONLY through a reap-then-remove path") - it must never be
    /// mistaken for coverage.
    #[test]
    fn a_reap_call_after_the_removal_never_covers_it_remove_then_reap_is_still_flagged() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str, root: &str) {
    let _ = std::fs::remove_dir_all(dir);
    reap_processes_rooted_under(std::path::Path::new(dir), std::path::Path::new(root));
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "a reap call textually AFTER the removal (remove-then-reap) must never cover it; \
             {findings:?}"
        );
        assert_eq!(findings[0].line_no, 2, "{findings:?}");
    }

    /// ROUND-2 FIX: an exemption marker has no such ordering requirement of its own (it is
    /// a documented human claim about the whole function, not a call with a
    /// happens-before relationship to the removal) - it must keep covering the removal
    /// even when the comment sits textually AFTER it in the function body.
    #[test]
    fn an_exemption_marker_after_the_removal_still_covers_it() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
    // reap-exempt (spec 79, criterion 2): dir is created and removed entirely within this
    // function and nothing is ever spawned with a cwd inside it.
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "the exemption marker is order-independent, unlike a reap-authority call; \
             {findings:?}"
        );
    }

    /// ROUND-3 FIX (upholding
    /// `arch-u79c2r2-exemption-marker-function-wide-not-site-scoped`): an exemption claimed
    /// for ONE removal in a multi-removal function must never silently cover an unrelated,
    /// unexempted second removal elsewhere in that same function - the marker's coverage is
    /// bounded to the segment between the removal sites, not the whole enclosing function.
    #[test]
    fn an_exemption_marker_attached_to_one_removal_never_covers_an_unrelated_second_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir_a: &str, dir_b: &str) {
    // reap-exempt (spec 79, criterion 2): dir_a is created and removed entirely within this
    // function and nothing is ever spawned with a cwd inside it.
    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "dir_a's exemption must not bleed onto the unrelated dir_b removal; {findings:?}"
        );
        assert_eq!(
            findings[0].line_no, 5,
            "the unexempted dir_b removal is the one that must be flagged; {findings:?}"
        );
    }

    /// ROUND-2 FIX (upholding `adv-u79c2-authorized-root-value-blind-textual-match`):
    /// `reap_dir_before_removal(dir, "")` is src/worktree.rs's own sanctioned no-op form
    /// (its doc comment: "Pass `""` when the caller has no such root... the reap becomes
    /// a no-op") - a GUARANTEED no-op at runtime, textually indistinguishable from a
    /// genuinely-effective call under a name-only substring match. It must never cover.
    #[test]
    fn a_literal_empty_string_authorized_root_argument_never_covers_the_removal() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(dir: &str) {
    reap_dir_before_removal(dir, \"\");
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "reap_dir_before_removal(dir, \"\") reaps nothing at runtime and must never be \
             mistaken for real coverage; {findings:?}"
        );
    }

    /// A real (non-empty) `authorized_root` argument must keep covering, including when
    /// rustfmt wraps the call's own argument list across multiple lines (proves the
    /// argument-value check's call-text extraction is not accidentally single-line-only,
    /// the same class of blindness as the worktree/remove multi-line fix below).
    #[test]
    fn a_non_empty_authorized_root_argument_still_covers_even_when_the_call_wraps_across_lines() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(a_very_long_directory_argument_name: &str, a_very_long_authorized_root_argument_name: &str) {
    reap_dir_before_removal(
        a_very_long_directory_argument_name,
        a_very_long_authorized_root_argument_name,
    );
    let _ = std::fs::remove_dir_all(a_very_long_directory_argument_name);
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a non-empty authorized_root must still cover across a wrapped call; {findings:?}"
        );
    }

    /// ROUND-2 FIX (upholding `sdet-u79c2-multiline-worktree-remove-args-array-evades-
    /// detection`, independently reproduced by the adjudicator with the real project
    /// rustfmt): a plausible, realistically-long-named `.args([...])` call wraps one
    /// element per line under this repo's own default rustfmt config, splitting the
    /// `"worktree"`/`"remove"` pair the old same-line-only match required.
    #[test]
    fn a_worktree_remove_args_array_wrapped_across_multiple_lines_by_rustfmt_is_still_caught() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(repo: &str, a_realistically_long_directory_variable_name: &str) {
    let _ = std::process::Command::new(\"git\")
        .args([
            \"worktree\",
            \"remove\",
            \"--force\",
            a_realistically_long_directory_variable_name,
        ])
        .output();
}
",
        );
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "a wrapped .args([...]) array must still be recognized as a bare worktree-remove \
             call; {findings:?}"
        );
        assert_eq!(findings[0].line_no, 4, "{findings:?}");
        assert!(
            findings[0].shape.contains("git worktree remove"),
            "{findings:?}"
        );
    }

    /// A `"worktree"` token with no paired `"remove"` token anywhere nearby (e.g. a
    /// wrapped `git worktree list --porcelain` call) must never be mistaken for the
    /// forbidden shape - the window lookahead must not over-trigger on an unrelated git
    /// subcommand that merely happens to also take `"worktree"` as its first arg.
    #[test]
    fn a_worktree_token_with_no_nearby_remove_token_is_never_mistaken_for_the_shape() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/somewhere.rs",
            "\
fn f(repo: &str) {
    let _ = std::process::Command::new(\"git\")
        .args([\"worktree\", \"list\", \"--porcelain\"])
        .output();
}
",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "\"worktree\" without a nearby \"remove\" token is not the forbidden shape; \
             {findings:?}"
        );
    }

    #[test]
    fn find_exemption_markers_lists_every_claimed_site() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/a.rs",
            "\
fn f(dir: &str) {
    // reap-exempt (spec 79, criterion 2): created and removed within this function.
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        write_file(
            root.path(),
            "src/b.rs",
            "\
fn g(dir: &str) {
    let _ = std::fs::remove_dir_all(dir);
}
",
        );
        let hits = find_exemption_markers(root.path());
        assert_eq!(hits, vec![("src/a.rs".to_string(), 2)], "{hits:?}");
    }

    /// The Done-when acceptance test itself: `tests/reap_before_removal_audit.rs` scans the
    /// REAL, currently checked-out `src/` tree (resolved from `CARGO_MANIFEST_DIR`, never the
    /// process CWD) and finds zero bare-removal sites - proving criterion 1's rewiring
    /// (`u79c1-rewiring-complete-and-exemption-marker`) actually covers or exempts every
    /// production site, and that no later change introduced a new one.
    #[test]
    fn the_real_tree_carries_no_bare_removal() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let findings = scan_tree(&root);
        assert!(
            findings.is_empty(),
            "bare-removal audit found {} uncovered/unexempted site(s) in the real tree:\n{}",
            findings.len(),
            findings
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// The listing half of spec 79's Design ("the exemption is claimed in a code comment at
    /// the site" - this test names every site actually claiming one at HEAD). A change to this
    /// count is a deliberate, review-worthy event (a new bare site was exempted rather than
    /// reap-wired) - re-ground the expected files/count here if it fails after a legitimate
    /// change, never silence it.
    #[test]
    fn the_real_trees_claimed_exemptions_are_exactly_the_three_u79c1_recorded() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let hits = find_exemption_markers(&root);
        let files: Vec<&str> = hits.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(
            hits.len(),
            3,
            "expected exactly the 3 exemptions u79c1-rewiring-complete-and-exemption-marker \
             recorded (heal_corrupt_worktree_admin, cmd_replay's replay_dir, \
             materialize_config_at_rev's checkout); got {hits:?}"
        );
        assert_eq!(
            files.iter().filter(|f| **f == "src/main.rs").count(),
            2,
            "{hits:?}"
        );
        assert_eq!(
            files.iter().filter(|f| **f == "src/worktree.rs").count(),
            1,
            "{hits:?}"
        );
    }
}
