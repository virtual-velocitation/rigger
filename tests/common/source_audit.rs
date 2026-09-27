//! Shared support for the whole-tree source audits (`tests/no_os_kill_audit.rs`,
//! `tests/reap_before_removal_audit.rs`): the one finding shape both report, the fixture-file
//! writer both prove detection with, and the real-tree assertion both close on. Included by
//! each suite through `#[path]`, never through `tests/common/mod.rs`.

use std::fs;
use std::path::Path;

/// One audit hit: which file, which 1-based line, which shape, and the offending line's own
/// text (for the failure message only - never re-scanned).
#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line_no: usize,
    pub shape: &'static str,
    pub line_text: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {} - `{}`",
            self.file,
            self.line_no,
            self.shape,
            self.line_text.trim()
        )
    }
}

/// Write `content` to `root/rel`, creating its parent directories - one fixture file.
pub fn write_file(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

/// The acceptance check both audits close on: `scan` over the REAL, currently checked-out tree
/// (resolved from `CARGO_MANIFEST_DIR`, never the process CWD) finds nothing. `audit` names the
/// audit and `what` the kind of site it counts, for the failure message.
pub fn assert_real_tree_clean(scan: fn(&Path) -> Vec<Finding>, audit: &str, what: &str) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let findings = scan(&root);
    assert!(
        findings.is_empty(),
        "{audit} found {} {what} in the real tree:\n{}",
        findings.len(),
        findings
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
