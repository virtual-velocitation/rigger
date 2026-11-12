#!/bin/sh
# The self-evaluation instrument: after a run of this repository's own workflow has landed, it
# measures that run from the event log, hands the evaluator persona (.rigger/agents/evaluator.md)
# the evidence, and grades the format of the run file the persona completes. Every number in a
# run file comes from this script, never from the persona. docs/evaluation/README.md is the
# procedure and the file format; docs/evaluation/baseline.md states the measuring method.
#
# It is wired to no stage - .rigger/workflow.yml declares it as the `evaluate` gate, `check`
# alone, so `rigger gate evaluate` can run the format check - and the operator runs it after a
# landing and before the next `rigger run`, so it can never block or undo a landing. It opens the
# event store and the progress store read-only (SQLite's read-only mode, with query_only set on
# top), writes only the run file under docs/evaluation/ and its own scratch under
# .rigger/tmp/evaluate/ (removed after use and again before every `run`), commits nothing, runs no
# test (so it does not source container-env.sh), and sends no process signal.
#
# Usage: sh .rigger/gates/evaluate.sh <measure|evidence|check|emit|run> [args]
#   measure [run] [out]  write the run file from docs/evaluation/TEMPLATE.md: the scorecard, the
#                        comparison with the baseline and the last five run files, and the
#                        previous run file's adopted proposal scored. Refuses to overwrite a
#                        file. Prints the file's path.
#   evidence [run]       print the evaluator's three sources as JSON: the failures (rejects, gate
#                        failures, other failures, escalations and deferred gate failures, each
#                        with its measured rework), the cost (time by unit, label, battery and
#                        gate, the repeats the log shows and what it cannot show), and the grep
#                        fallbacks grouped by what they searched for. Never committed.
#   check [file]         grade a run file's format (default: the newest run file); exit 1 with
#                        one line per problem. It never judges content.
#   emit [file]          once check passes, record each proposal as a ReviewFinding through
#                        `rigger emit`, about the surface it changes and the run file.
#   run [run]            measure, then the evaluator through $EVALUATE_AGENT, then check.
# [run] is `latest` (the default: the last RunStarted on the run stream), a run id, or a unique
# prefix of one.
#
# Environment:
#   EVALUATE_DB        the event store (default: .rigger/events.db beside the main checkout). While
#                      a run is live, a copy of the store files (database, -wal and -shm together).
#   EVALUATE_PROGRESS  the progress store, the source of grep_fallbacks (default:
#                      .rigger/progress.db beside the event store).
#   EVALUATE_STREAM    the run stream, when the store holds more than one.
#   EVALUATE_AGENT     the headless agent command, up to and including its system-prompt-file
#                      flag: the persona body's path is appended and the task goes on stdin.
#
# check fails a run file when:
#   - a block (scorecard, previous, classes, proposals) is missing, repeated, not JSON, or
#     still null, or any text still reads "TO BE WRITTEN", or the file carries an em dash;
#   - a class lacks a kebab-case slug, a summary or evidence, or repeats a slug;
#   - there are more than three proposals, or none while a class exists; ids are not P1, P2, P3
#     in order; they are not ranked by hours_saved, highest first;
#   - a proposal lacks its class (one of the file's classes), change, evidence, surface, path,
#     form, metric, a non-zero predicted_move or a positive hours_saved; its metric does not name
#     a measured number in the scorecard (a top-level key, or `<map>.<key>` of a by-map);
#   - a persona or spec-skill proposal is not of form `instruction` (text is all those surfaces
#     hold), or a proposal for a class the previous run file also diagnosed is not of form
#     `check` or `gate`: instruction text already failed to remove that cause once;
#   - a binary proposal is of form `gate`: rigger is language and process agnostic, so the binary
#     gains no gate, and a gate is this repository's own, on the `gate` or `workflow` surface;
#   - "adopted" is not null, "none" or one of the file's proposal ids;
#   - the previous block differs from what measure computes from the previous run file.
# WHY format only: the operator judges content when adopting at most one proposal; the script
# makes every proposal scorable and every number reproducible, nothing more.

case "${1:-}" in
measure | evidence | check | emit | run) ;;
*)
    echo "usage: sh .rigger/gates/evaluate.sh <measure|evidence|check|emit|run> [args]" >&2
    exit 2
    ;;
esac

root=$(git rev-parse --show-toplevel) || exit 1
common=$(git rev-parse --path-format=absolute --git-common-dir) || exit 1
EVALUATE_ROOT=$root
EVALUATE_DB=${EVALUATE_DB:-$(dirname "$common")/.rigger/events.db}
EVALUATE_PROGRESS=${EVALUATE_PROGRESS:-$(dirname "$EVALUATE_DB")/progress.db}
export EVALUATE_ROOT EVALUATE_DB EVALUATE_PROGRESS

# Run the one Python program below in mode "$1" with the remaining arguments.
evaluate_py() {
    python3 -I - "$@" <<'PY'
import json
import os
import re
import sqlite3
import subprocess
import sys
from collections import Counter, namedtuple
from datetime import datetime, timezone
from pathlib import Path

CAP_NS = 65 * 60 * 10**9
NS_PER_HOUR = 3600 * 10**9
REVIEW_ONLY = "(review-only: no integrated artifact)"
EM_DASH = chr(0x2014)
TODO = "TO BE WRITTEN"
TYPES = ("RunStarted", "RunHeld", "RunReleased", "UnitStarted", "UnitStatus", "UnitFailed",
         "UnitEscalated", "UnitIntegrated", "SpawnRequested", "SpawnResult", "GateVerdict",
         "DeferredGateFailed")
# The driver records a heartbeat every 30 s; a longer silence than this means it was not running.
BEAT_GAP_S = 120
# The spawns whose result opens a gate battery: the writers whose tree the driver then gates.
WRITERS = ("/implementer#", "/sdet-author#")
SPAWN_ID = re.compile(r"^(?P<unit>.+)/(?P<role>[^/#]+)#(?P<attempt>\d+)(?:~retry\d+)?$")
ITEM_CLASSES = (("wrong code", "wrong-code"), ("unmet criterion", "unmet-criterion"),
                ("missing test", "missing-test"))
SLUG = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
RUN_FILE = re.compile(r"^\d{8}-\d{4}-[0-9A-Za-z_-]{8}\.md$")
BLOCK = re.compile(r"^```json (\w+)\n(.*?)^```$", re.M | re.S)
BLOCKS = ("scorecard", "previous", "classes", "proposals")
# Where a class comes from: the run's failures, what its time cost, or its grep fallbacks.
SOURCES = ("failures", "cost", "grep-fallbacks")
SURFACES = ("persona", "gate", "workflow", "spec-skill", "binary")
FORMS = ("check", "gate", "instruction")
INSTRUCTION_SURFACES = ("persona", "spec-skill")
BY_MAPS = ("failures_by_cause", "reject_items_by_class", "rejects_by_cause",
           "gate_fails_by_gate", "deferred_gate_fails_by_gate", "red_verdicts_by_gate",
           "time_shares", "hours_by_label", "gate_hours_by_gate", "repeated_hours",
           "grep_fallbacks_by_target")
# The metrics a cost class may name: hours by-maps and top-level hours, or shares of unit elapsed
# time, which the bound on hours_saved converts to hours.
COST_HOURS = ("hours_by_label", "gate_hours_by_gate", "repeated_hours", "unit_elapsed_hours")
COST_SHARES = ("time_shares", "gates_share", "post_merge_gates_share")
GREP_METRICS = ("grep_fallbacks", "grep_fallbacks_by_target")
COMPARED = ("units_integrated", "first_pass_rate", "attempts_per_unit", "rejects_per_unit",
            "gate_fails_per_unit", "escalations", "agent_busy_hours_per_unit",
            "wall_hours_per_criterion_unit", "live_wall_hours_per_criterion_unit",
            "rework_share", "gates_share",
            "post_merge_gates_share", "grep_fallbacks")
# The prefix an agent's progress line opens with when it searched the sources by text because
# the graph could not answer; the binary's own grep-fallback count uses the same prefix.
GREP_FALLBACK = "grep-fallback:"
UNSOURCED = {
    "grounding_share": "unmeasured: no event marks the end of an agent's orientation, and "
                       "FileTouched is recorded from the diff after the gate battery, so no event "
                       "times a session's first edit; nearest instrument: the agent session "
                       "transcripts",
    "context_restarts": "unmeasured: SpawnResult records no end reason, so a session that ran out "
                        "of context is not told from one that finished; relaunched_sessions counts "
                        "the launches a relaunch continued, whatever stopped them",
}
SELF_REPORTED = ("self-reported: agents record these lines themselves, so a fallback no agent "
                 "reported is not counted")
# What a grep fallback searched for, by the first rule its line matches, in this order. A grep is
# always literal, so "literal" says nothing; a line also often says why the graph had no node, so
# the text rule comes before the symbol rule.
GREP_TARGETS = (
    ("caller", re.compile(r"\bcall(?:er|ers|s|ing)?\b|\bcall sites?\b|--around|\breaders?\b|"
                          r"\bconsumers?\b|\bconstructions?\b|\buses\b", re.I)),
    ("text", re.compile(r"\bprose\b|\btexts?\b|\bspellings?\b|\bwording\b|\bphrases?\b|"
                        r"\bstrings?\b|\bsentences?\b|\bcensus\b|\bmentions\b", re.I)),
    ("symbol", re.compile(r"\bno (?:graph |test-helper |head-tree )?nodes?\b|--show|\bshow\b|"
                          r"returned (?:none|no nodes)|\bhelpers?\b|\bimpls?\b|\bdefs?\b|"
                          r"\bdefinitions?\b|\bfields?\b", re.I)),
    ("file", re.compile(r"\bheadings?\b|\bsections?\b|\bdeclarations\b|\bfiles?\b", re.I)),
)
GREP_GROUPING = ("self-reported, grouped by the first rule a line matches, in the order caller, "
                 "text, symbol, file; a line matching none is unclassified")
UNIT_HOURS = ("unit hours: a unit's live elapsed time, outages removed; units run in parallel, so "
              "their hours add up past the run's wall time")
GATE_HOURS = ("each fresh verdict's time since the previous verdict of its battery, or since the "
              "battery opened; the driver runs a battery's gates one at a time, so this is the "
              "gate's run plus the driver's step before it")
REWORK_HOURS = ("each failure's unit hours from the failure to its unit's next failure, next merge "
                "attempt or integration, whichever comes first")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
# One gate battery: its opening (a writer's result), its last fresh verdict, whether that verdict
# is post-merge, the writer's spawn id, and its verdicts as (time, info, position) in time order.
Battery = namedtuple("Battery", "opened last post opener verdicts")
TAIL = 2000


def refuse(message):
    print("evaluate: " + message)
    sys.exit(1)


def decode(raw):
    if raw is None:
        return {}
    try:
        value = json.loads(raw)
    except ValueError:
        return {}
    return value if isinstance(value, dict) else {}


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def ratio(part, whole):
    return round(part / whole, 3) if whole else None


def local_time(ns):
    if ns is None:
        return "-"
    return datetime.fromtimestamp(ns / 1e9, tz=timezone.utc).astimezone().strftime(
        "%Y-%m-%d %H:%M %z")


def shown(path):
    """The path as the repository names it, relative to its root, when it lies inside it."""
    try:
        return path.resolve().relative_to(Path(os.environ["EVALUATE_ROOT"]).resolve()).as_posix()
    except ValueError:
        return str(path)


def tail(text):
    if isinstance(text, list):
        text = "\n".join(part for part in text if isinstance(part, str))
    text = text if isinstance(text, str) else ""
    return text[-TAIL:]


# ---- the store ----

# Every store opens here: read-only by the URI's mode, and query-only on top, so no statement this
# script runs can write to a store.
def open_read_only(db):
    con = sqlite3.connect(db.resolve().as_uri() + "?mode=ro", uri=True)
    con.execute("PRAGMA query_only = ON")
    return con


def store():
    db = Path(os.environ["EVALUATE_DB"])
    if not db.is_file():
        refuse(f"no event store at {db}")
    return open_read_only(db)


# The (position, data, recorded_at) rows of one type on the run's own progress stream
# (`<namespace>progress/<run id>`) in log order, or None when there is no progress store.
def progress_rows(run, typ):
    db = Path(os.environ["EVALUATE_PROGRESS"])
    if not db.is_file():
        return None
    con = open_read_only(db)
    suffix = "progress/" + run
    rows = con.execute(
        "SELECT position, data, recorded_at FROM events WHERE type = ? "
        "AND substr(stream, -length(?)) = ? ORDER BY position", (typ, suffix, suffix)).fetchall()
    con.close()
    return rows


# The progress lines that report a grep fallback, as {target: [{id, by, line}]} with each line's
# target by GREP_TARGETS, or None when there is no progress store. A line counts only when an
# agent recorded it, so a fallback no agent reported is not counted.
def grep_fallback_lines(run):
    rows = progress_rows(run, "AgentProgress")
    if rows is None:
        return None
    grouped = {name: [] for name, _ in GREP_TARGETS}
    grouped["unclassified"] = []
    for pos, data, _ in rows:
        record = decode(data)
        line = str(record.get("activity") or "").strip()
        if not line.startswith(GREP_FALLBACK):
            continue
        line = line[len(GREP_FALLBACK):].strip()
        target = next((name for name, rule in GREP_TARGETS if rule.search(line)), "unclassified")
        grouped[target].append({"id": f"progress:{pos}", "by": record.get("id") or "", "line": line})
    return grouped


# The driver's heartbeat gaps longer than BEAT_GAP_S, as (start, end) seconds, or None when the run
# recorded no heartbeat.
def beat_gaps(run):
    times = sorted(at / 1e9 for _, _, at in progress_rows(run, "SupervisorBeat") or [])
    if not times:
        return None
    return [(a, b) for a, b in zip(times, times[1:]) if b - a > BEAT_GAP_S]


def run_stream(con):
    named = os.environ.get("EVALUATE_STREAM", "")
    if named:
        return named
    streams = [row[0] for row in con.execute(
        "SELECT DISTINCT stream FROM events WHERE type = 'RunStarted' "
        "AND stream LIKE 'proj-%-run'")]
    if len(streams) != 1:
        refuse(f"expected one run stream, found {streams or 'none'}; set EVALUATE_STREAM")
    return streams[0]


# The run's slice: from its first RunStarted to the first RunStarted of another run (open-ended
# for the newest run), so a relaunch of the same run stays inside its own slice.
def run_slice(con, stream, wanted):
    starts = [(pos, decode(data).get("run", "")) for pos, data in con.execute(
        "SELECT position, data FROM events WHERE stream = ? AND type = 'RunStarted' "
        "ORDER BY position", (stream,))]
    if not starts:
        refuse(f"stream {stream} holds no RunStarted")
    if wanted in ("", "latest"):
        run = starts[-1][1]
    else:
        hits = sorted({r for _, r in starts if r.startswith(wanted)})
        if len(hits) != 1:
            refuse(f"run {wanted!r} matches {hits or 'no run'}; name one run")
        run = hits[0]
    lo = next(pos for pos, r in starts if r == run)
    hi = next((pos for pos, r in starts if pos > lo and r != run), None)
    return run, lo, hi


def slice_events(con, stream, lo, hi):
    sql = ("SELECT position, type, data, meta, recorded_at FROM events WHERE stream = ? "
           "AND position >= ? AND type IN (" + ",".join("?" * len(TYPES)) + ")")
    args = [stream, lo, *TYPES]
    if hi is not None:
        sql += " AND position < ?"
        args.append(hi)
    for pos, typ, data, meta, at in con.execute(sql + " ORDER BY position", args):
        yield pos, typ, decode(data), decode(meta), at


def last_json_line(text):
    for line in reversed(text.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            value = decode(line)
            if value:
                return value
    return {}


# The title of the spec file RunStarted names: its first heading in the checkout, or its path when
# the checkout no longer holds it.
def spec_title(path):
    try:
        body = (Path(os.environ["EVALUATE_ROOT"]) / path).read_text(encoding="utf-8")
    except (OSError, ValueError):
        return path
    for line in body.splitlines():
        if line.startswith("# "):
            return line[2:].strip()
    return path


class Fold:
    """One pass over a run's slice, keeping what the scorecard and the evidence read."""

    def __init__(self):
        self.started_ns = None
        self.last_ns = None
        self.spec = ""
        self.criterion = {}
        self.needs = {}
        self.held = None
        self.holds = []
        self.integrated = set()
        self.review_only = set()
        self.last_landing_ns = None
        self.failures = []
        self.failure_at = {}
        self.relaunches = []
        self.escalations = []
        self.deferred = []
        self.red_verdicts = Counter()
        self.verdict_cause = {}
        self.reject_cause = {}
        self.open_spawns = {}
        self.spawns = []
        self.timeline = {}

    # Each unit's events in log order, as (seconds, type, info, position), for the time shares and
    # the cost. A gate verdict belongs to the unit its replay key opens with; a unit's start to the
    # unit its branch names. A verdict's commit is the one its input digest
    # (`<command digest>:<commit>`) names.
    def track(self, pos, typ, data, meta, at):
        if typ in ("RunStarted", "RunHeld", "RunReleased", "UnitFailed", "UnitEscalated",
                   "DeferredGateFailed"):
            return
        if typ == "GateVerdict":
            key = str(meta.get("replay_key") or "")
            unit = key.split("/")[0]
            digest = str(meta.get("input_digest") or "")
            commit = digest.rpartition(":")[2]
            info = {"cache": "cache_hit_of" in meta, "post": "postmerge" in key,
                    "gate": str(data.get("gate") or ""), "skipped": bool(data.get("skipped")),
                    "digest": digest, "commit": commit if COMMIT.match(commit) else ""}
        else:
            unit = data.get("unit") or str(data.get("id") or "").split("/")[0]
            if typ == "UnitStarted":
                unit = str(data.get("branch") or "").replace("rigger/u/", "") or unit
            info = data.get("status") if typ == "UnitStatus" else str(data.get("id") or "")
        self.timeline.setdefault(unit, []).append((at / 1e9, typ, info, pos))

    def add(self, pos, typ, data, meta, at):
        self.track(pos, typ, data, meta, at)
        self.last_ns = at
        if typ == "RunStarted":
            if self.started_ns is None:
                self.started_ns = at
                self.spec = spec_title(data.get("spec") or "")
        elif typ == "RunHeld":
            # A hold stands until the next release; a second hold inside it keeps the first's author.
            if self.held is None:
                self.held = (at / 1e9, data.get("by") or "")
        elif typ == "RunReleased":
            if self.held is not None:
                self.holds.append((self.held[0], at / 1e9, self.held[1]))
                self.held = None
        elif typ == "UnitStarted":
            self.criterion[data.get("id", "")] = data.get("spec_criterion") or ""
            self.needs.setdefault(data.get("id", ""), data.get("needs"))
        elif typ == "UnitIntegrated":
            if data.get("commit") == REVIEW_ONLY:
                self.review_only.add(data.get("id", ""))
            else:
                self.integrated.add(data.get("id", ""))
                self.last_landing_ns = at
        elif typ == "UnitStatus":
            # A relaunch continues an interrupted launch under a new session id, so the session
            # the spawn continued differs from the one it ran as.
            resumed = meta.get("resumed_from")
            if data.get("status") == "spawn-session" and resumed and resumed != meta.get("session_id"):
                self.relaunches.append((pos, str(meta.get("spawn") or "")))
        elif typ == "UnitFailed":
            self.failures.append((pos, data))
            self.failure_at[pos] = at / 1e9
            unit = data.get("id", "")
            # A review reject takes the cause the unit's last adjudicator reject stated.
            if data.get("cause") == "reject" and unit in self.verdict_cause:
                self.reject_cause[pos] = self.verdict_cause.pop(unit)
        elif typ == "UnitEscalated":
            self.escalations.append((pos, data))
        elif typ == "DeferredGateFailed":
            self.deferred.append((pos, data))
        elif typ == "GateVerdict":
            # A skip, or a legacy artifact-tagged landing record, is not a gate run.
            if not data.get("pass", False) and not data.get("artifact") and not data.get("skipped"):
                self.red_verdicts[data.get("gate", "")] += 1
        elif typ == "SpawnRequested":
            self.open_spawns[data.get("id", "")] = at
        elif typ == "SpawnResult":
            spawn = data.get("id", "")
            match = SPAWN_ID.match(spawn)
            start = self.open_spawns.pop(spawn, None)
            if start is not None:
                attempt = int(match["attempt"]) if match else 0
                self.spawns.append((start, min(max(at - start, 0), CAP_NS), attempt))
            if match and match["role"] == "adjudicator":
                verdict = last_json_line(data.get("output") or "")
                if verdict.get("verdict") == "reject":
                    self.verdict_cause[match["unit"]] = verdict.get("cause") or "unstated"


def folded(wanted):
    con = store()
    stream = run_stream(con)
    run, lo, hi = run_slice(con, stream, wanted)
    fold = Fold()
    for event in slice_events(con, stream, lo, hi):
        fold.add(*event)
    con.close()
    return fold, run, stream, lo, hi


def union_ns(spans):
    total, end = 0, None
    for start, length, _ in sorted(spans):
        stop = start + length
        if end is None or start > end:
            total += length
            end = stop
        elif stop > end:
            total += stop - end
            end = stop
    return total


def item_class(item):
    text = (item.get("finding") or "").strip().lower() if isinstance(item, dict) else ""
    for prefix, name in ITEM_CLASSES:
        if text.startswith(prefix):
            return name
    return "unclassified"


def merged(spans):
    out = []
    for a, b in sorted(spans):
        if out and a <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], b))
        elif b > a:
            out.append((a, b))
    return out


def minus(spans, cover):
    out, cover = [], merged(cover)
    for a, b in merged(spans):
        for c, d in cover:
            if d <= a or c >= b:
                continue
            if c > a:
                out.append((a, c))
            a = max(a, d)
            if a >= b:
                break
        if a < b:
            out.append((a, b))
    return out


def overlap(spans, a, b):
    return sum(max(0, min(b, d) - max(a, c)) for c, d in spans)


# One unit's agent sessions and gate batteries, in seconds, from its events in time order (log
# order on a tie). A gate battery runs from a writer's result to the last fresh (not cache-hit)
# verdict before the unit's next spawn request; a battery whose last verdict is post-merge is
# post-merge.
def unit_work(events):
    events = sorted(events, key=lambda event: event[0])
    spans, requested = [], {}
    for t, typ, info, _ in events:
        if typ == "SpawnRequested":
            requested[info] = t
        elif typ == "SpawnResult" and info in requested:
            spans.append((requested.pop(info), t))
    batteries = []

    def close(current):
        opened, opener, verdicts = current
        fresh = [v for v in verdicts if not v[1]["cache"]]
        if fresh:
            last, info, _ = fresh[-1]
            batteries.append(Battery(opened, last, info["post"], opener,
                                     [v for v in verdicts if v[0] <= last]))

    current = None
    for t, typ, info, pos in events:
        if typ == "SpawnResult" and any(w in info for w in WRITERS):
            current = (t, info, [])
        elif typ == "GateVerdict" and current is not None:
            current[2].append((t, info, pos))
        elif typ == "SpawnRequested" and current is not None:
            close(current)
            current = None
    if current is not None:
        close(current)
    return events, spans, batteries


# The run's outages as merged (start, end) seconds, and what they were read from. A heartbeat gap
# longer than BEAT_GAP_S is one: the driver was not running. A hold the host raised (a rate limit,
# a billing or a login fault) is one throughout: the session it stopped waits for the release. A
# hold the operator raised lets a running session or gate battery finish and stops only the next
# launch, so it is one only while no unit has either open. A hold still standing at the slice's
# end runs to its last event. ([], "none recorded") when the run recorded no heartbeat and no hold.
def outages(fold, work, run):
    gaps = beat_gaps(run)
    holds = list(fold.holds)
    if fold.held is not None:
        holds.append((fold.held[0], fold.last_ns / 1e9, fold.held[1]))
    if gaps is None and not holds:
        return [], "none recorded"
    busy = [(a, b) for _, spans, batteries in work for a, b in
            spans + [(battery.opened, battery.last) for battery in batteries]]
    by_host = [(a, b) for a, b, by in holds if by != "operator"]
    by_operator = minus([(a, b) for a, b, by in holds if by == "operator"], busy)
    source = "heartbeat and holds" if gaps is not None else "holds only: no heartbeat recorded"
    return merged((gaps or []) + by_host + by_operator), source


# One integrated unit's live seconds (its first start or spawn request to its last integration,
# outages removed) and how each was spent. Each second takes one label: an outage over an agent
# session (`merge_spawn` once the unit has tried a merge) over a gate battery over integration
# (its last review to its integration); a second with none is waiting. Outage seconds are then
# dropped. None when the unit has no start or no integration in the slice.
def unit_partition(events, spans, batteries, dead):
    t0 = min((t for t, typ, _, _ in events if typ in ("UnitStarted", "SpawnRequested")),
             default=None)
    t1 = max((t for t, typ, _, _ in events if typ == "UnitIntegrated"), default=None)
    if t0 is None or t1 is None:
        return None
    merging = min((t for t, typ, info, _ in events
                   if typ == "UnitStatus" and info == "integrate-merge-attempt"), default=None)
    reviewed = max((t for t, typ, info, _ in events
                    if typ == "UnitStatus" and info == "reviewed"), default=t1)
    label = {}

    def paint(a, b, name, rank):
        for second in range(int(a), int(b)):
            if second not in label or label[second][1] > rank:
                label[second] = (name, rank)

    for a, b in dead:
        paint(max(a, t0), min(b, t1), "outage", -1)
    for a, b in spans:
        paint(a, b, "merge_spawn" if merging is not None and a >= merging else "spawn", 0)
    for battery in batteries:
        paint(battery.opened, battery.last, "post_merge_gates" if battery.post else "gates", 1)
    paint(reviewed, t1, "integration", 2)
    part = Counter(label.get(s, ("waiting", 9))[0] for s in range(int(t0), int(t1)))
    dropped = part.pop("outage", 0)
    return max(int(t1) - int(t0) - dropped, 0), part


# ---- cost ----

def live(a, b, dead):
    """The seconds of [a, b) outside the outages."""
    return max(b - a, 0) - overlap(dead, a, b)


def attempt_of(spawn):
    match = SPAWN_ID.match(spawn or "")
    return int(match["attempt"]) if match else 0


# Each commit's tree as the repository resolves it, or None for a commit it cannot resolve.
def trees_of(commits):
    commits = sorted({c for c in commits if c})
    if not commits:
        return {}
    try:
        lines = subprocess.run(
            ["git", "-C", os.environ["EVALUATE_ROOT"], "cat-file", "--batch-check=%(objectname)"],
            input="".join(c + "^{tree}\n" for c in commits), capture_output=True, text=True,
            check=True).stdout.splitlines()
    except (OSError, subprocess.CalledProcessError):
        return {c: None for c in commits}
    return {c: (line if COMMIT.match(line) else None) for c, line in zip(commits, lines)}


def unit_hours(seconds):
    return round(seconds / 3600, 3)


# What the integrated units' time cost and what of it repeated, from the same partition as the
# time shares: hours by unit, label, battery and gate, and three repeats the log can show.
#   post_merge_same_tree: a post-merge battery over the same tree as the unit's last pre-merge
#     battery, so the merge changed nothing it gates (null when a tree does not resolve);
#   retry_batteries: a pre-merge battery opened by a writer at attempt 1 or later, re-running the
#     battery after a failure;
#   same_input_reruns: a fresh verdict of a gate over an input digest (command and commit) an
#     earlier fresh verdict of that gate already judged.
def cost(fold, work, dead):
    units, batteries, gates, by_label = [], [], {}, Counter()
    judged, reruns = set(), [0.0, []]
    for unit, (events, spans, unit_batteries) in zip(fold.timeline, work):
        partition = unit in fold.integrated and unit_partition(events, spans, unit_batteries, dead)
        if not partition:
            continue
        by_label.update(partition[1])
        attempts = 1 + max((attempt_of(info) for _, typ, info, _ in events
                            if typ == "SpawnRequested" and any(w in info for w in WRITERS)),
                           default=0)
        units.append({"unit": unit, "positions": [events[0][3], events[-1][3]],
                      "attempts": attempts, "batteries": len(unit_batteries),
                      "hours": {k: unit_hours(v) for k, v in sorted(partition[1].items())}})
        pre_merge_commit = ""
        for battery in unit_batteries:
            previous, counts, fresh = battery.opened, Counter(), []
            for t, info, pos in battery.verdicts:
                kind = "cache" if info["cache"] else "skipped" if info["skipped"] else "fresh"
                counts[kind] += 1
                if kind == "fresh":
                    seconds = live(previous, t, dead)
                    fresh.append((pos, info))
                    gate = gates.setdefault(info["gate"], Counter())
                    gate["runs"] += 1
                    gate["seconds"] += seconds
                    phase = "post_merge" if battery.post else (
                        "retry" if attempt_of(battery.opener) >= 1 else "")
                    if phase:
                        gate[phase + "_runs"] += 1
                        gate[phase + "_seconds"] += seconds
                    key = (info["gate"], info["digest"])
                    if info["digest"] and key in judged:
                        reruns[0] += seconds
                        reruns[1].append(pos)
                    judged.add(key)
                previous = t
            commit = next((info["commit"] for _, info in reversed(fresh) if info["commit"]), "")
            batteries.append({
                "unit": unit, "kind": "post-merge" if battery.post else "pre-merge",
                "opened_by": battery.opener, "attempt": attempt_of(battery.opener),
                "hours": unit_hours(live(battery.opened, battery.last, dead)),
                "verdicts": dict(sorted(counts.items())), "commit": commit,
                "pre_merge_commit": pre_merge_commit if battery.post else None,
                "positions": [pos for pos, _ in fresh]})
            if not battery.post and commit:
                pre_merge_commit = commit
    trees = trees_of([b["commit"] for b in batteries] + [b["pre_merge_commit"] or "" for b in batteries])
    for b in batteries:
        if b["kind"] == "post-merge":
            mine, theirs = trees.get(b["commit"]), trees.get(b["pre_merge_commit"] or "")
            b["same_tree_as_pre_merge"] = mine == theirs if mine and theirs else None

    def repeat(chosen):
        return {"hours": round(sum(b["hours"] for b in chosen), 3), "batteries": len(chosen),
                "evidence": [b["positions"][0] for b in chosen if b["positions"]]}

    post = [b for b in batteries if b["kind"] == "post-merge"]
    unresolved = sum(1 for b in post if b["same_tree_as_pre_merge"] is None)
    same_tree = repeat([b for b in post if b["same_tree_as_pre_merge"]])
    if unresolved:
        same_tree["hours"] = None
    repeats = {
        "post_merge_same_tree": same_tree,
        "retry_batteries": repeat([b for b in batteries
                                   if b["kind"] == "pre-merge" and b["attempt"] >= 1]),
        "same_input_reruns": {"hours": unit_hours(reruns[0]), "verdicts": len(reruns[1]),
                              "evidence": reruns[1]},
    }
    gate_rows = []
    for name, c in sorted(gates.items()):
        row = {"gate": name, "runs": c["runs"], "hours": unit_hours(c["seconds"])}
        for phase in ("retry", "post_merge"):
            row[phase + "_runs"] = c[phase + "_runs"]
            row[phase + "_hours"] = unit_hours(c[phase + "_seconds"])
        gate_rows.append(row)
    return {
        "hours_by_label": {k: unit_hours(v) for k, v in sorted(by_label.items())},
        "gate_hours_by_gate": {row["gate"]: row["hours"] for row in gate_rows},
        "repeated_hours": {k: v["hours"] for k, v in repeats.items()},
        "unresolved_trees": unresolved,
        "units": units, "batteries": batteries, "gates": gate_rows, "repeats": repeats,
        "relaunched_sessions": {"count": len(fold.relaunches),
                                "evidence": [pos for pos, _ in fold.relaunches],
                                "spawns": [spawn for _, spawn in fold.relaunches]},
    }


# Each failure's rework: its unit's live hours from the failure to the unit's next failure, its
# next merge attempt or its integration, whichever comes first (the unit's last event when none
# follows). Keyed by the failure's position.
def failure_rework(fold, dead):
    out = {}
    for pos, data in fold.failures:
        unit, t = data.get("id", ""), fold.failure_at[pos]
        events = fold.timeline.get(unit, [])
        ends = [e[0] for e in events if e[0] > t and (
            e[1] == "UnitIntegrated" or (e[1] == "UnitStatus" and e[2] == "integrate-merge-attempt"))]
        ends += [fold.failure_at[p] for p, d in fold.failures
                 if d.get("id", "") == unit and fold.failure_at[p] > t]
        end = min(ends, default=max((e[0] for e in events), default=t))
        out[str(pos)] = unit_hours(live(t, end, dead))
    return out


# The run's work per unit and its outages: what the scorecard and the evidence both read.
def run_work(fold, run):
    work = [unit_work(events) for events in fold.timeline.values()]
    dead, source = outages(fold, work, run)
    return work, dead, source


# A unit that joins a stage (its `needs` names no unit of the run, the way the check-in waits on
# every unit the fan-out stage expanded into) checks the whole spec rather than one criterion.
def joins_a_stage(fold, unit):
    needs = fold.needs.get(unit)
    return isinstance(needs, list) and any(n not in fold.criterion for n in needs)


def repeated_label(spent):
    unresolved = spent["unresolved_trees"]
    return ("unit hours of the batteries or verdicts that repeated earlier work; "
            + (f"post_merge_same_tree is null: {unresolved} post-merge battery tree(s) did not "
               "resolve in this repository" if unresolved else "every battery tree resolved"))


def scorecard(fold, run, stream, lo, hi):
    n = len(fold.integrated)
    failed_units = set()
    by_cause, items, reject_causes, gate_fails_by = Counter(), Counter(), Counter(), Counter()
    rejects = gate_fails = failures_on_integrated = 0
    for pos, data in fold.failures:
        unit, cause = data.get("id", ""), data.get("cause") or ""
        failed_units.add(unit)
        by_cause[cause or "none"] += 1
        if unit in fold.integrated:
            failures_on_integrated += 1
        if cause.startswith("gate:"):
            gate_fails += 1
            gate_fails_by[cause[len("gate:"):]] += 1
        if cause == "reject":
            rejects += 1
            reject_causes[fold.reject_cause.get(pos, "unpaired")] += 1
            required = data.get("required") or []
            if not required:
                items["no-items"] += 1
            for item in required:
                items[item_class(item)] += 1
    criterion_units = [u for u in fold.integrated
                       if fold.criterion.get(u) and not joins_a_stage(fold, u)]
    work, dead, outage_source = run_work(fold, run)
    spent = cost(fold, work, dead)
    grep_lines = grep_fallback_lines(run)
    wall = outage = live_wall = None
    if fold.started_ns is not None and fold.last_landing_ns is not None:
        start, end = fold.started_ns / 1e9, fold.last_landing_ns / 1e9
        wall = (end - start) / 3600
        if outage_source != "none recorded":
            outage = overlap(dead, start, end) / 3600
            live_wall = wall - outage
    spawn_total = sum(length for _, length, _ in fold.spawns)
    rework = sum(length for _, length, attempt in fold.spawns if attempt >= 1)
    busy = union_ns(fold.spawns) / NS_PER_HOUR if fold.spawns else None
    elapsed, parts = 0, Counter()
    for unit, (events, spans, batteries) in zip(fold.timeline, work):
        partition = unit in fold.integrated and unit_partition(events, spans, batteries, dead)
        if partition:
            elapsed += partition[0]
            parts.update(partition[1])
    # A slice with no spawn record cannot say how its time was spent, only how much passed.
    share = lambda label: ratio(parts[label], elapsed) if fold.spawns else None
    if not fold.spawns:
        shares_label = "unmeasured: the slice has no spawn record"
    elif outage_source == "none recorded":
        shares_label = ("approximate: no heartbeat or hold is recorded, so a driver outage "
                        "counts under the label that covers it, usually waiting")
    else:
        shares_label = f"by the method: driver outages ({outage_source}) are subtracted"
    open_units = set(fold.criterion) - fold.integrated - fold.review_only
    checked_in = any(joins_a_stage(fold, u) for u in fold.integrated)
    state = "landed" if checked_in and not open_units else (
        f"interim: {len(open_units)} started unit(s) not integrated, "
        f"check-in {'landed' if checked_in else 'not landed'}")
    hours = lambda value: round(value, 3) if value is not None else None
    per = lambda value, units: ratio(value, units) if value is not None else None
    return {
        "run": run,
        "spec": fold.spec,
        "stream": stream,
        "positions": [lo, hi],
        "units_integrated": n,
        "criterion_units": len(criterion_units),
        "units_not_integrated": len(open_units),
        "escalations": len({data.get("id", "") for _, data in fold.escalations}),
        "first_pass_rate": ratio(len(fold.integrated - failed_units), n),
        "failures_on_integrated": failures_on_integrated,
        "rejects": rejects,
        "gate_fails": gate_fails,
        "attempts_per_unit": ratio(n + failures_on_integrated, n),
        "rejects_per_unit": ratio(rejects, n),
        "gate_fails_per_unit": ratio(gate_fails, n),
        "agent_busy_hours": hours(busy),
        "agent_busy_hours_per_unit": per(busy, n),
        "wall_hours": hours(wall),
        "wall_hours_per_unit": per(wall, n),
        "wall_hours_per_criterion_unit": per(wall, len(criterion_units)),
        "outage_source": outage_source,
        "outage_hours": hours(outage),
        "live_wall_hours_per_criterion_unit": per(live_wall, len(criterion_units)),
        "rework_share": ratio(rework, spawn_total),
        "unit_elapsed_hours": round(elapsed / 3600, 3) if elapsed else None,
        "gates_share": share("gates"),
        "post_merge_gates_share": share("post_merge_gates"),
        "time_shares": {k: share(k) for k in sorted(parts)} if fold.spawns else None,
        "grounding_share": None,
        "grep_fallbacks": sum(map(len, grep_lines.values())) if grep_lines is not None else None,
        "hours_by_label": spent["hours_by_label"] if fold.spawns else None,
        "gate_hours_by_gate": spent["gate_hours_by_gate"],
        "repeated_hours": spent["repeated_hours"],
        "relaunched_sessions": spent["relaunched_sessions"]["count"],
        "failure_rework_hours": failure_rework(fold, dead),
        "grep_fallbacks_by_target":
            {k: len(v) for k, v in grep_lines.items()} if grep_lines is not None else None,
        "failures_by_cause": dict(sorted(by_cause.items())),
        "reject_items_by_class": dict(sorted(items.items())),
        "rejects_by_cause": dict(sorted(reject_causes.items())),
        "gate_fails_by_gate": dict(sorted(gate_fails_by.items())),
        "deferred_gate_fails_by_gate":
            dict(sorted(Counter(d.get("gate", "") for _, d in fold.deferred).items())),
        "red_verdicts_by_gate": dict(sorted(fold.red_verdicts.items())),
        "unsourced": UNSOURCED,
        "labels": {"run": state, "time_shares": shares_label, "grep_fallbacks": SELF_REPORTED,
                   "hours_by_label": UNIT_HOURS, "gate_hours_by_gate": GATE_HOURS,
                   "repeated_hours": repeated_label(spent), "failure_rework_hours": REWORK_HOURS,
                   "grep_fallbacks_by_target": GREP_GROUPING},
    }


# ---- run files ----

def blocks_of(text):
    found, repeated = {}, []
    for name, body in BLOCK.findall(text):
        if name in found:
            repeated.append(name)
        try:
            found[name] = json.loads(body)
        except ValueError:
            found[name] = ValueError
    return found, repeated


def run_files(directory):
    return sorted(p for p in directory.iterdir() if RUN_FILE.match(p.name))


# The run files a run file named `name` follows: those under docs/evaluation/ that sort before it.
# measure and check both select through here, wherever the file being written or checked lives.
def earlier_run_files(name):
    directory = Path(os.environ["EVALUATE_ROOT"]) / "docs" / "evaluation"
    return [p for p in run_files(directory) if p.name < name] if directory.is_dir() else []


def metric_value(card, key):
    if not isinstance(card, dict) or not isinstance(key, str):
        return None
    if is_number(card.get(key)):
        return card[key]
    head, _, sub = key.partition(".")
    if head in BY_MAPS and sub and isinstance(card.get(head), dict):
        value = card[head].get(sub, 0)
        return value if is_number(value) else None
    return None


def score_previous(card, previous_path):
    if previous_path is None:
        return {"file": None, "adopted": "none"}
    blocks, _ = blocks_of(previous_path.read_text(encoding="utf-8"))
    proposals = blocks.get("proposals")
    adopted = proposals.get("adopted") if isinstance(proposals, dict) else None
    if adopted in (None, "none"):
        return {"file": previous_path.name, "adopted": "none"}
    chosen = [p for p in proposals.get("proposals") or [] if isinstance(p, dict)
              and p.get("id") == adopted]
    if not chosen:
        refuse(f"{previous_path.name} adopts {adopted!r}, which it does not propose")
    proposal = chosen[0]
    metric, predicted = proposal.get("metric"), proposal.get("predicted_move")
    was, now = metric_value(blocks.get("scorecard"), metric), metric_value(card, metric)
    move = round(now - was, 3) if is_number(was) and is_number(now) else None
    kept = is_number(predicted) and move not in (None, 0) and (move > 0) == (predicted > 0)
    return {"file": previous_path.name, "adopted": adopted, "class": proposal.get("class"),
            "metric": metric, "predicted_move": predicted, "before": was, "after": now,
            "actual_move": move, "verdict": "kept" if kept else "reverted"}


def baseline_of(directory):
    path = directory / "baseline.md"
    if not path.is_file():
        return {}
    value = blocks_of(path.read_text(encoding="utf-8"))[0].get("baseline")
    return value if isinstance(value, dict) else {}


def comparison(card, history, baseline):
    columns = [("baseline", baseline)]
    columns += [(name[14:22], c) for name, c in history]
    columns.append(("this run", card))
    cell = lambda v: "-" if not is_number(v) else str(v)
    rows = ["| metric | " + " | ".join(label for label, _ in columns) + " |",
            "|---" * (len(columns) + 1) + "|"]
    for key in COMPARED:
        rows.append(f"| {key} | " + " | ".join(cell(c.get(key)) for _, c in columns) + " |")
    return "\n".join(rows)


def measure(wanted, out):
    fold, run, stream, lo, hi = folded(wanted)
    card = scorecard(fold, run, stream, lo, hi)
    directory = Path(os.environ["EVALUATE_ROOT"]) / "docs" / "evaluation"
    if fold.started_ns is None:
        refuse(f"run {run} has no RunStarted in its slice")
    stamp = datetime.fromtimestamp(fold.started_ns / 1e9, tz=timezone.utc).strftime("%Y%m%d-%H%M")
    own = directory / f"{stamp}-{run[:8]}.md"
    path = Path(out) if out else own
    if path.exists():
        refuse(f"{path} exists; remove it to measure again")
    template = directory / "TEMPLATE.md"
    if not template.is_file():
        refuse(f"no template at {template}")
    earlier = earlier_run_files(own.name)
    history = []
    for p in earlier[-5:]:
        c = blocks_of(p.read_text(encoding="utf-8"))[0].get("scorecard")
        history.append((p.name, c if isinstance(c, dict) else {}))
    previous = score_previous(card, earlier[-1] if earlier else None)
    fill = {
        "run": run,
        "run8": run[:8],
        "spec": card["spec"] or "(untitled spec)",
        "started": local_time(fold.started_ns),
        "last_landing": local_time(fold.last_landing_ns),
        "scorecard": json.dumps(card, indent=1),
        "comparison": comparison(card, history, baseline_of(directory)),
        "previous": json.dumps(previous, indent=1),
    }
    text = template.read_text(encoding="utf-8")
    for key, value in fill.items():
        text = text.replace("{{" + key + "}}", value)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    print(shown(path))


# The evaluator's three sources: the failures (each with its measured rework), the cost (time by
# unit, label, battery and gate, the repeats the log shows, and what it cannot show), and the grep
# fallbacks grouped by what they searched for.
def evidence(wanted):
    fold, run, _, _, _ = folded(wanted)
    work, dead, outage_source = run_work(fold, run)
    rework = failure_rework(fold, dead)
    spent = cost(fold, work, dead)
    grep_lines = grep_fallback_lines(run)
    out = {"run": run, "rejects": [], "gate_failures": [], "other_failures": [],
           "escalations": [], "deferred_gate_failures": [],
           "red_verdicts_by_gate": dict(sorted(fold.red_verdicts.items()))}
    for pos, data in fold.failures:
        cause = data.get("cause") or ""
        record = {"pos": pos, "unit": data.get("id"), "attempts": data.get("attempts"),
                  "cause": cause, "rework_hours": rework[str(pos)]}
        if cause == "reject":
            record["adjudicator_cause"] = fold.reject_cause.get(pos)
            record["review_reason"] = data.get("review_reason")
            record["required"] = data.get("required") or []
            out["rejects"].append(record)
        else:
            record["evidence_tail"] = tail(data.get("gate_evidence") or data.get("review_reason"))
            out["gate_failures" if cause.startswith("gate:") else "other_failures"].append(record)
    for pos, data in fold.escalations:
        out["escalations"].append({"pos": pos, "unit": data.get("id"), "reason": data.get("reason")})
    for pos, data in fold.deferred:
        out["deferred_gate_failures"].append(
            {"pos": pos, "gate": data.get("gate"), "evidence_tail": tail(data.get("evidence"))})
    out["rework_label"] = REWORK_HOURS + "; " + UNIT_HOURS
    out["cost"] = {
        "label": UNIT_HOURS, "outages": outage_source, "gate_hours_label": GATE_HOURS,
        "repeated_label": repeated_label(spent),
        **{k: spent[k] for k in ("hours_by_label", "gate_hours_by_gate", "repeated_hours",
                                 "repeats", "relaunched_sessions", "gates", "units",
                                 "batteries")},
        "not_shown": {"first_edit": UNSOURCED["grounding_share"],
                      "context_restarts": UNSOURCED["context_restarts"]},
    }
    out["grep_fallbacks"] = {"label": GREP_GROUPING, "by_target": grep_lines}
    print(json.dumps(out, indent=1))


# ---- check ----

def check_classes(classes, problems):
    slugs = []
    if not isinstance(classes, list):
        problems.append("the classes block is not a list")
        return slugs
    for i, c in enumerate(classes, 1):
        if not isinstance(c, dict):
            problems.append(f"class {i} is not an object")
            continue
        slug = c.get("class")
        if not isinstance(slug, str) or not SLUG.match(slug):
            problems.append(f"class {i} has no kebab-case slug in `class`")
        elif slug in slugs:
            problems.append(f"class {slug!r} is listed twice")
        else:
            slugs.append(slug)
        if not isinstance(c.get("summary"), str) or not c["summary"].strip():
            problems.append(f"class {i} has no summary")
        if not nonempty_strings(c.get("evidence")):
            problems.append(f"class {i} has no evidence")
    return slugs


def nonempty_strings(value):
    return isinstance(value, list) and value and all(
        isinstance(v, str) and v.strip() for v in value)


def check_proposal(i, p, slugs, recurring, card, problems):
    name = f"P{i}"
    if not isinstance(p, dict):
        problems.append(f"proposal {i} is not an object")
        return
    if p.get("id") != name:
        problems.append(f"proposal {i} has id {p.get('id')!r}; ids run P1, P2, P3 in order")
    if p.get("class") not in slugs:
        problems.append(f"{name}: class {p.get('class')!r} is not one of the diagnosed classes")
    for field in ("change", "path"):
        if not isinstance(p.get(field), str) or not p[field].strip():
            problems.append(f"{name}: no {field}")
    if not nonempty_strings(p.get("evidence")):
        problems.append(f"{name}: no evidence")
    surface, form = p.get("surface"), p.get("form")
    if surface not in SURFACES:
        problems.append(f"{name}: surface {surface!r} is not one of {', '.join(SURFACES)}")
    if form not in FORMS:
        problems.append(f"{name}: form {form!r} is not one of {', '.join(FORMS)}")
    if surface in INSTRUCTION_SURFACES and form in FORMS and form != "instruction":
        problems.append(f"{name}: a {surface} change is instruction text; its form is instruction")
    if surface == "binary" and form == "gate":
        problems.append(f"{name}: the binary gains no gate, since rigger is language and process "
                        "agnostic; a gate is this repository's own, on the gate or workflow surface")
    if p.get("class") in recurring and form not in ("check", "gate"):
        problems.append(f"{name}: class {p.get('class')!r} was diagnosed in the previous run "
                        "file too, so its change must be a check or a gate")
    if metric_value(card, p.get("metric")) is None:
        problems.append(f"{name}: metric {p.get('metric')!r} names no measured number in the "
                        "scorecard")
    if not is_number(p.get("predicted_move")) or p["predicted_move"] == 0:
        problems.append(f"{name}: predicted_move is not a non-zero number")
    if not is_number(p.get("hours_saved")) or p["hours_saved"] <= 0:
        problems.append(f"{name}: hours_saved is not a positive number")


def problems_of(path):
    problems = []
    text = path.read_text(encoding="utf-8")
    if EM_DASH in text:
        problems.append("the file carries an em dash (U+2014); use a hyphen")
    if TODO in text:
        problems.append(f"a section still reads {TODO!r}")
    blocks, repeated = blocks_of(text)
    for name in BLOCKS:
        if name not in blocks:
            problems.append(f"no ```json {name}``` block")
        elif blocks[name] is ValueError:
            problems.append(f"the {name} block is not valid JSON")
        elif blocks[name] is None:
            problems.append(f"the {name} block is still null")
    for name in repeated:
        problems.append(f"the {name} block appears more than once")
    card = blocks.get("scorecard")
    if not isinstance(card, dict) or "run" not in card:
        problems.append("the scorecard block is not a measured scorecard")
        card = {}
    slugs = []
    if blocks.get("classes") not in (None, ValueError):
        slugs = check_classes(blocks["classes"], problems)
    earlier = earlier_run_files(path.name)
    previous_path = earlier[-1] if earlier else None
    recurring = set()
    if previous_path is not None:
        prior = blocks_of(previous_path.read_text(encoding="utf-8"))[0].get("classes")
        recurring = {c.get("class") for c in prior if isinstance(c, dict)} \
            if isinstance(prior, list) else set()
    proposals = blocks.get("proposals")
    if isinstance(proposals, dict):
        listed = proposals.get("proposals")
        if not isinstance(listed, list):
            problems.append("the proposals block has no `proposals` list")
            listed = []
        if len(listed) > 3:
            problems.append(f"{len(listed)} proposals; at most three")
        if slugs and not listed:
            problems.append("classes are diagnosed but nothing is proposed")
        for i, p in enumerate(listed, 1):
            check_proposal(i, p, slugs, recurring, card, problems)
        saved = [p.get("hours_saved") for p in listed if isinstance(p, dict)]
        if all(is_number(s) for s in saved) and saved != sorted(saved, reverse=True):
            problems.append("proposals are not ranked by hours_saved, highest first")
        if "adopted" not in proposals:
            problems.append("the proposals block has no `adopted` field")
        elif proposals["adopted"] not in [None, "none"] + [f"P{i}" for i in range(1, len(listed) + 1)]:
            problems.append(f"adopted {proposals['adopted']!r} is not null, \"none\" or a proposal id")
    elif proposals not in (None, ValueError) and "proposals" in blocks:
        problems.append("the proposals block is not an object")
    if card and blocks.get("previous") not in (None, ValueError):
        expected = score_previous(card, previous_path)
        if blocks["previous"] != expected:
            problems.append("the previous block is not the scoring measure computes from "
                            f"{previous_path.name if previous_path else 'no previous run file'}: "
                            + json.dumps(expected))
    return problems, blocks


def chosen_file(arg):
    if arg:
        path = Path(arg)
        if not path.is_file() and not path.is_absolute():
            path = Path(os.environ["EVALUATE_ROOT"]) / arg
    else:
        files = run_files(Path(os.environ["EVALUATE_ROOT"]) / "docs" / "evaluation")
        if not files:
            refuse("no run file under docs/evaluation/")
        path = files[-1]
    if not path.is_file():
        refuse(f"no run file at {path}")
    return path


def check(arg):
    path = chosen_file(arg)
    problems, _ = problems_of(path)
    for problem in problems:
        print(f"evaluate check FAILED: {path.name}: {problem}")
    if problems:
        sys.exit(1)
    print(f"evaluate check: {path.name} passes")


def emit(arg):
    path = chosen_file(arg)
    problems, blocks = problems_of(path)
    if problems:
        refuse(f"{path.name} fails check; run check and fix it before emitting")
    record = shown(path)
    run8 = blocks["scorecard"]["run"][:8]
    for p in blocks["proposals"]["proposals"]:
        summary = (f"[{p['class']}] {p['change']} Moves {p['metric']} by {p['predicted_move']}; "
                   f"about {p['hours_saved']} h saved per run. Surface: {p['surface']} "
                   f"({p['form']}).")
        print(json.dumps({"id": f"eval-{run8}-{p['id']}", "by": "evaluator",
                          "summary": summary, "about": [p["path"], record]}))


def persona():
    text = (Path(os.environ["EVALUATE_ROOT"]) / ".rigger" / "agents" / "evaluator.md").read_text(
        encoding="utf-8")
    if not text.startswith("---"):
        refuse("evaluator.md has no frontmatter")
    rest = text[3:].lstrip("\n")
    end = rest.find("\n---")
    if end < 0:
        refuse("evaluator.md has an unterminated frontmatter")
    print(rest[end + 4:].strip())


mode, args = sys.argv[1], sys.argv[2:] + ["", ""]
if mode == "measure":
    measure(args[0], args[1])
elif mode == "evidence":
    evidence(args[0])
elif mode == "check":
    check(args[0])
elif mode == "emit":
    emit(args[0])
elif mode == "persona":
    persona()
PY
}

case "$1" in
measure | evidence | check)
    evaluate_py "$@"
    ;;
emit)
    findings=$(evaluate_py emit "${2:-}") || { echo "$findings"; exit 1; }
    printf '%s\n' "$findings" | while IFS= read -r finding; do
        test -n "$finding" || continue
        echo "+ rigger emit ReviewFinding $finding"
        rigger emit ReviewFinding "$finding" || exit 1
    done || exit 1
    ;;
run)
    file=$(evaluate_py measure "${2:-latest}") || { echo "$file"; exit 1; }
    echo "evaluate: measured $file"
    if [ -z "${EVALUATE_AGENT:-}" ]; then
        echo "evaluate: EVALUATE_AGENT is unset; run the evaluator on $file by hand, then"
        echo "  sh .rigger/gates/evaluate.sh check $file"
        exit 0
    fi
    scratch=$root/.rigger/tmp/evaluate
    rm -rf "$scratch" && mkdir -p "$scratch" || exit 1
    evaluate_py persona > "$scratch/evaluator.prompt" || { cat "$scratch/evaluator.prompt"; rm -rf "$scratch"; exit 1; }
    printf '%s\n' "Evaluate the landed run whose run file is $file. Its measured sections are filled; read the evidence with \`sh .rigger/gates/evaluate.sh evidence ${2:-latest}\`, write the Diagnosis, classes and proposals, and run \`sh .rigger/gates/evaluate.sh check $file\` until it passes." |
        $EVALUATE_AGENT "$scratch/evaluator.prompt"
    status=$?
    rm -rf "$scratch"
    test "$status" -eq 0 || { echo "evaluate: the evaluator exited $status"; exit 1; }
    evaluate_py check "$file" || exit 1
    echo "evaluate: commit $file, then: sh .rigger/gates/evaluate.sh emit $file"
    ;;
esac
