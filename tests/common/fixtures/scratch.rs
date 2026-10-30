//! Scratch fixtures: the fresh temporary directory a test keeps its files under.

/// A fresh temporary directory and its path, the directory removed when the handle drops.
pub fn temp_root() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();
    (dir, root)
}
