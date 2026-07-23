//! `rigger dash` - an embedded, read-only observability page over the existing
//! projections (spec 11, unit 2).
//!
//! This module owns ALL of the dash's HTTP serving and rendering. It is a THIN
//! adapter: every number it shows is folded by an existing read-model
//! ([`crate::ledger::project`], [`crate::metrics::project`],
//! [`crate::spawn::step_result`], and the [`crate::contextgraph`] subgraph). There is
//! no new business logic here and, in particular, review verdicts are NOT re-derived -
//! they come straight from [`crate::metrics`]'s classification (there is no verdict
//! event type; it is inferred from `UnitStatus` transitions), so the dash and
//! `rigger stats` can never disagree.
//!
//! Two hard lines the spec draws, enforced structurally:
//!   - **No async runtime.** The HTTP layer is hand-rolled and synchronous over
//!     [`std::net::TcpListener`] (one request at a time, loopback only). The default
//!     build gains no tokio/axum and no new dependency at all.
//!   - **No write/control surface.** [`route`] answers only `GET`; every other method,
//!     on every path, is refused with `405`. The conductor stays the sole mutation
//!     authority - control goes through the CLI, never the dash.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::contextgraph::{Graph, Node, KIND_DECISION, KIND_FINDING, REL_SUPERSEDES};
use crate::eventstore::{Event, Position};
use crate::progress::{self, AgentActivity};
use crate::{blocker, ledger, metrics, spawn};

/// The single-file page, embedded at compile time (vanilla HTML/CSS/JS, no build step).
/// [`STATE_PLACEHOLDER`] is substituted with `null` for live serving (the page polls the
/// JSON endpoints) or with an inlined snapshot for `--export` (a static, shareable file).
const PAGE_TEMPLATE: &str = include_str!("dash.html");

/// The token in [`PAGE_TEMPLATE`] replaced with the embedded state. It sits on the right
/// of a JS assignment, so substituting `null` (live) or a JSON object literal (export)
/// both yield valid JavaScript.
const STATE_PLACEHOLDER: &str = "__RIGGER_STATE__";

/// The default loopback port for `rigger dash` when `--port` is not given.
pub const DEFAULT_PORT: u16 = 7420;

/// The first bindable loopback port at or above `start` (pass [`DEFAULT_PORT`]).
///
/// The always-on dash (spec 19b, unit 1) auto-starts on `DEFAULT_PORT` "or the next free
/// port so concurrent harnesses each get their own": the first harness binds `DEFAULT_PORT`,
/// a second finds it busy and takes the next free port, so two harnesses (e.g. two repos)
/// never fight over one port. Each candidate is bound and immediately released to test it, so
/// the returned port is free at probe time. A concurrent process could still claim it in the
/// narrow window before the dash re-binds, in which case the dash's OWN `bind` fails loudly
/// at startup rather than silently serving nothing - the safe direction (the same ephemeral
/// probe pattern the reaping test's `free_loopback_port` uses). `std`-only, so it is
/// identical on the default and `--no-default-features` lanes.
pub fn free_port_from(start: u16) -> io::Result<u16> {
    for port in start..=u16::MAX {
        if let Ok(listener) = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
            return listener.local_addr().map(|addr| addr.port());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "no free loopback port at or above the requested start port",
    ))
}

/// The per-project record of the run dashboard currently serving a project: the loopback
/// PORT it bound and the PID of its process. The step drive path writes it when it starts a
/// dash and reads it before starting one, so at most one run dashboard serves a project at a
/// time (spec 39, criterion 1: idempotent start on step). It sits alongside the dash-url
/// breadcrumb `rigger status` already reads, and is a plain `port\npid` text record - so it
/// round-trips with no serde and compiles identically in BOTH feature lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashMarker {
    /// The loopback port the recorded dash bound.
    pub port: u16,
    /// The PID of the recorded dash process, used to check whether it is still serving.
    pub pid: u32,
}

impl DashMarker {
    /// Render the marker as its on-disk `port\npid\n` record.
    pub fn serialize(&self) -> String {
        format!("{}\n{}\n", self.port, self.pid)
    }

    /// Parse a marker from its on-disk record, or `None` when it is malformed. A corrupt or
    /// truncated marker reads as "no dash recorded" so the step path starts a fresh dash
    /// rather than trusting garbage - the safe direction (start-if-unsure never suppresses a
    /// real dash).
    pub fn parse(s: &str) -> Option<DashMarker> {
        let mut lines = s.lines();
        let port = lines.next()?.trim().parse().ok()?;
        let pid = lines.next()?.trim().parse().ok()?;
        Some(DashMarker { port, pid })
    }

    /// Read the marker at `path`, or `None` when it is absent, unreadable, or malformed
    /// (each of which means "no dash is recorded as serving here").
    pub fn read(path: &Path) -> Option<DashMarker> {
        Self::parse(&std::fs::read_to_string(path).ok()?)
    }

    /// Write the marker to `path`, overwriting any prior record. Best-effort at the call
    /// site: a failed write only means a later step cannot discover this dash and may start
    /// a second one, never a broken step.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.serialize())
    }
}

/// Whether process `pid` is still alive, Linux-first via `/proc/<pid>` existence
/// (`std`-only - no `libc` - so it holds in BOTH feature lanes, exactly as
/// [`crate::reap`] detects processes). Off a platform without `/proc` the directory is
/// absent, so this reports `false`; the step path treats "not verifiably alive" as "no
/// dash serving" and starts a fresh one rather than suppressing one on an unverifiable
/// marker - the same safe direction [`DashMarker::parse`] takes for a corrupt record.
pub fn pid_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).is_dir()
}

/// The idempotency decision for the step drive path (spec 39, criterion 1): given the
/// per-project [`DashMarker`] recorded on disk (if any) and a predicate reporting whether a
/// recorded dash is STILL serving, returns `true` iff the step must START a run dashboard -
/// i.e. NONE is already serving. A marker naming a still-serving dash short-circuits to
/// `false`, so the second and every later `step` of a run is a no-op, never a second dash
/// or a port fight. `still_serving` is injected so the decision is provable without a real
/// dash process; production passes [`pid_is_alive`] over the marker's pid.
pub fn dash_start_needed(
    marker: Option<DashMarker>,
    still_serving: impl Fn(DashMarker) -> bool,
) -> bool {
    match marker {
        Some(m) => !still_serving(m),
        None => true,
    }
}

/// The self-reap decision for the step-path PERSISTENT dashboard (spec 39, criterion 3):
/// given a snapshot of the run it serves, returns `true` iff the dash should REAP ITSELF now -
/// the run is complete or its liveness has gone stale - so a completed or crashed run leaves no
/// orphaned dash. This is the domain core the detached dash's watcher polls; the watcher owns
/// only the I/O (reading the store, scanning the heartbeat markers, sleeping, and exiting on
/// `true`), so the DECISION is provable here without a real dashboard process or a real run.
///
/// The trigger is LIVENESS, never the pure `done` flag alone: between two `rigger step`
/// processes the log's [`spawn::step_result`] `done` is transiently `true` (the last wave is
/// answered but the conductor has not parked the next one yet), so a continuously-polling
/// watcher that reaped on `done` would kill a live run's dash in an inter-step gap. The run's
/// HEARTBEAT - the freshest liveness-marker mtime, which stays fresh while any worker is alive
/// and only goes absent/stale once the run stops - distinguishes a truly idle run from one merely
/// between steps FOR A BOUNDED RUN. But an UNBOUNDED run (the default scaffold sets no
/// `max_wall_clock`) writes NO liveness marker at all, so its heartbeat is PERMANENTLY `None`; the
/// heartbeat alone cannot then tell a genuinely-complete run from one transiently terminal between
/// waves. So the `None` arm is additionally gated on a UNIT-LEVEL fixpoint, `run_settled`, which
/// only the conductor's integration pass produces - never a bare inter-wave frontier snapshot.
///
/// - `run_started`: the run has produced at least one event (a non-empty current-run slice).
///   Guards a just-started dash on an empty store from reaping before its run has begun - an
///   empty log is vacuously `done`, so without this a fresh dash would reap on its first poll.
/// - `run_terminal`: the run reached a SPAWN-level terminal fixpoint (`terminal_and_no_live_worker`
///   in the binary): every parked spawn answered, no hung spawn, no manual-review pause. This is
///   TRANSIENTLY true in every inter-wave gap (the wave answered, the next not parked yet).
/// - `run_settled`: the run reached a UNIT-level fixpoint - it has at least one unit and EVERY unit
///   is terminal (integrated or escalated). A unit becomes terminal ONLY through the conductor's
///   integration pass (which runs inside `rigger step`), never merely because a worker reported its
///   result, so `run_settled` is FALSE in the transient inter-wave window where results are in but
///   not yet integrated, and false while any later-wave unit is still pending. It is the
///   will-not-advance signal that distinguishes genuine completion from a transiently-terminal
///   snapshot when there is no heartbeat to consult (the unbounded run), and it stays correct for a
///   bounded run too (whose markers the final teardown reclaims, so it also lands on the `None` arm).
/// - `heartbeat_age`: the freshest per-spawn liveness-marker age across the WHOLE run's markers
///   (not just the unanswered wave), or `None` when none is recorded - a run not yet heartbeating,
///   an unbounded run that never heartbeats, or a completed run whose `agent-live` markers the
///   terminal teardown reclaimed.
/// - `stale_after`: the heartbeat-staleness bound; a heartbeat older than this means the run's
///   liveness has gone stale (a crashed or wedged run that never reached a clean fixpoint).
///
/// The two reap arms:
/// - `None` heartbeat: reap only when the run has STARTED, is spawn-level TERMINAL, and is
///   unit-level SETTLED - genuine completion (or an escalation-halt), where every unit reached a
///   terminal state and, for a bounded run, the final step's teardown reclaimed the markers. A
///   `None` heartbeat that is terminal but NOT settled is either a run still coming up (a wave
///   parked whose workers have not touched a marker yet) OR an unbounded run merely between waves
///   (results reported, integration not yet run) - both must keep serving.
/// - `Some(age)` heartbeat: reap once `age > stale_after`. A fresh heartbeat (small age) means a
///   worker is alive - even when the log reads terminal in an inter-step gap - so the dash keeps
///   serving; a stale heartbeat means the run's liveness died, whether it reached a clean
///   fixpoint (markers not yet reclaimed) or wedged, so the dash reaps.
pub fn should_reap_on_idle(
    run_started: bool,
    run_terminal: bool,
    run_settled: bool,
    heartbeat_age: Option<Duration>,
    stale_after: Duration,
) -> bool {
    match heartbeat_age {
        // No heartbeat recorded: reap only when the run has STARTED, the log confirms a spawn-level
        // terminal fixpoint, AND every unit is terminal (the unit-level fixpoint the conductor's
        // integration pass produces). Requiring `run_settled` here is what keeps an UNBOUNDED run's
        // dash serving through inter-wave gaps: with no heartbeat, `run_terminal` alone is
        // transiently true between waves, so without the settled gate the dash would self-reap
        // mid-run. A `None` heartbeat that is terminal-but-not-settled is a run still coming up or
        // merely between waves - keep serving.
        None => run_started && run_terminal && run_settled,
        // A heartbeat is recorded: reap once it has gone stale. A fresh heartbeat means a worker
        // is alive (a live run, even between steps when the log reads terminal), so keep serving.
        Some(age) => age > stale_after,
    }
}

/// What the dash's data provider yields per request: the run's events, its context subgraph,
/// this run's progress reports (spec 14), and each in-flight spawn's liveness-marker age.
/// Factored into a `type` so the provider signature stays readable across the server, its
/// callers, and the tests.
pub type DashInputs = (Vec<Event>, Graph, Vec<Event>, HashMap<String, u64>);

// ---------------------------------------------------------------------------
// View DTOs. These live HERE, not on the projection types: adding `Serialize` to
// `metrics::Metrics` / `ledger::RunState` / `contextgraph::Graph` would make the dash a
// co-owner of modules it only reads. Translating their public fields into these plain
// serde structs keeps the dash a thin adapter and the projections' blast radius clean.
// ---------------------------------------------------------------------------

/// The whole `/api/state` payload: one snapshot of the run, assembled from the four
/// projections. `events` is populated only for `--export` (a static page cannot fetch);
/// the live `/api/state` leaves it absent and the page tails [`events_json`] separately.
#[derive(Debug, Serialize)]
pub struct StateView {
    /// Unix seconds when this snapshot was built (client shows it as the freshness clock).
    pub generated_at: u64,
    /// The highest global event position folded into this snapshot - the cursor a live
    /// client can poll `/api/events?since=` from.
    pub position: Position,
    pub run: RunView,
    pub metrics: MetricsView,
    /// One current-blocker line per unfinished unit, plus the run-level budget halt (spec
    /// 19a, unit 1). Folded by the SHARED [`blocker`] classifier that `rigger status` also
    /// renders, so the two surfaces show the SAME lines. Deterministically ordered (the
    /// run-level budget first, then units lexically).
    pub blockers: Vec<BlockerView>,
    /// The live pending frontier + fixpoint/halt, reused verbatim from
    /// [`spawn::step_result`] (already `Serialize`).
    pub step: spawn::Step,
    /// The live per-agent view (spec 14): for each in-flight spawn, what it is doing now, how
    /// long since its last activity and heartbeat, and its last store milestone - the present
    /// view that fills the milestone-to-milestone blackout. Empty when nothing is in flight or
    /// no progress store was supplied.
    pub activity: Vec<AgentActivity>,
    pub graph: GraphView,
    /// The run-tree SPINE (spec 30 c3): the run projected as
    /// `spec -> unit -> stage -> role -> agent`, with the collapse/expand hints and live
    /// status the page renders. One root per spec (typically one).
    pub tree: Vec<TreeNode>,
    /// Present only in an exported snapshot, so the static page can render its event feed
    /// without a network fetch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<EventView>>,
    /// The ready-to-release handoff (spec 38, criterion 3): present ONLY when the run is done
    /// (every unit integrated, no failed deferred gate), naming the run branch, the
    /// release-target base, the integrated-unit count, and the PR command - so the dash and
    /// `rigger status` surface the SAME handoff from the SAME authority
    /// ([`ledger::RunState::release_ready`]). Absent (`None`) for a run that is not done, so an
    /// unfinished run surfaces no release-ready signal here either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_ready: Option<ledger::ReleaseReady>,
}

/// The ledger projection, flattened for the wire.
#[derive(Debug, Serialize)]
pub struct RunView {
    pub spec_defect: bool,
    pub deferred_gate_failed: bool,
    pub units: Vec<UnitView>,
    /// Unit ids currently awaiting a human (a `ManualReview` with the unit not yet
    /// terminal) - the other half of the action-needed inbox alongside escalations. Read
    /// verbatim from [`ledger::RunState::manual_review`]; the dash does not fold it.
    pub manual_review: Vec<String>,
}

/// One current-blocker line (spec 19a, unit 1), from the shared [`blocker::Blocker`].
/// `line` is the exact one-liner `rigger status` also prints, so the two surfaces cannot
/// drift; `subject` and `kind` are the same value pre-split for the page's table + styling.
#[derive(Debug, Serialize)]
pub struct BlockerView {
    /// The subject: a unit id, or `run` for the run-level budget halt.
    pub subject: String,
    /// A short kind tag for grouping/styling (e.g. `building`, `escalated`, `budget`).
    pub kind: String,
    /// The kind's description, without the subject prefix.
    pub detail: String,
    /// The full shared render (`<subject>: <detail>`) - identical to the `rigger status`
    /// line for the same blocker.
    pub line: String,
}

/// One unit's lifecycle, from [`ledger::Unit`].
#[derive(Debug, Serialize)]
pub struct UnitView {
    pub id: String,
    pub spec_criterion: String,
    pub status: String,
    pub depends_on: Vec<String>,
    pub attempts: u32,
    pub commit: String,
    pub branch: String,
    pub evidence: BTreeMap<String, String>,
}

/// The metrics projection, with the two derived ratios materialized for the client.
#[derive(Debug, Serialize)]
pub struct MetricsView {
    pub units_started: u64,
    pub first_pass_clean: u64,
    pub units_escalated: u64,
    /// Reviews classified as APPROVE by [`metrics::project`] (a `reviewed` transition).
    pub review_approve: u64,
    /// Reviews classified as REJECT by [`metrics::project`] (a loop-back `UnitFailed`).
    pub review_reject: u64,
    pub first_pass_yield: f64,
    pub escalation_rate: f64,
    pub gates: Vec<GateView>,
}

/// One gate's remediation tally (fail is the remediation signal).
#[derive(Debug, Serialize)]
pub struct GateView {
    pub gate: String,
    pub pass: u64,
    pub fail: u64,
    pub total: u64,
}

/// The decisions and findings reachable in the context subgraph around the run.
#[derive(Debug, Serialize)]
pub struct GraphView {
    pub decisions: Vec<DecisionView>,
    pub findings: Vec<FindingView>,
}

/// A decision node; `superseded` is true when a currently-valid `SUPERSEDES` edge points
/// at it (so the page strikes it through), read straight from the context graph rather
/// than re-folding supersession here.
#[derive(Debug, Serialize)]
pub struct DecisionView {
    pub id: String,
    pub summary: String,
    pub superseded: bool,
}

/// A review-finding node from the context graph.
#[derive(Debug, Serialize)]
pub struct FindingView {
    pub id: String,
    pub summary: String,
    pub by: String,
    pub unit: String,
}

/// The `/api/graph` body (spec 30 c5): the seeded neighborhood of a selected node as
/// self-contained JSON - the reachable nodes and the tier-tagged edges among them, plus the `seed`
/// and `depth` the panel echoes. Built by [`neighborhood`] from the graph the dash already
/// projected, so the KG detail panel is a pure read (never a live re-query, never an error).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Neighborhood {
    pub seed: String,
    pub depth: i64,
    pub nodes: Vec<NeighborhoodNode>,
    pub edges: Vec<NeighborhoodEdge>,
    /// The QUERY-PATH between two selected nodes (spec 30 c6): the shortest chain of node ids from
    /// `from` to `to` (inclusive) over the currently-valid edges, filled ONLY when the route is
    /// given both `from=` and `to=`. Empty (and omitted from the JSON) for a plain seed request, so
    /// the panel highlights a path only when the operator has selected two nodes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    /// The PROVENANCE of the SEED node (spec 30 c7): the events/decisions that produced it, as the
    /// currently-valid edges incident to the seed (each stamped with its source event position and
    /// tier). Filled by the route for a seed that resolves to a graph node; absent (omitted) for an
    /// unknown seed / empty graph, so the panel shows provenance only when there is a node to explain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explain: Option<Explanation>,
    /// The DRILL RENDER-BUDGET marker (spec 42 c3): the FULL member count of a drilled cluster whose
    /// membership EXCEEDED [`CLUSTER_RENDER_BUDGET`], so the panel can caption "showing the N
    /// most-connected of M". Set ONLY by [`cluster_detail`] when the cap fired; omitted (`None`) for a
    /// COMPLETE node set - a plain [`neighborhood`] and a drill at/under the budget - so a present
    /// `truncated` unambiguously means "this view is capped to its highest-degree members".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<usize>,
}

/// One node in a seeded KG neighborhood (spec 30 c5). `label` is the node's human-readable handle
/// (its summary / title / name, else its id), so the panel renders it without re-deriving the
/// label, and `kind` lets the panel style it. `degree` and `god` are the c6 GOD-NODE analysis: the
/// node's degree WITHIN the returned neighborhood and whether that makes it a high-degree hub, so
/// the panel flags hubs without re-counting edges.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NeighborhoodNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    /// This node's degree WITHIN the returned neighborhood: the number of returned (currently-valid,
    /// both-endpoints-in-set) edges incident to it. A self-loop counts once. It is the degree of
    /// what the panel actually draws, so a hub only reads as a hub when enough of its neighbors are
    /// in view.
    pub degree: usize,
    /// True when this node is a GOD-NODE (spec 30 c6): its in-neighborhood `degree` is strictly
    /// above [`GOD_NODE_DEGREE_THRESHOLD`], i.e. a high-degree hub the panel flags.
    pub god: bool,
}

/// One TIER-TAGGED edge in a seeded KG neighborhood (spec 30 c5). `tier` is the edge's confidence
/// tier (`extracted` / `inferred` / `ambiguous`) carried verbatim from the graph, so a later
/// criterion can partition edge visibility by tier without the server re-deriving it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NeighborhoodEdge {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub tier: String,
}

/// The PROVENANCE of a node (spec 30 c7): the graph facts that produced it, as a self-contained
/// view DTO over the already-projected neighborhood - so `explain(<node>)` answers "what produced
/// this node" without a second store query. Built by [`explain`] and carried on the `/api/graph`
/// response for the SEED node (the selected node the panel already centers on), so the KG panel
/// shows a node's origin with no new route param.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Explanation {
    /// The explained node's id (echoed so the panel can label the provenance section).
    pub node: String,
    /// The provenance facts: every currently-valid edge incident to the node, each stamped with the
    /// event that folded it. Empty when the node exists but is isolated (no incident edges).
    pub sources: Vec<ProvenanceEdge>,
}

/// One provenance fact (spec 30 c7): a currently-valid edge incident to an explained node, carrying
/// what the edge asserts (`rel` + its endpoints), the confidence `tier` it was folded at, and the
/// `source` event POSITION that produced it - so the operator can trace the node back to the event /
/// decision on the log that wove it into the graph. Read straight off the graph's recorded
/// [`crate::contextgraph::Edge::source`] stamp; `explain` re-derives no fold logic.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ProvenanceEdge {
    pub rel: String,
    pub from: String,
    pub to: String,
    pub tier: String,
    pub source: Position,
}

/// One super-node in the whole-graph clustered overview (spec 42 c2): a [`cluster_key`] bucket the
/// KG panel draws as a single circle instead of its member nodes. `count` is how many graph nodes
/// folded into it (the circle's size) and `kind` is its DOMINANT member kind (the kind the most of
/// its members carry, for the circle's colour). Ties for the dominant kind resolve to the
/// lexicographically-smallest kind, so a given graph yields one stable colour per cluster. Built by
/// [`clustered_overview`] so the overview is a pure read over the already-projected graph.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Cluster {
    /// The cluster's fold key: a module DIRECTORY, the [`CLUSTER_ROOT`] sentinel, or a node KIND -
    /// whatever [`cluster_key`] folded its members into. Also the panel's cluster label.
    pub key: String,
    /// The number of graph nodes that folded into this cluster (its super-node size).
    pub count: usize,
    /// The cluster's DOMINANT member kind (the most common kind among its members; ties broken by the
    /// lexicographically-smallest kind), so the panel colours the super-node without re-counting.
    pub kind: String,
}

/// A weighted, symmetric edge between two DIFFERENT clusters in the overview (spec 42 c2): its
/// `weight` is the number of currently-valid graph edges that cross from one cluster to the other.
/// Directionless - `from` and `to` are canonicalized so `from <= to` by cluster key - so an `a -> b`
/// and a `b -> a` graph edge fold into ONE cluster edge whose weight sums both. Intra-cluster graph
/// edges (both endpoints in one cluster, self-loops included) contribute nothing. Built by
/// [`clustered_overview`], so the panel scales the line thickness by `weight` with no re-derivation.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ClusterEdge {
    /// The lexicographically-smaller endpoint cluster key (the canonical `from <= to` orientation).
    pub from: String,
    /// The lexicographically-larger endpoint cluster key.
    pub to: String,
    /// How many currently-valid graph edges cross between the two clusters (the line's thickness).
    pub weight: usize,
}

/// The whole-graph clustered overview (spec 42 c2): the DEFAULT KG view. Every graph node is folded
/// (by [`cluster_key`]) into a few dozen [`Cluster`] super-nodes and the [`ClusterEdge`]s among them,
/// plus the full node `total` so the panel can say "N nodes in M clusters". Bounded by the module /
/// kind count, never the node count, so it renders at any graph size. Built by [`clustered_overview`]
/// as a pure read over the already-projected graph - it adds no event type and never touches the
/// store; the panel drills a cluster (spec 42 c3) and finally seeds one node's neighborhood (spec 30).
#[derive(Debug, Serialize, PartialEq, Eq, Default)]
pub struct ClusterOverview {
    /// The cluster super-nodes, ordered deterministically by [`Cluster::key`].
    pub clusters: Vec<Cluster>,
    /// The cross-cluster edges, ordered deterministically by `(from, to)` key.
    pub edges: Vec<ClusterEdge>,
    /// The full graph node count (every node, folded or not), so the panel reports the whole size.
    pub total: usize,
}

/// One node in the run-tree SPINE (spec 30 c3): the run projected as
/// `spec -> unit -> stage -> role -> agent`, each node carrying its live status plus the
/// collapse/expand hints the client renders. It is a plain serde DTO built HERE from the
/// existing projections; dash.html renders the tree HTML client-side and `dash.rs` never
/// emits it (the spec-30 render boundary: `dash.rs` ships JSON, the page draws it).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TreeNode {
    /// The node's display label (spec id, unit id, stage name, role, or agent handle).
    pub label: String,
    /// The spine level: `spec` | `unit` | `stage` | `role` | `agent` | `driver`. A `driver`
    /// node is the collapsed courier line for a driver-run step (Gates, Integrate).
    pub kind: String,
    /// The node's live status, rolled up from its subtree (`running` / `done` / `failed`
    /// for the machinery levels; the unit's own live status - `building` / `reviewing` /
    /// `reject-recurrence` / `integrated` / `escalated` / ... - for a unit node).
    pub status: String,
    /// True when this level has exactly one child, so the client renders it collapsed: a
    /// single-child level carries no navigational choice.
    pub auto_collapse: bool,
    /// True when this node lies on the path to a RUNNING leaf (a spawn parked without a
    /// result), so the client auto-expands it and the operator lands on the live work.
    pub auto_expand: bool,
    /// The live courier "doing" line for a RUNNING agent (spec 14's `latest_activity`),
    /// folded onto its tree node so the spine subsumes the old live-agent-activity panel
    /// without losing it. Absent on non-agent nodes and on agents with nothing reported yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doing: Option<String>,
    pub children: Vec<TreeNode>,
}

/// One event on the `/api/events` feed: a generic, per-type-agnostic view (position,
/// type, and a truncated payload) so the feed adapts over the raw log with no
/// event-specific logic.
#[derive(Debug, Serialize)]
pub struct EventView {
    pub position: Position,
    #[serde(rename = "type")]
    pub type_: String,
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Builders: projections -> view DTOs.
// ---------------------------------------------------------------------------

/// Assemble the `/api/state` snapshot from an ordered slice of run events and a
/// pre-fetched context [`Graph`]. Pure and side-effect free, so it is unit-testable
/// against a seeded slice with no socket, store, or repo.
///
/// `include_events` inlines the event feed into the snapshot (for `--export`); the live
/// endpoint passes `false` and serves the feed from [`events_json`] instead.
///
/// `configured_max_retries` is `defaults.max_retries` (the caller's config, unresolved):
/// it sets the `#n/max` bound on a `reject-recurrence` current-blocker line so it matches
/// the depth the run escalates at. `run_branch`/`base` name the release target for the
/// ready-to-release handoff (spec 38, criterion 3), threaded from the serving command.
#[allow(clippy::too_many_arguments)]
pub fn build_state(
    events: &[Event],
    graph: &Graph,
    include_events: bool,
    progress_events: &[Event],
    liveness_ages: &HashMap<String, u64>,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> Result<StateView, serde_json::Error> {
    let run = ledger::project(events)?;
    // The ready-to-release handoff (spec 38, criterion 3): `Some` only on a done run, from the
    // SAME authority `rigger status` reads, so the two surfaces cannot drift. The release-target
    // base is the one PERSISTED on this run's RunStarted (read from the same `events`), so the
    // dash names the base the run actually anchored on - the auto-started dash inherits only the
    // environment and so cannot see the run's `--base` flag. `base` (the serving command's
    // env/default resolution) is the fallback for a run started before base persistence existed.
    let effective_base = crate::run::current_run_base(events).unwrap_or_else(|| base.to_string());
    let release_ready = run.release_ready(run_branch, &effective_base);
    let m = metrics::project(events);
    let step = spawn::step_result(events)?;
    // The live per-agent view, folded from the frontier + this run's progress + the marker
    // ages the caller read. `now` is the wall clock (like `generated_at` below), so the
    // snapshot's activity ages are as of when it was built.
    let activity =
        progress::consolidate(events, progress_events, liveness_ages, SystemTime::now())?;

    let units = run
        .units
        .values()
        .map(|u| UnitView {
            id: u.id.clone(),
            spec_criterion: u.spec_criterion.clone(),
            status: u.status.as_str().to_string(),
            depends_on: u.depends_on.clone(),
            attempts: u.attempts,
            commit: u.commit.clone(),
            branch: u.branch.clone(),
            evidence: u.evidence.clone(),
        })
        .collect();

    let gates = m
        .gates
        .iter()
        .map(|(gate, c)| GateView {
            gate: gate.clone(),
            pass: c.pass,
            fail: c.fail,
            total: c.total(),
        })
        .collect();

    let metrics_view = MetricsView {
        units_started: m.units_started,
        first_pass_clean: m.first_pass_clean,
        units_escalated: m.units_escalated,
        review_approve: m.review_approve,
        review_reject: m.review_reject,
        first_pass_yield: m.first_pass_yield(),
        escalation_rate: m.escalation_rate(),
        gates,
    };

    // The current-blocker lines, from the SHARED classifier `rigger status` also renders
    // (over the same projected run + the budget fold). `from_state` reuses the `run` we
    // already projected above rather than re-projecting. The raw blockers are also the run
    // tree's live-status source, so we classify ONCE and reuse (no second derivation).
    let raw_blockers = blocker::from_state(&run, events, configured_max_retries);
    let blockers = raw_blockers
        .iter()
        .map(|b| BlockerView {
            subject: b.subject().to_string(),
            kind: b.kind_tag().to_string(),
            detail: b.line(),
            line: b.full_line(),
        })
        .collect();

    // The run-tree spine (spec 30 c3): projected from the same `run`, the same live blocker
    // classification, the recorded spawns, and the same live agent activity (folded onto
    // running agents) - a thin adapter, no re-derivation.
    let tree = build_run_tree(events, &run, &raw_blockers, &activity)?;

    let events_view = if include_events {
        Some(events.iter().map(event_view).collect())
    } else {
        None
    };

    Ok(StateView {
        generated_at: now_unix(),
        position: events.iter().map(|e| e.position).max().unwrap_or(0),
        run: RunView {
            spec_defect: run.spec_defect,
            deferred_gate_failed: run.deferred_gate_failed,
            units,
            // Read straight from the ledger projection (folded by `ledger::project`); the
            // dash does not re-derive the inbox, keeping this a thin adapter.
            manual_review: run.manual_review,
        },
        metrics: metrics_view,
        blockers,
        step,
        activity,
        graph: build_graph_view(graph),
        tree,
        events: events_view,
        release_ready,
    })
}

/// Translate a context [`Graph`] into the decisions/findings the page renders. A decision
/// is marked `superseded` when a currently-valid `SUPERSEDES` edge targets it - the graph
/// keeps such edges valid (only the superseded decision's GOVERNS edges are invalidated),
/// so this is a faithful read of the graph's own supersession, not a re-derivation.
fn build_graph_view(graph: &Graph) -> GraphView {
    let superseded: std::collections::BTreeSet<&str> = graph
        .edges
        .iter()
        .filter(|e| e.rel == REL_SUPERSEDES)
        .map(|e| e.to.as_str())
        .collect();

    let mut decisions = Vec::new();
    let mut findings = Vec::new();
    for n in &graph.nodes {
        match n.kind.as_str() {
            KIND_DECISION => decisions.push(DecisionView {
                id: n.id.clone(),
                summary: n.attrs.get("summary").cloned().unwrap_or_default(),
                superseded: superseded.contains(n.id.as_str()),
            }),
            KIND_FINDING => findings.push(FindingView {
                id: n.id.clone(),
                summary: n.attrs.get("summary").cloned().unwrap_or_default(),
                by: n.attrs.get("by").cloned().unwrap_or_default(),
                unit: n.attrs.get("unit").cloned().unwrap_or_default(),
            }),
            _ => {}
        }
    }
    decisions.sort_by(|a, b| a.id.cmp(&b.id));
    findings.sort_by(|a, b| a.id.cmp(&b.id));
    GraphView {
        decisions,
        findings,
    }
}

// ---------------------------------------------------------------------------
// The unified-KG detail panel (spec 30 c5): the `/api/graph` route projects the seeded neighborhood
// of a selected node - the detail view the tree drives (select-to-seed). Pure over the graph the
// dash already fetched: an in-memory walk that mirrors `Projection::subgraph`, so the panel is a
// read-only projection with no live re-query and no error path (an unknown seed / empty graph
// yields an empty neighborhood - the graceful degradation the spec requires).
// ---------------------------------------------------------------------------

/// The default `/api/graph` traversal depth when the request omits `depth`. Two hops matches the
/// run's own subgraph seed depth ([`crate::contextgraph::Projection::subgraph`] as `dash_read_graph`
/// calls it) and `rigger graph --around`, so the panel's default breadth is the same the run grounds
/// on.
pub const DEFAULT_GRAPH_DEPTH: i64 = 2;

/// The upper bound on `/api/graph`'s `depth`, so an over-large (or hostile) `depth=` can never make
/// the in-memory walk churn the whole graph. A neighborhood detail view needs only a few hops; the
/// run itself grounds at depth 2.
pub const MAX_GRAPH_DEPTH: i64 = 6;

/// The GOD-NODE degree threshold (spec 30 c6): a node whose degree WITHIN the returned neighborhood
/// is STRICTLY above this is a high-degree hub the panel flags. A neighborhood detail view is a
/// handful of nodes, so a node wired to more than this many of its in-view neighbors dominates the
/// picture and is worth calling out; a leaf or an ordinary chain node stays well under it.
pub const GOD_NODE_DEGREE_THRESHOLD: usize = 5;

/// The human-readable label of a graph node: its `summary` (a decision / finding), else its `title`
/// (a design-doc / rule), else its `name` (a code entity), else its id. ONE label authority the KG
/// panel and any later consumer read, never a re-invented derivation.
fn node_label(node: &Node) -> String {
    for key in ["summary", "title", "name"] {
        if let Some(v) = node.attrs.get(key) {
            if !v.is_empty() {
                return v.clone();
            }
        }
    }
    node.id.clone()
}

// ---------------------------------------------------------------------------
// The whole-graph exploration fold (spec 42 c1): [`cluster_key`] folds EVERY graph node into one
// super-node bucket, so the KG panel can render a 7k-node graph as a few dozen clusters instead of
// node-for-node. A node whose id NAMES A FILE - a code entity (`<file>::<name>`), a rationale anchor
// (`<file>#L<n>`), or a path id (a file / design-doc whose last segment carries an extension) -
// clusters by that file's DIRECTORY (its module); a directory-less (repo-root) path falls back to
// the `(root)` bucket. Every other node - the dev-loop nodes with NO path id (a decision, finding,
// unit, agent, gate, lesson) - clusters by its KIND. The fold is a pure function of `(id, kind)`, so
// a given graph yields one stable overview (the determinism the spec requires by construction). This
// is the fold KEY only; the overview and drill aggregations (c2, c3) consume it.
// ---------------------------------------------------------------------------

/// The bucket a directory-less (repo-root) path id folds to, since it names a file with no parent
/// module. A `(root)` sentinel - the parentheses keep it from ever colliding with a real directory
/// name - so the overview can name and colour the repo-root cluster like any other.
pub const CLUSTER_ROOT: &str = "(root)";

/// Fold a graph node `(id, kind)` into its exploration super-node bucket (spec 42 c1).
///
/// A node whose id NAMES A FILE clusters by that file's DIRECTORY (its module); a directory-less
/// (repo-root) file falls back to [`CLUSTER_ROOT`]. Every other node clusters by its `kind`. An id
/// names a file after reducing it to a file path: a code entity `<file>::<name>` reduces to the part
/// before the first `::`; a rationale anchor `<file>#L<n>` or a design-doc section `<doc>#<slug>`
/// reduces to the part before the first `#`; a plain path id `<file>` (a file / design-doc) is
/// itself. The reduced path names a file iff its last segment carries an extension. A file path
/// contains neither `::` nor `#`, so those splits leave a plain path untouched, and a dev-loop id (a
/// decision / finding / unit / agent / gate / lesson), whose last segment carries no extension, is
/// never mistaken for one. The fold is a pure, total function of `(id, kind)`, so a given graph folds
/// to one stable set of buckets (the determinism the exploration view relies on).
pub fn cluster_key(id: &str, kind: &str) -> String {
    // Reduce a path-bearing id to the file path it names by stripping a code-entity `::name` suffix,
    // then a rationale / doc-section `#...` suffix. A plain path id survives both untouched.
    let file = id.split_once("::").map_or(id, |(f, _)| f);
    let file = file.split_once('#').map_or(file, |(f, _)| f);

    // The id names a file iff its LAST path segment carries an extension: a `.` with a non-empty
    // stem AND a non-empty suffix (so a dotfile like `.gitignore` - whose only `.` is leading - is
    // NOT treated as an extensioned path, and a dev-loop id like `plan-critique` never is either).
    let last_segment = file.rsplit_once('/').map_or(file, |(_, seg)| seg);
    let names_a_file = last_segment
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty());

    if names_a_file {
        // Cluster by the file's DIRECTORY (its module); a directory-less repo-root file -> `(root)`.
        return match file.rsplit_once('/') {
            Some((dir, _)) if !dir.is_empty() => dir.to_string(),
            _ => CLUSTER_ROOT.to_string(),
        };
    }

    // Every other node is a dev-loop node with no path id: cluster by its KIND.
    kind.to_string()
}

/// Fold the WHOLE graph into its clustered overview (spec 42 c2): the default KG view that renders a
/// ~7k-node graph as a few dozen super-nodes. Every node is folded (by [`cluster_key`]) into a
/// [`Cluster`] carrying its member count and its DOMINANT member kind; every currently-valid edge
/// whose endpoints fall in two DIFFERENT clusters weights a symmetric [`ClusterEdge`]; and `total`
/// carries the full node count. Deterministic by construction (folds over `BTreeMap`s keyed by
/// cluster key / kind, so clusters and edges come out sorted and the dominant-kind tie resolves to
/// the lexicographically-smallest kind). A pure read over the already-projected `graph`: it reads
/// nothing from the store and adds no event type. Bounded by the module / kind count, never the node
/// count, so it renders at any graph size; an empty graph yields an empty overview (zero clusters,
/// zero total), never an error.
pub fn clustered_overview(graph: &Graph) -> ClusterOverview {
    // Fold every node into its cluster bucket, remembering each node's cluster (so edges can be
    // classified) and, per bucket, the member count and a kind histogram (to pick the dominant kind).
    // A `BTreeMap` keeps the buckets - and each bucket's kind histogram - in sorted order, so the
    // overview is deterministic and the dominant-kind tie resolves to the smallest kind by iteration.
    let mut node_cluster: BTreeMap<&str, String> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut kind_hist: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    for n in &graph.nodes {
        let key = cluster_key(&n.id, &n.kind);
        node_cluster.insert(n.id.as_str(), key.clone());
        *counts.entry(key.clone()).or_default() += 1;
        *kind_hist
            .entry(key)
            .or_default()
            .entry(n.kind.as_str())
            .or_default() += 1;
    }

    // Each bucket becomes a Cluster whose kind is its DOMINANT member kind: the kind with the highest
    // member count, ties broken by the lexicographically-smallest kind. The histogram iterates in
    // sorted kind order, so replacing only on a STRICTLY-greater count keeps the first (smallest) kind
    // on a tie - the deterministic colour the spec requires by construction.
    let clusters: Vec<Cluster> = counts
        .into_iter()
        .map(|(key, count)| {
            let hist = kind_hist.remove(&key).unwrap_or_default();
            let mut dominant = "";
            let mut best = 0usize;
            for (kind, c) in &hist {
                if *c > best {
                    best = *c;
                    dominant = kind;
                }
            }
            Cluster {
                key,
                count,
                kind: dominant.to_string(),
            }
        })
        .collect();

    // Every currently-valid edge whose two endpoints are known nodes in DIFFERENT clusters weights a
    // symmetric cluster edge. The endpoint pair is canonicalized (smaller cluster key first) so an
    // `a -> b` and a `b -> a` graph edge fold into one weighted edge; an intra-cluster edge (both
    // endpoints in one cluster, self-loops included) is skipped, and an invalidated edge counts for
    // nothing (matching the currently-valid view the rest of the KG panel reads).
    let mut weights: BTreeMap<(String, String), usize> = BTreeMap::new();
    for e in &graph.edges {
        if e.valid_to.is_some() {
            continue;
        }
        let (Some(a), Some(b)) = (
            node_cluster.get(e.from.as_str()),
            node_cluster.get(e.to.as_str()),
        ) else {
            continue; // an endpoint that is not a known graph node has no cluster to weight
        };
        if a == b {
            continue; // an intra-cluster edge (or self-loop) adds no cross-cluster weight
        }
        let pair = if a <= b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        *weights.entry(pair).or_default() += 1;
    }
    let edges: Vec<ClusterEdge> = weights
        .into_iter()
        .map(|((from, to), weight)| ClusterEdge { from, to, weight })
        .collect();

    ClusterOverview {
        clusters,
        edges,
        total: graph.nodes.len(),
    }
}

/// The RENDER BUDGET a drilled cluster is capped to (spec 42 c3). A cluster with at most this many
/// members renders WHOLE; a bigger one (e.g. a `src` module of 1000+ code entities) is capped to its
/// this-many highest-degree members - the hubs worth seeing - so the library-free SVG panel never
/// tries to draw a thousand nodes. The cap bounds the drill render at ANY graph size, the drill's
/// analogue of the overview folding a whole graph to a few dozen clusters.
pub const CLUSTER_RENDER_BUDGET: usize = 60;

/// Drill a cluster to its members (spec 42 c3): the nodes whose [`cluster_key`] equals `key`, the
/// currently-valid edges AMONG them, each returned node carrying its degree WITHIN the returned set
/// and its god-node flag - reusing spec 30's [`Neighborhood`] shape so the SAME renderer draws it.
///
/// A cluster with at most [`CLUSTER_RENDER_BUDGET`] members renders WHOLE ([`Neighborhood::truncated`]
/// stays `None`). A bigger one keeps only its [`CLUSTER_RENDER_BUDGET`] highest-degree members - the
/// hubs worth seeing - ranked by INTRA-CLUSTER degree (its true connectivity within the fully-known
/// cluster) with an ID tie-break for a pick stable across polls, and sets `truncated = Some(total)` so
/// the panel can caption "showing the N most-connected of M". Every returned edge has BOTH endpoints
/// in the rendered set, so a budget-dropped member never dangles.
///
/// The degree the returned nodes CARRY is the in-view (returned-edge) degree - exactly what
/// [`neighborhood`] reports and what [`NeighborhoodNode::degree`] documents - so the drawn hub is
/// never over-claimed against edges the cap elided (the two coincide at/under budget). Nodes are
/// emitted in ascending-id order for a poll-stable, spiral-seeded layout. An unknown / empty `key`
/// (no node folds to it) yields an empty drill, never an error - the graceful degradation the panel
/// relies on. `seed` echoes the drilled cluster `key` and `depth` is 0 (a cluster is not a
/// hop-bounded walk), so the panel labels the drill and its "<- overview" back link.
pub fn cluster_detail(graph: &Graph, key: &str) -> Neighborhood {
    // The cluster's members: every node folding to `key`, keyed by id for a deterministic, deduped set.
    let members: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| cluster_key(&n.id, &n.kind) == key)
        .map(|n| n.id.as_str())
        .collect();
    let total = members.len();

    // Each member's INTRA-CLUSTER degree: the count of currently-valid edges with BOTH endpoints in
    // the cluster incident to it (a self-loop counts once). This is the FULL cluster connectivity that
    // ranks the hubs when the cluster is over budget; it is computed before any cap.
    let mut cluster_degree: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &graph.edges {
        if e.valid_to.is_none()
            && members.contains(e.from.as_str())
            && members.contains(e.to.as_str())
        {
            *cluster_degree.entry(e.from.as_str()).or_default() += 1;
            if e.to != e.from {
                *cluster_degree.entry(e.to.as_str()).or_default() += 1;
            }
        }
    }

    // Choose the rendered members: WHOLE at/under budget, else the CLUSTER_RENDER_BUDGET highest
    // intra-cluster degree members (ties broken by id ascending, for a pick stable across polls).
    // `truncated` carries the full member count only when the cap fired.
    let (rendered, truncated) = if total <= CLUSTER_RENDER_BUDGET {
        (members.iter().copied().collect::<Vec<&str>>(), None)
    } else {
        let mut ranked: Vec<&str> = members.iter().copied().collect();
        ranked.sort_by(|a, b| {
            let da = cluster_degree.get(*a).copied().unwrap_or(0);
            let db = cluster_degree.get(*b).copied().unwrap_or(0);
            db.cmp(&da).then_with(|| a.cmp(b))
        });
        ranked.truncate(CLUSTER_RENDER_BUDGET);
        (ranked, Some(total))
    };
    let rendered: BTreeSet<&str> = rendered.into_iter().collect();

    // The returned edges: currently-valid, BOTH endpoints in the RENDERED set (a dropped member's
    // edges never dangle). Built FIRST so the in-view degree counts exactly what the panel draws.
    let edges: Vec<NeighborhoodEdge> = graph
        .edges
        .iter()
        .filter(|e| {
            e.valid_to.is_none()
                && rendered.contains(e.from.as_str())
                && rendered.contains(e.to.as_str())
        })
        .map(|e| NeighborhoodEdge {
            from: e.from.clone(),
            to: e.to.clone(),
            rel: e.rel.clone(),
            tier: e.tier.clone(),
        })
        .collect();

    // Each rendered node's degree WITHIN the returned set (the honest in-view degree [`neighborhood`]
    // reports): the count of returned edges incident to it, a self-loop once.
    let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &edges {
        *degree.entry(e.from.as_str()).or_default() += 1;
        if e.to != e.from {
            *degree.entry(e.to.as_str()).or_default() += 1;
        }
    }

    // Emit the rendered nodes in ascending-id order (the `rendered` BTreeSet order) for a poll-stable
    // layout, reusing `node_label` - the one label authority.
    let by_id: BTreeMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let nodes: Vec<NeighborhoodNode> = rendered
        .iter()
        .filter_map(|id| {
            by_id.get(id).map(|n| {
                let d = degree.get(id).copied().unwrap_or(0);
                NeighborhoodNode {
                    id: n.id.clone(),
                    kind: n.kind.clone(),
                    label: node_label(n),
                    degree: d,
                    god: d > GOD_NODE_DEGREE_THRESHOLD,
                }
            })
        })
        .collect();

    Neighborhood {
        // The panel echoes the drilled cluster key (labels the drill + its back link); a cluster is
        // not a hop-bounded walk, so `depth` is 0 and there is no query path or seed provenance.
        seed: key.to_string(),
        depth: 0,
        nodes,
        edges,
        path: Vec::new(),
        explain: None,
        truncated,
    }
}

/// Compute the seeded neighborhood of `seed` WITHIN the already-projected `graph` (spec 30 c5): a
/// breadth-first walk following currently-valid edges in EITHER direction up to `depth` hops,
/// returning the reachable nodes and the TIER-TAGGED edges among them. This mirrors
/// [`crate::contextgraph::Projection::subgraph`]'s traversal (both-direction, valid-only,
/// node-and-edge-in-set) applied to the graph the dash already loaded, so the route stays a pure
/// read over the projected inputs - the panel never re-queries the store. An unknown seed or an
/// empty graph yields an empty neighborhood (never an error), the graceful degradation the spec's
/// KG-feature-off / empty-graph case requires.
pub fn neighborhood(graph: &Graph, seed: &str, depth: i64) -> Neighborhood {
    // Reached-node set (the seed itself is always in it, matching `subgraph`'s CTE seed row), and a
    // BFS frontier of only the nodes newly reached at the previous hop, so `depth` bounds the number
    // of hops exactly as the recursive CTE's `depth < ?` does.
    let mut reached: BTreeSet<String> = BTreeSet::new();
    reached.insert(seed.to_string());
    let mut frontier: Vec<String> = vec![seed.to_string()];
    let mut hops = 0;
    while hops < depth && !frontier.is_empty() {
        let mut next: Vec<String> = Vec::new();
        for e in &graph.edges {
            if e.valid_to.is_some() {
                continue; // an invalidated (superseded) edge is not currently valid
            }
            // Follow the edge in whichever direction touches the frontier: reaching `b` from an
            // edge `b -> a` when `a` is the seed proves the walk is undirected (an agent's blast
            // radius reaches both the decisions it made and the files that reference it).
            for (near, far) in [(&e.from, &e.to), (&e.to, &e.from)] {
                if frontier.iter().any(|f| f == near) && reached.insert(far.clone()) {
                    next.push(far.clone());
                }
            }
        }
        frontier = next;
        hops += 1;
    }

    // The tier-tagged edges of the neighborhood: currently-valid, both endpoints reached. Built
    // FIRST so the GOD-NODE degree is counted over the edges the panel actually draws.
    let edges: Vec<NeighborhoodEdge> = graph
        .edges
        .iter()
        .filter(|e| e.valid_to.is_none() && reached.contains(&e.from) && reached.contains(&e.to))
        .map(|e| NeighborhoodEdge {
            from: e.from.clone(),
            to: e.to.clone(),
            rel: e.rel.clone(),
            tier: e.tier.clone(),
        })
        .collect();

    // Each node's degree WITHIN the returned neighborhood (spec 30 c6 GOD-NODE analysis): the count
    // of returned edges incident to it. Each edge adds one to each distinct endpoint, so a self-loop
    // counts once. A node reads as a hub only when enough of its neighbors are in the returned set,
    // which is the honest degree of what the panel renders (never a global-graph claim that the
    // depth-bounded pre-fetch could not back).
    let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &edges {
        *degree.entry(e.from.as_str()).or_default() += 1;
        if e.to != e.from {
            *degree.entry(e.to.as_str()).or_default() += 1;
        }
    }

    let nodes = graph
        .nodes
        .iter()
        .filter(|n| reached.contains(&n.id))
        .map(|n| {
            let d = degree.get(n.id.as_str()).copied().unwrap_or(0);
            NeighborhoodNode {
                id: n.id.clone(),
                kind: n.kind.clone(),
                label: node_label(n),
                degree: d,
                god: d > GOD_NODE_DEGREE_THRESHOLD,
            }
        })
        .collect();

    Neighborhood {
        seed: seed.to_string(),
        depth,
        nodes,
        edges,
        // A plain seeded neighborhood carries no query path; the route fills it when given `from`/`to`.
        path: Vec::new(),
        // The seed's provenance (spec 30 c7); the route fills it from `explain`, absent by default.
        explain: None,
        // A seeded neighborhood is a COMPLETE node set (never capped); only `cluster_detail` sets this.
        truncated: None,
    }
}

/// Compute the PROVENANCE of `node` (spec 30 c7): the graph facts that produced it - every
/// currently-valid edge incident to the node, each carrying its relation, endpoints, confidence
/// tier, and the source event POSITION that folded it. `explain(<node>)` answers "what produced
/// this node" purely over the already-projected `graph` (the same neighborhood input the rest of
/// the KG panel reads), reusing the graph's recorded [`crate::contextgraph::Edge::source`] stamp
/// rather than re-deriving any fold logic. Returns `None` when `node` is not a graph node (an
/// unknown / absent id explains nothing - the graceful empty the panel degrades to); a superseded
/// (invalidated) edge is not live provenance, matching the currently-valid view [`neighborhood`]
/// and [`path`] present.
pub fn explain(graph: &Graph, node: &str) -> Option<Explanation> {
    if !graph.nodes.iter().any(|n| n.id == node) {
        return None;
    }
    let sources: Vec<ProvenanceEdge> = graph
        .edges
        .iter()
        .filter(|e| e.valid_to.is_none() && (e.from == node || e.to == node))
        .map(|e| ProvenanceEdge {
            rel: e.rel.clone(),
            from: e.from.clone(),
            to: e.to.clone(),
            tier: e.tier.clone(),
            source: e.source,
        })
        .collect();
    Some(Explanation {
        node: node.to_string(),
        sources,
    })
}

/// Compute the QUERY-PATH between two selected nodes (spec 30 c6): the shortest chain of node ids
/// from `from` to `to` (inclusive) over the graph's currently-valid edges, walked in EITHER
/// direction (the same undirected, valid-only traversal [`neighborhood`] uses). A breadth-first
/// search, so the returned chain is a fewest-hops path; ties break by the deterministic edge order.
/// Returns just `[from]` when `from == to` and an EMPTY path when `to` is unreachable or either
/// endpoint is absent, so the panel highlights a path only when one genuinely exists - never an
/// error. Pure over the already-projected `graph`, like the rest of the KG detail panel.
pub fn path(graph: &Graph, from: &str, to: &str) -> Vec<String> {
    // Neither endpoint present -> no path (a selection that is not a node highlights nothing).
    let is_node = |id: &str| graph.nodes.iter().any(|n| n.id == id);
    if !is_node(from) || !is_node(to) {
        return Vec::new();
    }
    if from == to {
        return vec![from.to_string()];
    }
    // BFS over currently-valid edges, both-direction, recording each node's predecessor so the
    // shortest chain can be reconstructed once `to` is dequeued.
    let mut predecessor: BTreeMap<String, String> = BTreeMap::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    visited.insert(from.to_string());
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    queue.push_back(from.to_string());
    while let Some(current) = queue.pop_front() {
        for e in &graph.edges {
            if e.valid_to.is_some() {
                continue; // an invalidated (superseded) edge does not carry the path
            }
            for (near, far) in [(&e.from, &e.to), (&e.to, &e.from)] {
                if near == &current && visited.insert(far.clone()) {
                    predecessor.insert(far.clone(), current.clone());
                    if far == to {
                        // Reconstruct from `to` back to `from`, then reverse to a forward chain.
                        let mut chain = vec![to.to_string()];
                        let mut step = to.to_string();
                        while let Some(prev) = predecessor.get(&step) {
                            chain.push(prev.clone());
                            step = prev.clone();
                        }
                        chain.reverse();
                        return chain;
                    }
                    queue.push_back(far.clone());
                }
            }
        }
    }
    Vec::new()
}

/// The `/api/graph` response body: the seeded [`neighborhood`] as JSON. When BOTH `from` and `to`
/// are given (the operator selected two nodes), the body also carries the QUERY-PATH between them
/// (spec 30 c6); with either absent the path stays empty and is omitted. Pure over the pre-fetched
/// graph; serialization of these plain view DTOs cannot realistically fail, but the `Result` keeps
/// the route's error handling uniform with [`state_json`].
pub fn graph_json(
    graph: &Graph,
    seed: &str,
    depth: i64,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<String, serde_json::Error> {
    let mut n = neighborhood(graph, seed, depth);
    if let (Some(from), Some(to)) = (from, to) {
        n.path = path(graph, from, to);
    }
    // The seed's provenance (spec 30 c7): the events/decisions that produced the selected node,
    // riding the existing response so `explain(<seed>)` needs no new route param. Absent (omitted)
    // when the seed is not a graph node - graceful, never an error.
    n.explain = explain(graph, seed);
    serde_json::to_string(&n)
}

// ---------------------------------------------------------------------------
// The run-tree spine (spec 30 c3): project the run into
// spec -> unit -> stage -> role -> agent, with collapse/expand hints and each node's live
// status. A thin adapter over the ledger projection, the shared blocker classifier, and the
// recorded spawns - it derives nothing those authorities already own.
// ---------------------------------------------------------------------------

/// Project the run into the tree the dash renders as its SPINE. One root per spec (units
/// group by their id's spec prefix); under each unit its present lifecycle stages; under each
/// worker stage the roles; under each role its agents (one per recorded spawn). `Gates` and
/// `Integrate` are run by the stepwise driver itself (no worker agent), so each collapses to a
/// single `driver` line instead of a node per courier step.
///
/// Pure and side-effect free: it reads the already-projected `run`, the already-classified
/// live `blockers` (so a unit's status is the SAME line `rigger status` shows, never
/// re-derived here), and the recorded spawns in `events`. A spawn with no result - or whose
/// LATEST result is a step-synthesized liveness fault (a re-park the driver treats as still
/// hung) - is RUNNING, and the whole path down to it is marked auto-expand; its answered /
/// errored state is read per-spawn from `spawn::result_of` (last-write-wins), never a second
/// fold over the raw event stream.
pub fn build_run_tree(
    events: &[Event],
    run: &ledger::RunState,
    blockers: &[blocker::Blocker],
    activity: &[AgentActivity],
) -> Result<Vec<TreeNode>, serde_json::Error> {
    let spawns = spawn::recorded(events)?;

    // The live courier "doing" line per spawn id (spec 14), folded onto running agents so the
    // tree subsumes the old live-agent-activity panel without losing its signal.
    let doing_by_id: HashMap<&str, &str> = activity
        .iter()
        .filter_map(|a| a.latest_activity.as_deref().map(|d| (a.id.as_str(), d)))
        .collect();

    // Which recorded spawns have finished (answered by a result), and which finished with an
    // error - so an agent leaf reads running / failed / done. Derived PER SPAWN from the typed
    // authority `spawn::result_of` (the SAME last-write-wins the replay driver reads), never a
    // second parallel fold over the raw event stream:
    //   * a hung-then-recovered agent whose LATEST result is a success reads `done`, not the
    //     stale fault (last-write-wins), and
    //   * a step-synthesized LIVENESS fault is a re-park, not an answer - the replay driver
    //     treats a still-hung agent as RUNNING - so it counts as neither answered nor errored
    //     here (no false failure rolled up).
    let mut answered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut errored: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for id in spawns.keys() {
        if let Some(res) = spawn::result_of(events, id)? {
            if res.is_liveness_fault() {
                // Re-parked by the driver: still running, awaiting a real result.
                continue;
            }
            answered.insert(id.clone());
            if res.is_error() {
                errored.insert(id.clone());
            }
        }
    }

    // A unit's live status: reuse the shared blocker classification for in-flight units
    // (building / reviewing / reject-recurrence / ...); terminal units read their ledger
    // status. Classified once by the caller and passed in.
    let blocker_kind: HashMap<&str, &str> = blockers
        .iter()
        .map(|b| (b.subject(), b.kind_tag()))
        .collect();

    // This unit's spawns, found in one pass.
    let mut spawns_by_unit: BTreeMap<&str, Vec<&spawn::SpawnRequest>> = BTreeMap::new();
    for req in spawns.values() {
        spawns_by_unit
            .entry(req.unit.as_str())
            .or_default()
            .push(req);
    }

    // Units grouped by spec (the id prefix) - each spec is a tree root.
    let mut by_spec: BTreeMap<String, Vec<&ledger::Unit>> = BTreeMap::new();
    for u in run.units.values() {
        by_spec.entry(spec_of(&u.id)).or_default().push(u);
    }

    let mut roots = Vec::new();
    for (spec_label, units) in by_spec {
        let mut unit_nodes = Vec::new();
        for u in units {
            let unit_spawns = spawns_by_unit
                .get(u.id.as_str())
                .cloned()
                .unwrap_or_default();
            // The unit's REAL gate outcome, read from the recorded gate verdict (the single
            // gate-outcome authority) rather than inferred from ledger status. `None` means the
            // unit's gates have not run yet.
            let gate_outcome = crate::conductor::recorded_gate_outcome(events, u.id.as_str());
            unit_nodes.push(unit_node(
                u,
                &unit_spawns,
                &answered,
                &errored,
                &blocker_kind,
                &doing_by_id,
                gate_outcome,
            ));
        }
        let auto_expand = unit_nodes.iter().any(|n| n.auto_expand);
        // A terminal FAILURE must surface at the spec root, never be masked as "building" or
        // hidden behind a running sibling: a dead unit (escalated, or a lingering failed) rolls
        // its status up here so the operator sees the failure at the spec level instead of a
        // spec that renders "building" forever.
        let status = if unit_nodes.iter().any(|n| n.status == "escalated") {
            "escalated"
        } else if unit_nodes.iter().any(|n| n.status == "failed") {
            "failed"
        } else if auto_expand {
            "running"
        } else if !unit_nodes.is_empty() && unit_nodes.iter().all(|n| n.status == "integrated") {
            "integrated"
        } else {
            "building"
        };
        roots.push(TreeNode {
            label: spec_label,
            kind: "spec".into(),
            status: status.into(),
            auto_collapse: unit_nodes.len() == 1,
            auto_expand,
            doing: None,
            children: unit_nodes,
        });
    }
    Ok(roots)
}

/// Build one unit node: its present lifecycle stages, in the order a unit walks them.
/// `Implement`/`Review` carry worker roles + agents; `Gates`/`Integrate` collapse to a driver
/// line. A unit's own node carries its live status (the shared blocker classification).
fn unit_node(
    u: &ledger::Unit,
    spawns: &[&spawn::SpawnRequest],
    answered: &std::collections::BTreeSet<String>,
    errored: &std::collections::BTreeSet<String>,
    blocker_kind: &HashMap<&str, &str>,
    doing_by_id: &HashMap<&str, &str>,
    gate_outcome: Option<bool>,
) -> TreeNode {
    let advanced = advanced_past_gates(u.status);

    // Partition this unit's spawns into the implement stage and the review stage by role.
    let mut implement: Vec<&spawn::SpawnRequest> = Vec::new();
    let mut review: Vec<&spawn::SpawnRequest> = Vec::new();
    for req in spawns {
        match stage_of_role(spawn::spawn_role(&req.id)) {
            LifecycleStage::Implement => implement.push(req),
            LifecycleStage::Review => review.push(req),
            LifecycleStage::Other => {}
        }
    }

    let mut stages: Vec<TreeNode> = Vec::new();
    // Implement: present when there is implementer / sdet-author spawn evidence.
    if !implement.is_empty() {
        stages.push(role_stage(
            "Implement",
            &implement,
            answered,
            errored,
            doing_by_id,
        ));
    }
    // Gates: the driver-run local cargo gates. Present ONLY when the unit provably reached the
    // gates - it ADVANCED past them on the linear lifecycle (`advanced`, i.e. green+ reached by
    // PASSING them), OR a gate verdict is RECORDED, OR a SUCCESSFUL implementer finished (a gate
    // can run). The successful-implementer clause excludes a CRASHED implementer (`errored` is a
    // subset of `answered`, so an error result still answers the spawn). Crucially this does NOT
    // present a Gates node for a `Failed` / `Escalated` unit that a numeric rank would alias to
    // green's without passing the gates: a crash-to-exhaustion unit (implementer crashed every attempt,
    // the gate block skipped on `spawn_err`, NO gate ran, NO recorded verdict) is off the linear
    // path (`advanced` false) with no successful implementer and no verdict, so it renders NO phantom
    // Gates line - which, read from a `None` verdict on the aliased rank, would fabricate a `passed`
    // for gates that never ran - and surfaces its failure at Implement. When present, this collapses
    // to one driver line whose status is the unit's REAL gate outcome, read from the RECORDED gate
    // verdict (a gate-failed / escalated unit with a recorded failing verdict renders `failed`; a
    // review-rejected unit whose gates passed renders `passed`, never a fabricated failure).
    if advanced
        || gate_outcome.is_some()
        || implement
            .iter()
            .any(|r| answered.contains(&r.id) && !errored.contains(&r.id))
    {
        stages.push(driver_stage("Gates", gates_status(gate_outcome, advanced)));
    }
    // Review: present when there is a lens / adversary / adjudicator spawn.
    if !review.is_empty() {
        stages.push(role_stage(
            "Review",
            &review,
            answered,
            errored,
            doing_by_id,
        ));
    }
    // Integrate: driver-run (the conductor folds integration - there is no integrator spawn),
    // present once the unit landed. One driver line.
    if matches!(u.status, ledger::Status::Integrated) {
        stages.push(driver_stage("Integrate", "integrated"));
    }

    let auto_expand = stages.iter().any(|s| s.auto_expand);
    TreeNode {
        label: u.id.clone(),
        kind: "unit".into(),
        status: unit_live_status(u, blocker_kind),
        auto_collapse: stages.len() == 1,
        auto_expand,
        doing: None,
        children: stages,
    }
}

/// A worker stage (`Implement` / `Review`): group its spawns by role, each role its agents,
/// deterministically ordered so the render is stable.
fn role_stage(
    label: &str,
    spawns: &[&spawn::SpawnRequest],
    answered: &std::collections::BTreeSet<String>,
    errored: &std::collections::BTreeSet<String>,
    doing_by_id: &HashMap<&str, &str>,
) -> TreeNode {
    let mut by_role: BTreeMap<String, Vec<TreeNode>> = BTreeMap::new();
    for req in spawns {
        let (role_label, agent_label) = role_and_agent(&req.id);
        let status = if !answered.contains(&req.id) {
            "running"
        } else if errored.contains(&req.id) {
            "failed"
        } else {
            "done"
        };
        by_role.entry(role_label).or_default().push(TreeNode {
            label: agent_label,
            kind: "agent".into(),
            status: status.into(),
            auto_collapse: false,
            auto_expand: status == "running",
            // The live courier doing-line, folded onto the agent (subsumes the activity panel).
            doing: doing_by_id.get(req.id.as_str()).map(|d| d.to_string()),
            children: Vec::new(),
        });
    }

    let mut roles: Vec<TreeNode> = by_role
        .into_iter()
        .map(|(role_label, mut agents)| {
            agents.sort_by(|a, b| a.label.cmp(&b.label));
            let auto_expand = agents.iter().any(|a| a.auto_expand);
            let status = rollup(&agents);
            TreeNode {
                label: role_label,
                kind: "role".into(),
                status,
                auto_collapse: agents.len() == 1,
                auto_expand,
                doing: None,
                children: agents,
            }
        })
        .collect();
    roles.sort_by(|a, b| a.label.cmp(&b.label));

    let auto_expand = roles.iter().any(|r| r.auto_expand);
    let status = rollup(&roles);
    TreeNode {
        label: label.into(),
        kind: "stage".into(),
        status,
        auto_collapse: roles.len() == 1,
        auto_expand,
        doing: None,
        children: roles,
    }
}

/// A driver-run stage (`Gates` / `Integrate`): the stepwise driver runs it with no worker
/// agent, so its couriers collapse to a SINGLE `driver` line rather than one node per courier
/// step - the spec-30 "step couriers collapse to a single driver line" behavior.
fn driver_stage(label: &str, driver_status: &str) -> TreeNode {
    let driver = TreeNode {
        label: "driver".into(),
        kind: "driver".into(),
        status: driver_status.into(),
        auto_collapse: false,
        auto_expand: false,
        doing: None,
        children: Vec::new(),
    };
    TreeNode {
        label: label.into(),
        kind: "stage".into(),
        status: driver_status.into(),
        auto_collapse: true,
        auto_expand: false,
        doing: None,
        children: vec![driver],
    }
}

/// Roll a node's status up from its children: running if any descendant runs, else failed if
/// any child failed, else done.
fn rollup(children: &[TreeNode]) -> String {
    if children.iter().any(|c| c.status == "running") {
        "running".into()
    } else if children.iter().any(|c| c.status == "failed") {
        "failed".into()
    } else {
        "done".into()
    }
}

/// Which lifecycle stage a review/implement ROLE belongs to.
enum LifecycleStage {
    Implement,
    Review,
    Other,
}

/// Map a spawn's role token to its lifecycle stage. The implementer and the SDET periphery
/// author write at the build seam (Implement); the lenses, adversary, and adjudicator review
/// (Review). Anything else is not a spine leaf.
fn stage_of_role(role: &str) -> LifecycleStage {
    if role == spawn::ROLE_IMPLEMENTER || role == spawn::ROLE_SDET_AUTHOR {
        LifecycleStage::Implement
    } else if role == spawn::ROLE_ADVERSARY
        || role == spawn::ROLE_ADJUDICATOR
        || role.starts_with("lens:")
    {
        LifecycleStage::Review
    } else {
        LifecycleStage::Other
    }
}

/// The (role-group label, agent label) for a spawn id. A `lens:X` spawn groups under the
/// `lens` role with agent `X` (e.g. sdet / arch); every other role keeps its token and labels
/// the agent by its remediation attempt (`attempt#N`). A Gap-18 reviewer RESPAWN carries a
/// `~retryN` suffix that shares the original's attempt ordinal, so the agent label appends a
/// ` retryN` marker - otherwise a respawn and its original would collapse to the IDENTICAL
/// label (an indistinguishable pair precisely on the remediation path an operator inspects).
fn role_and_agent(id: &str) -> (String, String) {
    let role = spawn::spawn_role(id);
    // The attempt / retry ordinals are read from spawn.rs, the single owner of the spawn-id
    // grammar (it both mints and parses `#{attempt}` / `~retry{n}`), so this view adapter never
    // re-parses the id structure and cannot drift if the separators move with the struct.
    let retry = spawn::retry_of(id);
    if let Some(agent) = role.strip_prefix("lens:") {
        let label = if retry > 0 {
            format!("{agent} retry{retry}")
        } else {
            agent.to_string()
        };
        ("lens".to_string(), label)
    } else {
        let label = if retry > 0 {
            format!("attempt#{} retry{retry}", spawn::attempt_of(id))
        } else {
            format!("attempt#{}", spawn::attempt_of(id))
        };
        (role.to_string(), label)
    }
}

/// The Gates driver line's live outcome for a unit (spec 30 c3), read from the RECORDED gate
/// verdict ([`conductor::recorded_gate_outcome`](crate::conductor::recorded_gate_outcome)), NOT
/// inferred from `ledger::Status`. This is what makes the Gates node - the only driver-run place
/// a gate failure surfaces in the spine - carry the unit's REAL gate outcome:
///
/// - `Some(true)` -> `passed`, `Some(false)` -> `failed`: the recorded verdict is authoritative.
///   A gate FAILURE surfaces here ONLY from a recorded FAILING verdict, so a `red` / escalated
///   unit whose gate ran and failed reads `failed`, while a review-REJECTED unit (`Failed` =
///   reject-recurrence) whose last gate PASSED reads `passed` - the reject is a unit/review-level
///   status surfaced there, never a fabricated gate failure that masks it.
/// - `None` (no recorded verdict): the gates have not produced an outcome, so this can NEVER
///   render `failed` - and never a fabricated `passed` off the linear path. The `advanced` flag
///   ([`advanced_past_gates`]) is TRUE only when the ledger advanced the unit to green or beyond,
///   which it does ONLY after the gates PASS, so `passed` is honest there (gates-ALREADY-CLEARED,
///   e.g. a windowed / pruned slice). It is FALSE for a pre-green between-steps window (implementer
///   answered but no gate has run yet -> `running`) AND for the OFF-LINEAR terminals `Failed` /
///   `Escalated`: those reached green's *rank* by FAILING, not by clearing the gates, so a
///   verdict-less off-linear unit must never read `passed` (that is the fabricate-from-status
///   defect). With `advanced` false, status can only choose `running`, never `passed` or a failure.
fn gates_status(gate_outcome: Option<bool>, advanced: bool) -> &'static str {
    match gate_outcome {
        Some(true) => "passed",
        Some(false) => "failed",
        None if advanced => "passed",
        None => "running",
    }
}

/// True iff the unit ADVANCED along the LINEAR lifecycle to green or beyond - the ledger moves a
/// unit past the gates ONLY after they PASS, so this is the honest "gates ALREADY CLEARED" signal.
/// `Failed` / `Escalated` are OFF the linear path (a mid-remediation reject-recurrence or an
/// exhausted-remediation terminal) and did NOT necessarily run - let alone pass - the gates, so
/// they are EXCLUDED even though a numeric rank would alias them to green's position: a
/// crash-to-exhaustion unit escalates with ZERO gate verdicts, and inferring a gate PASS from its
/// status would fabricate an outcome that never happened. Both the Gates node's PRESENCE and its
/// `None`-verdict outcome key off this predicate, never a rank that conflates the off-linear
/// terminals with a genuine linear advance.
fn advanced_past_gates(s: ledger::Status) -> bool {
    use ledger::Status::*;
    matches!(s, Green | Verified | Reviewed | Integrated)
}

/// The live status a unit node carries: terminal units read their ledger status; in-flight
/// units read the SHARED blocker classification (`building` / `reviewing` /
/// `reject-recurrence` / ...) so the tree and `rigger status` cannot drift.
fn unit_live_status(u: &ledger::Unit, blocker_kind: &HashMap<&str, &str>) -> String {
    match u.status {
        ledger::Status::Integrated => "integrated".to_string(),
        ledger::Status::Escalated => "escalated".to_string(),
        _ => blocker_kind
            .get(u.id.as_str())
            .map(|k| k.to_string())
            .unwrap_or_else(|| u.status.as_str().to_string()),
    }
}

/// The spec bucket a unit id belongs to: strip a leading `u`, take the leading run of ASCII
/// digits, and render `spec <N>` (so `u30-c1` groups under `spec 30`). An id with no leading
/// spec number falls into a single generic `spec` bucket.
fn spec_of(unit_id: &str) -> String {
    let rest = unit_id.strip_prefix('u').unwrap_or(unit_id);
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        "spec".to_string()
    } else {
        format!("spec {digits}")
    }
}

/// The node ids to seed the context subgraph with: every unit, decision, and finding id
/// named in the run's own events. Seeding by the ids the run actually produced (rather
/// than a blast-radius file walk) lets the subgraph return their authoritative nodes and
/// the valid SUPERSEDES edges among them at a shallow depth, independent of whether the
/// run emitted the file-touch edges that would otherwise connect them.
pub fn graph_seeds(events: &[Event]) -> Vec<String> {
    use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_REVIEW_FINDING, TYPE_UNIT_STARTED};
    let mut seeds: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in events {
        let key = match e.type_.as_str() {
            TYPE_DECISION_MADE | TYPE_REVIEW_FINDING => "id",
            TYPE_UNIT_STARTED => "unit",
            _ => continue,
        };
        if let Some(id) = field_str(e, key) {
            if !id.is_empty() {
                seeds.insert(id);
            }
        }
    }
    seeds.into_iter().collect()
}

/// A generic feed view of one event: position, type, and a bounded, per-type-agnostic
/// preview of the payload.
fn event_view(e: &Event) -> EventView {
    let raw = String::from_utf8_lossy(&e.data);
    let mut summary: String = raw.chars().take(160).collect();
    if raw.chars().count() > 160 {
        summary.push_str("...");
    }
    EventView {
        position: e.position,
        type_: e.type_.clone(),
        summary,
    }
}

/// Read a top-level string field from an event's JSON payload (best-effort).
fn field_str(e: &Event, key: &str) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(&e.data)
        .ok()?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// JSON endpoint bodies.
// ---------------------------------------------------------------------------

/// The `/api/state` body: the full projected snapshot as JSON. `progress_events` (this run's
/// slice of the separate progress store) and `liveness_ages` (marker ages the caller read)
/// feed the live per-agent `activity` view; both empty is fine (the view is then empty).
pub fn state_json(
    events: &[Event],
    graph: &Graph,
    progress_events: &[Event],
    liveness_ages: &HashMap<String, u64>,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&build_state(
        events,
        graph,
        false,
        progress_events,
        liveness_ages,
        configured_max_retries,
        run_branch,
        base,
    )?)
}

/// The `/api/events?since=<position>` body: every event whose global position is strictly
/// greater than `since` (the same exclusive convention as `EventStore::read_all`), so a
/// client polls forward from its last-seen cursor. `since = 0` returns the whole feed
/// (positions are 1-based).
pub fn events_json(events: &[Event], since: Position) -> String {
    let feed: Vec<EventView> = events
        .iter()
        .filter(|e| e.position > since)
        .map(event_view)
        .collect();
    // A tiny hand-built object so the endpoint has no dedicated wrapper DTO.
    serde_json::json!({ "events": feed }).to_string()
}

/// The live page: the template with the state placeholder resolved to `null`, so the
/// browser polls the JSON endpoints.
pub fn live_page() -> String {
    PAGE_TEMPLATE.replace(STATE_PLACEHOLDER, "null")
}

/// The `--export` page: the template with the snapshot (including its event feed) inlined,
/// yielding a self-contained static file that renders offline and never fetches.
///
/// The serialized snapshot is neutralized ([`escape_for_script`]) before it is spliced into
/// the `<script>` element, so no string field it carries can break out of that container.
pub fn render_export(
    events: &[Event],
    graph: &Graph,
    progress_events: &[Event],
    liveness_ages: &HashMap<String, u64>,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(&build_state(
        events,
        graph,
        true,
        progress_events,
        liveness_ages,
        configured_max_retries,
        run_branch,
        base,
    )?)?;
    Ok(PAGE_TEMPLATE.replace(STATE_PLACEHOLDER, &escape_for_script(&json)))
}

/// Neutralize a serialized-JSON payload for safe inlining inside an HTML `<script>` element.
///
/// `serde_json` escapes none of `<`, `>`, `&`, so a string field carrying `</script>` - an
/// agent-authored `DecisionMade`/`ReviewFinding` summary, a unit `spec_criterion`, or a raw
/// event payload, all of which flow verbatim into an exported snapshot's inlined feed - would
/// close the script element and inject executing markup into the shared file. Rewriting each to
/// its `\uXXXX` JSON escape - plus the U+2028/U+2029 line separators, which are valid inside a
/// JSON string but terminate a JavaScript statement - keeps the value byte-identical once the
/// browser parses the object literal while making a `</script>` breakout impossible. These five
/// characters only ever occur inside JSON string content (structural JSON uses none of them), so
/// a blanket rewrite of the serialized form stays valid JSON.
fn escape_for_script(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// HTTP: a hand-rolled synchronous response + router. No async runtime, no dependency.
// ---------------------------------------------------------------------------

/// A minimal HTTP response the router returns and the server writes.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    fn html(status: u16, body: String) -> Self {
        Response {
            status,
            content_type: "text/html; charset=utf-8",
            body: body.into_bytes(),
        }
    }
    fn json(status: u16, body: String) -> Self {
        Response {
            status,
            content_type: "application/json",
            body: body.into_bytes(),
        }
    }
    fn text(status: u16, body: &str) -> Self {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            _ => "OK",
        }
    }

    /// Write this response as HTTP/1.1 with `Connection: close`, so a bare client knows
    /// the body ends at the connection close (no keep-alive bookkeeping).
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nConnection: close\r\n\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len(),
        );
        w.write_all(header.as_bytes())?;
        w.write_all(&self.body)?;
        w.flush()
    }
}

/// The single routing authority. Answers only `GET`; every other method - on every path -
/// is a `405`, which is the structural guarantee that the dash exposes NO mutating
/// endpoint. Pure over the projected inputs, so it is unit-testable without a socket.
/// `run_branch`/`base` name the release target for the ready-to-release handoff (spec 38,
/// criterion 3) the `/api/state` body carries on a done run.
#[allow(clippy::too_many_arguments)]
pub fn route(
    method: &str,
    target: &str,
    events: &[Event],
    graph: &Graph,
    progress_events: &[Event],
    liveness_ages: &HashMap<String, u64>,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> Response {
    if method != "GET" {
        return Response::text(
            405,
            "rigger dash is read-only: it serves GET requests only and has no write or \
             control endpoint (the conductor is the sole mutation authority).",
        );
    }
    let path = target.split('?').next().unwrap_or(target);
    match path {
        "/" | "/index.html" => Response::html(200, live_page()),
        "/api/state" => {
            match state_json(
                events,
                graph,
                progress_events,
                liveness_ages,
                configured_max_retries,
                run_branch,
                base,
            ) {
                Ok(body) => Response::json(200, body),
                Err(e) => Response::text(500, &format!("dash: state projection failed: {e}")),
            }
        }
        "/api/events" => {
            let since = query_param(target, "since")
                .and_then(|v| v.parse::<Position>().ok())
                .unwrap_or(0);
            Response::json(200, events_json(events, since))
        }
        // The unified-KG panel: ONE route, THREE views selected by parameter (spec 42 c4, extending
        // the spec 30 c5 seeded panel):
        //   * `cluster=<key>` -> the DRILL: `cluster_detail(key)` as a Neighborhood (the cluster's
        //     members, so the same renderer draws it). The key is a module DIRECTORY (carries `/`) or
        //     a node KIND, `encodeURIComponent`d by the client like a seed id, so it is percent-decoded
        //     back to the exact fold key; an empty / unknown key drills to an empty neighborhood
        //     (graceful), never an error.
        //   * an empty `seed` with no `cluster` -> the DEFAULT view: `clustered_overview`, the
        //     whole-graph fold the panel loads on open.
        //   * a non-empty `seed` -> the spec 30 seeded neighborhood, UNCHANGED. A spec-30 request never
        //     carries `cluster=`, so it always falls through to this branch; the c4 dispatch cannot
        //     regress the seeded panel.
        // The seed branch is verbatim spec 30: `seed` is percent-decoded (the client encodes an id that
        // may carry `#` / `::` / `/`); `depth` defaults to two hops and is clamped so a hostile value
        // cannot make the walk churn; `tier=` is accepted but NOT filtered here (the neighborhood ships
        // every edge TIER-TAGGED and the c7 tier filter partitions visibility CLIENT-side over those
        // tags, per d30-tier-param-ownership); `from=`/`to=` (spec 30 c6) select two nodes whose
        // shortest QUERY-PATH rides the body when BOTH are present; and the body carries the seed's
        // EXPLAIN provenance (spec 30 c7), all built by `graph_json` over the neighborhood.
        "/api/graph" => {
            let body = if let Some(raw_cluster) = query_param(target, "cluster") {
                serde_json::to_string(&cluster_detail(graph, &percent_decode(raw_cluster)))
            } else {
                let seed = query_param(target, "seed")
                    .map(percent_decode)
                    .unwrap_or_default();
                if seed.is_empty() {
                    serde_json::to_string(&clustered_overview(graph))
                } else {
                    let depth = query_param(target, "depth")
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(DEFAULT_GRAPH_DEPTH)
                        .clamp(0, MAX_GRAPH_DEPTH);
                    let from = query_param(target, "from").map(percent_decode);
                    let to = query_param(target, "to").map(percent_decode);
                    graph_json(graph, &seed, depth, from.as_deref(), to.as_deref())
                }
            };
            match body {
                Ok(body) => Response::json(200, body),
                Err(e) => Response::text(500, &format!("dash: graph projection failed: {e}")),
            }
        }
        _ => Response::text(404, "not found"),
    }
}

/// Percent-decode a URL query value (`%XX` -> the byte; every other byte verbatim). The client
/// `encodeURIComponent`s a seed id before putting it on `/api/graph?seed=`, because graph node ids
/// carry `#` (a rationale's `<file>#L<line>`), `::` (a `<file>::<name>` entity), and `/` (a path);
/// the route decodes it back to the exact node id. `+` is NOT treated as a space:
/// `encodeURIComponent` emits `%20` for a space, so a literal `+` in an id round-trips unchanged. An
/// invalid or truncated escape is passed through verbatim, so decoding can never fail and the route
/// stays graceful.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The first value of query parameter `key` in a request target (`/path?a=1&b=2`).
fn query_param<'a>(target: &'a str, key: &str) -> Option<&'a str> {
    let q = target.split_once('?')?.1;
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then_some(v)
    })
}

/// Parse the method and target out of an HTTP request line (`GET /path HTTP/1.1`).
/// Returns `None` for a malformed line, which the server answers with `400`.
fn parse_request_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    Some((method, target))
}

// ---------------------------------------------------------------------------
// The blocking server loop.
// ---------------------------------------------------------------------------

/// Serve the dash on `addr` until the process is stopped, re-reading fresh projection
/// inputs from `provider` on each request (the run advances while the dash watches).
///
/// One connection at a time, synchronously: loopback single-operator traffic needs no
/// concurrency, and a serial loop keeps the sqlite reads and the whole server free of any
/// async runtime. Only the `/api/*` paths consult `provider`; the static page and the
/// method/not-found guards need no store read, so the page still serves before a run has
/// created the store.
pub fn serve<F>(
    addr: SocketAddr,
    provider: F,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> io::Result<()>
where
    F: Fn() -> Result<DashInputs, String>,
{
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    eprintln!("rigger dash: serving on http://{bound}/ (read-only; Ctrl-C to stop)");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_conn(s, &provider, configured_max_retries, run_branch, base)
                {
                    eprintln!("rigger dash: connection error: {e}");
                }
            }
            Err(e) => eprintln!("rigger dash: accept error: {e}"),
        }
    }
    Ok(())
}

/// Read one request, route it, and write the response. Splits the store read from the
/// pure [`route`] so a `provider` failure degrades only the `/api/*` paths (to `500`),
/// never the static page.
fn handle_conn<F>(
    stream: TcpStream,
    provider: &F,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> io::Result<()>
where
    F: Fn() -> Result<DashInputs, String>,
{
    // Bound how long a slow or broken client can hold the single serving slot.
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(()); // client closed before sending anything
    }
    // Drain the remaining request headers (bounded) so the client's write completes before
    // we reply; we route on the request line alone (GET has no body).
    let mut header = String::new();
    while reader.read_line(&mut header)? > 0 {
        if header == "\r\n" || header == "\n" {
            break;
        }
        header.clear();
    }

    let mut stream = reader.into_inner();
    let response = match parse_request_line(request_line.trim_end()) {
        None => Response::text(400, "bad request"),
        Some((method, target)) => {
            let needs_data = method == "GET" && target.starts_with("/api/");
            if needs_data {
                match provider() {
                    Ok((events, graph, progress, liveness)) => route(
                        &method,
                        &target,
                        &events,
                        &graph,
                        &progress,
                        &liveness,
                        configured_max_retries,
                        run_branch,
                        base,
                    ),
                    Err(e) => Response::text(500, &format!("dash: reading the store failed: {e}")),
                }
            } else {
                // The page, 404, and the 405 read-only guard need no projection input.
                route(
                    &method,
                    &target,
                    &[],
                    &Graph::default(),
                    &[],
                    &HashMap::new(),
                    configured_max_retries,
                    run_branch,
                    base,
                )
            }
        }
    };
    response.write_to(&mut stream)
}

/// A supervised handle over a long-lived `rigger` child PROCESS - the auto-started
/// dashboard, and any future `rigger` child a run spawns. When this guard is dropped,
/// the child is KILLED and REAPED, so it can never outlive the run that started it.
///
/// This is the single reaping mechanism the dash and the other `rigger` children rely
/// on (spec 19b, unit 3: no orphaned `rigger` processes). `Drop` runs on BOTH a normal
/// scope exit AND an unwinding panic, so a normally-finishing OR a crashing driver
/// leaves no orphaned `rigger` process reparented to `init`. Reaping is `kill` followed
/// by `wait` (not `kill` alone): the `wait` collects the exited child, so a
/// finished-but-unwaited process leaves no defunct zombie either.
///
/// It is deliberately `std`-only (`std::process::Child`, not a `PR_SET_PDEATHSIG`
/// `prctl`): `libc` is an optional feature-gated dependency, but this guard must compile
/// on BOTH the default and the `--no-default-features` lane, and `std::process` is the
/// only child-lifecycle primitive available on both.
///
/// The two other long-lived children are supervised by the same DISCIPLINE at their own
/// ownership boundary, not through this handle:
///   - the peers side-car ([`crate::sidecar::Sidecar`]) is the IN-PROCESS instance - its
///     own `Drop` stops and joins its collector thread;
///   - `rigger serve` is spawned ONLY by the Node shim over an stdio transport, so the
///     Rust conductor never holds its `Child` to wrap in a Rust guard. Its
///     kill-on-parent-exit is STRUCTURAL: [`crate::mcpserver::Server::run`] serves only
///     until the input closes (the shim's stdin), and the OS closes that pipe whenever
///     the shim dies - a clean exit, a thrown error, or an uncatchable signal alike - so
///     an orphaned `rigger serve` sees EOF on stdin and exits on its own.
pub struct ReapedChild {
    child: std::process::Child,
}

impl ReapedChild {
    /// Take ownership of an already-spawned child so it is reaped when this guard drops.
    /// The caller owns spawning (dependency injection); this guard owns only its death.
    pub fn new(child: std::process::Child) -> Self {
        ReapedChild { child }
    }

    /// The supervised child's OS process id (e.g. to log the serving dash, or surface it
    /// in `rigger status`).
    pub fn id(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        // If the child already exited, `try_wait` has reaped the zombie and there is
        // nothing to kill. Otherwise kill it and `wait` to collect it. Every call is
        // best-effort - a reaper whose child is already gone must never panic in `drop`.
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
}

#[cfg(test)]
mod supervised_lifecycle {
    //! Spec 19b, unit 3: the reaper mechanism reaps every long-lived `rigger` child
    //! after its guard is dropped / the driver exits, so a normally-finishing OR
    //! crashing agent leaves no orphaned `rigger` process. The standalone-`rigger dash`
    //! proof the criterion names lives in `tests/cli.rs` (it needs the compiled binary);
    //! these hermetic tests prove the SAME [`ReapedChild`] discipline generically - on a
    //! stand-in child on the CRASH path, and on the always-present in-process child, the
    //! peers [`crate::sidecar::Sidecar`].
    use super::ReapedChild;
    use std::time::Duration;

    /// A real long-lived child that would outlive the test unless it is reaped. Its
    /// stdout is piped and never written to, so a reader on it blocks until the child
    /// EXITS (the child's write end closes -> EOF). That is a std-only, race-free "is
    /// it still alive?" probe that needs no `libc` (unavailable in the light lane).
    fn spawn_blocking_child() -> (std::process::Child, std::process::ChildStdout) {
        use std::process::{Command, Stdio};
        let mut child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn a long-lived child");
        let out = child.stdout.take().expect("child stdout is piped");
        (child, out)
    }

    /// Watch a child's piped stdout on a helper thread: a `recv` that BLOCKS means the
    /// child is still alive (its write end is open); a `recv` that yields `0` means the
    /// child exited and its stdout reached EOF - i.e. it was reaped.
    fn watch_for_exit(mut out: std::process::ChildStdout) -> std::sync::mpsc::Receiver<usize> {
        use std::io::Read;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            let n = out.read(&mut buf).unwrap_or(0);
            let _ = tx.send(n);
        });
        rx
    }

    #[test]
    fn reaped_child_reaps_even_when_the_driver_panics() {
        let (child, out) = spawn_blocking_child();
        let exited = watch_for_exit(out);

        // A CRASHING driver (a panicking agent) still unwinds through the guard's Drop,
        // so the child is reaped on the crash path exactly as on the clean path.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ReapedChild::new(child);
            panic!("the driving agent crashed");
        }));
        assert!(panicked.is_err(), "the closure was expected to panic");

        let n = exited
            .recv_timeout(Duration::from_secs(5))
            .expect("a panic-unwound ReapedChild did not reap its process");
        assert_eq!(n, 0, "a reaped child's stdout should be at EOF");
    }

    #[test]
    fn dropping_the_peers_sidecar_reaps_its_collector_thread() {
        use crate::eventstore::sqlite::Store;
        use crate::eventstore::Filter;
        use crate::sidecar::Sidecar;
        use std::sync::mpsc;

        let store = Store::open(":memory:").unwrap();
        let sidecar = Sidecar::start(&store, 0, Filter::default()).unwrap();

        // The peers side-car is the in-process instance of the supervised lifecycle: its
        // Drop sets the stop flag and JOINS the collector thread. Prove the join returns
        // (the thread saw stop and ended) rather than leaking - drop on a helper thread
        // and require it to complete within a bound. A leaked collector would hang the
        // join forever and the recv would time out.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            drop(sidecar);
            let _ = tx.send(());
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "dropping the peers side-car did not reap (join) its collector thread"
        );
        drop(store);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contextgraph::{
        Edge, Node, KIND_AGENT, KIND_CODE_ENTITY, KIND_DESIGN_DOC, KIND_FILE, KIND_RATIONALE,
        KIND_UNIT, REL_DECIDED, REL_GOVERNS, REL_REFERENCES, TIER_AMBIGUOUS, TIER_EXTRACTED,
        TIER_INFERRED,
    };
    use crate::eventstore::Event;

    fn ev(type_: &str, json: &str) -> Event {
        Event::new(type_, json.as_bytes().to_vec())
    }

    /// Give a slice of events 1-based positions, as the store would on append, so
    /// position-sensitive reads (`/api/events?since=`) are exercised realistically.
    fn positioned(mut events: Vec<Event>) -> Vec<Event> {
        for (i, e) in events.iter_mut().enumerate() {
            e.position = (i + 1) as Position;
        }
        events
    }

    fn seeded_run() -> Vec<Event> {
        positioned(vec![
            ev(
                "UnitStarted",
                r#"{"id":"u1","spec_criterion":"do the thing"}"#,
            ),
            ev("UnitStatus", r#"{"id":"u1","status":"green"}"#),
            ev("GateVerdict", r#"{"gate":"cargo test","pass":true}"#),
            ev("GateVerdict", r#"{"gate":"cargo test","pass":false}"#),
            ev("UnitStatus", r#"{"id":"u1","status":"reviewed"}"#),
            ev("UnitIntegrated", r#"{"id":"u1","commit":"abc123"}"#),
        ])
    }

    #[test]
    fn root_serves_the_embedded_page_with_the_placeholder_resolved() {
        let r = route(
            "GET",
            "/",
            &[],
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "text/html; charset=utf-8");
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("rigger dash"), "serves the page");
        assert!(
            !body.contains(STATE_PLACEHOLDER),
            "the live page must resolve the state placeholder (to null), not leak the token"
        );
        assert!(
            body.contains("EMBEDDED_STATE = null"),
            "live serving inlines a null state so the page polls"
        );
    }

    /// [`gates_status`] chooses the Gates driver line's outcome from the RECORDED verdict, and on a
    /// `None` verdict it decides passed-vs-running from `advanced` ALONE - it must NEVER fabricate a
    /// `passed` for an off-linear terminal that only *aliases* to green's rank. A recorded verdict
    /// is authoritative (`Some(true)`->passed, `Some(false)`->failed) regardless of `advanced`; a
    /// `None` verdict reads `passed` ONLY when the unit genuinely advanced past the gates (green+),
    /// and `running` otherwise - so a `Failed` / `Escalated` unit with no verdict (`advanced` false)
    /// reads `running`, never a phantom `passed`. Dropping the `advanced` guard on the `None` arm
    /// (rendering `None`=>passed unconditionally) reddens the off-linear case below.
    #[test]
    fn gates_status_never_fabricates_passed_for_an_off_linear_unverdicted_unit() {
        // A recorded verdict is authoritative, independent of the lifecycle position.
        assert_eq!(gates_status(Some(true), false), "passed");
        assert_eq!(gates_status(Some(false), true), "failed");
        assert_eq!(gates_status(Some(false), false), "failed");

        // No verdict + genuinely advanced (green+, gates cleared by passing) => passed.
        assert_eq!(
            gates_status(None, true),
            "passed",
            "a gates-cleared (advanced) unit with no recorded verdict reads passed"
        );
        // No verdict + NOT advanced (pre-gate window, OR an off-linear Failed/Escalated whose rank
        // merely aliases to green) => running, NEVER a fabricated passed.
        assert_eq!(
            gates_status(None, false),
            "running",
            "no verdict off the linear-advance path reads running, never a phantom passed"
        );
        assert_ne!(
            gates_status(None, false),
            "passed",
            "an off-linear (Failed/Escalated) unit with no recorded verdict must never read Gates:passed"
        );

        // `advanced_past_gates` is TRUE only for the linear-advance ranks, FALSE for the off-linear
        // terminals a numeric rank would alias to green - the exact conflation the fix removes.
        for s in [
            ledger::Status::Green,
            ledger::Status::Verified,
            ledger::Status::Reviewed,
            ledger::Status::Integrated,
        ] {
            assert!(
                advanced_past_gates(s),
                "{s:?} is on the linear-advance path"
            );
        }
        for s in [
            ledger::Status::Pending,
            ledger::Status::Grounding,
            ledger::Status::Red,
            ledger::Status::Failed,
            ledger::Status::Escalated,
        ] {
            assert!(
                !advanced_past_gates(s),
                "{s:?} did not clear the gates by advancing, so it must not alias to green"
            );
        }
    }

    /// Spec 19b, unit 1 (always-on dash, "on `DEFAULT_PORT` or the next free port so
    /// concurrent harnesses each get their own"): the port selector returns the requested
    /// start port when it is free, and SKIPS to the next free port when it is taken - so a
    /// second harness auto-starting its dash never collides with the first's.
    #[test]
    fn free_port_from_returns_the_start_port_when_free_and_the_next_free_one_when_it_is_taken() {
        // Free: the requested start port is chosen as-is (a lone harness gets DEFAULT_PORT).
        // An ephemeral high port stands in for DEFAULT_PORT so the test never fights a real
        // dash. The retry loop absorbs the rare window where a PARALLEL test grabs the
        // just-released probe port between finding it free and calling free_port_from - it is
        // the CONTRACT (pick the requested port when free), not the OS scheduler, under test.
        let mut chose_start = false;
        for _ in 0..25 {
            let start = TcpListener::bind(("127.0.0.1", 0))
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            // The probe listener is dropped, so `start` is free again for free_port_from.
            if free_port_from(start).ok() == Some(start) {
                chose_start = true;
                break;
            }
        }
        assert!(
            chose_start,
            "a free start port must be returned unchanged (a lone harness gets DEFAULT_PORT)"
        );

        // Taken: HOLD an ephemeral port (a first harness's dash), then ask for a dash starting
        // at that same port - it must SKIP the held port for a strictly higher free one, so
        // two concurrent harnesses never collide on one port. Robust because we hold the port
        // ourselves, so free_port_from can never return it.
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let taken = held.local_addr().unwrap().port();
        let next = free_port_from(taken).unwrap();
        assert!(
            next > taken,
            "a busy start port is skipped for the next free one; got {next} for start {taken}"
        );
        drop(held);
    }

    /// Spec 19b c2 (responsive redesign): the page BODY must never scroll horizontally at
    /// narrow OR wide widths, and the decision history must wrap long text instead of pushing
    /// the body wide. Visual responsiveness is outside the gate set (rule 4), so this is a
    /// STRUCTURAL guard on the CSS mechanisms that deliver that behavior - it pins them so a
    /// later edit cannot silently reintroduce the `1fr` = `minmax(auto,1fr)` blowout, drop the
    /// `min-width:0` that lets grid children shrink, remove the body backstop, or un-wrap the
    /// decision cells. The adjudicator still demands the changed CSS/markup + a narrow/wide
    /// behavior description; this test guarantees they cannot regress unnoticed.
    #[test]
    fn the_page_layout_cannot_scroll_the_body_horizontally() {
        let page = live_page();

        // Grid tracks use `minmax(0, 1fr)`, never a bare `1fr`, so a wide child cannot force
        // the track (and thus the body) past the viewport - `1fr` alone is `minmax(auto, 1fr)`
        // whose `auto` minimum is the child's max-content.
        assert!(
            page.contains("minmax(0, 1fr)"),
            "grid columns must be minmax(0, 1fr) so a track can shrink below its content"
        );
        assert!(
            !page.contains("grid-template-columns: 1fr 1fr"),
            "the bare `1fr 1fr` blowout track must be gone (replaced by minmax(0, 1fr) pairs)"
        );

        // Grid children (the cards, the view sections) get `min-width: 0` so they honor the
        // shrinkable track instead of refusing to go below their content's min-content width.
        assert!(
            page.contains("min-width: 0"),
            "grid children need min-width: 0 to actually shrink into the minmax(0, 1fr) track"
        );

        // The body carries an overflow backstop (the one-screen shell hides page overflow entirely),
        // so a stray wide child is clipped, never turned into a body-level scrollbar.
        assert!(
            page.contains("overflow: hidden") || page.contains("overflow-x: hidden"),
            "the body needs an overflow backstop so it can never scroll"
        );

        // The decision history wraps long decision/finding text instead of scrolling it far
        // right - rendered as wrapped rows, breaking even an unbreakable token.
        assert!(
            page.contains("overflow-wrap: anywhere"),
            "decision/finding text must wrap (overflow-wrap: anywhere), not scroll horizontally"
        );
    }

    /// Return the CSS declaration block for `selector` (from the selector to its closing `}`),
    /// so an assertion can bind to one rule instead of the whole page. Panics if the selector is
    /// absent, which is itself a meaningful failure (the rule must exist to be checked).
    fn css_rule<'a>(page: &'a str, selector: &'a str) -> &'a str {
        let start = page
            .find(selector)
            .unwrap_or_else(|| panic!("CSS selector {selector:?} not found in the page"));
        let end = page[start..]
            .find('}')
            .map(|i| start + i + 1)
            .unwrap_or(page.len());
        &page[start..end]
    }

    /// Spec 30 c1, revised to the ONE-SCREEN dashboard: the page fits exactly one viewport with NO
    /// page scroll. The body is a full-height flex column (`height: 100vh` + `overflow: hidden`),
    /// the KG holds the top ~half for graph exploration, and the two columns hold the remaining half
    /// and scroll INTERNALLY so a content-heavy panel never overflows the page. `main` keeps no fixed
    /// `max-width`. Visual layout is outside the gate set (rule 4), so this is a STRUCTURAL guard on
    /// the CSS mechanisms that deliver the one-screen fit, pinning them so a later edit cannot re-cap
    /// the shell, let the page scroll, or drop the columns' internal scroll. It binds to specific
    /// rules so it cannot be satisfied by some other block.
    #[test]
    fn the_dashboard_fits_one_screen_with_internal_scroll() {
        let page = live_page();
        let main_rule = css_rule(&page, "main {");
        let body_rule = css_rule(&page, "body {");

        // No fixed max-width cap on the content region: it fills the whole viewport.
        assert!(
            !main_rule.contains("max-width"),
            "the content region (main) must not re-cap its width: {main_rule}"
        );

        // The body is a full-height flex column that never scrolls the page - the one-screen shell.
        assert!(
            body_rule.contains("height: 100vh")
                && body_rule.contains("flex-direction: column")
                && body_rule.contains("overflow: hidden"),
            "the body must be a full-height flex column with overflow: hidden (one screen, no page scroll): {body_rule}"
        );

        // main fills the remaining height as a flex column and hides its own overflow, so its
        // children (the KG and the columns) partition the viewport instead of overflowing the page.
        assert!(
            main_rule.contains("flex-direction: column") && main_rule.contains("overflow: hidden"),
            "main must be a flex column with overflow: hidden so its children partition the viewport: {main_rule}"
        );

        // The KG reserves ~half the viewport height for graph exploration.
        let kg_rule = css_rule(&page, "#kg {");
        assert!(
            kg_rule.contains("48%") || kg_rule.contains("50%"),
            "#kg must reserve ~half the viewport height (flex-basis ~48-50%): {kg_rule}"
        );

        // The columns scroll INTERNALLY so the bottom half contains its content without page scroll.
        let col_rule = css_rule(&page, ".columns > .col {");
        assert!(
            col_rule.contains("overflow-y: auto"),
            "the columns must scroll internally (overflow-y: auto) so the page never scrolls: {col_rule}"
        );

        // Narrow screens drop the fixed one-screen layout and allow normal page scroll.
        assert!(
            page.contains("body { height: auto; overflow: auto; }"),
            "a narrow-screen media query must let the body scroll normally when the one-screen layout won't fit"
        );
    }

    /// Spec 30 c2 (CELLS FIT OR WRAP): id and long-text table cells must SIZE-TO-CONTENT or
    /// WRAP at their hyphen/slash break opportunities - never one char per line, never forcing a
    /// page-level horizontal scrollbar - and the genuinely-wide cells (the event-feed JSON and
    /// the agent doing-line) must live inside an in-cell `overflow-x:auto` scroll/wrap container
    /// so any residual width scrolls INSIDE the cell, never the page body. Visual layout is
    /// outside the gate set (rule 4), so this is a STRUCTURAL guard on the CSS mechanisms that
    /// deliver fit-or-wrap: it pins them so a later edit cannot silently re-`nowrap` the cells
    /// (reintroducing the char-by-char / body-scroll blowout) or drop the in-cell scroll
    /// container. This criterion OWNS cell fit/wrap; the shell (`main {}`) is criterion 1's, so
    /// the test binds the CELL-level CSS rules (`th, td` / `.scroll` / `.feed`), not the markup
    /// ids the concurrent tree/panel units restructure.
    #[test]
    fn cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container() {
        let page = live_page();

        // (a) id + long-text cells wrap / size-to-content: the default table cell must NOT pin
        // `white-space: nowrap` (which keeps a long id on one line and forces the table - and,
        // without containment, the body - wide) and it carries `overflow-wrap` so a long id
        // breaks at its hyphen/slash opportunities and even a token with no break opportunity
        // breaks INSIDE the cell rather than rendering one char per line.
        let cell_rule = css_rule(&page, "th, td {");
        assert!(
            !cell_rule.contains("nowrap"),
            "table cells must not be white-space:nowrap or a long id cannot wrap at its hyphens: {cell_rule}"
        );
        assert!(
            cell_rule.contains("overflow-wrap"),
            "table cells need overflow-wrap so an unbreakable id breaks inside the cell, not char-by-char: {cell_rule}"
        );

        // (b) the wide cells scroll INSIDE their cell: `.scroll` is the in-cell overflow-x:auto
        // container the wide tables (the agent doing-line, the event/dag tables) render into, so
        // a genuinely-wide row scrolls within its card and never drags the page body horizontally.
        let scroll_rule = css_rule(&page, ".scroll {");
        assert!(
            scroll_rule.contains("overflow-x: auto"),
            "the in-cell wide-cell container (.scroll) must be overflow-x: auto: {scroll_rule}"
        );

        // (b) the event-feed cell (the widest, raw event JSON) is its OWN overflow container, so a
        // long JSON summary stays inside the feed panel instead of widening the body.
        let feed_rule = css_rule(&page, ".feed {");
        assert!(
            feed_rule.contains("overflow"),
            "the event feed (event JSON) must be its own overflow container so it stays in-cell: {feed_rule}"
        );
    }

    /// Spec 30 c4 (DECISION PREVIEW/EXPAND): the decision history must render as PROGRESSIVE
    /// DISCLOSURE - each decision a native `<details>` whose `<summary>` previews `id + a
    /// one-line summary` and whose expandable body carries the FULL reasoning, so a multi-KB
    /// decision never dumps inline (the dash charter: no framework, no inline multi-KB dumps).
    /// Interactive expand/collapse is a browser behavior outside the gate set (rule 4), so this
    /// is a STRUCTURAL guard on the render mechanisms that deliver it: it binds to the decisions
    /// render region (`el("decisions")` .. the empty-state sentinel) so it cannot be satisfied by
    /// a `<details>` some other panel emits, and it pins that the old flat `<table>` dump is gone,
    /// the `<summary>` carries `id + preview(summary)`, the body carries the full `summary`, the
    /// `preview()` helper collapses to ONE line, and superseded entries stay struck. This
    /// criterion OWNS progressive disclosure; the tree section is criterion 3's, so the test does
    /// NOT touch the tree render.
    #[test]
    fn the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body() {
        let page = live_page();

        // Bind to the decisions render region: from the `el("decisions")` assignment to its
        // empty-state sentinel, so a `<details>` another panel emits cannot satisfy the guard.
        let start = page
            .find("el(\"decisions\")")
            .expect("the decisions render region must exist");
        let end = page[start..]
            .find("no decisions recorded")
            .map(|i| start + i)
            .expect("the decisions render must keep its empty-state sentinel");
        let region = &page[start..end];

        // Native progressive disclosure: each decision is a `<details>` with a `<summary>` line -
        // NOT the old flat `<table>` that dumped every (possibly multi-KB) summary inline.
        assert!(
            region.contains("<details"),
            "each decision must render as a native <details> element: {region}"
        );
        assert!(
            region.contains("<summary>"),
            "each decision's <details> needs a one-line <summary> preview: {region}"
        );
        assert!(
            !region.contains("<table"),
            "the decisions must no longer render as a flat <table> dump: {region}"
        );

        // The `<summary>` previews id + a ONE-LINE summary; the expandable body carries the FULL
        // reasoning. Both the id and the truncated preview feed the summary line, and the full
        // `summary` text feeds the body, so a long decision collapses to one line but expands whole.
        assert!(
            region.contains("esc(d.id)"),
            "the summary line must show the decision id: {region}"
        );
        assert!(
            region.contains("preview(d.summary)"),
            "the summary line must show a one-line preview of the decision summary: {region}"
        );
        assert!(
            region.contains("esc(d.summary)"),
            "the expandable body must carry the full decision reasoning (esc(d.summary)): {region}"
        );
        // Superseded decisions stay visually struck through in the collapsed line.
        assert!(
            region.contains("d.superseded"),
            "superseded decisions must still be distinguished (struck): {region}"
        );

        // The `preview()` helper collapses the summary to a SINGLE line (whitespace runs collapsed)
        // and truncates it with an ellipsis, so the always-visible line is never a multi-KB dump.
        let p = page
            .find("function preview(")
            .expect("a preview() helper must collapse a summary to one line");
        let body = &page[p..(p + 320).min(page.len())];
        assert!(
            body.contains("replace(/\\s+/"),
            "preview() must collapse whitespace runs so the preview is one line: {body}"
        );
        assert!(
            body.contains(".slice(") && body.contains("..."),
            "preview() must truncate a long summary with an ellipsis: {body}"
        );
    }

    #[test]
    fn state_endpoint_projects_the_seeded_run() {
        let events = seeded_run();
        let r = route(
            "GET",
            "/api/state",
            &events,
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "application/json");
        let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();

        assert_eq!(v["run"]["units"][0]["id"], "u1");
        assert_eq!(v["run"]["units"][0]["status"], "integrated");
        // Metrics folds are present and reflect the seeded gate verdicts.
        assert_eq!(v["metrics"]["units_started"], 1);
        let gates = v["metrics"]["gates"].as_array().unwrap();
        assert_eq!(gates[0]["gate"], "cargo test");
        assert_eq!(gates[0]["pass"], 1);
        assert_eq!(gates[0]["fail"], 1);
        // The live /api/state does not inline the event feed (the page tails it separately).
        assert!(v.get("events").is_none() || v["events"].is_null());
    }

    #[test]
    fn state_carries_the_live_agent_activity() {
        // spec 14, unit 4: the present view carries each in-flight agent's live activity +
        // ages, folded by the consolidator from the frontier + this run's progress + the
        // marker ages the caller read, and it appears in the /api/state body the page consumes.
        use crate::spawn::SpawnRequest;
        let req = SpawnRequest::new("u", "u", "implementer", 0, "do it");
        // A run: a unit started, its implementer parked (in-flight, no result).
        let events = positioned(vec![
            ev("UnitStarted", r#"{"id":"u"}"#),
            req.to_event().unwrap(),
        ]);
        // A recent progress report (small age) + a known marker age.
        let ap = progress::AgentProgress {
            id: req.id.clone(),
            activity: "grep #12: conductor.rs".into(),
        };
        let mut prog = Event::new(
            progress::TYPE_AGENT_PROGRESS,
            serde_json::to_vec(&ap).unwrap(),
        );
        prog.recorded_at = SystemTime::now();
        let progress_events = vec![prog];
        let liveness = HashMap::from([(req.id.clone(), 15u64)]);

        let state = build_state(
            &events,
            &Graph::default(),
            false,
            &progress_events,
            &liveness,
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert_eq!(
            state.activity.len(),
            1,
            "the one in-flight agent appears in the present view"
        );
        let a = &state.activity[0];
        assert_eq!(a.id, req.id);
        assert_eq!(a.stage, "u");
        assert_eq!(a.latest_activity.as_deref(), Some("grep #12: conductor.rs"));
        assert_eq!(a.liveness_age_s, Some(15));
        assert_eq!(a.last_milestone.as_deref(), Some("UnitStarted"));

        // And the activity serializes into the /api/state body the page renders.
        let body = state_json(
            &events,
            &Graph::default(),
            &progress_events,
            &liveness,
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(
            body.contains("grep #12: conductor.rs"),
            "the live activity appears in the emitted state"
        );
    }

    /// Spec 30 c3 (the run-tree spine): `dash.rs` projects the run's events into a
    /// `spec -> unit -> stage -> role -> agent` tree with correct nesting; single-child
    /// levels are marked auto-collapse, the path to whatever is RUNNING is marked
    /// auto-expand, the driver-run steps (Gates, Integrate) collapse to a single "driver"
    /// line, and every node carries its live status. This is the criterion-3 OWNED
    /// projection; the tree HTML is rendered client-side in dash.html (the render boundary).
    #[test]
    fn run_tree_projects_the_spine_with_collapse_expand_and_driver_lines() {
        use crate::spawn::{
            lens_role, SpawnRequest, ROLE_ADJUDICATOR, ROLE_ADVERSARY, ROLE_IMPLEMENTER,
        };

        // A recorded RESULT answers a spawn (so it reads as done, not running).
        fn done(req: &SpawnRequest) -> Event {
            ev(
                "SpawnResult",
                &format!(r#"{{"id":"{}","output":"ok"}}"#, req.id),
            )
        }

        // Unit A (u30-c1): fully integrated - an implementer, four review agents, then
        // integration - so all four lifecycle stages appear with worker agents + driver lines.
        let a_impl = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "impl A");
        let a_sdet = SpawnRequest::new("u30-c1", "review", &lens_role("sdet"), 0, "sdet A");
        let a_arch = SpawnRequest::new("u30-c1", "review", &lens_role("arch"), 0, "arch A");
        let a_adv = SpawnRequest::new("u30-c1", "review", ROLE_ADVERSARY, 0, "adv A");
        let a_adj = SpawnRequest::new("u30-c1", "review", ROLE_ADJUDICATOR, 0, "adj A");
        // Unit B (u30-c2): in-flight, its implementer parked with NO result yet (running).
        let b_impl = SpawnRequest::new("u30-c2", "implement", ROLE_IMPLEMENTER, 0, "impl B");

        let events = positioned(vec![
            ev(
                "UnitStarted",
                r#"{"id":"u30-c1","spec_criterion":"the shell"}"#,
            ),
            a_impl.to_event().unwrap(),
            done(&a_impl),
            ev("UnitStatus", r#"{"id":"u30-c1","status":"green"}"#),
            ev("UnitStatus", r#"{"id":"u30-c1","status":"verified"}"#),
            a_sdet.to_event().unwrap(),
            done(&a_sdet),
            a_arch.to_event().unwrap(),
            done(&a_arch),
            a_adv.to_event().unwrap(),
            done(&a_adv),
            a_adj.to_event().unwrap(),
            done(&a_adj),
            ev("UnitStatus", r#"{"id":"u30-c1","status":"reviewed"}"#),
            ev("UnitIntegrated", r#"{"id":"u30-c1","commit":"abc"}"#),
            ev(
                "UnitStarted",
                r#"{"id":"u30-c2","spec_criterion":"the cells"}"#,
            ),
            b_impl.to_event().unwrap(),
        ]);

        // A live "doing" report for unit B's running implementer, so the tree subsumes the
        // old live-agent-activity panel by folding the doing-line onto the running agent.
        let bp = progress::AgentProgress {
            id: b_impl.id.clone(),
            activity: "grep #7: dash.rs".into(),
        };
        let mut bprog = Event::new(
            progress::TYPE_AGENT_PROGRESS,
            serde_json::to_vec(&bp).unwrap(),
        );
        bprog.recorded_at = SystemTime::now();
        let progress_events = vec![bprog];
        let liveness = HashMap::from([(b_impl.id.clone(), 5u64)]);

        let state = build_state(
            &events,
            &Graph::default(),
            false,
            &progress_events,
            &liveness,
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        let tree = &state.tree;

        // One spec root groups both units (the id prefix `u30` maps to `spec 30`).
        assert_eq!(tree.len(), 1, "both units nest under one spec root");
        let spec = &tree[0];
        assert_eq!(spec.kind, "spec");
        assert_eq!(spec.label, "spec 30");
        assert_eq!(spec.children.len(), 2, "spec 30 carries both units");

        let unit_a = spec.children.iter().find(|n| n.label == "u30-c1").unwrap();
        let unit_b = spec.children.iter().find(|n| n.label == "u30-c2").unwrap();
        assert_eq!(unit_a.kind, "unit");
        assert_eq!(
            unit_a.status, "integrated",
            "a node carries its live status"
        );

        // Correct nesting: the four lifecycle stages in order.
        let stages: Vec<&str> = unit_a.children.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(stages, vec!["Implement", "Gates", "Review", "Integrate"]);
        assert!(unit_a.children.iter().all(|s| s.kind == "stage"));

        // Implement -> one role (implementer) -> one agent (attempt#0); single-child
        // levels auto-collapse.
        let implement = &unit_a.children[0];
        assert!(implement.auto_collapse, "a one-role stage auto-collapses");
        assert_eq!(implement.children.len(), 1);
        let impl_role = &implement.children[0];
        assert_eq!(
            (impl_role.kind.as_str(), impl_role.label.as_str()),
            ("role", "implementer")
        );
        assert!(impl_role.auto_collapse, "a one-agent role auto-collapses");
        assert_eq!(
            (
                impl_role.children[0].kind.as_str(),
                impl_role.children[0].label.as_str()
            ),
            ("agent", "attempt#0")
        );

        // Gates is driver-run: its couriers collapse to a single "driver" line.
        let gates = &unit_a.children[1];
        assert_eq!(
            gates.children.len(),
            1,
            "the gate step collapses to one driver line"
        );
        assert_eq!(gates.children[0].kind, "driver");
        assert!(gates.auto_collapse);

        // Review -> the lens/adversary/adjudicator roles; the lens role groups sdet + arch.
        let review = &unit_a.children[2];
        let roles: Vec<&str> = review.children.iter().map(|r| r.label.as_str()).collect();
        assert!(
            roles.contains(&"lens")
                && roles.contains(&"adversary")
                && roles.contains(&"adjudicator")
        );
        let lens = review.children.iter().find(|r| r.label == "lens").unwrap();
        let lens_agents: Vec<&str> = lens.children.iter().map(|a| a.label.as_str()).collect();
        assert!(lens_agents.contains(&"sdet") && lens_agents.contains(&"arch"));
        assert!(lens.children.iter().all(|a| a.kind == "agent"));

        // Integrate is driver-run (the conductor folds it - no integrator spawn): one driver line.
        let integrate = &unit_a.children[3];
        assert_eq!(integrate.children[0].kind, "driver");

        // Unit B is in-flight with a RUNNING implementer: the whole path to it auto-expands.
        assert!(
            spec.auto_expand,
            "the spec on the running path auto-expands"
        );
        assert!(unit_b.auto_expand, "the in-flight unit auto-expands");
        let b_implement = &unit_b.children[0];
        assert!(b_implement.auto_expand, "the running stage auto-expands");
        let b_agent = &b_implement.children[0].children[0];
        assert_eq!(b_agent.kind, "agent");
        assert_eq!(
            b_agent.status, "running",
            "the parked-but-unanswered spawn is live"
        );
        assert!(b_agent.auto_expand);
        assert_eq!(
            b_agent.doing.as_deref(),
            Some("grep #7: dash.rs"),
            "the running agent folds in its live doing-line (subsumes the activity panel)"
        );

        // The fully-integrated unit is NOT on the running path.
        assert!(!unit_a.auto_expand, "a done unit is not auto-expanded");

        // The tree serializes into the /api/state body the page renders.
        let body = state_json(
            &events,
            &Graph::default(),
            &progress_events,
            &liveness,
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(
            body.contains("\"tree\""),
            "the run tree ships in the emitted state"
        );
        assert!(body.contains("u30-c1"));
    }

    /// Review verdicts on the wire are exactly `metrics::project`'s classification, never a
    /// second derivation in the dash. Locks the reuse the spec mandates.
    #[test]
    fn review_verdicts_come_straight_from_the_metrics_classification() {
        // A per-unit review reject: a `verified` transition then a loop-back UnitFailed.
        // And a separate approve: a `reviewed` transition.
        let events = positioned(vec![
            ev("UnitStarted", r#"{"id":"a","agent":"impl"}"#),
            ev("UnitStatus", r#"{"id":"a","status":"verified"}"#),
            ev("UnitFailed", r#"{"id":"a"}"#),
            ev("UnitStarted", r#"{"id":"b","agent":"impl"}"#),
            ev("UnitStatus", r#"{"id":"b","status":"reviewed"}"#),
        ]);
        let m = metrics::project(&events);
        let state = build_state(
            &events,
            &Graph::default(),
            false,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert_eq!(state.metrics.review_reject, m.review_reject);
        assert_eq!(state.metrics.review_approve, m.review_approve);
        assert_eq!(
            state.metrics.review_reject, 1,
            "the verified-then-failed loop-back classifies as one reject"
        );
        assert_eq!(state.metrics.review_approve, 1);
    }

    #[test]
    fn events_endpoint_is_since_exclusive() {
        let events = seeded_run();
        let all: serde_json::Value = serde_json::from_str(&events_json(&events, 0)).unwrap();
        assert_eq!(all["events"].as_array().unwrap().len(), events.len());

        let tail: serde_json::Value = serde_json::from_str(&events_json(&events, 4)).unwrap();
        let tail = tail["events"].as_array().unwrap();
        assert_eq!(tail.len(), 2, "since=4 returns only positions 5 and 6");
        assert_eq!(tail[0]["position"], 5);
        assert_eq!(tail[0]["type"], "UnitStatus");
    }

    /// The structural read-only pin: NO mutating endpoint exists. Every write-shaped method,
    /// on every path (including ones that look like write targets), is refused with 405 and
    /// mutates nothing.
    #[test]
    fn no_mutating_endpoint_exists() {
        let events = seeded_run();
        for method in ["POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] {
            for path in [
                "/",
                "/api/state",
                "/api/events",
                "/api/units/u1",
                "/api/run",
                "/anything",
            ] {
                let r = route(
                    method,
                    path,
                    &events,
                    &Graph::default(),
                    &[],
                    &HashMap::new(),
                    3,
                    "rigger-run",
                    "origin/main",
                );
                assert_eq!(
                    r.status, 405,
                    "{method} {path} must be refused: the dash has no write surface"
                );
            }
        }
    }

    #[test]
    fn unknown_get_path_is_404() {
        let r = route(
            "GET",
            "/does/not/exist",
            &[],
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 404);
    }

    #[test]
    fn export_inlines_the_snapshot_as_a_static_page() {
        let events = seeded_run();
        let html = render_export(
            &events,
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(
            !html.contains(STATE_PLACEHOLDER),
            "export must resolve the placeholder"
        );
        assert!(
            !html.contains("EMBEDDED_STATE = null"),
            "an export is NOT the live/null page - it carries the snapshot"
        );
        assert!(
            html.contains("\"id\":\"u1\""),
            "the snapshot's unit is inlined into the static page"
        );
        // The static page renders offline: its state carries the event feed.
        assert!(
            html.contains("UnitIntegrated"),
            "the exported feed is inlined so the static page renders without fetching"
        );
    }

    /// Regression (adjudicator-blocked stored XSS): an agent-authored string field - a
    /// finding/decision summary or a raw event payload, all of which flow verbatim into the
    /// exported snapshot's inlined event feed - must never break out of the `<script>`
    /// container. serde_json escapes none of `< > /`, so a payload carrying `</script>` would
    /// close the script element and inject executing markup into the shared export file.
    #[test]
    fn export_neutralizes_a_script_breakout_in_the_inlined_state() {
        // A realistic malicious payload: it inlines verbatim into the feed summary.
        let payload = r#"{"id":"u1","note":"</script><img src=x onerror=alert(1)>"}"#;
        let events = positioned(vec![ev("DecisionMade", payload)]);
        let html = render_export(
            &events,
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();

        // The template carries exactly ONE real `</script>` (its own script close). Were the
        // inlined snapshot left raw, the payload's `</script>` would add a second and break the
        // container; neutralization keeps the count at one.
        assert_eq!(
            html.matches("</script>").count(),
            1,
            "the inlined snapshot must carry no raw </script> that escapes the script container"
        );
        // The breakout markup must not survive verbatim anywhere in the file.
        assert!(
            !html.contains("</script><img"),
            "the </script>-prefixed injection must be neutralized, not inlined raw"
        );
        // Neutralized, not dropped: the `<` is escaped to its < JSON form, so the browser
        // still parses the state back to the original string value.
        assert!(
            html.contains(r"\u003c/script\u003e"),
            "the payload's < is escaped to its \\u003c JSON form, preserving the value while defanging the tag"
        );
        // The escaped state is still valid JSON that round-trips to the original string.
        let start = html.find("EMBEDDED_STATE = ").unwrap() + "EMBEDDED_STATE = ".len();
        let rest = &html[start..];
        let end = rest.find(";\n").unwrap();
        let state: serde_json::Value = serde_json::from_str(&rest[..end]).unwrap();
        let feed = state["events"].as_array().unwrap();
        assert!(
            feed.iter().any(|e| e["summary"]
                .as_str()
                .unwrap_or("")
                .contains("</script><img")),
            "the round-tripped value is the original payload, unharmed by the transport escaping"
        );
    }

    #[test]
    fn decision_view_strikes_through_superseded_entries() {
        let node = |id: &str, kind: &str, summary: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::from([("summary".to_string(), summary.to_string())]),
        };
        let graph = Graph {
            nodes: vec![
                node("d-new", KIND_DECISION, "the new call"),
                node("d-old", KIND_DECISION, "the old call"),
            ],
            edges: vec![Edge {
                from: "d-new".to_string(),
                to: "d-old".to_string(),
                rel: REL_SUPERSEDES.to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: TIER_EXTRACTED.to_string(),
            }],
        };
        let view = build_graph_view(&graph);
        let old = view.decisions.iter().find(|d| d.id == "d-old").unwrap();
        let new = view.decisions.iter().find(|d| d.id == "d-new").unwrap();
        assert!(old.superseded, "a SUPERSEDES target is struck through");
        assert!(!new.superseded, "the superseding decision is not");
    }

    /// Spec 42 c1: [`cluster_key`] folds every node into ONE super-node bucket. A node whose id NAMES
    /// A FILE (a code entity `<file>::<name>`, a rationale anchor `<file>#L<n>`, or a path id whose
    /// last segment carries an extension) clusters by that file's DIRECTORY (its module); a
    /// directory-less repo-root path falls back to the [`CLUSTER_ROOT`] bucket; every other node - a
    /// dev-loop node with no path id - clusters by its KIND. The mapping is deterministic. This test
    /// OWNS the fold key; it does not exercise the overview/drill aggregations (c2, c3).
    #[test]
    fn cluster_key_folds_paths_by_directory_and_dev_loop_nodes_by_kind() {
        // A code entity `<file>::<name>` folds to its file's DIRECTORY (its module) - the `::name`
        // suffix is stripped, then the file clusters by its parent directory.
        assert_eq!(
            cluster_key("src/conductor.rs::gate_verdict_key", KIND_CODE_ENTITY),
            "src"
        );
        // A nested module keeps its FULL directory path (not just the leaf directory).
        assert_eq!(
            cluster_key("src/contextgraph/sqlite.rs::project", KIND_CODE_ENTITY),
            "src/contextgraph"
        );
        // A rationale anchor `<file>#L<n>` folds to the SAME file directory as its code entity.
        assert_eq!(
            cluster_key("src/conductor.rs#L20616", KIND_RATIONALE),
            "src"
        );
        // A plain path id (a file / design-doc whose last segment carries an extension) folds to its
        // directory, whatever its node kind is.
        assert_eq!(
            cluster_key("shim/mock-rigger-server.mjs", KIND_FILE),
            "shim"
        );
        assert_eq!(cluster_key("docs/architecture.md", KIND_DESIGN_DOC), "docs");
        // A design-doc SECTION id `<doc>#<slug>` folds to the doc's directory too (the `#slug` is
        // stripped exactly like a rationale's `#L<n>`).
        assert_eq!(
            cluster_key("docs/architecture.md#grounding", KIND_DESIGN_DOC),
            "docs"
        );
        // A directory-less (repo-root) path id falls back to the `(root)` bucket - a bare file, a
        // root-file code entity, and a root-doc section all land there.
        assert_eq!(cluster_key("Cargo.toml", KIND_FILE), CLUSTER_ROOT);
        assert_eq!(
            cluster_key("build.rs::args", KIND_CODE_ENTITY),
            CLUSTER_ROOT
        );
        assert_eq!(
            cluster_key("README.md#usage", KIND_DESIGN_DOC),
            CLUSTER_ROOT
        );
        // A non-path dev-loop node (a decision / finding / agent - no path id) folds to its KIND.
        assert_eq!(
            cluster_key("adj-u41c1-approve", KIND_DECISION),
            KIND_DECISION
        );
        assert_eq!(
            cluster_key("adv-pc-project-scoping-untested", KIND_FINDING),
            KIND_FINDING
        );
        assert_eq!(
            cluster_key("adjudicator/plan-critique", KIND_AGENT),
            KIND_AGENT
        );
        // A dev-loop id carrying slashes AND a `#` (e.g. a spawn-style agent id) is still NOT a path:
        // its last segment has no extension, so it folds by kind, never mistaken for a file.
        assert_eq!(cluster_key("u42-c1/implementer#0", KIND_AGENT), KIND_AGENT);
        // Determinism: two entities in the SAME file fold to one identical module bucket.
        assert_eq!(
            cluster_key("src/dash.rs::cluster_key", KIND_CODE_ENTITY),
            cluster_key("src/dash.rs::neighborhood", KIND_CODE_ENTITY),
            "two entities in the same file fold to the same module bucket"
        );
    }

    /// Spec 42 c2: [`clustered_overview`] folds the WHOLE graph into cluster super-nodes. Each
    /// [`cluster_key`] bucket becomes a [`Cluster`] carrying its member COUNT and its DOMINANT member
    /// KIND (ties broken by the lexicographically-smallest kind, so the colour is deterministic);
    /// every currently-valid edge whose endpoints fall in two DIFFERENT clusters adds weight to a
    /// symmetric [`ClusterEdge`] (an intra-cluster edge adds none, an invalidated edge counts for
    /// nothing); and `total` carries the full node count. This test OWNS the overview aggregation; it
    /// does NOT own the fold key (c1) or the drill projection (c3).
    #[test]
    fn clustered_overview_folds_the_graph_into_counted_dominant_kind_clusters_and_cross_cluster_edges(
    ) {
        let node = |id: &str, kind: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::new(),
        };
        let edge = |from: &str, to: &str, valid_to: Option<i64>| Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        };
        let graph = Graph {
            nodes: vec![
                // Cluster "src": one code-entity + one file -> count 2, kinds TIE 1-1, so the
                // dominant kind resolves to the lexicographically-smallest ("code-entity" < "file").
                node("src/a.rs::foo", KIND_CODE_ENTITY),
                node("src/b.rs", KIND_FILE),
                // Cluster "docs": three design-docs -> count 3, dominant "design-doc".
                node("docs/x.md", KIND_DESIGN_DOC),
                node("docs/y.md", KIND_DESIGN_DOC),
                node("docs/z.md#s", KIND_DESIGN_DOC),
                // Cluster "decision": two dev-loop decision nodes -> count 2, dominant "decision".
                node("d1", KIND_DECISION),
                node("d2", KIND_DECISION),
            ],
            edges: vec![
                // Cross-cluster src<->docs, twice -> one symmetric edge of weight 2.
                edge("src/a.rs::foo", "docs/x.md", None),
                edge("src/b.rs", "docs/y.md", None),
                // Cross-cluster decision<->src -> weight 1.
                edge("d1", "src/a.rs::foo", None),
                // INTRA-cluster (both in "src") -> adds NO weight.
                edge("src/a.rs::foo", "src/b.rs", None),
                // INTRA-cluster (both in "decision") -> adds NO weight.
                edge("d1", "d2", None),
                // Cross-cluster decision<->docs but INVALIDATED -> must NOT count.
                edge("d2", "docs/x.md", Some(42)),
            ],
        };

        let overview = clustered_overview(&graph);

        // `total` is the FULL node count, independent of the cluster count.
        assert_eq!(overview.total, 7, "total carries every node in the graph");

        // Clusters come out deterministically ordered by key, each with its member count and its
        // dominant kind; the "src" tie (1 code-entity vs 1 file) resolves to the smallest kind.
        assert_eq!(
            overview.clusters,
            vec![
                Cluster {
                    key: "decision".to_string(),
                    count: 2,
                    kind: KIND_DECISION.to_string(),
                },
                Cluster {
                    key: "docs".to_string(),
                    count: 3,
                    kind: KIND_DESIGN_DOC.to_string(),
                },
                Cluster {
                    key: "src".to_string(),
                    count: 2,
                    kind: KIND_CODE_ENTITY.to_string(),
                },
            ],
            "each cluster_key bucket folds to a counted, dominant-kind Cluster; the src tie resolves to the smallest kind"
        );

        // Only cross-cluster, currently-valid edges carry weight; symmetric pairs canonicalize to
        // from<=to and merge, so src<->docs (twice) is one weight-2 edge, and the invalidated
        // decision<->docs edge is absent entirely.
        assert_eq!(
            overview.edges,
            vec![
                ClusterEdge {
                    from: "decision".to_string(),
                    to: "src".to_string(),
                    weight: 1,
                },
                ClusterEdge {
                    from: "docs".to_string(),
                    to: "src".to_string(),
                    weight: 2,
                },
            ],
            "cross-cluster currently-valid edges weight symmetric ClusterEdges; intra-cluster and invalidated edges add none"
        );
    }

    /// Spec 42 c3: [`cluster_detail`] drills a cluster to its members, reusing spec 30's
    /// [`Neighborhood`] shape so the SAME renderer draws it. A cluster at/under
    /// [`CLUSTER_RENDER_BUDGET`] renders WHOLE (`truncated` omitted); a bigger one keeps exactly
    /// `CLUSTER_RENDER_BUDGET` members - the highest INTRA-CLUSTER degree, ties broken by id - sets
    /// `truncated = Some(total)`, and every returned edge has BOTH endpoints in the returned set. The
    /// DISPLAYED per-node degree is the in-view (returned-edge) degree, honoring
    /// [`NeighborhoodNode`]'s documented `degree` contract while the SELECTION ranks by the full
    /// intra-cluster degree. This test OWNS the drill projection + the budget cap; it does NOT
    /// exercise the overview aggregation (c2) or the route dispatch (c4).
    #[test]
    fn cluster_detail_drills_a_cluster_to_its_members_and_caps_a_big_one_by_degree() {
        let ce = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: BTreeMap::new(),
        };
        let dec = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_DECISION.to_string(),
            attrs: BTreeMap::new(),
        };
        let refs = |from: &str, to: &str| Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        };

        let b = CLUSTER_RENDER_BUDGET;
        // The hub's id sorts AFTER every spoke, so it survives the cap ONLY because its degree ranks
        // it first - proving degree beats the id tie-break (not that the smallest id is kept).
        let big_hub = "src/big/mod.rs::zzz_hub";
        let spoke = |i: usize| format!("src/big/mod.rs::s{i:05}");

        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();

        // OVER-BUDGET cluster `src/big`: a hub wired to b+1 spokes => b+2 members (over the b cap). The
        // spokes all tie at intra-cluster degree 1, so the id tie-break keeps the b-1 SMALLEST ids and
        // drops the two largest.
        nodes.push(ce(big_hub));
        for i in 0..=b {
            nodes.push(ce(&spoke(i)));
            edges.push(refs(big_hub, &spoke(i)));
        }

        // UNDER-BUDGET cluster `src/small`: a hub + 6 leaves (7 members, well under b). The hub's 6
        // intra-cluster edges make it a god-node (degree 6 > threshold 5). A SUPERSEDED intra edge and
        // a CROSS-cluster edge are both excluded from the drill.
        let sm_hub = "src/small/lib.rs::hub";
        for l in ["a", "b", "c", "d", "e", "f"] {
            let leaf = format!("src/small/lib.rs::{l}");
            nodes.push(ce(&leaf));
            edges.push(refs(sm_hub, &leaf));
        }
        nodes.push(ce(sm_hub));
        // A SUPERSEDED intra-cluster edge (a -> b): currently-invalid, so NOT a returned edge and it
        // adds no degree.
        edges.push(Edge {
            from: "src/small/lib.rs::a".to_string(),
            to: "src/small/lib.rs::b".to_string(),
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to: Some(9),
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        });

        // A dev-loop `decision` cluster (folds by KIND): two decision nodes + a CROSS-cluster edge from
        // the small hub into it (which the src/small drill must exclude).
        nodes.push(dec("d-xyz"));
        nodes.push(dec("d-abc"));
        edges.push(refs(sm_hub, "d-xyz"));

        let g = Graph { nodes, edges };

        // --- OVER-BUDGET DRILL: `src/big` (b+2 members, capped to b) ---
        let big = cluster_detail(&g, "src/big");
        assert_eq!(
            big.seed, "src/big",
            "the drill echoes the drilled cluster key as its seed"
        );
        assert_eq!(big.depth, 0, "a cluster drill is not a hop-bounded walk");
        assert!(big.path.is_empty() && big.explain.is_none());
        assert_eq!(
            big.truncated,
            Some(b + 2),
            "an over-budget cluster reports its FULL member count as truncated"
        );
        assert_eq!(
            big.nodes.len(),
            b,
            "exactly CLUSTER_RENDER_BUDGET members render"
        );

        let kept: std::collections::BTreeSet<&str> =
            big.nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(
            kept.contains(big_hub),
            "the highest-degree hub is kept even with the largest id (degree beats the id tie-break)"
        );
        for i in 0..=(b - 2) {
            assert!(
                kept.contains(spoke(i).as_str()),
                "the b-1 smallest-id spokes are kept"
            );
        }
        assert!(
            !kept.contains(spoke(b - 1).as_str()),
            "a largest-id spoke is dropped by the id tie-break"
        );
        assert!(
            !kept.contains(spoke(b).as_str()),
            "the largest-id spoke is dropped by the id tie-break"
        );

        for e in &big.edges {
            assert!(
                kept.contains(e.from.as_str()) && kept.contains(e.to.as_str()),
                "a returned edge {} -> {} references a budget-dropped member",
                e.from,
                e.to
            );
        }
        assert_eq!(
            big.edges.len(),
            b - 1,
            "one edge to each of the b-1 kept spokes; edges to dropped spokes are excluded"
        );
        // The DISPLAYED degree is the in-view (returned-edge) degree, NOT the b+1 intra-cluster degree.
        let hub_view = big.nodes.iter().find(|n| n.id == big_hub).unwrap();
        assert_eq!(
            hub_view.degree,
            b - 1,
            "the hub's DISPLAYED degree is its returned-edge (in-view) degree, not b+1"
        );
        assert!(hub_view.god, "a degree-{} hub is a god-node", b - 1);
        let spoke_view = big.nodes.iter().find(|n| n.id == spoke(0)).unwrap();
        assert_eq!(
            spoke_view.degree, 1,
            "a kept spoke has one returned edge (to the hub)"
        );
        assert!(!spoke_view.god);

        // --- UNDER-BUDGET DRILL: `src/small` (7 members, whole) ---
        let small = cluster_detail(&g, "src/small");
        assert_eq!(
            small.truncated, None,
            "an at/under-budget cluster renders WHOLE - truncated omitted"
        );
        assert_eq!(small.nodes.len(), 7, "all 7 members render");
        let small_ids: std::collections::BTreeSet<&str> =
            small.nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(
            !small_ids.contains("d-xyz"),
            "a different cluster's node is not a drill member"
        );
        for e in &small.edges {
            assert!(
                small_ids.contains(e.from.as_str()) && small_ids.contains(e.to.as_str()),
                "a src/small drill edge crosses out of the cluster: {} -> {}",
                e.from,
                e.to
            );
        }
        assert!(
            !small
                .edges
                .iter()
                .any(|e| e.from == "src/small/lib.rs::a" && e.to == "src/small/lib.rs::b"),
            "a superseded (currently-invalid) intra-cluster edge is excluded"
        );
        let sm_hub_view = small.nodes.iter().find(|n| n.id == sm_hub).unwrap();
        assert_eq!(
            sm_hub_view.degree, 6,
            "the small hub's in-view degree counts only its intra-cluster edges (not the cross edge)"
        );
        assert!(
            sm_hub_view.god,
            "a degree-6 hub is a god-node (above the threshold of 5)"
        );

        // --- DEV-LOOP KIND DRILL: `decision` (folds by kind) ---
        let decisions = cluster_detail(&g, KIND_DECISION);
        assert_eq!(decisions.truncated, None);
        let dec_ids: std::collections::BTreeSet<&str> =
            decisions.nodes.iter().map(|n| n.id.as_str()).collect();
        let want: std::collections::BTreeSet<&str> = ["d-abc", "d-xyz"].into_iter().collect();
        assert_eq!(
            dec_ids, want,
            "a dev-loop KIND drill returns exactly the nodes folding to that kind"
        );

        // --- GRACEFUL: an unknown cluster key drills to an empty result, never a panic ---
        let empty = cluster_detail(&g, "no/such/module");
        assert!(empty.nodes.is_empty() && empty.edges.is_empty() && empty.truncated.is_none());
    }

    /// A small tier-tagged fixture graph: a chain seed `a` -[extracted]- `b` -[inferred]- `c`
    /// -[ambiguous]- `d`, so a depth-2 walk from `a` reaches {a,b,c} (never the depth-3 `d`) and the
    /// reachable edges carry two distinct tiers. `a` is a unit node; `b` a decision (its label is its
    /// summary); the rest are bare. Used by the `/api/graph` route + `neighborhood` tests.
    fn tiered_chain_graph() -> Graph {
        let node = |id: &str, kind: &str, summary: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: if summary.is_empty() {
                BTreeMap::new()
            } else {
                BTreeMap::from([("summary".to_string(), summary.to_string())])
            },
        };
        let edge = |from: &str, to: &str, rel: &str, tier: &str| Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: rel.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: tier.to_string(),
        };
        Graph {
            nodes: vec![
                node("a", KIND_UNIT, ""),
                node("b", KIND_DECISION, "the b decision"),
                node("c", "code-entity", ""),
                node("d", "file", ""),
            ],
            edges: vec![
                // `b -> a` deliberately points AT the seed, so reaching `b` from `a` proves the walk
                // follows edges in EITHER direction (not just outgoing).
                edge("b", "a", REL_DECIDED, TIER_EXTRACTED),
                edge("b", "c", REL_REFERENCES, TIER_INFERRED),
                edge("c", "d", REL_REFERENCES, TIER_AMBIGUOUS),
            ],
        }
    }

    #[test]
    fn the_graph_route_returns_a_tier_tagged_seeded_neighborhood_as_json() {
        let graph = tiered_chain_graph();
        let r = route(
            "GET",
            "/api/graph?seed=a&depth=2",
            &[],
            &graph,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 200, "the KG route answers 200");
        assert_eq!(r.content_type, "application/json", "self-contained JSON");
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();

        // The seeded neighborhood reaches {a,b,c} at depth 2 - never the depth-3 `d`.
        let ids: std::collections::BTreeSet<&str> = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            ["a", "b", "c"].into_iter().collect(),
            "depth-2 neighborhood of `a` is {{a,b,c}}, bounded before the depth-3 `d`: {body}"
        );

        // Every node carries its own label (a decision node's label is its summary; a bare node's is
        // its id) and kind, so the panel renders it without re-deriving.
        let b = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "b")
            .unwrap();
        assert_eq!(
            b["label"], "the b decision",
            "a node's label is its summary"
        );
        assert_eq!(b["kind"], KIND_DECISION);

        // Edges are TIER-TAGGED and only the ones with BOTH endpoints in the neighborhood are
        // returned (b-a extracted, b-c inferred; the c-d ambiguous edge to the out-of-range `d` is
        // excluded).
        let edges = body["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 2, "only in-neighborhood edges: {body}");
        let tiers: std::collections::BTreeSet<&str> =
            edges.iter().map(|e| e["tier"].as_str().unwrap()).collect();
        assert_eq!(
            tiers,
            [TIER_EXTRACTED, TIER_INFERRED].into_iter().collect(),
            "each returned edge is tagged with its confidence tier: {body}"
        );
        assert!(
            edges
                .iter()
                .all(|e| e["from"].is_string() && e["to"].is_string() && e["rel"].is_string()),
            "each edge carries from/to/rel: {body}"
        );
        assert_eq!(body["seed"], "a", "the neighborhood echoes its seed");
    }

    #[test]
    fn the_graph_route_percent_decodes_the_seed_so_select_to_seed_reaches_ids_with_special_chars() {
        // A rationale / code-entity id carries `#` and `::` and `/`, which the client
        // `encodeURIComponent`s before putting on `?seed=`. The route must decode it back to the
        // EXACT node id, or select-to-seed on such a node would seed nothing.
        let raw_id = "src/conductor.rs#L19930";
        let node = |id: &str| Node {
            id: id.to_string(),
            kind: "rationale".to_string(),
            attrs: BTreeMap::new(),
        };
        let graph = Graph {
            nodes: vec![node(raw_id), node("src/conductor.rs")],
            edges: vec![Edge {
                from: raw_id.to_string(),
                to: "src/conductor.rs".to_string(),
                rel: "explains".to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: TIER_EXTRACTED.to_string(),
            }],
        };
        // encodeURIComponent("src/conductor.rs#L19930") == "src%2Fconductor.rs%23L19930".
        let r = route(
            "GET",
            "/api/graph?seed=src%2Fconductor.rs%23L19930&depth=1",
            &[],
            &graph,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 200);
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(
            body["seed"], raw_id,
            "the route percent-decodes the seed back to the exact node id: {body}"
        );
        let ids: std::collections::BTreeSet<&str> = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert!(
            ids.contains(raw_id) && ids.contains("src/conductor.rs"),
            "the decoded seed reaches its own node and neighbor: {body}"
        );
    }

    #[test]
    fn the_graph_route_degrades_gracefully_for_an_unknown_seed_and_an_empty_graph() {
        // Spec 30 global constraint: with the KG feature off / an empty graph (or a seed that is not
        // a node), the panel degrades to an empty neighborhood - never an error.
        for (label, graph) in [
            ("empty graph", Graph::default()),
            ("populated graph, unknown seed", tiered_chain_graph()),
        ] {
            let r = route(
                "GET",
                "/api/graph?seed=does-not-exist",
                &[],
                &graph,
                &[],
                &HashMap::new(),
                3,
                "rigger-run",
                "origin/main",
            );
            assert_eq!(r.status, 200, "{label}: never a 500/404");
            assert_eq!(r.content_type, "application/json", "{label}");
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            assert!(
                body["nodes"].as_array().unwrap().is_empty(),
                "{label}: an unknown seed yields no nodes: {body}"
            );
            assert!(
                body["edges"].as_array().unwrap().is_empty(),
                "{label}: an unknown seed yields no edges: {body}"
            );
        }
    }

    #[test]
    fn the_graph_route_is_read_only_a_non_get_is_405() {
        // The KG route inherits the dash's structural read-only guarantee: only GET is answered.
        for method in ["POST", "PUT", "DELETE", "PATCH"] {
            let r = route(
                method,
                "/api/graph?seed=a",
                &[],
                &tiered_chain_graph(),
                &[],
                &HashMap::new(),
                3,
                "rigger-run",
                "origin/main",
            );
            assert_eq!(
                r.status, 405,
                "{method} /api/graph must be rejected read-only"
            );
        }
    }

    /// A small two-module + decision graph the ROUTE-DISPATCH test drills, overviews, and seeds. Two
    /// distinct file directories (`src/a`, `src/b`) fold to two file clusters and a bare `decision`
    /// node folds by KIND, so the three views are visibly different: the overview reports all three
    /// clusters, the drill returns one cluster's members, and the seed walks one node's neighborhood.
    fn dispatch_graph() -> Graph {
        let ce = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: BTreeMap::new(),
        };
        let refs = |from: &str, to: &str| Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        };
        Graph {
            nodes: vec![
                ce("src/a/mod.rs::foo"), // cluster "src/a"
                ce("src/a/mod.rs::bar"), // cluster "src/a"
                ce("src/b/mod.rs::baz"), // cluster "src/b"
                Node {
                    id: "d1".to_string(),
                    kind: KIND_DECISION.to_string(),
                    attrs: BTreeMap::new(),
                }, // cluster "decision"
            ],
            edges: vec![
                refs("src/a/mod.rs::foo", "src/a/mod.rs::bar"), // intra src/a
                refs("src/a/mod.rs::bar", "src/b/mod.rs::baz"), // cross src/a <-> src/b
                refs("d1", "src/a/mod.rs::foo"),                // cross decision <-> src/a
            ],
        }
    }

    /// The `/api/graph` route is ONE endpoint with THREE views selected by parameter (spec 42 c4):
    /// `cluster=<key>` returns the cluster DRILL, an empty `seed` with no `cluster` returns the
    /// clustered OVERVIEW (the new default KG view), and a non-empty `seed` returns the spec-30 SEEDED
    /// neighborhood unchanged. This test OWNS the route dispatch; it does NOT re-prove the projections
    /// (c1-c3 own the fold / overview / drill) - it proves each parameter combination reaches the
    /// RIGHT projection and serves its shape as JSON.
    #[test]
    fn the_graph_route_dispatches_cluster_overview_and_seed_by_parameter() {
        let graph = dispatch_graph();
        let call = |target: &str| {
            let r = route(
                "GET",
                target,
                &[],
                &graph,
                &[],
                &HashMap::new(),
                3,
                "rigger-run",
                "origin/main",
            );
            assert_eq!(r.status, 200, "{target} answers 200");
            assert_eq!(r.content_type, "application/json", "{target} is JSON");
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            body
        };

        // VIEW 1 - DRILL: `cluster=<key>` returns `cluster_detail(key)` (a Neighborhood echoing the
        // drilled cluster key as its seed, its members as nodes). The key `src/a` carries a `/`, so the
        // client `encodeURIComponent`s it (`src%2Fa`) and the route percent-decodes it back, exactly
        // like a seed id.
        let drill = call("/api/graph?cluster=src%2Fa");
        assert_eq!(
            drill["seed"], "src/a",
            "a cluster drill echoes the decoded cluster key as its seed: {drill}"
        );
        assert!(
            drill["clusters"].is_null(),
            "a drill is a neighborhood, not an overview (no clusters key): {drill}"
        );
        let drill_ids: std::collections::BTreeSet<&str> = drill["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            drill_ids,
            ["src/a/mod.rs::foo", "src/a/mod.rs::bar"]
                .into_iter()
                .collect(),
            "the drill returns exactly the src/a cluster's members: {drill}"
        );

        // VIEW 2 - OVERVIEW: an empty `seed` with no `cluster` returns `clustered_overview` (the
        // default KG view) - the whole-graph fold, NOT a neighborhood. Both the no-argument request
        // and an explicit empty `seed=` select it.
        for target in ["/api/graph", "/api/graph?seed="] {
            let overview = call(target);
            assert_eq!(
                overview["total"], 4,
                "{target}: the overview reports the full node total: {overview}"
            );
            assert!(
                overview["nodes"].is_null(),
                "{target}: the overview is not a neighborhood (no nodes key): {overview}"
            );
            let keys: std::collections::BTreeSet<&str> = overview["clusters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["key"].as_str().unwrap())
                .collect();
            assert_eq!(
                keys,
                ["decision", "src/a", "src/b"].into_iter().collect(),
                "{target}: the overview folds the graph into its three clusters: {overview}"
            );
        }

        // VIEW 3 - SEED: a non-empty `seed` returns the spec-30 seeded neighborhood UNCHANGED - the
        // depth-1 walk from `d1` reaches `d1` and its only neighbor `foo`, the seed is echoed, and no
        // `clusters`/`truncated` key rides along (the spec-30 shape is untouched).
        let seeded = call("/api/graph?seed=d1&depth=1");
        assert_eq!(
            seeded["seed"], "d1",
            "a non-empty seed echoes that seed: {seeded}"
        );
        assert_eq!(
            seeded["depth"], 1,
            "the seeded walk echoes its depth: {seeded}"
        );
        assert!(
            seeded["clusters"].is_null() && seeded["truncated"].is_null(),
            "the seeded neighborhood carries no overview/drill keys: {seeded}"
        );
        let seeded_ids: std::collections::BTreeSet<&str> = seeded["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            seeded_ids,
            ["d1", "src/a/mod.rs::foo"].into_iter().collect(),
            "the depth-1 neighborhood of d1 is {{d1, foo}}: {seeded}"
        );
    }

    #[test]
    fn neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges() {
        let graph = tiered_chain_graph();

        // Depth 1 from `a` reaches only its immediate neighbor `b` (via the `b -> a` edge - proving
        // the walk follows an edge that points AT the seed, not just outgoing ones).
        let n1 = neighborhood(&graph, "a", 1);
        let ids1: std::collections::BTreeSet<&str> =
            n1.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids1,
            ["a", "b"].into_iter().collect(),
            "depth 1 from `a` is {{a,b}} (both-direction: reached `b` across `b -> a`)"
        );

        // Depth 3 reaches the whole chain {a,b,c,d}; depth 2 stops at {a,b,c}. The depth argument
        // bounds the hop count exactly.
        let ids3: std::collections::BTreeSet<String> = neighborhood(&graph, "a", 3)
            .nodes
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(
            ids3,
            ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect(),
            "depth 3 reaches the full chain"
        );

        // An INVALIDATED (superseded) edge is not currently valid, so it does not carry the walk and
        // is never returned. Invalidate `b -> c`: now `c` (and `d`) are unreachable from `a`.
        let mut g2 = tiered_chain_graph();
        for e in &mut g2.edges {
            if e.from == "b" && e.to == "c" {
                e.valid_to = Some(42);
            }
        }
        let n2 = neighborhood(&g2, "a", 3);
        let ids2: std::collections::BTreeSet<&str> =
            n2.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids2,
            ["a", "b"].into_iter().collect(),
            "an invalidated edge does not carry the walk"
        );
        // The only surviving edge is `b -> a`; the invalidated `b -> c` (and the now-unreachable
        // `c -> d`) are never returned.
        assert_eq!(
            n2.edges.len(),
            1,
            "only the currently-valid in-set edge remains"
        );
        assert!(
            n2.edges.iter().all(|e| e.to != "c" && e.from != "c"),
            "no invalidated / out-of-neighborhood edge is returned"
        );
    }

    /// A star graph: one hub wired to `spokes` bare leaf nodes (each edge `extracted`, pointing
    /// hub -> spoke). A depth-1 walk from the hub reaches the whole star, so its returned
    /// neighborhood carries every hub-spoke edge - the hub's IN-NEIGHBORHOOD degree is exactly
    /// `spokes`.
    fn star_graph(hub: &str, spokes: usize) -> Graph {
        let mut nodes = vec![Node {
            id: hub.to_string(),
            kind: KIND_UNIT.to_string(),
            attrs: BTreeMap::new(),
        }];
        let mut edges = Vec::new();
        for i in 0..spokes {
            let spoke = format!("{hub}-s{i}");
            nodes.push(Node {
                id: spoke.clone(),
                kind: "code-entity".to_string(),
                attrs: BTreeMap::new(),
            });
            edges.push(Edge {
                from: hub.to_string(),
                to: spoke,
                rel: REL_REFERENCES.to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: TIER_EXTRACTED.to_string(),
            });
        }
        Graph { nodes, edges }
    }

    #[test]
    fn neighborhood_flags_god_nodes_by_degree_within_the_returned_neighborhood() {
        // A hub wired to one MORE than the threshold's worth of spokes: its in-neighborhood degree
        // is `threshold + 1`, strictly ABOVE the threshold, so it is a god-node (a high-degree hub).
        let hub_spokes = GOD_NODE_DEGREE_THRESHOLD + 1;
        let g = star_graph("hub", hub_spokes);
        let n = neighborhood(&g, "hub", 1);

        let hub = n.nodes.iter().find(|n| n.id == "hub").unwrap();
        assert_eq!(
            hub.degree, hub_spokes,
            "the hub's degree is its edge count WITHIN the returned neighborhood"
        );
        assert!(
            hub.god,
            "a node whose in-neighborhood degree ({}) is ABOVE the threshold ({}) is a god-node",
            hub.degree, GOD_NODE_DEGREE_THRESHOLD
        );

        // A spoke has a single incident edge (to the hub): degree 1, never a god-node.
        let spoke = n.nodes.iter().find(|n| n.id == "hub-s0").unwrap();
        assert_eq!(spoke.degree, 1, "a leaf spoke has degree 1");
        assert!(!spoke.god, "a degree-1 leaf is not a god-node");

        // The boundary is STRICT ("degree above a threshold"): a hub wired to EXACTLY the threshold
        // is NOT flagged. This pins `> threshold`, not `>= threshold`.
        let edge_g = star_graph("edge", GOD_NODE_DEGREE_THRESHOLD);
        let edge_n = neighborhood(&edge_g, "edge", 1);
        let edge_hub = edge_n.nodes.iter().find(|n| n.id == "edge").unwrap();
        assert_eq!(edge_hub.degree, GOD_NODE_DEGREE_THRESHOLD);
        assert!(
            !edge_hub.god,
            "a node AT the threshold is not a god-node - the flag is strictly above"
        );
    }

    #[test]
    fn path_is_the_shortest_route_between_two_selected_nodes_over_currently_valid_edges() {
        // Two routes from `a` to `d`: the long chain a -> b -> c -> d (3 hops) and the short detour
        // a -> e ... d -> e (2 hops, the `d -> e` edge traversed BACKWARD). BFS returns the SHORTER
        // route, proving it is a shortest-path search that follows edges in EITHER direction.
        let edge = |from: &str, to: &str| Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        };
        let node = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_UNIT.to_string(),
            attrs: BTreeMap::new(),
        };
        let mut g = Graph {
            nodes: vec![
                node("a"),
                node("b"),
                node("c"),
                node("d"),
                node("e"),
                node("z"),
            ],
            edges: vec![
                edge("a", "b"),
                edge("b", "c"),
                edge("c", "d"),
                edge("a", "e"),
                edge("d", "e"), // points d -> e, reached backward from e
            ],
        };
        assert_eq!(
            path(&g, "a", "d"),
            vec!["a".to_string(), "e".to_string(), "d".to_string()],
            "the shortest a -> d route is a -> e -> d (2 hops), not the 3-hop chain"
        );

        // A selected node's path to ITSELF is the single node; the path is symmetric endpoints.
        assert_eq!(path(&g, "a", "a"), vec!["a".to_string()]);

        // An unreachable target (`z` is isolated) and a missing endpoint both yield an EMPTY path -
        // the panel highlights nothing, never an error.
        assert!(
            path(&g, "a", "z").is_empty(),
            "no route to an isolated node"
        );
        assert!(
            path(&g, "a", "does-not-exist").is_empty(),
            "a missing endpoint has no path"
        );

        // An INVALIDATED (superseded) edge does not carry the path: cutting the short detour's
        // `a -> e` edge forces the path onto the surviving 3-hop chain.
        for e in &mut g.edges {
            if e.from == "a" && e.to == "e" {
                e.valid_to = Some(7);
            }
        }
        assert_eq!(
            path(&g, "a", "d"),
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ],
            "with the detour invalidated the only route is the currently-valid chain"
        );
    }

    #[test]
    fn the_graph_route_flags_god_nodes_and_returns_the_query_path_between_two_selected_nodes() {
        // Seeding the hub returns the star; the hub is flagged as a god-node on the wire and every
        // node carries its in-neighborhood degree, so the panel renders the hub without re-deriving.
        let g = star_graph("hub", GOD_NODE_DEGREE_THRESHOLD + 1);
        let r = route(
            "GET",
            "/api/graph?seed=hub&depth=1",
            &[],
            &g,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 200);
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let hub = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "hub")
            .unwrap();
        assert_eq!(
            hub["god"], true,
            "the hub crosses the wire flagged god: {body}"
        );
        assert_eq!(
            hub["degree"].as_u64().unwrap(),
            (GOD_NODE_DEGREE_THRESHOLD + 1) as u64,
            "the hub's degree crosses the wire: {body}"
        );
        // A plain seed request (no from/to) carries NO `path` key - the panel highlights a path only
        // when two nodes are selected.
        assert!(
            body.get("path").is_none(),
            "a seed-only neighborhood omits the query path: {body}"
        );

        // Selecting a second node (`from`/`to`) returns the query path between the two on the wire.
        let chain = chain_graph_local(5); // n0 -> n1 -> n2 -> n3 -> n4
        let r2 = route(
            "GET",
            "/api/graph?seed=n0&depth=4&from=n0&to=n3",
            &[],
            &chain,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r2.status, 200);
        let body2: serde_json::Value = serde_json::from_slice(&r2.body).unwrap();
        let got: Vec<&str> = body2["path"]
            .as_array()
            .expect("a from+to request carries the query path")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            got,
            vec!["n0", "n1", "n2", "n3"],
            "the route returns the shortest path between the two selected nodes: {body2}"
        );
    }

    /// A linear chain `n0 -> n1 -> ... -> n{len-1}` of bare nodes, for the route-level path proof.
    fn chain_graph_local(len: usize) -> Graph {
        let nodes = (0..len)
            .map(|i| Node {
                id: format!("n{i}"),
                kind: KIND_UNIT.to_string(),
                attrs: BTreeMap::new(),
            })
            .collect();
        let edges = (0..len.saturating_sub(1))
            .map(|i| Edge {
                from: format!("n{i}"),
                to: format!("n{}", i + 1),
                rel: REL_REFERENCES.to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: TIER_EXTRACTED.to_string(),
            })
            .collect();
        Graph { nodes, edges }
    }

    /// A provenance fixture (spec 30 c7): a decision `d1` that DECIDED a unit `u1` and GOVERNS a
    /// file `foo` (both folded by ONE event, position 42) and SUPERSEDES a prior decision `d0`
    /// (now invalidated, `valid_to` set); a SEPARATE code event (position 99) folds a REFERENCES
    /// edge from `bar` into `foo`. Exercises `explain`'s provenance: both edge directions, multiple
    /// distinct source events, and the currently-valid filter (the superseded edge is excluded).
    fn provenance_graph() -> Graph {
        let node = |id: &str, kind: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::new(),
        };
        let edge = |from: &str,
                    to: &str,
                    rel: &str,
                    tier: &str,
                    source: Position,
                    valid_to: Option<i64>| {
            Edge {
                from: from.to_string(),
                to: to.to_string(),
                rel: rel.to_string(),
                valid_from: 0,
                valid_to,
                source,
                tier: tier.to_string(),
            }
        };
        Graph {
            nodes: vec![
                node("d1", KIND_DECISION),
                node("u1", KIND_UNIT),
                node("foo", "file"),
                node("bar", "file"),
                node("d0", KIND_DECISION),
            ],
            edges: vec![
                edge("d1", "u1", REL_DECIDED, TIER_EXTRACTED, 42, None),
                edge("d1", "foo", REL_GOVERNS, TIER_EXTRACTED, 42, None),
                edge("d1", "d0", REL_SUPERSEDES, TIER_EXTRACTED, 42, Some(50)),
                edge("bar", "foo", REL_REFERENCES, TIER_INFERRED, 99, None),
            ],
        }
    }

    #[test]
    fn explain_returns_a_nodes_incident_edges_as_source_and_tier_tagged_provenance() {
        let g = provenance_graph();

        // explain(d1): the currently-valid edges INCIDENT to d1 (it is their `from`), each carrying
        // the relation, tier, and the SOURCE EVENT POSITION that folded it - the "events/decisions
        // that produced it". The SUPERSEDES edge is invalidated, so it is NOT live provenance.
        let ex = explain(&g, "d1").expect("a real node has an explanation");
        assert_eq!(ex.node, "d1");
        let facts: BTreeSet<(&str, &str, Position)> = ex
            .sources
            .iter()
            .map(|p| (p.rel.as_str(), p.tier.as_str(), p.source))
            .collect();
        assert_eq!(
            facts,
            [
                (REL_DECIDED, TIER_EXTRACTED, 42),
                (REL_GOVERNS, TIER_EXTRACTED, 42),
            ]
            .into_iter()
            .collect(),
            "explain(d1) is its two currently-valid incident edges, source-stamped; the superseded \
             SUPERSEDES edge is excluded"
        );

        // explain(foo): BOTH directions (the GOVERNS edge into it from d1, event 42; the REFERENCES
        // edge into it from bar, event 99) and DISTINCT source events - provenance gathers every
        // event that wove the node in, not just its outgoing edges.
        let exf = explain(&g, "foo").expect("foo is a node");
        let sources: BTreeSet<Position> = exf.sources.iter().map(|p| p.source).collect();
        assert_eq!(
            sources,
            [42, 99].into_iter().collect(),
            "explain(foo) carries the distinct source events that produced it (in both directions)"
        );
        assert!(
            exf.sources
                .iter()
                .any(|p| p.rel == REL_REFERENCES && p.from == "bar" && p.to == "foo"),
            "explain gathers the edge where the node is the `to` endpoint too"
        );

        // An unknown / absent id explains nothing (None), the graceful empty the panel degrades to.
        assert!(
            explain(&g, "does-not-exist").is_none(),
            "explaining a non-node yields no explanation"
        );
    }

    #[test]
    fn the_graph_route_carries_the_seed_nodes_explain_provenance() {
        let g = provenance_graph();
        let r = route(
            "GET",
            "/api/graph?seed=d1&depth=2",
            &[],
            &g,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        assert_eq!(r.status, 200);
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();

        // The response carries the SEED's explain provenance (spec 30 c7): the node it explains and
        // the source-stamped edges that produced it, so the panel answers explain(seed) with no
        // extra query and NO new route param (it rides the existing /api/graph response).
        assert_eq!(
            body["explain"]["node"], "d1",
            "the response explains the seed node: {body}"
        );
        let rels: BTreeSet<&str> = body["explain"]["sources"]
            .as_array()
            .expect("the explain provenance carries its sources")
            .iter()
            .map(|s| s["rel"].as_str().unwrap())
            .collect();
        assert_eq!(
            rels,
            [REL_DECIDED, REL_GOVERNS].into_iter().collect(),
            "the seed's provenance edges cross the wire (the superseded edge excluded): {body}"
        );
        let sources: BTreeSet<u64> = body["explain"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["source"].as_u64().unwrap())
            .collect();
        assert_eq!(
            sources,
            [42].into_iter().collect(),
            "each provenance edge carries its source event position: {body}"
        );

        // An unknown seed has no node to explain -> the explain key is OMITTED (graceful, no error).
        let r2 = route(
            "GET",
            "/api/graph?seed=ghost",
            &[],
            &g,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        );
        let body2: serde_json::Value = serde_json::from_slice(&r2.body).unwrap();
        assert!(
            body2.get("explain").is_none(),
            "an unknown seed omits the explain provenance: {body2}"
        );
    }

    #[test]
    fn graph_seeds_enumerate_unit_decision_and_finding_ids() {
        let events = vec![
            ev("UnitStarted", r#"{"unit":"u1"}"#),
            ev("DecisionMade", r#"{"id":"d1","summary":"x"}"#),
            ev("ReviewFinding", r#"{"id":"f1","by":"sdet"}"#),
            ev("GateVerdict", r#"{"gate":"g","pass":true}"#),
        ];
        let seeds = graph_seeds(&events);
        assert_eq!(
            seeds,
            vec!["d1".to_string(), "f1".to_string(), "u1".to_string()]
        );
    }

    #[test]
    fn build_state_on_an_empty_run_is_empty_not_a_panic() {
        let state = build_state(
            &[],
            &Graph::default(),
            false,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(state.run.units.is_empty());
        assert!(state.blockers.is_empty());
        assert_eq!(state.metrics.units_started, 0);
        assert_eq!(state.position, 0);
        assert!(state.step.wave.is_empty());
        // An empty run is not done, so no release-ready handoff is surfaced on the dash.
        assert!(state.release_ready.is_none());
    }

    /// Spec 38, criterion 3: the dash surfaces the SAME ready-to-release handoff as `rigger
    /// status`, from the SAME authority ([`ledger::RunState::release_ready`]) - present in the
    /// `/api/state` snapshot ONLY on a done run, naming the run branch, the release-target
    /// base, the integrated-unit count, and the PR command; absent for a run that is not done.
    #[test]
    fn release_ready_is_surfaced_on_the_dash_only_for_a_done_run() {
        // A done run: one integrated unit, no failed deferred gate.
        let done = positioned(vec![
            ev("UnitStarted", r#"{"id":"u1"}"#),
            ev("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
        ]);
        let state = build_state(
            &done,
            &Graph::default(),
            false,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        let rr = state
            .release_ready
            .as_ref()
            .expect("a done run surfaces the release-ready handoff on the dash");
        assert_eq!(rr.run_branch, "rigger-run");
        assert_eq!(rr.base, "main");
        assert_eq!(rr.integrated_units, 1);
        assert_eq!(rr.pr_command, "gh pr create --base main --head rigger-run");
        // It serializes into the /api/state body the page reads.
        let body = state_json(
            &done,
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(
            body.contains("gh pr create --base main --head rigger-run"),
            "the handoff appears in the emitted state: {body}"
        );

        // A run with a still-un-integrated unit surfaces no release-ready signal.
        let running = positioned(vec![
            ev("UnitStarted", r#"{"id":"u1"}"#),
            ev("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
            ev("UnitStarted", r#"{"id":"u2"}"#),
        ]);
        let state = build_state(
            &running,
            &Graph::default(),
            false,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(state.release_ready.is_none());
        // ... and the absent field is omitted from the serialized snapshot entirely.
        let body = state_json(
            &running,
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
        )
        .unwrap();
        assert!(!body.contains("release_ready"), "{body}");
    }

    #[test]
    fn request_line_parsing_extracts_method_and_target() {
        assert_eq!(
            parse_request_line("GET /api/state?since=3 HTTP/1.1"),
            Some(("GET".to_string(), "/api/state?since=3".to_string()))
        );
        assert_eq!(parse_request_line(""), None);
        assert_eq!(parse_request_line("GET"), None);
    }

    #[test]
    fn query_param_reads_since() {
        assert_eq!(query_param("/api/events?since=42", "since"), Some("42"));
        assert_eq!(
            query_param("/api/events?a=1&since=7&b=2", "since"),
            Some("7")
        );
        assert_eq!(query_param("/api/events", "since"), None);
    }

    /// The whole HTTP stack, end to end, against a REAL seeded sqlite store: seed a run,
    /// bind the hand-rolled server on an ephemeral loopback port, drive a real GET over a
    /// TCP socket, and assert the projected JSON comes back. Exercises [`handle_conn`], the
    /// store-reading provider, [`route`], and the response writer together - the literal
    /// "a test drives the JSON endpoints against a seeded store" the done-when calls for.
    #[test]
    fn endpoints_serve_over_a_real_socket_against_a_seeded_store() {
        use crate::conductor;
        use crate::eventstore::namespace::Namespaced;
        use crate::eventstore::sqlite::Store;
        use crate::eventstore::{Direction, EventStore, ExpectedRevision};
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("events.db");
        let db_str = db.to_str().unwrap().to_string();
        {
            let backend = Store::open(&db_str).unwrap();
            let store = Namespaced::new(&backend, "proj-dash");
            // Append unpositioned events; the store stamps the real 1-based positions.
            let seed = vec![
                ev("UnitStarted", r#"{"id":"u1","unit":"u1","agent":"impl"}"#),
                ev("UnitStatus", r#"{"id":"u1","status":"reviewed"}"#),
                ev("UnitIntegrated", r#"{"id":"u1","commit":"deadbee"}"#),
            ];
            store
                .append(conductor::STREAM, ExpectedRevision::Any, &seed)
                .unwrap();
        }

        // The same shape of read cmd_dash's provider performs (store -> run events).
        let db_for_provider = db_str.clone();
        let provider = move || -> Result<DashInputs, String> {
            let backend = Store::open(&db_for_provider).map_err(|e| e.to_string())?;
            let store = Namespaced::new(&backend, "proj-dash");
            let events = store
                .read_stream(conductor::STREAM, 0, Direction::Forward)
                .map_err(|e| e.to_string())?;
            Ok((events, Graph::default(), Vec::new(), HashMap::new()))
        };

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            handle_conn(conn, &provider, 3, "rigger-run", "origin/main").unwrap();
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        server.join().unwrap();

        assert!(
            resp.starts_with("HTTP/1.1 200 OK"),
            "state endpoint returns 200:\n{resp}"
        );
        assert!(resp.contains("application/json"), "content type is JSON");
        let body = resp.split("\r\n\r\n").nth(1).expect("a response body");
        let v: serde_json::Value = serde_json::from_str(body).expect("body is JSON");
        assert_eq!(v["run"]["units"][0]["id"], "u1");
        assert_eq!(v["run"]["units"][0]["status"], "integrated");
        assert_eq!(v["metrics"]["review_approve"], 1);
    }

    /// The read-only guard also holds over a real socket: a POST is refused 405 and the
    /// provider is never even consulted (it would panic if called), proving no request can
    /// reach a mutation path.
    #[test]
    fn a_post_over_a_real_socket_is_refused_without_touching_the_store() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        let provider = || -> Result<DashInputs, String> {
            panic!("a non-GET request must never read the store");
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            handle_conn(conn, &provider, 3, "rigger-run", "origin/main").unwrap();
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"POST /api/state HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        server.join().unwrap();

        assert!(
            resp.starts_with("HTTP/1.1 405"),
            "a write method is refused read-only:\n{resp}"
        );
    }

    // --- Spec 39, criterion 1: the per-project dash marker + idempotency decision ---

    #[test]
    fn dash_marker_round_trips_through_its_on_disk_record() {
        let m = DashMarker {
            port: 7431,
            pid: 12345,
        };
        assert_eq!(
            DashMarker::parse(&m.serialize()),
            Some(m),
            "a marker must survive serialize -> parse unchanged"
        );
    }

    #[test]
    fn dash_marker_parse_rejects_a_malformed_record() {
        // A corrupt/truncated marker reads as "no dash recorded" (None), so the step path
        // starts a fresh dash rather than trusting garbage.
        assert_eq!(DashMarker::parse(""), None, "empty is not a marker");
        assert_eq!(
            DashMarker::parse("7431"),
            None,
            "a port alone is not a marker"
        );
        assert_eq!(
            DashMarker::parse("not-a-port\n123"),
            None,
            "a non-numeric port is not a marker"
        );
        assert_eq!(
            DashMarker::parse("7431\nnot-a-pid"),
            None,
            "a non-numeric pid is not a marker"
        );
    }

    #[test]
    fn dash_marker_reads_none_for_an_absent_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dash.marker");
        assert_eq!(
            DashMarker::read(&path),
            None,
            "an absent marker file reads as no dash recorded"
        );
        let m = DashMarker {
            port: 7440,
            pid: 99,
        };
        m.write(&path).unwrap();
        assert_eq!(
            DashMarker::read(&path),
            Some(m),
            "a written marker reads back verbatim"
        );
    }

    #[test]
    fn pid_is_alive_reports_self_and_rejects_an_impossible_pid() {
        // These probes assume `/proc` (Linux, as CI and the operator run). Skip elsewhere.
        if !Path::new("/proc").is_dir() {
            return;
        }
        assert!(
            pid_is_alive(std::process::id()),
            "this very process must read as alive"
        );
        assert!(
            !pid_is_alive(u32::MAX),
            "an impossible pid must read as not alive"
        );
    }

    #[test]
    fn dash_start_needed_is_true_when_none_serving_and_false_when_one_serves() {
        let m = DashMarker { port: 7442, pid: 7 };
        // No marker at all -> a step must start one.
        assert!(
            dash_start_needed(None, |_| panic!("must not probe when there is no marker")),
            "no recorded dash -> start one"
        );
        // A marker whose dash is NOT serving (e.g. a crashed/reaped dash) -> start a fresh one.
        assert!(
            dash_start_needed(Some(m), |_| false),
            "a stale marker (dash gone) -> start a fresh one"
        );
        // A marker whose dash IS still serving -> no-op (the idempotent short-circuit).
        assert!(
            !dash_start_needed(Some(m), |_| true),
            "a live recorded dash -> start NO second one"
        );
    }

    #[test]
    fn should_reap_on_idle_reaps_on_completion_or_stale_liveness_but_never_on_a_live_or_starting_run(
    ) {
        let stale = Duration::from_secs(900);
        let fresh = Some(Duration::from_secs(5));
        let gone_stale = Some(Duration::from_secs(1_000));

        // A run that has not started yet (empty log is vacuously terminal): a just-spawned dash
        // must KEEP serving, never reap on its first poll.
        assert!(
            !should_reap_on_idle(false, true, false, None, stale),
            "a not-yet-started run's dash must not reap"
        );

        // A started run mid-flight, wave parked but no worker has touched a marker yet
        // (heartbeat None, not terminal): the dash is coming up on a live run - keep serving.
        assert!(
            !should_reap_on_idle(true, false, false, None, stale),
            "a starting run (parked wave, no heartbeat yet) must not reap"
        );

        // THE GATING CASE (unbounded run, between waves): started + spawn-level terminal + NO
        // heartbeat, but NOT unit-level settled (a wave's results are in yet the conductor has not
        // integrated them, or later-wave units are still pending). An unbounded run has NO marker
        // to consult, so a reap keyed on `terminal` alone would exit the dash MID-RUN here. The
        // `run_settled` gate is what keeps it serving until the run genuinely completes.
        assert!(
            !should_reap_on_idle(true, true, false, None, stale),
            "an unbounded run that is only transiently terminal between waves (not settled) must \
             not reap - this is the mid-run reap the settled gate prevents"
        );

        // A live run with a FRESH heartbeat, even when the log reads terminal in an inter-step
        // gap (the next wave is not parked yet): a worker touched a marker seconds ago, so the
        // run is between steps, NOT idle - keep serving. This is the inter-step false-positive
        // the done-flag alone would trip.
        assert!(
            !should_reap_on_idle(true, true, false, fresh, stale),
            "a fresh heartbeat means a live run between steps - must not reap"
        );
        assert!(
            !should_reap_on_idle(true, false, false, fresh, stale),
            "a fresh heartbeat on a non-terminal run must not reap"
        );

        // Normal completion: the run is started + spawn-terminal + unit-settled (every unit
        // integrated) and its `agent-live` markers were reclaimed by the final step's teardown
        // (heartbeat None) -> reap. This is genuine completion for both bounded (markers reclaimed)
        // and unbounded (never had markers) runs.
        assert!(
            should_reap_on_idle(true, true, true, None, stale),
            "a completed run (every unit terminal) whose heartbeat is None must reap"
        );

        // A crashed / wedged run that never reached a clean fixpoint but whose heartbeat has
        // gone stale (no worker touched a marker within the bound) -> reap, the backstop. The
        // `Some(age)` arm is independent of `run_settled`: a stale heartbeat is itself the
        // liveness-died signal.
        assert!(
            should_reap_on_idle(true, false, false, gone_stale, stale),
            "a non-terminal run whose heartbeat went stale must reap (crashed-run backstop)"
        );
        // A terminal run whose markers were not reclaimed but have aged past the bound -> reap.
        assert!(
            should_reap_on_idle(true, true, false, gone_stale, stale),
            "a terminal run whose stale heartbeat markers linger must still reap"
        );

        // Exactly-at-the-bound is NOT yet stale (strictly greater): still serving.
        assert!(
            !should_reap_on_idle(true, false, false, Some(stale), stale),
            "a heartbeat exactly at the bound is not yet stale"
        );
    }
}
