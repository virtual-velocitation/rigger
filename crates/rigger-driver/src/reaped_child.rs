//! The supervised child-process guard (workspace split: moved from the dash, whose auto-started
//! child it first wrapped, beside the agent host that reaps through it too), and the one timed
//! line reader over a child's stream, [`line_channel`]. The root `rigger` crate re-exports the
//! module as `rigger::reaped_child` and the guard under its historical `rigger::dash::ReapedChild`
//! path.

/// A supervised handle over a long-lived `rigger` child PROCESS - the auto-started
/// dashboard, and any future `rigger` child a run spawns. When this guard is dropped,
/// the child is KILLED and REAPED, so it can never outlive the run that started it.
///
/// This is the single reaping mechanism the dash and the other `rigger` children rely
/// on (spec 19b, unit 3: no orphaned `rigger` processes). `Drop` runs on BOTH a normal
/// scope exit AND an unwinding panic, so a normally-finishing OR a crashing driver
/// leaves no orphaned `rigger` process reparented to `init`. Reaping is `kill` followed
/// by `wait` (not `kill` alone): the `wait` collects the exited child, so a
/// finished-but-unwaited process leaves no defunct zombie either.
///
/// It is deliberately `std`-only (`std::process::Child`, not a `PR_SET_PDEATHSIG`
/// `prctl`): `libc` is an optional feature-gated dependency, but this guard must compile
/// on BOTH the default and the `--no-default-features` lane, and `std::process` is the
/// only child-lifecycle primitive available on both.
pub struct ReapedChild {
    child: std::process::Child,
}

impl ReapedChild {
    /// Take ownership of an already-spawned child so it is reaped when this guard drops.
    /// The caller owns spawning (dependency injection); this guard owns only its death.
    pub fn new(child: std::process::Child) -> Self {
        ReapedChild { child }
    }

    /// The supervised child's OS process id (e.g. to log the serving dash, or surface it
    /// in `rigger status`).
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Borrow the supervised child mutably - a caller that must still interact with the
    /// live process (take its stdio pipes, drop its stdin to signal EOF) while this guard
    /// keeps owning the eventual reap. `pub(crate)`: an in-crate escape hatch for a
    /// caller that needs more than `id()`, never a public API a caller outside this
    /// crate should reach for (spec 104 round-4 REQUIRED FIX 2 - `driver::claude_code`'s
    /// `read_stream` is the first such caller, taking ownership of `Launch::child` and
    /// wrapping it here instead of re-deriving this same Drop-based reap guard itself).
    pub(crate) fn child_mut(&mut self) -> &mut std::process::Child {
        &mut self.child
    }

    /// Hand each line of the child's stdout to `each` until the child ends the stream, `each`
    /// breaks, a line is no text or `deadline` passes, whichever comes first: a child that answers
    /// nothing holds this read no longer than `deadline`. A child with no stdout pipe hands over
    /// nothing. The child is left running; this guard's drop still reaps it.
    pub fn read_lines_until(
        &mut self,
        deadline: std::time::Instant,
        mut each: impl FnMut(&str) -> std::ops::ControlFlow<()>,
    ) {
        let Some(stdout) = self.child.stdout.take() else {
            return;
        };
        let (lines, _reader) = line_channel(stdout);
        while let Ok(Ok(line)) =
            lines.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            if each(&line).is_break() {
                return;
            }
        }
    }
}

/// A stream (a child's stdout or stderr) read line by line on its own thread and forwarded over a
/// channel, so its reader can wait on a line with a timeout where `BufRead::lines` alone would
/// block until the child writes. Each line arrives as `BufRead::lines` yields it, a read error
/// included, and the receiver disconnects at the stream's end. The thread ends there or once
/// the receiver is gone; its handle is the caller's to join or to leave.
pub fn line_channel(
    stream: impl std::io::Read + Send + 'static,
) -> (
    std::sync::mpsc::Receiver<std::io::Result<String>>,
    std::thread::JoinHandle<()>,
) {
    use std::io::BufRead;
    let (line_tx, line_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stream).lines() {
            if line_tx.send(line).is_err() {
                return; // the reader is gone; nothing left to feed.
            }
        }
    });
    (line_rx, reader)
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        // If the child already exited, `try_wait` has reaped the zombie and there is
        // nothing to kill. Otherwise kill it and `wait` to collect it. Every call is
        // best-effort - a reaper whose child is already gone must never panic in `drop`.
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
}

#[cfg(test)]
mod supervised_lifecycle {
    //! Spec 19b, unit 3: the reaper mechanism reaps every long-lived `rigger` child
    //! after its guard is dropped / the driver exits, so a normally-finishing OR
    //! crashing agent leaves no orphaned `rigger` process. The standalone-`rigger dash`
    //! proof the criterion names lives in `tests/cli.rs` (it needs the compiled binary);
    //! these hermetic tests prove the SAME [`ReapedChild`] discipline generically, on a
    //! stand-in child on the CRASH path.
    use super::ReapedChild;
    use std::time::Duration;

    /// A real long-lived child that would outlive the test unless it is reaped. Its
    /// stdout is piped and never written to, so a reader on it blocks until the child
    /// EXITS (the child's write end closes -> EOF). That is a std-only, race-free "is
    /// it still alive?" probe that needs no `libc` (unavailable in the light lane).
    fn spawn_blocking_child() -> (std::process::Child, std::process::ChildStdout) {
        use std::process::{Command, Stdio};
        let mut child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn a long-lived child");
        let out = child.stdout.take().expect("child stdout is piped");
        (child, out)
    }

    /// Watch a child's piped stdout on a helper thread: a `recv` that BLOCKS means the
    /// child is still alive (its write end is open); a `recv` that yields `0` means the
    /// child exited and its stdout reached EOF - i.e. it was reaped.
    fn watch_for_exit(mut out: std::process::ChildStdout) -> std::sync::mpsc::Receiver<usize> {
        use std::io::Read;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            let n = out.read(&mut buf).unwrap_or(0);
            let _ = tx.send(n);
        });
        rx
    }

    #[test]
    fn reaped_child_reaps_even_when_the_driver_panics() {
        let (child, out) = spawn_blocking_child();
        let exited = watch_for_exit(out);

        // A CRASHING driver (a panicking agent) still unwinds through the guard's Drop,
        // so the child is reaped on the crash path exactly as on the clean path.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ReapedChild::new(child);
            panic!("the driving agent crashed");
        }));
        assert!(panicked.is_err(), "the closure was expected to panic");

        let n = exited
            .recv_timeout(Duration::from_secs(5))
            .expect("a panic-unwound ReapedChild did not reap its process");
        assert_eq!(n, 0, "a reaped child's stdout should be at EOF");
    }
}

#[cfg(test)]
mod bounded_read {
    //! [`ReapedChild::read_lines_until`]: a read of the child's stdout that hands each line over
    //! until the child ends it, the reader breaks, a line is no text or the deadline passes,
    //! whichever comes first, so a child that answers nothing holds its reader no longer than the
    //! deadline.
    use super::ReapedChild;
    use std::ops::ControlFlow;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// One row of the table below.
    type Row<'a> = (
        &'a str,
        &'a str,
        bool,
        Duration,
        Option<&'a str>,
        &'a [&'a str],
        (Duration, Duration),
    );

    #[cfg(unix)]
    #[test]
    fn a_read_ends_at_the_end_of_the_stream_a_break_a_line_that_is_no_text_or_the_deadline() {
        let long = Duration::from_secs(5);
        let short = Duration::from_millis(600);
        let early = (Duration::ZERO, Duration::from_secs(2));
        let at_the_bound = (short, short + Duration::from_millis(1500));
        // (name, the child's shell, its stdout piped, the bound, the line the reader breaks at,
        // the lines read, elapsed from and below)
        let rows: [Row; 5] = [
            (
                "a child that ends",
                "printf 'a\\nb\\n'",
                true,
                long,
                None,
                &["a", "b"],
                early,
            ),
            (
                "a child still running at the deadline",
                "printf 'a\\nb\\n'; exec sleep 3",
                true,
                short,
                None,
                &["a", "b"],
                at_the_bound,
            ),
            (
                "a reader that breaks",
                "printf 'a\\nb\\n'; exec sleep 3",
                true,
                long,
                Some("a"),
                &["a"],
                early,
            ),
            (
                "a line that is no text",
                "printf 'a\\n\\377\\nb\\n'; exec sleep 3",
                true,
                long,
                None,
                &["a"],
                early,
            ),
            (
                "a child with no stdout pipe",
                "exec sleep 3",
                false,
                long,
                None,
                &[],
                early,
            ),
        ];
        for (name, script, piped, bound, break_at, expected, (from, below)) in rows {
            let child = Command::new("sh")
                .args(["-c", script])
                .stdout(if piped { Stdio::piped() } else { Stdio::null() })
                .spawn()
                .unwrap();
            let mut child = ReapedChild::new(child);
            let mut read = Vec::new();
            let started = Instant::now();
            child.read_lines_until(started + bound, |line| {
                read.push(line.to_string());
                if Some(line) == break_at {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
            let elapsed = started.elapsed();
            assert_eq!(read, expected, "{name}");
            assert!(
                from <= elapsed && elapsed < below,
                "{name}: the read ended after {elapsed:?}, outside {from:?}..{below:?}"
            );
        }
    }
}
