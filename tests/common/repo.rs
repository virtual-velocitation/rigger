//! Fixtures that read this repository's own checked-in files: sources, docs, manifests and the
//! committed audit ledgers.

use std::path::{Path, PathBuf};

/// The repository root (the package's manifest directory).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The text of the checked-in file at `rel` (relative to [`repo_root`]).
pub fn repo_text(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The committed JSON file at `rel`, decoded as the documented `contract` a downstream
/// consumer relies on - failing loudly, naming both, when it does not decode.
pub fn committed_json<T: serde::de::DeserializeOwned>(rel: &str, contract: &str) -> T {
    serde_json::from_str(&repo_text(rel))
        .unwrap_or_else(|e| panic!("{rel} does not deserialize as the documented {contract}: {e}"))
}

/// The lines of the TOML table `[header]` in `manifest`, up to the next table header.
pub fn table_lines(manifest: &str, header: &str) -> Vec<String> {
    let want = format!("[{header}]");
    let mut in_table = false;
    let mut out = Vec::new();
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_table = line == want;
            continue;
        }
        if in_table {
            out.push(raw.to_string());
        }
    }
    out
}

/// Whether the TOML table `[header]` in `manifest` declares `key` (as `key = ..`, `key=..` or a
/// dotted `key.sub = ..`).
pub fn table_declares_key(manifest: &str, header: &str, key: &str) -> bool {
    table_lines(manifest, header).iter().any(|line| {
        let t = line.trim();
        t == key
            || t.starts_with(&format!("{key} "))
            || t.starts_with(&format!("{key}="))
            || t.starts_with(&format!("{key}."))
    })
}

/// Call `visit` with the path and text of every `.rs` file under `dir`, recursively.
pub fn for_each_rs_file(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read dir {}: {e}", dir.display()),
    };
    for entry in entries {
        let path = entry.expect("dir entry must be readable").path();
        if path.is_dir() {
            for_each_rs_file(&path, visit);
        } else if path.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            visit(&path, &text);
        }
    }
}
