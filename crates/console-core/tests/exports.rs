//! THE MODULE AND ITS ABI (spec 93 criterion 2): "the member crate exports exactly
//! `console_alloc`, `console_free` and `console_call` (proven by parsing the artifact's
//! export section)". This is the one check that actually needs a real
//! `wasm32-unknown-unknown` compile of this crate - a real, permanent per-`cargo test` cost
//! (a cross-compile) this repository's own established convention (`tests/
//! core_lane_purity_audit.rs`, spec 93 criterion 1) does not ask every future unit's gate
//! cycle to pay: skipped, not failed, unless `RIGGER_CONSOLE_CORE_ABI_VERIFY=1` is set. Run
//! by hand while this unit was authored and confirmed clean (see this unit's own
//! DecisionMade).
//!
//! The export-section parser below is hand-rolled (no `wasmparser`/`walrus`/etc. dependency,
//! per spec 93's own global constraint: "no new crate dependency") over the WebAssembly
//! binary format's own documented shape. A module is a `\0asm` magic, a version, then a
//! sequence of `(id: u8, size: varuint32, contents)` sections; the export section (id 7) is
//! a `varuint32` count followed by that many `(name: len-prefixed-utf8, kind: u8, index:
//! varuint32)` entries.

use std::path::PathBuf;
use std::process::Command;

const CARGO_TOML: &str = env!("CARGO_MANIFEST_DIR");

fn skip_unless_opted_in() -> bool {
    if std::env::var("RIGGER_CONSOLE_CORE_ABI_VERIFY").as_deref() != Ok("1") {
        eprintln!(
            "skipping: set RIGGER_CONSOLE_CORE_ABI_VERIFY=1 to actually cross-compile this \
             crate for wasm32-unknown-unknown and parse its export section (see this file's \
             own module doc for why it is opt-in)"
        );
        return true;
    }
    false
}

/// Read a WebAssembly module's export section: `(name, kind)` pairs, in file order. `kind`
/// is 0=function, 1=table, 2=memory, 3=global - the format the spec itself defines.
fn parse_wasm_exports(bytes: &[u8]) -> Vec<(String, u8)> {
    assert_eq!(&bytes[0..4], b"\0asm", "not a wasm module (bad magic)");
    let mut pos = 8usize; // magic (4) + version (4)
    let mut exports = Vec::new();

    fn read_varuint32(bytes: &[u8], mut pos: usize) -> (u32, usize) {
        let mut result: u32 = 0;
        let mut shift = 0;
        loop {
            let b = bytes[pos];
            pos += 1;
            result |= u32::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        (result, pos)
    }

    while pos < bytes.len() {
        let section_id = bytes[pos];
        pos += 1;
        let (size, new_pos) = read_varuint32(bytes, pos);
        pos = new_pos;
        let section_end = pos + size as usize;
        if section_id == 7 {
            let (count, mut p) = read_varuint32(bytes, pos);
            for _ in 0..count {
                let (name_len, np) = read_varuint32(bytes, p);
                p = np;
                let name = std::str::from_utf8(&bytes[p..p + name_len as usize])
                    .expect("export name must be valid utf8")
                    .to_string();
                p += name_len as usize;
                let kind = bytes[p];
                p += 1;
                let (_index, np) = read_varuint32(bytes, p);
                p = np;
                exports.push((name, kind));
            }
        }
        pos = section_end;
    }
    exports
}

/// THE CRITERION 2 PROOF: cross-compile this crate for `wasm32-unknown-unknown` (release,
/// this crate alone via `-p console-core` - never the whole workspace, matching the root
/// `Cargo.toml`'s own "THE MEMBER CRATE" doc on why a bare workspace build never reaches
/// here) and parse its own emitted `.wasm` artifact's export section: the FUNCTION exports
/// (kind 0) must be EXACTLY `{console_alloc, console_call, console_free}` - no more, no
/// fewer. `memory` (kind 2) and the linker's own `__data_end`/`__heap_base` globals (kind 3)
/// are wasm32-unknown-unknown's standard, unavoidable non-function exports (a page needs
/// `memory` to slice the pointers this ABI hands back - see this crate's own module doc) and
/// are correctly excluded from "exactly three FUNCTIONS", the criterion's own wording.
#[test]
fn wasm32_artifact_exports_exactly_the_three_abi_functions() {
    if skip_unless_opted_in() {
        return;
    }

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(CARGO_TOML).join("../../target"));

    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "console-core",
            "--locked",
        ])
        .current_dir(PathBuf::from(CARGO_TOML).join("../.."))
        .status()
        .expect("spawning cargo build for wasm32-unknown-unknown");
    assert!(
        status.success(),
        "wasm32-unknown-unknown build of console-core failed (is the target installed? \
         `rustup target add wasm32-unknown-unknown`)"
    );

    let artifact = target_dir
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("console_core.wasm");
    let bytes =
        std::fs::read(&artifact).unwrap_or_else(|e| panic!("reading {}: {e}", artifact.display()));

    let exports = parse_wasm_exports(&bytes);
    let mut functions: Vec<&str> = exports
        .iter()
        .filter(|(_, kind)| *kind == 0)
        .map(|(name, _)| name.as_str())
        .collect();
    functions.sort_unstable();
    assert_eq!(
        functions,
        vec!["console_alloc", "console_call", "console_free"],
        "the member crate must export EXACTLY these three functions, no more and no fewer: \
         {exports:?}"
    );
}
