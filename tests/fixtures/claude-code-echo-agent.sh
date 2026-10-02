#!/bin/sh
# A fixture "agent" for the headless host launch (spec 104 criterion 1: THE LAUNCH IS
# TYPED). It stands in for the real `claude` binary so a test can assert the launch was
# typed correctly without a real session: it prints its own cwd, a couple of env vars a
# test cares about, and echoes back the first line it reads from stdin (the host's typed
# first stream-json user message) - then exits. A checked-in executable fixture (never
# written at test time), matching the same rationale cli.rs's own fixtures give: no
# concurrent test's fork+exec can inherit a write fd to it.
pwd
echo "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY"
echo "RIGGER_TEST_VAR=$RIGGER_TEST_VAR"
IFS= read -r line
echo "STDIN=$line"
