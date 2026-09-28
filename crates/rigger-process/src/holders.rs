//! Whether any process still holds a directory - its working directory, or one of its open
//! file descriptors, resolves inside it. The check a reclaim runs before it deletes a
//! directory it counts as dead, so a directory a live process is still using stays where it
//! is. Read-only: it reads `/proc` and never signals anything.

use std::path::Path;

/// Every process other than this one whose working directory or any open file descriptor
/// resolves to `dir` itself or to a path inside it, as pids. Empty when `dir` cannot be
/// resolved or `/proc` cannot be read (a platform without it) - best-effort, like the
/// reaper's own scan ([`crate::reap::processes_rooted_under`]), whose cwd rule this extends
/// with open descriptors.
pub fn processes_holding(dir: &Path) -> Vec<u32> {
    let Ok(base) = dir.canonicalize() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let self_pid = std::process::id();
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()))
        .filter(|&pid| pid != self_pid && holds(pid, &base))
        .collect()
}

/// Whether process `pid`'s cwd or one of its open file descriptors resolves inside `base`
/// (already canonical). A link that cannot be read - the process exited, or belongs to
/// another user - holds nothing.
fn holds(pid: u32, base: &Path) -> bool {
    let proc = Path::new("/proc").join(pid.to_string());
    let inside =
        |link: &Path| std::fs::read_link(link).is_ok_and(|target| target.starts_with(base));
    inside(&proc.join("cwd"))
        || std::fs::read_dir(proc.join("fd"))
            .is_ok_and(|fds| fds.flatten().any(|fd| inside(&fd.path())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};

    /// A shell that runs `setup`, reports readiness on stdout, then waits for its stdin to
    /// close - so dropping the handle's stdin ends it without any signal.
    fn holder(setup: &str, cwd: &Path) -> Child {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(format!("{setup}; echo ready; read _"))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn the holder");
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(child.stdout.as_mut().unwrap()),
            &mut line,
        )
        .unwrap();
        assert_eq!(line.trim(), "ready");
        child
    }

    /// Close `child`'s stdin so it exits on its own, then reap it.
    fn release(mut child: Child) {
        let mut stdin = child.stdin.take().unwrap();
        let _ = stdin.flush();
        drop(stdin);
        child.wait().unwrap();
    }

    #[test]
    fn a_process_whose_cwd_is_inside_the_dir_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let child = holder("true", &inner);
        let pid = child.id();
        assert!(processes_holding(dir.path()).contains(&pid));
        release(child);
    }

    #[test]
    fn a_process_with_an_open_descriptor_inside_the_dir_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let child = holder(&format!("exec 3< '{}'", file.display()), elsewhere.path());
        let pid = child.id();
        assert!(processes_holding(dir.path()).contains(&pid));
        assert!(
            !processes_holding(elsewhere.path()).is_empty(),
            "its cwd holds the other dir"
        );
        release(child);
    }

    #[test]
    fn nothing_holds_a_dir_no_process_uses_and_this_process_never_counts() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let _own = std::fs::File::open(&file).unwrap();
        assert!(processes_holding(dir.path()).is_empty());
        assert!(processes_holding(&dir.path().join("missing")).is_empty());
    }
}
