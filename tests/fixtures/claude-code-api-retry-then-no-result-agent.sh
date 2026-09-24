#!/bin/sh
# A fixture "agent" for FAILURE CLASS (spec 104 criterion 5): a session that carries a
# `system/api_retry` line - so the host has a category in hand - then ends WITHOUT ever
# writing a `result`, and without a `StopFailure` hook record either. Proves the SECOND
# priority source in "the class from, in order: the record written by the `StopFailure`
# hook, ... else the last `api_retry` category, else `unknown`" (Design, FAILURE CLASS).
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"33333333-3333-4333-8333-333333333333","model":"claude-haiku-4-5","mcp_servers":[]}'
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":500,"error_status":401,"error":"authentication_failed"}'
