//! Whether any process still holds a directory - its working directory, or one of its open
//! files, lies inside it. The check a reclaim runs before it deletes a directory it counts as
//! dead, so a directory a live process is still using stays where it is. Read-only: it reads
//! processes through the process interface ([`crate::os_process`]) and never signals anything.

use crate::os_process;
use std::path::{Path, PathBuf};

pub use crate::os_process::open_files;

/// Every process other than this one, and other than a fork of it that has not exec'd, whose
/// working directory or any open file lies in `dir` or inside it, as pids
/// ([`os_process::holders`] over `dir` resolved); on Windows, which reports neither, every other
/// process the Restart Manager reports holding a file inside it. Empty when `dir` cannot be
/// resolved - best-effort, like the reaper's own scan ([`crate::reap::processes_rooted_under`]),
/// whose cwd rule this extends with open files.
pub fn processes_holding(dir: &Path) -> Vec<u32> {
    let Ok(base) = dir.canonicalize() else {
        return Vec::new();
    };
    os_process::holders(&base)
}

/// The files process `pid` itself opened and holds ([`open_files`]): none while it is a fork of
/// this process that has not exec'd ([`os_process::is_unexeced_fork_of`]), since every
/// descriptor it holds then is this process's own, copied by the fork. The reader for a caller
/// that has just spawned `pid` and asks what the child opened, which the host test fixtures
/// consume.
pub fn own_open_files(pid: u32) -> Vec<PathBuf> {
    if os_process::is_unexeced_fork_of(pid, std::process::id()) {
        Vec::new()
    } else {
        open_files(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{release, waiting_shell};

    #[test]
    fn a_process_whose_cwd_is_inside_the_dir_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let child = waiting_shell("true", &inner);
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
        let child = waiting_shell(&format!("exec 3< '{}'", file.display()), elsewhere.path());
        let pid = child.id();
        assert!(processes_holding(dir.path()).contains(&pid));
        assert!(
            !processes_holding(elsewhere.path()).is_empty(),
            "its cwd holds the other dir"
        );
        release(child);
    }

    #[test]
    fn a_processes_open_files_name_each_file_it_holds_and_a_gone_process_holds_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().canonicalize().unwrap().join("f");
        std::fs::write(&file, b"x").unwrap();
        let child = waiting_shell(
            &format!("exec 3< '{}' 4< '{}'", file.display(), file.display()),
            dir.path(),
        );
        let pid = child.id();
        let held = open_files(pid)
            .into_iter()
            .filter(|named| *named == file)
            .count();
        let counted = crate::test_support::files_open_by(pid, &file);
        release(child);
        assert_eq!(
            (held, counted, open_files(pid).len()),
            (2, 2, 0),
            "each descriptor on the file names it, the host fixture counts each, and a process \
             that is gone holds nothing"
        );
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
