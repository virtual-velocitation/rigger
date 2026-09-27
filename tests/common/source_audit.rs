//! Shared support for the whole-tree source audits (`tests/no_os_kill_audit.rs`,
//! `tests/reap_before_removal_audit.rs`): the one finding shape both report, the fixture-file
//! writer both prove detection with, and the real-tree assertion both close on. Included by
//! each suite through `#[path]`, never through `tests/common/mod.rs`. Also the line-level
//! source reading those audits and `tests/boundary_audit.rs` share: which lines are test code,
//! and which function encloses a line. Each suite uses the subset it needs (hence the
//! module-wide `dead_code` allowance, as in `tests/common/mod.rs`).

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
