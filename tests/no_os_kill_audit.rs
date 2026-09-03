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
fn pkill_word() -> String {
    join("p", &kill_word())
}
fn killall_word() -> String {
    join(&kill_word(), "all")
}
fn xkill_word() -> String {
    join("x", &kill_word())
}
fn pg_signal_word() -> String {
    join(&kill_word(), "pg")
}
fn kill_process_open() -> String {
    join(&join(&kill_word(), "_process"), "(")
}
fn libc_kill_open() -> String {
    join("libc::", &join(&kill_word(), "("))
}
fn signal_kill_open() -> String {
    join("signal::", &join(&kill_word(), "("))
}

/// The exact set of words the Command::new shell-out shape and the bare shell
/// kill/killall-plus-signal shape both key off (see `.rigger/workflow.yml` for the literal
/// enumeration), largest-first so `killall` is tried before its own prefix `kill` at the
/// same start position.
fn shell_kill_words() -> Vec<String> {
    vec![killall_word(), kill_word()]
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
    let words = [pkill_word(), killall_word(), xkill_word(), kill_word()];
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
    let word: Vec<char> = pkill_word().chars().collect();
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

/// The libc process-group signal call name (see `.rigger/workflow.yml`), banned as a bare
/// substring anywhere - no delimiter required.
fn shape_pg_signal(line: &str) -> bool {
    line.contains(&pg_signal_word())
}

/// A direct call through `libc` (see `.rigger/workflow.yml` for the literal pattern).
fn shape_libc_kill(line: &str) -> bool {
    line.contains(&libc_kill_open())
}

/// A direct call through a `signal` module (see `.rigger/workflow.yml`).
fn shape_signal_kill(line: &str) -> bool {
    line.contains(&signal_kill_open())
}

/// The sanctioned rustix signal call itself (see `.rigger/workflow.yml`); forbidden
/// everywhere OTHER than the two sanctioned files (checked by the caller, not here).
fn shape_direct_rustix_call(line: &str) -> bool {
    line.contains(&kill_process_open())
}

/// `\.arg\(.--.\)` - the bare `--` argv separator passed to `.arg(...)`.
fn shape_arg_dashdash(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let marker: Vec<char> = ".arg(".chars().collect();
    let mlen = marker.len();
    if chars.len() < mlen {
        return false;
    }
    for start in 0..=(chars.len() - mlen) {
        if chars[start..start + mlen] != marker[..] {
            continue;
        }
        let x = start + mlen; // position of the gate's leading `.` wildcard
        if x + 4 >= chars.len() {
            continue;
        }
        if chars[x + 1] == '-' && chars[x + 2] == '-' && chars[x + 4] == ')' {
            return true;
        }
    }
    false
}

/// `format!\(.-\{` - a `format!` call shaped to build a leading-hyphen (negative-pid-style)
/// argument.
fn shape_format_dash_brace(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let marker: Vec<char> = "format!(".chars().collect();
    let mlen = marker.len();
    if chars.len() < mlen {
        return false;
    }
    for start in 0..=(chars.len() - mlen) {
        if chars[start..start + mlen] != marker[..] {
            continue;
        }
        let x = start + mlen; // position of the gate's leading `.` wildcard
        if x + 2 >= chars.len() {
            continue;
        }
        if chars[x + 1] == '-' && chars[x + 2] == '{' {
            return true;
        }
    }
    false
}

/// The two files spec 78 sanctions to call the signal API directly (`src/reap.rs`'s
/// `send_signal`, `tests/common/mod.rs`'s `terminate_pid`/`is_alive`) - this audit's own
/// record of the boundary, checked against each scanned file's REPO-RELATIVE, forward-slash
/// path, independent of `.rigger/workflow.yml`'s copy.
const SANCTIONED_FILES: [&str; 2] = ["src/reap.rs", "tests/common/mod.rs"];

/// One forbidden-shape hit: which file, which 1-based line, which shape, and the offending
/// line text (for the failure message only - never re-scanned).
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
    if shape_pg_signal(line) {
        hits.push("libc process-group signal call");
    }
    if shape_libc_kill(line) {
        hits.push("direct libc signal call");
    }
    if shape_signal_kill(line) {
        hits.push("direct signal-module call");
    }
    if shape_direct_rustix_call(line) {
        hits.push("the sanctioned rustix signal call used outside its two sanctioned sites");
    }
    if shape_arg_dashdash(line) {
        hits.push("-- argv separator passed to .arg(...)");
    }
    if shape_format_dash_brace(line) {
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
    if shape_arg_dashdash(line) {
        hits.push("-- argv separator passed to .arg(...)");
    }
    if shape_format_dash_brace(line) {
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

    #[test]
    fn command_new_shell_out_is_caught_outside_the_sanctioned_files() {
        let root = tempfile::tempdir().unwrap();
        // `killall`, not the p-kill utility - it does not ALSO satisfy the standalone-token
        // shape, so this fixture isolates the Command::new shape alone.
        let line = format!(
            "let _ = std::process::Command::new(\"{}\");\n",
            killall_word()
        );
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file, "src/somewhere.rs");
        assert_eq!(findings[0].line_no, 1);
        assert!(findings[0].shape.contains("Command::new"), "{findings:?}");
    }

    #[test]
    fn bare_shell_kill_dash_form_is_caught() {
        let root = tempfile::tempdir().unwrap();
        let line = format!("// once shelled out: {} -9 $target_pid\n", kill_word());
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("shell kill"), "{findings:?}");
    }

    #[test]
    fn standalone_pkill_token_is_caught() {
        let root = tempfile::tempdir().unwrap();
        let line = format!("let cmd = \"{}\";\n", pkill_word());
        write_file(root.path(), "tests/somewhere_test.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("p-kill"), "{findings:?}");
    }

    #[test]
    fn pg_signal_call_name_is_caught() {
        let root = tempfile::tempdir().unwrap();
        let line = format!("unsafe {{ libc2::{}(pgid, 9); }}\n", pg_signal_word());
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("process-group"), "{findings:?}");
    }

    #[test]
    fn libc_kill_call_is_caught_outside_the_sanctioned_files() {
        let root = tempfile::tempdir().unwrap();
        let line = format!("unsafe {{ {}pid, 9); }}\n", libc_kill_open());
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("libc"), "{findings:?}");
    }

    #[test]
    fn signal_kill_call_is_caught_outside_the_sanctioned_files() {
        let root = tempfile::tempdir().unwrap();
        let line = format!("{}pid, term); }}\n", signal_kill_open());
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("signal-module"), "{findings:?}");
    }

    #[test]
    fn kill_process_call_is_caught_outside_the_sanctioned_files() {
        let root = tempfile::tempdir().unwrap();
        let line = format!(
            "let _ = rustix::process::{}rpid, sig);\n",
            kill_process_open()
        );
        write_file(root.path(), "src/somewhere_else.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].shape.contains("sanctioned rustix signal call"),
            "{findings:?}"
        );
    }

    #[test]
    fn arg_dashdash_separator_is_caught_outside_the_sanctioned_files() {
        let root = tempfile::tempdir().unwrap();
        let dashes = join("-", "-");
        let line = format!("cmd.arg(\"{dashes}\");\n");
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("--"), "{findings:?}");
    }

    #[test]
    fn negative_pid_format_is_caught_outside_the_sanctioned_files() {
        let root = tempfile::tempdir().unwrap();
        let shape = join("-", "{}");
        let line = format!("let arg = format!(\"{shape}\", pgid);\n");
        write_file(root.path(), "src/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].shape.contains("negative-pid"), "{findings:?}");
    }

    #[test]
    fn a_finding_names_its_exact_file_and_line_number() {
        let root = tempfile::tempdir().unwrap();
        let content = format!(
            "fn a() {{}}\nfn b() {{}}\nlet cmd = \"{}\";\nfn c() {{}}\n",
            pkill_word()
        );
        write_file(root.path(), "src/multi_line.rs", &content);
        let findings = scan_tree(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file, "src/multi_line.rs");
        assert_eq!(
            findings[0].line_no, 3,
            "the violation sits on line 3; {findings:?}"
        );
    }

    #[test]
    fn kill_process_is_never_flagged_inside_either_sanctioned_file() {
        let root = tempfile::tempdir().unwrap();
        let reap_line = format!(
            "    let _ = rustix::process::{}rpid, signal);\n",
            kill_process_open()
        );
        write_file(root.path(), "src/reap.rs", &reap_line);
        let helper_line = format!(
            "pub fn terminate_pid(pid: u32) {{ let _ = rustix::process::{}rpid, sig); }}\n",
            kill_process_open()
        );
        write_file(root.path(), "tests/common/mod.rs", &helper_line);
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "the sanctioned files' own direct signal call must never be flagged; {findings:?}"
        );
    }

    #[test]
    fn a_shell_out_inside_a_sanctioned_file_is_still_caught() {
        let root = tempfile::tempdir().unwrap();
        let line = format!(
            "let _ = std::process::Command::new(\"{}\");\n",
            killall_word()
        );
        write_file(root.path(), "src/reap.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "a shell-out remains banned even inside a sanctioned file; {findings:?}"
        );
        assert_eq!(findings[0].file, "src/reap.rs");
    }

    #[test]
    fn a_dashdash_separator_inside_a_sanctioned_file_is_still_caught() {
        let root = tempfile::tempdir().unwrap();
        let dashes = join("-", "-");
        let line = format!("cmd.arg(\"{dashes}\");\n");
        write_file(root.path(), "tests/common/mod.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "the -- separator remains banned even inside a sanctioned file; {findings:?}"
        );
        assert_eq!(findings[0].file, "tests/common/mod.rs");
    }

    #[test]
    fn a_negative_pid_format_inside_a_sanctioned_file_is_still_caught() {
        let root = tempfile::tempdir().unwrap();
        let shape = join("-", "{}");
        let line = format!("let arg = format!(\"{shape}\", pgid);\n");
        write_file(root.path(), "src/reap.rs", &line);
        let findings = scan_tree(root.path());
        assert_eq!(
            findings.len(),
            1,
            "a negative-pid format! remains banned even inside a sanctioned file; {findings:?}"
        );
        assert_eq!(findings[0].file, "src/reap.rs");
    }

    #[test]
    fn a_shape_outside_src_and_tests_is_never_scanned() {
        let root = tempfile::tempdir().unwrap();
        // Same violation, but rooted outside src/ and tests/ entirely - must be invisible.
        let line = format!(
            "let _ = std::process::Command::new(\"{}\");\n",
            pkill_word()
        );
        write_file(root.path(), "scripts/somewhere.rs", &line);
        let findings = scan_tree(root.path());
        assert!(
            findings.is_empty(),
            "a .rs file outside src/ and tests/ must never be scanned; {findings:?}"
        );
    }

    /// The Done-when-c3 acceptance test itself: `tests/no_os_kill_audit.rs` scans the REAL,
    /// currently checked-out `src/` and `tests/` trees (resolved from `CARGO_MANIFEST_DIR`,
    /// never the process CWD) and finds zero forbidden shapes - proving criteria 1 and 2
    /// (THE TEST HELPER, THE REAPER) actually converted every prior unsafe termination site,
    /// and that no other `#[cfg(test)]` module or integration suite introduced a new one.
    #[test]
    fn the_real_tree_carries_no_forbidden_pattern() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let findings = scan_tree(&root);
        assert!(
            findings.is_empty(),
            "no-os-kill audit found {} forbidden pattern(s) in the real tree:\n{}",
            findings.len(),
            findings
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
