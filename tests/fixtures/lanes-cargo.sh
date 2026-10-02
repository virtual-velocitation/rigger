#!/bin/sh
# A fixture standing in for the real `cargo` under `.rigger/gates/lanes.sh`, for
# tests/ci_lanes.rs: answers `cargo metadata` with a one-member workspace whose member declares
# a `core` feature, passes every other command, and has every `cargo test` print two test
# binaries' `running` lines - the first running $LANES_CARGO_TESTS tests (default 0), the second
# none - before exiting $LANES_CARGO_EXIT (default 0). A test drives the lane's zero-test
# refusal and its exit-status propagation through it without compiling anything.
case "$1" in
metadata)
    echo '{"packages":[{"id":"m 0.1.0","name":"m","features":{"core":[]},"targets":[{"kind":["lib"]}]}],"workspace_members":["m 0.1.0"]}'
    ;;
test)
    echo "running ${LANES_CARGO_TESTS:-0} tests"
    echo "running 0 tests"
    exit "${LANES_CARGO_EXIT:-0}"
    ;;
esac
exit 0
