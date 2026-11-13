//! A line said on a stream other processes share, kept whole: a run's stderr, which the run's
//! dash child inherits and writes its own lines to. `writeln!` and `eprintln!` write each
//! formatted piece separately, so the other process writing between two pieces lands inside the
//! line. Formatting the text first and handing it to the stream in one write keeps it whole,
//! since a pipe never interleaves a single write of at most `PIPE_BUF` bytes with another.

use std::fmt;
use std::io::{self, Write};

/// Write `text` to `out` in one write, formatted first.
pub fn write_whole(out: &mut dyn Write, text: fmt::Arguments<'_>) -> io::Result<()> {
    out.write_all(text.to_string().as_bytes())
}

/// Write `line` and its newline to `out` in one write.
pub fn write_line(out: &mut dyn Write, line: fmt::Arguments<'_>) -> io::Result<()> {
    write_whole(out, format_args!("{line}\n"))
}

/// Have `say` write its line to this process's stderr. A failure is dropped: a stderr that cannot
/// take the line has nowhere else to say so.
pub fn on_stderr(say: impl FnOnce(&mut dyn Write) -> io::Result<()>) {
    let _ = say(&mut io::stderr().lock());
}

/// Say `line` and its newline on this process's stderr in one write ([`write_line`]).
pub fn say(line: fmt::Arguments<'_>) {
    on_stderr(|err| write_line(err, line));
}

/// The writes a stream took, each as the bytes it carried: the double through which a table
/// proving a line is said in one write reads its stream ([`writes_of`]).
#[doc(hidden)]
#[derive(Default)]
pub struct Writes(pub Vec<Vec<u8>>);

impl Write for Writes {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.push(buf.to_vec());
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The writes `say` hands its stream, each as the bytes it carried.
#[doc(hidden)]
pub fn writes_of(say: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> Vec<Vec<u8>> {
    let mut out = Writes::default();
    say(&mut out).unwrap();
    out.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN text said on a stream another process shares, WHEN it is written, THEN it reaches the
    // stream in one write, which a pipe keeps whole.
    crate::test_cases! {
        a_line_and_its_newline_reach_the_stream_in_one_write: assert_eq!(
            writes_of(|out| write_line(out, format_args!("rigger: {} of {}", 2, "three"))),
            [b"rigger: 2 of three\n"]
        );
        a_block_of_lines_reaches_the_stream_in_one_write_as_written: assert_eq!(
            writes_of(|out| write_whole(out, format_args!("usage:\n  {}\n", "rigger run"))),
            [b"usage:\n  rigger run\n"]
        );
    }
}
