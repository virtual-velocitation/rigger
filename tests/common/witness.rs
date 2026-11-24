//! The FIFO witness: a test's proof, without a signal sent, that every process a fixture started
//! has exited, and the release that ends whatever outlived the code under test.
//!
//! A fixture opens the witness FIFO's write end (`exec 9>"$RIGGER_TEST_WITNESS"`) before it starts
//! anything, so every process it starts inherits that descriptor, and one byte goes there for each
//! process that is up: the process's own, or for a holder the byte its starting shell writes once
//! the fork has returned. The test holds the only read end, opened before the fixture starts, and
//! close-on-exec, so no process the test or the code under test spawns inherits it. A process
//! drops its descriptors when it exits, as a zombie too, so a read answers end-of-file only once
//! every fixture process has exited, and "would block" while any one is still running.
//!
//! The fixture's processes block reading the release FIFO (`exec 8<"$RIGGER_TEST_RELEASE"`), whose
//! only write end the test holds, so none exits on its own while the test reads the witness.
//! Dropping that end, at [`Witness::release`] or at drop, gives each of them end-of-file, and they
//! exit. The two FIFOs live in a temporary directory removed when the witness drops.
//!
//! A fixture opens both FIFOs by sourcing `tests/fixtures/witness.sh`, which also gives it `up`,
//! `held` and `holder`; [`Witness::script`] writes a shell fixture that starts that way.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What a read of the witness answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// End-of-file: no process the fixture started is still running.
    Ended,
    /// Would block: at least one process the fixture started is still running.
    Held,
}

/// How long [`Witness::release`] waits for the released processes to exit.
const RELEASE_BOUND: Duration = Duration::from_secs(10);

/// The witness and release FIFOs of one fixture run; see the module documentation.
pub struct Witness {
    witness: File,
    release: Option<File>,
    witness_path: PathBuf,
    release_path: PathBuf,
    up: usize,
    _dir: tempfile::TempDir,
}

impl Witness {
    /// Two fresh FIFOs, the witness's read end and the release's write end held by this test.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory for the witness FIFOs");
        let witness_path = dir.path().join("witness");
        let release_path = dir.path().join("release");
        make_fifo(&witness_path);
        make_fifo(&release_path);
        let witness = open_read_end(&witness_path);
        // A FIFO's write end opens only while a read end is open, so a read end is held across
        // the open and dropped after it: the fixture's own read ends are the only ones left.
        let reader = open_read_end(&release_path);
        let release = OpenOptions::new()
            .write(true)
            .open(&release_path)
            .expect("open the release FIFO's write end");
        drop(reader);
        Witness {
            witness,
            release: Some(release),
            witness_path,
            release_path,
            up: 0,
            _dir: dir,
        }
    }

    /// The environment the fixture reads both FIFOs from.
    pub fn env(&self) -> Vec<(String, String)> {
        [
            ("RIGGER_TEST_WITNESS", &self.witness_path),
            ("RIGGER_TEST_RELEASE", &self.release_path),
        ]
        .into_iter()
        .map(|(var, path)| (var.to_string(), path.to_string_lossy().into_owned()))
        .collect()
    }

    /// Reads every byte written so far, counting each as one process up, and answers whether any
    /// fixture process is still running.
    pub fn read(&mut self) -> Reading {
        let mut buf = [0u8; 64];
        loop {
            match self.witness.read(&mut buf) {
                Ok(0) => return Reading::Ended,
                Ok(n) => self.up += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Reading::Held,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => panic!("read the witness FIFO: {e}"),
            }
        }
    }

    /// How many fixture processes have written their up-byte, as of the last [`Witness::read`].
    pub fn up(&self) -> usize {
        self.up
    }

    /// Reads the witness until `count` fixture processes are up, and answers whether they were
    /// within [`RELEASE_BOUND`].
    pub fn wait_up(&mut self, count: usize) -> bool {
        let deadline = Instant::now() + RELEASE_BOUND;
        loop {
            self.read();
            if self.up >= count {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The text of a shell fixture that sources `tests/fixtures/witness.sh` and then runs `body`,
    /// for `sh -c` with [`Witness::env`] set.
    pub fn script(body: &str) -> String {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sourced = manifest
            .ancestors()
            .map(|dir| dir.join("tests/fixtures/witness.sh"))
            .find(|file| file.is_file())
            .expect("tests/fixtures/witness.sh above the crate's manifest");
        format!(". '{}'\n{body}", sourced.display())
    }

    /// Releases every fixture process and answers [`Reading::Ended`] once all have exited, or
    /// [`Reading::Held`] if one is still running [`RELEASE_BOUND`] later.
    pub fn release(&mut self) -> Reading {
        self.release = None;
        let deadline = Instant::now() + RELEASE_BOUND;
        loop {
            let reading = self.read();
            if reading == Reading::Ended || Instant::now() >= deadline {
                return reading;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Default for Witness {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Witness {
    /// Releases what is still running, so no fixture process outlives the test by more than its
    /// release, a failed assertion's unwind included.
    fn drop(&mut self) {
        if self.release.is_some() {
            self.release();
        }
    }
}

/// Makes a FIFO at `path`.
pub fn make_fifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "test setup: mkfifo {path:?} must succeed");
}

/// Opens `path`'s read end without blocking, for a FIFO no writer has opened yet.
fn open_read_end(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .unwrap_or_else(|e| panic!("open {path:?}'s read end: {e}"))
}
