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

/// The text of the file at `path`, a panic naming it when it cannot be read.
pub fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Write `bytes` to `path`, creating its parent directories first.
pub fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// Write each `(root-relative path, text)` of `files` under `root` through [`write_file`].
pub fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        write_file(&root.join(path), text.as_bytes());
    }
}

/// Write `content` at `rel` under `root`, creating parent directories.
pub fn write_text(root: &Path, rel: &str, content: &str) {
    write_file(&root.join(rel), content.as_bytes());
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

/// The teardown reap's proof (spec 23): [`assert_teardown_reaps_only_what_is_rooted_inside`]
/// with at most one process rooted `outside`.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn assert_teardown_reaps_what_is_rooted_inside(
    inside: &Path,
    outside: Option<&Path>,
    teardown: impl FnOnce(),
    what: &str,
) {
    assert_teardown_reaps_only_what_is_rooted_inside(inside, outside.as_slice(), teardown, what);
}

/// A reap's proof: a SIGTERM-ignoring child rooted at `inside` and a plain sleeper rooted at each
/// of `outside`; once the inside child is seen rooted under `inside`, `teardown` runs, and the
/// inside child must have been reaped (only the SIGKILL escalation can end it) while every
/// outside one survives - the safety boundary. Every child is cleaned up before the assertions,
/// so a failure never leaks a process. `what` names the teardown in the messages.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn assert_teardown_reaps_only_what_is_rooted_inside(
    inside: &Path,
    outside: &[&Path],
    teardown: impl FnOnce(),
    what: &str,
) {
    let mut inside_child = sigterm_ignorer_in(inside);
    let mut outside_children: Vec<Child> = outside.iter().map(|dir| sleeper_in(dir)).collect();
    assert!(
        wait_until(|| rigger::reap::processes_rooted_under(inside)
            .iter()
            .any(|(pid, _)| *pid == inside_child.id())),
        "precondition: the inside child is rooted under {} before {what} runs",
        inside.display()
    );

    teardown();

    let inside_died = wait_until(|| matches!(inside_child.try_wait(), Ok(Some(_))));
    let outside_alive: Vec<bool> = outside_children
        .iter_mut()
        .map(|c| matches!(c.try_wait(), Ok(None)))
        .collect();
    outside_children.iter_mut().for_each(cleanup);
    if !inside_died {
        cleanup(&mut inside_child);
    }
    assert!(
        inside_died,
        "a process rooted inside {} must be reaped by {what} (SIGTERM then SIGKILL)",
        inside.display()
    );
    assert_eq!(
        outside_alive,
        vec![true; outside.len()],
        "a process rooted OUTSIDE, at each of {outside:?}, must survive {what} - the safety \
         boundary"
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

/// The ids of every thread this process holds right now, as `/proc/self/task` lists them.
pub fn thread_ids() -> std::collections::BTreeSet<u32> {
    std::fs::read_dir("/proc/self/task")
        .expect("this process's thread list")
        .map(|task| {
            task.expect("a thread entry")
                .file_name()
                .to_str()
                .and_then(|id| id.parse().ok())
                .expect("a numeric thread id")
        })
        .collect()
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

/// The brace-balanced declaration in `src` that starts at `start_marker`, from the marker
/// through its closing brace, cut by counting braces alone.
pub fn braced_declaration<'a>(src: &'a str, start_marker: &str) -> &'a str {
    let start = src
        .find(start_marker)
        .unwrap_or_else(|| panic!("the source must contain `{start_marker}`"));
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
