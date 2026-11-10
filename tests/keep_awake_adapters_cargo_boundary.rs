//! Spec 105's macOS and Windows keep-awake adapters, proven at the TOOL BOUNDARY by driving
//! the real dependency resolver.
//!
//! Both adapters compile out on a Linux host, so no test here can run them. The crate's own
//! test in `crates/rigger-process/src/keep_awake.rs` pins their dependency constraint
//! STATICALLY, reading the committed manifest and lock as text. This file is the outside-in
//! half: it asks `cargo tree`, which resolves a target's graph without that target's standard
//! library installed, what each OS's build of `rigger-process` takes - the Windows adapter's
//! `windows-sys` under Windows alone, the macOS adapter no crate - and whether the
//! `windows-sys` it takes is the one the lock already resolves for `rustix` beside it.
//!
//! Every resolve passes `--locked`, so a manifest the lock does not already resolve fails
//! instead of rewriting `Cargo.lock`. Unlike `turbovec_retired_cargo_boundary.rs` it passes no
//! `--offline`: a Linux build never fetches the packages only a Windows build takes
//! (`windows-sys` and `windows-link`), so an offline Windows resolve fails on any cache only a
//! Linux build has warmed. A warm cache touches no network; a cold one fetches what the Windows
//! resolve needs, as `cargo build` would.

use crate::common;

/// The Windows target the resolver is asked about.
const WINDOWS: &str = "x86_64-pc-windows-msvc";

/// The direct normal dependencies of `package` (a cargo package spec) on `target`, each by
/// name and version, in the resolver's order.
fn direct_dependencies(package: &str, target: &str) -> Vec<(String, String)> {
    let out = common::repo::cargo_tree(&[
        "--locked",
        "--edges",
        "normal",
        "--depth",
        "1",
        "--target",
        target,
        "--package",
        package,
    ]);
    assert!(
        out.status.success(),
        "cargo tree --package {package} --target {target} resolves; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .map(|line| {
            let mut words = line.split_whitespace();
            let mut word = || words.next().unwrap_or_default().to_owned();
            (word(), word())
        })
        .collect()
}

/// The version `dependencies` holds for the crate `name`.
fn version_of(dependencies: &[(String, String)], name: &str) -> String {
    dependencies
        .iter()
        .find(|(dependency, _)| dependency == name)
        .map(|(_, version)| version.clone())
        .unwrap_or_else(|| panic!("no {name} among {dependencies:?}"))
}

/// Given the committed manifest and lock, when the resolver is asked what each OS's build of
/// `rigger-process` takes, then Windows alone takes `windows-sys` for its power request, macOS
/// takes no crate for its IOKit assertion, and Linux takes `zbus` for its logind inhibitor and
/// `tokio` for the runtime its acquire drives that client on.
#[test]
fn each_os_build_of_rigger_process_takes_its_own_keep_awake_adapters_crate_alone() {
    let rows: [(&str, &[&str]); 3] = [
        (WINDOWS, &["fs2", "rigger-domain", "rustix", "windows-sys"]),
        ("aarch64-apple-darwin", &["fs2", "rigger-domain", "rustix"]),
        (
            "x86_64-unknown-linux-gnu",
            &["fs2", "rigger-domain", "rustix", "tokio", "zbus"],
        ),
    ];
    for (target, crates) in rows {
        let names: Vec<String> = direct_dependencies("rigger-process", target)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, crates, "the crates rigger-process takes on {target}");
    }
}

/// Given the committed manifest and lock, when the resolver is asked which `windows-sys` the
/// Windows build of `rigger-process` takes, then it is the version the lock already resolves for
/// `rustix` on Windows, so the adapter adds no `windows-sys` of its own to the build.
#[test]
fn the_windows_adapters_windows_sys_is_the_one_rustix_already_resolves() {
    let process = direct_dependencies("rigger-process", WINDOWS);
    let rustix = format!(
        "rustix@{}",
        version_of(&process, "rustix").trim_start_matches('v')
    );
    assert_eq!(
        version_of(&process, "windows-sys"),
        version_of(&direct_dependencies(&rustix, WINDOWS), "windows-sys"),
        "the windows-sys rigger-process takes on {WINDOWS}, against the one {rustix} takes"
    );
}
