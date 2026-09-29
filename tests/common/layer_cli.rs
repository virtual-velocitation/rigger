//! Shared support for the two derived-layer CLI suites (`tests/community_detection_cli.rs`,
//! `tests/concepts_derivation_cli.rs`): `rigger graph communities` and `rigger graph concepts`
//! share one shape - a subcommand that reads the project's graph, derives a layer of nodes at a
//! `--resolution` grain, and records it through `append_and_fold_batch` as live membership
//! edges. [`LayerCli`] names one such subcommand; its methods are the binary-boundary checks
//! both suites run against it. Included by each suite through `#[path]`, next to its own
//! `mod common;`.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

use rigger::contextgraph::sqlite::Projector;

use crate::common::rigger_bin;

/// A throwaway git project with `.rigger/project.id` pinned to `identity`. Its own git repo
/// makes the identity resolution deterministic (the top-level is the fixture), and the pinned
/// id file makes the seed and the binary agree on the store namespace. Kept alive by the caller.
pub fn project(identity: &str) -> tempfile::TempDir {
    let dir = crate::common::cli::identified_git_project();
    std::fs::write(dir.path().join(".rigger").join("project.id"), identity).unwrap();
    dir
}

/// The local `.rigger/<name>` path under the fixture, as the binary's `db_path` resolves it.
pub fn rigger_db(root: &Path, name: &str) -> String {
    root.join(".rigger")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// The layer node a member currently belongs to (its single membership target), if any.
pub fn member_of<'a>(edges: &'a [(String, String)], node: &str) -> Option<&'a str> {
    edges
        .iter()
        .find(|(from, _)| from == node)
        .map(|(_, to)| to.as_str())
}

/// One derived-layer subcommand under test.
pub struct LayerCli {
    /// The `rigger graph <subcommand>` name: `communities` or `concepts`.
    pub subcommand: &'static str,
    /// The project identity the fixture pins and the seed opens its store under.
    pub identity: &'static str,
    /// The layer's node kind (`KIND_COMMUNITY`, `KIND_CONCEPT`).
    pub kind: &'static str,
    /// The layer's live membership relation (`REL_IN_COMMUNITY`, `REL_REALIZES`).
    pub rel: &'static str,
    /// The layer's node-id prefix: ids read `<prefix>/<resolution>/<n>`.
    pub id_prefix: &'static str,
    /// The three summary fragments an EMPTY pass prints: zero layer nodes, zero linked input
    /// nodes, zero recorded events.
    pub empty_summary: [&'static str; 3],
    /// Seeds a non-empty input graph into the fixture's real store.
    pub seed: fn(&Path),
}

impl LayerCli {
    /// A pinned fixture project for this layer's identity.
    pub fn project(&self) -> tempfile::TempDir {
        project(self.identity)
    }

    /// Run `rigger graph <subcommand> <args>` in `root` over the built binary.
    pub fn run(&self, root: &Path, args: &[&str]) -> Output {
        let mut argv = vec!["graph", self.subcommand];
        argv.extend_from_slice(args);
        Command::new(rigger_bin())
            .args(&argv)
            .current_dir(root)
            .env("RIGGER_NO_DASH", "1")
            .output()
            .unwrap_or_else(|e| panic!("spawn rigger graph {}: {e}", self.subcommand))
    }

    /// The live layer read back over the PUBLIC projection surface: the sorted set of layer
    /// node ids, and every live `<member> --rel--> <layer node>` edge as `(member, node)` pairs.
    pub fn live_layer(&self, root: &Path) -> (Vec<String>, Vec<(String, String)>) {
        let graph = Projector::open(&rigger_db(root, "graph.db"), self.identity).unwrap();
        let whole = graph.whole().unwrap();
        let mut nodes: Vec<String> = whole
            .nodes
            .iter()
            .filter(|n| n.kind == self.kind)
            .map(|n| n.id.clone())
            .collect();
        nodes.sort();
        nodes.dedup();
        let mut edges: Vec<(String, String)> = whole
            .edges
            .iter()
            .filter(|e| e.rel == self.rel)
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();
        edges.sort();
        (nodes, edges)
    }

    /// The layer node ids of resolution grain `grain` among `nodes`.
    fn grain(&self, nodes: &[String], grain: &str) -> BTreeSet<String> {
        let prefix = format!("{}/{grain}/", self.id_prefix);
        nodes
            .iter()
            .filter(|c| c.starts_with(&prefix))
            .cloned()
            .collect()
    }

    /// A project with no input edges is a clean no-op end-to-end: the pass derives nothing,
    /// records nothing, and exits 0 (never an error). This proves the CLI wiring, the store
    /// bootstrap, and the empty no-op path over the built binary in BOTH feature lanes.
    pub fn an_empty_project_records_nothing_and_still_succeeds(&self) {
        let dir = self.project();
        let root = dir.path();

        let out = self.run(root, &[]);
        assert!(
            out.status.success(),
            "an empty project is a no-op for graph {}, not an error: {}",
            self.subcommand,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        for fragment in self.empty_summary {
            assert!(
                stdout.contains(fragment),
                "the empty-pass summary reports `{fragment}`: {stdout}"
            );
        }

        let (nodes, edges) = self.live_layer(root);
        assert!(
            nodes.is_empty() && edges.is_empty(),
            "no {} layer materialized for an empty project: {} node(s), {} edge(s)",
            self.id_prefix,
            nodes.len(),
            edges.len()
        );
    }

    /// Two grains are recorded from the same input graph, then the default grain is re-run.
    /// The `--resolution` grains coexist (distinct `<prefix>/<r>/*` ids, both live), and a
    /// re-run of one grain REPLACES only that grain's set (the `fresh` pass boundary) - the
    /// other grain is untouched, and the re-run leaves exactly one live membership per member
    /// (no duplicates).
    pub fn resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain(&self) {
        let dir = self.project();
        let root = dir.path();
        (self.seed)(root);

        assert!(
            self.run(root, &[]).status.success(),
            "recording the default grain succeeds"
        );
        assert!(
            self.run(root, &["--resolution", "2"]).status.success(),
            "recording a second grain (r=2) succeeds"
        );

        let (nodes, _edges) = self.live_layer(root);
        let grain1 = self.grain(&nodes, "1");
        let grain2_before = self.grain(&nodes, "2");
        assert!(!grain1.is_empty(), "the default grain is live");
        assert!(
            !grain2_before.is_empty(),
            "the r=2 grain coexists with the default grain (distinct ids), not destroyed by it"
        );

        // Re-run the default grain: it must supersede ONLY grain 1, leaving grain 2 whole.
        assert!(
            self.run(root, &[]).status.success(),
            "re-running the default grain succeeds"
        );
        let (nodes2, edges2) = self.live_layer(root);
        let grain2_after = self.grain(&nodes2, "2");
        assert_eq!(
            grain2_before, grain2_after,
            "re-running the default grain leaves the r=2 grain's {} nodes intact",
            self.id_prefix
        );

        // After the re-run each default-grain member carries exactly ONE live membership - the
        // prior pass's memberships were superseded, not left as stale duplicates.
        let default_prefix = format!("{}/1/", self.id_prefix);
        let mut default_members: Vec<&String> = edges2
            .iter()
            .filter(|(_, to)| to.starts_with(&default_prefix))
            .map(|(from, _)| from)
            .collect();
        let total = default_members.len();
        default_members.sort();
        default_members.dedup();
        assert!(
            total > 0,
            "the default grain still has live memberships after the re-run"
        );
        assert_eq!(
            default_members.len(),
            total,
            "each member has exactly one live default-grain membership after the re-run \
             (supersession, no duplicates)"
        );
    }

    /// Argument validation runs BEFORE any store side effect: a non-numeric resolution, a
    /// non-positive resolution, and an unknown argument each exit non-zero with the documented
    /// message. This pins the subcommand's parsing contract over the built binary.
    pub fn a_malformed_resolution_or_unknown_argument_fails_loudly(&self) {
        let dir = self.project();
        let root = dir.path();

        for (args, what, message) in [
            (
                &["--resolution", "not-a-number"][..],
                "a non-numeric --resolution",
                "--resolution expects a number",
            ),
            (
                &["--resolution", "0"][..],
                "a non-positive --resolution",
                "positive finite number",
            ),
            (&["--bogus"][..], "an unknown argument", "unknown argument"),
        ] {
            let out = self.run(root, args);
            assert!(!out.status.success(), "{what} is rejected");
            assert!(
                String::from_utf8_lossy(&out.stderr).contains(message),
                "the error for {what} names `{message}`: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    /// Determinism THROUGH THE BINARY, observed on the MATERIALIZED layer: running the
    /// subcommand twice on the same store re-derives, supersedes the grain's prior memberships,
    /// and re-folds - and the live layer read back over the public projection is identical to
    /// the first pass's (same layer node ids, same membership edges). This guards the
    /// derive -> supersede -> fold seam's end-to-end determinism as it lands in the store, which
    /// neither the supersession check (duplicate-freedom + the OTHER grain's survival only) nor
    /// a library-level byte-identical-events test (which never drives the binary or the fold)
    /// asserts.
    pub fn re_running_a_grain_reproduces_the_byte_identical_live_layer(&self) {
        let dir = self.project();
        let root = dir.path();
        (self.seed)(root);

        assert!(
            self.run(root, &[]).status.success(),
            "the first pass records the default grain"
        );
        let (nodes1, edges1) = self.live_layer(root);
        assert!(
            !nodes1.is_empty() && !edges1.is_empty(),
            "the first pass materialized a non-empty live {} layer (else the guard is vacuous): \
             {} node(s), {} edge(s)",
            self.id_prefix,
            nodes1.len(),
            edges1.len()
        );

        // Re-run the SAME grain over the SAME store: a deterministic pass reproduces the exact
        // live layer byte for byte.
        assert!(
            self.run(root, &[]).status.success(),
            "re-running the default grain succeeds"
        );
        let (nodes2, edges2) = self.live_layer(root);

        assert_eq!(
            nodes1, nodes2,
            "re-running the same grain reproduces the identical {} nodes",
            self.id_prefix
        );
        assert_eq!(
            edges1, edges2,
            "re-running the same grain reproduces the identical membership edges"
        );
    }

    /// Given a seeded project whose pass has folded, when the same pass runs while another
    /// writer holds `graph.db` locked past the busy timeout, then its line is the folded pass's
    /// line with the fold it could not make, and the reason, added - a folded pass adds nothing -
    /// and the graph now owes its rebuild, so the next pass refuses naming `rigger setup` rather
    /// than deriving from a graph that lost the recorded layer.
    pub fn a_pass_whose_fold_is_lost_to_a_lock_says_so_and_the_next_pass_refuses(&self) {
        let dir = self.project();
        let root = dir.path();
        (self.seed)(root);
        let folded = self.run(root, &[]);
        let folded_line = String::from_utf8_lossy(&folded.stdout).into_owned();
        assert!(
            folded.status.success()
                && folded_line.starts_with(&format!("graph {}: ", self.subcommand)),
            "the unlocked pass folds; stdout: {folded_line}"
        );

        let graph_db = root.join(".rigger").join("graph.db");
        let locked = crate::common::cli::with_graph_locked(&graph_db, || self.run(root, &[]));
        assert_eq!(
            (
                locked.status.success(),
                String::from_utf8_lossy(&locked.stdout).into_owned()
            ),
            (
                true,
                format!(
                    "{}; not folded into the context graph: graph: database is locked\n",
                    folded_line.trim_end_matches('\n')
                )
            ),
            "the locked pass says the fold it could not make; stderr: {}",
            String::from_utf8_lossy(&locked.stderr)
        );

        let next = self.run(root, &[]);
        let stderr = String::from_utf8_lossy(&next.stderr);
        assert!(
            !next.status.success()
                && stderr.contains(&rigger::contextgraph::rebuild_owed_refusal(&format!(
                    "graph {}",
                    self.subcommand
                ))),
            "the next pass refuses naming `rigger setup`; stderr: {stderr}"
        );
    }
}
