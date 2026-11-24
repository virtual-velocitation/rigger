//! The lines of a reader's worktree a doc span is read from ([`SourceLines`]): each file opened at
//! its path under the worktree, through std alone.

use rigger_domain::code_view::SourceLines;
use std::path::{Path, PathBuf};

/// The files of the worktree at its path, read whole when a span of one is asked for.
pub struct WorktreeLines {
    worktree: PathBuf,
}

impl WorktreeLines {
    pub fn new(worktree: &Path) -> Self {
        Self {
            worktree: worktree.to_path_buf(),
        }
    }
}

/// A file's bytes are read as UTF-8 with each invalid sequence read as U+FFFD, so only a file
/// that cannot be opened, or a span outside it, gives none.
impl SourceLines for WorktreeLines {
    fn lines(&self, path: &str, (first, last): (u32, u32)) -> Option<String> {
        let bytes = std::fs::read(self.worktree.join(path)).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let from = usize::try_from(first).ok()?.checked_sub(1)?;
        let span = lines.get(from..usize::try_from(last).ok()?)?;
        (!span.is_empty()).then(|| span.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A span's lines are read from the file at its path in the worktree: GIVEN a worktree holding
    /// a four-line file, a file of CRLF lines and a file whose bytes are not UTF-8, WHEN each span
    /// is read, THEN the lines from its first to its last are given as [`str::lines`] splits them
    /// (a trailing `\r` dropped), a byte that is not UTF-8 read as U+FFFD, and none for a span
    /// reaching past the file's last line, starting at line 0 or ending before it starts, or for
    /// a file the read cannot open.
    #[test]
    fn a_span_s_lines_are_read_from_the_worktree_file() {
        let worktree = tempfile::tempdir().unwrap();
        let files: [(&str, &[u8]); 3] = [
            ("src/a.rs", b"one\ntwo\nthree\nfour\n"),
            ("crlf.rs", b"one\r\ntwo\r\n"),
            ("latin.rs", b"caf\xe9\n"),
        ];
        for (path, bytes) in files {
            let path = worktree.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let lines = WorktreeLines::new(worktree.path());
        // (case, the path, the span, its lines)
        let cases = [
            ("the span's lines", "src/a.rs", (2, 3), Some("two\nthree")),
            ("the first line", "src/a.rs", (1, 1), Some("one")),
            ("the file's last line", "src/a.rs", (4, 4), Some("four")),
            ("one line past the end", "src/a.rs", (4, 5), None),
            ("from line 0", "src/a.rs", (0, 1), None),
            ("an end before its start", "src/a.rs", (3, 2), None),
            ("a file the read cannot open", "src/gone.rs", (1, 1), None),
            ("CRLF lines", "crlf.rs", (1, 2), Some("one\ntwo")),
            (
                "bytes that are not UTF-8",
                "latin.rs",
                (1, 1),
                Some("caf\u{FFFD}"),
            ),
        ];
        for (case, path, span, want) in cases {
            assert_eq!(lines.lines(path, span).as_deref(), want, "{case}");
        }
    }
}
