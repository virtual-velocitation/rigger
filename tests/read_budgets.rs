//! Spec 117, THE READ BUDGETS HOLD. Run with `RIGGER_READ_BUDGETS=1`, the measuring test clones
//! HEAD into the project's `.rigger/tmp/agent-scratch`, builds the release binary there once, runs
//! `rigger graph build`, starts the dash on an ephemeral port and measures each budgeted read as
//! the median of five runs after one warm-up, through the release binary: `rigger graph --around`
//! and `--show` under 50 ms, `--lookup` under 250 ms, and the dash's `/api/graph` overview under
//! 150 ms by default, under 300 ms with `tests=include` and under 300 ms at a non-default
//! resolution. Without the variable it skips: the build and the timings take minutes and want a
//! quiet machine, so the gates do not run it and a unit's evidence carries the figures it prints.
//!
//! The suite is its own executable, so no sibling suite shares the machine while it times. The
//! tests beside the measuring one always run: they pin the median, the budget's bound, the answer
//! checks and the drop guard it rests on.

mod common;

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::git::{git_ok, git_out};
use common::served::{header_value, http_attempt, http_probe};

/// The variable that runs the measurement.
const MEASURE_ENV: &str = "RIGGER_READ_BUDGETS";

/// The timed runs of each read, after its one warm-up.
const SAMPLES: usize = 5;

/// `rigger dash` on an ephemeral loopback port, printing the address it bound and ending once its
/// stdin closes.
const DASH: [&str; 4] = ["dash", "--port-from", "0", "--exit-on-stdin-close"];

/// How long a started dash has to report the address it serves at.
const DASH_READY: Duration = Duration::from_secs(30);

/// The budget of `rigger graph --around` and `--show`.
const AROUND_AND_SHOW: Duration = Duration::from_millis(50);

/// The budget of `rigger graph --lookup`.
const LOOKUP: Duration = Duration::from_millis(250);

/// The budget of the dash's default `/api/graph` overview.
const OVERVIEW: Duration = Duration::from_millis(150);

/// The budget of the overview with `tests=include`, and of the overview at a non-default
/// resolution.
const WIDE_OVERVIEW: Duration = Duration::from_millis(300);

/// One read the budgets bound.
enum Read {
    /// `rigger <args>` in the clone. Its answer carries each of `shows` as a whole line, its words
    /// separated by any run of spaces, and its text section counts at least `matches` matching
    /// lines, so a term chosen for its thousands of matches stays one.
    Cli {
        args: &'static [&'static str],
        shows: &'static [&'static str],
        matches: usize,
    },
    /// `GET <path>` from the clone's dash.
    Dash { path: &'static str },
}

/// A read and the budget its median stays strictly under.
struct Row {
    read: Read,
    under: Duration,
}

/// Every budgeted read: `--around` on the two widest entities measured before this suite existed
/// and on a small neighbourhood, `--show` on a hit and on a miss, `--lookup` on a name, on a
/// many-match term and on a term with thousands of matches, and the three overviews.
const ROWS: &[Row] = &[
    Row {
        read: Read::Cli {
            args: &["graph", "--around", "src/cli/setup.rs::cmd_setup"],
            shows: &[
                "subgraph around \"src/cli/setup.rs::cmd_setup\" (depth 2):",
                "node src/cli/setup.rs::cmd_setup code-entity",
            ],
            matches: 0,
        },
        under: AROUND_AND_SHOW,
    },
    Row {
        read: Read::Cli {
            args: &["graph", "--around", "src/cli/graph.rs::print_show"],
            shows: &[
                "subgraph around \"src/cli/graph.rs::print_show\" (depth 2):",
                "node src/cli/graph.rs::print_show code-entity",
            ],
            matches: 0,
        },
        under: AROUND_AND_SHOW,
    },
    Row {
        read: Read::Cli {
            args: &[
                "graph",
                "--around",
                "src/cli/graph.rs::AROUND_GOVERNANCE_CAP",
            ],
            shows: &[
                "subgraph around \"src/cli/graph.rs::AROUND_GOVERNANCE_CAP\" (depth 2):",
                "node src/cli/graph.rs::AROUND_GOVERNANCE_CAP code-entity",
            ],
            matches: 0,
        },
        under: AROUND_AND_SHOW,
    },
    Row {
        read: Read::Cli {
            args: &["graph", "--show", "src/cli/setup.rs::cmd_setup"],
            shows: &["show src/cli/setup.rs::cmd_setup", "name cmd_setup"],
            matches: 0,
        },
        under: AROUND_AND_SHOW,
    },
    Row {
        read: Read::Cli {
            args: &["graph", "--show", "no_such_entity_xyz"],
            shows: &[
                "show \"no_such_entity_xyz\": no such entity in the graph (has it been \
                 built? try `rigger graph build`)",
            ],
            matches: 0,
        },
        under: AROUND_AND_SHOW,
    },
    Row {
        read: Read::Cli {
            args: &["graph", "--lookup", "cmd_setup"],
            shows: &[
                "# lookup: cmd_setup",
                "### cmd_setup",
                "exact match: 1 entity",
            ],
            matches: 0,
        },
        under: LOOKUP,
    },
    Row {
        read: Read::Cli {
            args: &["graph", "--lookup", "gate"],
            shows: &["# lookup: gate", "### gate"],
            matches: 0,
        },
        under: LOOKUP,
    },
    Row {
        read: Read::Cli {
            args: &["graph", "--lookup", "new"],
            shows: &["# lookup: new", "### new"],
            matches: 1000,
        },
        under: LOOKUP,
    },
    Row {
        read: Read::Dash { path: "/api/graph" },
        under: OVERVIEW,
    },
    Row {
        read: Read::Dash {
            path: "/api/graph?tests=include",
        },
        under: WIDE_OVERVIEW,
    },
    Row {
        read: Read::Dash {
            path: "/api/graph?lens=code&resolution=2",
        },
        under: WIDE_OVERVIEW,
    },
];

/// The median of [`SAMPLES`] timed runs of `read` after one warm-up run whose time is dropped,
/// and the samples in the order they ran.
fn median_after_warm_up(mut read: impl FnMut() -> Duration) -> (Duration, Vec<Duration>) {
    read();
    let samples: Vec<Duration> = (0..SAMPLES).map(|_| read()).collect();
    let mut sorted = samples.clone();
    sorted.sort();
    (sorted[SAMPLES / 2], samples)
}

/// Whether `median` is under `budget`, strictly, as the spec states each budget.
fn within(median: Duration, budget: Duration) -> bool {
    median < budget
}

/// Whether `answer` holds a line whose words, split on any run of whitespace, are `line`'s.
fn has_line(answer: &str, line: &str) -> bool {
    answer
        .lines()
        .any(|l| l.split_whitespace().eq(line.split_whitespace()))
}

/// The matching lines a `--lookup` answer's text section counts, read from its truncation line
/// (`truncated: <shown> of <n> matching lines shown ...`); 0 for an answer with no such line.
fn text_matches(answer: &str) -> usize {
    answer
        .lines()
        .find_map(|line| {
            line.strip_prefix("truncated: ")?
                .split_once(" of ")?
                .1
                .split_once(" matching lines shown")?
                .0
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

/// A duration in milliseconds to a tenth.
fn ms(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1000.0)
}

/// The first lines of a long answer, for a failure message.
fn head(answer: &str) -> String {
    answer.lines().take(20).collect::<Vec<_>>().join("\n")
}

/// The scratch the measuring test works in and the dash it starts there. Dropped on every way out
/// of a test, a panic's unwind included, it stops the dash first, through the `Child` handle that
/// spawned it ([`rigger::dash::ReapedChild`] ends and reaps it on drop), then removes the
/// directory holding the clone, its build and its cache.
struct Scratch {
    // Fields drop in declaration order: the dash stops before its tree is removed.
    dash: Option<rigger::dash::ReapedChild>,
    dir: tempfile::TempDir,
}

impl Scratch {
    /// A fresh directory under the project's `.rigger/tmp/agent-scratch`, with no dash started.
    fn new() -> Self {
        let parent = common::repo::repo_root().join(".rigger/tmp/agent-scratch");
        std::fs::create_dir_all(&parent).expect("create the project's agent scratch");
        let dir = tempfile::Builder::new()
            .prefix("read-budgets-")
            .tempdir_in(&parent)
            .expect("create the read budgets scratch");
        Scratch { dash: None, dir }
    }

    /// The clone of HEAD.
    fn root(&self) -> PathBuf {
        self.dir.path().join("clone")
    }

    /// The release binary the clone builds, in a target directory beside the clone, so the
    /// clone's tree stays exactly HEAD's.
    fn binary(&self) -> PathBuf {
        self.dir
            .path()
            .join("target/release")
            .join(format!("rigger{}", std::env::consts::EXE_SUFFIX))
    }

    /// The release `rigger` with `args`, run in the clone under the suites' environment hygiene
    /// ([`common::unfenced`]), with its cache home inside this scratch.
    fn rigger(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(self.binary());
        common::unfenced(&mut cmd)
            .args(args)
            .current_dir(self.root())
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"));
        cmd
    }

    /// Clone the project into [`Scratch::root`], checked out detached at its HEAD, and return
    /// that commit.
    fn clone_head(&self) -> String {
        let project = common::repo::repo_root();
        let head = git_out(&project, &["rev-parse", "HEAD"]);
        let root = self.root();
        let (from, to) = (project.to_str().unwrap(), root.to_str().unwrap());
        git_ok(&project, &["clone", "--quiet", "--no-checkout", from, to]);
        git_ok(&root, &["fetch", "--quiet", from, "HEAD"]);
        git_ok(&root, &["checkout", "--quiet", "--detach", &head]);
        head
    }

    /// Build the clone's release binary once, locked, into the target directory beside it.
    fn build_release(&self) {
        let out = Command::new("cargo")
            .args(["build", "--release", "--locked", "--bin", "rigger"])
            .current_dir(self.root())
            .env("CARGO_TARGET_DIR", self.dir.path().join("target"))
            .output()
            .expect("run cargo");
        assert!(
            out.status.success(),
            "the clone's release build failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Advance the clone's code store to its head with `rigger graph build`, leaving the clone's
    /// tree exactly HEAD's, so every read is the clean tree's.
    fn build_graph(&self) {
        let out = self
            .rigger(&["graph", "build"])
            .output()
            .expect("run rigger graph build");
        assert!(
            out.status.success(),
            "rigger graph build failed in the clone:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            git_out(self.root(), &["status", "--porcelain"]),
            "",
            "the clone's tree must stay exactly HEAD's"
        );
    }

    /// Start `dash`, a `rigger dash` with [`DASH`]'s arguments, owned by this scratch from its
    /// spawn, and return the `host:port` it reports serving at.
    fn start_dash(&mut self, mut dash: Command) -> String {
        let child = dash
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn rigger dash");
        let dash = self.dash.insert(rigger::dash::ReapedChild::new(child));
        let mut served = None;
        dash.read_lines_until(Instant::now() + DASH_READY, |line| {
            match line.strip_prefix("http://") {
                Some(at) => {
                    served = Some(at.trim_end_matches('/').to_string());
                    ControlFlow::Break(())
                }
                None => ControlFlow::Continue(()),
            }
        });
        served.expect("the dash never reported the address it serves at")
    }

    /// One timed run of `rigger <args>` in the clone, from its spawn to its exit. The run must
    /// succeed with an answer carrying `label` and each of `shows` as whole lines and at least
    /// `matches` matching lines in its text section.
    fn timed_cli(&self, args: &[&str], label: &str, shows: &[&str], matches: usize) -> Duration {
        let mut cmd = self.rigger(args);
        let start = Instant::now();
        let out = cmd.output().expect("run the release rigger");
        let took = start.elapsed();
        let answer = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "rigger {args:?} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for line in std::iter::once(&label).chain(shows) {
            assert!(
                has_line(&answer, line),
                "rigger {args:?} must answer the line {line:?}; it began:\n{}",
                head(&answer)
            );
        }
        assert!(
            text_matches(&answer) >= matches,
            "rigger {args:?} must count at least {matches} text matches; it counted {}",
            text_matches(&answer)
        );
        took
    }
}

/// One timed `GET path` from the dash at `served`, from the connect to the response's end. The
/// response must be a `200` whose graph label is `label`.
fn timed_get(served: &str, path: &str, label: &str) -> Duration {
    let start = Instant::now();
    let response =
        http_attempt(served, path).unwrap_or_else(|| panic!("the dash answered nothing to {path}"));
    let took = start.elapsed();
    let (headers, _) = response
        .split_once("\r\n\r\n")
        .expect("an answered response has a header block");
    assert_eq!(
        headers.lines().next(),
        Some("HTTP/1.1 200 OK"),
        "GET {path} must succeed"
    );
    assert_eq!(
        header_value(headers, rigger::dash::GRAPH_LABEL_HEADER),
        Some(label),
        "GET {path} must serve the clone's head"
    );
    took
}

#[test]
fn the_read_budgets_hold_on_the_release_binary() {
    if std::env::var(MEASURE_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping the_read_budgets_hold_on_the_release_binary: set {MEASURE_ENV}=1 to clone \
             HEAD, build its release binary and measure the read budgets (see this file's \
             module doc for why it is opt-in)"
        );
        return;
    }
    let mut scratch = Scratch::new();
    let head = scratch.clone_head();
    scratch.build_release();
    scratch.build_graph();
    let label = format!("describes {head}");
    let dash = scratch.rigger(&DASH);
    let served = scratch.start_dash(dash);

    let mut figures = vec![format!(
        "read budgets at {head}, median of {SAMPLES} after one warm-up, ms"
    )];
    let mut misses = Vec::new();
    for row in ROWS {
        let (name, (median, samples)) = match row.read {
            Read::Cli {
                args,
                shows,
                matches,
            } => (
                format!("rigger {}", args.join(" ")),
                median_after_warm_up(|| scratch.timed_cli(args, &label, shows, matches)),
            ),
            Read::Dash { path } => (
                format!("GET {path}"),
                median_after_warm_up(|| timed_get(&served, path, &label)),
            ),
        };
        let samples: Vec<String> = samples.into_iter().map(ms).collect();
        figures.push(format!(
            "{name:<58} {:>7} under {:>3}  samples {}",
            ms(median),
            row.under.as_millis(),
            samples.join(" ")
        ));
        if !within(median, row.under) {
            misses.push(name);
        }
    }
    let table = figures.join("\n");
    println!("{table}");
    assert_eq!(
        misses,
        Vec::<String>::new(),
        "each read's median must be under its budget:\n{table}"
    );
}

#[test]
fn the_median_drops_the_warm_up_and_takes_the_middle_of_five() {
    // (the six times a read takes in turn, the median, the samples)
    let rows: [([u64; 6], u64, [u64; 5]); 2] = [
        ([100, 50, 40, 30, 20, 10], 30, [50, 40, 30, 20, 10]),
        ([7, 3, 9, 1, 5, 2], 3, [3, 9, 1, 5, 2]),
    ];
    for (times, median, samples) in rows {
        let mut script = times.iter().map(|&t| Duration::from_millis(t));
        let measured = median_after_warm_up(|| script.next().expect("at most six runs"));
        assert_eq!(
            measured,
            (
                Duration::from_millis(median),
                samples.map(Duration::from_millis).to_vec()
            ),
            "times {times:?}"
        );
        assert_eq!(script.next(), None, "times {times:?} must all be run");
    }
}

#[test]
fn a_budget_holds_only_strictly_under_its_limit() {
    let budget = Duration::from_millis(50);
    for (median_us, holds) in [(49_999, true), (50_000, false), (50_001, false)] {
        assert_eq!(
            within(Duration::from_micros(median_us), budget),
            holds,
            "a median of {median_us} us against 50 ms"
        );
    }
}

#[test]
fn an_answer_is_checked_by_its_whole_lines_and_its_text_matches() {
    // (answer, line, whether the answer has it)
    let lines = [
        (
            "x\n  node a::b          code-entity\ny",
            "node a::b code-entity",
            true,
        ),
        ("  node a::b_c code-entity", "node a::b code-entity", false),
        (
            "  node a::b code-entity extra",
            "node a::b code-entity",
            false,
        ),
        ("", "node a::b code-entity", false),
    ];
    for (answer, line, has) in lines {
        assert_eq!(has_line(answer, line), has, "{line:?} in {answer:?}");
    }
    // (answer, the text matches it counts)
    let counts = [
        (
            "## text\ntruncated: 50 of 10257 matching lines shown (10207 dropped)\n",
            10257,
        ),
        (
            "## prose\ntruncated: top 10 of 1468 matching sections shown (1458 dropped)\n",
            0,
        ),
        ("## text\nsrc/a.rs:1: new\n", 0),
    ];
    for (answer, n) in counts {
        assert_eq!(text_matches(answer), n, "{answer:?}");
    }
}

#[test]
fn the_scratch_stops_its_dash_and_removes_its_directory_on_every_way_out() {
    let project = common::cli::temp_store_project();
    for panics in [false, true] {
        let mut seen = None;
        let left = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scratch = Scratch::new();
            let mut dash = common::rigger_courier();
            dash.args(DASH).current_dir(project.path());
            let served = scratch.start_dash(dash);
            let pid = scratch.dash.as_ref().map(rigger::dash::ReapedChild::id);
            assert!(
                http_probe(&served, "/").is_some(),
                "the started dash must serve"
            );
            seen = Some((scratch.dir.path().to_path_buf(), pid, served));
            if panics {
                panic!("a measurement fails inside the scratch");
            }
        }));
        assert_eq!(left.is_err(), panics, "panics: {panics}");
        let (dir, pid, served) = seen.expect("the scratch was made");
        let pid = pid.expect("the scratch owns the dash it started");
        assert!(
            !dir.exists(),
            "panics: {panics}: {} must be removed",
            dir.display()
        );
        assert!(
            !common::is_running(pid),
            "panics: {panics}: dash {pid} must be stopped"
        );
        assert_eq!(
            http_attempt(&served, "/"),
            None,
            "panics: {panics}: {served} must not answer"
        );
    }
}
