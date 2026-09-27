//! Shared support for the suites that pin a LIGHTWEIGHT `workflow.yml` probe's lib-API contract
//! (`tests/scratch_workdir_config.rs` for `read_scratch_workdir`, `tests/store_config.rs` for
//! `read_store_config`): a temp `.rigger` directory, the committed config written into it, and
//! the one "this body reads as exactly this value" check both probes are held to. Included by each
//! suite through `#[path]`.

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A temp `.rigger` directory the reader is anchored at (it joins `workflow.yml` onto this). The
/// returned `TempDir` must be kept alive by the caller or the directory is removed underneath it.
pub fn rigger_dir() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dir = tmp.path().join(".rigger");
    std::fs::create_dir_all(&dir).expect("create .rigger");
    (tmp, dir)
}

/// Write `<rigger_dir>/workflow.yml` with `body` - the committed project config the reader parses.
pub fn write_workflow(dir: &Path, body: &str) {
    std::fs::write(dir.join("workflow.yml"), body).expect("write workflow.yml");
}

/// A workflow.yml of `body` reads, through the probe `read`, as exactly `expected`; `why` names
/// the contract for the failure message.
pub fn assert_probe_reads<T: Debug + PartialEq, E: Debug>(
    read: impl Fn(&Path) -> Result<T, E>,
    body: &str,
    expected: T,
    why: &str,
) {
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, body);
    let got = read(&dir).unwrap_or_else(|e| panic!("{why}: the probe must read it: {e:?}"));
    assert_eq!(got, expected, "{why}");
}
