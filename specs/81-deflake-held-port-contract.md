# 81 - Deflake the held-port contract test: compare the stable contract, not a scheduler race

**Goal:** `held_port_holder_public_contract_holds_at_the_crate_boundary` (tests/cli.rs:~26309,
spec 62 u62c3) asserts that `held_port_holder`'s message half and
`describe_held_port_if_confirmed`'s return are byte-identical - but the message embeds the
holder's `/proc` scheduler state (`(state S)`), which is time-varying, and the test computes
the two messages by two independently-timed reads. CI hit the race on the first run of PR #27:
`state S` vs `state R` for the same pid, everything else identical. The shared-implementation
contract the test documents is real; the assertion is what races.

## Design

Decided here: fix the TEST, not the product - the state field is diagnostic value and stays in
the message. The test normalizes the volatile scheduler-state token in BOTH strings (e.g.
replace the `(state <letter>)` suffix with `(state _)`) before the equality assert, so the
assertion proves exactly the documented contract - same pid, same address, same wording, one
shared implementation - while a scheduler flap between reads can no longer fail it. No change
to `held_port_holder` or `describe_held_port_if_confirmed`; no other test touched.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green; the no-os-kill gate green on the diff.
- Test-only diff: zero changes under `src/`.

## Done when

- [ ] the contract test normalizes the scheduler-state token in both compared strings before asserting equality, keeps asserting pid/address/wording agreement verbatim, and demonstrably cannot be failed by a state flap (the normalization is itself exercised by comparing strings that differ only in the state letter). This criterion OWNS tests/cli.rs's held-port contract test and nothing else.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
