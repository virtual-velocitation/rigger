//! Periphery for spec 104 criterion 4 (THE WRITE GUARD): the ONE property none of
//! `src/driver/claude_code.rs`'s own `mod tests` for `install_write_guard_hook` /
//! `write_guard_command` can reach - that the shell-quoted command string those functions
//! build is not merely SHAPED as expected (the implementer's own tests assert the exact
//! `'rigger' guard-write --root '...'` text) but actually BEHAVES as claimed once a real
//! POSIX shell parses it, which is exactly what Claude Code itself does with the installed
//! `PreToolUse` hook's `command` field. No implementer test ever spawns a shell - every one
//! of them inspects the returned bytes as a string - so none of them can distinguish a
//! quoting bug that only a real shell's word-splitting would expose from one that only a
//! hand-rolled parser would catch.
//!
//! Three properties follow that only a real `sh -c` process, fed a real `PreToolUse`
//! payload on stdin and driving the real compiled `rigger guard-write` binary, can prove:
//!
//! 1. A root containing a space and an embedded single quote survives the shell's own
//!    word-splitting intact (`write_guard_command`'s own doc comment's claim: "a root
//!    containing a space or a shell metacharacter can never split an argument").
//! 2. Two roots, each independently quoted, arrive at `guard-write` as two SEPARATE
//!    `--root` values in the given order - not merged, dropped, or reordered by the shell.
//! 3. A root string shaped like a shell-injection payload (an embedded `'` immediately
//!    followed by a shell command) never actually RUNS that command - the quoting
//!    neutralizes it rather than merely relying on `guard-write` happening to ignore
//!    malformed argv, the strong form of "never be reinterpreted", not the weak one.
//!
//! NOT OWNED HERE: `cmd_guard_write`'s own allow/deny decision surface for absolute,
//! relative, `..` and symlink-escaping targets, multiple plainly-spelled roots,
//! `NotebookEdit`, and the missing-`--root` argument error - `tests/cli.rs`'s own
//! `run_guard_write`-based suite already drives the compiled binary directly for all of
//! that (`guard_write_allows_under_the_root_and_denies_outside_naming_the_first` and its
//! neighbors); this file adds only the shell-quoting layer underneath it. Also not owned
//! here: `install_write_guard_hook`'s JSON-merge shape (empty settings, an existing
//! `StopFailure` family, idempotence) - `src/driver/claude_code.rs`'s own `mod tests`
//! already prove those against the real `hooks::install_pretooluse_hook` function, in
//! process; this file only adds the shell boundary those tests cannot cross.

mod common;

use rigger::driver::claude_code::install_write_guard_hook;
use std::io::Write;
use std::process::Stdio;

/// Build the exact hook command `install_write_guard_hook` would inject for `roots`,
/// against the REAL compiled binary this suite drives (never the bare `"rigger"` the
/// implementer's own tests use as a placeholder - a name that may not resolve on `PATH` in
/// a sandboxed test run).
fn write_guard_hook_command(roots: &[String]) -> String {
    let bin = common::rigger_bin();
    let out = install_write_guard_hook(b"", roots, bin.to_str().expect("utf-8 test path"))
        .expect("install_write_guard_hook must succeed against empty settings");
    let settings: serde_json::Value = serde_json::from_slice(&out).unwrap();
    settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("the merged settings JSON must carry the hook command")
        .to_string()
}

/// Run `command` (the exact string a `PreToolUse` hook entry carries) through a REAL POSIX
/// shell - `sh -c <command>` - exactly as Claude Code itself does, piping `payload` on
/// stdin and parsing stdout as the one JSON object `guard-write` always prints. Panics with
/// the raw, unparsed stdout/stderr on any shell or parse failure, so a quoting bug that
/// leaks injected shell output onto stdout - or breaks the command line's own syntax - is
/// visible in the failure message rather than swallowed by a generic `serde_json` error.
fn run_through_a_real_shell(command: &str, payload: &str) -> serde_json::Value {
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sh -c <installed hook command>");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let out = child
        .wait_with_output()
        .expect("sh -c <installed hook command> must exit");
    assert!(
        out.status.success(),
        "the installed hook command must exit 0 under a real shell; stderr:\n{}\nstdout:\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "the shell-run hook command must print exactly one JSON object: {e}\nstdout:\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// A PreToolUse `Write` payload naming `file_path` under `cwd`.
fn write_payload(cwd: &str, file_path: &str) -> String {
    serde_json::json!({
        "tool_name": "Write",
        "cwd": cwd,
        "tool_input": {"file_path": file_path},
    })
    .to_string()
}

#[test]
fn installed_hook_command_allows_and_denies_correctly_through_a_real_shell() {
    let root = tempfile::tempdir().unwrap();
    let root_str = std::fs::canonicalize(root.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let outside = tempfile::tempdir().unwrap();

    let command = write_guard_hook_command(std::slice::from_ref(&root_str));

    let allowed = run_through_a_real_shell(&command, &write_payload(&root_str, "notes.txt"));
    assert_eq!(
        allowed,
        serde_json::json!({}),
        "a target under the installed root must be allowed once a real shell has parsed \
         the installed command; got:\n{allowed}"
    );

    let outside_target = outside.path().join("notes.txt");
    let denied = run_through_a_real_shell(
        &command,
        &write_payload(&root_str, outside_target.to_str().unwrap()),
    );
    assert_eq!(
        denied["hookSpecificOutput"]["permissionDecisionReason"],
        serde_json::json!(format!(
            "write target is outside the allowed root: {root_str}"
        )),
        "a target outside the installed root must be denied naming it, through a real \
         shell; got:\n{denied}"
    );
}

#[test]
fn installed_hook_command_survives_a_root_with_a_space_and_an_embedded_quote_through_a_real_shell()
{
    let base = tempfile::tempdir().unwrap();
    let tricky = base.path().join("it's a root with space");
    std::fs::create_dir_all(&tricky).unwrap();
    let tricky_str = std::fs::canonicalize(&tricky)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let command = write_guard_hook_command(std::slice::from_ref(&tricky_str));

    let allowed = run_through_a_real_shell(&command, &write_payload(&tricky_str, "notes.txt"));
    assert_eq!(
        allowed,
        serde_json::json!({}),
        "a root with a space and an embedded single quote must reach guard-write as ONE \
         intact argument once a real shell has word-split the installed command; got:\n\
         {allowed}"
    );
}

#[test]
fn installed_hook_command_preserves_two_roots_in_order_through_a_real_shell() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_str = std::fs::canonicalize(first.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let second_str = std::fs::canonicalize(second.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let command = write_guard_hook_command(&[first_str.clone(), second_str.clone()]);

    for (label, root_str) in [("first", &first_str), ("second", &second_str)] {
        let allowed = run_through_a_real_shell(&command, &write_payload(root_str, "notes.txt"));
        assert_eq!(
            allowed,
            serde_json::json!({}),
            "the {label} root must reach guard-write as its own --root once a real shell \
             has parsed the installed two-root command; got:\n{allowed}"
        );
    }

    // A target under NEITHER root is denied naming the FIRST - proving both `--root`
    // flags actually arrived as two SEPARATE arguments. A shell bug that merged them into
    // one argument would still deny here, but could never name the first root correctly,
    // since guard-write would have seen only a single, wrong root string.
    let outside = tempfile::tempdir().unwrap();
    let denied = run_through_a_real_shell(
        &command,
        &write_payload(&first_str, outside.path().join("x.txt").to_str().unwrap()),
    );
    assert_eq!(
        denied["hookSpecificOutput"]["permissionDecisionReason"],
        serde_json::json!(format!(
            "write target is outside the allowed root: {first_str}"
        )),
        "denying a target outside both roots must name the FIRST exactly, through a real \
         shell; got:\n{denied}"
    );
}

#[test]
fn installed_hook_command_neutralizes_an_injection_shaped_root_through_a_real_shell() {
    let marker_dir = tempfile::tempdir().unwrap();
    let marker = marker_dir.path().join("injected-by-a-broken-quote");
    let root = tempfile::tempdir().unwrap();
    let root_str = std::fs::canonicalize(root.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    // An embedded `'` immediately followed by a real shell command: if
    // `shell_single_quote` ever closed the quote early instead of escaping it, a real
    // shell would run `touch <marker>` as a SEPARATE command rather than treat the whole
    // string as one `--root` argument value.
    let evil_root = format!("{root_str}'; touch {} ; echo '", marker.to_str().unwrap());

    let command = write_guard_hook_command(&[evil_root]);

    // Whatever guard-write decides for this synthetic root is not the point here -
    // `installed_hook_command_allows_and_denies_correctly_through_a_real_shell` already
    // proves the decision logic on a plain root; this call's job is only to prove the
    // shell ran ONE command (via `run_through_a_real_shell`'s own successful parse of
    // exactly one JSON object) and that the embedded command never executed.
    let _ = run_through_a_real_shell(&command, &write_payload(&root_str, "notes.txt"));
    assert!(
        !marker.exists(),
        "an embedded `'` in a root must never let a real shell run a command that \
         followed it - the marker file must not exist"
    );
}
