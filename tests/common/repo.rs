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

/// The committed JSON ledger `rel`, decoded as `contract`, holds at least one entry - a
/// downstream consumer pinning counts against it never silently sees nothing.
pub fn assert_committed_ledger_is_nonempty<T: serde::de::DeserializeOwned>(
    rel: &str,
    contract: &str,
) {
    let entries: Vec<T> = committed_json(rel, contract);
    assert!(
        !entries.is_empty(),
        "{rel} deserialized to zero entries - a downstream consumer pinning counts against \
         this file would silently see nothing"
    );
}

/// Decoding the committed JSON file `rel` as `contract` and re-encoding it reproduces the
/// committed bytes exactly - a downstream consumer decoding and re-encoding it never silently
/// diverges from the committed artifact.
pub fn assert_committed_json_round_trips<T>(rel: &str, contract: &str)
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let decoded: T = committed_json(rel, contract);
    let mut reencoded = serde_json::to_string_pretty(&decoded).expect("the contract re-serializes");
    reencoded.push('\n');
    assert_eq!(
        repo_text(rel),
        reencoded,
        "{rel} does not round-trip byte-for-byte through the documented {contract}"
    );
}

/// The committed text file `rel`, compared lowercase, carries none of `phrasings`; the failure
/// names what `rel` must describe instead (`why`) and every phrasing still present.
pub fn assert_doc_carries_none_of(rel: &str, phrasings: &[&str], why: &str) {
    let text = repo_text(rel).to_lowercase();
    let present: Vec<&str> = phrasings
        .iter()
        .copied()
        .filter(|phrasing| text.contains(phrasing))
        .collect();
    assert!(
        present.is_empty(),
        "{rel} {why}. Phrasings still present in the document: {present:#?}"
    );
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

/// Every `.rs` file strictly under `dir`, recursively, appended to `out` in deterministic
/// (sorted) finding order regardless of readdir order; an unreadable directory is skipped.
pub fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// The `(name, needle)` rows of `table` whose needle `text` does not contain, each rendered as
/// `{name}  ({label}: {needle:?})`.
pub fn missing_rows(text: &str, table: &[(&str, &str)], label: &str) -> Vec<String> {
    table
        .iter()
        .filter(|(_, needle)| !text.contains(needle))
        .map(|(name, needle)| format!("{name}  ({label}: {needle:?})"))
        .collect()
}
