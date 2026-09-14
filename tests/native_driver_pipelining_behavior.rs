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

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub
/// `ubuntu-latest`, which ships Node.js on PATH, so this runtime guard runs in CI) - mirrors the
/// identical guard the sibling `dash`/`code_lens` node-`vm` periphery tests use.
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The driver BODY: `workflows/rigger.js`, read fresh at test time (never copied inline, so an
/// edit to the shipped driver is exercised without touching this file), sliced from the `args`
/// resolution onward, through the top-level `for (;;)` loop and its trailing `return { waves }`,
/// with the leading `export const meta = {...}` ES-module export cut off. `export` is invalid
/// inside a plain script/`vm` context, and `meta` (the workflow's display metadata) carries none
/// of the loop's own logic, so slicing it off loses nothing this test needs.
fn rigger_js_driver_body() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join("rigger.js");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
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
fn run_pipelining_harness() -> (bool, String, String) {
    let driver_body = rigger_js_driver_body();
    let dir = tempfile::tempdir().expect("a scratch dir for the runtime harness");
    let harness_path = dir.path().join("harness.js");
    let driver_path = dir.path().join("driver-body.js");
    std::fs::write(&harness_path, HARNESS).expect("write the runtime harness");
    std::fs::write(&driver_path, driver_body).expect("write the driver body");

    let mut child = Command::new("node")
        .arg(&harness_path)
        .arg(&driver_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn node to drive the pipelining behavior harness");

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll the harness child") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "the pipelining behavior harness did not finish within 20s (scripted worker \
                 delays total well under 1s) - it likely hung on an unsettled promise; killed \
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

/// RUNTIME guard (spec 89, criterion 5): with two units landing in one wave, the driver reviews
/// the unit whose result lands first while its sibling is still building, never spawns a
/// still-running item a second time, never falls back to a monolithic `parallel()` wave-await,
/// and still resolves at the run's fixpoint. Dropping the in-flight filter, re-introducing the
/// per-wave `parallel()` await, or resolving the fixpoint on `step.done` alone (ignoring a
/// straggler still in `inFlight`) each reddens this by actually running the driver, not by
/// pattern-matching its source text.
#[test]
fn fast_units_review_runs_while_the_slow_sibling_still_builds() {
    if !node_available() {
        eprintln!(
            "SKIP fast_units_review_runs_while_the_slow_sibling_still_builds: no `node` runtime \
             on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }
    let (ok, stdout, stderr) = run_pipelining_harness();
    assert!(
        ok,
        "the pipelining behavior harness must drive the real driver body to the expected \
         fixpoint:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK per-unit-pipelining-reviews-the-fast-unit-while-the-slow-one-still-builds"),
        "the harness must confirm the criterion held:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
