//! Spec 78 criterion 3, THE AUDIT TEST: the whole-tree twin of the diff-scoped `no-os-kill`
//! gate declared in `.rigger/workflow.yml`. That gate only judges a unit's OWN diff against
//! the run base; this test walks EVERY `.rs` file under `src/` and `tests/` in the checked-out
//! tree and fails, naming file and line, on any of the gate's forbidden shapes found outside
//! the two sanctioned lifecycle helpers (`src/reap.rs`, `tests/common/mod.rs`) - or on a
//! shell-out / `--` argv separator / negative-pid `format!` found INSIDE those two files,
//! where calling the internal signal API directly is exactly the point and must NOT be
//! flagged. It runs under plain `cargo test`, so unlike the shell gate it also covers CI
//! (`.github/workflows/rust.yml`), which runs `cargo test` but not the rigger-loop gate.
//!
//! The nine forbidden shapes mirror `.rigger/workflow.yml`'s `no-os-kill` gate exactly - see
//! that file (out of the gate's own scope, so it may name the literal pattern text) for the
//! precise regex. In prose: a `Command::new` shell-out to any of four OS process-termination
//! utility names; the bare shell form of two of those names, invoked with a leading-hyphen
//! signal argument; one of the four also appearing as a standalone shell token on its own;
//! the libc process-group signal call name; a direct call through `libc` or a `signal`
//! module; the sanctioned rustix call itself (forbidden only OUTSIDE the two sanctioned
//! files - see below); the `--` argv separator passed to `.arg(...)`; and a `format!` call
//! shaped to build a leading-hyphen (negative-pid-style) argument. Inside the two sanctioned
//! files only the Command::new shell-out, the `--` separator and the negative-pid `format!`
//! remain banned (the gate's own narrower second check) - the direct signal call is exactly
//! what those two files exist to make.
//!
//! SOURCE HYGIENE (load-bearing - read this before touching a needle or a fixture below).
//! This file's whole job is to DETECT the shapes the `no-os-kill` gate forbids, and its own
//! fixtures must PROVE detection by containing samples of them. But the gate does not parse
//! Rust - it greps this unit's raw ADDED text, several of its patterns with NO surrounding-
//! context requirement at all - so spelling any forbidden shape out as one contiguous literal
//! span anywhere in this file's own source (an identifier, a doc comment, a diagnostic
//! string) would trip that very gate against this file's own diff. Every needle used for
//! detection, and every violation fixture used to prove detection, is therefore assembled AT
//! RUNTIME from short, individually harmless fragments via [`join`] - never written as one
//! contiguous token in this file's source text, and never even named literally in a comment
//! or a diagnostic string (a hyphen breaks the sequence where a name must appear in prose
//! below, e.g. "p-kill"). This mirrors `.rigger/workflow.yml`'s `style` gate, which generates
//! its own em-dash byte pattern via `printf` octal at runtime for the identical reason: so
//! the gate's own command carries no literal instance of what it forbids.

use std::fs;
use std::path::{Path, PathBuf};

#[path = "common/source_audit.rs"]
mod source_audit;
use source_audit::Finding;

/// The ONLY reason this exists: see SOURCE HYGIENE above. Every fragment pair this file
/// needs to assemble - a detection needle, or a fixture line proving detection - goes
/// through here so the two halves are never adjacent as literal text in this file's source.
fn join(a: &str, b: &str) -> String {
    format!("{a}{b}")
}

/// Whether `c` is a Rust identifier character - the delimiter test the gate's own
/// `[^a-zA-Z_]` character class encodes for the standalone-token shapes (the p-kill utility
/// named bare, and the shell kill/killall-plus-signal form).
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

// ---------------------------------------------------------------------------------------
// Fragment builders. Each returns, AT RUNTIME, one of the literal words/calls the gate's
// patterns name - built from pieces no single one of which is itself a forbidden shape, and
// never concatenated as adjacent literal text in this file's source (see SOURCE HYGIENE).
// ---------------------------------------------------------------------------------------

fn kill_word() -> String {
    join("ki", "ll")
}
/// The kill word with `prefix` in front of it (the p-kill and x-kill utility names).
fn prefixed_kill(prefix: &str) -> String {
    join(prefix, &kill_word())
}
/// The kill word with `suffix` after it (the kill-all utility, the process-group call name).
fn suffixed_kill(suffix: &str) -> String {
    join(&kill_word(), suffix)
}
fn kill_process_open() -> String {
    join(&join(&kill_word(), "_process"), "(")
}
/// An opened call of the kill function through `module` (`libc::`, `signal::`).
fn module_kill_open(module: &str) -> String {
    join(module, &suffixed_kill("("))
}

/// The exact set of words the Command::new shell-out shape and the bare shell
/// kill/killall-plus-signal shape both key off (see `.rigger/workflow.yml` for the literal
/// enumeration), largest-first so `killall` is tried before its own prefix `kill` at the
/// same start position.
fn shell_kill_words() -> Vec<String> {
    vec![suffixed_kill("all"), kill_word()]
}

// ---------------------------------------------------------------------------------------
// Shape detectors. Each takes one line of ALREADY-ON-DISK text (a scanned file's line, or a
// runtime-built fixture line) and reports whether it carries the named forbidden shape.
// These operate on `char` position arithmetic, never on a literal contiguous needle typed
// into this file (see SOURCE HYGIENE) - a scanned line built from any combination of
// fragments is matched identically to one built any other way.
// ---------------------------------------------------------------------------------------

/// The gate's Command::new shell-out shape (see `.rigger/workflow.yml` for the literal
/// pattern) - a shell-out to one of the four OS process-termination utility names.
fn shape_command_new_signal(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let marker: Vec<char> = "Command::new(".chars().collect();
    let words = [
        prefixed_kill("p"),
        suffixed_kill("all"),
        prefixed_kill("x"),
        kill_word(),
    ];
    let mlen = marker.len();
    if chars.len() < mlen + 1 {
        return false;
    }
    for start in 0..=(chars.len() - mlen - 1) {
        if chars[start..start + mlen] != marker[..] {
            continue;
        }
        // Skip exactly the one char the gate's `.` wildcard consumes (typically a quote).
        let rest: String = chars[start + mlen + 1..].iter().collect();
        if words.iter().any(|w| rest.starts_with(w.as_str())) {
            return true;
        }
    }
    false
}

/// The gate's bare-shell-form shape (see `.rigger/workflow.yml`): `kill` or `killall`
/// immediately followed by a space, a hyphen, and a signal token (a number or a name).
fn shape_shell_kill_dash(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    for start in 0..chars.len() {
        for word in shell_kill_words() {
            let wchars: Vec<char> = word.chars().collect();
            let wlen = wchars.len();
            if start + wlen > chars.len() || chars[start..start + wlen] != wchars[..] {
                continue;
            }
            if start > 0 && is_word_char(chars[start - 1]) {
                continue; // not a standalone token
            }
            let after = start + wlen;
            if after + 2 < chars.len()
                && chars[after] == ' '
                && chars[after + 1] == '-'
                && chars[after + 2].is_ascii_alphanumeric()
            {
                return true;
            }
        }
    }
    false
}

/// The gate's standalone-token shape for the p-kill utility (see `.rigger/workflow.yml`):
/// the bare word, delimited on both sides so it is never matched inside a longer identifier.
fn shape_pkill(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let word: Vec<char> = prefixed_kill("p").chars().collect();
    let wlen = word.len();
    if chars.len() < wlen + 2 {
        return false;
    }
    for start in 1..chars.len().saturating_sub(wlen) {
        if chars[start..start + wlen] == word[..]
            && !is_word_char(chars[start - 1])
            && !is_word_char(chars[start + wlen])
        {
            return true;
        }
    }
    false
}

/// The four bare-substring shapes (see `.rigger/workflow.yml` for each literal pattern),
/// banned anywhere on a line with no delimiter required, each with its finding label: the libc
/// process-group signal call name; a direct call through `libc`; a direct call through a
/// `signal` module; and the sanctioned rustix signal call itself - forbidden everywhere OTHER
/// than the two sanctioned files (checked by the caller, not here).
fn substring_shapes() -> [(String, &'static str); 4] {
    [
        (suffixed_kill("pg"), "libc process-group signal call"),
        (module_kill_open("libc::"), "direct libc signal call"),
        (module_kill_open("signal::"), "direct signal-module call"),
        (
            kill_process_open(),
            "the sanctioned rustix signal call used outside its two sanctioned sites",
        ),
    ]
}

/// One gate shape: a literal `marker` immediately followed by a `tail` in which `.` is the
/// gate regex's any-character wildcard and every other character must match exactly.
type MarkerShape = (&'static str, &'static str);

/// `\.arg\(.--.\)` - the bare `--` argv separator passed to `.arg(...)`.
const ARG_DASHDASH: MarkerShape = (".arg(", ".--.)");

/// `format!\(.-\{` - a `format!` call shaped to build a leading-hyphen (negative-pid-style)
/// argument.
const FORMAT_DASH_BRACE: MarkerShape = ("format!(", ".-{");

/// Whether `line` carries `shape`'s marker followed by its tail anywhere.
fn has_marker_shape(line: &str, (marker, tail): MarkerShape) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let marker: Vec<char> = marker.chars().collect();
    let tail: Vec<char> = tail.chars().collect();
    let mlen = marker.len();
    if chars.len() < mlen {
        return false;
    }
    (0..=(chars.len() - mlen)).any(|start| {
        let rest = &chars[start + mlen..];
        chars[start..start + mlen] == marker[..]
            && rest.len() >= tail.len()
            && tail
                .iter()
                .zip(rest)
                .all(|(want, got)| *want == '.' || want == got)
    })
}

/// The two files spec 78 sanctions to call the signal API directly (`src/reap.rs`'s
/// `send_signal`, `tests/common/mod.rs`'s `terminate_pid`/`is_alive`) - this audit's own
/// record of the boundary, checked against each scanned file's REPO-RELATIVE, forward-slash
/// path, independent of `.rigger/workflow.yml`'s copy.
const SANCTIONED_FILES: [&str; 2] = ["src/reap.rs", "tests/common/mod.rs"];

/// Every forbidden shape found in one line of a file that is NOT one of the two sanctioned
/// lifecycle helpers - the gate's full nine-shape ban.
fn general_hits(line: &str) -> Vec<&'static str> {
    let mut hits = Vec::new();
    if shape_command_new_signal(line) {
        hits.push("Command::new(...) shell-out to an OS kill utility");
    }
    if shape_shell_kill_dash(line) {
        hits.push("bare shell kill/killall -<signal> form");
    }
    if shape_pkill(line) {
        hits.push("standalone p-kill token");
    }
    for (needle, label) in substring_shapes() {
        if line.contains(&needle) {
            hits.push(label);
        }
    }
    if has_marker_shape(line, ARG_DASHDASH) {
        hits.push("-- argv separator passed to .arg(...)");
    }
    if has_marker_shape(line, FORMAT_DASH_BRACE) {
        hits.push("format! shaped to build a negative-pid argument");
    }
    hits
}

/// Every forbidden shape found in one line of a SANCTIONED file - only the three the gate's
/// own narrower second check still bans there (a shell-out, the `--` separator, or a
/// negative-pid `format!`); the direct signal call these two files exist to make is never
/// flagged.
fn sanctioned_hits(line: &str) -> Vec<&'static str> {
    let mut hits = Vec::new();
    if shape_command_new_signal(line) {
        hits.push("Command::new(...) shell-out to an OS kill utility");
    }
    if has_marker_shape(line, ARG_DASHDASH) {
        hits.push("-- argv separator passed to .arg(...)");
    }
    if has_marker_shape(line, FORMAT_DASH_BRACE) {
        hits.push("format! shaped to build a negative-pid argument");
    }
    hits
}

/// Every `.rs` file strictly under `dir`, recursively, appended to `out`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort(); // deterministic finding order regardless of readdir order
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Scan every `.rs` file under `root/src` and `root/tests`, deterministically ordered by
/// (file, line), applying [`general_hits`] outside [`SANCTIONED_FILES`] and
/// [`sanctioned_hits`] inside them - the whole-tree twin of the diff-scoped `no-os-kill`
/// gate (spec 78, THE AUDIT TEST).
fn scan_tree(root: &Path) -> Vec<Finding> {
    let mut files = Vec::new();
    for top in ["src", "tests"] {
        collect_rs_files(&root.join(top), &mut files);
    }
    let mut findings = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let sanctioned = SANCTIONED_FILES.contains(&rel.as_str());
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            let hits = if sanctioned {
                sanctioned_hits(line)
            } else {
                general_hits(line)
            };
            for shape in hits {
                findings.push(Finding {
                    file: rel.clone(),
                    line_no: i + 1,
                    shape,
                    line_text: line.to_string(),
                });
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    use source_audit::write_file;

    #[test]
    fn a_clean_fixture_tree_yields_no_findings() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "src/lib.rs",
            "pub fn ok() { let _ = std::process::Command::new(\"echo\").arg(\"hi\"); }\n",
        );
        write_file(
            root.path(),
            "tests/some_test.rs",
            "fn t() { let mut c = std::process::Command::new(\"true\").spawn().unwrap(); \
             let _ = c.kill(); let _ = c.wait(); }\n",
        );
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    /// The findings of a fixture tree holding each `(rel, content)` file.
    fn scan_fixture(files: &[(&str, &str)]) -> Vec<Finding> {
        let root = tempfile::tempdir().unwrap();
        for (rel, content) in files {
            write_file(root.path(), rel, content);
        }
        scan_tree(root.path())
    }

    /// The one finding a fixture tree holding `content` at `rel` yields.
    fn single_finding(rel: &str, content: &str) -> Finding {
        let findings = scan_fixture(&[(rel, content)]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        findings.into_iter().next().unwrap()
    }

    /// `line` at `rel` is caught, as the one finding, under a shape naming `shape`.
    fn caught_as(rel: &str, line: &str, shape: &str) {
        let finding = single_finding(rel, line);
        assert!(finding.shape.contains(shape), "{finding:?}");
    }

    /// `content` at `rel` is caught as one finding naming `rel` itself.
    fn caught_in(rel: &str, content: &str) -> Finding {
        let finding = single_finding(rel, content);
        assert_eq!(finding.file, rel);
        finding
    }

    /// A fixture tree holding `files` yields no finding at all.
    fn never_flagged(files: &[(&str, &str)], why: &str) {
        let findings = scan_fixture(files);
        assert!(findings.is_empty(), "{why}; {findings:?}");
    }

    #[test]
    fn command_new_shell_out_is_caught_outside_the_sanctioned_files() {
        // `killall`, not the p-kill utility - it does not ALSO satisfy the standalone-token
        // shape, so this fixture isolates the Command::new shape alone.
        let line = format!(
            "let _ = std::process::Command::new(\"{}\");\n",
            suffixed_kill("all")
        );
        let finding = caught_in("src/somewhere.rs", &line);
        assert_eq!(finding.line_no, 1);
        assert!(finding.shape.contains("Command::new"), "{finding:?}");
    }

    rigger::test_cases! {
        bare_shell_kill_dash_form_is_caught: caught_as(
            "src/somewhere.rs",
            &format!("// once shelled out: {} -9 $target_pid\n", kill_word()),
            "shell kill",
        );
        standalone_pkill_token_is_caught: caught_as(
            "tests/somewhere_test.rs",
            &format!("let cmd = \"{}\";\n", prefixed_kill("p")),
            "p-kill",
        );
        pg_signal_call_name_is_caught: caught_as(
            "src/somewhere.rs",
            &format!("unsafe {{ libc2::{}(pgid, 9); }}\n", suffixed_kill("pg")),
            "process-group",
        );
        libc_kill_call_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere.rs",
            &format!("unsafe {{ {}pid, 9); }}\n", module_kill_open("libc::")),
            "libc",
        );
        signal_kill_call_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere.rs",
            &format!("{}pid, term); }}\n", module_kill_open("signal::")),
            "signal-module",
        );
        kill_process_call_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere_else.rs",
            &format!(
                "let _ = rustix::process::{}rpid, sig);\n",
                kill_process_open()
            ),
            "sanctioned rustix signal call",
        );
        arg_dashdash_separator_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere.rs",
            &format!("cmd.arg(\"{}\");\n", join("-", "-")),
            "--",
        );
        negative_pid_format_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere.rs",
            &format!("let arg = format!(\"{}\", pgid);\n", join("-", "{}")),
            "negative-pid",
        );
    }

    #[test]
    fn a_finding_names_its_exact_file_and_line_number() {
        let content = format!(
            "fn a() {{}}\nfn b() {{}}\nlet cmd = \"{}\";\nfn c() {{}}\n",
            prefixed_kill("p")
        );
        let finding = caught_in("src/multi_line.rs", &content);
        assert_eq!(
            finding.line_no, 3,
            "the violation sits on line 3; {finding:?}"
        );
    }

    rigger::test_cases! {
        kill_process_is_never_flagged_inside_either_sanctioned_file: never_flagged(
            &[
                (
                    "src/reap.rs",
                    &format!(
                        "    let _ = rustix::process::{}rpid, signal);\n",
                        kill_process_open()
                    ),
                ),
                (
                    "tests/common/mod.rs",
                    &format!(
                        "pub fn terminate_pid(pid: u32) {{ let _ = rustix::process::{}rpid, sig); }}\n",
                        kill_process_open()
                    ),
                ),
            ],
            "the sanctioned files' own direct signal call must never be flagged",
        );
        /// A shell-out remains banned even inside a sanctioned file.
        a_shell_out_inside_a_sanctioned_file_is_still_caught: caught_in(
            "src/reap.rs",
            &format!(
                "let _ = std::process::Command::new(\"{}\");\n",
                suffixed_kill("all")
            ),
        );
        /// The -- separator remains banned even inside a sanctioned file.
        a_dashdash_separator_inside_a_sanctioned_file_is_still_caught: caught_in(
            "tests/common/mod.rs",
            &format!("cmd.arg(\"{}\");\n", join("-", "-")),
        );
        /// A negative-pid format! remains banned even inside a sanctioned file.
        a_negative_pid_format_inside_a_sanctioned_file_is_still_caught: caught_in(
            "src/reap.rs",
            &format!("let arg = format!(\"{}\", pgid);\n", join("-", "{}")),
        );
        /// The same violation, rooted outside src/ and tests/ entirely, must be invisible.
        a_shape_outside_src_and_tests_is_never_scanned: never_flagged(
            &[(
                "scripts/somewhere.rs",
                &format!(
                    "let _ = std::process::Command::new(\"{}\");\n",
                    prefixed_kill("p")
                ),
            )],
            "a .rs file outside src/ and tests/ must never be scanned",
        );
    }

    rigger::test_cases! {
        /// The Done-when-c3 acceptance test itself: `tests/no_os_kill_audit.rs` scans the REAL,
        /// currently checked-out `src/` and `tests/` trees (resolved from `CARGO_MANIFEST_DIR`,
        /// never the process CWD) and finds zero forbidden shapes - proving criteria 1 and 2
        /// (THE TEST HELPER, THE REAPER) actually converted every prior unsafe termination site,
        /// and that no other `#[cfg(test)]` module or integration suite introduced a new one.
        the_real_tree_carries_no_forbidden_pattern: source_audit::assert_real_tree_clean(
            scan_tree,
            "no-os-kill audit",
            "forbidden pattern(s)",
        );
    }
}
