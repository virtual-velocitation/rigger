#!/bin/sh
# The audit gate (workspace split, step 0): DRY and YAGNI as a red gate. The simplification
# audit (tests/simplification_audit.rs) fails on any duplicate cluster left open, any dead-code
# ledger entry and any disposition that names no cluster.
#
# Regenerate first, then assert: `docs/audit/*` is generated, so two units that each regenerate
# it on their own branch leave the merged tree's catalog stale even when both are clean. The
# gate therefore regenerates the catalog in its own worktree (RIGGER_AUDIT_WRITE=1, output
# discarded) and only then asserts, so a landing is never red on drift alone.
#
# A red assertion fails the gate with its own diagnostic line under the assertion's output,
# naming the three ways to close it.
#
# A regenerated catalog the unit has not committed is named on its own line (the attempt
# checkpoint commits it), never reported as a failure.
#
# Usage: sh .rigger/gates/audit.sh   (from the worktree root)

RIGGER_AUDIT_WRITE=1 cargo test --test simplification_audit > /dev/null 2>&1
cargo test --test simplification_audit ||
    { echo "error[audit]: the simplification audit is red - close each open duplicate cluster or record why it is not one in docs/audit/duplication-dispositions.json, delete each dead-code ledger entry, and fix each disposition that names no cluster; the failing assertion is above"; exit 1; }
if test -n "$(git status --porcelain -- docs/audit)"; then
    echo "audit: docs/audit was regenerated for this tree and is not committed - commit the regenerated files with the change (drift alone never fails this gate)"
fi
exit 0
