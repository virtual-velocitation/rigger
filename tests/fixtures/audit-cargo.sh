#!/bin/sh
# A fixture standing in for the real `cargo` under `.rigger/gates/audit.sh`, for
# tests/principle_gates_wiring.rs. In write mode (RIGGER_AUDIT_WRITE=1) it regenerates the
# catalog, writing $AUDIT_CARGO_FRESH to docs/audit/catalog.json; otherwise it asserts, printing
# $AUDIT_CARGO_RED and failing when that is set, and passing when it is empty.
if [ "${RIGGER_AUDIT_WRITE:-}" = 1 ]; then
    printf '%s' "$AUDIT_CARGO_FRESH" > docs/audit/catalog.json
    exit 0
fi
if [ -n "${AUDIT_CARGO_RED:-}" ]; then
    echo "$AUDIT_CARGO_RED"
    exit 101
fi
echo "test result: ok"
