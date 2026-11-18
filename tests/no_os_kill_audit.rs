//! Spec 78 criterion 3, THE AUDIT TEST: the whole-tree twin of the diff-scoped `no-os-kill`
//! gate declared in `.rigger/workflow.yml`. That gate only judges a unit's OWN diff against
//! the run base; this test walks EVERY `.rs` file under `src/`, `crates/` and `tests/` in the checked-out
//! tree and fails, naming file and line, on any of the gate's forbidden shapes found outside
//! the two sanctioned lifecycle helpers (`crates/rigger-process/src/reap.rs`, `tests/common/mod.rs`) - or on a
//! shell-out / `--` argv separator / negative-pid `format!` found INSIDE those two files (and
//! on any reaper-only shape inside the test helper),
//! where calling the internal signal API directly is exactly the point and must NOT be
//! flagged. It runs under plain `cargo test`, so unlike the shell gate it also covers CI
//! (`.github/workflows/rust.yml`), which runs `cargo test` but not the rigger-loop gate.
//!
//! The fourteen forbidden shapes mirror `.rigger/gates/content.sh`'s `no-os-kill` check exactly - see
//! that file (out of the gate's own scope, so it may name the literal pattern text) for the
//! precise regex. In prose: a `Command::new` shell-out to any of five OS process-termination
//! utility names, the Windows one among them, the name in any case; the bare shell form of two
//! of those names, invoked with a leading-hyphen signal argument; one of the five also appearing
//! as a standalone shell token on its own; the libc process-group signal call name; a direct call
//! through `libc` or a `signal` module; the sanctioned rustix call itself (forbidden only
//! OUTSIDE the two sanctioned files - see below); the `--` argv separator passed to
//! `.arg(...)`; a `format!` call shaped to build a leading-hyphen (negative-pid-style)
//! argument; an opened call of either Windows termination function (the process one and the
//! job-object one); and the four confined signal calls: an opened call of the rustix
//! process-group function, and the thread, queued and descriptor signal call names. The last six
//! are the reaper-only shapes, forbidden in every file but `reap.rs`. Inside the two sanctioned
//! files the Command::new shell-out, the `--` separator and the negative-pid `format!` remain
//! banned (the gate's own narrower second check), and so do the reaper-only shapes inside the test
//! helper - the direct unix signal call is exactly what those two files exist to make, and the
//! reaper's `send_signal` and `end_job` are the Windows termination sites.
//!
//! The test module also runs the shipped script itself, by path, over one fixture unit branch
//! per row, so the script's own patterns for the reaper-only shapes are pinned by its exit and
//! its whole output, not only mirrored by this file's needles.
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
//! below, e.g. "p-kill"). This mirrors `.rigger/gates/content.sh`'s `style` check, which generates
//! its own em-dash byte pattern via `printf` octal at runtime for the identical reason: so
//! the gate's own command carries no literal instance of what it forbids.

use crate::common;
use common::repo::collect_rs_files;

use std::fs;
use std::path::Path;

use crate::source_audit;
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
/// An opened call of the Windows process-termination function (Terminate-Process).
fn windows_termination_open() -> String {
    join("Terminate", "Process(")
}
/// An opened call of the Windows job-termination function (Terminate-JobObject), whose one site
/// is the reaper's `end_job`, ending a job object the process created.
fn job_termination_open() -> String {
    join("TerminateJob", "Object(")
}
/// An opened call of the rustix process-group signal function (kill-process-group).
fn group_signal_open() -> String {
    join(&join(&kill_word(), "_process"), "_group(")
}
/// The thread signal call name (t-g-kill).
fn thread_signal_word() -> String {
    prefixed_kill("tg")
}
/// The queued signal call name (sig-queue).
fn queued_signal_word() -> String {
    join("sig", "queue")
}
/// The descriptor signal call name (pidfd-send-signal).
fn descriptor_signal_word() -> String {
    join("pidfd_send", "_signal")
}

/// The exact set of words the Command::new shell-out shape and the bare shell
/// kill/killall-plus-signal shape both key off (see `.rigger/gates/content.sh` for the literal
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

/// The gate's Command::new shell-out shape (see `.rigger/gates/content.sh` for the literal
/// pattern) - a shell-out to one of the five OS process-termination utility names, the Windows
/// one (task-kill) among them, matched in any case as the gate matches it: Windows, and macOS's
/// default filesystem, resolve a program name case-insensitively.
fn shape_command_new_signal(line: &str) -> bool {
    let chars: Vec<char> = line.to_ascii_lowercase().chars().collect();
    let marker: Vec<char> = "command::new(".chars().collect();
    let words = [
        prefixed_kill("p"),
        suffixed_kill("all"),
        prefixed_kill("x"),
        prefixed_kill("task"),
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

/// The gate's bare-shell-form shape (see `.rigger/gates/content.sh`): `kill` or `killall`
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

/// The gate's standalone-token shape for the p-kill utility (see `.rigger/gates/content.sh`):
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

/// The four bare-substring shapes (see `.rigger/gates/content.sh` for each literal pattern),
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

/// The two files spec 78 sanctions to call the signal API directly (`crates/rigger-process/src/reap.rs`'s
/// `send_signal`, `tests/common/mod.rs`'s `delivered`, behind `terminate_pid`, `stop_pid` and
/// `interrupt_pid`, and `is_alive`) - this audit's own
/// record of the boundary, checked against each scanned file's REPO-RELATIVE, forward-slash
/// path, independent of `.rigger/gates/content.sh`'s copy.
const SANCTIONED_FILES: [&str; 2] = [REAPER, "tests/common/mod.rs"];

/// The reaper, whose `send_signal` and `end_job` are the sites of the two Windows termination
/// calls: the one file [`scan_tree`] never flags for a [`reaper_only_shapes`] entry (the test
/// helper is sanctioned for the unix signal call only).
const REAPER: &str = "crates/rigger-process/src/reap.rs";

/// The shapes only [`REAPER`] may carry, each with its finding label: both Windows termination
/// calls, then the four confined signal calls (the rustix process-group call, opened, and the
/// thread, queued and descriptor signal call names, bare), as the gate's own pattern names them.
fn reaper_only_shapes() -> [(String, &'static str); 6] {
    [
        (
            windows_termination_open(),
            "Windows process-termination call outside reap.rs",
        ),
        (
            job_termination_open(),
            "Windows job-termination call outside reap.rs",
        ),
        (
            group_signal_open(),
            "rustix process-group call outside reap.rs",
        ),
        (thread_signal_word(), "thread signal call outside reap.rs"),
        (queued_signal_word(), "queued signal call outside reap.rs"),
        (
            descriptor_signal_word(),
            "descriptor signal call outside reap.rs",
        ),
    ]
}

/// Every forbidden shape found in one line of a file that is NOT one of the two sanctioned
/// lifecycle helpers - the gate's ban on every shape but the reaper-only ones, which
/// [`scan_tree`] checks in every file but [`REAPER`].
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

/// Scan every `.rs` file under `root/src`, `root/crates` and `root/tests`, deterministically ordered by
/// (file, line), applying [`general_hits`] outside [`SANCTIONED_FILES`] and
/// [`sanctioned_hits`] inside them, and flagging every [`reaper_only_shapes`] entry in every
/// file but [`REAPER`] - the whole-tree twin of the diff-scoped `no-os-kill` gate (spec 78, THE
/// AUDIT TEST).
fn scan_tree(root: &Path) -> Vec<Finding> {
    let mut files = Vec::new();
    for top in ["src", "crates", "tests"] {
        collect_rs_files(&root.join(top), &mut files);
    }
    let reaper_only = reaper_only_shapes();
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
            let mut hits = if sanctioned {
                sanctioned_hits(line)
            } else {
                general_hits(line)
            };
            if rel != REAPER {
                hits.extend(
                    reaper_only
                        .iter()
                        .filter(|(needle, _)| line.contains(needle.as_str()))
                        .map(|(_, label)| *label),
                );
            }
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

    /// A line opening a call of the Windows process-termination function.
    fn windows_termination_call() -> String {
        format!("unsafe {{ {}handle, 1) }};\n", windows_termination_open())
    }

    /// A line opening a call of the Windows job-termination function.
    fn job_termination_call() -> String {
        format!("unsafe {{ {}job, 1) }};\n", job_termination_open())
    }

    /// The four confined signal calls, each opened.
    fn confined_signal_calls() -> [String; 4] {
        [
            group_signal_open(),
            join(&thread_signal_word(), "("),
            join(&queued_signal_word(), "("),
            join(&descriptor_signal_word(), "("),
        ]
    }

    /// A line making the opened signal `call`.
    fn signal_call_line(call: &str) -> String {
        format!("let _ = rustix::process::{call}target, sig);\n")
    }

    /// Every confined signal call, one line each.
    fn every_confined_signal_call() -> String {
        confined_signal_calls()
            .iter()
            .map(|call| signal_call_line(call))
            .collect()
    }

    /// A fixture holding every confined signal call at `rel` yields one finding per line, each
    /// under its own shape.
    fn each_confined_signal_call_is_caught_in(rel: &str) {
        let findings = scan_fixture(&[(rel, &every_confined_signal_call())]);
        let shapes: Vec<(&str, usize, &str)> = findings
            .iter()
            .map(|finding| (finding.file.as_str(), finding.line_no, finding.shape))
            .collect();
        assert_eq!(
            shapes,
            vec![
                (rel, 1, "rustix process-group call outside reap.rs"),
                (rel, 2, "thread signal call outside reap.rs"),
                (rel, 3, "queued signal call outside reap.rs"),
                (rel, 4, "descriptor signal call outside reap.rs"),
            ]
        );
    }

    /// A line shelling out to `program`.
    fn shell_out_to(program: &str) -> String {
        format!("let _ = std::process::Command::new(\"{program}\");\n")
    }

    /// The Windows process-termination utility's name (task-kill).
    fn task_kill() -> String {
        prefixed_kill("task")
    }

    #[test]
    fn command_new_shell_out_is_caught_outside_the_sanctioned_files() {
        // `killall`, not the p-kill utility - it does not ALSO satisfy the standalone-token
        // shape, so this fixture isolates the Command::new shape alone.
        let finding = caught_in("src/somewhere.rs", &shell_out_to(&suffixed_kill("all")));
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
        windows_termination_call_is_caught_outside_reap_rs: caught_as(
            "crates/somewhere/src/lib.rs",
            &windows_termination_call(),
            "Windows process-termination call",
        );
        job_termination_call_is_caught_outside_reap_rs: caught_as(
            "crates/somewhere/src/lib.rs",
            &job_termination_call(),
            "Windows job-termination call",
        );
        taskkill_shell_out_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere.rs",
            &shell_out_to(&task_kill()),
            "Command::new",
        );
        task_kill_shell_out_in_another_case_is_caught_outside_the_sanctioned_files: caught_as(
            "src/somewhere.rs",
            &shell_out_to(&join("Task", "Kill")),
            "Command::new",
        );
        unix_utility_shell_out_in_another_case_is_caught_outside_the_sanctioned_files: caught_as(
            "tests/somewhere_test.rs",
            &shell_out_to(&join("KI", "LLALL")),
            "Command::new",
        );
        the_confined_signal_calls_are_caught_in_a_crate:
            each_confined_signal_call_is_caught_in("crates/somewhere/src/lib.rs");
        the_confined_signal_calls_are_caught_in_the_bin:
            each_confined_signal_call_is_caught_in("src/somewhere.rs");
        the_confined_signal_calls_are_caught_in_a_test_suite:
            each_confined_signal_call_is_caught_in("tests/somewhere_test.rs");
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
                    "crates/rigger-process/src/reap.rs",
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
            "crates/rigger-process/src/reap.rs",
            &shell_out_to(&suffixed_kill("all")),
        );
        /// So does a shell-out naming its utility in another case.
        a_shell_out_in_another_case_inside_a_sanctioned_file_is_still_caught: caught_as(
            "crates/rigger-process/src/reap.rs",
            &shell_out_to(&join("TASK", "KILL.EXE")),
            "Command::new",
        );
        /// The -- separator remains banned even inside a sanctioned file.
        a_dashdash_separator_inside_a_sanctioned_file_is_still_caught: caught_in(
            "tests/common/mod.rs",
            &format!("cmd.arg(\"{}\");\n", join("-", "-")),
        );
        /// A negative-pid format! remains banned even inside a sanctioned file.
        a_negative_pid_format_inside_a_sanctioned_file_is_still_caught: caught_in(
            "crates/rigger-process/src/reap.rs",
            &format!("let arg = format!(\"{}\", pgid);\n", join("-", "{}")),
        );
        /// The reaper's send_signal is the one Windows process-termination site.
        a_windows_termination_call_in_reap_rs_is_never_flagged: never_flagged(
            &[(
                "crates/rigger-process/src/reap.rs",
                &format!("            {}handle, 1);\n", windows_termination_open()),
            )],
            "reap.rs's own Windows process-termination call must never be flagged",
        );
        /// The test helper is sanctioned for the unix signal call only, never the Windows call.
        a_windows_termination_call_in_the_test_helper_is_caught: caught_as(
            "tests/common/mod.rs",
            &windows_termination_call(),
            "Windows process-termination call",
        );
        /// The reaper's end_job is the one Windows job-termination site.
        a_job_termination_call_in_reap_rs_is_never_flagged: never_flagged(
            &[("crates/rigger-process/src/reap.rs", &job_termination_call())],
            "reap.rs's own Windows job-termination call must never be flagged",
        );
        /// The test helper may not end a job either.
        a_job_termination_call_in_the_test_helper_is_caught: caught_as(
            "tests/common/mod.rs",
            &job_termination_call(),
            "Windows job-termination call",
        );
        /// The reaper is the one file that may make a confined signal call.
        the_confined_signal_calls_in_reap_rs_are_never_flagged: never_flagged(
            &[(REAPER, &every_confined_signal_call())],
            "reap.rs's own confined signal calls must never be flagged",
        );
        /// The test helper is sanctioned for the unix signal call only, never a confined one.
        the_confined_signal_calls_in_the_test_helper_are_caught:
            each_confined_signal_call_is_caught_in("tests/common/mod.rs");
        /// Naming the Windows function without calling it (an import, a doc) is never a call.
        a_windows_termination_name_without_a_call_is_never_flagged: never_flagged(
            &[(
                "src/somewhere.rs",
                &format!(
                    "use windows_sys::Win32::System::Threading::{{{}}};\n",
                    join("Terminate", "Process")
                ),
            )],
            "only an opened call of the Windows process-termination function is the shape",
        );
        /// A shell-out to the Windows termination utility is banned even inside reap.rs.
        a_taskkill_shell_out_inside_a_sanctioned_file_is_still_caught: caught_as(
            "crates/rigger-process/src/reap.rs",
            &shell_out_to(&task_kill()),
            "Command::new",
        );
        /// The same violation, rooted outside src/, crates/ and tests/ entirely, must be invisible.
        a_shape_outside_src_and_tests_is_never_scanned: never_flagged(
            &[("scripts/somewhere.rs", &shell_out_to(&prefixed_kill("p")))],
            "a .rs file outside src/, crates/ and tests/ must never be scanned",
        );
    }

    /// The shipped gate's whole output when its main check fires: a forbidden shape added outside
    /// the two sanctioned lifecycle files.
    const OUTSIDE_THE_SANCTIONED_FILES: &str = "no-os-kill gate FAILED: an OS-level kill, a \
        process-group target, or a direct signal call was introduced outside the sanctioned \
        lifecycle helpers (crates/rigger-process/src/reap.rs, tests/common/mod.rs). Lifecycle is \
        handle-bound: Child::kill()/wait(), or the shared helper. See \
        specs/78-no-os-level-kills.md\n";

    /// The shipped gate's whole output when its sanctioned-file check fires: a shell-out added
    /// inside one of the two sanctioned lifecycle files.
    const SHELL_OUT_IN_A_SANCTIONED_FILE: &str = "no-os-kill gate FAILED: a sanctioned lifecycle \
        file shelled out to kill or formatted a process-group target; use the internal guarded \
        signal call only\n";

    /// The shipped gate's whole output when its reaper-only check fires: a call only the reaper
    /// may make, either Windows termination call or a confined signal call, added anywhere else.
    fn a_call_only_the_reaper_makes() -> String {
        format!(
            "no-os-kill gate FAILED: a call only the reaper may make ({}, {}, {}, {}, {} or {}) was \
             introduced outside crates/rigger-process/src/reap.rs, the one file that may make \
             them. See specs/78-no-os-level-kills.md\n",
            join("Terminate", "Process"),
            join("TerminateJob", "Object"),
            join(&kill_word(), "_process_group"),
            thread_signal_word(),
            queued_signal_word(),
            descriptor_signal_word(),
        )
    }

    /// The shipped `.rigger/gates/content.sh no-os-kill`, run by path from a fixture repository as
    /// a unit's gate runs it, judges a unit branch whose one commit adds `line` at `rel` on a
    /// `rigger-run` base, and answers exactly `(passed, output)`.
    fn gate_judges(rel: &str, line: &str, (passed, output): (bool, &str)) {
        let (dir, _) =
            common::git::unit_branch_repo("pub fn f() {}\n", &[(rel, line, "adds the line")]);
        assert_eq!(
            common::repo::run_gate_script(dir.path(), "content.sh", &["no-os-kill"], &[]),
            (passed, output.to_string()),
            "the no-os-kill gate's verdict on {rel} adding {line:?}"
        );
    }

    // GIVEN a unit branch whose one commit adds a line, WHEN the shipped no-os-kill gate judges
    // its diff from the run branch, THEN an opened call of the Windows process-termination
    // function fails it in every code root but the reaper, the test helper included, while the
    // function's name imported without a call, or a call named in prose outside the code roots,
    // passes; and a shell-out to the Windows termination utility fails it in every code root, the
    // reaper and the test helper included, as does a shell-out naming any termination utility in
    // another case, since Windows and macOS's default filesystem resolve a program name in any
    // case. Each confined signal call fails it in every code root but the reaper, the test helper
    // included. Each failure prints the message of the check that caught it.
    rigger::test_cases! {
        the_gate_fails_a_windows_termination_call_added_to_a_crate: gate_judges(
            "crates/somewhere/src/lib.rs",
            &windows_termination_call(),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_windows_termination_call_added_to_the_bin: gate_judges(
            "src/somewhere.rs",
            &windows_termination_call(),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_windows_termination_call_added_to_a_test_suite: gate_judges(
            "tests/somewhere.rs",
            &windows_termination_call(),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_windows_termination_call_added_to_the_test_helper: gate_judges(
            "tests/common/mod.rs",
            &windows_termination_call(),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_passes_a_windows_termination_call_added_to_the_reaper: gate_judges(
            REAPER,
            &windows_termination_call(),
            (true, ""),
        );
        the_gate_fails_a_job_termination_call_added_to_a_crate: gate_judges(
            "crates/somewhere/src/lib.rs",
            &job_termination_call(),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_job_termination_call_added_to_the_test_helper: gate_judges(
            "tests/common/mod.rs",
            &job_termination_call(),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_passes_a_job_termination_call_added_to_the_reaper: gate_judges(
            REAPER,
            &job_termination_call(),
            (true, ""),
        );
        the_gate_fails_a_group_signal_call_added_to_the_test_helper: gate_judges(
            "tests/common/mod.rs",
            &signal_call_line(&group_signal_open()),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_thread_signal_call_added_to_a_crate: gate_judges(
            "crates/somewhere/src/lib.rs",
            &signal_call_line(&join(&thread_signal_word(), "(")),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_queued_signal_call_added_to_the_bin: gate_judges(
            "src/somewhere.rs",
            &signal_call_line(&join(&queued_signal_word(), "(")),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_descriptor_signal_call_added_to_a_test_suite: gate_judges(
            "tests/somewhere.rs",
            &signal_call_line(&join(&descriptor_signal_word(), "(")),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_fails_a_descriptor_signal_call_added_to_the_test_helper: gate_judges(
            "tests/common/mod.rs",
            &signal_call_line(&join(&descriptor_signal_word(), "(")),
            (false, &a_call_only_the_reaper_makes()),
        );
        the_gate_passes_the_confined_signal_calls_added_to_the_reaper: gate_judges(
            REAPER,
            &every_confined_signal_call(),
            (true, ""),
        );
        the_gate_passes_the_windows_termination_name_imported_without_a_call: gate_judges(
            "src/somewhere.rs",
            &format!(
                "use windows_sys::Win32::System::Threading::{{{}}};\n",
                join("Terminate", "Process")
            ),
            (true, ""),
        );
        the_gate_passes_a_windows_termination_call_named_in_prose: gate_judges(
            "docs/somewhere.md",
            &windows_termination_call(),
            (true, ""),
        );
        the_gate_fails_a_task_kill_shell_out_added_to_a_crate: gate_judges(
            "crates/somewhere/src/lib.rs",
            &shell_out_to(&task_kill()),
            (false, OUTSIDE_THE_SANCTIONED_FILES),
        );
        the_gate_fails_a_task_kill_shell_out_added_to_the_bin: gate_judges(
            "src/somewhere.rs",
            &shell_out_to(&task_kill()),
            (false, OUTSIDE_THE_SANCTIONED_FILES),
        );
        the_gate_fails_a_task_kill_shell_out_added_to_a_test_suite: gate_judges(
            "tests/somewhere.rs",
            &shell_out_to(&task_kill()),
            (false, OUTSIDE_THE_SANCTIONED_FILES),
        );
        the_gate_fails_a_task_kill_shell_out_added_to_the_reaper: gate_judges(
            REAPER,
            &shell_out_to(&task_kill()),
            (false, SHELL_OUT_IN_A_SANCTIONED_FILE),
        );
        the_gate_fails_a_task_kill_shell_out_added_to_the_test_helper: gate_judges(
            "tests/common/mod.rs",
            &shell_out_to(&task_kill()),
            (false, SHELL_OUT_IN_A_SANCTIONED_FILE),
        );
        the_gate_fails_a_task_kill_shell_out_in_another_case_added_to_a_crate: gate_judges(
            "crates/somewhere/src/lib.rs",
            &shell_out_to(&join("Task", "Kill")),
            (false, OUTSIDE_THE_SANCTIONED_FILES),
        );
        the_gate_fails_a_unix_utility_shell_out_in_another_case_added_to_the_bin: gate_judges(
            "src/somewhere.rs",
            &shell_out_to(&join("KI", "LLALL")),
            (false, OUTSIDE_THE_SANCTIONED_FILES),
        );
        the_gate_fails_a_shell_out_in_another_case_added_to_the_reaper: gate_judges(
            REAPER,
            &shell_out_to(&join("TASK", "KILL.EXE")),
            (false, SHELL_OUT_IN_A_SANCTIONED_FILE),
        );
    }

    rigger::test_cases! {
        /// The Done-when-c3 acceptance test itself: `tests/no_os_kill_audit.rs` scans the REAL,
        /// currently checked-out `src/`, `crates/` and `tests/` trees (resolved from `CARGO_MANIFEST_DIR`,
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
