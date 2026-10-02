//! Periphery (behavioral) test for spec 89, criterion 5 (PER-UNIT PIPELINING): "with two units
//! in one wave, the unit whose result lands first is reviewed while the other still builds, no
//! running item is spawned twice, and the run reaches the same fixpoint."
//!
//! WHY THIS FILE, DISTINCT FROM THE EXISTING SOURCE-FIXTURE TESTS. `tests/cli.rs` already carries
//! several `native_driver_*` tests (the implementer's own, following this file's established
//! convention for `workflows/rigger.js`: it runs only under the Workflow harness - top-level
//! await, injected `agent`/`parallel`/`log` globals - so it cannot execute under `cargo test`,
//! and every existing proof is a SOURCE FIXTURE: `code.contains("inFlight.set(req.id,")`, an
//! `rfind` position ordering, and so on. Those pin the SHAPE of the code. None of them ever RUN
//! the pipelining logic, so none can catch a defect that leaves the text looking right while the
//! actual runtime behavior is wrong - e.g. the in-flight filter reading the set at the wrong
//! point in time, `Promise.race` resolving on the wrong collection, or a str-match-passing
//! rewrite that still serializes the two units in practice. That is exactly the boundary this
//! file closes: it EXECUTES the real driver body (read fresh from `workflows/rigger.js`, not
//! copied) inside a `node` `vm` context under a scripted two-unit wave, and proves the emergent
//! behavior the criterion names, not just the text that is supposed to produce it.
//!
//! MECHANISM. The driver script is a top-level, module-scoped program (`export const meta = ...`
//! followed by top-level `await`s and a trailing `return { waves }`), executed by the real
//! Workflow harness under machinery this crate does not have access to. This harness slices off
//! only the `export const meta` block (ES-module syntax invalid in a plain `vm` script) and runs
//! the remainder - the `args` resolution, every helper, and the `for (;;)` loop itself - as the
//! body of one async IIFE inside a fresh `vm.createContext`, whose only globals are the four a
//! real workflow run injects (`args`, `agent`, `parallel`, `log`) plus `setTimeout`/`clearTimeout`
//! (used by the marker-staleness and outer-wall-clock races). The mocks and every assertion live
//! in the OUTER, unsandboxed node process - never inside the executed string - so the test's own
//! bookkeeping (call counts, millisecond timestamps) is plain host JS, not text the sandbox could
//! see or influence.
//!
//! SCENARIO. Two units, `u1` (fast: a 20ms build, a 20ms review) and `u2` (slow: a 300ms build,
//! a 20ms review), land in ONE wave. The scripted `rigger step` responses mirror the REAL
//! contract `step_result` (`src/spawn.rs`) documents and this unit's own commit relies on: the
//! FULL PENDING FRONTIER on every call, so `u2/implementer#1` is listed again, verbatim, on the
//! two steps that courier while it is still running - exactly what makes the in-flight guard
//! load-bearing rather than a no-op.

mod common;

use common::fixtures::tool_available;
use common::repo::repo_text;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The driver BODY: `workflows/rigger.js`, read fresh at test time (never copied inline, so an
/// edit to the shipped driver is exercised without touching this file), sliced from the `args`
/// resolution onward, through the top-level `for (;;)` loop and its trailing `return { waves }`,
/// with the leading `export const meta = {...}` ES-module export cut off. `export` is invalid
/// inside a plain script/`vm` context, and `meta` (the workflow's display metadata) carries none
/// of the loop's own logic, so slicing it off loses nothing this test needs.
fn rigger_js_driver_body() -> String {
    let src = repo_text("workflows/rigger.js");
    let anchor = "// args: a spec path string, or { repo, spec, base, fresh }.";
    let at = src.find(anchor).unwrap_or_else(|| {
        panic!(
            "the driver source must still carry its `args` anchor comment so this harness can \
             slice off the ES-module export ahead of it: {anchor}\n(if this comment's wording \
             changed, update the anchor here to match)"
        )
    });
    src[at..].to_string()
}

/// The complete node harness: reads the driver body from `argv[2]`, scripts a two-unit wave
/// (`u1` fast, `u2` slow) through five successive `rigger step` responses shaped like the real
/// full-pending-frontier contract, runs the driver body inside a `vm` context exposing only
/// `args`/`agent`/`parallel`/`log`/`setTimeout`/`clearTimeout`, and asserts the criterion's own
/// three clauses before printing `ok_token`. See the module doc comment for the full mechanism
/// and scenario.
const HARNESS: &str = r#"
"use strict";
const vm = require("vm");
const fs = require("fs");

const driverBody = fs.readFileSync(process.argv[2], "utf8");

const T0 = Date.now();
const events = [];
function record(what) {
  events.push({ t: Date.now() - T0, what: what });
}

// u1 is FAST end to end; u2's build alone outlasts u1's whole build-then-review pair. If the
// driver still awaited the wave as one batch, u1's review could not even start until u2's slow
// build resolved at +300ms - the exact defect this scenario is built to catch.
const WORKERS = {
  "u1/implementer#1": 20,
  "u1/lens:sdet#1": 20,
  "u2/implementer#1": 300,
  "u2/lens:sdet#1": 20,
};
const callCounts = {};

// `step_result` (src/spawn.rs) returns the FULL PENDING FRONTIER on every call - every request
// with no recorded result yet, not merely what that one call newly parked (its own doc comment,
// and this unit's commit message says so explicitly). So `u2/implementer#1` reappears, verbatim,
// on steps 2 and 3 below, exactly as the real conductor would report it while it is still
// running - which is what makes the driver's in-flight filter load-bearing rather than a no-op.
const STEPS = [
  {
    wave: [
      { id: "u1/implementer#1", unit: "u1", stage: "build" },
      { id: "u2/implementer#1", unit: "u2", stage: "build" },
    ],
    done: false,
  },
  {
    wave: [
      { id: "u2/implementer#1", unit: "u2", stage: "build" },
      { id: "u1/lens:sdet#1", unit: "u1", stage: "review" },
    ],
    done: false,
  },
  {
    wave: [{ id: "u2/implementer#1", unit: "u2", stage: "build" }],
    done: false,
  },
  {
    wave: [{ id: "u2/lens:sdet#1", unit: "u2", stage: "review" }],
    done: false,
  },
  { wave: [], done: true },
];
let stepIdx = 0;
let parallelCalls = 0;

async function agent(prompt, opts) {
  const label = (opts && opts.label) || "";
  if (label === "resolve-repo") {
    record("agent:resolve-repo");
    return { path: "/repo" };
  }
  if (label.indexOf("step#") === 0) {
    if (stepIdx >= STEPS.length) {
      throw new Error(
        "test harness: the courier was invoked more times than the scripted sequence covers (call " +
          (stepIdx + 1) +
          ")"
      );
    }
    const resp = STEPS[stepIdx];
    stepIdx += 1;
    record("step-return:" + stepIdx);
    return resp;
  }
  const m = /^You are the rigger worker for spawn (\S+) \(unit /.exec(prompt);
  if (m) {
    const id = m[1];
    const delay = WORKERS[id];
    if (delay === undefined) {
      throw new Error("test harness: no worker delay scripted for " + id);
    }
    callCounts[id] = (callCounts[id] || 0) + 1;
    if (callCounts[id] > 1) {
      throw new Error(
        "REGRESSION (spec 89 criterion 5): worker " +
          id +
          " was spawned a SECOND time while its first run was still in flight - the in-flight guard did not hold"
      );
    }
    record("worker-start:" + id);
    await new Promise(function (resolve) {
      setTimeout(resolve, delay);
    });
    record("worker-resolve:" + id);
    return {};
  }
  throw new Error(
    "test harness: unexpected agent() call - opts=" + JSON.stringify(opts) + " prompt=" + prompt.slice(0, 160)
  );
}

async function parallel(_fns) {
  parallelCalls += 1;
  throw new Error(
    "REGRESSION (spec 89 criterion 5): the driver awaited the wave as one parallel() batch again - pipelining was lost"
  );
}

function log(msg) {
  record("log:" + String(msg));
}

const sandbox = {
  args: { repo: "/repo", spec: "spec.md", outer_wall_clock: 5 },
  agent: agent,
  parallel: parallel,
  log: log,
  setTimeout: setTimeout,
  clearTimeout: clearTimeout,
};
vm.createContext(sandbox);

function dumpEvents() {
  let out = "";
  for (let i = 0; i < events.length; i++) {
    out += "  +" + events[i].t + "ms " + events[i].what + "\n";
  }
  return out;
}

function fail(msg) {
  console.error(msg);
  console.error("driver events (chronological):");
  console.error(dumpEvents());
  process.exit(1);
}

const src = "(async () => {\n" + driverBody + "\n})()";

let resultPromise;
try {
  resultPromise = vm.runInContext(src, sandbox, { filename: "rigger-driver-pipelining-harness.js" });
} catch (e) {
  fail("the driver body threw SYNCHRONOUSLY before returning its promise: " + String((e && e.stack) || e));
  throw e;
}

resultPromise
  .then(function (result) {
    // The run reaches the same fixpoint: the driver resolved (never `stop()`'d loudly) and
    // walked the full scripted step sequence - an early stop or an extra, unscripted courier
    // call would already have thrown above, inside agent().
    if (stepIdx !== STEPS.length) {
      fail("expected exactly " + STEPS.length + " courier calls, the driver made " + stepIdx);
    }
    if (!result || result.waves !== 3) {
      fail(
        "expected the driver to report 3 spawn-batches (both units' builds together, then " +
          "each unit's own review spawned separately as it becomes ready), got: " + JSON.stringify(result)
      );
    }

    // No running item was ever spawned twice: already enforced inside agent() (a second call
    // for the same id throws, which would have rejected this promise instead of reaching here).
    // Re-assert the call counts explicitly so a change to that guard's throw site cannot
    // silently stop enforcing it.
    for (const id of Object.keys(WORKERS)) {
      if (callCounts[id] !== 1) {
        fail("worker " + id + " must run exactly once; ran " + (callCounts[id] || 0) + " time(s)");
      }
    }

    // The pre-pipelining monolithic wave-await is gone at RUNTIME, not merely absent from the
    // source text: parallel() must never be invoked.
    if (parallelCalls !== 0) {
      fail("parallel() must never be invoked by the pipelined driver; it was called " + parallelCalls + " time(s)");
    }

    // THE criterion itself: "the unit whose result lands first is reviewed while the other
    // still builds". u1's build (20ms) lands well before u2's build (300ms); u1's review worker
    // must have STARTED before u2's build RESOLVED - proving u1 was already in review while u2
    // was still building, not queued behind it.
    const startU1Review = events.find(function (e) {
      return e.what === "worker-start:u1/lens:sdet#1";
    });
    const resolveU2Build = events.find(function (e) {
      return e.what === "worker-resolve:u2/implementer#1";
    });
    if (!startU1Review || !resolveU2Build) {
      fail("expected both worker-start:u1/lens:sdet#1 and worker-resolve:u2/implementer#1 in the event log");
    }
    if (!(startU1Review.t < resolveU2Build.t)) {
      fail(
        "u1's review must start WHILE u2's build is still running (u1 review started @" +
          startU1Review.t +
          "ms, which must precede u2's build resolving @" +
          resolveU2Build.t +
          "ms) - the driver waited for the whole wave instead of pipelining"
      );
    }

    console.log("OK per-unit-pipelining-reviews-the-fast-unit-while-the-slow-one-still-builds");
  })
  .catch(function (err) {
    fail("the driver body rejected or a scripted assertion failed: " + String((err && err.stack) || err));
  });
"#;

/// Spawn `node` on the harness against the real `workflows/rigger.js` driver body, bounded by an
/// explicit deadline enforced through the spawned `Child` handle (`kill()` + `wait()`) - never a
/// shelled-out OS-level signal, and never a signal to any pid this test did not itself spawn -
/// so a defect that leaves the harness's own promise chain unsettled (a dropped `.then`, a
/// forgotten `setTimeout` clear) fails the test loudly instead of hanging the suite.
fn run_driver_harness(harness: &str) -> (bool, String, String) {
    let driver_body = rigger_js_driver_body();
    let dir = tempfile::tempdir().expect("a scratch dir for the runtime harness");
    let harness_path = dir.path().join("harness.js");
    let driver_path = dir.path().join("driver-body.js");
    std::fs::write(&harness_path, harness).expect("write the runtime harness");
    std::fs::write(&driver_path, driver_body).expect("write the driver body");

    let mut child = Command::new("node")
        .arg(&harness_path)
        .arg(&driver_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn node to drive the driver behavior harness");

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll the harness child") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "the driver behavior harness did not finish within 20s (scripted worker \
                 delays total well under 2s) - it likely hung on an unsettled promise; killed \
                 via the spawned Child handle"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    use std::io::Read;
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .expect("harness stdout must be piped")
        .read_to_string(&mut stdout)
        .expect("read harness stdout");
    child
        .stderr
        .take()
        .expect("harness stderr must be piped")
        .read_to_string(&mut stderr)
        .expect("read harness stderr");

    (status.success(), stdout, stderr)
}

/// Run `harness` against the real driver body and assert it exited cleanly and printed
/// `ok_token`, failing with `what` and both output streams otherwise. A missing `node` runtime
/// (present on dev machines and on ubuntu-latest CI) skips `test` with a notice rather than
/// failing it - an environment fact, never a test failure.
fn assert_driver_harness_holds(test: &str, harness: &str, ok_token: &str, what: &str) {
    if !tool_available("node", "--version") {
        eprintln!("SKIP {test}: no `node` runtime on PATH; install node to run it.");
        return;
    }
    let (ok, stdout, stderr) = run_driver_harness(harness);
    assert!(
        ok && stdout.contains(ok_token),
        "{what}:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// RUNTIME guard (spec 89, criterion 5): with two units landing in one wave, the driver reviews
/// the unit whose result lands first while its sibling is still building, never spawns a
/// still-running item a second time, never falls back to a monolithic `parallel()` wave-await,
/// and still resolves at the run's fixpoint. Dropping the in-flight filter, re-introducing the
/// per-wave `parallel()` await, or resolving the fixpoint on `step.done` alone (ignoring a
/// straggler still in `inFlight`) each reddens this by actually running the driver, not by
/// pattern-matching its source text.
#[test]
fn fast_units_review_runs_while_the_slow_sibling_still_builds() {
    assert_driver_harness_holds(
        "fast_units_review_runs_while_the_slow_sibling_still_builds",
        HARNESS,
        "OK per-unit-pipelining-reviews-the-fast-unit-while-the-slow-one-still-builds",
        "the pipelining behavior harness must drive the real driver body to the expected \
         fixpoint and confirm the criterion held",
    );
}

/// Node harness for a worker settling WHILE the `rigger step` courier is still running. Each
/// scenario runs the real driver body in a fresh `vm` context with a courier that takes 100ms per
/// step, so a worker scripted to finish inside that window settles during the step - before the
/// loop builds its wait over `inFlight`.
///
/// WAKE-UP: step 1 parks `a` (20ms), `c` (60ms) and `b` (1000ms). `a` settling wakes step 2; `c`
/// settles during step 2, whose wave holds only `b`. The loop must step again at once to fold
/// `c`'s result - step 3 must start long before `b` resolves - never wait on `b` alone.
///
/// NO FALSE STOP: step 1 parks `a` (20ms) and `c` (60ms). `a` settling wakes step 2; `c` - the
/// last worker - settles during step 2, which returns an empty, not-done wave. The loop must step
/// again (step 3 reports `done`) instead of stopping as "nothing is in flight, yet is not done".
const SETTLE_DURING_STEP_HARNESS: &str = r#"
"use strict";
const vm = require("vm");
const fs = require("fs");

const driverBody = fs.readFileSync(process.argv[2], "utf8");
const COURIER_MS = 100;

function sleep(ms) {
  return new Promise(function (resolve) { setTimeout(resolve, ms); });
}

async function runScenario(name, workers, steps) {
  const T0 = Date.now();
  const events = [];
  const record = function (what) { events.push({ t: Date.now() - T0, what: what }); };
  let stepIdx = 0;
  async function agent(prompt, opts) {
    const label = (opts && opts.label) || "";
    if (label === "resolve-repo") return { path: "/repo" };
    if (label.indexOf("step#") === 0) {
      if (stepIdx >= steps.length) {
        throw new Error(name + ": the courier was invoked more times than scripted (call " + (stepIdx + 1) + ")");
      }
      const resp = steps[stepIdx];
      stepIdx += 1;
      record("step-start:" + stepIdx);
      await sleep(COURIER_MS);
      record("step-return:" + stepIdx);
      return resp;
    }
    const m = /^You are the rigger worker for spawn (\S+) \(unit /.exec(prompt);
    if (m && workers[m[1]] !== undefined) {
      record("worker-start:" + m[1]);
      await sleep(workers[m[1]]);
      record("worker-resolve:" + m[1]);
      return {};
    }
    throw new Error(name + ": unexpected agent() call - opts=" + JSON.stringify(opts) + " prompt=" + prompt.slice(0, 160));
  }
  const sandbox = {
    args: { repo: "/repo", spec: "spec.md", outer_wall_clock: 5 },
    agent: agent,
    parallel: async function () { throw new Error(name + ": parallel() must never be invoked"); },
    log: function (msg) { record("log:" + String(msg)); },
    setTimeout: setTimeout,
    clearTimeout: clearTimeout,
  };
  vm.createContext(sandbox);
  const src = "(async () => {\n" + driverBody + "\n})()";
  let error = null;
  try {
    await vm.runInContext(src, sandbox, { filename: "rigger-driver-" + name + "-harness.js" });
  } catch (e) {
    error = e;
  }
  return { events: events, stepIdx: stepIdx, error: error };
}

function fail(name, msg, run) {
  console.error(name + ": " + msg);
  if (run && run.error) console.error("driver error: " + String((run.error && run.error.stack) || run.error));
  if (run) {
    console.error("driver events (chronological):");
    for (const e of run.events) console.error("  +" + e.t + "ms " + e.what);
  }
  process.exit(1);
}

function at(run, what) {
  const e = run.events.find(function (x) { return x.what === what; });
  return e ? e.t : undefined;
}

(async function () {
  const wake = await runScenario(
    "wake-up",
    { "a/implementer#1": 20, "c/implementer#1": 60, "b/implementer#1": 1000 },
    [
      {
        wave: [
          { id: "a/implementer#1", unit: "a", stage: "build" },
          { id: "c/implementer#1", unit: "c", stage: "build" },
          { id: "b/implementer#1", unit: "b", stage: "build" },
        ],
        done: false,
      },
      { wave: [{ id: "b/implementer#1", unit: "b", stage: "build" }], done: false },
      { wave: [{ id: "b/implementer#1", unit: "b", stage: "build" }], done: false },
      { wave: [], done: true },
    ]
  );
  if (wake.error) fail("wake-up", "the driver stopped instead of reaching its fixpoint", wake);
  const step3 = at(wake, "step-start:3");
  const bDone = at(wake, "worker-resolve:b/implementer#1");
  if (step3 === undefined || bDone === undefined) fail("wake-up", "expected step 3 and b's resolve in the event log", wake);
  if (!(step3 < bDone)) {
    fail(
      "wake-up",
      "a worker that settled during step 2 must wake step 3 at once (step 3 started @" + step3 +
        "ms, after b resolved @" + bDone + "ms) - the loop waited on another worker instead",
      wake
    );
  }

  const last = await runScenario(
    "no-false-stop",
    { "a/implementer#1": 20, "c/implementer#1": 60 },
    [
      {
        wave: [
          { id: "a/implementer#1", unit: "a", stage: "build" },
          { id: "c/implementer#1", unit: "c", stage: "build" },
        ],
        done: false,
      },
      { wave: [], done: false },
      { wave: [], done: true },
    ]
  );
  if (last.error) {
    fail("no-false-stop", "the last worker settling during a step with an empty wave must not stop the run", last);
  }
  if (last.stepIdx !== 3) fail("no-false-stop", "expected 3 courier calls, the driver made " + last.stepIdx, last);

  console.log("OK a-settle-during-the-courier-step-steps-again-at-once");
})().catch(function (err) {
  fail("harness", String((err && err.stack) || err), null);
});
"#;

/// RUNTIME guard: a worker that settles while the `rigger step` courier is still running wakes
/// the loop - the next step runs at once instead of waiting on a different worker - and the last
/// worker settling during a step whose wave is empty never reads as the "nothing is in flight,
/// yet is not done" stop.
#[test]
fn a_worker_settling_during_the_courier_step_wakes_the_loop_and_never_reads_as_a_false_stop() {
    assert_driver_harness_holds(
        "a_worker_settling_during_the_courier_step_wakes_the_loop_and_never_reads_as_a_false_stop",
        SETTLE_DURING_STEP_HARNESS,
        "OK a-settle-during-the-courier-step-steps-again-at-once",
        "a settle during the courier step must step again at once and never stop the run",
    );
}

/// Node harness for what the driver tells each WORKER: one wave parks a BOUNDED spawn, an
/// UNBOUNDED spawn and a spawn whose step resolved no marker path, and the harness records the
/// prompt the real driver body hands each worker, then passes them to `check(prompts)` - a
/// function declaration each test appends (see [`worker_prompts_harness`]) that asserts on them
/// and prints its ok token.
const WORKER_PROMPTS_HARNESS: &str = r#"
"use strict";
const vm = require("vm");
const fs = require("fs");

const driverBody = fs.readFileSync(process.argv[2], "utf8");

const BOUNDED_MARKER = "/scratch/agent-live/r1/b_2fimplementer_231";
const UNBOUNDED_MARKER = "/scratch/agent-live/r1/u_2fimplementer_231";
const WAVE = [
  { id: "b/implementer#1", unit: "b", stage: "build", max_wall_clock: 60, marker_path: BOUNDED_MARKER },
  { id: "u/implementer#1", unit: "u", stage: "build", max_wall_clock: null, marker_path: UNBOUNDED_MARKER },
  { id: "n/implementer#1", unit: "n", stage: "build", max_wall_clock: null, marker_path: null },
];
const prompts = {};
let steps = 0;

function fail(msg) {
  console.error(msg);
  process.exit(1);
}

async function agent(prompt, opts) {
  const label = (opts && opts.label) || "";
  if (label === "resolve-repo") return { path: "/repo" };
  if (label.indexOf("step#") === 0) {
    steps += 1;
    if (steps > 10) throw new Error("the driver kept stepping past its fixpoint");
    return steps === 1 ? { wave: WAVE, done: false } : { wave: [], done: true };
  }
  const m = /^You are the rigger worker for spawn (\S+) \(unit /.exec(prompt);
  if (m) {
    prompts[m[1]] = prompt;
    await new Promise(function (resolve) { setTimeout(resolve, 20); });
    return {};
  }
  throw new Error("unexpected agent() call - opts=" + JSON.stringify(opts) + " prompt=" + prompt.slice(0, 160));
}

const sandbox = {
  args: { repo: "/repo", spec: "spec.md", outer_wall_clock: 5 },
  agent: agent,
  parallel: async function () { throw new Error("parallel() must never be invoked"); },
  log: function () {},
  setTimeout: setTimeout,
  clearTimeout: clearTimeout,
};
vm.createContext(sandbox);

vm.runInContext("(async () => {\n" + driverBody + "\n})()", sandbox, { filename: "rigger-driver-worker-prompts-harness.js" })
  .then(function () {
    for (const item of WAVE) {
      if (!prompts[item.id]) fail("every wave item must reach a worker; got prompts for " + Object.keys(prompts));
    }
    check(prompts);
  })
  .catch(function (err) {
    fail("the driver body rejected: " + String((err && err.stack) || err));
  });
"#;

/// The [`WORKER_PROMPTS_HARNESS`] completed by `check`, a JS `function check(prompts)`
/// declaration that asserts on the prompt each worker received and prints its ok token.
fn worker_prompts_harness(check: &str) -> String {
    format!("{WORKER_PROMPTS_HARNESS}\n{check}")
}

/// The LIVENESS HEARTBEAT check (spec 101: every spawn carries a liveness marker, so the
/// live-writer guard sees every worker): the bounded spawn keeps its bounded heartbeat, the
/// unbounded one is told to keep its marker fresh anyway (never hung, read as live until its
/// result), and the unmarked one is told no heartbeat at all.
const HEARTBEAT_CHECK: &str = r#"
function check(prompts) {
    const touch = function (marker) { return 'mkdir -p "$(dirname "' + marker + '")" && touch "' + marker + '"'; };
    const bounded = prompts["b/implementer#1"];
    const unbounded = prompts["u/implementer#1"];
    const unmarked = prompts["n/implementer#1"];
    if (!bounded.includes("LIVENESS HEARTBEAT (spec 10): your spawn carries a 60s wall-clock bound.") || !bounded.includes(touch(BOUNDED_MARKER))) {
      fail("the BOUNDED spawn keeps its bounded heartbeat over its own marker:\n" + bounded);
    }
    if (!unbounded.includes("LIVENESS HEARTBEAT") || !unbounded.includes(touch(UNBOUNDED_MARKER))) {
      fail("the UNBOUNDED spawn must be told to touch its own marker too:\n" + unbounded);
    }
    if (!unbounded.includes("carries no wall-clock bound") || !unbounded.includes("never declares an unbounded spawn hung") || !unbounded.includes("live-writer guard") || !unbounded.includes("until your result is recorded")) {
      fail("the UNBOUNDED spawn's heartbeat must say it has no bound, is never declared hung, and reads as a live worker to the live-writer guard until its result is recorded:\n" + unbounded);
    }
    if (unbounded.includes("wall-clock bound. Prove you are alive")) {
      fail("the UNBOUNDED spawn must not be handed the bounded wording:\n" + unbounded);
    }
    if (unmarked.includes("LIVENESS HEARTBEAT")) {
      fail("a spawn with no resolved marker path has nothing to touch and gets no heartbeat:\n" + unmarked);
    }
    console.log("OK every-marked-spawn-is-told-to-heartbeat");
}
"#;

/// RUNTIME guard (spec 101): the driver hands every spawn that carries a marker path the
/// heartbeat instruction - a bounded one its bounded wording, an unbounded one the no-bound
/// wording that still keeps its marker fresh for the live-writer guard - and a spawn with no
/// marker path none.
#[test]
fn every_spawn_with_a_marker_path_is_told_to_keep_it_fresh() {
    assert_driver_harness_holds(
        "every_spawn_with_a_marker_path_is_told_to_keep_it_fresh",
        &worker_prompts_harness(HEARTBEAT_CHECK),
        "OK every-marked-spawn-is-told-to-heartbeat",
        "every spawn carrying a marker path must be told to keep it fresh",
    );
}

/// The ENDED-SPAWN check (gap 108): a driver resume replays its cached courier steps and
/// re-spawns their waves' workers, and `rigger prompt` refuses a spawn that already ended - so
/// every worker is told, right after its prompt fetch and before any driver instruction, to stop
/// there on that refusal, doing nothing and recording nothing.
const ENDED_SPAWN_CHECK: &str = r#"
function check(prompts) {
  for (const id of Object.keys(prompts)) {
    const prompt = prompts[id];
    const fetch = prompt.indexOf("rigger prompt '" + id + "'");
    const stop = prompt.indexOf("If that command instead refuses because spawn " + id + " already ended");
    const instructions = prompt.indexOf("--- rigger driver instructions ---");
    if (stop < 0 || !prompt.includes("do nothing and record nothing")) {
      fail("worker " + id + " must be told to stop, recording nothing, when its spawn already ended:\n" + prompt);
    }
    if (!(fetch < stop && stop < instructions)) {
      fail("worker " + id + "'s stop clause must follow its prompt fetch and precede every driver instruction:\n" + prompt);
    }
  }
  console.log("OK every-worker-stops-on-an-ended-spawn");
}
"#;

/// RUNTIME guard (gap 108): every worker the driver spawns is told to stop - no edits, no emits,
/// no result - when `rigger prompt` refuses because its spawn already ended.
#[test]
fn the_worker_preamble_stops_on_an_ended_spawn() {
    assert_driver_harness_holds(
        "the_worker_preamble_stops_on_an_ended_spawn",
        &worker_prompts_harness(ENDED_SPAWN_CHECK),
        "OK every-worker-stops-on-an-ended-spawn",
        "every worker must be told to stop on an ended spawn before any driver instruction",
    );
}
