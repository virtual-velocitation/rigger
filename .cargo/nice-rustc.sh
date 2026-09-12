#!/bin/sh
# rustc wrapper: every compile runs at low CPU priority.
#
# WHY (2026-09-11): a rigger run fans out reviewers - per unit an SDET author, two lenses,
# an adversary and an adjudicator - and each one does an independent cold `cargo test` in its
# own target dir. Four parallel units put ~90 cargo/rustc/rust-lld processes on a 32-core
# workstation at once (load average 125) and the operator's desktop and foreground work
# stalled. `[build] jobs` caps one cargo, not the number of cargos, so the fix is priority:
# builds yield to interactive work and still use every idle core. Cargo invokes this wrapper
# as `<wrapper> <rustc> <args...>`; it execs the real rustc under `nice`. An environment
# `RUSTC_WRAPPER` (CI's sccache) overrides this config entry, so CI is unaffected.
exec nice -n 10 "$@"
