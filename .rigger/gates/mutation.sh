#!/bin/sh
# The check-in mutation gate (spec 91): the diff-scoped `cargo mutants` sweep, run ONCE at the
# `checkin` stage - after every implement unit has integrated - never per implementer round.
# Wired as `mutation: { run: "sh .rigger/gates/mutation.sh" }` in .rigger/workflow.yml; `rigger
# init` writes this same file into a consumer project (one home: src/cli/setup.rs includes it).
#
# THE GATE ENVIRONMENT. `$MUTANTS` is the unit-keyed mutants root the conductor exports to
# every gate command (`worktree::unit_mutants_sibling`) and reaps at unit terminus; this script
# owns creating and wiping it each run. `$RIGGER_RUN_BASE` is the run branch's tip AT THE
# MOMENT the run started (`RunStarted.base_tip`) - never a merge base with the run branch: the
# checkin worktree branches off the run branch after every unit integrated, so a merge base
# there is already HEAD and would diff nothing. A run with no recorded base refuses loud.
#
# THE VERDICT. The gate fails on MISSED mutants only. A TIMEOUT is a detection (cargo-mutants
# exits 3): hang-class mutants are caught only by bounded waits (on spec 89's check-in the same
# four timed out at 1.5x and 3x the baseline with zero misses). Each mutant's test run is
# bounded at an absolute 300 s, never a multiple of the baseline: cargo-mutants tests only the
# mutated packages in its baseline, while each mutant also runs the root package's suite, so a
# multiple of the baseline says nothing about it (measured 2026-09-28: a rigger-domain-only
# baseline took 0 s, and the auto-set 20 s bound then timed out both mutants of the smoke run
# before their root-suite test run could report the catch). nextest ends any single hung test at 240 s (.config/nextest.toml).
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
# manager (a CI container) the sweep runs unbounded and the gate says so. `-j` is sized from
# the same bound: one job (a mutant copy's 8-way build plus its nextest run) is budgeted at
# 6 GiB, so jobs = bound / 6 GiB, at least 1 and at most 3 (three copies already saturate the
# 32 cores at 8 build jobs each). A bound too tight for the suite shows as a failing baseline
# run below, never as a quietly "caught" mutant.
#
# THE BASELINE STAYS ON. The checkin stage lists `test` before `mutation`, but the conductor
# runs every listed gate whatever the earlier ones returned and exports no record of their
# verdicts to a gate command, and the `test` gate's plain `cargo test` covers the root package
# alone - so nothing here can confirm the mutated packages are green on this tree. The
# baseline (cargo-mutants runs it over the mutated packages only: seconds, not minutes) is
# that confirmation; only the by-name rerun, which follows it on the same tree, skips its own.
#
# INCREMENTAL RE-SWEEPS (Byran, 2026-09-16: "only run mutations when the test has changed or
# the logic has changed"). The sweep leaves three facts under `mutation-anchor/`, a sibling of
# the unit roots under the scratch root (`${MUTANTS%/*}`): the tree it examined (`tip`), its
# misses (`missed.txt`) and, per caught mutant, the test binary whose first failure caught it
# (`caught.map`, read off the nextest FAIL line in each mutant's log). The anchor lives
# OUTSIDE `$MUTANTS` because that root is reclaimed with the unit (2026-09-19: an escalation
# reclaimed five hours of sweep state seconds after the gate wrote it). When the tip is an
# ancestor of HEAD the sweep covers (a) every mutant in the diff since it and (b) by name,
# every earlier miss plus every caught mutant whose catching binary's `tests/<binary>.rs`
# changed since the tip (a change under a nested tests/ directory re-runs every mutant a
# tests/*.rs binary caught; the crate's own unit-test binaries cannot see tests/ and keep
# their catches). The rerun's misses join the sweep's own missed.txt so one file is the
# verdict. No usable tip (a reclaimed unit, a fresh spec, the first sweep): the whole spec diff
# against `$RIGGER_RUN_BASE`. A solo-merging unit's post-merge re-sweep is therefore the empty
# merge delta and passes in seconds. A mutant the main sweep already examined is not examined
# again by name.
#
# THE GATE OWNS ITS INSTRUMENT (2026-09-17: a remediation round excluded two survivors by name
# with an equivalence argument that was wrong for one). The unit diff since `$RIGGER_RUN_BASE`
# may add no exclusion or examine key to .cargo/mutants.toml and no `mutants::skip` attribute;
# a diff that does fails the gate before any sweep, naming the lines. A mutant no test can
# distinguish is dead code to delete, never a name to exclude.
#
# THE STATE SURVIVES A FAILING SWEEP (2026-09-17). The new tree, misses and map are written to
# `last.new` and swapped in only once both passes completed; a sweep that dies earlier leaves
# the previous state in place. A by-name rerun whose names match no mutant is nothing to rerun,
# not a failure. The map carries forward every earlier entry for a mutant this sweep did not
# examine, so a catch recorded three sweeps ago still re-runs when its test changes.
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

last="${MUTANTS:-/nonexistent}"
last="${last%/*}/mutation-anchor"
anchor="$(cat "$last/tip" 2>/dev/null || true)"
{ test -n "$anchor" && test "$(git rev-list --count HEAD.."$anchor" 2>/dev/null || echo 1)" = 0; } || anchor=""

rerun="$(if test -n "$anchor"; then
    cat "$last/missed.txt" 2>/dev/null
    changed="$(git diff --name-only "$anchor" -- tests | sed -E 's#^tests/([^/]+)\.rs$#\1#; s#^tests/.*/.*#ALL#' | sort -u)"
    test -z "$changed" || awk -F '\t' -v ch="$changed" 'BEGIN { n = split(ch, a, "\n"); for (i = 1; i <= n; i++) set[a[i]] = 1 } ($2 in set) || ("ALL" in set && $2 != "rigger" && $2 != "bin/rigger") { print $1 }' "$last/caught.map" 2>/dev/null
fi | sort -u)"

if test -n "$anchor"; then
    git diff "$anchor" -- '*.rs' > unit.diff || exit 1
else
    git diff "$RIGGER_RUN_BASE" -- '*.rs' > unit.diff || exit 1
fi

if { git diff "$RIGGER_RUN_BASE" -- .cargo/mutants.toml; git diff "$RIGGER_RUN_BASE" -- '*.rs'; } | grep -E '^\+.*(exclude_re|examine_re|exclude_globs|examine_globs|mutants::skip)'; then
    echo "mutation gate: the unit narrows the sweep itself (an exclusion in .cargo/mutants.toml or a mutants::skip in the code); the gate owns its instrument - remove it and pin the mutant with a test or delete the code it mutates"
    exit 1
fi

rm -rf "${MUTANTS:?}"/rerun "$MUTANTS"/rerun.args "$MUTANTS"/rerun.re "$MUTANTS"/rerun.all "$MUTANTS"/rerun.list "$MUTANTS"/rerun.todo "$MUTANTS"/rerun.diff "$MUTANTS"/last.new "$MUTANTS"/examined.txt "$MUTANTS"/cargo-mutants-* mutants.out mutants.out.old &&
    mkdir -p "$MUTANTS/last.new" mutants.out || exit 1

if test -n "$rerun"; then
    printf '%s\n' "$rerun" | sed -E 's/[][\\.*^$+?(){}|]/\\&/g; s/^([^:]+):[0-9]+:[0-9]+:/\1(:[0-9]+:[0-9]+)?:/; s/^/-F\n/' > "$MUTANTS/rerun.args" || exit 1
fi

# The memory bound and the job count, both from MemAvailable at launch (see THE SWEEP IS
# BOUNDED AS A WHOLE above). RIGGER_MEMINFO names another meminfo file for the gate's tests.
avail_kb="$(awk '/^MemAvailable:/ { print $2; exit }' "${RIGGER_MEMINFO:-/proc/meminfo}" 2>/dev/null)"
avail_kb="${avail_kb:-0}"
bound_kb=$((avail_kb / 2))
jobs=$((bound_kb / (6 * 1024 * 1024)))
test "$jobs" -ge 1 || jobs=1
test "$jobs" -le 3 || jobs=3
scope=""
if test "$bound_kb" -gt 0 && command -v systemd-run > /dev/null 2>&1 &&
    systemd-run --user --scope --quiet -- true > /dev/null 2>&1; then
    scope="systemd-run --user --scope --quiet -p MemoryMax=${bound_kb}K --"
    echo "mutation gate: the sweep runs in its own scope, MemoryMax=${bound_kb}K (half of MemAvailable ${avail_kb}K), -j $jobs"
else
    echo "mutation gate: advisory - no systemd user scope here (systemd-run is absent, has no user manager, or MemAvailable is unreadable), so the sweep runs WITHOUT its own memory bound; -j $jobs"
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
# below from missed.txt) and 3 (timeouts, which are detections).
sweep() {
    TMPDIR="$MUTANTS" CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=1 CARGO_PROFILE_TEST_DEBUG=1 $scope env -u CARGO_TARGET_DIR \
        cargo mutants --workspace --test-tool nextest --cargo-arg=--target-dir=target --timeout 300 -j "$jobs" "$@"
    rc=$?
    if test "$rc" -ge 128; then
        echo "error[mutation]: the sweep ended on signal $((rc - 128)) - past its memory bound the kernel ends processes inside the sweep's scope; the step and the session are outside it"
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
