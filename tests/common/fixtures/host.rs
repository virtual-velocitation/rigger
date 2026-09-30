//! Host fixtures: tools on PATH, files, the current directory, processes, and source text.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Whether `program` is on PATH and answers `version_arg` successfully - the guard a test that
/// needs an optional external tool (`node`, `npm`, `go-gitsemver`) checks before skipping.
pub fn tool_available(program: &str, version_arg: &str) -> bool {
    Command::new(program)
        .arg(version_arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write `bytes` to `path`, creating its parent directories first.
pub fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// Every entry of `dir` whose name starts with `prefix` - every entry, for an empty one - with its
/// bytes (`None` for a directory), sorted by name: the snapshot a test compares to prove a step
/// left the directory exactly as it found it.
pub fn dir_snapshot(dir: &Path, prefix: &str) -> Vec<(String, Option<Vec<u8>>)> {
    let mut entries: Vec<(String, Option<Vec<u8>>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .map(|entry| {
            (
                entry.file_name().into_string().unwrap(),
                (!entry.file_type().unwrap().is_dir())
                    .then(|| std::fs::read(entry.path()).unwrap()),
            )
        })
        .collect();
    entries.sort();
    entries
}

/// Restores the process's current directory to the held path when dropped, so a test that
/// changes directory cannot leak that change past a failed assertion.
pub struct CwdGuard(pub PathBuf);

impl CwdGuard {
    /// Enter `dir`, returning the guard that restores the directory this was called from.
    pub fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().expect("read the current directory");
        std::env::set_current_dir(dir).expect("enter the throwaway directory");
        CwdGuard(original)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// The process-group id of live process `pid`, read from `/proc/<pid>/stat`.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn pgid_of(pid: u32) -> u32 {
    rigger::reap::stat_field_after_comm(pid, 2)
        .unwrap_or_else(|| panic!("/proc/{pid}/stat has a pgrp field after comm"))
        .parse()
        .expect("pgrp is a base-10 integer")
}

/// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only a SIGKILL escalation
/// can end it - exercising the full SIGTERM-then-SIGKILL reap, not just a plain `sleep` a bare
/// SIGTERM would already end.
pub fn sigterm_ignorer_in(dir: &Path) -> Child {
    Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while :; do sleep 1; done")
        .current_dir(dir)
        .spawn()
        .expect("spawn a SIGTERM-ignoring fixture process")
}

/// Spawn a plain long-lived `sleep` rooted at `dir`, so it appears in `/proc` rooted there and a
/// bare SIGTERM ends it.
pub fn sleeper_in(dir: &Path) -> Child {
    Command::new("sleep")
        .arg("300")
        .current_dir(dir)
        .spawn()
        .expect("spawn a sleeping fixture process")
}

/// The teardown reap's proof (spec 23): a SIGTERM-ignoring child rooted at `inside` - and, with
/// `outside`, a plain sleeper rooted there; once the inside child is seen rooted under `inside`,
/// `teardown` runs, and the inside child must have been reaped (only the SIGKILL escalation can
/// end it) while any outside one survives - the safety boundary. Every child is cleaned up before
/// the assertions, so a failure never leaks a process. `what` names the teardown in the messages.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn assert_teardown_reaps_what_is_rooted_inside(
    inside: &Path,
    outside: Option<&Path>,
    teardown: impl FnOnce(),
    what: &str,
) {
    let mut inside_child = sigterm_ignorer_in(inside);
    let mut outside_child = outside.map(sleeper_in);
    assert!(
        wait_until(|| rigger::reap::processes_rooted_under(inside)
            .iter()
            .any(|(pid, _)| *pid == inside_child.id())),
        "precondition: the inside child is rooted under {} before {what} runs",
        inside.display()
    );

    teardown();

    let inside_died = wait_until(|| matches!(inside_child.try_wait(), Ok(Some(_))));
    let outside_alive = outside_child
        .as_mut()
        .map(|c| matches!(c.try_wait(), Ok(None)));
    if let Some(c) = outside_child.as_mut() {
        cleanup(c);
    }
    if !inside_died {
        cleanup(&mut inside_child);
    }
    assert!(
        inside_died,
        "a process rooted inside {} must be reaped by {what} (SIGTERM then SIGKILL) before \
         its dir is removed",
        inside.display()
    );
    assert_ne!(
        outside_alive,
        Some(false),
        "a process rooted OUTSIDE must survive {what} - the safety boundary"
    );
}

/// Poll `pred` up to `tries` times, sleeping 25ms between checks, and return whether it held -
/// the latency tolerance a test needs to observe an asynchronous OS-level effect (a signal
/// delivered, a process reaped, a kernel lock taken) without a flaky zero-wait check or a fixed
/// sleep long enough to slow the suite.
pub fn wait_until_for(tries: u32, mut pred: impl FnMut() -> bool) -> bool {
    for _ in 0..tries {
        if pred() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

/// [`wait_until_for`] over the default 5 seconds.
pub fn wait_until(pred: impl FnMut() -> bool) -> bool {
    wait_until_for(200, pred)
}

/// How many of the open files of process `pid` ([`rigger::holders::open_files`]) name the file at
/// `path`: a connection that opened the file holds one. A process that is gone holds none.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn files_open_by(pid: u32, path: &Path) -> usize {
    rigger::holders::open_files(pid)
        .filter(|named| named == path)
        .count()
}

/// A shell in `cwd` that runs `setup` and, once it succeeded, says `ready` on its stdout and waits
/// on its stdin, returned once it said so - so whatever `setup` opened stays open until the shell
/// is ended: [`release`] closes its stdin so it exits on its own, and [`cleanup`] ends it through
/// its handle. A `setup` that fails reaps the shell and panics.
pub fn waiting_shell(setup: &str, cwd: &Path) -> Child {
    let mut shell = Command::new("sh")
        .arg("-c")
        .arg(format!("{setup} && echo ready && read _"))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the waiting shell");
    let mut said = String::new();
    std::io::BufReader::new(shell.stdout.as_mut().unwrap())
        .read_line(&mut said)
        .unwrap();
    if said != "ready\n" {
        cleanup(&mut shell);
        panic!("the waiting shell's setup `{setup}` failed");
    }
    shell
}

/// Close the stdin of a [`waiting_shell`] so it exits on its own, then reap it.
pub fn release(mut shell: Child) {
    drop(shell.stdin.take());
    shell.wait().unwrap();
}

/// A process holding the OS advisory lock on `file` - the lock `std::fs::File::try_lock` takes -
/// returned once it holds it: a [`waiting_shell`] keeps `file` open on one descriptor, which
/// `flock(1)` locks. The OS releases the lock when the process is gone ([`cleanup`]).
pub fn lock_holder(file: &Path) -> Child {
    waiting_shell(
        &format!("exec 9>>'{}' && flock -n 9", file.display()),
        Path::new("/"),
    )
}

/// End and reap a fixture child unconditionally, ignoring errors - through the `Child` handle it
/// was spawned with, never a computed pid. Ending it first means the reap returns promptly even
/// when the child is still alive.
pub fn cleanup(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The brace-balanced JavaScript declaration in `src` that starts at `start_marker`, from the
/// marker through its closing brace.
pub fn js_declaration<'a>(src: &'a str, start_marker: &str) -> &'a str {
    let start = src
        .find(start_marker)
        .unwrap_or_else(|| panic!("workflow must contain `{start_marker}`"));
    let open = start
        + src[start..]
            .find('{')
            .expect("declaration must open a brace");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("`{start_marker}` is not brace-balanced");
}

/// The workflow driver `code` guards a null step (spec 44, criterion 2): `agent()` can RESOLVE
/// to null - rather than reject - when the courier agent dies on a TERMINAL error (an expired
/// login, an exhausted API quota), so the driver must test `if (!step)` BEFORE its first
/// `deref` read of the step (the code token naming the dereference), stop there loudly through
/// `stop(...)`, and name both the likely cause and that the run is RESUMABLE. `what` names the
/// driver source in failure messages.
pub fn assert_driver_guards_a_null_step(code: &str, deref: &str, what: &str) {
    let guard = code
        .find("if (!step)")
        .unwrap_or_else(|| panic!("{what} must guard a null step with `if (!step)`"));
    let deref_at = code
        .find(deref)
        .unwrap_or_else(|| panic!("{what} must read `{deref}` after the guard"));
    assert!(
        guard < deref_at,
        "the `if (!step)` guard must precede the `{deref}` dereference in {what}, or a null \
         step (agent() resolved to null) would still crash before the guard runs"
    );
    assert!(
        code[guard..deref_at].contains("stop("),
        "the null-step guard in {what} must stop loudly via `stop(...)` before the \
         dereference, not fall through"
    );
    assert!(
        code.contains("resolved to null"),
        "the null-step diagnostic in {what} must name the cause: agent() RESOLVED TO NULL \
         rather than rejecting"
    );
    assert!(
        code.contains("expired login") && code.contains("quota"),
        "the null-step diagnostic in {what} must name the likely terminal cause (an expired \
         login or an exhausted API quota)"
    );
    assert!(
        code.contains("RESUMABLE"),
        "the null-step diagnostic in {what} must tell the operator the run is RESUMABLE"
    );
}
