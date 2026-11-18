//! The hooks `rigger setup` writes into a project's `.claude/settings.json` name `rigger` alone,
//! in exec form ([`rigger::hooks::PRIME_COMMAND`], [`rigger::hooks::GREP_GUARD_COMMAND`]), which
//! the host spawns by that name on `PATH` with no shell. Its own executable, never a module of
//! `all`, so a run on any OS builds and runs this suite alone.

use std::path::Path;
use std::process::{Command, Stdio};

use rigger::hooks;

#[path = "common/product_binary.rs"]
mod product_binary;

/// The `command` and `args` of the one inner hook `settings` carries under `event`.
fn hook_command(settings: &[u8], event: &str) -> (String, Vec<String>) {
    let settings: serde_json::Value = serde_json::from_slice(settings).unwrap();
    let hook = &settings["hooks"][event][0]["hooks"][0];
    let command = hook["command"].as_str().unwrap().to_string();
    let args = hook["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| arg.as_str().unwrap().to_string())
        .collect();
    (command, args)
}

/// `command` with `args` spawned by name as the host spawns an exec-form hook, from `cwd`, with
/// `PATH` naming `path` alone, no input: whether it started, and then its success and stdout.
fn spawn_on_path(
    command: &str,
    args: &[String],
    cwd: &Path,
    path: &Path,
) -> Option<(bool, String)> {
    let out = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .env("PATH", path)
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(out) => Some((
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("spawning {command}: {e}"),
    }
}

/// A BARE `rigger` IN AN EXEC-FORM HOOK RUNS THE BINARY `PATH` HOLDS, ON EVERY OS: each hook
/// `rigger setup` installs naming `rigger` alone, spawned by that name with its `args` and no
/// shell, from a directory holding no project, with `PATH` naming only a directory that holds the
/// built binary as `rigger` plus the OS's executable suffix (`rigger.exe` on Windows), runs that
/// binary: the session-start hook primes with the instructions in force, and the grep guard,
/// handed no call, answers its empty verdict. The resolver here is the standard library's, which
/// on Windows appends `.exe` and searches the child's `PATH`; the host's own resolver is not
/// exercised, and its hooks documentation is what names a bare program resolving to a real `.exe`
/// as exec form that runs on every OS. On Unix the same spawn with `PATH` naming an empty
/// directory finds no program, so a pass comes through `PATH` alone, never a `rigger` installed
/// elsewhere.
#[test]
fn a_bare_rigger_exec_form_hook_runs_the_binary_path_holds() {
    let bin = tempfile::tempdir().unwrap();
    let named = bin
        .path()
        .join(format!("rigger{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(product_binary::rigger_bin(), &named).unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let rows = [
        (
            "the session-start hook",
            hooks::install_session_start(b"", hooks::PRIME_COMMAND).unwrap(),
            "SessionStart",
            "# Rigger",
        ),
        (
            "the grep guard",
            hooks::install_grep_guard(b"").unwrap(),
            "PreToolUse",
            "{}",
        ),
    ];
    for (row, settings, event, says) in rows {
        let (command, args) = hook_command(&settings, event);
        assert_eq!(command, "rigger", "{row}: the hook names `rigger` alone");
        let ran = spawn_on_path(&command, &args, cwd.path(), bin.path());
        assert!(
            ran.as_ref()
                .is_some_and(|(ok, out)| *ok && out.starts_with(says)),
            "{row}: `{command}` with {args:?} runs {} and prints {says:?} first: {ran:?}",
            named.display()
        );
        #[cfg(unix)]
        {
            let empty = tempfile::tempdir().unwrap();
            assert_eq!(
                spawn_on_path(&command, &args, cwd.path(), empty.path()),
                None,
                "{row}: with no `rigger` on `PATH` the spawn finds no program"
            );
        }
    }
}
