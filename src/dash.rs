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

use crate::contextgraph::{
    CallGraph, Direction, Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT,
    KIND_DECISION, KIND_FILE, KIND_FINDING, KIND_LESSON, REL_ABOUT, REL_CONTAINS, REL_GOVERNS,
    REL_IN_COMMUNITY, REL_REALIZES, REL_SUPERSEDES, TIER_INFERRED,
};
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

/// The HTTP response header every dash response carries (spec 50, criterion 1). A second
/// `rigger dash` invocation probes an already-bound port for this header to recognize an
/// already-serving SINGLETON and short-circuit - reporting the existing address instead of
/// binding a second one. Its PRESENCE is the signal; the value (the crate version) is
/// informational only. Adding the header keeps the dash read-only and introduces no new
/// endpoint - the recognition rides the root page every dash already serves.
pub const DASH_HEADER: &str = "X-Rigger-Dash";

/// The response header carrying the pid of whichever process is ACTUALLY holding the accept
/// loop that answered this response (spec 62 round 2:
/// adv-u62c1-marker-pid-not-the-serving-pid-on-singleton-race). Every dash response carries it
/// (see `Response::write_to`), stamped fresh from `std::process::id()` at write time by the
/// process that is genuinely serving - never a value plumbed in from outside. This is what lets
/// [`dash_serving_pid_on`] answer "who is REALLY serving this port" directly from the wire,
/// rather than a caller having to assume its own locally-spawned child is the one that bound it:
/// in a fixed-address singleton race (spec 50, criterion 4) the LOSING side's own spawned
/// `rigger dash` recognizes `bind_singleton`'s `AlreadyServing` arm and exits without ever
/// binding, so `dash_serving_on(port)` answering `true` proves only that SOMETHING is serving,
/// never that it was the caller's own spawn - the caller needs the served pid itself to attribute
/// a marker correctly.
pub const DASH_HEADER_PID: &str = "X-Rigger-Dash-Pid";

/// Connects to loopback `port`, issues a bare `GET /`, and reads the response HEAD (the status
/// line and headers) into a buffer - the ONE probe-a-loopback-port-for-a-bounded-dash-response
/// implementation both [`dash_serving_on`] and [`dash_serving_pid_on`] drive
/// (arch-u62c1-dash-serving-pid-on-duplicates-the-probe-read-loop, spec 62 round 2): before this
/// extraction each carried its own copy of this exact connect/write-timeout/deadline/cap/read-loop
/// machinery, a second parallel implementation of the same concern the one-mutation-authority
/// rule exists to prevent.
///
/// Bounded THREE independent ways so a dead, slow, or hostile holder can NEVER stall the caller:
///   * an overall wall-clock DEADLINE across the whole head - the per-read timeout is reset to
///     the REMAINING budget each iteration. This is the load-bearing bound: a holder that
///     dribbles bytes just under a fixed per-read timeout while NEVER sending a newline would
///     reset a per-read-only timeout forever and never complete a line, so only a bound on the
///     TOTAL read defeats it;
///   * a TOTAL byte cap - a real HTTP header block is small, so an endless within-a-line dribble
///     is bounded in volume (memory) even inside the deadline;
///   * the blank end-of-headers line ([`head_block_ended`]) - once the header block is fully read,
///     stop rather than keep reading a body, so a genuine non-dash conflict fails fast.
///
/// `stop_early` is consulted after every chunk arrives; the instant it returns `true` the
/// accumulated head is returned WITHOUT waiting for the rest of the header block - this is what
/// lets [`dash_serving_on`] short-circuit the moment it recognizes [`DASH_HEADER`], while
/// [`dash_serving_pid_on`] (which needs the FULL block, since [`DASH_HEADER_PID`] can arrive on a
/// LATER line) passes a predicate that never fires early.
///
/// Returns `None` on any connect/write/read failure, or once the deadline elapses before the
/// header block ends (and before `stop_early` fires) - the caller then reports its own failure
/// sentinel (`false` / `None`), never distinguishing WHY the probe failed. Returns `Some(head)`
/// once EITHER `stop_early` fires, OR the header block ends, OR the byte cap is reached, OR the
/// peer closes the connection - in every one of those cases the caller inspects `head` itself to
/// decide what it found (mirroring each original function's own "decide on exactly what arrived"
/// handling of a peer close).
fn probe_dash_head(port: u16, mut stop_early: impl FnMut(&[u8]) -> bool) -> Option<Vec<u8>> {
    use std::io::Read;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .ok()?;

    let deadline = std::time::Instant::now() + Duration::from_millis(750);
    const MAX_HEAD_BYTES: usize = 8 * 1024;
    let mut head: Vec<u8> = Vec::with_capacity(512);
    let mut buf = [0u8; 512];
    loop {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        if remaining.is_zero() || stream.set_read_timeout(Some(remaining)).is_err() {
            return None;
        }
        match stream.read(&mut buf) {
            Ok(0) => return Some(head), // closed: caller decides on exactly what arrived
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if stop_early(&head) || head.len() >= MAX_HEAD_BYTES || head_block_ended(&head) {
                    return Some(head);
                }
            }
            Err(_) => return None, // a slow/silent holder times out here, or the peer reset
        }
    }
}

/// Whether a rigger dash is ALREADY serving on loopback `port` (spec 50, criterion 1). Drives
/// one `GET /` (via [`probe_dash_head`]) and returns `true` only when the response carries the
/// [`DASH_HEADER`] response header, so an unrelated process that merely holds the port is NEVER
/// mistaken for a dash. Any connect / write / read failure, or a header-less response, returns
/// `false`. Bounded by short timeouts so a dead, slow, or silent holder cannot stall the caller.
/// `std`-only, so it is identical on the default and `--no-default-features` lanes.
///
/// Matching stays line-anchored and case-insensitive ([`head_has_header_line`]): only a line that
/// STARTS with the header name counts, so the marker cannot be spoofed by the same text inside
/// another header's value. The `stop_early` predicate passed to `probe_dash_head` IS this same
/// match test, so the probe returns the instant the header line is seen - this function never
/// waits out the rest of the header block once it already has its answer.
pub fn dash_serving_on(port: u16) -> bool {
    let needle = format!("{DASH_HEADER}:").to_ascii_lowercase();
    probe_dash_head(port, |head| head_has_header_line(head, needle.as_bytes()))
        .is_some_and(|head| head_has_header_line(&head, needle.as_bytes()))
}

/// The pid ACTUALLY serving loopback `port` right now, or `None` when no rigger dash answers
/// there (spec 62 round 2: adv-u62c1-marker-pid-not-the-serving-pid-on-singleton-race). Confirms
/// dash-ness the SAME way [`dash_serving_on`] does - the [`DASH_HEADER`] marker must be present,
/// so an unrelated listener holding the port is never mistaken for a dash naming a pid - and
/// then reads [`DASH_HEADER_PID`]'s value off that SAME response. This is what lets a caller ask
/// the port itself who is REALLY serving it, instead of assuming its own locally-spawned child
/// is the one that bound it: in a fixed-address singleton race (spec 50, criterion 4) the LOSING
/// side's own spawned `rigger dash` recognizes `bind_singleton`'s `AlreadyServing` arm and exits
/// without ever binding, so `dash_serving_on(port)` answering `true` proves only that SOMETHING
/// is serving, never that it was the caller's own spawn.
///
/// `None` is returned not only when nothing (or something non-dash) answers, but ALSO in the
/// STEADY STATE when a genuine rigger dash answers [`DASH_HEADER`] but never sends
/// [`DASH_HEADER_PID`] at all - a build that predates this header, or a foreign dash-shaped
/// responder. Callers must never treat that `None` as narrowly timing-related and fall back to a
/// GUESSED value of their own, the way a round-2 draft of `spawn_run_dashboard_detached` once did
/// (spec 62 round 2 fix point, adj-u62c1r2-verdict-reject-version-skew-fallback - rejected): the
/// ONLY production caller instead records the documented [`UNATTRIBUTED_PID`] sentinel on this
/// `None`, never a value it cannot prove.
///
/// Shares [`probe_dash_head`] with [`dash_serving_on`] but passes a `stop_early` that never fires:
/// [`DASH_HEADER_PID`] can arrive on a LATER line than [`DASH_HEADER`], so extracting a value
/// needs the full header block every time, unlike `dash_serving_on`'s early exit. Bounded
/// identically to `dash_serving_on` otherwise (the same connect/write timeouts, the same overall
/// deadline, and the same total-byte cap, because both run through the same probe), so this can
/// never hang or misbehave where that proven probe would not - a dead, slow, or hostile holder
/// still resolves to `None` within the same bound.
pub fn dash_serving_pid_on(port: u16) -> Option<u32> {
    let dash_needle = format!("{DASH_HEADER}:").to_ascii_lowercase();
    let pid_needle = format!("{DASH_HEADER_PID}:").to_ascii_lowercase();
    let head = probe_dash_head(port, |_| false)?;
    if !head_has_header_line(&head, dash_needle.as_bytes()) {
        return None; // not a rigger dash at all - never guess a pid from an unrelated listener
    }
    header_line_value(&head, pid_needle.as_bytes())?
        .parse()
        .ok()
}

/// The value substring of the FIRST line in `head` whose NAME matches `needle_lower` (e.g.
/// `x-rigger-dash-pid:`), trimmed - or `None` when no line matches. Case-insensitive and
/// anchored to a line start, mirroring [`head_has_header_line`]'s own matching exactly so the
/// two can never disagree about which line is "the" header.
fn header_line_value<'a>(head: &'a [u8], needle_lower: &[u8]) -> Option<&'a str> {
    if needle_lower.is_empty() {
        return None;
    }
    let mut start = 0usize;
    loop {
        if start >= head.len() {
            return None;
        }
        let line_end = head[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|rel| start + rel)
            .unwrap_or(head.len());
        let line = &head[start..line_end];
        if line.len() >= needle_lower.len()
            && line[..needle_lower.len()]
                .iter()
                .zip(needle_lower)
                .all(|(b, n)| b.to_ascii_lowercase() == *n)
        {
            return std::str::from_utf8(&line[needle_lower.len()..])
                .ok()
                .map(str::trim);
        }
        if line_end >= head.len() {
            return None;
        }
        start = line_end + 1;
    }
}

/// Whether any LINE of the HTTP response head in `head` BEGINS with `needle_lower` - the
/// lowercased header-name prefix (e.g. `x-rigger-dash:`). The match is case-insensitive and
/// ANCHORED to a line start (byte 0, or just after a `\n`), so the marker is recognized only as a
/// header NAME and can never be spoofed by the same text appearing inside another header's value.
fn head_has_header_line(head: &[u8], needle_lower: &[u8]) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    let mut start = 0usize;
    loop {
        let line = &head[start..];
        if line.len() >= needle_lower.len()
            && line[..needle_lower.len()]
                .iter()
                .zip(needle_lower)
                .all(|(b, n)| b.to_ascii_lowercase() == *n)
        {
            return true;
        }
        match line.iter().position(|&b| b == b'\n') {
            Some(rel) => start += rel + 1,
            None => return false,
        }
        if start >= head.len() {
            return false;
        }
    }
}

/// Whether the HTTP header block in `head` has ENDED - a blank line (`\r\n\r\n`, or a bare `\n\n`)
/// separating the headers from the body. Once seen, no further header can appear, so the probe can
/// stop instead of draining a body.
fn head_block_ended(head: &[u8]) -> bool {
    head.windows(4).any(|w| w == b"\r\n\r\n") || head.windows(2).any(|w| w == b"\n\n")
}

/// The outcome of binding the dash's fixed address as a SINGLETON (spec 50, criterion 1).
#[derive(Debug)]
pub enum SingletonBind {
    /// The address was free; serve on this freshly-bound listener.
    Bound(TcpListener),
    /// A rigger dash is ALREADY serving this address, so the caller reports it and exits 0
    /// instead of binding a second one (the singleton is the point).
    AlreadyServing(SocketAddr),
}

/// Bind the dash's fixed `addr` as a SINGLETON (spec 50, criterion 1): bind it DIRECTLY, with
/// NO free-port search. When the port is already held:
///   * by another rigger dash (recognized via [`dash_serving_on`]) -> [`SingletonBind::AlreadyServing`],
///     so the caller reports the existing address and exits cleanly rather than starting a second
///     dash (the second invocation is a no-op that never binds a second port);
///   * by an UNRELATED process -> the `AddrInUse` error propagates - a genuine conflict the
///     operator resolves with an explicit `--port`, never a silent drift to another port.
///
/// This is the one place the fixed-address policy lives: the address in / the address out, or a
/// loud conflict; it never searches upward the way [`free_port_from`] does. `std`-only, so it
/// holds identically on the default and `--no-default-features` lanes.
pub fn bind_singleton(addr: SocketAddr) -> io::Result<SingletonBind> {
    match TcpListener::bind(addr) {
        Ok(listener) => Ok(SingletonBind::Bound(listener)),
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            if dash_serving_on(addr.port()) {
                Ok(SingletonBind::AlreadyServing(addr))
            } else {
                Err(e)
            }
        }
        Err(e) => Err(e),
    }
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
    /// The PID of the recorded dash process. Informational only for display (e.g. `rigger
    /// watch`'s dead-dash report naming which pid stopped answering) - every liveness /
    /// idempotency decision made OVER a marker ([`dash_start_needed`]'s `still_serving`,
    /// [`dash_status`]) re-probes the marker's PORT, never this field, so a stale or
    /// unattributable value here can never wrongly suppress or fabricate a start. May be
    /// [`UNATTRIBUTED_PID`] when the port was confirmed serving but the real serving process
    /// could not be identified - never a guessed real pid.
    pub pid: u32,
}

/// The documented sentinel [`DashMarker::pid`] value recorded when a dash is confirmed serving
/// a port but the real serving process's pid could not be attributed (spec 62 round 4,
/// adj-u62c1r3-verdict-reject-idempotency-regression) - `spawn_run_dashboard_detached`'s only
/// production write site for it. `0` is never a real OS pid (the kernel reserves it; no process
/// is ever assigned it), so it can never collide with, or be mistaken for, an actual serving
/// process, and [`pid_is_alive`] naturally reads it as not alive - the safe direction.
///
/// Recording THIS rather than refusing to write any marker at all is what keeps the step path's
/// idempotent no-op working even when the winning dash's pid can never be named: the marker's
/// PORT (which `dash_start_needed`/`dash_marker_serving` actually probe; neither ever reads this
/// pid) is enough for the next `step` to recognize this dash as already serving. A round-3 fix
/// that instead wrote NO marker at all in this exact case regressed spec 39 criterion 1's
/// no-op-on-later-steps invariant, repeating the full spawn/probe/attribute cycle on every later
/// step forever - and it also leaves a marker for spec 62's sibling self-heal (u62c2) to
/// eventually correct if the real pid ever becomes attributable, where refusing to record
/// anything left it nothing to correct.
pub const UNATTRIBUTED_PID: u32 = 0;

/// The one shared filter for every DISPLAY site that renders a marker's raw pid (spec 62 round
/// 5, adj-u62c1r4-verdict-reject-sentinel-pid-leaks-to-status): maps [`UNATTRIBUTED_PID`] to
/// `None` so it renders identically to the already-correct no-matching-marker case, and passes
/// any other value through unchanged. This must be called ONLY at the point a pid is handed to
/// something that prints or serializes it ([`dash_status`]'s `NotServing` construction,
/// `watch_poll`'s three `watch::DashProbe::NotServing` construction sites in `src/main.rs`) -
/// NEVER upstream of a liveness/idempotency decision such as [`pid_if_port_matches`], whose
/// `Some`/`None` also drives which file's mtime `watch_poll` trusts for
/// `dash_breadcrumb_written_at`; filtering there would turn a genuinely port-matching sentinel
/// marker into an apparent mismatch and reintroduce the wrong-file's-mtime defect class closed
/// at round 9 (adv-u69c1-mismatched-marker-suppression-borrows-wrong-files-mtime). One function
/// for this one concern rather than four hand-rolled `.filter(|&p| p != UNATTRIBUTED_PID)`
/// copies, so a future fifth display site cannot forget it.
pub fn displayable_pid(pid: Option<u32>) -> Option<u32> {
    pid.filter(|&p| p != UNATTRIBUTED_PID)
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

/// The inode of the socket bound to `port` in `/proc/net/tcp` (the dash only ever binds IPv4
/// loopback - spec 62's own loopback-only charter). One row per socket:
/// `sl local_address rem_address st tx:rx tr:tm retrnsmt uid timeout inode ...`,
/// whitespace-separated, header row skipped; `local_address` is `<hex-ip>:<hex-port>` in the
/// kernel's own hex formatting. Matching on the PORT alone (not the ip half) is correct here: a
/// bind to `0.0.0.0:<port>` reserves every interface including loopback, so it is EXACTLY as
/// much a conflict for [`bind_singleton`]'s loopback bind as a same-address holder, and must be
/// diagnosed the same way. `None` when `/proc/net/tcp` is absent/unreadable (a non-Linux
/// platform) or no row's port matches - [`pid_holding_port`]'s "holder undiscoverable" case.
fn tcp_listen_inode_for_port(port: u16) -> Option<u64> {
    let text = std::fs::read_to_string("/proc/net/tcp").ok()?;
    let port_hex = format!("{port:04X}");
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(local_address) = fields.get(1) else {
            continue;
        };
        let Some((_, local_port)) = local_address.split_once(':') else {
            continue;
        };
        if !local_port.eq_ignore_ascii_case(&port_hex) {
            continue;
        }
        if let Some(inode) = fields.get(9).and_then(|s| s.parse().ok()) {
            return Some(inode);
        }
    }
    None
}

/// The pid of the process HOLDING `port` on this machine right now, discovered via the Linux
/// `/proc` surface (spec 62, criterion 3 - HELD-PORT DIAGNOSIS): read the kernel's own
/// listening-socket table ([`tcp_listen_inode_for_port`]) for the inode bound to `port`, then
/// scan every process's open file descriptors (`/proc/<pid>/fd/*`) for the matching
/// `socket:[<inode>]` link - the same technique `lsof`/`ss` use, done here directly over
/// `std::fs` (no `libc`, no new dependency) so it compiles identically on both feature lanes.
/// Best-effort throughout, mirroring [`crate::reap::processes_rooted_under`]'s own established
/// `/proc`-scanning discipline: a platform without `/proc`, a permission-denied `fd` dir, or a
/// holder that exits mid-scan all degrade to `None` - never a panic, never a guess.
fn pid_holding_port(port: u16) -> Option<u32> {
    let inode = tcp_listen_inode_for_port(port)?;
    let proc = Path::new("/proc");
    let entries = std::fs::read_dir(proc).ok()?;
    let needle = format!("socket:[{inode}]");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(proc.join(&name).join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Ok(link) = std::fs::read_link(fd.path()) {
                if link.to_str() == Some(needle.as_str()) {
                    return Some(pid);
                }
            }
        }
    }
    None
}

/// The single-character process state of `pid` from `/proc/<pid>/stat` - `R` running, `S`
/// sleeping, `D` uninterruptible sleep, `T` stopped by job control, `t` stopped under a
/// tracer, `Z` zombie, and so on (see `proc(5)`). Parses the same field layout every
/// `/proc/<pid>/stat` reader in this codebase already relies on (`pid (comm) state ...`, split
/// AFTER the last `)` since `comm` may itself embed spaces or parens) - `std`-only, no `libc`.
/// `None` when the pid is gone or `/proc` is unreadable, the same graceful-degrade discipline
/// as [`pid_is_alive`].
fn process_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

/// Render the HELD-PORT DIAGNOSIS (spec 62, criterion 3) for a bind failure at `addr`, given
/// whatever [`pid_holding_port`]/[`process_state`] discovered about the holder - the PURE half,
/// kept separate from the `/proc` reads so the message text is directly testable without
/// spawning a process. ALWAYS names `addr` (spec 62 Notes: "other platforms still get the
/// held-address report" - a bind failure must never surface as a bare, unexplained exit). When
/// the holder's pid is known, names it; when its state is ALSO known, names that too. A `T`/`t`
/// (stopped) holder gets the explicit diagnosis this criterion exists for: a stopped listener
/// keeps the port bound - the kernel still completes the TCP handshake into its backlog - but
/// its process never calls `accept()`, so a client HANGS instead of getting a clean refusal;
/// naming the fix (resume or kill that pid) rather than leaving the operator to guess why the
/// port looks phantom-held.
fn format_held_port(addr: SocketAddr, holder: Option<(u32, Option<char>)>) -> String {
    match holder {
        None => format!("address {addr} is already in use (holding process not found)"),
        Some((pid, Some('T' | 't'))) => format!(
            "address {addr} is already in use by pid {pid}, which is STOPPED - a stopped \
             listener keeps the port bound but its process never accepts a connection; resume \
             or kill pid {pid} to free the port"
        ),
        Some((pid, Some(state))) => {
            format!("address {addr} is already in use by pid {pid} (state {state})")
        }
        Some((pid, None)) => {
            format!("address {addr} is already in use by pid {pid} (state not discoverable)")
        }
    }
}

/// The impure half of the HELD-PORT DIAGNOSIS (spec 62, criterion 3): discover whatever this
/// machine's `/proc` surface can prove about the process holding `addr`'s port, and render it
/// via [`format_held_port`]. `pub` - `cmd_dash` (`src/main.rs`) is the one production caller,
/// reporting THIS as the `Err` it surfaces when [`bind_singleton`] finds the address genuinely
/// held by a non-dash process (a real rigger dash already on this address resolves to
/// `AlreadyServing` instead, so a caller only ever reaches this on a genuine conflict). Because
/// that precondition already holds at the call site (the OS itself confirmed `AddrInUse`), a
/// `None` holder here is safe to render as [`format_held_port`]'s "already in use (holding
/// process not found)" - occupancy is already a given, `/proc` has merely failed to attribute
/// it. Built on [`describe_held_port_if_confirmed`], whose `None` this arm is the ONE place
/// allowed to promote to that wording, precisely because this caller's precondition licenses
/// it and no other caller's does. Best-effort throughout: a platform without `/proc`, or a
/// holder that exits mid-scan, degrades to the "holding process not found" report - never a
/// panic and never a bare, unexplained exit (spec 62 Notes).
pub fn describe_held_port(addr: SocketAddr) -> String {
    describe_held_port_if_confirmed(addr).unwrap_or_else(|| format_held_port(addr, None))
}

/// The raw `(pid, rendered message)` pair [`describe_held_port_if_confirmed`] resolves from a
/// SINGLE `/proc` discovery - exposed separately (spec 62 round 4 fix,
/// adj-u62c3r3-verdict-reject-child-self-attribution) because a caller sometimes needs the
/// discovered pid ITSELF, not only the human-readable message about it. The one such caller is
/// `spawn_run_dashboard_detached` (`src/main.rs`): when its own `wait_for_dash_bind` gives up, it
/// must tell a genuinely competing external process apart from its OWN just-spawned child having
/// merely bound the port slower than the startup window allows - a distinction only the raw pid
/// (compared against the child's already-known pid), never the rendered message text, can carry.
/// Splitting this out is also what lets [`describe_held_port_if_confirmed`] and this function
/// share the exact same discovery rather than each re-running [`pid_holding_port`] independently:
/// two scans of a live, mutable `/proc` could in principle disagree (a holder can appear or
/// vanish between them); one discovery, consumed both ways, cannot.
///
/// `None` under the identical "nothing independently confirmed" gate as
/// [`describe_held_port_if_confirmed`] - see that function's doc for why an unconfirmed holder
/// must never be promoted to a claim.
pub fn held_port_holder(addr: SocketAddr) -> Option<(u32, String)> {
    let pid = pid_holding_port(addr.port())?;
    Some((pid, format_held_port(addr, Some((pid, process_state(pid))))))
}

/// The gated half of the HELD-PORT DIAGNOSIS (spec 62 round 3 fix,
/// adj-u62c3r2-verdict-reject-non-addrinuse-mislabel): unlike [`describe_held_port`], this
/// never asserts occupancy it has not independently confirmed via `/proc`
/// ([`pid_holding_port`]) - `None` when nothing can be confirmed holding `addr`'s port, `Some`
/// with the full pid/state diagnosis when something can. `pub` (cross-crate: `src/main.rs` is a
/// separate binary crate that depends on this library) -
/// `spawn_run_dashboard_detached` (`src/main.rs`) is the one caller: unlike `cmd_dash`'s manual
/// arm (which only ever calls [`describe_held_port`] AFTER `bind_singleton` has itself
/// confirmed a genuine `AddrInUse`), the step-path auto-start has no such confirmation
/// available - its bind attempt runs inside a detached child whose `io::Error` never reaches
/// the parent (`Stdio::null()`, spec 44), so ALL it knows going in is that `wait_for_dash_bind`
/// (`src/main.rs`) gave up, a signal that is equally true of a genuine held port, a permission
/// error, a slow machine, or an unrelated config problem (measured indistinguishable by exit
/// timing alone - a duration-based gate is not hermetic). Calling
/// [`describe_held_port`] unconditionally on that weaker signal was round 2's defect: its
/// `None` arm is licensed ONLY by an already-confirmed conflict, so firing it on an
/// unconfirmed one falsely told an operator a phantom process held their port. This function
/// supplies the missing confirmation itself, from the same `/proc` read `describe_held_port`
/// would have made anyway - never a second, differently-worded guess. Defined in terms of
/// [`held_port_holder`] (round 4) so the two can never drift apart.
pub fn describe_held_port_if_confirmed(addr: SocketAddr) -> Option<String> {
    held_port_holder(addr).map(|(_, msg)| msg)
}

/// The loopback port embedded in a recorded dash URL (`http://127.0.0.1:<port>/`, the only
/// shape any dash-starting path writes - [`crate`]'s `spawn_run_dashboard` and
/// `spawn_run_dashboard_detached` both format it this way). `None` for anything that does not
/// parse as `scheme://host:port...` with a valid `u16` port, so a malformed or foreign URL is
/// treated as unparseable rather than guessed at - the safe direction [`dash_status`] takes for
/// every other ambiguous input.
///
/// This is the ONE url-port parser shared by the library (`dash_status` below) and the `rigger`
/// binary's `watch_poll` (spec 69, round 11 architecture/adversary review,
/// `arch-u69c1-duplicate-url-port-parser`). `watch_poll` (`src/main.rs`) used to hand-roll a
/// second, DIVERGENT copy (`port_from_dash_url`, last-colon-in-the-whole-url) that only agreed
/// with this scheme-and-path-aware parser on the single documented no-path URL shape; it now
/// calls this fn directly instead. `pub`, not `pub(crate)`: the binary is a separate crate that
/// depends on this library crate, so a `pub(crate)` item here would be invisible to it.
pub fn url_port(url: &str) -> Option<u16> {
    let after_scheme = url.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    host_port.rsplit_once(':')?.1.parse().ok()
}

/// The port-match-then-name-pid rule (spec 69, round 11 architecture/adversary review,
/// `arch-u69c1-pid-match-rule-duplicated-dash-status-watch-poll` /
/// `adv-u69c1-pid-match-duplication-verified-and-escalated`): a marker's `pid` is only ever
/// attributable to `port` when the marker's OWN port matches it - a marker naming some OTHER
/// dash's port carries a pid that belongs to an unrelated process, never this one's. `dash_status`
/// and `watch_poll` (`src/main.rs`) both need exactly this rule when deciding whether to name a
/// pid, so it is factored here as the crate's one implementation rather than each hand-rolling its
/// own copy (round 9's `adv-u69c1r9-watch-poll-dashprobe-diverges-from-dash-status-mismatch-
/// handling` was a real, adjudicator-upheld regression traced to exactly that duplication).
/// `pub`, not `pub(crate)`: `watch_poll` lives in the `rigger` BINARY, a separate crate from
/// this library, so `pub(crate)` here would not reach it.
pub fn pid_if_port_matches(marker: &DashMarker, port: u16) -> Option<u32> {
    (marker.port == port).then_some(marker.pid)
}

/// The truthful presentation of the dash breadcrumb `rigger status` shows (spec 69, criterion
/// 4: "`rigger status` never lies about the dash"). A recorded URL alone is not proof the dash
/// is still up - a crashed or killed process leaves the breadcrumb behind on disk - so this
/// weighs it against the per-project [`DashMarker`] and a serving predicate before deciding
/// what a caller may trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashStatus {
    /// No dash URL has ever been recorded for this project - nothing to show.
    Absent,
    /// A URL is recorded and TRUSTED: either a probe PROVES it is still serving, or no marker
    /// exists to check against and it is left unverified. An absent marker reads as
    /// "unverifiable", never "dead" - the marker LIFECYCLE is a separate concern (spec 62), and
    /// more than one dash-starting path (the guard-bound `rigger run` / `rigger serve` dash)
    /// records a URL but no marker at all, so treating "no marker" as "dead" would falsely
    /// report a genuinely live dash as down.
    Serving(String),
    /// A probe PROVED the recorded URL's own port is not serving - the lie this criterion
    /// closes: the recorded URL is withheld, so an operator is never sent chasing it.
    NotServing {
        /// The pid a MATCHING marker names, when one is recorded (`Some`). A marker whose port
        /// names some OTHER dash never supplies this: its pid belongs to an unrelated process,
        /// not the one that used to serve this URL, so printing it would be a second lie in the
        /// other direction (round 3, adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url).
        pid: Option<u32>,
    },
}

/// Decide [`DashStatus`] from the two on-disk breadcrumbs and an injected port-serving probe
/// (spec 69, criterion 4). Pure, so both the trusted-URL and the caught-lie outcomes are
/// provable without a real dashboard process; the production caller (`rigger status`) injects
/// [`dash_serving_on`] directly - the SAME underlying probe the step path's own idempotent-start
/// decision ([`dash_start_needed`]) verifies through, so `rigger status` and the step path can
/// never disagree about whether a recorded dash is alive.
///
/// Round 2 (adv-u69c4-dash-status-verifies-wrong-port): a marker only PROVES `recorded_url`'s
/// liveness when its port MATCHES the port embedded in that URL - two independent
/// dash-starting paths write `dash.url` and `dash.marker` separately (one writes a URL alone
/// on a free-searched port, the other writes both together on a fixed port) and neither is
/// ever cleared, so a project that has used both can be left with breadcrumbs naming two
/// different dashes.
///
/// Round 3 (adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url): the round-2 fix stopped a
/// mismatched marker's OWN liveness standing in as proof about `recorded_url`, but then fell
/// back to filtering the marker to `None` and taking the SAME unconditional-trust branch a
/// genuinely absent marker uses - which is not "nothing to check": a marker that exists but
/// names a different dash is a positive signal something IS being tracked, it just cannot prove
/// THIS url. So a genuinely dead `recorded_url` paired with a mismatched-but-otherwise-live
/// marker sailed straight through as trusted, unverified. The fix: probe `url_port(&url)`
/// itself directly whenever no MATCHING marker exists to prove it. A marker that is truly
/// ABSENT still skips the probe and stays trusted unconditionally (the guard-bound `rigger run`
/// / `rigger serve` dash never writes one at all, so treating its absence as suspicious would
/// falsely distrust the one path that is documented to have none - spec 62 owns making that
/// path write one, not this criterion). A mismatched marker no longer gets that free pass: its
/// port differs from `port`, so the branch below probes `port` (the URL's own) regardless, and
/// only a genuinely matching marker's pid is ever named in the [`DashStatus::NotServing`] this
/// returns - a mismatched marker's pid is never printed as though it belonged to this URL.
pub fn dash_status(
    recorded_url: Option<String>,
    marker: Option<DashMarker>,
    port_serving: impl Fn(u16) -> bool,
) -> DashStatus {
    let Some(url) = recorded_url else {
        return DashStatus::Absent;
    };
    let Some(marker) = marker else {
        // Nothing recorded to check against at all - unverifiable but trusted, unchanged.
        return DashStatus::Serving(url);
    };
    let Some(port) = url_port(&url) else {
        // A recorded URL this crate never wrote (foreign or malformed) - unparseable, so
        // unverifiable, the same safe direction taken for every other ambiguous input here.
        return DashStatus::Serving(url);
    };
    // A pid is only ever named when the marker's port MATCHES this url's - a mismatched
    // marker's pid belongs to some other, unrelated dash and must never be printed as though it
    // were this url's. Shared with `watch_poll` (`src/main.rs`) via [`pid_if_port_matches`] so
    // the rule is implemented exactly once in the crate.
    let pid = pid_if_port_matches(&marker, port);
    if port_serving(port) {
        DashStatus::Serving(url)
    } else {
        // Round 5 (adj-u62c1r4-verdict-reject-sentinel-pid-leaks-to-status): filtered HERE, at
        // the display construction site, never inside `pid_if_port_matches` itself, via the one
        // shared `displayable_pid` (see its doc for why). A pid of `UNATTRIBUTED_PID` names no
        // real process - `spawn_run_dashboard_detached` (`src/main.rs`) records it only to keep
        // the marker's PORT usable for idempotency, and documents that no reader may treat it as
        // a real pid. Printing it unfiltered here would render "marker names dead pid 0" for a
        // process that was never assigned that pid - a literal violation of spec 69 criterion 4's
        // "never lies about the dash" text.
        DashStatus::NotServing {
            pid: displayable_pid(pid),
        }
    }
}

/// The idempotency decision for the step drive path (spec 39, criterion 1): given the
/// per-project [`DashMarker`] recorded on disk (if any) and a predicate reporting whether a
/// recorded dash is STILL serving, returns `true` iff the step must START a run dashboard -
/// i.e. NONE is already serving. A marker naming a still-serving dash short-circuits to
/// `false`, so the second and every later `step` of a run is a no-op, never a second dash
/// or a port fight. `still_serving` is injected so the decision is provable without a real
/// dash process; production passes a probe over [`dash_serving_on`] (a marker left by a
/// self-reaped or pid-recycled dash must never masquerade as still serving on a bare pid
/// check) - the SAME underlying probe [`dash_status`]'s truthful presentation verifies
/// through, so the two decisions can never disagree about whether a recorded dash is alive.
pub fn dash_start_needed(
    marker: Option<DashMarker>,
    still_serving: impl Fn(DashMarker) -> bool,
) -> bool {
    match marker {
        Some(m) => !still_serving(m),
        None => true,
    }
}

/// The self-reap decision for the machine-level SINGLETON dashboard (spec 50, criterion 5;
/// spec 62, criterion 5): given the count of registered instances currently LIVE, whether the
/// watcher has EVER observed a live instance, and whether a fresh AGENT liveness signal is
/// present, returns `true` iff the singleton should REAP ITSELF now - so a quiet machine leaves
/// no orphaned dash. This is the domain core the detached dash's watcher polls; the watcher owns
/// only the I/O (reading the machine-global instance registry, scanning the local project's
/// agent-liveness markers, sleeping, and exiting on `true`), so the DECISION is provable here
/// without a real dashboard process or a real run.
///
/// This RETARGETS spec 39's per-run trigger ("my run went idle") at the singleton ("NOTHING has
/// been registered or alive for the idle window"). The dash is no longer a per-run, per-project
/// process watching only its OWN run's liveness markers: it is one machine-level process that
/// serves every registered instance and outlives any single run, so its PRIMARY liveness signal
/// is the discovery [`crate::registry`], not one run's `agent-live` heartbeat. `live_instances`
/// is the length of [`crate::registry::read_live`], which already applies the idle window (an
/// instance counts as live only while its heartbeat is fresher than the window; a reader prunes
/// the rest), so "no live instance" means EVERY registered instance's heartbeat has aged past the
/// idle window and none was refreshed within it.
///
/// Spec 62 criterion 5 adds a SECOND, independent signal on top of that registry view: `rigger
/// progress` / `emit` / `result` couriers refresh the registry (spec 62 criterion 4), but an
/// agent's OWN liveness-marker touch (spec 10's heartbeat, e.g. mid-build with no courier call in
/// between) does not - so a registry that has genuinely aged out can still be sitting under a
/// project with a live, working agent. The idle judgment must see that agent, not just the
/// registry: it does, through `agent_live`.
///
/// - `live_instances`: how many registered instances are currently live (heartbeat within the idle
///   window). Greater than zero means at least one run - on THIS project or any other, local or a
///   shared store - is alive and needs the dash, so it keeps serving. This is what lets the
///   singleton SURVIVE one project's run ending while another's is still live: that other instance
///   keeps the count positive.
/// - `ever_seen_live`: whether the watcher has observed a live instance on any prior poll. This is
///   the startup-race guard, the direct analogue of spec 39's `run_started`: a singleton the step
///   path just ensured reads zero live instances until its ensuring run writes its registry entry,
///   and it must NOT reap on those first empty polls before the entry lands. Once any live instance
///   has been seen, a return to zero is genuine machine idle and reaps. The safe direction on
///   uncertainty (never yet seen a live instance) is to keep serving. Scoped to the REGISTRY only
///   (unchanged by criterion 5): the agent-liveness signal has no analogous startup race to guard
///   (an absent marker degrades to `agent_live: false`, the same safe-to-check-again-next-poll
///   default the registry's own absent-directory read already uses).
/// - `agent_live`: whether the SAME liveness authority `rigger status` presents - a per-spawn
///   `agent-live` marker ([`crate::liveness::any_marker_fresh`]) - shows a fresh signal right now,
///   for the launching project OR (spec 62 criterion 5 round 2) ANY other currently- or
///   formerly-registered project the watcher has ever seen (`watch_and_self_reap_on_idle`'s own
///   doc comment covers the per-project derivation and its `known_roots` durability). `true`
///   withholds the reap even when the registry has genuinely gone quiet, because a live agent
///   working under a lapsed courier cadence - on this project or any other one the singleton
///   outlives - is still real work in flight. This function itself stays agnostic to WHERE the
///   signal came from; it takes one already-folded boolean.
///
/// A genuinely quiet machine - registry empty past the startup guard AND no fresh agent liveness
/// signal - reaps exactly as spec 50 criterion 5 always has.
pub fn should_reap_singleton(
    live_instances: usize,
    ever_seen_live: bool,
    agent_live: bool,
) -> bool {
    // Reap only once the watcher has seen a live instance, none remains live, AND no agent is
    // signalling liveness directly: a quiet machine. A positive count never reaps (a live run -
    // any project's - keeps the singleton serving); an empty registry that has never yet held a
    // live instance keeps serving (the startup guard); and a fresh agent liveness signal keeps
    // serving even once the registry itself has aged out (spec 62 criterion 5).
    ever_seen_live && live_instances == 0 && !agent_live
}

/// What the dash's data provider yields per request: the run's events, its context subgraph,
/// this run's progress reports (spec 14), and each in-flight spawn's liveness-marker age.
/// Factored into a `type` so the provider signature stays readable across the server, its
/// callers, and the tests.
pub type DashInputs = (Vec<Event>, Graph, Vec<Event>, HashMap<String, u64>);

/// One row of the dash's LANDING view (spec 50, criterion 3): a registered rigger instance the
/// operator can ATTACH to. It is the presentation projection of a [`crate::registry::Instance`],
/// carrying only what the page needs to label and select it - and CREDENTIAL-FREE by
/// construction, because the registry entry it is built from is already redacted (the shared
/// endpoint passed through [`crate::eventstore::endpoint_label`] at registration, never a raw
/// connection string). The `id` is the OPAQUE selector the client echoes back on
/// `?instance=<id>` to attach the run/graph views to this instance's stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstanceView {
    /// The registry entry's stable id - the token the client puts on `?instance=` to attach.
    pub id: String,
    /// The project's stream-namespace identity, shown as the instance's name.
    pub project: String,
    /// The project root on disk, so two same-named projects are still told apart.
    pub root: String,
    /// `local` (an embedded sqlite log) or `shared` (a server backend) - drives the label/icon.
    pub kind: String,
    /// The CREDENTIAL-FREE store label: the local sqlite path, or the bare `scheme://host:port`
    /// of a shared endpoint. Taken verbatim from the already-redacted registry entry.
    pub store: String,
    /// Whole seconds since this instance last heartbeat (0 when the stamp is in the future under
    /// clock skew) - the page shows it as freshness / sorts the live ones first.
    pub age_secs: u64,
}

/// Project the live registry entries into the landing view's rows (spec 50, criterion 3), sorted
/// deterministically (by project, then root) because [`crate::registry::read_live`] returns entries
/// in an unspecified filesystem order. Pure and credential-free: every field is copied from the
/// already-redacted [`crate::registry::Instance`], so no connection secret can reach the view.
pub fn instance_views(instances: &[crate::registry::Instance], now_ms: u64) -> Vec<InstanceView> {
    use crate::registry::StoreIdentity;
    let mut views: Vec<InstanceView> = instances
        .iter()
        .map(|i| {
            let (kind, store) = match &i.store {
                StoreIdentity::Local { path } => ("local", path.clone()),
                StoreIdentity::Shared { endpoint } => ("shared", endpoint.clone()),
            };
            InstanceView {
                id: i.id(),
                project: i.project.clone(),
                root: i.root.clone(),
                kind: kind.to_string(),
                store,
                age_secs: now_ms.saturating_sub(i.heartbeat_ms) / 1000,
            }
        })
        .collect();
    views.sort_by(|a, b| a.project.cmp(&b.project).then_with(|| a.root.cmp(&b.root)));
    views
}

/// The `/api/instances` body (spec 50, criterion 3): the landing list of registered instances as
/// a JSON array of [`InstanceView`]. A tiny hand-built wrapper so the endpoint has no extra DTO,
/// mirroring [`events_json`].
pub fn instances_json(instances: &[InstanceView]) -> String {
    serde_json::json!({ "instances": instances }).to_string()
}

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
    /// `grep-fallback:` progress lines recorded during this run, counted by
    /// [`metrics::grep_fallbacks`] over the run's progress slice (spec 58): the standing signal
    /// of how often an agent still reached for grep over the graph. Carried in the
    /// review-outcomes data so the fallback rate is visible run-over-run.
    pub grep_fallbacks: u64,
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
    /// The DIRECTED-CALL view marker (spec 52 c4): the direction the `view=calls` walk ran -
    /// `"down"` (execution path / callees), `"up"` (call sites / callers), or `"both"` (the flow
    /// through a centered seed). Set ONLY on a call view; omitted (`None`) for every neighborhood /
    /// overview / drill, so a present `dir` unambiguously tells the renderer to draw the layered
    /// left-to-right DAG instead of the force layout, and its absence keeps those views
    /// byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// The UP call view's "referenced but not called" sidecar (spec 52 c4): the FILE nodes that
    /// import / use the seed's name at file level but call it from no function - the who-uses-this
    /// sites the traversed caller DAG deliberately excludes. Carried verbatim from
    /// [`crate::contextgraph::CallGraph::referenced_not_called`], sorted by id. Empty (and omitted)
    /// for a DOWN walk and for every non-call view, so a plain neighborhood is byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub referenced_not_called: Vec<NeighborhoodNode>,
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
    /// The DIRECTED-CALL LAYER (spec 52 c4): the node's SIGNED x-ordinate in a `view=calls` DAG -
    /// the seed is `0`, a callee sits at `+hop` (so a DOWN walk draws the seed at the LEFT), and a
    /// caller at `-hop` (so an UP walk draws the seed at the RIGHT); a `dir=both` walk carries both
    /// signs around the centered seed. The left-to-right renderer maps `layer` directly to x. `None`
    /// for every non-call node (a neighborhood / drill node), so those views are byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<i64>,
    /// The MULTI-CANDIDATE FRONTIER marker (spec 52 c4): when `Some`, this cross-file hop's name has
    /// more than one definition, so the walk did NOT descend it and returns the SORTED candidate
    /// definition ids for the human to re-seed on - honest by construction (the view may be
    /// INCOMPLETE but never confidently wrong). Carried verbatim from
    /// [`crate::contextgraph::CallNode::frontier`]. `None` for a fully-resolved node and for every
    /// non-call node, so a plain neighborhood is byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontier: Option<Vec<String>>,
    /// The SHARED-MEMBERSHIP marker (spec 54 c3): true when this node realizes MORE THAN ONE derived
    /// concept, so a [`Lens::Concepts`] drill flags it - it folds under its PRIMARY concept and
    /// appears once, never silently duplicated across the concepts it realizes. `false` (and omitted
    /// from the JSON) for a single-concept or membership-less node and for every non-concepts view, so
    /// a plain neighborhood / drill / call node stays byte-identical.
    #[serde(skip_serializing_if = "is_not_shared", default)]
    pub shared: bool,
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
    /// The RECURSION / BACK-edge marker (spec 52 c4): true when this edge (in a `view=calls` DAG)
    /// points at a node whose layer is NOT deeper than its source - a recursion / mutual call the
    /// walk marked rather than followed a second time, which the renderer draws as a distinct curved
    /// return arc. Carried verbatim from [`crate::contextgraph::CallEdge::back`]. Always `false` for
    /// a neighborhood / drill edge (and omitted from the JSON), so those views are byte-identical.
    #[serde(skip_serializing_if = "is_not_back", default)]
    pub back: bool,
}

/// Serde `skip_serializing_if` predicate for [`NeighborhoodEdge::back`]: keep the recursion marker
/// off the wire for the common forward edge, so a plain neighborhood / drill edge (which is never a
/// back edge) serializes byte-identically to before the call views existed.
fn is_not_back(back: &bool) -> bool {
    !*back
}

/// Serde `skip_serializing_if` predicate for [`NeighborhoodNode::shared`]: keep the shared-membership
/// marker off the wire for the common single-concept / membership-less node, so a plain neighborhood /
/// drill / call node serializes byte-identically to before the concepts lens existed.
fn is_not_shared(shared: &bool) -> bool {
    !*shared
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

/// One RATIONALE LEAF (spec 55, the rationale overlay "why" layer): a decision, finding, or lesson
/// attached to a graph node through a live knowledge edge - a `decision` that `GOVERNS` the node, or
/// a `finding` / `lesson` that is `ABOUT` it. It carries the leaf's CONTENT only - its `id`, its
/// `kind` (`"decision"` / `"finding"` / `"lesson"`), and its `summary` - and deliberately NOT the
/// builder-agent attribution a finding node also carries (the `by` reviewer and the `unit`): that is
/// run machinery, not the target project's design memory, so the overlay never surfaces it (spec 55
/// "content, not machinery"; the `unit` attr is in any case dormant in production). Built by
/// [`node_rationale`] over the already-projected graph, so the overlay is a pure read.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RationaleLeaf {
    /// The leaf node's id (the decision / finding / lesson id, so the client can key its disclosure).
    pub id: String,
    /// The leaf's node kind: [`KIND_DECISION`], [`KIND_FINDING`], or [`KIND_LESSON`].
    pub kind: String,
    /// The leaf's human CONTENT: the `summary` attr the fold records for a decision / finding /
    /// lesson node. Empty only if the node carries no summary (never for a real emitted leaf).
    pub summary: String,
}

/// The rationale leaves attached to ONE node (spec 55): the node id echoed, and its leaves sorted
/// deterministically. Only ever produced for a node that carries AT LEAST ONE leaf - a node with no
/// rationale is absent from the batch (the client badges only the nodes that have any; spec 55
/// "batched per request for the visible nodes that have any"), so this shape never carries an empty
/// `leaves`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NodeRationale {
    /// The node whose rationale these leaves are (echoed so the client keys the badge to it).
    pub node: String,
    /// The node's rationale leaves, sorted by `(kind, id)` so the same graph yields byte-identical
    /// output every request. Always non-empty (see [`NodeRationale`]).
    pub leaves: Vec<RationaleLeaf>,
}

/// The `/api/graph?explain=<id>[,<id>...]` batch body (spec 55, the rationale overlay data path): the
/// per-node rationale for the VISIBLE nodes the client asked about, in ONE request. Only the nodes
/// that carry any rationale appear, ordered by node id - so the client renders a "why" badge exactly
/// on the nodes that have leaves and nowhere else. A distinct response shape from the neighborhood /
/// overview / drill, served over the SAME lazy whole-graph provider `/api/graph` already reads, never
/// the state poll. Built by [`rationale_batch`], a pure read over the already-projected graph.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RationaleBatch {
    /// The nodes that carry rationale, ordered by node id (deterministic). Empty when NONE of the
    /// requested nodes carries a decision / finding / lesson - the graceful empty a project with no
    /// decisions degrades to, never an error.
    pub nodes: Vec<NodeRationale>,
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
    /// The human DISPLAY label under [`Lens::Code`]: a coupling community's deterministic `label`
    /// attr (its highest-degree member, folded by the `CommunityAssigned` recording of spec 53 c3),
    /// so the panel names the subsystem instead of its opaque `community/<r>/<n>` id. Absent (skipped
    /// in JSON) under [`Lens::Files`] and for a non-community bucket, where `key` already names the
    /// module / kind - so the default files overview stays byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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
    /// The documented empty state under [`Lens::Code`] when the selected resolution grain has NO
    /// derived community assignments (the offline detection pass never ran at that grain): carries
    /// [`CODE_LENS_UNDERIVED`] so the panel prompts the operator to run the derivation instead of an
    /// error or a bare kind-bucket view. Absent (skipped in JSON) under [`Lens::Files`] and under a
    /// derived code grain, so the default files overview stays byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
}

/// The SUBJECT x LENS re-projection body (spec 55 c1): a SELECTED subject re-grained at a lens's
/// altitude, in place, rather than switched to a whole-graph overview. Built by [`reproject`] from
/// the subject's MEMBER SET (a concept's `REALIZES` members; a community's members; a file's
/// contained entities; a single entity is its own set), re-bucketed under the requested lens - by
/// coupling community under [`Lens::Code`], by derived concept under [`Lens::Concepts`], by DISTINCT
/// DEFINING FILE under [`Lens::Files`]. A pure read over the already-projected graph: no store touch,
/// no new event type. Deterministic by construction, so the same graph + subject + lens yield a
/// byte-identical body.
#[derive(Debug, Serialize, PartialEq, Eq, Default)]
pub struct Reprojection {
    /// The re-grained subject's id, echoed so the panel labels the view and its back link (the
    /// re-projection's analogue of [`Neighborhood::seed`]).
    pub subject: String,
    /// The re-bucketed member set as [`Cluster`] super-nodes, ordered deterministically by
    /// [`Cluster::key`]: coupling-community buckets under [`Lens::Code`], concept buckets under
    /// [`Lens::Concepts`], and DISTINCT DEFINING FILE buckets under [`Lens::Files`] (the same
    /// renderer draws them as the whole-graph overview's clusters).
    pub clusters: Vec<Cluster>,
    /// The symmetric cross-bucket coupling edges AMONG the member set (the same [`ClusterEdge`] fold
    /// the overview uses, restricted to member-to-member edges), ordered by `(from, to)`.
    pub edges: Vec<ClusterEdge>,
    /// The member-set size (every member, resolved or not), so the panel reports the re-grain size -
    /// NOT the whole-graph node count.
    pub total: usize,
    /// The MARKED-UNRESOLVED members (spec 55 c1 honesty rule), set only under [`Lens::Files`]: a
    /// bare cross-file placeholder member whose name resolves to MORE THAN ONE definition (or to
    /// none) cannot be attributed to a single defining file, so it is surfaced here - each carrying
    /// its SORTED candidate definition ids - rather than folded into the WRONG file bucket its
    /// (referencing-file) id would encode. Ordered by member id. Empty (and omitted from the JSON)
    /// under [`Lens::Code`] / [`Lens::Concepts`] and for a fully-resolvable FILES re-grain, so those
    /// bodies stay lean.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<UnresolvedMember>,
    /// The member ids flagged SHARED (spec 55 c2): a member realizing MORE THAN ONE concept folds
    /// under its PRIMARY concept (criterion 1, so it appears ONCE) and is listed here, so the panel
    /// marks a multi-bucket member rather than silently duplicating or hiding it - the re-projection's
    /// twin of the drill's per-node `shared` flag. Sorted by member id. Always empty (and omitted from
    /// the JSON) under [`Lens::Code`] (a node carries at most one community) and [`Lens::Files`] (files
    /// never share), so those bodies stay lean.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shared: Vec<String>,
    /// The full BUCKET count when a WIDE re-grain was capped to [`CLUSTER_RENDER_BUDGET`] (spec 55 c2):
    /// the largest buckets are kept (ties by key), the cross-bucket edges are pruned to the kept set,
    /// and this carries M so the panel captions "showing N of M". Absent (`None`, omitted from the
    /// JSON) for an at/under-budget re-grain, so a present `truncated` unambiguously means the cell was
    /// capped - mirroring [`Neighborhood::truncated`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<usize>,
    /// The documented empty-CELL message (spec 55 c2), set under a DERIVED lens when the member set
    /// folds into NO community/concept bucket: [`REPROJECT_NO_COMMUNITY`] under [`Lens::Code`],
    /// [`REPROJECT_NO_CONCEPT`] under [`Lens::Concepts`]. The criterion-1 kind-fallback clusters still
    /// render, so this caption is ADDITIVE - the defined-but-empty cell is explained, never blanked.
    /// Absent (`None`, omitted from the JSON) under [`Lens::Files`] (a file re-grain always resolves)
    /// and whenever any member DID fold into a derived bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
}

/// One MARKED-UNRESOLVED member of a FILES re-projection (spec 55 c1): a bare cross-file placeholder
/// whose entity-name has NOT exactly one definition, so it cannot be honestly attributed to a single
/// defining file. `candidates` carries the SORTED ids of the definitions sharing the name (empty when
/// the name is defined nowhere the graph knows) - the frontier a human re-seeds on, never a silent
/// wrong attribution. Mirrors the directed-call view's frontier honesty (spec 52), applied to
/// re-projection.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UnresolvedMember {
    /// The bare placeholder member's id (the call target in its referencing file's namespace).
    pub id: String,
    /// The SORTED ids of the code-entity definitions sharing the member's entity-name: MORE THAN ONE
    /// (the ambiguous case) or ZERO (defined nowhere the graph knows).
    pub candidates: Vec<String>,
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
        // Counted off the SEPARATE progress slice (the same one the live activity view folds),
        // never the run stream - so the run-stream projections stay byte-identical (spec 58).
        grep_fallbacks: metrics::grep_fallbacks(progress_events),
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
    match file_of(id) {
        // A file-bearing id clusters by the file's DIRECTORY (its module); a directory-less repo-root
        // file -> `(root)`.
        Some(file) => match file.rsplit_once('/') {
            Some((dir, _)) if !dir.is_empty() => dir.to_string(),
            _ => CLUSTER_ROOT.to_string(),
        },
        // Every other node is a dev-loop node with no path id: cluster by its KIND.
        None => kind.to_string(),
    }
}

/// The FILE PATH a node id names, or `None` when the id names no file (a dev-loop node - a decision /
/// finding / unit / community / concept). The single file-naming authority [`cluster_key`] (which
/// folds to the file's DIRECTORY) and the subject-by-lens FILES re-projection (which folds to the
/// FILE itself, spec 55 c1) both read.
///
/// An id is reduced to a file path by stripping a code-entity `::name` suffix, then a rationale /
/// doc-section `#...` suffix (a plain path id survives both untouched). The reduced path names a file
/// iff its LAST segment carries an extension: a `.` with a non-empty stem AND a non-empty suffix - so
/// a dotfile like `.gitignore` (whose only `.` is leading) is NOT a file, and a dev-loop id like
/// `plan-critique` (no extension) never is either. A file path contains neither `::` nor `#`, so the
/// splits leave a plain path untouched. Pure and total.
fn file_of(id: &str) -> Option<&str> {
    let file = id.split_once("::").map_or(id, |(f, _)| f);
    let file = file.split_once('#').map_or(file, |(f, _)| f);
    let last_segment = file.rsplit_once('/').map_or(file, |(_, seg)| seg);
    let names_a_file = last_segment
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty());
    names_a_file.then_some(file)
}

/// The entity-name SUFFIX of a `<file>::<name>` id (the part after the first `::`), or the whole id
/// when it carries no `::`. The in-memory twin of the pinned `substr(id, instr(id, '::') + 2)`
/// expression the store's cross-file name resolution uses (spec 52), so the FILES re-projection's
/// bare-node resolution (spec 55 c1) matches a bare placeholder to the DEFINITIONS sharing its name.
fn name_suffix(id: &str) -> &str {
    match id.find("::") {
        Some(i) => &id[i + 2..],
        None => id,
    }
}

// ---------------------------------------------------------------------------
// The overview/drill LENS (spec 53 c4): the bucket key is PLUGGABLE. `lens=files` is the default
// spec-42 directory/kind fold ([`cluster_key`]), byte-identical to today and to a `lens`-absent
// request. `lens=code` buckets a node by its DERIVED coupling-community membership at a resolution
// grain - the SAME overview/drill folds, a different key - so the middle-altitude view groups the
// graph by how the code WORKS TOGETHER, not where it sits on disk. There is ONE fold authority: both
// aggregations consume [`Buckets`], never a second parallel fold reconciled after the fact.
// ---------------------------------------------------------------------------

/// The default community-detection resolution grain, as its canonical string. The detection pass
/// defaults to resolution `1.0`, whose `f64` display is `1`, so its communities are `community/1/<n>`
/// (spec 53). The [`Lens::Code`] view reads THIS grain when a request omits `resolution=`.
pub const DEFAULT_COMMUNITY_RESOLUTION: &str = "1";

/// The documented empty-state message the [`Lens::Code`] overview carries when the selected
/// resolution grain has NO derived community assignments (the offline detection pass never ran at
/// that grain): the panel shows this instead of an error or a bare kind-bucket view, so an underived
/// code lens degrades gracefully to a prompt to run the derivation (spec 53 c4).
pub const CODE_LENS_UNDERIVED: &str = "code lens not derived yet - run `rigger graph communities`";

/// The default concept-derivation resolution grain, as its canonical string. The offline
/// intent-derivation pass defaults to resolution `1.0`, whose `f64` display is `1`, so its concepts
/// are `concept/1/<n>` (spec 54). The [`Lens::Concepts`] view reads THIS grain when a request omits
/// `resolution=`.
pub const DEFAULT_CONCEPT_RESOLUTION: &str = "1";

/// The documented empty-state message the [`Lens::Concepts`] overview carries when the selected
/// resolution grain has NO derived concept assignments (the offline intent-derivation pass never ran
/// at that grain): the panel shows this instead of an error or a bare kind-bucket view, so an
/// underived concepts lens degrades gracefully to a prompt to run the derivation (spec 54 c3).
pub const CONCEPTS_LENS_UNDERIVED: &str = "concepts not derived yet - run `rigger graph concepts`";

/// The documented empty-CELL message a [`Lens::Code`] RE-PROJECTION (spec 55 c2) carries when the
/// selected subject's member set folds into NO coupling community - the members exist, but none is
/// part of any derived community at this grain. Distinct from [`CODE_LENS_UNDERIVED`] (the whole-graph
/// "the offline pass never ran" prompt): a re-projection cell is empty when THIS subject's members
/// carry no membership, whether or not the grain is derived elsewhere, so the panel captions the
/// defined-but-empty cell rather than showing a bare kind-bucket fold with no explanation.
pub const REPROJECT_NO_COMMUNITY: &str = "no derived communities";

/// The documented empty-CELL message a [`Lens::Concepts`] RE-PROJECTION (spec 55 c2) carries when the
/// selected subject's member set realizes NO concept - the [`Lens::Concepts`] twin of
/// [`REPROJECT_NO_COMMUNITY`]. The kind-fallback clusters criterion 1 ships still render; this message
/// is additive, so a "not part of any concept" caption never hides the members it re-grains.
pub const REPROJECT_NO_CONCEPT: &str = "not part of any concept";

/// The overview/drill bucket lens (spec 53 c4): how a graph node folds to its super-node bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lens {
    /// The DEFAULT fold (spec 42): a node buckets by its file's DIRECTORY (module) or its KIND, via
    /// [`cluster_key`]. Byte-identical to a `lens`-absent request.
    Files,
    /// The CODE fold (spec 53): a node with a live `IN_COMMUNITY` membership at `resolution` buckets
    /// by its coupling COMMUNITY (its `community/<resolution>/<n>` id); a membership-less node keeps
    /// its KIND bucket (so the view stays whole-graph). The `resolution` grain string selects which
    /// derived grain to read.
    Code {
        /// The resolution grain to read, as the community id's grain segment (e.g. `1`, `1.5`).
        resolution: String,
    },
    /// The CONCEPTS fold (spec 54): a node with a live `REALIZES` membership at `resolution` buckets
    /// by its intent CONCEPT (its `concept/<resolution>/<n>` id) - the idea the docs and code realize,
    /// grouped across directory lines; a membership-less node keeps its KIND bucket (so the view stays
    /// whole-graph). A node realizing MORE THAN ONE concept folds under its PRIMARY (the largest
    /// concept by member count, ties by lexicographically-smallest id) and is flagged `shared` -
    /// counted once, never silently duplicated. The `resolution` grain string selects which derived
    /// grain to read.
    Concepts {
        /// The resolution grain to read, as the concept id's grain segment (e.g. `1`, `1.5`).
        resolution: String,
    },
}

impl Lens {
    /// Resolve the lens from the `/api/graph` selector params: `lens=code` selects the code fold and
    /// `lens=concepts` the concepts fold, each at `resolution=` (defaulting to the derivation's
    /// default grain - [`DEFAULT_COMMUNITY_RESOLUTION`] / [`DEFAULT_CONCEPT_RESOLUTION`] - when absent
    /// or empty); every other value - `lens=files`, an unknown lens, or an absent one - resolves to
    /// [`Lens::Files`], the byte-identical default. Total and infallible, so a hostile selector can
    /// never error the route; it just falls back to the files view.
    pub fn from_query(lens: Option<&str>, resolution: Option<&str>) -> Lens {
        match lens {
            Some("code") => Lens::Code {
                resolution: resolution
                    .filter(|r| !r.is_empty())
                    .unwrap_or(DEFAULT_COMMUNITY_RESOLUTION)
                    .to_string(),
            },
            Some("concepts") => Lens::Concepts {
                resolution: resolution
                    .filter(|r| !r.is_empty())
                    .unwrap_or(DEFAULT_CONCEPT_RESOLUTION)
                    .to_string(),
            },
            _ => Lens::Files,
        }
    }
}

/// The bucket resolver for one `(graph, lens)` (spec 53 c4): the SINGLE authority mapping a node to
/// its super-node bucket key - or `None` to EXCLUDE it from the fold - that both the overview and the
/// drill consume. Built ONCE per request, so the code lens scans the live `IN_COMMUNITY` memberships
/// a single time.
struct Buckets<'g> {
    lens: &'g Lens,
    /// A node id -> its single bucket super-node id: under [`Lens::Code`] the `community/<r>/<n>` it
    /// lives in (at most one live membership per grain, per the spec 53 c3 fold); under
    /// [`Lens::Concepts`] the PRIMARY `concept/<r>/<n>` it realizes (the largest concept it realizes,
    /// ties by lexicographically-smallest id, when it realizes more than one). Empty under
    /// [`Lens::Files`], and empty under a derived-lens grain with NO assignments - the empty-state
    /// signal [`Buckets::underived`] reads.
    membership: BTreeMap<&'g str, &'g str>,
    /// The member nodes carrying MORE THAN ONE live `REALIZES` membership at this grain (spec 54 c3):
    /// each folds under its PRIMARY concept above and is FLAGGED `shared` in the drill, so a
    /// multi-concept member appears once, never silently duplicated. Always empty under
    /// [`Lens::Files`] and [`Lens::Code`] (a node carries at most one community).
    shared: BTreeSet<&'g str>,
}

impl<'g> Buckets<'g> {
    /// Build the resolver. Under [`Lens::Code`], index every live `IN_COMMUNITY` edge whose target
    /// carries the selected grain's `community/<resolution>/` prefix (a substring equality on the id,
    /// never a wildcard match). Under [`Lens::Concepts`], index every live `REALIZES` edge to a
    /// `concept/<resolution>/` target, then fold each member to its PRIMARY concept (the largest
    /// concept by member count, ties by smallest id) and record the members that realize more than one
    /// as `shared`. A no-op under [`Lens::Files`].
    fn new(graph: &'g Graph, lens: &'g Lens) -> Self {
        let mut membership: BTreeMap<&str, &str> = BTreeMap::new();
        let mut shared: BTreeSet<&str> = BTreeSet::new();
        match lens {
            Lens::Files => {}
            Lens::Code { resolution } => {
                let prefix = format!("community/{resolution}/");
                for e in &graph.edges {
                    if e.valid_to.is_none()
                        && e.rel == REL_IN_COMMUNITY
                        && e.to.starts_with(&prefix)
                    {
                        membership.insert(e.from.as_str(), e.to.as_str());
                    }
                }
            }
            Lens::Concepts { resolution } => {
                // Every live `<member> --REALIZES--> concept/<resolution>/<n>` membership, grouped per
                // member. The derivation records at most one membership per grain, but a later
                // model-assisted refinement may realize a member under several concepts, so index them
                // ALL and fold honestly rather than trust a single-membership assumption.
                let prefix = format!("concept/{resolution}/");
                let mut realized: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
                for e in &graph.edges {
                    if e.valid_to.is_none() && e.rel == REL_REALIZES && e.to.starts_with(&prefix) {
                        realized
                            .entry(e.from.as_str())
                            .or_default()
                            .insert(e.to.as_str());
                    }
                }
                // Each concept's member count, to pick a multi-concept member's PRIMARY bucket: the
                // largest concept, ties by lexicographically-smallest id.
                let mut size: BTreeMap<&str, usize> = BTreeMap::new();
                for concepts in realized.values() {
                    for c in concepts {
                        *size.entry(*c).or_default() += 1;
                    }
                }
                for (node, concepts) in &realized {
                    let primary = concepts
                        .iter()
                        .copied()
                        .max_by(|a, b| {
                            let sa = size.get(a).copied().unwrap_or(0);
                            let sb = size.get(b).copied().unwrap_or(0);
                            // Larger member count wins; on a tie the lexicographically-SMALLER id wins
                            // (so `b.cmp(a)` makes the smaller `a` compare greater).
                            sa.cmp(&sb).then_with(|| b.cmp(a))
                        })
                        .expect("a realized member has at least one concept");
                    membership.insert(node, primary);
                    if concepts.len() > 1 {
                        shared.insert(node);
                    }
                }
            }
        }
        Buckets {
            lens,
            membership,
            shared,
        }
    }

    /// The bucket key a node folds to, or `None` to EXCLUDE it. Under [`Lens::Files`] every node
    /// folds by [`cluster_key`] (never excluded). Under [`Lens::Code`] / [`Lens::Concepts`] a member
    /// folds by its (primary) super-node id; the super-node itself ([`KIND_COMMUNITY`] /
    /// [`KIND_CONCEPT`]) is EXCLUDED (it IS a bucket, not a member, so it never inflates a bucket's
    /// member count or dominant kind); every other membership-less node keeps its KIND bucket.
    fn key(&self, node: &Node) -> Option<String> {
        match self.lens {
            Lens::Files => Some(cluster_key(&node.id, &node.kind)),
            Lens::Code { .. } | Lens::Concepts { .. } => {
                if let Some(bucket) = self.membership.get(node.id.as_str()) {
                    Some((*bucket).to_string())
                } else if self.excludes_super_node(&node.kind) {
                    None
                } else {
                    Some(node.kind.clone())
                }
            }
        }
    }

    /// The super-node KIND this lens EXCLUDES from the member fold (it IS a bucket, not a member):
    /// [`KIND_COMMUNITY`] under [`Lens::Code`], [`KIND_CONCEPT`] under [`Lens::Concepts`]. Excludes
    /// nothing under [`Lens::Files`].
    fn excludes_super_node(&self, kind: &str) -> bool {
        match self.lens {
            Lens::Files => false,
            Lens::Code { .. } => kind == KIND_COMMUNITY,
            Lens::Concepts { .. } => kind == KIND_CONCEPT,
        }
    }

    /// A derived lens at the selected grain has NO assignments: the documented empty state (the
    /// offline pass never ran at this resolution). Always `false` under [`Lens::Files`].
    fn underived(&self) -> bool {
        matches!(self.lens, Lens::Code { .. } | Lens::Concepts { .. }) && self.membership.is_empty()
    }

    /// The documented empty-state message for this lens when [`Buckets::underived`]: the derivation
    /// prompt for the active derived lens. `None` under [`Lens::Files`] (never underived).
    fn underived_message(&self) -> Option<&'static str> {
        match self.lens {
            Lens::Files => None,
            Lens::Code { .. } => Some(CODE_LENS_UNDERIVED),
            Lens::Concepts { .. } => Some(CONCEPTS_LENS_UNDERIVED),
        }
    }

    /// The documented empty-CELL message for a RE-PROJECTION whose member set folds into NO derived
    /// bucket (spec 55 c2): [`REPROJECT_NO_COMMUNITY`] under [`Lens::Code`], [`REPROJECT_NO_CONCEPT`]
    /// under [`Lens::Concepts`], `None` under [`Lens::Files`] (a file re-grain always resolves).
    /// Distinct from [`underived_message`]: a re-projection cell is empty when THIS subject's members
    /// carry no membership, independent of whether the grain is derived for the graph at large.
    fn no_membership_message(&self) -> Option<&'static str> {
        match self.lens {
            Lens::Files => None,
            Lens::Code { .. } => Some(REPROJECT_NO_COMMUNITY),
            Lens::Concepts { .. } => Some(REPROJECT_NO_CONCEPT),
        }
    }

    /// The super-node KIND whose deterministic `label` attr names a bucket cluster under this lens:
    /// [`KIND_COMMUNITY`] (a coupling community) under [`Lens::Code`], [`KIND_CONCEPT`] (a derived
    /// concept) under [`Lens::Concepts`]. `None` under [`Lens::Files`], where a bucket key already
    /// names its module / kind and no label is attached.
    fn label_kind(&self) -> Option<&'static str> {
        match self.lens {
            Lens::Files => None,
            Lens::Code { .. } => Some(KIND_COMMUNITY),
            Lens::Concepts { .. } => Some(KIND_CONCEPT),
        }
    }

    /// Whether `id` carries MORE THAN ONE live concept membership at this grain (spec 54 c3): a shared
    /// member the drill flags. Always `false` under [`Lens::Files`] and [`Lens::Code`].
    fn is_shared(&self, id: &str) -> bool {
        self.shared.contains(id)
    }
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
///
/// The bucket key is the pluggable [`Lens`] (spec 53 c4): [`Lens::Files`] is the default fold above
/// (byte-identical to today and to a `lens`-absent request); [`Lens::Code`] buckets each member node
/// by its coupling COMMUNITY at a resolution grain - the SAME aggregation over a different key - so a
/// community super-node is sized by member count, coloured by its dominant member kind, and labelled
/// by the community node's deterministic label, while membership-less nodes keep their kind buckets.
/// A code grain with NO derived assignments returns the [`CODE_LENS_UNDERIVED`] empty state.
pub fn clustered_overview(graph: &Graph, lens: &Lens) -> ClusterOverview {
    let buckets = Buckets::new(graph, lens);

    // The documented empty state (spec 53 c4 / spec 54 c3): a derived lens whose selected resolution
    // grain has NO assignments. Return an empty overview carrying the lens's derivation prompt, so the
    // panel says "run `rigger graph communities`" / "run `rigger graph concepts`" instead of showing
    // an error or a bare kind-bucket view. `total` still reports the whole graph size.
    if buckets.underived() {
        return ClusterOverview {
            clusters: Vec::new(),
            edges: Vec::new(),
            total: graph.nodes.len(),
            empty_state: buckets.underived_message().map(str::to_string),
        };
    }

    // Fold the WHOLE graph through the shared bucket fold: every node folds by the lens's
    // [`Buckets::key`], and cross-bucket edges weight the super-edges. `total` reports the whole node
    // count; a derived overview is not the empty state (handled above).
    let bucket_label = bucket_label_index(graph, &buckets);
    let (clusters, edges) = fold_buckets(
        graph.nodes.iter(),
        &graph.edges,
        |n| buckets.key(n),
        &bucket_label,
    );
    ClusterOverview {
        clusters,
        edges,
        total: graph.nodes.len(),
        empty_state: None,
    }
}

/// Index each bucket super-node's deterministic display `label` attr, so a bucket cluster can name
/// its subsystem / idea instead of its opaque id: a coupling community under [`Lens::Code`] (folded
/// by spec 53 c3), a derived concept under [`Lens::Concepts`] (spec 54). Empty under [`Lens::Files`]
/// (no super-node bucket exists there) and for the FILES re-projection (a file names itself). Read by
/// the shared [`fold_buckets`].
fn bucket_label_index<'g>(graph: &'g Graph, buckets: &Buckets<'g>) -> BTreeMap<&'g str, &'g str> {
    match buckets.label_kind() {
        Some(super_kind) => graph
            .nodes
            .iter()
            .filter(|n| n.kind == super_kind)
            .filter_map(|n| {
                n.attrs
                    .get("label")
                    .filter(|l| !l.is_empty())
                    .map(|l| (n.id.as_str(), l.as_str()))
            })
            .collect(),
        None => BTreeMap::new(),
    }
}

/// The SINGLE bucket-fold authority the whole-graph overview ([`clustered_overview`]) and the
/// subject re-projection ([`reproject`]) both consume - implemented ONCE over the shared abstraction,
/// never a second parallel fold. Fold each node `key_of` yields a bucket key for into a [`Cluster`]
/// carrying its MEMBER COUNT and DOMINANT member kind (ties -> the lexicographically-smallest kind),
/// attach the bucket's display `label` when `bucket_label` names one, and weight the SYMMETRIC
/// cross-bucket [`ClusterEdge`]s over `edges`.
///
/// A node `key_of` maps to `None` is EXCLUDED (a lens super-node, or a FILES re-projection member the
/// honesty rule could not resolve to one file). An edge is followed only when currently valid and
/// BOTH endpoints fall in the folded set (so a member-restricted fold naturally drops edges leaving
/// the member set); an intra-bucket edge (or self-loop) adds no weight; the pair is canonicalized
/// (smaller key first) so an `a -> b` and a `b -> a` graph edge fold into one weighted super-edge.
/// Deterministic by construction (`BTreeMap` folds), so clusters sort by key, each bucket's dominant
/// kind resolves the tie to the smallest kind, and edges sort by `(from, to)`.
fn fold_buckets<'g>(
    nodes: impl Iterator<Item = &'g Node>,
    edges: &[Edge],
    key_of: impl Fn(&Node) -> Option<String>,
    bucket_label: &BTreeMap<&str, &str>,
) -> (Vec<Cluster>, Vec<ClusterEdge>) {
    let mut node_cluster: BTreeMap<&str, String> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut kind_hist: BTreeMap<String, BTreeMap<&'g str, usize>> = BTreeMap::new();
    for n in nodes {
        let Some(key) = key_of(n) else {
            continue; // excluded from the member fold (a super-node / an unresolvable member)
        };
        node_cluster.insert(n.id.as_str(), key.clone());
        *counts.entry(key.clone()).or_default() += 1;
        *kind_hist
            .entry(key)
            .or_default()
            .entry(n.kind.as_str())
            .or_default() += 1;
    }

    // Each bucket becomes a Cluster whose kind is its DOMINANT member kind: the highest-count kind,
    // ties broken by the lexicographically-smallest kind (the histogram iterates in sorted kind
    // order, so replacing only on a STRICTLY-greater count keeps the first/smallest kind on a tie).
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
            let label = bucket_label.get(key.as_str()).map(|l| l.to_string());
            Cluster {
                key,
                count,
                kind: dominant.to_string(),
                label,
            }
        })
        .collect();

    // Every currently-valid edge whose two endpoints are known FOLDED nodes in DIFFERENT clusters
    // weights a symmetric cluster edge; an endpoint outside the folded set (e.g. a member-set fold's
    // edge to a non-member) has no cluster to weight, and an intra-cluster edge adds none.
    let mut weights: BTreeMap<(String, String), usize> = BTreeMap::new();
    for e in edges {
        if e.valid_to.is_some() {
            continue;
        }
        let (Some(a), Some(b)) = (
            node_cluster.get(e.from.as_str()),
            node_cluster.get(e.to.as_str()),
        ) else {
            continue;
        };
        if a == b {
            continue;
        }
        let pair = if a <= b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        *weights.entry(pair).or_default() += 1;
    }
    let cluster_edges: Vec<ClusterEdge> = weights
        .into_iter()
        .map(|((from, to), weight)| ClusterEdge { from, to, weight })
        .collect();

    (clusters, cluster_edges)
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
///
/// The membership is the pluggable [`Lens`] (spec 53 c4): the SAME drill over a different bucket key.
/// Under [`Lens::Code`], drilling a `community/<r>/<n>` key yields exactly that community's member
/// nodes and the coupling edges AMONG them (the community super-node is not a member, and a
/// membership spoke to it is not an intra-community edge, so neither renders); drilling a kind key
/// yields that kind's membership-less nodes.
pub fn cluster_detail(graph: &Graph, key: &str, lens: &Lens) -> Neighborhood {
    let buckets = Buckets::new(graph, lens);
    // The cluster's members: every node the lens folds to `key`, keyed by id for a deterministic,
    // deduped set. A node the lens EXCLUDES (a community super-node under the code lens) is never a
    // member of any bucket.
    let members: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| buckets.key(n).as_deref() == Some(key))
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
            // A neighborhood / drill edge is never a directed-call back edge (spec 52 c4).
            back: false,
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
                    // A neighborhood / drill node carries no directed-call layer or frontier (spec 52 c4).
                    layer: None,
                    frontier: None,
                    // A concepts-lens drill flags a member that realizes MORE THAN ONE concept (spec 54
                    // c3); every other lens leaves this false (the resolver's `shared` set is empty).
                    shared: buckets.is_shared(n.id.as_str()),
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
        // A cluster drill is not a directed-call view (spec 52 c4).
        dir: None,
        referenced_not_called: Vec::new(),
    }
}

/// Re-grain a SELECTED subject at a lens's altitude, in place (spec 55 c1): the SUBJECT x LENS
/// re-projection. Rather than switching to a whole-graph overview, this resolves the subject's MEMBER
/// SET at its own grain and re-buckets that set under the requested lens - so a concept flipped to the
/// Code lens shows its members grouped by coupling community, and flipped to Files shows the DISTINCT
/// DEFINING FILES those members resolve to. The two controls compose freely; a single entity is its
/// own member set, so the instrument composes rather than modes.
///
/// The MEMBER SET is read at the subject's grain from its OWN node kind: a [`KIND_CONCEPT`] resolves
/// to the nodes that `REALIZES` it; a [`KIND_COMMUNITY`] to the nodes `IN_COMMUNITY` it; a
/// [`KIND_FILE`] to the entities it `CONTAINS`; any other node (a single code entity / doc) is its own
/// singleton set; an unknown subject is the empty set (the documented empty cell, spec 55 c2). The
/// RE-BUCKET is the shared [`fold_buckets`] authority: coupling community under [`Lens::Code`],
/// derived concept under [`Lens::Concepts`], and the DISTINCT DEFINING FILE under [`Lens::Files`].
///
/// Under [`Lens::Files`] the fold resolves CROSS-GRAIN honestly ([`Reprojection::unresolved`]): a
/// member that is a real definition (or a doc / file path) folds under its own file; a BARE cross-file
/// placeholder (no `name` attr) resolves by name-suffix to the DEFINITION sharing its name - EXACTLY
/// ONE folds under that definition's file, MORE THAN ONE (or zero) is surfaced as a marked-unresolved
/// entry carrying the sorted candidate ids, never a wrong attribution to the referencing file its id
/// encodes. A pure read over the already-projected graph: no store touch, no new event type, and
/// deterministic by construction.
pub fn reproject(graph: &Graph, subject: &str, lens: &Lens) -> Reprojection {
    let members = member_set(graph, subject);
    let mut re = match lens {
        Lens::Files => reproject_files(graph, subject, &members),
        Lens::Code { .. } | Lens::Concepts { .. } => {
            reproject_derived(graph, subject, lens, &members)
        }
    };
    // Spec 55 c2, the WIDE cell: cap the bucket list to the render budget UNIFORMLY across lenses, so
    // a re-grain over a huge subject (e.g. a concept whose members span hundreds of files) renders at
    // any size. `total` (the member-set size) is untouched; `truncated` carries the full bucket count.
    re.truncated = cap_clusters(&mut re.clusters, &mut re.edges);
    re
}

/// Cap a re-projection's bucket list to [`CLUSTER_RENDER_BUDGET`] (spec 55 c2, the WIDE cell): keep
/// the LARGEST buckets (ties broken by key ascending, for a pick stable across polls), and PRUNE every
/// cross-bucket edge that touches a dropped bucket so none dangles. Returns `Some(full bucket count)`
/// for the panel's "showing N of M" caption when the cap fired, `None` when the re-grain already fit.
/// The kept buckets keep their by-key sort order ([`fold_buckets`] emits them sorted), so the capped
/// body is deterministic. Mirrors the [`cluster_detail`] drill's degree cap, at the bucket grain.
fn cap_clusters(clusters: &mut Vec<Cluster>, edges: &mut Vec<ClusterEdge>) -> Option<usize> {
    let total = clusters.len();
    if total <= CLUSTER_RENDER_BUDGET {
        return None;
    }
    // The kept keys: the budget's worth of largest buckets, ties by key ascending. Collected as owned
    // strings so the immutable borrow ends before the retains below mutate `clusters` / `edges`.
    let kept: BTreeSet<String> = {
        let mut ranked: Vec<&Cluster> = clusters.iter().collect();
        ranked.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
        ranked
            .into_iter()
            .take(CLUSTER_RENDER_BUDGET)
            .map(|c| c.key.clone())
            .collect()
    };
    clusters.retain(|c| kept.contains(&c.key));
    edges.retain(|e| kept.contains(&e.from) && kept.contains(&e.to));
    Some(total)
}

/// The subject's MEMBER SET at its own grain (spec 55 c1), as the member NODES (so the fold can read
/// each member's kind / attrs). Dispatched on the SUBJECT'S node kind: a concept's `REALIZES`
/// members, a community's `IN_COMMUNITY` members, a file's `CONTAINS` entities, else the subject's own
/// singleton set; an unknown subject (absent from the graph) is empty. Deterministic - members come
/// out in ascending-id order - and deduped.
fn member_set<'g>(graph: &'g Graph, subject: &str) -> Vec<&'g Node> {
    let by_id: BTreeMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    match by_id.get(subject).map(|n| n.kind.as_str()) {
        // A concept re-grains to the members that REALIZE it (spec 54).
        Some(KIND_CONCEPT) => {
            for e in &graph.edges {
                if e.valid_to.is_none() && e.rel == REL_REALIZES && e.to == subject {
                    ids.insert(e.from.as_str());
                }
            }
        }
        // A coupling community re-grains to its IN_COMMUNITY members (spec 53).
        Some(KIND_COMMUNITY) => {
            for e in &graph.edges {
                if e.valid_to.is_none() && e.rel == REL_IN_COMMUNITY && e.to == subject {
                    ids.insert(e.from.as_str());
                }
            }
        }
        // A file re-grains to the entities it CONTAINS (spec 29a).
        Some(KIND_FILE) => {
            for e in &graph.edges {
                if e.valid_to.is_none() && e.rel == REL_CONTAINS && e.from == subject {
                    ids.insert(e.to.as_str());
                }
            }
        }
        // A single entity (a code entity / doc / any other node) is its own member set.
        Some(_) => {
            ids.insert(subject);
        }
        // An unknown subject has no member set (the documented empty cell, spec 55 c2).
        None => {}
    }
    ids.into_iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect()
}

/// Re-bucket a member set under a DERIVED lens ([`Lens::Code`] / [`Lens::Concepts`]): fold each
/// member by its coupling community / derived concept through the shared [`fold_buckets`] authority,
/// restricted to the member set so cross-bucket edges among members weight the super-edges. `total`
/// is the member-set size, not the whole graph.
fn reproject_derived(graph: &Graph, subject: &str, lens: &Lens, members: &[&Node]) -> Reprojection {
    let buckets = Buckets::new(graph, lens);
    let bucket_label = bucket_label_index(graph, &buckets);
    let (clusters, edges) = fold_buckets(
        members.iter().copied(),
        &graph.edges,
        |n| buckets.key(n),
        &bucket_label,
    );
    // Spec 55 c2, the EMPTY cell: when NO member folds into a derived (community/concept) bucket, the
    // cell is defined-but-empty. The kind-fallback clusters above still render (criterion 1, nothing
    // dropped); this message is the additive caption the panel shows. A single member with a derived
    // membership makes the cell full and clears the message.
    let has_derived_bucket = members
        .iter()
        .any(|m| buckets.membership.contains_key(m.id.as_str()));
    let empty_state = (!has_derived_bucket)
        .then(|| buckets.no_membership_message().map(str::to_string))
        .flatten();
    // Spec 55 c2, the SHARED member: a member realizing MORE THAN ONE concept folds under its PRIMARY
    // bucket above (appears once) and is flagged here. `members` is ascending-id ordered (member_set),
    // so the flagged list is sorted by construction; only the concepts lens ever populates it.
    let shared: Vec<String> = members
        .iter()
        .filter(|m| buckets.is_shared(m.id.as_str()))
        .map(|m| m.id.clone())
        .collect();
    Reprojection {
        subject: subject.to_string(),
        clusters,
        edges,
        total: members.len(),
        unresolved: Vec::new(),
        shared,
        // The wide-cell cap runs once, uniformly, in `reproject`.
        truncated: None,
        empty_state,
    }
}

/// Re-bucket a member set under [`Lens::Files`] to its DISTINCT DEFINING FILES, resolving cross-grain
/// honestly (spec 55 c1). Each member is resolved to a file key, or surfaced as marked-unresolved:
///
/// - a member that IS a definition (a `name` attr) or a doc / file-path node folds under its OWN
///   file ([`file_of`]);
/// - a BARE cross-file code-entity placeholder (no `name` attr) resolves by name-suffix over the
///   DEFINITION nodes sharing its name: EXACTLY ONE folds under that definition's file; MORE THAN ONE
///   (or zero) is marked-unresolved with the sorted candidate ids;
/// - a member with no file identity at all (a rare dev-loop node) keeps its KIND bucket, mirroring
///   the derived lens - nothing is silently dropped.
///
/// The resolved keys feed the shared [`fold_buckets`] authority (no bucket label under files), so the
/// file buckets are sized, dominant-kind coloured, and cross-file coupling edges weighted exactly as
/// every other lens. `total` is the member-set size (resolved or not).
fn reproject_files(graph: &Graph, subject: &str, members: &[&Node]) -> Reprojection {
    // Index every code-entity DEFINITION (a `name` attr) by its entity-name suffix, for the
    // conservative cross-file resolution - the in-memory twin of the store's `definitions_with_suffix`
    // (spec 52). Each candidate list is sorted + deduped for a deterministic frontier.
    let mut defs_by_suffix: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in &graph.nodes {
        if n.kind == KIND_CODE_ENTITY && n.attrs.contains_key("name") {
            defs_by_suffix
                .entry(name_suffix(&n.id))
                .or_default()
                .push(n.id.as_str());
        }
    }
    for cands in defs_by_suffix.values_mut() {
        cands.sort_unstable();
        cands.dedup();
    }

    // Resolve each member to a file key (or mark it unresolved). `resolved` is member-id -> file key
    // the fold reads back; `unresolved` collects the marked-unresolved frontiers.
    let mut resolved: BTreeMap<&str, String> = BTreeMap::new();
    let mut unresolved: Vec<UnresolvedMember> = Vec::new();
    for m in members {
        let is_bare = m.kind == KIND_CODE_ENTITY && !m.attrs.contains_key("name");
        if is_bare {
            // A bare cross-file placeholder resolves by name-suffix; the honesty rule decides.
            let cands = defs_by_suffix
                .get(name_suffix(&m.id))
                .map(Vec::as_slice)
                .unwrap_or_default();
            match cands {
                [only] => {
                    if let Some(file) = file_of(only) {
                        resolved.insert(m.id.as_str(), file.to_string());
                        continue;
                    }
                    // A definition whose id names no file cannot attribute one: unresolved.
                    unresolved.push(UnresolvedMember {
                        id: m.id.clone(),
                        candidates: vec![(*only).to_string()],
                    });
                }
                _ => unresolved.push(UnresolvedMember {
                    id: m.id.clone(),
                    candidates: cands.iter().map(|c| (*c).to_string()).collect(),
                }),
            }
            continue;
        }
        // A definition / doc / file-path member folds under its OWN file; a member with no file
        // identity keeps its kind bucket (never silently dropped).
        match file_of(&m.id) {
            Some(file) => {
                resolved.insert(m.id.as_str(), file.to_string());
            }
            None => {
                resolved.insert(m.id.as_str(), m.kind.clone());
            }
        }
    }

    // Fold the resolved members into their file (or kind) buckets through the shared authority; a
    // member marked unresolved has no key, so it is excluded from every bucket and edge. Files name
    // themselves, so there is no bucket label.
    let empty_label: BTreeMap<&str, &str> = BTreeMap::new();
    let (clusters, edges) = fold_buckets(
        members.iter().copied(),
        &graph.edges,
        |n| resolved.get(n.id.as_str()).cloned(),
        &empty_label,
    );
    unresolved.sort_by(|a, b| a.id.cmp(&b.id));
    Reprojection {
        subject: subject.to_string(),
        clusters,
        edges,
        total: members.len(),
        unresolved,
        // A files re-grain always resolves a member (to a file, its kind, or the unresolved sidecar)
        // and never shares, so the spec 55 c2 empty-cell message and shared flag never apply here; the
        // wide-cell cap runs once, uniformly, in `reproject`.
        shared: Vec::new(),
        truncated: None,
        empty_state: None,
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
    neighborhood_of(graph, std::slice::from_ref(&seed.to_string()), seed, depth)
}

/// The multi-seed core of [`neighborhood`]: the seeded BFS over `seeds` (each seed is initially
/// reached, so the walk fans out from ALL of them at once), returning the reached nodes and the
/// tier-tagged edges among them, with `echo_seed` recorded as the response's `seed`. This is the
/// single traversal authority; the single-seed [`neighborhood`] is the one-element case. The
/// re-pointed run-tree click (spec 43) uses it to seed from a unit's several decision/finding
/// content nodes at once - the unit id itself being no longer a node - and still echo the unit id
/// the client asked for.
fn neighborhood_of(graph: &Graph, seeds: &[String], echo_seed: &str, depth: i64) -> Neighborhood {
    // Reached-node set (each seed is always in it, matching `subgraph`'s CTE seed rows), and a
    // BFS frontier of only the nodes newly reached at the previous hop, so `depth` bounds the number
    // of hops exactly as the recursive CTE's `depth < ?` does.
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = Vec::new();
    for seed in seeds {
        if reached.insert(seed.clone()) {
            frontier.push(seed.clone());
        }
    }
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
            // A neighborhood / drill edge is never a directed-call back edge (spec 52 c4).
            back: false,
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
                // A plain neighborhood node carries no directed-call layer or frontier (spec 52 c4).
                layer: None,
                frontier: None,
                // A plain neighborhood is not a lens fold, so no node is a shared concept member.
                shared: false,
            }
        })
        .collect();

    Neighborhood {
        seed: echo_seed.to_string(),
        depth,
        nodes,
        edges,
        // A plain seeded neighborhood carries no query path; the route fills it when given `from`/`to`.
        path: Vec::new(),
        // The seed's provenance (spec 30 c7); the route fills it from `explain`, absent by default.
        explain: None,
        // A seeded neighborhood is a COMPLETE node set (never capped); only `cluster_detail` sets this.
        truncated: None,
        // A plain neighborhood is not a directed-call view (spec 52 c4): no direction, no
        // referenced-but-not-called sidecar. Absent, these keep the neighborhood byte-identical.
        dir: None,
        referenced_not_called: Vec::new(),
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

/// The RATIONALE of a single node (spec 55, the rationale overlay data path): the decisions,
/// findings, and lessons attached to `node` through the live knowledge edges - a `decision` that
/// `GOVERNS` it, or a `finding` / `lesson` that is `ABOUT` it - as CONTENT-only [`RationaleLeaf`]s.
///
/// A leaf is the SOURCE of a currently-valid (`valid_to` unset) edge whose TARGET is `node`, whose
/// relation is [`REL_GOVERNS`] or [`REL_ABOUT`], and whose source node is a [`KIND_DECISION`],
/// [`KIND_FINDING`], or [`KIND_LESSON`]. Both filters are load-bearing:
///   - restricting the relation to `GOVERNS`/`ABOUT` excludes a `SUPERSEDES` edge, so a decision that
///     supersedes `node` is NOT reported as `node`'s rationale;
///   - restricting the kind to decision/finding/lesson excludes a `handbook-rule`, which also reuses
///     `GOVERNS` for its rule-governs-code edge (see [`crate::contextgraph::REL_GOVERNS`]) - the
///     overlay is the dev-loop's design MEMORY (decisions/findings/lessons), not the ingested
///     handbook rules the intent layer carries.
///
/// Leaves are DEDUPED by id and sorted by `(kind, id)` - kind first (so decisions, then findings,
/// then lessons), id within a kind - so the same graph yields a byte-identical list every request
/// (spec 55 "deterministically ordered"). A node with no attached decision/finding/lesson returns an
/// EMPTY vec (the "nodes without rationale return none" case). Pure over the already-projected
/// `graph`, like [`explain`] and the rest of the KG panel.
pub fn node_rationale(graph: &Graph, node: &str) -> Vec<RationaleLeaf> {
    // Collect the SOURCE of every live GOVERNS/ABOUT edge into `node` whose source node is a
    // decision / finding / lesson. Keyed by id in a `BTreeMap` so a leaf reached by two edges (a
    // decision that governs the node twice) is counted once.
    let mut leaves: BTreeMap<String, RationaleLeaf> = BTreeMap::new();
    for e in &graph.edges {
        if e.valid_to.is_some() || e.to != node {
            continue; // only LIVE edges that TARGET this node
        }
        if e.rel != REL_GOVERNS && e.rel != REL_ABOUT {
            continue; // excludes SUPERSEDES / DECIDED / ... - only the rationale attachments
        }
        let Some(src) = graph.nodes.iter().find(|n| n.id == e.from) else {
            continue;
        };
        if src.kind != KIND_DECISION && src.kind != KIND_FINDING && src.kind != KIND_LESSON {
            continue; // excludes a handbook-rule (also GOVERNS) and any other kind
        }
        leaves
            .entry(src.id.clone())
            .or_insert_with(|| RationaleLeaf {
                id: src.id.clone(),
                kind: src.kind.clone(),
                // CONTENT only: the summary attr. A finding's `by`/`unit` are deliberately not read.
                summary: src.attrs.get("summary").cloned().unwrap_or_default(),
            });
    }
    let mut leaves: Vec<RationaleLeaf> = leaves.into_values().collect();
    leaves.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.id.cmp(&b.id)));
    leaves
}

/// The RATIONALE OVERLAY BATCH (spec 55): the per-node rationale for a set of visible `nodes`, in one
/// pass over the graph. The requested ids are DEDUPED and iterated in sorted order (so the response
/// is deterministic regardless of the request's id order or repeats), and only the nodes that carry
/// AT LEAST ONE leaf appear - "the visible nodes that have any" - so the client badges exactly those.
/// Each node's leaves come from [`node_rationale`]. Pure over the already-projected `graph`.
pub fn rationale_batch(graph: &Graph, nodes: &[String]) -> Vec<NodeRationale> {
    // Dedup + deterministically order the requested ids (a `BTreeSet`), then attach each node's
    // leaves, keeping ONLY the nodes that carry any (the client badges only those).
    let requested: BTreeSet<&str> = nodes.iter().map(String::as_str).collect();
    requested
        .into_iter()
        .filter_map(|id| {
            let leaves = node_rationale(graph, id);
            (!leaves.is_empty()).then(|| NodeRationale {
                node: id.to_string(),
                leaves,
            })
        })
        .collect()
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
    requested_seed: &str,
    effective_seeds: &[String],
    depth: i64,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<String, serde_json::Error> {
    // `effective_seeds` is the seed the client asked for (a single-element slice) UNLESS the route
    // re-pointed a run-tree unit click off the (now-absent) unit node onto that unit's content nodes
    // (spec 43); either way the response echoes `requested_seed`, the id the client selected.
    let mut n = neighborhood_of(graph, effective_seeds, requested_seed, depth);
    if let (Some(from), Some(to)) = (from, to) {
        n.path = path(graph, from, to);
    }
    // The seed's provenance (spec 30 c7): the events/decisions that produced the selected node,
    // riding the existing response so `explain(<seed>)` needs no new route param. Absent (omitted)
    // when the seed is not a graph node (a re-pointed unit id is not) - graceful, never an error.
    n.explain = explain(graph, requested_seed);
    serde_json::to_string(&n)
}

// ---------------------------------------------------------------------------
// The DIRECTED-CALL views (spec 52 c4): `/api/graph?view=calls&dir=down|up|both`. A second seeded
// branch beside the neighborhood, dispatching to the store-side directed traversal
// `Projection::calls` through the SAME spec-45 lazy direct-projection provider - never the state
// poll, never a second traversal implementation. The response reuses the [`Neighborhood`] shape with
// the additive `layer`/`frontier`/`back`/`referenced_not_called`/`dir` fields, so the layered
// left-to-right renderer draws it; an absent `view` keeps the neighborhood byte-identical.
// ---------------------------------------------------------------------------

/// The direction of a `view=calls` request (spec 52 c4): the execution path (`Down` - callees),
/// the call sites (`Up` - callers), or the flow through a centered seed (`Both`). Parsed from the
/// `dir=` query param, defaulting to the execution path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallDir {
    Down,
    Up,
    Both,
}

/// Parse the `dir=` query value into a [`CallDir`] (spec 52 c4). `up` and `both` select those
/// directions; every other value - including an absent param and an unrecognized string - defaults
/// to `down` (the execution path, "what does this call"), so the view is always well-defined.
fn parse_call_dir(dir: Option<&str>) -> CallDir {
    match dir {
        Some("up") => CallDir::Up,
        Some("both") => CallDir::Both,
        _ => CallDir::Down,
    }
}

/// Map one directed-traversal [`CallGraph`] node into a [`NeighborhoodNode`] the layered renderer
/// draws (spec 52 c4). `sign` is the LAYER x-ordinate multiplier: `+1` for a DOWN (callee) walk so
/// the seed sits at the LEFT, `-1` for an UP (caller) walk so it sits at the RIGHT; the seed itself
/// is layer 0 either way. The multi-candidate `frontier` marker rides through verbatim, and `degree`
/// is filled later from the merged edge set (a call node is never a god-node - the DAG is drawn by
/// layer, not by hub degree).
fn call_node_view(cn: &crate::contextgraph::CallNode, sign: i64) -> NeighborhoodNode {
    NeighborhoodNode {
        id: cn.node.id.clone(),
        kind: cn.node.kind.clone(),
        label: node_label(&cn.node),
        degree: 0,
        god: false,
        layer: Some(sign * cn.layer),
        frontier: cn.frontier.clone(),
        // A directed-call node is not a lens fold, so it is never a shared concept member.
        shared: false,
    }
}

/// Map one directed-traversal [`CallGraph`] edge into a [`NeighborhoodEdge`] (spec 52 c4), carrying
/// the recursion `back` marker the renderer draws as a distinct return arc.
fn call_edge_view(ce: &crate::contextgraph::CallEdge) -> NeighborhoodEdge {
    NeighborhoodEdge {
        from: ce.edge.from.clone(),
        to: ce.edge.to.clone(),
        rel: ce.edge.rel.clone(),
        tier: ce.edge.tier.clone(),
        back: ce.back,
    }
}

/// A file that references the seed's name but never calls it (spec 52 c4 - the UP sidecar), as a
/// flat [`NeighborhoodNode`] with no traversal metadata (it is not a walked node - no layer, no
/// frontier, no degree).
fn ref_node_view(n: &Node) -> NeighborhoodNode {
    NeighborhoodNode {
        id: n.id.clone(),
        kind: n.kind.clone(),
        label: node_label(n),
        degree: 0,
        god: false,
        layer: None,
        frontier: None,
        // A referenced-not-called sidecar node is not a lens fold, so never a shared concept member.
        shared: false,
    }
}

/// Build the `view=calls` response body (spec 52 c4) from the directed traversal's `CallGraph`(s) as
/// a [`Neighborhood`]-shaped view the layered left-to-right renderer draws. `down` is the callee walk
/// (present for `dir=down` and `dir=both`), `up` the caller walk (present for `dir=up` and
/// `dir=both`); the direction echoed on the body is inferred from which are present.
///
/// LAYERS are the SIGNED x-ordinate: a callee sits at `+hop` (so a DOWN walk draws the seed at the
/// LEFT), a caller at `-hop` (so an UP walk draws the seed at the RIGHT), the seed at 0 - so a
/// `dir=both` walk lays both flows around ONE centered seed in a SINGLE node array the existing SVG
/// emitter draws with no per-node side flag. When a node appears on BOTH sides (a mutual call), the
/// DOWN/callee placement wins (first-writer), so an id is drawn once; edges dedup by
/// `(from, to, rel)`. Nodes emit in `(layer, id)` order and edges in `(from, to, rel)` order, so the
/// same traversal yields a byte-identical body across polls. The UP `referenced_not_called` sidecar
/// rides through; a DOWN-only walk carries none.
fn calls_view(
    down: Option<&CallGraph>,
    up: Option<&CallGraph>,
    seed: &str,
    depth: i64,
) -> Neighborhood {
    let dir = match (down.is_some(), up.is_some()) {
        (true, true) => "both",
        (false, true) => "up",
        _ => "down",
    };

    // Merge the nodes into one id-keyed map: DOWN (callee, +layer) first so a mutual-call node keeps
    // its callee placement; UP (caller, -layer) fills only ids the DOWN side did not already place.
    let mut node_by_id: BTreeMap<String, NeighborhoodNode> = BTreeMap::new();
    if let Some(cg) = down {
        for cn in &cg.nodes {
            node_by_id
                .entry(cn.node.id.clone())
                .or_insert_with(|| call_node_view(cn, 1));
        }
    }
    if let Some(cg) = up {
        for cn in &cg.nodes {
            node_by_id
                .entry(cn.node.id.clone())
                .or_insert_with(|| call_node_view(cn, -1));
        }
    }

    // Merge the edges, deduped by (from, to, rel) so a mutual call drawn from both walks is one edge.
    let mut edge_by_key: BTreeMap<(String, String, String), NeighborhoodEdge> = BTreeMap::new();
    for cg in [down, up].into_iter().flatten() {
        for ce in &cg.edges {
            edge_by_key
                .entry((
                    ce.edge.from.clone(),
                    ce.edge.to.clone(),
                    ce.edge.rel.clone(),
                ))
                .or_insert_with(|| call_edge_view(ce));
        }
    }
    let edges: Vec<NeighborhoodEdge> = edge_by_key.into_values().collect();

    // The honest in-view degree of each node (incident merged edges, a self-loop once) - the same
    // measure the neighborhood reports, so a node's degree is of what the panel actually draws.
    let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &edges {
        *degree.entry(e.from.as_str()).or_default() += 1;
        if e.to != e.from {
            *degree.entry(e.to.as_str()).or_default() += 1;
        }
    }
    let mut nodes: Vec<NeighborhoodNode> = node_by_id.into_values().collect();
    for n in &mut nodes {
        n.degree = degree.get(n.id.as_str()).copied().unwrap_or(0);
    }
    // Emit in (layer, id) order: the renderer places x by layer, and a stable order keeps the body
    // byte-identical across polls. A call node always carries a layer, so the unwrap_or is unreached.
    nodes.sort_by(|a, b| {
        a.layer
            .unwrap_or(0)
            .cmp(&b.layer.unwrap_or(0))
            .then_with(|| a.id.cmp(&b.id))
    });

    // The "referenced but not called" sidecar is an UP-direction concept; carry it from the UP walk,
    // as flat FILE nodes (already sorted by id by the traversal).
    let referenced_not_called: Vec<NeighborhoodNode> = up
        .map(|cg| cg.referenced_not_called.iter().map(ref_node_view).collect())
        .unwrap_or_default();

    Neighborhood {
        seed: seed.to_string(),
        depth,
        nodes,
        edges,
        path: Vec::new(),
        explain: None,
        truncated: None,
        dir: Some(dir.to_string()),
        referenced_not_called,
    }
}

/// Dispatch a `/api/graph?view=calls` request to the store-side directed traversal (spec 52 c4),
/// returning `Some(Response)` for a call view and `None` for every other `/api/graph` request (so
/// the caller falls through to the byte-identical neighborhood / overview / drill path).
///
/// The traversal runs through `calls_provider` - the SAME spec-45 lazy direct-projection provider
/// the whole-graph views use, opened only on a graph request, never on the state poll - so this
/// never materializes a second traversal. `dir=` picks the direction (default `down`); `depth=` is
/// clamped like the neighborhood ([`DEFAULT_GRAPH_DEPTH`] / [`MAX_GRAPH_DEPTH`]); `tier=` is the
/// confidence FLOOR passed straight to the traversal ([`TIER_INFERRED`] by default, excluding the
/// unresolved `ambiguous` tier until the caller opts it in). The `seed` is percent-decoded like the
/// neighborhood seed (a code-entity id carries `::` and `/`); an empty or missing seed degrades to
/// an empty view (the traversal seeds on real nodes only), never an error. `instance` is the
/// spec-50 attach selector threaded to the provider so the walk opens the SELECTED instance's store.
fn calls_route<G>(instance: Option<&str>, target: &str, calls_provider: &G) -> Option<Response>
where
    G: Fn(Option<&str>, &[String], Direction, i64, &str) -> CallGraph,
{
    if query_param(target, "view").map(percent_decode).as_deref() != Some("calls") {
        return None;
    }
    let seed = query_param(target, "seed")
        .map(percent_decode)
        .unwrap_or_default();
    let seeds = [seed.clone()];
    let depth = query_param(target, "depth")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_GRAPH_DEPTH)
        .clamp(0, MAX_GRAPH_DEPTH);
    // The confidence FLOOR: the traversal maps an absent / unrecognized value to the resolvable
    // `inferred` floor itself, so passing the raw `tier=` (or the default) is safe.
    let floor = query_param(target, "tier")
        .map(percent_decode)
        .unwrap_or_else(|| TIER_INFERRED.to_string());
    let dir = parse_call_dir(query_param(target, "dir").map(percent_decode).as_deref());

    let down = matches!(dir, CallDir::Down | CallDir::Both)
        .then(|| calls_provider(instance, &seeds, Direction::Down, depth, &floor));
    let up = matches!(dir, CallDir::Up | CallDir::Both)
        .then(|| calls_provider(instance, &seeds, Direction::Up, depth, &floor));

    let view = calls_view(down.as_ref(), up.as_ref(), &seed, depth);
    // Serializing these plain view DTOs cannot realistically fail; degrade a serialization error to
    // a 500 with the same shape the neighborhood route uses, so the panel never sees a torn body.
    match serde_json::to_string(&view) {
        Ok(body) => Some(Response::json(200, body)),
        Err(e) => Some(Response::text(
            500,
            &format!("dash: calls projection failed: {e}"),
        )),
    }
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

/// The node ids to seed the RUN-SCOPED context subgraph the dash pre-fetches on open: every decision
/// and finding the run produced, plus the files those decisions GOVERN and those findings are ABOUT.
/// De-noise (spec 43): the graph no longer carries a KIND_UNIT node, so a unit-id seed would land
/// nowhere; this pre-fetch therefore enumerates the content and file nodes the run actually produced
/// (which remain in the graph). Seeding by the ids the run actually produced (rather than a
/// blast-radius file walk) lets the subgraph return their authoritative nodes and the valid
/// SUPERSEDES edges among them at a shallow depth, independent of whether the run emitted the file
/// edges that connect them. The per-UNIT click re-point (a run-tree unit click lands on that unit's
/// content) is the route's job, via [`repoint_seed`] / [`unit_seeds`] over this same pre-fetched
/// graph, so both seed views share ONE derivation ([`event_seed_ids`]) and never drift.
pub fn graph_seeds(events: &[Event]) -> Vec<String> {
    let mut seeds: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in events {
        for id in event_seed_ids(e) {
            seeds.insert(id);
        }
    }
    seeds.into_iter().collect()
}

/// The seed ids a single content event contributes to a context subgraph: the content node's own
/// id (a decision / finding) plus the files it GOVERNS / is ABOUT. De-noise (spec 43): a unit is no
/// longer a graph node, so a content event contributes its content id and the files it concerns,
/// never a unit id. The ONE derivation both the run-scoped [`graph_seeds`] pre-fetch and the
/// unit-scoped [`unit_seeds`] click re-point share, so the two seed views never drift.
fn event_seed_ids(e: &Event) -> Vec<String> {
    use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_REVIEW_FINDING};
    let files_key = match e.type_.as_str() {
        TYPE_DECISION_MADE => "governs",
        TYPE_REVIEW_FINDING => "about",
        _ => return Vec::new(),
    };
    let mut ids: Vec<String> = Vec::new();
    // The content node id (the decision / finding itself) - always a graph node.
    if let Some(id) = field_str(e, "id") {
        if !id.is_empty() {
            ids.push(id);
        }
    }
    // The files it concerns - the code the unit produced. A raw path that never became a canonical
    // node contributes nothing when a consumer filters to real nodes, so it is harmless; the file
    // is reached anyway via the content node's GOVERNS / ABOUT edge.
    for f in field_str_array(e, files_key) {
        if !f.is_empty() {
            ids.push(f);
        }
    }
    ids
}

/// The seed ids for ONE unit's content (spec 43, the run-tree click-to-seed re-point): the
/// decisions that unit's agents made and the findings drawn about it, plus the files each concerns.
/// A unit is no longer a graph node, so the run-tree's click - which passes the unit id - must
/// re-point onto these content nodes (which remain). BOTH a `DecisionMade` and a `ReviewFinding` are
/// attributed to their unit the SAME single way: by the emitting spawn stamped in `meta`
/// (`spawn::unit_of` of the `META_SPAWN` value, exactly the id `rigger emit --spawn` records - a
/// reviewer emits its finding through that same path, so the stamp is always present). As production
/// emits it a finding carries NO `unit` event field; the `$.unit` disposition-expiry keys on is the
/// finding NODE's attribute the adjudication fold stamps (and the integration fold reads to expire
/// it), not a field on the raw event. Deterministically ordered (a sorted set).
pub fn unit_seeds(events: &[Event], unit: &str) -> Vec<String> {
    use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_REVIEW_FINDING};
    let mut seeds: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in events {
        // A decision and a finding are both content the unit's spawns emitted, so both attribute to
        // the unit by their emitting spawn's `meta.spawn` - one derivation, never a second parallel
        // one keyed on an event field production does not carry.
        let belongs = match e.type_.as_str() {
            TYPE_DECISION_MADE | TYPE_REVIEW_FINDING => {
                e.meta
                    .get(crate::conductor::META_SPAWN)
                    .and_then(|s| crate::spawn::unit_of(s))
                    == Some(unit)
            }
            _ => false,
        };
        if belongs {
            for id in event_seed_ids(e) {
                seeds.insert(id);
            }
        }
    }
    seeds.into_iter().collect()
}

/// Resolve a `/api/graph` seed to the EFFECTIVE seed set the neighborhood BFS walks (spec 43, the
/// click-to-seed re-point):
///
/// - A seed that IS a node in the pre-fetched `graph` (a content or code node the operator clicked)
///   is returned unchanged - the spec 30 seeded panel, so a normal node click never regresses.
/// - A seed that is NOT a node is a run-tree UNIT click (the unit node was de-noised away, spec 43):
///   re-point it onto that unit's content nodes ([`unit_seeds`]), keeping ONLY the ones that are
///   real nodes in the graph. Filtering to present nodes is the canonicalization guard
///   (arch-u43c1-graphseeds-raw-vs-canonical-path): a raw, uncanonicalized file path that never
///   became a node is dropped rather than seeded best-effort, and its file is still reached through
///   the content node's GOVERNS / ABOUT edge.
/// - When that yields nothing (a genuinely unknown seed, no unit content in the graph), fall back to
///   the seed itself so the neighborhood degrades to the graceful empty the panel already handles.
pub fn repoint_seed(events: &[Event], graph: &Graph, seed: &str) -> Vec<String> {
    if graph.nodes.iter().any(|n| n.id == seed) {
        return vec![seed.to_string()];
    }
    let repointed: Vec<String> = unit_seeds(events, seed)
        .into_iter()
        .filter(|id| graph.nodes.iter().any(|n| &n.id == id))
        .collect();
    if repointed.is_empty() {
        vec![seed.to_string()]
    } else {
        repointed
    }
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

/// Read a top-level array-of-strings field from an event's JSON payload (best-effort). An absent
/// field, a non-array value, or non-string elements yield an empty vec.
fn field_str_array(e: &Event, key: &str) -> Vec<String> {
    serde_json::from_slice::<serde_json::Value>(&e.data)
        .ok()
        .and_then(|v| v.get(key).cloned())
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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
    /// the body ends at the connection close (no keep-alive bookkeeping). Every response
    /// carries the [`DASH_HEADER`] marker so a second `rigger dash` invocation can recognize
    /// an already-serving singleton on the port (spec 50, criterion 1), AND the
    /// [`DASH_HEADER_PID`] marker naming THIS process's own pid (`std::process::id()`, read
    /// fresh at write time - never plumbed in from outside) so a caller can learn WHO is
    /// actually serving, not merely THAT something is (spec 62 round 2:
    /// adv-u62c1-marker-pid-not-the-serving-pid-on-singleton-race).
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             {}: {}\r\n{}: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len(),
            DASH_HEADER,
            env!("CARGO_PKG_VERSION"),
            DASH_HEADER_PID,
            std::process::id(),
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
    instances: &[InstanceView],
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
        // The LANDING list (spec 50, criterion 3): every registered rigger instance the operator
        // can attach to, read from the machine-global registry by the server's `instances`
        // provider. A registry projection, independent of any single instance's store - so it
        // serves even before this dash's own run has created a store.
        "/api/instances" => Response::json(200, instances_json(instances)),
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
        // The overview and drill bucket key is the pluggable `lens=` (spec 53 c4): `lens=code` folds
        // by coupling community at `resolution=` (default grain otherwise); an absent / other `lens`
        // is `Lens::Files`, byte-identical to today. The lens rides only the overview and drill (both
        // are whole-graph folds); the seeded neighborhood is a walk from a single node and takes none.
        // The seed branch is verbatim spec 30: `seed` is percent-decoded (the client encodes an id that
        // may carry `#` / `::` / `/`); `depth` defaults to two hops and is clamped so a hostile value
        // cannot make the walk churn; `tier=` is accepted but NOT filtered here (the neighborhood ships
        // every edge TIER-TAGGED and the c7 tier filter partitions visibility CLIENT-side over those
        // tags, per d30-tier-param-ownership); `from=`/`to=` (spec 30 c6) select two nodes whose
        // shortest QUERY-PATH rides the body when BOTH are present; and the body carries the seed's
        // EXPLAIN provenance (spec 30 c7), all built by `graph_json` over the neighborhood.
        "/api/graph" => {
            // The RATIONALE OVERLAY batch (spec 55): `explain=<id>[,<id>...]` returns the decisions,
            // findings, and lessons attached to each requested node (content only, deterministically
            // ordered), for the visible nodes that carry any - the overlay's data path, in ONE
            // request. A distinct response shape from the neighborhood / overview / drill below,
            // served over the SAME lazy whole-graph provider this arm already reads (never the state
            // poll). The ids are comma-separated, each `encodeURIComponent`d by the client (an id
            // carries `#` / `::` / `/`), so the list is split on `,` FIRST and each piece is
            // percent-decoded. Checked before the lens/seed dispatch, and taken ONLY when `explain=`
            // is present, so every existing `/api/graph` view stays byte-identical.
            if let Some(raw_explain) = query_param(target, "explain") {
                let ids: Vec<String> = raw_explain
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(percent_decode)
                    .collect();
                let batch = RationaleBatch {
                    nodes: rationale_batch(graph, &ids),
                };
                return match serde_json::to_string(&batch) {
                    Ok(body) => Response::json(200, body),
                    Err(e) => {
                        Response::text(500, &format!("dash: rationale projection failed: {e}"))
                    }
                };
            }
            // The overview/drill bucket lens (spec 53 c4), resolved from `lens=` + `resolution=`.
            // Absent / unknown `lens` -> `Lens::Files` (byte-identical), so a spec-30/42 request is
            // untouched. The seeded neighborhood below is lens-independent and ignores it.
            let lens = Lens::from_query(
                query_param(target, "lens").map(percent_decode).as_deref(),
                query_param(target, "resolution")
                    .map(percent_decode)
                    .as_deref(),
            );
            let body = if let Some(raw_cluster) = query_param(target, "cluster") {
                serde_json::to_string(&cluster_detail(graph, &percent_decode(raw_cluster), &lens))
            } else {
                let seed = query_param(target, "seed")
                    .map(percent_decode)
                    .unwrap_or_default();
                if seed.is_empty() {
                    serde_json::to_string(&clustered_overview(graph, &lens))
                } else if query_param(target, "lens").is_some() {
                    // SUBJECT x LENS re-projection (spec 55 c1): a non-empty seed WITH an explicit
                    // `lens=` re-grains THAT subject's member set at the chosen altitude, in place -
                    // NOT a whole-graph overview and NOT the seeded neighborhood. The composition
                    // fires only when a lens is explicitly present, so a lens-ABSENT seed request
                    // stays the byte-identical spec-30 seeded neighborhood below (spec 55 c4's
                    // composition-absent back-compat).
                    serde_json::to_string(&reproject(graph, &seed, &lens))
                } else {
                    let depth = query_param(target, "depth")
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(DEFAULT_GRAPH_DEPTH)
                        .clamp(0, MAX_GRAPH_DEPTH);
                    let from = query_param(target, "from").map(percent_decode);
                    let to = query_param(target, "to").map(percent_decode);
                    // De-noise (spec 43): a run-tree unit click passes a unit id, which is no longer
                    // a graph node. Re-point it onto that unit's content nodes so the click lands on
                    // a real neighborhood; a seed that already resolves to a node is unchanged.
                    let seeds = repoint_seed(events, graph, &seed);
                    graph_json(graph, &seed, &seeds, depth, from.as_deref(), to.as_deref())
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
/// Two providers, split by cadence (spec 45, criterion 1): `provider` yields the cheap
/// run-scoped inputs (events, the run-seeded graph, progress, liveness) every `/api/*`
/// request rides - including the 1.5s state poll - while `graph_provider` opens the
/// projection and reads the whole graph LAZILY, consulted ONLY on a `/api/graph` request.
/// So the state poll never triggers a whole-graph read; the overview/drill/neighborhood
/// views read the projection directly through their own provider.
///
/// One connection at a time, synchronously: loopback single-operator traffic needs no
/// concurrency, and a serial loop keeps the sqlite reads and the whole server free of any
/// async runtime. Only the `/api/*` paths consult a provider; the static page and the
/// method/not-found guards need no store read, so the page still serves before a run has
/// created the store.
// The four injected providers plus the three run-context values put this one over clippy's
// arg-count altitude (as the `route` dispatch already is); the providers are the composition
// root's concretions and belong at this seam, so the lint is allowed rather than the seam bundled.
#[allow(clippy::too_many_arguments)]
pub fn serve<F, G, H, I>(
    addr: SocketAddr,
    provider: F,
    graph_provider: G,
    calls_provider: I,
    instances_provider: H,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> io::Result<()>
where
    F: Fn(Option<&str>) -> Result<DashInputs, String>,
    G: Fn(Option<&str>) -> Graph,
    H: Fn() -> Vec<InstanceView>,
    I: Fn(Option<&str>, &[String], Direction, i64, &str) -> CallGraph,
{
    let listener = TcpListener::bind(addr)?;
    serve_on(
        listener,
        provider,
        graph_provider,
        calls_provider,
        instances_provider,
        configured_max_retries,
        run_branch,
        base,
    )
}

/// Serve the dash on an ALREADY-BOUND `listener` - the singleton-aware entrypoint (spec 50,
/// criterion 1). `cmd_dash` binds the fixed address itself via [`bind_singleton`] (so the
/// AddrInUse that decides the singleton short-circuit is seen BEFORE this accept loop), then
/// hands the bound listener here. [`serve`] is the thin wrapper that binds `addr` and delegates,
/// preserving its existing bind-internally contract for callers that pass an address.
///
/// Identical serving semantics to [`serve`]: one connection at a time over the same
/// cadence-split providers, re-reading fresh projection inputs each request.
///
/// The ATTACH flow (spec 50, criterion 3) rides the SAME per-request providers: a request
/// carrying `?instance=<id>` is served against THAT registered instance's stores (the
/// providers open them read-only per request); an absent selector keeps serving the dash's own
/// local project (backward compatible). The `instances_provider` reads the machine-global
/// registry for the `/api/instances` landing list.
#[allow(clippy::too_many_arguments)]
pub fn serve_on<F, G, H, I>(
    listener: TcpListener,
    provider: F,
    graph_provider: G,
    calls_provider: I,
    instances_provider: H,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> io::Result<()>
where
    F: Fn(Option<&str>) -> Result<DashInputs, String>,
    G: Fn(Option<&str>) -> Graph,
    H: Fn() -> Vec<InstanceView>,
    I: Fn(Option<&str>, &[String], Direction, i64, &str) -> CallGraph,
{
    let bound = listener.local_addr()?;
    eprintln!("rigger dash: serving on http://{bound}/ (read-only; Ctrl-C to stop)");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_conn(
                    s,
                    &provider,
                    &graph_provider,
                    &calls_provider,
                    &instances_provider,
                    configured_max_retries,
                    run_branch,
                    base,
                ) {
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
///
/// The graph is sourced by cadence (spec 45, criterion 1): every `/api/*` path reads the
/// cheap run-scoped inputs from `provider`, but a `/api/graph` request additionally opens
/// the whole-graph projection through `graph_provider` (consulted HERE and nowhere else),
/// so the state poll never rides a whole-graph read.
///
/// Which STORE the run/graph providers read is chosen HERE from the request's `?instance=<id>`
/// selector (spec 50, criterion 3): present, they open that registered instance's stores;
/// absent, the dash's own local project. `/api/instances` is served from the separate
/// `instances_provider` (the registry landing) and needs no store read at all.
#[allow(clippy::too_many_arguments)]
fn handle_conn<F, G, H, I>(
    stream: TcpStream,
    provider: &F,
    graph_provider: &G,
    calls_provider: &I,
    instances_provider: &H,
    configured_max_retries: u32,
    run_branch: &str,
    base: &str,
) -> io::Result<()>
where
    F: Fn(Option<&str>) -> Result<DashInputs, String>,
    G: Fn(Option<&str>) -> Graph,
    H: Fn() -> Vec<InstanceView>,
    I: Fn(Option<&str>, &[String], Direction, i64, &str) -> CallGraph,
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
            let path = target.split('?').next().unwrap_or(&target);
            // The selected instance to ATTACH to (spec 50, criterion 3), decoded like a seed id
            // (the id is opaque). Absent/empty means the dash's own local project. Threaded to the
            // store providers so a per-request open lands on the right instance's stores.
            let instance = query_param(&target, "instance").map(percent_decode);
            let instance = instance.as_deref().filter(|s| !s.is_empty());
            if method == "GET" && path == "/api/instances" {
                // The LANDING list is a registry projection - no per-instance store read.
                let instances = instances_provider();
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
                    &instances,
                )
            } else if method == "GET" && target.starts_with("/api/") {
                // The DIRECTED-CALL views (spec 52 c4) dispatch to the store-side traversal through
                // the SAME lazy provider - checked BEFORE the polled read so a `view=calls` request
                // opens only the calls provider, never the whole-graph read. `calls_route` returns
                // `None` for every other `/api/graph` request (and every non-graph path), so those
                // fall through to the byte-identical neighborhood / overview / drill path below.
                if let Some(resp) = (path == "/api/graph")
                    .then(|| calls_route(instance, &target, calls_provider))
                    .flatten()
                {
                    resp
                } else {
                    match provider(instance) {
                        Ok((events, polled_graph, progress, liveness)) => {
                            // The whole-graph views (only `/api/graph`) read the projection through
                            // the SEPARATE lazy provider, opened HERE and never on the state poll;
                            // every other `/api/*` path keeps the cheap run-seeded graph the polled
                            // provider yields (spec 45, criterion 1). Both open the SELECTED
                            // instance's store (spec 50, criterion 3).
                            let graph = if path == "/api/graph" {
                                graph_provider(instance)
                            } else {
                                polled_graph
                            };
                            route(
                                &method,
                                &target,
                                &events,
                                &graph,
                                &progress,
                                &liveness,
                                configured_max_retries,
                                run_branch,
                                base,
                                &[],
                            )
                        }
                        Err(e) => {
                            Response::text(500, &format!("dash: reading the store failed: {e}"))
                        }
                    }
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
                    &[],
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
        Edge, Node, KIND_AGENT, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_DESIGN_DOC, KIND_FILE,
        KIND_RATIONALE, KIND_UNIT, REL_CALLS, REL_DECIDED, REL_GOVERNS, REL_IN_COMMUNITY,
        REL_REFERENCES, TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED,
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

    fn local_instance(project: &str, root: &str, hb: u64) -> crate::registry::Instance {
        crate::registry::Instance {
            project: project.to_string(),
            root: root.to_string(),
            store: crate::registry::StoreIdentity::Local {
                path: format!("{root}/.rigger/events.db"),
            },
            heartbeat_ms: hb,
        }
    }

    /// The landing projection (spec 50 c3): registry entries become sorted, credential-free
    /// [`InstanceView`] rows. Order is deterministic (by project then root) regardless of the
    /// registry's filesystem order, the `id` round-trips the registry entry's stable id (so the
    /// client can echo it back on `?instance=`), and a shared endpoint is carried VERBATIM from
    /// the already-redacted registry - the view never re-derives a label from a raw connection.
    #[test]
    fn instance_views_project_a_sorted_credential_free_landing() {
        let shared = crate::registry::Instance {
            project: "alpha".to_string(),
            root: "/home/dev/alpha".to_string(),
            // Already redacted at registration - the view must carry exactly this, no credential.
            store: crate::registry::StoreIdentity::Shared {
                endpoint: "kurrentdb://db.example:2113".to_string(),
            },
            heartbeat_ms: 4_000,
        };
        // Registry order is unspecified; hand them in reverse of the expected sort.
        let insts = vec![
            local_instance("beta", "/home/dev/beta", 5_000),
            shared.clone(),
        ];
        let views = instance_views(&insts, 9_000);

        assert_eq!(
            views.iter().map(|v| v.project.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "beta"],
            "the landing is sorted by project, not by the registry's filesystem order"
        );
        let a = &views[0];
        assert_eq!(
            a.id,
            shared.id(),
            "the id round-trips the registry entry id"
        );
        assert_eq!(a.kind, "shared");
        assert_eq!(
            a.store, "kurrentdb://db.example:2113",
            "the shared endpoint is carried verbatim from the redacted registry entry"
        );
        assert_eq!(a.age_secs, 5, "age is (now - heartbeat) in whole seconds");
        let b = &views[1];
        assert_eq!(b.kind, "local");
        assert!(
            b.store.ends_with("/.rigger/events.db"),
            "a local instance labels with its sqlite path: {}",
            b.store
        );

        // Not one field of the serialized landing carries a credential fragment.
        let body = instances_json(&views);
        for secret in ["password", "admin", "hunter2", "user:"] {
            assert!(
                !body.contains(secret),
                "landing must be credential-free: {body}"
            );
        }
    }

    /// The landing's freshness clock never goes negative (spec 50 c3): under clock skew an instance's
    /// heartbeat stamp can be AHEAD of the reader's `now`, and `age_secs` is a `saturating_sub`, so it
    /// FLOORS at 0 (the "live" sentinel the page renders) rather than underflowing. The sorted-landing
    /// test only exercises PAST heartbeats; this pins the future-heartbeat edge.
    #[test]
    fn instance_view_age_floors_at_zero_for_a_future_heartbeat() {
        // heartbeat_ms strictly AFTER now (now=1_000ms, heartbeat=9_000ms - 8s in the future).
        let insts = vec![local_instance("alpha", "/home/dev/alpha", 9_000)];
        let views = instance_views(&insts, 1_000);
        assert_eq!(
            views[0].age_secs, 0,
            "a future heartbeat floors age at 0 (saturating_sub), never an underflow"
        );
    }

    /// `/api/instances` (spec 50 c3): the landing route renders the supplied instance list as a
    /// JSON array the page reads to populate its instance picker. It is a GET-only read like every
    /// other `/api/*` path, and needs no run/graph inputs (they are empty here) - the landing is a
    /// registry projection, independent of any single instance's store.
    #[test]
    fn api_instances_route_renders_the_landing_list() {
        let views = instance_views(
            &[
                local_instance("alpha", "/home/dev/alpha", 1_000),
                local_instance("beta", "/home/dev/beta", 1_000),
            ],
            1_000,
        );
        let r = route(
            "GET",
            "/api/instances",
            &[],
            &Graph::default(),
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
            &views,
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "application/json");
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("\"instances\""), "wraps the list: {body}");
        assert!(
            body.contains("alpha") && body.contains("beta"),
            "lists every registered instance: {body}"
        );
        assert!(
            body.contains(&views[0].id),
            "carries the attach selector id: {body}"
        );
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
            &[],
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

    /// Spec 50, criterion 1 (fixed address, no free-port search): `bind_singleton` binds the
    /// EXACT requested address when it is free, and NEVER drifts to another port when the port
    /// is held by an UNRELATED (non-dash) process - that is a genuine conflict surfaced as an
    /// `AddrInUse` error, the deliberate opposite of `free_port_from`'s search-upward behavior.
    #[test]
    fn bind_singleton_binds_the_exact_port_and_never_searches() {
        // A free ephemeral port (learn it, release it): `bind_singleton` returns `Bound` on
        // exactly that address, never a drifted one. The learn-release-rebind window is a
        // TOCTOU under a busy machine (a live dash's poll churn recycles ephemeral ports fast
        // enough to steal the freed port), so interference retries with a FRESH port - the
        // assertion is about bind_singleton's behavior on a genuinely free port, not about
        // winning an OS port race.
        let mut bound_ok = false;
        for _ in 0..16 {
            let free = TcpListener::bind(("127.0.0.1", 0))
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            let addr = SocketAddr::from(([127, 0, 0, 1], free));
            match bind_singleton(addr) {
                Ok(SingletonBind::Bound(listener)) => {
                    assert_eq!(
                        listener.local_addr().unwrap().port(),
                        free,
                        "bind_singleton must bind the EXACT requested port, never drift to another"
                    );
                    bound_ok = true;
                    break;
                }
                // The freed port was re-taken in the race window - not our behavior under
                // test; learn a fresh port and try again.
                Err(e) if e.kind() == io::ErrorKind::AddrInUse => continue,
                other => panic!("a free port must yield Bound on that exact port, got {other:?}"),
            }
        }
        assert!(
            bound_ok,
            "16 straight ephemeral-port races is not interference; investigate"
        );

        // A port HELD by an unrelated listener that never emits the dash header: a genuine
        // conflict -> AddrInUse, never a silent drift and never a false AlreadyServing.
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let held_addr = SocketAddr::from(([127, 0, 0, 1], held.local_addr().unwrap().port()));
        match bind_singleton(held_addr) {
            Err(e) => assert_eq!(
                e.kind(),
                io::ErrorKind::AddrInUse,
                "a non-dash holder is a genuine conflict surfaced as AddrInUse, got {e:?}"
            ),
            Ok(other) => panic!("a non-dash holder must be a genuine conflict, not {other:?}"),
        }
        drop(held);
    }

    /// Spec 50, criterion 1 (singleton): when a rigger dash is ALREADY serving the address,
    /// `bind_singleton` short-circuits to `AlreadyServing(addr)` - recognizing it by the
    /// [`DASH_HEADER`] response header - instead of binding a second port. This is the behavior
    /// a second `rigger dash` invocation relies on to report the existing address and exit clean.
    /// Serialized with the other real-serving dash tests (the spec-44 discipline): this test
    /// brings a REAL dash up and polls it ready, and under a fully parallel suite the readiness
    /// window flakes on load - one dash-serving test at a time keeps the probe deterministic.
    #[test]
    #[serial_test::serial(dash_default_port)]
    fn bind_singleton_short_circuits_on_an_already_serving_rigger_dash() {
        use std::sync::mpsc;
        use std::time::Instant;

        // Bring a REAL dash up on an ephemeral port and wait until it answers as a dash.
        //
        // The port is discovered by binding it and is HELD by that same listener all the way into
        // `serve_on` - it is never released and re-bound. Discovering a port by binding it,
        // DROPPING the listener and re-binding the number is a time-of-check/time-of-use race:
        // in the window between the drop and the re-bind, any `bind(0)` on the machine (a sibling
        // test, or a second agent running this same suite in another worktree) can be handed the
        // just-freed port. `serve` then fails AddrInUse and this test, which only ever observes
        // the port, burns its whole deadline and reports a misleading "never came up".
        // `serve_on` is the same race-free seam production uses: `bind_singleton` returns the
        // listener it bound (`SingletonBind::Bound`) and the caller serves on THAT listener.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();
        let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
            Ok((Vec::new(), Graph::default(), Vec::new(), HashMap::new()))
        };
        let graph_provider = |_instance: Option<&str>| Graph::default();
        let instances_provider = Vec::new;
        let calls_provider =
            |_: Option<&str>, _: &[String], _: crate::contextgraph::Direction, _: i64, _: &str| {
                crate::contextgraph::CallGraph::default()
            };
        // A dash that FAILS instead of serving reports its error here, so the failure surfaces
        // LOUD and named (spec 19c) rather than as a silent stall that only shows up much later
        // as an unexplained deadline.
        let (serve_failed, serve_failure) = mpsc::channel();
        std::thread::spawn(move || {
            if let Err(e) = serve_on(
                listener,
                provider,
                graph_provider,
                calls_provider,
                instances_provider,
                3,
                "rigger-run",
                "origin/main",
            ) {
                let _ = serve_failed.send(e.to_string());
            }
        });

        // Condition-based readiness with a LOAD-PROOF bound: under the fully parallel suite
        // (863 tests saturating every core) the dash thread can starve for many seconds before
        // it answers, and a tight deadline flakes the whole lane. 60s is a bound on brokenness,
        // not an expectation - the loop exits the moment the dash answers (typically <100ms).
        let deadline = Instant::now() + Duration::from_secs(60);
        while !dash_serving_on(port) {
            if let Ok(e) = serve_failure.try_recv() {
                panic!("the dash on port {port} failed instead of serving: {e}");
            }
            assert!(
                Instant::now() < deadline,
                "the dash never came up on port {port} within the deadline"
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        // The port is now held by a genuine rigger dash: bind_singleton must NOT bind a second
        // one - it reports the existing address.
        match bind_singleton(addr) {
            Ok(SingletonBind::AlreadyServing(reported)) => assert_eq!(
                reported, addr,
                "the reported address must be the fixed address the singleton already serves"
            ),
            other => {
                panic!("an already-serving rigger dash must short-circuit to AlreadyServing, got {other:?}")
            }
        }
    }

    /// Spec 50, criterion 1: `dash_serving_on` recognizes ONLY a rigger dash (by its
    /// [`DASH_HEADER`]). A raw listener that answers WITHOUT that header is not mistaken for a
    /// dash, so a genuine conflict with an unrelated process is never swallowed as a false
    /// singleton short-circuit.
    #[test]
    fn dash_serving_on_is_false_for_a_non_dash_listener() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                // A well-formed HTTP reply that carries NO dash header - any unrelated
                // process holding the port.
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi",
                );
            }
        });
        assert!(
            !dash_serving_on(port),
            "a non-dash listener must not be recognized as a rigger dash"
        );
    }

    /// Spec 50, criterion 1 + spec 19c (a hang always surfaces loud, never a stall): `dash_serving_on`
    /// must be bounded in WALL CLOCK against a holder that DRIBBLES bytes - one byte slower than any
    /// per-read timeout while NEVER sending a newline. A probe that bounds only each `read()` (so every
    /// byte resets the timeout) and caps only LINES would spin forever here: `read_line` never returns
    /// (no `\n`) so the line cap never fires. The probe carries an OVERALL deadline and a total-byte
    /// cap, so it returns `false` within a hard bound. The probe runs on a worker thread guarded by a
    /// `recv_timeout`, so a regression to the unbounded loop fails LOUD (the recv times out) instead of
    /// hanging the whole suite.
    #[test]
    fn dash_serving_on_is_bounded_against_a_byte_dribbling_holder() {
        use std::sync::mpsc;
        use std::time::Instant;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                // A plausible status-line start, then an endless newline-less dribble: one byte
                // every 100ms, faster than any single per-read timeout expires but never a `\n`.
                if s.write_all(b"HTTP/1.1 200 OK\r\nX-Filler: ").is_err() {
                    continue;
                }
                let _ = s.flush();
                loop {
                    if s.write_all(b"a").is_err() || s.flush().is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let start = Instant::now();
            let served = dash_serving_on(port);
            let _ = tx.send((served, start.elapsed()));
        });
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok((served, elapsed)) => {
                assert!(
                    !served,
                    "a dribbling non-dash holder must never be recognized as a rigger dash"
                );
                assert!(
                    elapsed < Duration::from_secs(2),
                    "dash_serving_on must be bounded against a dribbler; it took {elapsed:?}"
                );
            }
            Err(_) => panic!(
                "dash_serving_on HUNG against a byte-dribbling holder - it never returned within 2s \
                 (the per-read-only timeout never bounds a newline-less dribble)"
            ),
        }
    }

    /// Spec 62 round 3 mutation-efficacy follow-up (probe_dash_head extraction,
    /// arch-u62c1-dash-serving-pid-on-duplicates-the-probe-read-loop): `probe_dash_head`'s
    /// `stop_early(&head) || head.len() >= MAX_HEAD_BYTES || head_block_ended(&head)` must fire
    /// `dash_serving_on`'s early exit the INSTANT `stop_early` (the `DASH_HEADER` match) is true,
    /// regardless of the other two conditions - a `||` -> `&&` flip on the FIRST operator ties
    /// the early exit to `head.len() >= MAX_HEAD_BYTES` too (virtually never true for a small
    /// response), silently falling back to waiting for `head_block_ended` (or the deadline) on
    /// every real dash. `dash_serving_on_is_bounded_against_a_byte_dribbling_holder` cannot catch
    /// this: its holder never sends `DASH_HEADER` at all, so `stop_early` is false there under
    /// EITHER version - the flip is invisible unless a holder sends the header FIRST and then
    /// never completes the header block, isolating whether recognition happens on the header
    /// line itself or only once the whole block (or the deadline) resolves.
    ///
    /// This holder does exactly that: it sends a genuine `DASH_HEADER` line immediately, then
    /// dribbles harmlessly forever WITHOUT ever sending the terminating blank line. Correct code
    /// recognizes the header and returns `true` almost immediately (well under the probe's own
    /// 750ms deadline); the mutated `&&` never short-circuits on a small response, so it falls
    /// through to `head_block_ended` (never true here) and idles out to `false` only once the
    /// full deadline elapses - both distinctly different from "fast `true`", so a generous
    /// wall-clock bound well under the deadline separates them without racing a specific millisecond.
    #[test]
    fn dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block() {
        use std::sync::mpsc;
        use std::time::Instant;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                // The real dash header, sent whole, immediately - then an endless newline-less
                // dribble that never reaches the terminating blank line.
                if s.write_all(format!("HTTP/1.1 200 OK\r\n{DASH_HEADER}: probe\r\n").as_bytes())
                    .is_err()
                {
                    continue;
                }
                let _ = s.flush();
                loop {
                    if s.write_all(b"a").is_err() || s.flush().is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let start = Instant::now();
            let served = dash_serving_on(port);
            let _ = tx.send((served, start.elapsed()));
        });
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok((served, elapsed)) => {
                assert!(
                    served,
                    "a genuine DASH_HEADER line must be recognized even though the holder never \
                     finishes the header block"
                );
                assert!(
                    elapsed < Duration::from_millis(500),
                    "recognizing DASH_HEADER must short-circuit almost immediately, well under \
                     the probe's own 750ms deadline - it took {elapsed:?}, which is only \
                     possible if the early exit degraded into waiting out the block or the \
                     deadline instead"
                );
            }
            Err(_) => panic!(
                "dash_serving_on HUNG against a header-then-dribble holder - it never returned \
                 within 2s"
            ),
        }
    }

    /// Spec 62 round 2 (adv-u62c1-marker-pid-not-the-serving-pid-on-singleton-race):
    /// `dash_serving_pid_on` reports the pid a REAL rigger-dash-shaped response names via
    /// [`DASH_HEADER_PID`] - the whole point being that a caller can learn WHO is actually
    /// serving a port without assuming it is whichever process the caller itself happens to have
    /// spawned. A fake listener stands in for the winner of a singleton race, answering with an
    /// ARBITRARY pid value in the header (never the test process's own pid), so a pass here can
    /// only be explained by the probe reading the header off the wire, not by any coincidental
    /// match with `std::process::id()`.
    #[test]
    fn dash_serving_pid_on_reports_the_pid_a_real_dash_response_names() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let winner_pid: u32 = 424_242; // deliberately NOT this test process's own pid
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\n{DASH_HEADER}: probe\r\n{DASH_HEADER_PID}: \
                         {winner_pid}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                );
            }
        });
        assert_eq!(
            dash_serving_pid_on(port),
            Some(winner_pid),
            "the probe must report the EXACT pid the response names via X-Rigger-Dash-Pid"
        );
    }

    /// Spec 62 round 2 mutation-efficacy follow-up (adv-u62c1-marker-pid-not-the-serving-pid-on-
    /// singleton-race): [`dash_serving_pid_on`]'s read loop has its own local byte-cap constant
    /// and a `head.len() >= <cap>` termination check, both of which matter, not merely
    /// `head_block_ended` - a response whose header block is genuinely LARGER than the
    /// fixed-size read buffer forces multiple `read` calls to assemble, so a wrong cap (too
    /// small, or the comparison direction flipped) truncates the head BEFORE the real
    /// [`DASH_HEADER_PID`] line ever arrives, well short of the block's actual end.
    ///
    /// The response here is ~2KB: status line, then ~2000 bytes of an unrelated padding header
    /// (never matching either needle), THEN the real `DASH_HEADER`/`DASH_HEADER_PID` lines. Two
    /// structural facts make the assertion robust regardless of exact OS-level TCP chunking:
    /// (1) the read loop's own buffer is a fixed 512-byte array, so `Read::read` can never
    /// return more than 512 bytes in one call - reaching the real content (past byte ~2030)
    /// PROVABLY requires at least 4 calls; (2) since 1032 (`8 * 1024` mis-computed as `8 + 1024`
    /// or `8 / 1024`) and the flipped-comparison cap are both far below 2030 while the real cap
    /// (8192) is far above it, a wrong cap is GUARANTEED to trip - monotonically, on whichever
    /// read call first crosses it - strictly before the padding ends, while the correct cap
    /// never trips at all (this response never reaches 8192 bytes) and the loop instead runs to
    /// completion exactly once `head_block_ended` sees the real trailing blank line.
    #[test]
    fn dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let winner_pid: u32 = 424_243; // deliberately NOT this test process's own pid
        let padding = "A".repeat(2000);
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nX-Padding: {padding}\r\n{DASH_HEADER}: \
                         probe\r\n{DASH_HEADER_PID}: {winner_pid}\r\nConnection: \
                         close\r\n\r\n"
                    )
                    .as_bytes(),
                );
            }
        });
        assert_eq!(
            dash_serving_pid_on(port),
            Some(winner_pid),
            "a head spanning many read() calls must still be fully assembled and parsed - \
             a premature length-cap break truncates it before the real header lines arrive"
        );
    }

    /// The false direction, mirroring `dash_serving_on_is_false_for_a_non_dash_listener`: a
    /// listener that answers but carries no [`DASH_HEADER`] at all (an unrelated process holding
    /// the port) must never be mistaken for a dash naming a pid, even if it happens to send a
    /// same-shaped header by coincidence-free construction here (it sends none).
    #[test]
    fn dash_serving_pid_on_is_none_for_a_non_dash_listener() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi",
                );
            }
        });
        assert_eq!(
            dash_serving_pid_on(port),
            None,
            "a non-dash listener must never be reported as naming a serving pid"
        );
    }

    /// Nothing listening at all must resolve to `None`, never a stale or default pid - mirrors
    /// `dash_serving_on`'s false-on-no-connection direction.
    #[test]
    fn dash_serving_pid_on_is_none_when_nothing_answers() {
        let port = free_port_from(45000).expect("a free loopback port must be available");
        assert_eq!(dash_serving_pid_on(port), None);
    }

    /// The sentinel arm of `dash_serving_pid_on`'s own `.parse().ok()` (spec 62 round 2, SDET
    /// lens periphery: neither the mutation-efficacy accounting recorded in
    /// `d-u62c1-mutation-accounting-round2` nor any existing test in this file exercises this
    /// exact path - `cargo-mutants`' default mutator set never touches a `Result::ok()` call on
    /// a std `.parse()`, so this arm is invisible to that tool and only a hand-written test
    /// closes it). A listener that DOES carry a genuine `DASH_HEADER` (so the "is this even a
    /// dash" check at the top of the function passes) but whose `DASH_HEADER_PID` value is not a
    /// valid `u32` must resolve to `None`, never panic and never silently coerce to some other
    /// value (e.g. `0`) - a malformed or truncated pid header must never be reported as a real
    /// pid a caller could act on. A round-2 draft of the one production call site,
    /// `spawn_run_dashboard_detached`, once used `dash_serving_pid_on(port).unwrap_or(pid)` -
    /// this exact `None` was what let that (since-rejected,
    /// adj-u62c1r2-verdict-reject-version-skew-fallback) fallback engage instead of recording a
    /// nonsense pid. The current call site no longer falls back to a guessed pid at all: it
    /// records the documented [`UNATTRIBUTED_PID`] sentinel on this `None` instead - so this
    /// test's lasting job is proving `dash_serving_pid_on` itself never manufactures a value
    /// from unparseable input, regardless of what any caller later does with the `None`.
    #[test]
    fn dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\n{DASH_HEADER}: probe\r\n{DASH_HEADER_PID}: \
                         not-a-number\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                );
            }
        });
        assert_eq!(
            dash_serving_pid_on(port),
            None,
            "a non-numeric X-Rigger-Dash-Pid value must resolve to None, never panic or \
             coerce to a default pid"
        );
    }

    /// Spec 50, criterion 1 (cold-race loser): when two dashes bind the fixed address at once, the
    /// winner binds and the LOSER hits `AddrInUse`. Even when the loser probes during the winner's
    /// bind-THEN-accept window (the port is bound but the winner has not entered its accept loop yet),
    /// the probe's read budget spans that short window, so the loser resolves to a clean
    /// `AlreadyServing` - never the loud `AddrInUse` a genuine unrelated-process conflict raises.
    #[test]
    fn bind_singleton_cold_race_loser_resolves_across_the_accept_window() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        // WINNER: holds the fixed port but only begins accepting/answering after a short delay -
        // the bind-then-accept window a just-bound dash has before its serve loop runs. It then
        // answers as a real dash would, carrying the DASH_HEADER marker.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!("HTTP/1.1 200 OK\r\n{DASH_HEADER}: probe\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                );
            }
        });
        // LOSER: bind_singleton hits AddrInUse (the winner holds the port) and probes. The probe's
        // read budget (well over the 100ms window) sees the header once the winner starts serving,
        // so the loser resolves cleanly rather than surfacing a spurious AddrInUse.
        match bind_singleton(addr) {
            Ok(SingletonBind::AlreadyServing(reported)) => assert_eq!(
                reported, addr,
                "a cold-race loser must resolve to the exact address the winner already serves"
            ),
            other => panic!(
                "a cold-race loser probing inside the winner's accept window must be AlreadyServing, \
                 not {other:?}"
            ),
        }
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

    /// Spec 50 c3, the LANDING VIEW's operator-facing WIRING - the criterion-3 deliverable the
    /// backend endpoint/resolver only ENABLE. The done-when is "the dash's landing view LISTS
    /// registered instances, and SELECTING one serves THAT instance's run and graph views," so the
    /// served page must actually CONSUME the registry: fetch GET `/api/instances`, render the list,
    /// and thread the selected `?instance=<id>` into its store-reading polls. The cli wire-contract
    /// test pins the JSON the endpoint SHIPS; this pins that the page is not left un-wired (the exact
    /// regression that shipped criterion 3 with a complete backend but a dead-ended UI). Structural
    /// string-pins on the live page's JS, mirroring the sibling one-screen/layout page tests.
    #[test]
    fn the_landing_view_lists_instances_and_threads_the_attach_selector() {
        let page = live_page();

        // The page FETCHES the landing list from the registry projection, and renders it into a
        // container, tracking which instance is selected.
        assert!(
            page.contains("fetch(\"/api/instances\""),
            "the page must fetch the landing list from /api/instances"
        );
        assert!(
            page.contains("id=\"instances\"") && page.contains("selectedInstance"),
            "the page must render the instances list into a container and track the selection"
        );
        assert!(
            page.contains("function renderInstances") && page.contains("data-instance"),
            "the page must render selectable instance rows (data-instance carries the attach id)"
        );

        // It THREADS the selected instance into the STORE-READING polls via the apiUrl helper (which
        // appends `instance=<id>`), so selecting an instance switches the run and graph views to it.
        assert!(
            page.contains("function apiUrl") && page.contains("instance=\" + encodeURIComponent"),
            "the page must thread ?instance=<id> onto its store-reading API calls"
        );
        assert!(
            page.contains("apiUrl(\"/api/state\")")
                && page.contains("apiUrl(\"/api/events")
                && page.contains("apiUrl(\"/api/graph"),
            "each store-reading poll (state/events/graph) must go through apiUrl (attach-threaded)"
        );

        // The registry landing itself is NEVER threaded - it lists every instance regardless of the
        // current selection (a threaded /api/instances would only ever show the attached one).
        assert!(
            !page.contains("apiUrl(\"/api/instances"),
            "the /api/instances landing list must not be threaded with the attach selector"
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
            &[],
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

    #[test]
    fn state_counts_grep_fallbacks_and_carries_them_in_the_review_outcomes_data() {
        // Spec 58, criterion 4: `grep-fallback:` progress lines recorded during the run are
        // counted by the metrics projection and carried in the dash's review-outcomes data.
        // The count reads the SEPARATE progress slice `build_state` already threads for the
        // live activity view - not the run stream - so ordinary narration does not count.
        use crate::spawn::SpawnRequest;
        let req = SpawnRequest::new("u", "u", "implementer", 0, "do it");
        let events = positioned(vec![
            ev("UnitStarted", r#"{"id":"u"}"#),
            req.to_event().unwrap(),
        ]);
        let mkprog = |id: &str, activity: &str| {
            let ap = progress::AgentProgress {
                id: id.into(),
                activity: activity.into(),
            };
            Event::new(
                progress::TYPE_AGENT_PROGRESS,
                serde_json::to_vec(&ap).unwrap(),
            )
        };
        let progress_events = vec![
            mkprog(
                &req.id,
                "grep-fallback: no --show for effective_max_retries",
            ),
            mkprog(&req.id, "cargo build green"), // ordinary narration - not counted
            mkprog("u/adversary#0", "grep-fallback: quoting Blocker body"),
        ];
        let liveness = HashMap::new();

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
            state.metrics.grep_fallbacks, 2,
            "the two grep-fallback lines are counted into the review-outcomes data"
        );
        assert_eq!(
            state.metrics.grep_fallbacks,
            metrics::grep_fallbacks(&progress_events),
            "the dash carries exactly the metrics projection's count"
        );

        // And the count serializes into the /api/state body the review-outcomes panel reads.
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
            body.contains("\"grep_fallbacks\":2"),
            "the fallback count appears in the emitted state"
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
                    &[],
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
            &[],
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

        let overview = clustered_overview(&graph, &Lens::Files);

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
                    label: None,
                },
                Cluster {
                    key: "docs".to_string(),
                    count: 3,
                    kind: KIND_DESIGN_DOC.to_string(),
                    label: None,
                },
                Cluster {
                    key: "src".to_string(),
                    count: 2,
                    kind: KIND_CODE_ENTITY.to_string(),
                    label: None,
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
        let big = cluster_detail(&g, "src/big", &Lens::Files);
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
        let small = cluster_detail(&g, "src/small", &Lens::Files);
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
        let decisions = cluster_detail(&g, KIND_DECISION, &Lens::Files);
        assert_eq!(decisions.truncated, None);
        let dec_ids: std::collections::BTreeSet<&str> =
            decisions.nodes.iter().map(|n| n.id.as_str()).collect();
        let want: std::collections::BTreeSet<&str> = ["d-abc", "d-xyz"].into_iter().collect();
        assert_eq!(
            dec_ids, want,
            "a dev-loop KIND drill returns exactly the nodes folding to that kind"
        );

        // --- GRACEFUL: an unknown cluster key drills to an empty result, never a panic ---
        let empty = cluster_detail(&g, "no/such/module", &Lens::Files);
        assert!(empty.nodes.is_empty() && empty.edges.is_empty() && empty.truncated.is_none());
    }

    /// Spec 53 c4 - the CODE LENS VIEW: with `lens=code` the SAME overview/drill folds bucket every
    /// node carrying a live `IN_COMMUNITY` membership by its coupling COMMUNITY (a subsystem grouped
    /// ACROSS directory lines), sizing the community super-node by member count, colouring it by its
    /// dominant member kind, and labelling it with the community node's deterministic `label`;
    /// currently-valid coupling edges that cross two communities weight a symmetric cross-edge (an
    /// intra-community edge and the membership spokes to the excluded super-node add none); a
    /// membership-LESS node keeps its KIND bucket (so the view stays whole-graph); a community drills
    /// to exactly its members; a resolution grain with NO derived assignments returns the documented
    /// empty state (never an error); and `Lens::Files` is byte-identical to the spec-42 directory/kind
    /// fold (no `label`, no `empty_state`). This test OWNS the lens plumbing; it does not own detection
    /// (c1), the grain/supersession (c2), or the fold recording (c3).
    #[test]
    fn code_lens_buckets_members_by_community_keeps_kind_buckets_and_reports_underived_grain() {
        let ce = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: BTreeMap::new(),
        };
        let community = |id: &str, label: &str| Node {
            id: id.to_string(),
            kind: KIND_COMMUNITY.to_string(),
            attrs: BTreeMap::from([("label".to_string(), label.to_string())]),
        };
        let plain = |id: &str, kind: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::new(),
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

        // Two coupling communities, each a pair of code entities in DIFFERENT directories that call
        // each other (proving the grouping crosses directory lines - the whole point of the code
        // lens): community/1/0 = {foo, bar}, community/1/1 = {baz, qux}. Plus the two derived
        // KIND_COMMUNITY super-nodes (each with a deterministic label attr) and two membership-LESS
        // nodes (a decision and a design-doc) that must keep their KIND buckets under the code lens.
        let foo = "src/one/a.rs::foo";
        let bar = "src/two/b.rs::bar";
        let baz = "src/three/c.rs::baz";
        let qux = "src/four/d.rs::qux";
        let graph = Graph {
            nodes: vec![
                ce(foo),
                ce(bar),
                ce(baz),
                ce(qux),
                community("community/1/0", "foo"),
                community("community/1/1", "baz"),
                plain("d1", KIND_DECISION),
                plain("docs/x.md", KIND_DESIGN_DOC),
            ],
            edges: vec![
                // Live memberships at grain 1 (the fold's IN_COMMUNITY spokes).
                edge(foo, "community/1/0", REL_IN_COMMUNITY, TIER_INFERRED),
                edge(bar, "community/1/0", REL_IN_COMMUNITY, TIER_INFERRED),
                edge(baz, "community/1/1", REL_IN_COMMUNITY, TIER_INFERRED),
                edge(qux, "community/1/1", REL_IN_COMMUNITY, TIER_INFERRED),
                // Intra-community coupling (adds NO cross-community weight).
                edge(foo, bar, REL_CALLS, TIER_EXTRACTED),
                edge(baz, qux, REL_CALLS, TIER_EXTRACTED),
                // Cross-community coupling, twice -> one symmetric weight-2 cross edge.
                edge(foo, baz, REL_CALLS, TIER_EXTRACTED),
                edge(bar, qux, REL_CALLS, TIER_EXTRACTED),
            ],
        };

        // --- CODE LENS OVERVIEW at the default grain (resolution "1") ---
        let code = Lens::Code {
            resolution: DEFAULT_COMMUNITY_RESOLUTION.to_string(),
        };
        let overview = clustered_overview(&graph, &code);
        assert_eq!(
            overview.total, 8,
            "total carries every graph node, community super-nodes included"
        );
        assert_eq!(
            overview.empty_state, None,
            "a derived grain is not the empty state"
        );
        assert_eq!(
            overview.clusters,
            vec![
                // Each community super-node: sized by MEMBER count (2), coloured by dominant member
                // kind, labelled by its community node's deterministic `label`. The excluded
                // KIND_COMMUNITY node never inflates the count or the dominant kind.
                Cluster {
                    key: "community/1/0".to_string(),
                    count: 2,
                    kind: KIND_CODE_ENTITY.to_string(),
                    label: Some("foo".to_string()),
                },
                Cluster {
                    key: "community/1/1".to_string(),
                    count: 2,
                    kind: KIND_CODE_ENTITY.to_string(),
                    label: Some("baz".to_string()),
                },
                // Membership-less nodes KEEP their KIND buckets (not their directory buckets), so the
                // code lens stays whole-graph.
                Cluster {
                    key: KIND_DECISION.to_string(),
                    count: 1,
                    kind: KIND_DECISION.to_string(),
                    label: None,
                },
                Cluster {
                    key: KIND_DESIGN_DOC.to_string(),
                    count: 1,
                    kind: KIND_DESIGN_DOC.to_string(),
                    label: None,
                },
            ],
            "code lens buckets members by community (sized, dominant-kind, labelled) and keeps kind buckets for membership-less nodes"
        );
        assert_eq!(
            overview.edges,
            vec![ClusterEdge {
                from: "community/1/0".to_string(),
                to: "community/1/1".to_string(),
                weight: 2,
            }],
            "only cross-community coupling edges weight the super-edge; intra-community edges and the membership spokes to the excluded super-node add none"
        );

        // --- CODE LENS DRILL: a community drills to exactly its members (the excluded super-node is
        // not a member; the membership spokes are not intra-community edges) ---
        let drill = cluster_detail(&graph, "community/1/0", &code);
        assert_eq!(
            drill.seed, "community/1/0",
            "the drill echoes the community key"
        );
        assert_eq!(drill.truncated, None);
        let members: std::collections::BTreeSet<&str> =
            drill.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            members,
            [foo, bar].into_iter().collect(),
            "the community drills to exactly its member code entities: {drill:?}"
        );
        assert_eq!(
            drill.edges.len(),
            1,
            "only the intra-community coupling edge (foo->bar) renders; the membership spoke and the cross-community edge do not"
        );
        assert!(
            drill.edges.iter().all(|e| e.rel == REL_CALLS
                && members.contains(e.from.as_str())
                && members.contains(e.to.as_str())),
            "every drill edge is intra-community coupling: {drill:?}"
        );

        // --- UNDERIVED GRAIN: resolution "2" has no assignments -> the documented empty state ---
        let underived = clustered_overview(
            &graph,
            &Lens::Code {
                resolution: "2".to_string(),
            },
        );
        assert!(
            underived.clusters.is_empty() && underived.edges.is_empty(),
            "an underived grain folds no communities: {underived:?}"
        );
        assert_eq!(
            underived.empty_state.as_deref(),
            Some(CODE_LENS_UNDERIVED),
            "an underived grain carries the documented empty-state message, never an error"
        );

        // --- LENS=FILES is byte-identical to the spec-42 directory/kind fold (no label, no
        // empty_state); a membership-less node buckets by its DIRECTORY here, not its kind ---
        let files = clustered_overview(&graph, &Lens::Files);
        assert_eq!(files.total, 8);
        assert_eq!(files.empty_state, None, "files lens carries no empty state");
        assert!(
            files.clusters.iter().all(|c| c.label.is_none()),
            "the files fold attaches no community label"
        );
        let file_keys: Vec<&str> = files.clusters.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            file_keys,
            vec![
                KIND_COMMUNITY,
                KIND_DECISION,
                "docs",
                "src/four",
                "src/one",
                "src/three",
                "src/two",
            ],
            "the files lens folds by directory/kind (the community nodes bucket by their kind, the design-doc by its directory): {files:?}"
        );

        // The lens-selector parser: `lens=code` (default grain when resolution absent), an explicit
        // grain, and any other/absent lens value -> the byte-identical Files default.
        assert_eq!(
            Lens::from_query(Some("code"), None),
            Lens::Code {
                resolution: DEFAULT_COMMUNITY_RESOLUTION.to_string()
            }
        );
        assert_eq!(
            Lens::from_query(Some("code"), Some("1.5")),
            Lens::Code {
                resolution: "1.5".to_string()
            }
        );
        assert_eq!(Lens::from_query(None, None), Lens::Files);
        assert_eq!(Lens::from_query(Some("files"), None), Lens::Files);
        assert_eq!(Lens::from_query(Some("bogus"), Some("9")), Lens::Files);
    }

    /// The CONCEPTS LENS VIEW (spec 54 c3): `lens=concepts` buckets every `REALIZES`-carrying node by
    /// its intent CONCEPT through the SAME overview/drill folds - the idea the docs and code realize,
    /// grouped across directory lines. A node realizing MORE THAN ONE concept folds under its PRIMARY
    /// (the largest concept by member count, ties by lexicographically-smallest id) and is flagged
    /// `shared` - counted once, never silently duplicated; a membership-less node keeps its KIND
    /// bucket (so the view stays whole-graph); the `KIND_CONCEPT` super-node is a bucket, not a
    /// member, so it is excluded; an underived grain carries the documented empty state; and the files
    /// lens stays byte-identical. This is the criterion-3 fold behaviour driven inside-out.
    #[test]
    fn concepts_lens_buckets_members_by_concept_with_primary_shared_and_empty_state() {
        let ce = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: BTreeMap::new(),
        };
        let doc = |id: &str| Node {
            id: id.to_string(),
            kind: KIND_DESIGN_DOC.to_string(),
            attrs: BTreeMap::new(),
        };
        let concept = |id: &str, label: &str| Node {
            id: id.to_string(),
            kind: KIND_CONCEPT.to_string(),
            attrs: BTreeMap::from([("label".to_string(), label.to_string())]),
        };
        let plain = |id: &str, kind: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::new(),
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

        // Two derived concepts, each grouping a DOC with the CODE it governs across directory lines
        // (the whole point of the lens): concept/1/0 "the graph" = {docs/kg.md, graph::build,
        // store::append} (size 3, the LARGER); concept/1/1 "the review" = {docs/review.md, graph::build}
        // (size 2, the SMALLER). `graph::build` REALIZES BOTH - a SHARED member whose PRIMARY is the
        // larger concept/1/0 (by size, not the tie-break). Plus the two KIND_CONCEPT super-nodes (each
        // labelled) and TWO membership-less nodes (an unattached code entity + a decision) that keep
        // their KIND buckets. One cross-concept doc reference weights the super-edge; one intra-concept
        // GOVERNS edge adds none.
        let kg = "docs/kg.md";
        let review = "docs/review.md";
        let build = "src/graph/index.rs::build";
        let append = "src/store/log.rs::append";
        let helper = "src/util/misc.rs::helper";
        let graph = Graph {
            nodes: vec![
                doc(kg),
                doc(review),
                ce(build),
                ce(append),
                ce(helper),
                concept("concept/1/0", "the graph"),
                concept("concept/1/1", "the review"),
                plain("d1", KIND_DECISION),
            ],
            edges: vec![
                // Live REALIZES memberships at grain 1 (member --REALIZES--> concept).
                edge(kg, "concept/1/0", REL_REALIZES, TIER_INFERRED),
                edge(build, "concept/1/0", REL_REALIZES, TIER_INFERRED),
                edge(append, "concept/1/0", REL_REALIZES, TIER_INFERRED),
                edge(review, "concept/1/1", REL_REALIZES, TIER_INFERRED),
                // The SHARED member: build also realizes the smaller concept/1/1.
                edge(build, "concept/1/1", REL_REALIZES, TIER_INFERRED),
                // One CROSS-concept doc reference (kg in c0, review in c1) -> one weight-1 super-edge.
                edge(kg, review, REL_REFERENCES, TIER_INFERRED),
                // One INTRA-concept edge (kg and append both in c0) -> adds NO cross weight.
                edge(kg, append, REL_GOVERNS, TIER_INFERRED),
            ],
        };

        let concepts = Lens::Concepts {
            resolution: DEFAULT_CONCEPT_RESOLUTION.to_string(),
        };

        // --- OVERVIEW: buckets by concept, shared member counted ONCE under its primary ---
        let overview = clustered_overview(&graph, &concepts);
        assert_eq!(
            overview.total, 8,
            "total carries every graph node, the excluded concept super-nodes included"
        );
        assert_eq!(
            overview.empty_state, None,
            "a derived grain is not the empty state"
        );
        assert_eq!(
            overview.clusters,
            vec![
                // The unattached code entity keeps its KIND bucket (not its directory).
                Cluster {
                    key: KIND_CODE_ENTITY.to_string(),
                    count: 1,
                    kind: KIND_CODE_ENTITY.to_string(),
                    label: None,
                },
                // concept/1/0 (the larger): {kg.md, build, append} = 3 members, dominant kind
                // code-entity (build + append), labelled by the concept node's label.
                Cluster {
                    key: "concept/1/0".to_string(),
                    count: 3,
                    kind: KIND_CODE_ENTITY.to_string(),
                    label: Some("the graph".to_string()),
                },
                // concept/1/1 (the smaller): the SHARED build folds under its primary c0, so c1 counts
                // ONLY its sole non-shared member docs/review.md.
                Cluster {
                    key: "concept/1/1".to_string(),
                    count: 1,
                    kind: KIND_DESIGN_DOC.to_string(),
                    label: Some("the review".to_string()),
                },
                // The membership-less decision keeps its KIND bucket.
                Cluster {
                    key: KIND_DECISION.to_string(),
                    count: 1,
                    kind: KIND_DECISION.to_string(),
                    label: None,
                },
            ],
            "concepts lens folds members by concept (primary bucket, shared counted once), keeps kind buckets for the unattached nodes, and labels each concept: {overview:?}"
        );
        assert_eq!(
            overview.edges,
            vec![ClusterEdge {
                from: "concept/1/0".to_string(),
                to: "concept/1/1".to_string(),
                weight: 1,
            }],
            "only the cross-concept doc reference weights the super-edge; the intra-concept edge and the REALIZES spokes to the excluded super-node add none: {overview:?}"
        );

        // --- DRILL c0: exactly its primary members, the SHARED member flagged ---
        let drill0 = cluster_detail(&graph, "concept/1/0", &concepts);
        assert_eq!(
            drill0.seed, "concept/1/0",
            "the drill echoes the concept key"
        );
        let members0: BTreeMap<&str, bool> = drill0
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.shared))
            .collect();
        assert_eq!(
            members0,
            BTreeMap::from([(kg, false), (build, true), (append, false)]),
            "concept/1/0 drills to exactly {{kg, build, append}}; the multi-concept build carries shared=true, the single-concept members shared=false: {drill0:?}"
        );
        assert_eq!(
            drill0.edges.len(),
            1,
            "only the intra-concept kg->append edge renders; the REALIZES spokes and the cross-concept reference do not: {drill0:?}"
        );

        // --- DRILL c1: the shared build appears ONCE (under its primary c0), never here ---
        let drill1 = cluster_detail(&graph, "concept/1/1", &concepts);
        let members1: BTreeSet<&str> = drill1.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            members1,
            [review].into_iter().collect::<BTreeSet<&str>>(),
            "concept/1/1 drills to ONLY its non-shared member docs/review.md; the shared build appears once, under its primary c0: {drill1:?}"
        );
        assert!(
            drill1.nodes.iter().all(|n| !n.shared),
            "review realizes only one concept, so it is not shared: {drill1:?}"
        );

        // --- UNDERIVED grain: resolution 2 has no assignments -> the documented empty state ---
        let underived = clustered_overview(
            &graph,
            &Lens::Concepts {
                resolution: "2".to_string(),
            },
        );
        assert!(
            underived.clusters.is_empty() && underived.edges.is_empty(),
            "an underived concepts grain folds no concepts: {underived:?}"
        );
        assert_eq!(
            underived.total, 8,
            "the empty state still reports the whole graph size"
        );
        assert_eq!(
            underived.empty_state.as_deref(),
            Some(CONCEPTS_LENS_UNDERIVED),
            "an underived concepts grain carries the documented empty-state message, never an error"
        );

        // --- FILES lens byte-identical: no label, no empty_state, membership-less code buckets by
        // its DIRECTORY (not its kind), the concept super-nodes bucket by their kind ---
        let files = clustered_overview(&graph, &Lens::Files);
        assert_eq!(files.total, 8);
        assert_eq!(files.empty_state, None, "files lens carries no empty state");
        assert!(
            files.clusters.iter().all(|c| c.label.is_none()),
            "the files fold attaches no concept label: {files:?}"
        );
        assert!(
            files.clusters.iter().any(|c| c.key == "src/graph"),
            "under the files lens graph::build buckets by its directory src/graph, not by concept: {files:?}"
        );

        // --- THE PUBLIC SELECTOR: lens=concepts is a total, infallible parse ---
        assert_eq!(
            Lens::from_query(Some("concepts"), None),
            Lens::Concepts {
                resolution: DEFAULT_CONCEPT_RESOLUTION.to_string()
            },
            "lens=concepts with no resolution selects the default concept grain"
        );
        assert_eq!(
            Lens::from_query(Some("concepts"), Some("")),
            Lens::Concepts {
                resolution: DEFAULT_CONCEPT_RESOLUTION.to_string()
            },
            "an empty resolution still defaults to the default concept grain"
        );
        assert_eq!(
            Lens::from_query(Some("concepts"), Some("1.5")),
            Lens::Concepts {
                resolution: "1.5".to_string()
            },
            "an explicit resolution grain is honoured verbatim"
        );
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
            &[],
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
            &[],
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
                &[],
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
                &[],
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
                &[],
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

    /// GRACEFUL DEGRADATION (spec 42 c6): the overview route over an EMPTY graph returns a
    /// well-formed empty overview - zero clusters, zero cross-cluster edges, zero total - as a
    /// `200` JSON response, NOT an error, so the KG panel renders its "empty graph" message
    /// instead of throwing. This is the KG-feature-off / absent-graph.db case AT THE ROUTE
    /// boundary: the serving command builds the context graph best-effort, so an absent or
    /// unreadable graph arrives here as [`Graph::default`] (an empty graph). Both entry points
    /// to the overview - the no-argument default view and an explicit empty `seed=` - must
    /// degrade to the same well-formed empty overview, and (being un-feature-gated) this test
    /// runs and passes in BOTH feature lanes. This test OWNS the empty / degraded path; it does
    /// NOT re-prove the populated overview aggregation (c2 owns that) or the route's populated
    /// dispatch (c4 owns that).
    #[test]
    fn the_overview_route_degrades_gracefully_on_an_empty_graph() {
        let empty = Graph::default();
        // The two entry points to the DEFAULT overview view - a bare request and an explicit
        // empty `seed=` - are what the panel loads on open; each must degrade, never error.
        for target in ["/api/graph", "/api/graph?seed="] {
            let r = route(
                "GET",
                target,
                &[],
                &empty,
                &[],
                &HashMap::new(),
                3,
                "rigger-run",
                "origin/main",
                &[],
            );
            // A well-formed response, never the 500 projection-error path: the panel gets JSON.
            assert_eq!(
                r.status, 200,
                "{target}: an empty graph answers 200, not an error status"
            );
            assert_eq!(
                r.content_type, "application/json",
                "{target}: the empty overview is served as JSON"
            );
            let body: serde_json::Value = serde_json::from_slice(&r.body)
                .expect("the empty overview body is well-formed JSON");
            // A well-formed empty OVERVIEW (not a neighborhood): the overview carries no `nodes`
            // key, reports zero `total`, and folds into zero clusters and zero edges - exactly the
            // shape the panel keys its empty-graph message off.
            assert!(
                body["nodes"].is_null(),
                "{target}: the empty view is an overview, not a neighborhood (no `nodes`): {body}"
            );
            assert_eq!(
                body["total"], 0,
                "{target}: an empty graph reports zero total nodes: {body}"
            );
            let clusters = body["clusters"]
                .as_array()
                .expect("the overview carries a `clusters` array");
            assert!(
                clusters.is_empty(),
                "{target}: an empty graph folds into ZERO clusters: {body}"
            );
            let edges = body["edges"]
                .as_array()
                .expect("the overview carries an `edges` array");
            assert!(
                edges.is_empty(),
                "{target}: an empty graph has ZERO cross-cluster edges: {body}"
            );
        }
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
            &[],
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
            &[],
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
            &[],
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
            &[],
        );
        let body2: serde_json::Value = serde_json::from_slice(&r2.body).unwrap();
        assert!(
            body2.get("explain").is_none(),
            "an unknown seed omits the explain provenance: {body2}"
        );
    }

    #[test]
    fn graph_seeds_enumerate_decisions_findings_and_their_files_never_units() {
        // De-noise (spec 43): a unit is not a graph node, so its id is NEVER seeded - a unit seed
        // would land nowhere. The seed set is the decisions and findings the run produced plus the
        // files they GOVERN / are ABOUT: the content and code that remain in the graph.
        let events = vec![
            ev("UnitStarted", r#"{"unit":"u1"}"#),
            ev(
                "DecisionMade",
                r#"{"id":"d1","summary":"x","governs":["a.rs"]}"#,
            ),
            ev(
                "ReviewFinding",
                r#"{"id":"f1","by":"sdet","about":["b.rs"]}"#,
            ),
            ev("GateVerdict", r#"{"gate":"g","pass":true}"#),
        ];
        let seeds = graph_seeds(&events);
        assert_eq!(
            seeds,
            vec![
                "a.rs".to_string(),
                "b.rs".to_string(),
                "d1".to_string(),
                "f1".to_string(),
            ],
            "seeds are decisions + findings + the files they concern, never the unit id"
        );
        assert!(
            !seeds.contains(&"u1".to_string()),
            "a unit id is never a graph seed (it is not a node)"
        );
    }

    #[test]
    fn a_units_seed_lands_on_the_neighborhood_of_its_decisions_and_files() {
        // Spec 43 criterion 5 (the click-to-seed re-point): with the KIND_UNIT node gone, seeding
        // the graph from a unit's run - through the re-pointed graph_seeds - must STILL return a
        // NON-EMPTY, real neighborhood (the unit's decisions and the files they produced), not an
        // empty result. Fold a small run (a unit, and a decision it made governing a file) into a
        // real projection, then seed it with graph_seeds output and confirm a live neighborhood.
        use crate::contextgraph::sqlite::Projector;
        use crate::contextgraph::Projection;
        let run = positioned(vec![
            ev(
                "UnitStarted",
                r#"{"unit":"u1","criterion":"c","agent":"impl","needs":[]}"#,
            ),
            ev(
                "DecisionMade",
                r#"{"id":"d1","summary":"use the shared authority","governs":["combat.rs"],"supersedes":""}"#,
            ),
        ]);
        let p = Projector::open(":memory:", "test").unwrap();
        for e in &run {
            p.apply(e).unwrap();
        }
        let seeds = graph_seeds(&run);
        assert!(
            !seeds.contains(&"u1".to_string()),
            "the unit id is not a seed - it was re-pointed to the decisions/files"
        );
        let g = p.subgraph(&seeds, 2).unwrap();
        assert!(
            !g.nodes.is_empty(),
            "the re-pointed unit seed lands on a real, non-empty neighborhood, not an empty result"
        );
        assert!(
            g.nodes.iter().any(|n| n.id == "d1"),
            "the unit's decision is in the seeded neighborhood"
        );
        assert!(
            g.nodes.iter().any(|n| n.id == "combat.rs"),
            "the file the unit's decision produced is in the seeded neighborhood"
        );
        assert!(
            !g.nodes.iter().any(|n| n.id == "u1"),
            "no KIND_UNIT node exists (the machinery is gone); the seed landed via the decision/file"
        );
    }

    #[test]
    fn the_run_tree_click_to_seed_route_lands_a_unit_on_a_real_neighborhood() {
        // Spec 43 criterion 5, the INTERACTIVE half (adj-u43c1-click-to-seed): the run-tree renders
        // a unit node whose data-seed IS the unit id, and clicking it drives
        // `GET /api/graph?seed=<unit>`. With the KIND_UNIT node de-noised away a raw unit-id seed
        // resolves to no node, so the ROUTE must re-point it onto that unit's decisions/findings
        // (its content nodes, which remain in the graph) and return a NON-EMPTY neighborhood - never
        // the empty panel the raw unit seed would otherwise yield. This drives the ACTUAL route the
        // click crosses, which the graph_seeds-only tests never touch (adj-u43c1-click-to-seed).
        use crate::contextgraph::sqlite::Projector;
        use crate::contextgraph::Projection;

        // A run's events in production shape: the unit, a decision its implementer emitted and a
        // finding a reviewer drew ABOUT the unit - BOTH stamped with their emitting spawn (`u1`'s
        // implementer / `u1`'s sdet lens), exactly as `rigger emit --spawn` records them. The finding
        // carries no `$.unit` field; its unit is the `meta.spawn` stamp, as in production.
        let run = positioned(vec![
            ev(
                "UnitStarted",
                r#"{"unit":"u1","criterion":"c","agent":"impl","needs":[]}"#,
            ),
            ev(
                "DecisionMade",
                r#"{"id":"d1","summary":"use the shared authority","governs":["combat.rs"],"supersedes":""}"#,
            )
            .with_meta(crate::conductor::META_SPAWN, "u1/implementer#0"),
            ev(
                "ReviewFinding",
                r#"{"id":"f1","by":"sdet","summary":"y","about":["render.rs"]}"#,
            )
            .with_meta(crate::conductor::META_SPAWN, "u1/lens:sdet#0"),
        ]);

        // Fold the run and pre-fetch its subgraph EXACTLY as the dash does (graph_seeds -> subgraph
        // depth 2), so the route sees the same in-memory graph production serves.
        let p = Projector::open(":memory:", "test").unwrap();
        for e in &run {
            p.apply(e).unwrap();
        }
        let graph = p.subgraph(&graph_seeds(&run), 2).unwrap();
        assert!(
            !graph.nodes.iter().any(|n| n.id == "u1"),
            "no KIND_UNIT node exists - the click-to-seed must re-point off the (gone) unit node"
        );

        let r = route(
            "GET",
            "/api/graph?seed=u1",
            &run,
            &graph,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
            &[],
        );
        assert_eq!(r.status, 200, "the KG route answers 200 for a unit click");
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let ids: std::collections::BTreeSet<&str> = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert!(
            !ids.is_empty(),
            "the re-pointed unit click lands on a real, non-empty neighborhood, not an empty panel: {body}"
        );
        assert!(
            ids.contains("d1"),
            "the unit's decision is in the clicked neighborhood: {body}"
        );
        assert!(
            ids.contains("combat.rs"),
            "the file the unit's decision governs is reached canonically via its GOVERNS edge: {body}"
        );
        assert!(
            ids.contains("f1"),
            "the unit's finding is in the clicked neighborhood: {body}"
        );
        assert!(
            !ids.contains("u1"),
            "the unit id itself is never a node; the click landed via the unit's decisions/findings"
        );
    }

    #[test]
    fn unit_seeds_scope_content_to_the_owning_unit() {
        // A decision AND a finding are both attributed to their unit by the emitting spawn
        // (meta.spawn) - the production shape a reviewer's `rigger emit --spawn` records, which
        // carries no `$.unit` event field. unit_seeds returns ONLY the named unit's content ids +
        // files (sorted), never another unit's - so a run-tree click on `uA` never drags in `uB`'s
        // neighborhood.
        let events = positioned(vec![
            ev(
                "DecisionMade",
                r#"{"id":"dA","summary":"x","governs":["a.rs"]}"#,
            )
            .with_meta(crate::conductor::META_SPAWN, "uA/implementer#0"),
            ev(
                "DecisionMade",
                r#"{"id":"dB","summary":"y","governs":["b.rs"]}"#,
            )
            .with_meta(crate::conductor::META_SPAWN, "uB/implementer#0"),
            ev(
                "ReviewFinding",
                r#"{"id":"fA","by":"sdet","summary":"z","about":["c.rs"]}"#,
            )
            .with_meta(crate::conductor::META_SPAWN, "uA/lens:sdet#0"),
            ev(
                "ReviewFinding",
                r#"{"id":"fB","by":"sdet","summary":"z","about":["d.rs"]}"#,
            )
            .with_meta(crate::conductor::META_SPAWN, "uB/lens:sdet#0"),
        ]);
        assert_eq!(
            unit_seeds(&events, "uA"),
            vec![
                "a.rs".to_string(),
                "c.rs".to_string(),
                "dA".to_string(),
                "fA".to_string(),
            ],
            "uA's seeds are its decision + governed file and its finding + about file, sorted"
        );
        let s_b = unit_seeds(&events, "uB");
        assert!(
            s_b.contains(&"dB".to_string()) && s_b.contains(&"fB".to_string()),
            "uB's seeds carry uB's own content"
        );
        assert!(
            !s_b.contains(&"dA".to_string()) && !s_b.contains(&"fA".to_string()),
            "uB's seeds never include uA's content"
        );
        // A decision with no emitting-spawn stamp is attributed to no unit.
        let unstamped = vec![ev("DecisionMade", r#"{"id":"d0","governs":["x.rs"]}"#)];
        assert!(
            unit_seeds(&unstamped, "uA").is_empty(),
            "a decision with no meta.spawn stamp is attributed to no unit"
        );
    }

    #[test]
    fn repoint_seed_passes_a_known_node_and_re_points_a_unit_id() {
        // repoint_seed decides ONLY on node membership (it never walks edges), so an edgeless graph
        // holding just the content nodes is enough to pin its three arms.
        let mk = |id: &str, kind: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::new(),
        };
        let graph = Graph {
            nodes: vec![mk("dA", KIND_DECISION), mk("a.rs", "file")],
            edges: Vec::new(),
        };
        let events = vec![ev(
            "DecisionMade",
            r#"{"id":"dA","summary":"x","governs":["a.rs"]}"#,
        )
        .with_meta(crate::conductor::META_SPAWN, "uA/implementer#0")];

        // A seed that IS a node is returned unchanged - the spec 30 seeded panel, no regression.
        assert_eq!(
            repoint_seed(&events, &graph, "dA"),
            vec!["dA".to_string()],
            "a known node seed is passed through untouched"
        );
        // A unit id (not a node) re-points onto the unit's content nodes present in the graph.
        assert_eq!(
            repoint_seed(&events, &graph, "uA"),
            vec!["a.rs".to_string(), "dA".to_string()],
            "a unit-id seed re-points onto the unit's decision and its governed file node"
        );
        // A genuinely unknown seed with no unit content falls back to itself (graceful empty).
        assert_eq!(
            repoint_seed(&events, &graph, "nope"),
            vec!["nope".to_string()],
            "an unknown seed with no unit content degrades to itself, not a re-point"
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
        let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
            let backend = Store::open(&db_for_provider).map_err(|e| e.to_string())?;
            let store = Namespaced::new(&backend, "proj-dash");
            let events = store
                .read_stream(conductor::STREAM, 0, Direction::Forward)
                .map_err(|e| e.to_string())?;
            Ok((events, Graph::default(), Vec::new(), HashMap::new()))
        };

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let graph_provider = |_instance: Option<&str>| Graph::default();
        let calls_provider =
            |_: Option<&str>, _: &[String], _: crate::contextgraph::Direction, _: i64, _: &str| {
                crate::contextgraph::CallGraph::default()
            };
        let instances_provider = Vec::new;
        let server = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            handle_conn(
                conn,
                &provider,
                &graph_provider,
                &calls_provider,
                &instances_provider,
                3,
                "rigger-run",
                "origin/main",
            )
            .unwrap();
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

        let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
            panic!("a non-GET request must never read the store");
        };
        let graph_provider = |_instance: Option<&str>| -> Graph {
            panic!("a non-GET request must never open the graph projection");
        };
        let calls_provider = |_: Option<&str>,
                              _: &[String],
                              _: crate::contextgraph::Direction,
                              _: i64,
                              _: &str|
         -> crate::contextgraph::CallGraph {
            panic!("a non-GET request must never open the calls projection");
        };
        let instances_provider = || -> Vec<InstanceView> {
            panic!("a non-GET request must never read the instance registry");
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            handle_conn(
                conn,
                &provider,
                &graph_provider,
                &calls_provider,
                &instances_provider,
                3,
                "rigger-run",
                "origin/main",
            )
            .unwrap();
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

    /// Spec 45, criterion 1 (the PROVIDER SPLIT): `/api/graph` reads through a SEPARATE,
    /// lazy graph provider that is opened ONLY when a graph request arrives - a `/api/state`
    /// (or `/api/events`) request must NEVER consult it, so the 1.5s state poll no longer
    /// rides a whole-graph read. A spy graph provider counts each time it is consulted:
    /// after `/api/state` and `/api/events` the count stays 0; a `/api/graph` request opens it
    /// exactly once and the served body is derived from the graph the provider yields (not the
    /// polled tuple's run-seeded graph). This drives the real `serve` -> `handle_conn` -> `route`
    /// socket path, so it proves the split at the served boundary the pure `route` test is blind to.
    #[test]
    fn the_graph_provider_is_consulted_only_on_graph_requests_not_the_state_poll() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // The polled provider: the cheap run-scoped inputs `/api/state` and `/api/events` ride.
        // Its graph slot is the run-seeded slice (here empty); it is what the decisions/findings
        // panel reads, and it must be the ONLY graph the state poll touches.
        let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
            Ok((Vec::new(), Graph::default(), Vec::new(), HashMap::new()))
        };

        // The SEPARATE whole-graph provider: it counts every consultation and yields a fixture
        // graph carrying one node, so a graph request produces a graph-derived body while the
        // count proves it was opened ONLY on that request.
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_provider = Arc::clone(&hits);
        let graph_provider = move |_instance: Option<&str>| -> Graph {
            hits_for_provider.fetch_add(1, Ordering::SeqCst);
            Graph {
                nodes: vec![Node {
                    id: "seed-node".to_string(),
                    kind: KIND_UNIT.to_string(),
                    attrs: BTreeMap::new(),
                }],
                edges: Vec::new(),
            }
        };

        // A call view is not exercised here (no request carries `view=calls`), so the calls
        // provider must never be consulted; a plain empty walk keeps the wiring complete.
        let calls_provider =
            |_: Option<&str>, _: &[String], _: crate::contextgraph::Direction, _: i64, _: &str| {
                crate::contextgraph::CallGraph::default()
            };
        let instances_provider = Vec::new;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        // A bounded accept loop (three requests) so the server thread joins deterministically.
        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let (conn, _) = listener.accept().unwrap();
                handle_conn(
                    conn,
                    &provider,
                    &graph_provider,
                    &calls_provider,
                    &instances_provider,
                    3,
                    "rigger-run",
                    "origin/main",
                )
                .unwrap();
            }
        });

        let get = |path: &str| -> String {
            let mut client = TcpStream::connect(addr).unwrap();
            client
                .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
                .unwrap();
            let mut resp = String::new();
            client.read_to_string(&mut resp).unwrap();
            resp
        };

        // The state poll must NOT open the whole-graph projection.
        let state = get("/api/state");
        assert!(
            state.starts_with("HTTP/1.1 200 OK"),
            "the state poll is served: {state}"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "a /api/state request must NOT consult the whole-graph provider"
        );

        // Nor must the events feed.
        let events = get("/api/events");
        assert!(
            events.starts_with("HTTP/1.1 200 OK"),
            "the events feed is served: {events}"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "a /api/events request must NOT consult the whole-graph provider"
        );

        // A graph request DOES consult it - exactly once - and the body is graph-derived.
        let graph = get("/api/graph?seed=seed-node&depth=1");
        server.join().unwrap();
        assert!(
            graph.starts_with("HTTP/1.1 200 OK"),
            "the graph route is served: {graph}"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "a /api/graph request opens the whole-graph projection exactly once"
        );
        let body = graph
            .split("\r\n\r\n")
            .nth(1)
            .expect("a graph response body");
        assert!(
            body.contains("seed-node"),
            "the graph body is derived from the graph provider's projection, not the polled \
             run-seeded graph: {body}"
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

    // --- Spec 62, criterion 3: HELD-PORT DIAGNOSIS ---

    #[test]
    fn format_held_port_always_names_the_address_even_with_no_holder() {
        let addr: SocketAddr = "127.0.0.1:7450".parse().unwrap();
        let msg = format_held_port(addr, None);
        assert!(
            msg.contains("127.0.0.1:7450"),
            "the held address must always appear, even when the holder is undiscoverable; \
             got: {msg}"
        );
        assert!(
            !msg.to_lowercase().contains("resume"),
            "an undiscoverable holder must never invent a stopped-listener diagnosis; got: {msg}"
        );
    }

    #[test]
    fn format_held_port_names_the_pid_and_state_for_a_running_holder() {
        let addr: SocketAddr = "127.0.0.1:7451".parse().unwrap();
        let msg = format_held_port(addr, Some((4242, Some('R'))));
        assert!(msg.contains("127.0.0.1:7451"), "got: {msg}");
        assert!(
            msg.contains("4242"),
            "must name the holder's pid; got: {msg}"
        );
        assert!(
            msg.contains('R'),
            "must name the discoverable state; got: {msg}"
        );
        assert!(
            !msg.to_lowercase().contains("resume"),
            "a running (non-stopped) holder must not get the stopped-listener diagnosis; \
             got: {msg}"
        );
    }

    #[test]
    fn format_held_port_names_the_pid_alone_when_its_state_is_not_discoverable() {
        let addr: SocketAddr = "127.0.0.1:7452".parse().unwrap();
        let msg = format_held_port(addr, Some((4343, None)));
        assert!(msg.contains("127.0.0.1:7452"), "got: {msg}");
        assert!(
            msg.contains("4343"),
            "must still name the holder's pid; got: {msg}"
        );
    }

    #[test]
    fn format_held_port_gives_the_stopped_listener_diagnosis_naming_resume_or_kill() {
        for state in ['T', 't'] {
            let addr: SocketAddr = "127.0.0.1:7453".parse().unwrap();
            let msg = format_held_port(addr, Some((5454, Some(state))));
            assert!(msg.contains("127.0.0.1:7453"), "got: {msg}");
            assert!(
                msg.contains("5454"),
                "must name the stopped holder's pid; got: {msg}"
            );
            let lower = msg.to_lowercase();
            assert!(
                lower.contains("resume") && lower.contains("kill"),
                "a stopped ({state:?}) holder must name resume-or-kill explicitly; got: {msg}"
            );
        }
    }

    #[test]
    fn pid_holding_port_finds_the_pid_of_a_listener_bound_in_this_process() {
        if !Path::new("/proc").is_dir() {
            return;
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(
            pid_holding_port(port),
            Some(std::process::id()),
            "the /proc scan must find THIS process as the holder of its own listener"
        );
        drop(listener);
    }

    #[test]
    fn pid_holding_port_is_none_for_a_port_nothing_is_listening_on() {
        if !Path::new("/proc").is_dir() {
            return;
        }
        // Learn a free port and release it - nothing rebinds it, so no /proc/net/tcp row
        // should name it.
        let port = TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(
            pid_holding_port(port),
            None,
            "an unheld port must have no holder"
        );
    }

    #[test]
    fn describe_held_port_names_this_process_when_it_holds_the_port_itself() {
        if !Path::new("/proc").is_dir() {
            return;
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let msg = describe_held_port(addr);
        assert!(
            msg.contains(&std::process::id().to_string()),
            "describe_held_port must name this test process's own pid as the holder; got: {msg}"
        );
        assert!(msg.contains(&addr.to_string()), "got: {msg}");
        drop(listener);
    }

    /// Spec 62 round 3 fix (adj-u62c3r2-verdict-reject-non-addrinuse-mislabel): unlike
    /// [`describe_held_port`] (whose one production caller, `cmd_dash`, only ever reaches it
    /// AFTER the OS has already confirmed `AddrInUse`, so a `None` holder there still means a
    /// genuine-but-unattributed conflict), [`describe_held_port_if_confirmed`] has no such
    /// upstream confirmation available to its caller (`spawn_run_dashboard_detached`, whose
    /// bind attempt runs in a detached child with no observable `io::Error` at all) - so it must
    /// independently confirm occupancy itself before naming one. A port a real listener holds
    /// must still resolve `Some`, naming this test process's own pid.
    #[test]
    fn describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed() {
        if !Path::new("/proc").is_dir() {
            return;
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let msg = describe_held_port_if_confirmed(addr)
            .expect("a port a real listener holds must resolve Some, not None");
        assert!(
            msg.contains(&std::process::id().to_string()),
            "must name this test process's own pid as the holder; got: {msg}"
        );
        assert!(msg.contains(&addr.to_string()), "got: {msg}");
        drop(listener);
    }

    /// Spec 62 round 3 fix (adj-u62c3r2-verdict-reject-non-addrinuse-mislabel): the defect the
    /// adjudicator reproduced - a bind failure unrelated to any real conflict (permission error,
    /// slow machine, config problem) getting the false "already in use" framing anyway. A port
    /// NOTHING holds must resolve `None`, giving the caller nothing to falsely claim.
    #[test]
    fn describe_held_port_if_confirmed_is_none_when_nothing_holds_the_port() {
        if !Path::new("/proc").is_dir() {
            return;
        }
        let addr: SocketAddr = {
            let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            probe.local_addr().unwrap()
        };
        // The listener above is already dropped by the time this line runs - nothing rebinds
        // its port, so no /proc/net/tcp row should name a holder.
        assert_eq!(
            describe_held_port_if_confirmed(addr),
            None,
            "a port nothing holds must never be described as held - the exact false-positive \
             this round's fix exists to close"
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
    fn dash_status_trusts_a_url_with_no_marker_and_catches_a_marker_that_lies() {
        let m = DashMarker {
            port: 7442,
            pid: 4242,
        };
        let url = "http://127.0.0.1:7442/".to_string();

        // No URL ever recorded -> Absent, and the probe is never even consulted (nothing to
        // verify).
        assert_eq!(
            dash_status(None, Some(m), |_| panic!(
                "must not probe when there is no recorded URL"
            )),
            DashStatus::Absent,
            "no recorded dash -> Absent"
        );

        // A recorded URL with NO marker to check against is TRUSTED as-is, with NO probe - the
        // guard-bound `rigger run` / `rigger serve` dash records a URL but no marker at all
        // (spec 62 owns the marker lifecycle), so an absent marker must never read as "dead".
        assert_eq!(
            dash_status(Some(url.clone()), None, |_| panic!(
                "must not probe when there is no marker to verify"
            )),
            DashStatus::Serving(url.clone()),
            "a recorded URL with no marker to verify is trusted unchanged"
        );

        // A MATCHING marker whose port the probe PROVES is serving -> the URL is trusted.
        assert_eq!(
            dash_status(Some(url.clone()), Some(m), |p| {
                assert_eq!(p, 7442, "must probe the matching marker's own port");
                true
            }),
            DashStatus::Serving(url.clone()),
            "a marker proven serving -> the recorded URL is trusted"
        );

        // A MATCHING marker whose port the probe PROVES is NOT serving -> the lie this
        // criterion closes: no URL, just the pid the matching marker names.
        assert_eq!(
            dash_status(Some(url), Some(m), |p| {
                assert_eq!(p, 7442, "must probe the matching marker's own port");
                false
            }),
            DashStatus::NotServing { pid: Some(4242) },
            "a marker proven dead -> not serving, naming its pid"
        );
    }

    /// Round 5 (adj-u62c1r4-verdict-reject-sentinel-pid-leaks-to-status,
    /// sdet-u62c1r4-unattributed-pid-sentinel-renders-as-a-fabricated-dead-pid): a marker
    /// carrying [`UNATTRIBUTED_PID`] (spec 62 round 4's documented sentinel, recorded when a
    /// port was confirmed serving but the real serving process could not be identified) names no
    /// real process. Before this fix, `dash_status` handed that raw `0` straight through into
    /// `NotServing { pid: Some(0) }`, which every display site then printed as "marker names dead
    /// pid 0" - a literal lie, since `0` was never assigned to, or the pid of, any real process.
    /// This proves `dash_status` itself filters it to `None` at the point it constructs
    /// `NotServing`, so it renders identically to the already-correct no-matching-marker case.
    #[test]
    fn dash_status_never_names_the_unattributed_pid_sentinel_as_a_dead_process() {
        let sentinel = DashMarker {
            port: 7442,
            pid: UNATTRIBUTED_PID,
        };
        let url = "http://127.0.0.1:7442/".to_string();

        assert_eq!(
            dash_status(Some(url), Some(sentinel), |p| {
                assert_eq!(p, 7442, "must probe the matching marker's own port");
                false
            }),
            DashStatus::NotServing { pid: None },
            "a sentinel-pid marker proven dead must name NO pid, not the sentinel value \
             itself - the sentinel was never a real, assigned pid"
        );
    }

    #[test]
    fn url_port_parses_the_recorded_shape_and_rejects_anything_else() {
        assert_eq!(url_port("http://127.0.0.1:7420/"), Some(7420));
        assert_eq!(url_port("http://127.0.0.1:7420"), Some(7420));
        assert_eq!(url_port("http://127.0.0.1:7420/api/state"), Some(7420));
        assert_eq!(url_port(""), None, "empty is not a URL");
        assert_eq!(url_port("not-a-url"), None, "no scheme -> unparseable");
        assert_eq!(
            url_port("http://127.0.0.1/"),
            None,
            "no port at all -> unparseable"
        );
        assert_eq!(
            url_port("http://127.0.0.1:not-a-port/"),
            None,
            "a non-numeric port -> unparseable"
        );
        assert_eq!(
            url_port("http://127.0.0.1:99999/"),
            None,
            "a port past u16::MAX -> unparseable"
        );
    }

    /// Round 2 (adv-u69c4-dash-status-verifies-wrong-port): a marker naming a DIFFERENT port
    /// than the recorded url describes some OTHER dash, not this one - its OWN liveness must
    /// never stand in as proof about the url either direction (that would wrongly vouch for a
    /// dead url, or wrongly hide a genuinely live one).
    ///
    /// Round 3 (adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url): the round-2 fix filtered
    /// a mismatched marker to `None` and fell into the SAME unconditional-trust branch a
    /// genuinely absent marker uses - "nothing to check" - which let a genuinely DEAD url sail
    /// through as trusted whenever a mismatched marker happened to be recorded (empirically
    /// reproduced against the built binary; see the finding). A mismatched marker is a positive
    /// "something is tracked" signal, not "nothing to check": it must trigger a REAL probe of
    /// the url's own port, and never let the marker's own (unrelated) port or pid substitute for
    /// one.
    #[test]
    fn dash_status_probes_the_urls_own_port_when_the_marker_names_a_different_dash() {
        let url = "http://127.0.0.1:7442/".to_string();
        // Names a completely different port (9999) than the url (7442) - and a pid that
        // belongs to that OTHER, unrelated dash, not this url's.
        let mismatched = DashMarker {
            port: 9999,
            pid: 5555,
        };

        // The url's OWN port genuinely answers -> trusted. The probe must be asked about the
        // url's port (7442), never the mismatched marker's unrelated port (9999) - proving the
        // marker's own liveness plays no part in the decision either direction.
        assert_eq!(
            dash_status(Some(url.clone()), Some(mismatched), |p| {
                assert_eq!(
                    p, 7442,
                    "must probe the url's own port, never the mismatched marker's"
                );
                true
            }),
            DashStatus::Serving(url.clone()),
            "a mismatched marker must never suppress a genuinely-alive url"
        );

        // The url's OWN port genuinely does NOT answer -> not serving, with NO pid: the
        // mismatched marker's pid names an unrelated (possibly still-alive) dash and must
        // never be printed as though it belonged to this dead url - the exact lie round 3
        // closes (round 2 left this direction open: a genuinely dead url paired with a
        // mismatched marker sailed through as trusted).
        assert_eq!(
            dash_status(Some(url.clone()), Some(mismatched), |p| {
                assert_eq!(
                    p, 7442,
                    "must probe the url's own port, never the mismatched marker's"
                );
                false
            }),
            DashStatus::NotServing { pid: None },
            "a mismatched marker must not let a genuinely dead url sail through as trusted"
        );
    }

    #[test]
    fn should_reap_singleton_reaps_only_when_no_registered_instance_is_live() {
        // Spec 50, criterion 5: the machine-level singleton reaps itself ONLY when nothing is
        // registered-and-alive - and never before it has seen its first live instance (the
        // startup-race guard, the direct analogue of spec 39's `run_started`).

        // Startup: the ensuring run has not yet written its registry entry, so the watcher reads
        // ZERO live instances on its first polls. It must NOT reap before that entry lands.
        assert!(
            !should_reap_singleton(0, false, false),
            "a just-ensured singleton that has not yet seen any live instance must not reap"
        );

        // A live instance is registered (this project's run, or any other's): keep serving.
        assert!(
            !should_reap_singleton(1, true, false),
            "one live registered instance keeps the singleton serving"
        );
        // The multi-instance headline: one project's run ending while ANOTHER's is still live
        // leaves the count > 0, so the singleton survives.
        assert!(
            !should_reap_singleton(2, true, false),
            "several live instances keep the singleton serving"
        );

        // Every registered instance's heartbeat has aged past the idle window (so `read_live`
        // pruned them all), the watcher HAS seen a live instance before, and there is no agent
        // liveness signal either: a quiet machine -> reap.
        assert!(
            should_reap_singleton(0, true, false),
            "no live instance, after at least one was seen, reaps the singleton"
        );

        // A count > 0 that was never marked seen cannot occur in the watcher (a non-empty read
        // flips the flag first), but the decision stays safe: a positive count never reaps.
        assert!(
            !should_reap_singleton(1, false, false),
            "a positive live count never reaps regardless of the seen flag"
        );
    }

    #[test]
    fn should_reap_singleton_never_reaps_while_a_fresh_agent_liveness_signal_is_present() {
        // Spec 62, criterion 5 (SINGLETON SURVIVES LIVE WORK, OWNS the idle judgment): the
        // reap decision now requires BOTH the registry AND the agent liveness signal to be
        // quiet - a registry that has aged out (empty, but was once seen live) must NOT reap
        // while a fresh in-flight agent liveness marker is present.
        assert!(
            !should_reap_singleton(0, true, true),
            "an aged-out registry with a fresh agent liveness signal must not reap"
        );
        // With BOTH quiet, it reaps exactly as today.
        assert!(
            should_reap_singleton(0, true, false),
            "an aged-out registry with no agent liveness signal reaps exactly as before"
        );
        // A live registered instance keeps serving regardless of the agent signal either way.
        assert!(
            !should_reap_singleton(1, true, false),
            "a live registered instance keeps serving even with no agent liveness signal"
        );
        assert!(
            !should_reap_singleton(1, true, true),
            "a live registered instance plus a live agent signal still keeps serving"
        );
        // The startup-race guard is unchanged: never reaps before any instance has been seen,
        // agent signal or not.
        assert!(
            !should_reap_singleton(0, false, false),
            "the startup guard still holds with no agent signal"
        );
        assert!(
            !should_reap_singleton(0, false, true),
            "the startup guard still holds even with a live agent signal"
        );
    }

    /// Spec 52 c5 (the RENDERING): the served page carries the DIRECTED-CALL layered layout - the
    /// left-to-right DAG behind the SHARED SVG emitter (a barycenter within-layer sweep), the SVG
    /// ARROWHEAD marker definition (which the page did not have before), the DISTINCT back-edge
    /// rendering (a curved return arc), and the FRONTIER expand-and-reseed wiring - plus the entry
    /// affordance that offers the two directed queries from a code-entity node. Visual layout is
    /// outside the gate set (rule 4), so this is a STRUCTURAL guard on the JS that delivers the
    /// rendering, mirroring the sibling exploration-viz page tests: it pins the mechanisms so a later
    /// edit cannot drop the arrowheads, the back-edge distinction, the layered layout, or the
    /// frontier re-seed.
    #[test]
    fn the_page_carries_the_directed_call_layered_render() {
        let page = live_page();

        // The LAYERED layout behind the shared emitter: x by server layer, a within-layer barycenter
        // sweep (average of neighbour positions) - not a second force-layout copy.
        assert!(
            page.contains("function layeredLayout"),
            "the page must carry the layered left-to-right call layout",
        );
        assert!(
            page.contains("barycenter") || page.contains("bary"),
            "the layered layout must order within-layer nodes by a barycenter sweep",
        );
        // The layered layout is drawn through the SAME kgSvg emitter (an injected layout callback),
        // never a second SVG emitter reimplementing circles/lines.
        assert!(
            page.contains("layout: layeredLayout"),
            "the calls view must reuse the shared kgSvg emitter with an injected layered layout",
        );

        // Direction is DRAWN: an SVG arrowhead marker definition (new to the page) and a marker-end
        // on the forward edges.
        assert!(
            page.contains("<marker") && page.contains("marker-end"),
            "the page must define an SVG arrowhead marker and apply it to directed edges",
        );

        // BACK edges (recursion) render DISTINCTLY: a curved return arc (a path with a quadratic
        // segment) carrying a distinguishing class, not just another straight line.
        assert!(
            page.contains("kgline back"),
            "a back edge must carry a distinguishing class so recursion reads distinctly",
        );
        assert!(
            page.contains("edgeBack"),
            "the emitter must render a back edge as a distinct curved arc via edgeBack",
        );

        // FRONTIERS are ACTIONABLE: a frontier node carries its candidates and expands on click, and
        // choosing a candidate RE-SEEDS the call view on it.
        assert!(
            page.contains("data-frontier") && page.contains("data-candidates"),
            "a multi-candidate frontier node must carry its candidate ids for the expand",
        );
        assert!(
            page.contains("data-candidate") && page.contains("function seedCalls"),
            "choosing a frontier candidate must re-seed the directed-call view on it",
        );

        // The call views are REACHABLE from a code-entity node: the neighborhood offers the two
        // directed queries (execution path / call sites) beside it, wired through the delegated
        // listener the exploration views already share.
        assert!(
            page.contains("data-calls-down") && page.contains("data-calls-up"),
            "a code-entity node must offer the two directed queries (execution path / call sites)",
        );
        assert!(
            page.contains("view=calls"),
            "the page must fetch the directed-call views from the c4 route",
        );
        assert!(
            page.contains("function renderKgCalls"),
            "the page must carry the directed-call renderer",
        );

        // HIGH FAN-OUT within a layer caps at the render budget with a "+K more" note, so a
        // widely-called function does not overplot its layer into an unreadable smear.
        assert!(
            page.contains("LAYER_FANOUT_BUDGET") && page.contains("held back"),
            "a layer over the render budget must cap with a '+K more' held-back note",
        );
    }

    /// Spec 52 c4 (the ROUTE): the `/api/graph?view=calls&dir=down|up|both` dispatch. These are the
    /// implementer's inside-out unit tests over the pure builder [`calls_view`] and the dispatch
    /// [`calls_route`] - the CallGraph -> Neighborhood-shaped mapping (signed layers, frontier, back,
    /// the UP sidecar, the `dir=both` merge) and the param parse / provider dispatch / byte-identical
    /// fall-through. The traversal itself is spec 52 c1/c3, proven at the store; here we own only the
    /// route's presentation of it.
    mod calls_route_c4 {
        use super::*;
        use crate::contextgraph::sqlite::Projector;
        use crate::contextgraph::{
            CallEdge, CallGraph, CallNode, Direction, Projection, REL_CALLS,
            TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
        };

        /// One reached call node with a store-side (non-negative) hop `layer` and an optional
        /// multi-candidate `frontier`, as the traversal returns it.
        fn cnode(id: &str, layer: i64, frontier: Option<Vec<String>>) -> CallNode {
            CallNode {
                node: Node {
                    id: id.to_string(),
                    kind: KIND_CODE_ENTITY.to_string(),
                    attrs: BTreeMap::new(),
                },
                layer,
                frontier,
            }
        }

        /// One CALLS edge with the recursion `back` marker.
        fn cedge(from: &str, to: &str, back: bool) -> CallEdge {
            CallEdge {
                edge: Edge {
                    from: from.to_string(),
                    to: to.to_string(),
                    rel: REL_CALLS.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: 0,
                    tier: TIER_INFERRED.to_string(),
                },
                back,
            }
        }

        fn file_node(id: &str) -> Node {
            Node {
                id: id.to_string(),
                kind: KIND_FILE.to_string(),
                attrs: BTreeMap::new(),
            }
        }

        fn layer_of(v: &Neighborhood, id: &str) -> Option<i64> {
            v.nodes.iter().find(|n| n.id == id).and_then(|n| n.layer)
        }
        fn ids(v: &Neighborhood) -> Vec<String> {
            v.nodes.iter().map(|n| n.id.clone()).collect()
        }

        /// Fold a code definition into a Projector, exactly as the store-side periphery tests do, so
        /// the dispatch test drives the REAL `Projection::calls` through a store-backed provider.
        fn apply_def(p: &Projector, pos: u64, file: &str, name: &str, line: u32, fresh: bool) {
            let payload = serde_json::json!({
                "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
                "fresh": fresh,
            });
            let mut e = Event::new(
                TYPE_CODE_ENTITY_EXTRACTED,
                serde_json::to_vec(&payload).unwrap(),
            );
            e.position = pos;
            p.apply(&e).unwrap();
        }
        fn apply_call(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
            let payload = serde_json::json!({
                "file": file, "name": name, "lang": "rust", "caller": caller,
            });
            let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
            e.position = pos;
            p.apply(&e).unwrap();
        }

        /// DOWN: the callee layers stay POSITIVE (seed at the left), the frontier candidate ids ride
        /// through verbatim, a recursion edge keeps its back marker, and the nodes emit in (layer, id)
        /// order. The DOWN execution path carries NO referenced-but-not-called sidecar.
        #[test]
        fn calls_view_down_signs_callees_positive_and_carries_frontier_and_back() {
            let down = CallGraph {
                nodes: vec![
                    cnode("f.rs::s", 0, None),
                    cnode("f.rs::a", 1, None),
                    cnode(
                        "f.rs::fr",
                        1,
                        Some(vec!["a.rs::t".to_string(), "b.rs::t".to_string()]),
                    ),
                ],
                edges: vec![
                    cedge("f.rs::s", "f.rs::a", false),
                    cedge("f.rs::s", "f.rs::fr", false),
                    cedge("f.rs::a", "f.rs::s", true), // recursion: a back edge
                ],
                referenced_not_called: Vec::new(),
            };
            let v = calls_view(Some(&down), None, "f.rs::s", 5);

            assert_eq!(
                v.dir.as_deref(),
                Some("down"),
                "the body echoes the direction"
            );
            assert!(
                v.referenced_not_called.is_empty(),
                "a DOWN walk carries no referenced-but-not-called sidecar",
            );
            // Callees are POSITIVE, seed 0 - so the renderer draws the seed at the LEFT.
            assert_eq!(layer_of(&v, "f.rs::s"), Some(0));
            assert_eq!(layer_of(&v, "f.rs::a"), Some(1));
            assert_eq!(layer_of(&v, "f.rs::fr"), Some(1));
            // The frontier candidate ids ride through verbatim on the frontier node.
            let fr = v.nodes.iter().find(|n| n.id == "f.rs::fr").unwrap();
            assert_eq!(
                fr.frontier,
                Some(vec!["a.rs::t".to_string(), "b.rs::t".to_string()]),
            );
            assert_eq!(
                v.nodes.iter().filter(|n| n.frontier.is_some()).count(),
                1,
                "exactly the one multi-candidate node is a frontier",
            );
            // The recursion edge is marked back; the forward edges are not.
            let back = |from: &str, to: &str| {
                v.edges
                    .iter()
                    .find(|e| e.from == from && e.to == to)
                    .map(|e| e.back)
            };
            assert_eq!(back("f.rs::a", "f.rs::s"), Some(true));
            assert_eq!(back("f.rs::s", "f.rs::a"), Some(false));
            // Nodes emit in (layer, id) order: layer 0 (s), then layer 1 id-sorted (a, fr).
            assert_eq!(ids(&v), vec!["f.rs::s", "f.rs::a", "f.rs::fr"]);
        }

        /// UP: the caller layers are NEGATED (so the renderer draws the seed at the RIGHT), and the
        /// referenced-but-not-called sidecar rides through as flat FILE nodes.
        #[test]
        fn calls_view_up_negates_callers_and_carries_the_referenced_sidecar() {
            let up = CallGraph {
                nodes: vec![cnode("a.rs::t", 0, None), cnode("b.rs::c", 1, None)],
                edges: vec![cedge("b.rs::c", "a.rs::t", false)],
                referenced_not_called: vec![file_node("d.rs")],
            };
            let v = calls_view(None, Some(&up), "a.rs::t", 5);

            assert_eq!(v.dir.as_deref(), Some("up"));
            assert_eq!(layer_of(&v, "a.rs::t"), Some(0), "the seed stays at 0");
            assert_eq!(
                layer_of(&v, "b.rs::c"),
                Some(-1),
                "a caller is NEGATED so the seed draws at the right",
            );
            let refd: Vec<&str> = v
                .referenced_not_called
                .iter()
                .map(|n| n.id.as_str())
                .collect();
            assert_eq!(
                refd,
                vec!["d.rs"],
                "the UP sidecar carries the import-only file"
            );
            assert!(
                v.referenced_not_called.iter().all(|n| n.kind == KIND_FILE),
                "every sidecar entry is a FILE node",
            );
            // (layer, id) order: the caller (-1) sorts before the seed (0).
            assert_eq!(ids(&v), vec!["b.rs::c", "a.rs::t"]);
            // The caller edge keeps the real CALLS direction onto the seed.
            assert_eq!(
                v.edges
                    .iter()
                    .map(|e| (e.from.as_str(), e.to.as_str()))
                    .collect::<Vec<_>>(),
                vec![("b.rs::c", "a.rs::t")],
            );
        }

        /// BOTH: the seed is centered at 0, callees to the RIGHT (positive), callers to the LEFT
        /// (negative), the shared seed deduped to ONE node, edges deduped by (from, to, rel), and the
        /// UP sidecar carried - one "flow through this function" body from the two walks.
        #[test]
        fn calls_view_both_centers_the_seed_with_callees_right_and_callers_left() {
            let down = CallGraph {
                nodes: vec![cnode("m.rs::s", 0, None), cnode("m.rs::callee", 1, None)],
                edges: vec![cedge("m.rs::s", "m.rs::callee", false)],
                referenced_not_called: Vec::new(),
            };
            let up = CallGraph {
                nodes: vec![cnode("m.rs::s", 0, None), cnode("m.rs::caller", 1, None)],
                edges: vec![cedge("m.rs::caller", "m.rs::s", false)],
                referenced_not_called: vec![file_node("z.rs")],
            };
            let v = calls_view(Some(&down), Some(&up), "m.rs::s", 5);

            assert_eq!(v.dir.as_deref(), Some("both"));
            assert_eq!(layer_of(&v, "m.rs::s"), Some(0), "the seed is centered");
            assert_eq!(
                layer_of(&v, "m.rs::callee"),
                Some(1),
                "a callee sits to the RIGHT (positive)",
            );
            assert_eq!(
                layer_of(&v, "m.rs::caller"),
                Some(-1),
                "a caller sits to the LEFT (negative)",
            );
            assert_eq!(
                v.nodes.iter().filter(|n| n.id == "m.rs::s").count(),
                1,
                "the shared seed is deduped to a single node across the two walks",
            );
            // Both edges are present, deduped by (from, to, rel), in (from, to, rel) order.
            assert_eq!(
                v.edges
                    .iter()
                    .map(|e| (e.from.as_str(), e.to.as_str()))
                    .collect::<Vec<_>>(),
                vec![("m.rs::caller", "m.rs::s"), ("m.rs::s", "m.rs::callee")],
            );
            assert_eq!(
                v.referenced_not_called
                    .iter()
                    .map(|n| n.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["z.rs"],
                "the UP sidecar rides through on a both walk",
            );
            // (layer, id) order: caller(-1), seed(0), callee(1).
            assert_eq!(ids(&v), vec!["m.rs::caller", "m.rs::s", "m.rs::callee"]);
        }

        /// A plain neighborhood is BYTE-IDENTICAL after the additive fields (spec 52 c4 constraint):
        /// none of `layer` / `frontier` / `back` / `dir` / `referenced_not_called` serialize when a
        /// view is not a call view, so the serialized neighborhood carries exactly its original keys.
        #[test]
        fn a_plain_neighborhood_omits_every_additive_call_field() {
            let graph = Graph {
                nodes: vec![
                    Node {
                        id: "a".to_string(),
                        kind: KIND_UNIT.to_string(),
                        attrs: BTreeMap::new(),
                    },
                    Node {
                        id: "b".to_string(),
                        kind: KIND_UNIT.to_string(),
                        attrs: BTreeMap::new(),
                    },
                ],
                edges: vec![Edge {
                    from: "a".to_string(),
                    to: "b".to_string(),
                    rel: REL_REFERENCES.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: 0,
                    tier: TIER_EXTRACTED.to_string(),
                }],
            };
            let body = graph_json(&graph, "a", &["a".to_string()], 1, None, None).unwrap();
            for absent in [
                "\"layer\"",
                "\"frontier\"",
                "\"back\"",
                "\"dir\"",
                "referenced_not_called",
            ] {
                assert!(
                    !body.contains(absent),
                    "a plain neighborhood must not serialize the additive call field {absent}: {body}",
                );
            }
        }

        /// The dispatch: `view=calls` runs the store-side traversal through the provider and returns
        /// its layered body; an absent `view` DECLINES (so `handle_conn` falls through to the
        /// byte-identical neighborhood). Drives the REAL `Projection::calls` through a store-backed
        /// provider closure, so it proves the route wired the direction/seed onto the traversal.
        #[test]
        fn calls_route_runs_the_traversal_for_view_calls_and_declines_otherwise() {
            let p = Projector::open(":memory:", "test").unwrap();
            apply_def(&p, 1, "src/a.rs", "callee", 1, true);
            apply_def(&p, 2, "src/c.rs", "caller", 1, true);
            apply_call(&p, 3, "src/c.rs", "callee", "caller");
            let cp = |_inst: Option<&str>,
                      seed: &[String],
                      dir: Direction,
                      depth: i64,
                      floor: &str|
             -> CallGraph {
                p.calls(seed, dir, depth, floor).unwrap_or_default()
            };

            // No view=calls: the dispatch declines so the neighborhood path runs unchanged.
            assert!(
                calls_route(None, "/api/graph?seed=src/c.rs::caller&depth=2", &cp).is_none(),
                "a request with no view=calls is not a call view",
            );

            // view=calls&dir=down: the DOWN walk resolves the cross-file callee onto its definition.
            let resp = calls_route(
                None,
                "/api/graph?view=calls&dir=down&seed=src%2Fc.rs%3A%3Acaller&depth=5",
                &cp,
            )
            .expect("view=calls dispatches to the traversal");
            assert_eq!(resp.status, 200);
            let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
            assert_eq!(v["dir"], "down");
            assert_eq!(
                v["seed"], "src/c.rs::caller",
                "the body echoes the decoded seed"
            );
            let node_ids: Vec<&str> = v["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].as_str().unwrap())
                .collect();
            assert!(
                node_ids.contains(&"src/c.rs::caller") && node_ids.contains(&"src/a.rs::callee"),
                "the DOWN walk resolved the cross-file callee onto its definition: {node_ids:?}",
            );
            let callee = v["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == "src/a.rs::callee")
                .unwrap();
            assert_eq!(
                callee["layer"], 1,
                "the callee sits at layer 1 (seed at the left)"
            );
        }

        /// `depth=` is clamped and `tier=` is the floor (defaulting to `inferred`): a spy provider
        /// records the (depth, floor) the route passed it, so the clamp / default is pinned without a
        /// store. `dir` defaults to `down`, so a bare `view=calls` calls the provider once.
        #[test]
        fn calls_route_clamps_depth_and_defaults_the_tier_floor() {
            use std::cell::RefCell;
            let seen: RefCell<Vec<(i64, String)>> = RefCell::new(Vec::new());
            let cp = |_inst: Option<&str>,
                      _seed: &[String],
                      _dir: Direction,
                      depth: i64,
                      floor: &str|
             -> CallGraph {
                seen.borrow_mut().push((depth, floor.to_string()));
                CallGraph::default()
            };

            // Absent depth -> the neighborhood default; absent tier -> the resolvable inferred floor.
            let _ = calls_route(None, "/api/graph?view=calls&seed=x", &cp);
            assert_eq!(
                seen.borrow()[0],
                (DEFAULT_GRAPH_DEPTH, TIER_INFERRED.to_string()),
            );

            // An over-large depth is clamped to the ceiling; an explicit tier is the floor verbatim.
            seen.borrow_mut().clear();
            let _ = calls_route(
                None,
                "/api/graph?view=calls&seed=x&depth=9999&tier=ambiguous",
                &cp,
            );
            assert_eq!(seen.borrow()[0], (MAX_GRAPH_DEPTH, "ambiguous".to_string()));
        }

        /// `dir=both` calls the provider TWICE (once per direction) and merges; `dir=down` / `dir=up`
        /// call it once each - so the route asks the traversal for exactly the sides it draws.
        #[test]
        fn calls_route_walks_both_directions_for_dir_both() {
            use std::cell::RefCell;
            let dirs: RefCell<Vec<Direction>> = RefCell::new(Vec::new());
            let cp = |_inst: Option<&str>,
                      _seed: &[String],
                      dir: Direction,
                      _depth: i64,
                      _floor: &str|
             -> CallGraph {
                dirs.borrow_mut().push(dir);
                CallGraph::default()
            };
            let _ = calls_route(None, "/api/graph?view=calls&dir=both&seed=x", &cp);
            assert_eq!(
                *dirs.borrow(),
                vec![Direction::Down, Direction::Up],
                "dir=both walks BOTH the callees and the callers",
            );
        }
    }
}

/// Spec 55, criterion 3 - the RATIONALE OVERLAY DATA PATH. Inside-out unit tests over the pure
/// [`node_rationale`] / [`rationale_batch`] surface and the `/api/graph?explain=` route branch: the
/// per-node query returns the decisions/findings/lessons attached to a node (CONTENT only,
/// deterministically ordered), a node with no rationale returns none, and the batch endpoint covers a
/// set of visible nodes in one request. This criterion OWNS the overlay data. The served-boundary
/// proof (one real HTTP GET over `dash::serve`) lives in `tests/rationale_overlay_data.rs`.
#[cfg(test)]
mod rationale_overlay_c3 {
    use super::*;
    use crate::contextgraph::{Edge, KIND_CODE_ENTITY, KIND_FILE, KIND_HANDBOOK_RULE};

    /// A node with the given kind and optional `summary` (a decision / finding / lesson content
    /// node carries a summary; a plain file / entity target does not).
    fn node(id: &str, kind: &str, summary: &str) -> Node {
        Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: if summary.is_empty() {
                BTreeMap::new()
            } else {
                BTreeMap::from([("summary".to_string(), summary.to_string())])
            },
        }
    }

    /// A finding content node carrying the run-machinery attribution (`by` reviewer + `unit`)
    /// ALONGSIDE its `summary`, so a test can prove the leaf drops the machinery and keeps only the
    /// content.
    fn finding_node(id: &str, summary: &str, by: &str, unit: &str) -> Node {
        Node {
            id: id.to_string(),
            kind: KIND_FINDING.to_string(),
            attrs: BTreeMap::from([
                ("summary".to_string(), summary.to_string()),
                ("by".to_string(), by.to_string()),
                ("unit".to_string(), unit.to_string()),
            ]),
        }
    }

    fn edge(from: &str, to: &str, rel: &str, valid_to: Option<i64>) -> Edge {
        Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: rel.to_string(),
            valid_from: 0,
            valid_to,
            source: 0,
            tier: TIER_INFERRED.to_string(),
        }
    }

    /// The fixture. A target file `shared.rs` carries four live rationale leaves through
    /// `GOVERNS` (decisions) / `ABOUT` (finding, lesson) edges, laid out so the deterministic
    /// `(kind, id)` order is DISCRIMINATING:
    ///
    /// - two decisions `dz` and `da` (added `dz` first) prove the id-secondary sort within a kind;
    /// - a finding `a-find` whose id is lexicographically SMALLER than either decision id proves the
    ///   kind-primary sort (id-only ordering would float `a-find` to the front);
    /// - a lesson `l1`.
    ///
    /// Three NON-leaves also point at `shared.rs`, one per exclusion rule: a `handbook-rule` `hb`
    /// (GOVERNS, wrong kind), an INVALIDATED decision `dgone` (a superseded governing edge), and -
    /// separately - a superseding decision `dnew --SUPERSEDES--> dz` (so `node_rationale(dz)` proves
    /// SUPERSEDES is not rationale). The code entity `shared.rs::foo` carries ONE leaf (`da` governs
    /// it), and `other.rs` carries NONE.
    fn rationale_graph() -> Graph {
        Graph {
            nodes: vec![
                node("shared.rs", KIND_FILE, ""),
                node("shared.rs::foo", KIND_CODE_ENTITY, ""),
                node("other.rs", KIND_FILE, ""),
                node("dz", KIND_DECISION, "decision zed"),
                node("da", KIND_DECISION, "decision ay"),
                node("dnew", KIND_DECISION, "the superseding decision"),
                node("dgone", KIND_DECISION, "the superseded governing decision"),
                finding_node(
                    "a-find",
                    "the finding content",
                    "lens:architecture-reviewer",
                    "u7",
                ),
                node("l1", KIND_LESSON, "the lesson content"),
                node("hb", KIND_HANDBOOK_RULE, "the handbook rule"),
            ],
            edges: vec![
                edge("dz", "shared.rs", REL_GOVERNS, None),
                edge("da", "shared.rs", REL_GOVERNS, None),
                edge("a-find", "shared.rs", REL_ABOUT, None),
                edge("l1", "shared.rs", REL_ABOUT, None),
                // Non-leaves incident to shared.rs, one per exclusion rule.
                edge("hb", "shared.rs", REL_GOVERNS, None), // handbook rule: wrong kind
                edge("dgone", "shared.rs", REL_GOVERNS, Some(5)), // invalidated governing edge
                // A superseding decision points AT dz (so dz's own rationale query sees only this).
                edge("dnew", "dz", REL_SUPERSEDES, None),
                // shared.rs::foo carries exactly one leaf.
                edge("da", "shared.rs::foo", REL_GOVERNS, None),
            ],
        }
    }

    fn ids(leaves: &[RationaleLeaf]) -> Vec<String> {
        leaves.iter().map(|l| l.id.clone()).collect()
    }
    fn kinds(leaves: &[RationaleLeaf]) -> Vec<String> {
        leaves.iter().map(|l| l.kind.clone()).collect()
    }

    /// The per-node query returns the decisions/findings/lessons attached to a node, deterministically
    /// ordered by `(kind, id)` - kind first (decisions, then findings, then lessons), id within a
    /// kind. The fixture is laid out so this ONE assertion is load-bearing for BOTH sort keys.
    #[test]
    fn node_rationale_returns_attached_leaves_ordered_by_kind_then_id() {
        let g = rationale_graph();
        let leaves = node_rationale(&g, "shared.rs");
        assert_eq!(
            ids(&leaves),
            vec!["da", "dz", "a-find", "l1"],
            "leaves sort by (kind, id): decisions (da<dz) before findings before lessons - NOT by \
             id alone (which would float a-find first)"
        );
        assert_eq!(
            kinds(&leaves),
            vec!["decision", "decision", "finding", "lesson"],
            "each leaf carries its node kind"
        );
    }

    /// A leaf carries the CONTENT only - id, kind, summary - and NEVER the finding's run-machinery
    /// attribution (`by` reviewer / `unit`). Pinned as the exact serialized shape so a regression that
    /// leaked `by`/`unit` onto the wire reddens.
    #[test]
    fn a_finding_leaf_carries_content_only_never_the_by_or_unit_machinery() {
        let g = rationale_graph();
        let leaves = node_rationale(&g, "shared.rs");
        let find = leaves
            .iter()
            .find(|l| l.id == "a-find")
            .expect("the finding is a leaf of shared.rs");
        assert_eq!(
            find.summary, "the finding content",
            "the leaf keeps the content summary"
        );
        let json = serde_json::to_string(find).expect("a leaf serializes");
        assert_eq!(
            json, r#"{"id":"a-find","kind":"finding","summary":"the finding content"}"#,
            "the leaf is content-only on the wire: id/kind/summary, no by/unit machinery"
        );
        assert!(
            !json.contains("architecture-reviewer")
                && !json.contains("\"by\"")
                && !json.contains("\"unit\"")
                && !json.contains("u7"),
            "no builder-agent attribution surfaces: {json}"
        );
    }

    /// The kind filter excludes a `handbook-rule` even though it reuses `GOVERNS`, and the relation
    /// filter excludes a `SUPERSEDES` edge, so a superseding decision is not reported as its target's
    /// rationale.
    #[test]
    fn a_handbook_rule_and_a_supersedes_edge_are_not_rationale() {
        let g = rationale_graph();
        let shared = node_rationale(&g, "shared.rs");
        assert!(
            !shared.iter().any(|l| l.id == "hb"),
            "a handbook-rule that GOVERNS the node is NOT a decision/finding/lesson leaf: {shared:?}"
        );
        // dz is superseded by dnew (dnew --SUPERSEDES--> dz); the only edge INTO dz is that
        // SUPERSEDES edge, so dz's rationale is empty - a superseding decision is not rationale.
        assert!(
            node_rationale(&g, "dz").is_empty(),
            "a SUPERSEDES edge is not a rationale attachment"
        );
    }

    /// An INVALIDATED (superseded) governing edge is not live rationale: `dgone` GOVERNS `shared.rs`
    /// on an edge whose `valid_to` is set, so it never appears.
    #[test]
    fn an_invalidated_edge_is_not_live_rationale() {
        let g = rationale_graph();
        let leaves = node_rationale(&g, "shared.rs");
        assert!(
            !leaves.iter().any(|l| l.id == "dgone"),
            "a decision reaching the node only through an invalidated edge is not live rationale: \
             {leaves:?}"
        );
    }

    /// A node with no attached decision/finding/lesson returns NONE (empty) - "nodes without
    /// rationale return none".
    #[test]
    fn a_node_without_rationale_returns_none() {
        let g = rationale_graph();
        assert!(
            node_rationale(&g, "other.rs").is_empty(),
            "a node with no attached decision/finding/lesson has no rationale"
        );
        assert!(
            node_rationale(&g, "not-a-node").is_empty(),
            "an unknown id has no rationale (graceful, never an error)"
        );
    }

    /// The batch covers a SET of visible nodes in one call and keeps ONLY the nodes that carry any
    /// rationale, ordered by node id. `other.rs` (no rationale) is absent; `shared.rs` and
    /// `shared.rs::foo` are present with their leaves.
    #[test]
    fn the_batch_covers_the_visible_set_and_keeps_only_nodes_with_rationale() {
        let g = rationale_graph();
        let batch = rationale_batch(
            &g,
            &[
                "other.rs".to_string(),
                "shared.rs".to_string(),
                "shared.rs::foo".to_string(),
            ],
        );
        assert_eq!(
            batch.iter().map(|n| n.node.clone()).collect::<Vec<_>>(),
            vec!["shared.rs", "shared.rs::foo"],
            "only nodes with rationale appear, ordered by node id (other.rs is dropped)"
        );
        assert_eq!(
            ids(&batch[0].leaves),
            vec!["da", "dz", "a-find", "l1"],
            "shared.rs carries its four ordered leaves"
        );
        assert_eq!(
            ids(&batch[1].leaves),
            vec!["da"],
            "shared.rs::foo carries its one leaf"
        );
    }

    /// The batch is DETERMINISTIC regardless of the request's id order and repeats: a shuffled,
    /// duplicated request yields a byte-identical response.
    #[test]
    fn the_batch_is_deterministic_across_request_order_and_dedups() {
        let g = rationale_graph();
        let ordered = rationale_batch(&g, &["shared.rs".to_string(), "shared.rs::foo".to_string()]);
        let shuffled = rationale_batch(
            &g,
            &[
                "shared.rs::foo".to_string(),
                "shared.rs".to_string(),
                "shared.rs".to_string(), // a repeat must not double the node
                "other.rs".to_string(),
            ],
        );
        assert_eq!(
            serde_json::to_string(&RationaleBatch { nodes: ordered }).unwrap(),
            serde_json::to_string(&RationaleBatch { nodes: shuffled }).unwrap(),
            "the batch dedups and sorts, so id order/repeats do not change the bytes"
        );
    }

    /// Drive the `route` in-process: `GET /api/graph?explain=<ids>` returns the rationale batch as a
    /// 200 JSON body in ONE request, covering the visible set. A percent-encoded id (`::` -> `%3A%3A`)
    /// proves the split-then-decode of the comma-separated list.
    #[test]
    fn the_explain_route_returns_the_batch_in_one_request() {
        let g = rationale_graph();
        let resp = route(
            "GET",
            "/api/graph?explain=shared.rs,shared.rs%3A%3Afoo,other.rs",
            &[],
            &g,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
            &[],
        );
        assert_eq!(resp.status, 200, "the explain route answers 200");
        assert!(
            resp.content_type.contains("application/json"),
            "the batch is JSON: {}",
            resp.content_type
        );
        let body = String::from_utf8(resp.body).expect("a utf8 body");
        let json: serde_json::Value =
            serde_json::from_str(&body).expect("the explain body is valid JSON");
        let nodes = json["nodes"].as_array().expect("a nodes array");
        let got: Vec<&str> = nodes.iter().map(|n| n["node"].as_str().unwrap()).collect();
        assert_eq!(
            got,
            vec!["shared.rs", "shared.rs::foo"],
            "the encoded id decodes to shared.rs::foo and other.rs (no rationale) is dropped: {json}"
        );
        // The content crosses the wire and the machinery does not.
        assert!(
            body.contains("the finding content") && body.contains("the lesson content"),
            "leaf content is served: {body}"
        );
        assert!(
            !body.contains("architecture-reviewer") && !body.contains("\"by\""),
            "no builder-agent attribution is served: {body}"
        );
    }

    /// Additive guarantee: with `explain=` ABSENT, `/api/graph` is the existing view - the seeded
    /// neighborhood carries a `seed` and NO rationale `leaves`, and the branch fires ONLY on
    /// `explain=`.
    #[test]
    fn an_absent_explain_leaves_the_graph_route_unchanged() {
        let g = rationale_graph();
        let resp = route(
            "GET",
            "/api/graph?seed=shared.rs&depth=1",
            &[],
            &g,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "origin/main",
            &[],
        );
        assert_eq!(resp.status, 200);
        let body = String::from_utf8(resp.body).expect("a utf8 body");
        assert!(
            body.contains("\"seed\""),
            "an explain-less request is the seeded neighborhood: {body}"
        );
        assert!(
            !body.contains("\"leaves\""),
            "the neighborhood carries no rationale batch: {body}"
        );
    }
}
