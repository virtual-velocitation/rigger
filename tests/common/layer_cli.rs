//! Shared support for the two grouping-report CLI suites (`tests/community_detection_cli.rs`,
//! `tests/concepts_derivation_cli.rs`): `rigger graph communities` and `rigger graph concepts`
//! share one shape - a read-only report of the groupings the code store holds for the head, at
//! the default grain or at a `--resolution` computed on demand. [`LayerCli`] names one such
//! subcommand; its methods are the binary-boundary checks both suites run against it. Mounted
//! once by `tests/all.rs`; a suite reaches it with `use crate::layer_cli;`.

use std::path::Path;
use std::process::{Command, Output};

use crate::common::fixtures::{commit_tree, write_files};
use crate::common::lens::not_a_grain;
use crate::common::rigger_bin;

/// One grouping-report subcommand under test.
pub struct LayerCli {
    /// The `rigger graph <subcommand>` name: `communities` or `concepts`.
    pub subcommand: &'static str,
    /// The summary line a report over an empty tree prints.
    pub empty_summary: &'static str,
    /// The `(path, text)` files of a tree the subcommand groups into at least one cluster.
    #[cfg(feature = "symbols")]
    pub tree: fn() -> Vec<(String, String)>,
}

/// One cluster line of a report: `(id, member count, label)`.
pub type Line = (String, usize, String);

impl LayerCli {
    /// A git project whose head holds `files`.
    pub fn project(&self, files: &[(String, String)]) -> tempfile::TempDir {
        let dir = crate::common::cli::identified_git_project();
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        write_files(dir.path(), &files);
        commit_tree(dir.path());
        dir
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

    /// The summary line and the cluster lines of a successful report in `root`.
    pub fn report(&self, root: &Path, args: &[&str]) -> (String, Vec<Line>) {
        let out = self.run(root, args);
        assert!(
            out.status.success(),
            "graph {} {args:?} succeeds: {}",
            self.subcommand,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut lines = stdout.lines();
        let summary = lines.next().unwrap_or_default().to_string();
        let clusters = lines
            .map(|line| {
                let mut cols = line.splitn(3, '\t');
                let id = cols.next().unwrap_or_default().to_string();
                let count = cols
                    .next()
                    .and_then(|c| c.parse().ok())
                    .unwrap_or_else(|| panic!("a cluster line carries a member count: {line}"));
                (id, count, cols.next().unwrap_or_default().to_string())
            })
            .collect();
        (summary, clusters)
    }

    /// An empty tree reports no grouping and still exits 0.
    pub fn an_empty_project_reports_no_grouping_and_still_succeeds(&self) {
        let dir = self.project(&[]);
        let (summary, clusters) = self.report(dir.path(), &[]);
        assert_eq!(summary, self.empty_summary);
        assert!(clusters.is_empty(), "no cluster line: {clusters:?}");
    }

    /// The report reads the code store and writes nothing to the log, whatever its grain.
    #[cfg(feature = "symbols")]
    pub fn the_report_writes_nothing_to_the_log(&self) {
        use rigger::eventstore::sqlite::Store;
        use rigger::eventstore::{Direction, EventStore, Filter};

        let dir = self.project(&(self.tree)());
        let root = dir.path();
        for args in [&[][..], &["--resolution", "2"]] {
            let (_, clusters) = self.report(root, args);
            assert!(!clusters.is_empty(), "{args:?} reports a cluster");
        }
        let log = root.join(".rigger").join("events.db");
        if log.exists() {
            let store = Store::open(log.to_str().unwrap()).unwrap();
            let events = store
                .read_all(0, Direction::Forward, &Filter::default())
                .unwrap();
            assert!(events.is_empty(), "the report records nothing: {events:?}");
        }
    }

    /// The default grain names its clusters by resolution 1, and a grain asked for names its
    /// clusters by that grain: the default is stored, any other is computed on demand.
    #[cfg(feature = "symbols")]
    pub fn each_grain_names_its_clusters_by_its_resolution(&self) {
        let dir = self.project(&(self.tree)());
        let root = dir.path();
        for (args, resolution) in [(&[][..], "1"), (&["--resolution", "2"], "2")] {
            let (summary, clusters) = self.report(root, args);
            assert!(
                summary.ends_with(&format!("at resolution {resolution}")),
                "the summary names the grain: {summary}"
            );
            let members: usize = clusters.iter().map(|(_, count, _)| count).sum();
            assert!(
                summary.contains(&format!(" over {members} node(s) ")),
                "the summary counts the members of its {} cluster(s): {summary}",
                clusters.len()
            );
            let prefix = format!("/{resolution}/");
            assert!(
                clusters.iter().all(|(id, _, _)| id.contains(&prefix)),
                "every cluster id carries the grain {resolution}: {clusters:?}"
            );
        }
    }

    /// Argument validation: a non-numeric resolution and a non-positive resolution each exit
    /// non-zero saying the text is not a grain and what a grain is, in the dash's own sentence
    /// ([`not_a_grain`]); an unknown argument exits non-zero naming it.
    pub fn a_malformed_resolution_or_unknown_argument_fails_loudly(&self) {
        let dir = self.project(&[]);
        let root = dir.path();

        for (args, what, message) in [
            (
                &["--resolution", "not-a-number"][..],
                "a non-numeric --resolution",
                format!("graph {}: {}", self.subcommand, not_a_grain("not-a-number")),
            ),
            (
                &["--resolution", "0"][..],
                "a non-positive --resolution",
                format!("graph {}: {}", self.subcommand, not_a_grain("0")),
            ),
            (
                &["--bogus"][..],
                "an unknown argument",
                "unknown argument".to_string(),
            ),
        ] {
            let out = self.run(root, args);
            assert!(!out.status.success(), "{what} is rejected");
            assert!(
                String::from_utf8_lossy(&out.stderr).contains(message.as_str()),
                "the error for {what} names `{message}`: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
