#!/bin/sh
# The check-in mutation gate (spec 91): the diff-scoped `cargo mutants` sweep, run ONCE by a
# `checkin` stage that lists it - after every implement unit has integrated - never per
# implementer round. Declared as `mutation: { run: "sh .rigger/gates/mutation.sh" }` in
# .rigger/workflow.yml, where no stage runs it until issue #32 lands; the diff-base logic below
# is what a re-wired `checkin` stage will use. `rigger init` writes this same file into a
# consumer project (one home: src/cli/setup.rs includes it), whose scaffold `checkin` stage
# lists a `mutation` gate.
#
# THE GATE ENVIRONMENT. `$MUTANTS` is the unit-keyed mutants root the conductor exports to
# every gate command (`worktree::unit_mutants_sibling`) and reaps at unit terminus; this script
# owns creating and wiping it each run. `$RIGGER_RUN_BASE` is the run branch's tip AT THE
# MOMENT the run started (`RunStarted.base_tip`) - never a merge base with the run branch: the
# checkin worktree branches off the run branch after every unit integrated, so a merge base
# there is already HEAD and would diff nothing. A run with no base tip, or a base tip this
# repository does not hold, refuses loud before anything runs.
#
# THE VERDICT. The gate fails on MISSED mutants only. A TIMEOUT is a detection (cargo-mutants
# exits 3): hang-class mutants are caught only by bounded waits (on spec 89's check-in the same
# four timed out at 1.5x and 3x the baseline with zero misses). Each mutant's test run is
# bounded at an absolute 300 s, never a multiple of the baseline: cargo-mutants tests only the
# mutated packages in its baseline, while each mutant also runs the root package's suite, so a
# multiple of the baseline says nothing about it (measured 2026-09-28: a rigger-domain-only
# baseline took 0 s, and the auto-set 20 s bound then timed out both mutants of the smoke run
# before their root-suite test run could report the catch). nextest ends any single hung test at 240 s (.config/nextest.toml).
# A phase ENDED BY A SIGNAL (gap 100) is judged from the mutant's log, because cargo-mutants
# 27.1 exits 0 over it (measured 2026-09-28 in a 1 GiB scope, a real reaper each time: a test
# phase ended on signal 9 and a build phase ended on signal 9 both filed as "Failure" - the
# unclassified outcome no .txt list carries - and a signal-ended rustc under the build filed
# as "Unviable"; the sweep still exited 0). A TEST phase ended by a signal is a detection in
# the sense a timeout is: the mutant made its tests grow until the reaper ended them (a
# single test child ended the same way already fails nextest and reads as caught). A BUILD
# phase ended by a signal - its own process, or a compiler under it - means the mutant was
# never tested: the gate fails as an environment failure naming each such mutant and phase,
# before the anchor state is promoted, so the next sweep examines them again.
# The checkin stage's task text carries the survivor-closing protocol: read
# mutants.out/outcomes.json, close each missed mutant with a test that fails on it or a
# rewrite that removes the mutable site, commit, and record the accounting as a DecisionMade.
#
# SWEEP SHAPE (2026-09-16, 45 s per mutant and 5.4 h per pass measured on spec 92's 433-mutant
# check-in). The tests run under nextest (.config/nextest.toml): every binary at once, stop at
# the first failure, a hung test dies at the per-test bound. `env -u CARGO_TARGET_DIR` gives
# each mutant copy its own build dir (one shared target serialized every copy on cargo's lock
# and left the cache holding mutated artifacts); `--cargo-arg=--target-dir=target` pins it
# INSIDE the copy, because the scratch root's own .cargo/config.toml would otherwise send all
# copies to one `cargo-target-shared` dir. `CARGO_BUILD_JOBS=8` lifts .cargo/config.toml's
# 4-job cap for the sweep alone. Line-table debug info (`CARGO_PROFILE_*_DEBUG=1`) keeps each
# relink small: full debug info made 200-290 MB test binaries and 81 s relinks.
#
# THE WHOLE WORKSPACE IS MUTATED, ONLY THE TOUCHED PACKAGES ARE TESTED (2026-09-28). The
# root manifest is a package AND a workspace, so a bare `cargo mutants` generates mutants in
# the root package alone: after the crate split that silently skipped every diff under
# crates/ (measured: a diff touching three crates listed only its src/ mutants). `--workspace`
# makes `--in-diff` see every package. Each mutant then runs the tests of the packages whose
# files the swept diff touches, PLUS the root package - its tests/ integration suite drives
# every crate through the binary - never `--test-workspace`: a mutant in one crate gains
# nothing from the other crates' unit tests, and running every package's tests per mutant
# multiplies the per-mutant cost the check-in budget is made of. A survivor this exposes is
# closed by a test in the right crate, never by widening the scope back. The package of a file
# is the nearest enclosing Cargo.toml that declares a [package], so the rule holds for any
# workspace layout.
#
# THE SWEEP IS BOUNDED AS A WHOLE (gap 92, 2026-09-24: a loop mutant made eight concurrent
# test children allocate ~5.5 G each within 15 s; the global OOM killer took the check-in
# step itself, and its scope's failure ended the operator's session). A per-process cap sized
# for one runaway does nothing for N concurrent ones, so the sweep runs in its own transient
# systemd scope, `MemoryMax` = half of `MemAvailable` at launch - the other half stays with
# the step that launched the gate and with the desktop. Past the bound the kernel reclaims or
# ends processes INSIDE the scope only; rigger itself signals nothing. Without a systemd user
# manager (a CI container) the sweep runs unbounded and the gate says so.
#
# THE REAPER ENDS A MUTANT, NEVER THE SWEEP (gap 100, 2026-09-28: a 208-mutant sweep at -j 3
# in a 22 GB scope reached its bound seven minutes in; the reaper ended one process, and
# systemd's default OOMPolicy=stop then stopped the whole scope - cargo-mutants took signal
# 15 and the gate failed after 23 of 208 mutants). The scope carries `OOMPolicy=continue`: a
# process the reaper ends is that mutant's outcome (see THE VERDICT) and the sweep goes on.
#
# THE SWEEP'S MEMORY SCALES WITH ITS TOTAL IN-FLIGHT TEST PROCESSES, not with its copy count
# (measured 2026-09-28 on this 32-core box, root package: 2325 tests, each shape alone in a
# transient scope, anonymous memory sampled from the scope's memory.stat every 0.5 s). A
# healthy copy is small: its cold 8-job test build peaked at 1.3 GiB anonymous (memory.peak
# 19.7 GiB, all but that the page cache of the artifacts it writes, which the kernel reclaims
# at the bound rather than ending anything); its nextest run peaked at 3.0 GiB anonymous at 32
# test threads (memory.peak 3.2 GiB, 43 s) and 1.5 GiB at 10 threads (memory.peak 2.3 GiB,
# 95 s). A 2-copy sweep in a 30 GiB service showed MemoryPeak = its 30 GiB bound, reached by
# page cache alone: 5547 reclaims at the bound, zero processes ended. What reaches the bound
# is a RUNAWAY mutant: every in-flight test process of its copy runs the same mutated code
# and may grow to the runner's 4 GiB cap (.cargo/pidns-runner.sh), so the exposure is the
# number of test processes in flight across all copies - nextest's default one per core in
# EACH copy made three copies 96 of them. So the total is held at the core count: each copy
# runs cores / jobs nextest test threads (`-- --test-threads`, which cargo-mutants hands to
# the test phase only - a real sweep's argv showed it absent from the `--no-run` build), and
# the jobs come from the bound at 5 GiB per job, at least 1 and at most 3 (three copies
# already saturate the cores at 8 build jobs each). 5 GiB is the measured healthy need
# rounded up: a build (1.3 GiB) per copy plus one core-wide test fan-out (3.0 GiB) shared by
# all copies, so jobs x 1.3 + 3.0 stays under jobs x 5 GiB for every jobs >= 1 - one figure,
# no second constant, whatever -j the bound gives. A bound too tight for the suite shows as a
# failing baseline run below, never as a quietly "caught" mutant.
#
# THE CONTAINER-BACKED TESTS RUN (2026-09-29). A test that needs a container runtime skips and
# passes where it reaches none, so every mutant only it can catch - each one in the KurrentDB
# adapter - would read as missed. The gate sources `container-env.sh` beside it, which points
# testcontainers at the operator's rootless podman when no DOCKER_HOST is set. A project
# without that file sweeps exactly as before.
#
# THE BASELINE STAYS ON. A checkin stage that lists `test` before `mutation` (the scaffold's
# does) proves nothing to this gate: the conductor runs every listed gate whatever the earlier
# ones returned and exports no record of their verdicts to a gate command - so nothing here can
# confirm the mutated packages are green on this tree. The baseline (cargo-mutants runs it over
# the mutated packages only: seconds, not minutes) is that confirmation; only the by-name
# rerun, which follows it on the same tree, skips its own.
#
# INCREMENTAL RE-SWEEPS (Byran, 2026-09-16: "only run mutations when the test has changed or
# the logic has changed"). The sweep leaves four facts under `mutation-anchor/`, a sibling of
# the unit roots under the scratch root (`${MUTANTS%/*}`): the `$RIGGER_RUN_BASE` it was given
# (`base`), the tree it examined (`tip`), its misses (`missed.txt`) and, per caught mutant, the
# test binary whose first failure caught it (`caught.map`, read off the nextest FAIL line in
# each mutant's log). The anchor lives OUTSIDE `$MUTANTS` because that root is reclaimed with
# the unit (2026-09-19: an escalation reclaimed five hours of sweep state seconds after the
# gate wrote it). Misses are the run's own, and the recorded base alone says whose: when it is
# this sweep's own `$RIGGER_RUN_BASE` - an earlier sweep of this run, so of this spec - the
# sweep re-runs every earlier miss by name. Narrowing needs one more fact, HEAD holding the tip:
# then the sweep covers every mutant in the diff since the tip. An anchor of this run whose tip
# HEAD no longer holds (a rewritten attempt, or a tip pruned from the repository) narrows
# nothing and its misses are still re-run: a miss outside the spec diff would otherwise leave
# all gate state, and the same tree would fail and then pass. Any other anchor narrows nothing
# and re-runs none of its misses, and the sweep is the whole spec diff against
# `$RIGGER_RUN_BASE`: none at all (the first sweep, a reclaimed scratch root), one that records
# another run's base (a previous spec's), and one that records no base. Ownership is the
# recorded base, never ancestry: the anchor lives under the
# project's scratch root, so HEAD routinely holds a previous spec's last sweep tip - behind the
# run base on a run branch not rewritten between specs, or past it when that spec's escalated
# check-in is landed by hand during this run - and taking it would sweep only the changes since
# it and re-run that spec's misses, failing this spec on survivors that are not its own.
# Catches are the project's knowledge, whichever run recorded them: the sweep also re-runs by
# name every caught mutant in the map whose catching binary's `tests/<binary>.rs` changed since
# one point - the owned tip, else `$RIGGER_RUN_BASE` - so a spec that rewrites the test catching
# an earlier spec's mutant examines that mutant again on its first sweep (a change under a
# nested tests/ directory re-runs every mutant a tests/*.rs binary caught; the crate's own
# unit-test binaries cannot see tests/ and keep their catches). That same point is the base of
# the diff the sweep covers. The rerun's misses join the sweep's own missed.txt so one file is
# the verdict. A solo-merging unit's post-merge re-sweep is therefore the empty merge delta and
# passes in seconds. A mutant the main sweep already examined is not examined again by name.
#
# THE GATE OWNS ITS INSTRUMENT (2026-09-17: a remediation round excluded two survivors by name
# with an equivalence argument that was wrong for one). The unit diff since `$RIGGER_RUN_BASE`
# may add no exclusion or examine key to .cargo/mutants.toml and no `mutants::skip` attribute;
# a diff that does fails the gate before any sweep, naming the lines. A mutant no test can
# distinguish is dead code to delete, never a name to exclude.
#
# THE STATE SURVIVES A FAILING SWEEP (2026-09-17). The new base, tree, misses and map are
# written to `last.new` and swapped in only once both passes completed; a sweep that dies
# earlier leaves the previous state in place. A by-name rerun whose names match no mutant is
# nothing to rerun, not a failure. The map carries forward every earlier entry for a mutant
# this sweep did not examine, whichever run recorded it, so a catch recorded three sweeps ago -
# by this spec or an earlier one - still re-runs when this spec changes its catching test.
#
# THE RERUN EXITS WITH CARGO-MUTANTS' OWN CODE (2026-09-19: under xargs a one-miss rerun exited
# 123 and broke the chain before the promotion). The names are loaded into the positional
# parameters instead.
#
# THE RERUN IS A DIFF, NOT A NAME FILTER. cargo-mutants 27.1 lets every "delete field" mutant
# through `-F`/`-E`, so the names are resolved first (`--list -F ...`, kept only where they
# really match), a one-line synthetic hunk is written per (file, line) found - the line's own
# text, which cargo-mutants checks against the source - and the rerun runs `--in-diff` on that,
# still intersected with the names.
#
# THE VERDICT NAMES EVERY SURVIVOR (2026-09-18/19: gate evidence keeps five lines, and a sweep
# with 82 misses reached the implementer as three). A failing sweep ends with the survivor
# count, the path of the full list in the worktree and in the anchor, and the first three
# survivors, each shaped `error[...]` so the evidence keeps them.

test -n "$RIGGER_RUN_BASE" || {
    echo "mutation gate: RIGGER_RUN_BASE is unset - this run recorded no base tip, so there is no spec diff to sweep; refusing rather than sweeping an empty diff"
    exit 1
}
git rev-parse -q --verify "$RIGGER_RUN_BASE^{commit}" > /dev/null || {
    echo "mutation gate: RIGGER_RUN_BASE $RIGGER_RUN_BASE names no commit in this repository, so there is no spec diff to sweep; refusing rather than sweeping any other diff"
    exit 1
}

# The container runtime for the container-backed tests (see THE CONTAINER-BACKED TESTS RUN): a
# missing snippet is skipped, one that fails to source fails the gate.
container_env="$(dirname "$0")/container-env.sh"
test ! -f "$container_env" || . "$container_env" || exit 1

last="${MUTANTS:-/nonexistent}"
last="${last%/*}/mutation-anchor"
anchor="$(cat "$last/tip" 2>/dev/null || true)"
# The recorded base alone owns the misses; HEAD holding the tip governs narrowing alone (see
# INCREMENTAL RE-SWEEPS).
owned="$(cat "$last/base" 2>/dev/null)"
test "$owned" = "$RIGGER_RUN_BASE" || owned=""
{ test -n "$owned" && test -n "$anchor" &&
    test "$(git rev-list --count HEAD.."$anchor" 2>/dev/null || echo 1)" = 0; } || anchor=""
since="${anchor:-$RIGGER_RUN_BASE}"

rerun="$({
    test -z "$owned" || cat "$last/missed.txt" 2>/dev/null
    changed="$(git diff --name-only "$since" -- tests | sed -E 's#^tests/([^/]+)\.rs$#\1#; s#^tests/.*/.*#ALL#' | sort -u)"
    test -z "$changed" || awk -F '\t' -v ch="$changed" 'BEGIN { n = split(ch, a, "\n"); for (i = 1; i <= n; i++) set[a[i]] = 1 } ($2 in set) || ("ALL" in set && $2 != "rigger" && $2 != "bin/rigger") { print $1 }' "$last/caught.map" 2>/dev/null
} | sort -u)"

git diff "$since" -- '*.rs' > unit.diff || exit 1

if { git diff "$RIGGER_RUN_BASE" -- .cargo/mutants.toml; git diff "$RIGGER_RUN_BASE" -- '*.rs'; } | grep -E '^\+.*(exclude_re|examine_re|exclude_globs|examine_globs|mutants::skip)'; then
    echo "mutation gate: the unit narrows the sweep itself (an exclusion in .cargo/mutants.toml or a mutants::skip in the code); the gate owns its instrument - remove it and pin the mutant with a test or delete the code it mutates"
    exit 1
fi

rm -rf "${MUTANTS:?}"/rerun "$MUTANTS"/rerun.args "$MUTANTS"/rerun.re "$MUTANTS"/rerun.all "$MUTANTS"/rerun.list "$MUTANTS"/rerun.todo "$MUTANTS"/rerun.diff "$MUTANTS"/last.new "$MUTANTS"/examined.txt "$MUTANTS"/cargo-mutants-* mutants.out mutants.out.old &&
    mkdir -p "$MUTANTS/last.new" mutants.out || exit 1

if test -n "$rerun"; then
    printf '%s\n' "$rerun" | sed -E 's/[][\\.*^$+?(){}|]/\\&/g; s/^([^:]+):[0-9]+:[0-9]+:/\1(:[0-9]+:[0-9]+)?:/; s/^/-F\n/' > "$MUTANTS/rerun.args" || exit 1
fi

# The memory bound, the job count and each copy's test threads, from MemAvailable and the
# core count at launch (see THE SWEEP IS BOUNDED AS A WHOLE and THE SWEEP'S MEMORY SCALES
# above). RIGGER_MEMINFO names another meminfo file for the gate's tests.
avail_kb="$(awk '/^MemAvailable:/ { print $2; exit }' "${RIGGER_MEMINFO:-/proc/meminfo}" 2>/dev/null)"
avail_kb="${avail_kb:-0}"
bound_kb=$((avail_kb / 2))
jobs=$((bound_kb / (5 * 1024 * 1024)))
test "$jobs" -ge 1 || jobs=1
test "$jobs" -le 3 || jobs=3
threads=$(($(nproc 2>/dev/null || echo 1) / jobs))
test "$threads" -ge 1 || threads=1
scope=""
if test "$bound_kb" -gt 0 && command -v systemd-run > /dev/null 2>&1 &&
    systemd-run --user --scope --quiet -- true > /dev/null 2>&1; then
    scope="systemd-run --user --scope --quiet -p OOMPolicy=continue -p MemoryMax=${bound_kb}K --"
    echo "mutation gate: the sweep runs in its own scope, MemoryMax=${bound_kb}K (half of MemAvailable ${avail_kb}K), OOMPolicy=continue, -j $jobs x $threads test threads"
else
    echo "mutation gate: advisory - no systemd user scope here (systemd-run is absent, has no user manager, or MemAvailable is unreadable), so the sweep runs WITHOUT its own memory bound; -j $jobs x $threads test threads"
fi

# The `--test-package` arguments for the diff file $1: the package of every file it touches,
# plus the root package (the nearest Cargo.toml with a [package] of `Cargo.toml` itself).
test_packages() {
    { sed -n 's#^+++ b/##p; s#^--- a/##p' "$1"; echo Cargo.toml; } | while IFS= read -r f; do
        d="$(dirname "$f")"
        while :; do
            if test -f "$d/Cargo.toml"; then
                pkg="$(awk -F '"' '/^\[/ { in_pkg = ($0 == "[package]"); next } in_pkg && $1 ~ /^name *= *$/ { print $2; exit }' "$d/Cargo.toml")"
                if test -n "$pkg"; then
                    echo "$pkg"
                    break
                fi
            fi
            test "$d" = "." && break
            d="$(dirname "$d")"
        done
    done | sort -u | sed 's/^/--test-package /'
}

# One cargo-mutants pass inside the scope; succeeds on exit 0 (all caught), 2 (misses, judged
# below from missed.txt) and 3 (timeouts, which are detections). A mutant phase ended by a
# signal leaves the exit code as it was (0 when nothing else happened), so it is judged from
# the logs below, never from this code.
sweep() {
    TMPDIR="$MUTANTS" CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=1 CARGO_PROFILE_TEST_DEBUG=1 $scope env -u CARGO_TARGET_DIR \
        cargo mutants --workspace --test-tool nextest --cargo-arg=--target-dir=target --timeout 300 -j "$jobs" "$@" -- --test-threads "$threads"
    rc=$?
    if test "$rc" -ge 128; then
        echo "error[mutation]: environment failure - cargo-mutants itself ended on signal $((rc - 128)), so the sweep is incomplete; past its memory bound the kernel ends processes inside the sweep's scope, and the step and the session are outside it"
    fi
    test "$rc" -eq 0 -o "$rc" -eq 2 -o "$rc" -eq 3
}

packages="$(test_packages unit.diff)"
echo "mutation gate: tests from $(printf '%s\n' "$packages" | sed 's/^--test-package //' | tr '\n' ' ')"
# shellcheck disable=SC2086 # one word per argument, package names carry no spaces
sweep --in-diff unit.diff $packages || exit 1

if test -s "$MUTANTS/rerun.args"; then
    grep -v '^-F$' "$MUTANTS/rerun.args" > "$MUTANTS/rerun.re" || exit 1
    env -u CARGO_TARGET_DIR xargs -d '\n' -a "$MUTANTS/rerun.args" cargo mutants --list --workspace > "$MUTANTS/rerun.all" || exit 1
    grep -E -f "$MUTANTS/rerun.re" "$MUTANTS/rerun.all" | sort -u > "$MUTANTS/rerun.list"
    cat mutants.out/*.txt 2>/dev/null | sort -u > "$MUTANTS/examined.txt"
    awk 'FILENAME == ARGV[1] { seen[$0] = 1; next } !($0 in seen)' "$MUTANTS/examined.txt" "$MUTANTS/rerun.list" > "$MUTANTS/rerun.todo"
    if test ! -s "$MUTANTS/rerun.todo"; then
        echo "rerun: nothing to rerun - none of the listed mutants exists in this tree, or the sweep above already examined each"
    else
        sed -E 's/^([^:]+):([0-9]+):.*/\1 \2/' "$MUTANTS/rerun.todo" | sort -u | while read -r f l; do
            printf -- '--- a/%s\n+++ b/%s\n@@ -%s,1 +%s,1 @@\n' "$f" "$f" "$l" "$l"
            sed -n "${l}p" "$f" | sed 's/^/-/'
            sed -n "${l}p" "$f" | sed 's/^/+/'
        done > "$MUTANTS/rerun.diff" || exit 1
        rerun_packages="$(test_packages "$MUTANTS/rerun.diff")"
        set --
        while IFS= read -r a; do set -- "$@" "$a"; done < "$MUTANTS/rerun.args"
        # shellcheck disable=SC2086 # one word per argument, as above
        sweep "$@" --in-diff "$MUTANTS/rerun.diff" --output "$MUTANTS/rerun" --baseline skip $rerun_packages || exit 1
        test -s "$MUTANTS/rerun/mutants.out/outcomes.json" || exit 1
        cat "$MUTANTS/rerun/mutants.out/missed.txt" >> mutants.out/missed.txt 2>/dev/null
    fi
fi

# Every mutant phase ended by a signal, one line each: name, phase, signal (see THE VERDICT).
# The build phase is the command carrying `--no-run`; a compiler it ran that ended on a signal
# shows as cargo's "(signal: N" line, or rustc's "failed: signal: N" for its linker.
for f in mutants.out/log/*.log "$MUTANTS"/rerun/mutants.out/log/*.log; do
    test -f "$f" || continue
    awk 'name == "" && /^\*\*\* / { sub(/^\*\*\* /, ""); name = $0; next }
        /^\*\*\* / && !/^\*\*\* (result:|mutation diff:)/ { phase = index($0, " --no-run") ? "build" : "test"; next }
        phase == "build" && match($0, /(\(|failed: )signal: [0-9]+/) { s = substr($0, RSTART, RLENGTH); gsub(/[^0-9]/, "", s); print name "\tbuild\t" s; exit }
        /^\*\*\* result: Signalled\([0-9]+\)/ { s = $0; gsub(/[^0-9]/, "", s); print name "\t" phase "\t" s; exit }' "$f"
done > mutants.out/ended.tsv || exit 1
awk -F '\t' '$2 == "test" { print "mutation gate: " $1 " - its test phase ended on signal " $3 ": counted as a detection, like a timeout (the mutant made its tests grow until the reaper ended them)" }' mutants.out/ended.tsv
awk -F '\t' '$2 != "test"' mutants.out/ended.tsv > mutants.out/environment.tsv
if test -s mutants.out/environment.tsv; then
    echo "error[mutation]: environment failure - $(wc -l < mutants.out/environment.tsv | tr -d ' ') mutants were never tested because a process of their build ended on a signal (past the sweep's memory bound the kernel's reaper ends processes inside its scope); they are neither caught nor missed, the anchor is not advanced, and the full list is mutants.out/environment.tsv in this worktree - rerun the gate"
    awk -F '\t' '{ print "error[mutation]: ENDED " $1 " - its build phase ended on signal " $3 }' mutants.out/environment.tsv | head -n 3
    exit 1
fi

printf '%s\n' "$RIGGER_RUN_BASE" > "$MUTANTS/last.new/base" || exit 1
git rev-parse HEAD > "$MUTANTS/last.new/tip" || exit 1
cp mutants.out/missed.txt "$MUTANTS/last.new/missed.txt" 2>/dev/null || : > "$MUTANTS/last.new/missed.txt"
for f in mutants.out/log/*.log "$MUTANTS"/rerun/mutants.out/log/*.log; do
    test -f "$f" || continue
    awk 'name == "" && /^\*\*\* / { sub(/^\*\*\* /, ""); name = $0; next } /FAIL \[/ { gsub(/\033\[[0-9;]*m/, ""); if (match($0, /\/[0-9]+\) [^ ]+/)) { b = substr($0, RSTART, RLENGTH); sub(/^\/[0-9]+\) /, "", b); sub(/^rigger::/, "", b); print name "\t" b; exit } }' "$f"
done > "$MUTANTS/last.new/caught.map" || exit 1
cat mutants.out/*.txt "$MUTANTS"/rerun/mutants.out/*.txt 2>/dev/null | sort -u > "$MUTANTS/examined.txt"
awk -F '\t' 'FILENAME == ARGV[1] { seen[$0] = 1; next } !($1 in seen)' "$MUTANTS/examined.txt" "$last/caught.map" 2>/dev/null >> "$MUTANTS/last.new/caught.map"
sort -u -o "$MUTANTS/last.new/caught.map" "$MUTANTS/last.new/caught.map" || exit 1
rm -rf "$last" && mv "$MUTANTS/last.new" "$last" || exit 1

if test -s mutants.out/missed.txt; then
    echo "error[mutation]: $(wc -l < mutants.out/missed.txt | tr -d ' ') missed mutants survive; pin every one with a test or delete the code it mutates - the full list is mutants.out/missed.txt in this worktree"
    echo "error[mutation]: the same list outlives this worktree at $last/missed.txt"
    sed 's/^/error[mutation]: MISSED /' mutants.out/missed.txt | head -n 3
    exit 1
fi
