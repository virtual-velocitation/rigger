//! A line said on a stream other processes share, kept whole: a run's stderr, which the run's
//! dash child inherits and writes its own lines to. `writeln!` and `eprintln!` write each
//! formatted piece separately, so the other process writing between two pieces lands inside the
//! line. Formatting the line first and handing it to the stream in one write keeps it whole,
//! since a pipe never interleaves a single write of at most `PIPE_BUF` bytes with another.

use std::fmt;
use std::io::{self, Write};

/// Write `line` and its newline to `out` in one write.
pub fn write_line(out: &mut dyn Write, line: fmt::Arguments<'_>) -> io::Result<()> {
    out.write_all(format!("{line}\n").as_bytes())
}

/// Have `say` write its line to this process's stderr. A failure is dropped: a stderr that cannot
/// take the line has nowhere else to say so.
pub fn on_stderr(say: impl FnOnce(&mut dyn Write) -> io::Result<()>) {
    let _ = say(&mut io::stderr().lock());
}
