---
id: clean-checked-add
defect_class: none
planted: false
anchor: src/checked.rs
expected_verdict: approve
expected_tier: ""
---
A KNOWN-GOOD control: a checked addition that propagates overflow as an error
rather than wrapping or panicking. There is no defect to find here - a panel
that rejects this (or raises a real finding about it) is over-reaching, and the
adjudicator should APPROVE it. This anchors the canary's false-positive rate.

```rust
/// Add two counters, returning an error on overflow rather than wrapping.
pub fn add_counts(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_add(b).ok_or("counter overflow")
}
```
