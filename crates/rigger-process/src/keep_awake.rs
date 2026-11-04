//! The keep-awake adapter (spec 105): the host's facility behind
//! [`rigger_domain::sleep_hold::SleepHold`], which keeps the machine from sleeping while a run
//! drives, through the native adapter of the OS the binary is built for. Of the production code
//! outside this crate, only the composition root names [`OsSleepHold`].

use rigger_domain::sleep_hold::{SleepGuard, SleepHold, Unavailable};

// The one spelling of the OS set: each OS with a native adapter declares its module and binds
// the module's `acquire` as `native_acquire`, and every other OS binds the inline `unsupported`
// module's in the `_` arm.
std::cfg_select! {
    target_os = "linux" => {
        mod linux;
        use linux::acquire as native_acquire;
    }
    target_os = "macos" => {
        mod macos;
        use macos::acquire as native_acquire;
    }
    target_os = "windows" => {
        mod windows;
        use windows::acquire as native_acquire;
    }
    _ => {
        /// The OSes with no native adapter, the `_` arm of the dispatch above.
        mod unsupported {
            use rigger_domain::sleep_hold::{SleepGuard, Unavailable};

            /// Unavailable, naming the OS.
            pub(super) fn acquire(_reason: &str) -> Result<SleepGuard, Unavailable> {
                Err(Unavailable(format!(
                    "no keep-awake adapter for {}",
                    std::env::consts::OS
                )))
            }

            #[cfg(test)]
            mod tests {
                use crate::keep_awake::OsSleepHold;
                use rigger_domain::sleep_hold::{SleepHold, Unavailable};

                /// On an OS with no native adapter, the host's acquire answers unavailable naming
                /// the OS.
                #[test]
                fn the_hosts_acquire_answers_unavailable_naming_the_os() {
                    assert_eq!(
                        OsSleepHold.acquire("rigger run is driving a run").err(),
                        Some(Unavailable(format!(
                            "no keep-awake adapter for {}",
                            std::env::consts::OS
                        )))
                    );
                }
            }
        }
        use unsupported::acquire as native_acquire;
    }
}

/// The keep-awake facility of the operating system this binary runs on: on Linux a logind
/// inhibitor lock, on macOS an IOKit power assertion, on Windows a power request; on an OS with
/// no native adapter, every acquire answers unavailable naming the OS.
pub struct OsSleepHold;

impl SleepHold for OsSleepHold {
    fn acquire(&self, reason: &str) -> Result<SleepGuard, Unavailable> {
        native_acquire(reason)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::table_lines;
    use std::collections::{BTreeMap, BTreeSet};

    /// This crate's manifest.
    const MANIFEST: &str = include_str!("../Cargo.toml");

    /// The workspace's lock file.
    const LOCK: &str = include_str!("../../../Cargo.lock");

    /// One package of [`LOCK`], by name and version.
    type Package = (&'static str, &'static str);

    /// The crate a dependency-table line declares, the bare key before its `=`; none for a blank
    /// line, a comment or a continuation of a value spanning lines.
    fn declared_crate(line: &str) -> Option<&str> {
        let (key, _) = line.split_once('=')?;
        let key = key.trim();
        key.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            .then_some(key)
    }

    /// The value of `key` in one `[[package]]` block of [`LOCK`].
    fn field(block: &'static str, key: &str) -> Option<&'static str> {
        block.lines().find_map(|line| {
            line.strip_prefix(key)?
                .strip_prefix(" = \"")?
                .strip_suffix('"')
        })
    }

    /// [`LOCK`]'s dependency graph and the workspace's own packages, those it records no source
    /// for. Each package maps to the packages it depends on, each reference resolved to a name
    /// and version: `name version` names that version, and a bare `name` the one version of the
    /// name the lock holds.
    fn lock_graph() -> (BTreeMap<Package, Vec<Package>>, Vec<Package>) {
        let blocks: Vec<&'static str> = LOCK.split("[[package]]\n").skip(1).collect();
        let package = |block: &'static str| -> Package {
            (
                field(block, "name").unwrap(),
                field(block, "version").unwrap(),
            )
        };
        let mut versions: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (name, version) in blocks.iter().map(|block| package(block)) {
            versions.entry(name).or_default().push(version);
        }
        let resolve = |reference: &'static str| -> Package {
            let mut words = reference.split(' ');
            let name = words.next().unwrap();
            let version = words.next().unwrap_or_else(|| match versions[name][..] {
                [only] => only,
                _ => panic!("the bare reference {name} names one of several versions"),
            });
            (name, version)
        };
        let graph = blocks
            .iter()
            .map(|block| {
                let references = block
                    .split_once("dependencies = [\n")
                    .map_or("", |(_, rest)| {
                        rest.split_once("\n]").map_or(rest, |(listed, _)| listed)
                    });
                let dependencies = references
                    .lines()
                    .map(|line| resolve(line.trim().trim_end_matches(',').trim_matches('"')))
                    .collect();
                (package(block), dependencies)
            })
            .collect();
        let workspace = blocks
            .iter()
            .filter(|block| field(block, "source").is_none())
            .map(|block| package(block))
            .collect();
        (graph, workspace)
    }

    /// The packages of `graph` that no path from `roots` reaches without crossing `edge`, the
    /// packages the lock holds for that edge alone.
    fn held_only_through(
        graph: &BTreeMap<Package, Vec<Package>>,
        roots: &[Package],
        edge: (Package, Package),
    ) -> Vec<Package> {
        let mut reached = BTreeSet::new();
        let mut frontier = roots.to_vec();
        while let Some(package) = frontier.pop() {
            if reached.insert(package) {
                frontier.extend(
                    graph[&package]
                        .iter()
                        .copied()
                        .filter(|&dependency| (package, dependency) != edge),
                );
            }
        }
        graph
            .keys()
            .copied()
            .filter(|package| !reached.contains(package))
            .collect()
    }

    /// Spec 105's macOS and Windows adapters, which compile out on this host, pinned through the
    /// manifest and the lock: the Windows adapter's `windows-sys` is declared under the Windows
    /// target alone, at the version the lock already resolves for the crates beside it, so the
    /// lock holds no package for that edge alone; and the macOS adapter, which reaches IOKit and
    /// CoreFoundation through `extern` declarations, declares no crate in any table.
    #[test]
    fn the_windows_adapter_takes_a_locked_windows_sys_and_the_macos_adapter_no_crate() {
        let declared: Vec<String> = MANIFEST
            .lines()
            .filter_map(|line| line.trim().strip_prefix('[')?.strip_suffix(']'))
            .filter(|header| header.ends_with("dependencies"))
            .map(|header| {
                let lines = table_lines(MANIFEST, header);
                let crates: Vec<&str> = lines.iter().filter_map(|l| declared_crate(l)).collect();
                format!("[{header}] {}", crates.join(" "))
            })
            .collect();
        assert_eq!(
            declared,
            [
                "[dependencies] rigger-domain",
                "[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies] rustix fs2",
                "[target.'cfg(target_os = \"linux\")'.dependencies] zbus",
                "[target.'cfg(target_os = \"windows\")'.dependencies] windows-sys",
                "[dev-dependencies] tempfile",
            ],
            "each dependency table of crates/rigger-process/Cargo.toml, with the crates it declares"
        );

        let (graph, workspace) = lock_graph();
        let process: Package = ("rigger-process", "0.1.0");
        let windows_sys: Vec<Package> = graph[&process]
            .iter()
            .copied()
            .filter(|(name, _)| *name == "windows-sys")
            .collect();
        assert_eq!(
            windows_sys,
            vec![("windows-sys", "0.61.2")],
            "the windows-sys the lock resolves for rigger-process"
        );
        assert_eq!(
            held_only_through(&graph, &workspace, (process, ("windows-sys", "0.61.2"))),
            Vec::<Package>::new(),
            "the packages the lock holds for rigger-process's windows-sys edge alone"
        );
    }
}
