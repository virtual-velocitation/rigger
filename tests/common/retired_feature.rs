//! The source-hygiene check of a retired cargo feature, shared by the suites that retire one
//! (`tests/kurrentdb_always_available.rs`, `tests/turbovec_retired.rs`). Included by path
//! (`#[path = "common/retired_feature.rs"] mod retired_feature;`) next to `mod common;`,
//! whose `repo` walker it drives.

use std::path::PathBuf;

use crate::common::repo::for_each_rs_file;

/// No line under `src/` still gates on a retired cargo feature: none whose text, with every
/// whitespace character removed (so `feature="x"` and `feature = "x"` both match), contains
/// any of `gates`. `why` states the retirement for the failure message, which then lists the
/// offending `path:line`s.
pub fn assert_no_src_line_gates_on(gates: &[&str], why: &str) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for_each_rs_file(&src, &mut |path, text| {
        for (idx, line) in text.lines().enumerate() {
            let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if gates.iter().any(|gate| squeezed.contains(gate)) {
                offenders.push(format!("{}:{}", path.display(), idx + 1));
            }
        }
    });
    assert!(offenders.is_empty(), "{why} Offending lines: {offenders:?}");
}
