//! Host fixtures: tools on PATH, files, the current directory, processes, and source text.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

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
