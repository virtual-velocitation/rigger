//! Shared support for the store-resolution suites that drive a BARE courier
//! (`tests/store_resolution_cli.rs`, `tests/store_secrets.rs`). Included by each suite through
//! `#[path]`, next to its own `mod common;`.

use std::path::Path;
use std::process::Output;

/// Run a BARE courier - `rigger result <spawn> --error ...` - in `root`, the same shape a spawned
/// agent's self-report uses. `conn` sets `KURRENTDB_CONN` (`Some("")` sets it empty; `None`
/// removes it so the case is truly unset regardless of the ambient environment).
/// `RIGGER_NO_DASH` keeps the run's dashboard from starting under test. `XDG_STATE_HOME` is
/// redirected to a per-call temp dir (spec 62, "couriers count as activity"): `result` now
/// refreshes the machine-global instance registry too, so an unredirected call here would
/// otherwise seed a phantom, since-deleted-tempdir entry into the operator's real
/// `~/.local/state/rigger/instances`.
pub fn run_bare_result(root: &Path, conn: Option<&str>) -> Output {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    let mut cmd = crate::common::rigger_courier();
    cmd.args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .env_remove("KURRENTDB_CONN");
    if let Some(c) = conn {
        cmd.env("KURRENTDB_CONN", c);
    }
    cmd.output().expect("spawn rigger result")
}
