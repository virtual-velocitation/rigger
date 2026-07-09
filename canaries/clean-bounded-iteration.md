---
id: clean-bounded-iteration
defect_class: none
planted: false
anchor: src/iterate.rs
expected_verdict: approve
expected_tier: ""
---
A KNOWN-GOOD control: a correctly-bounded iteration that indexes only valid
positions and returns a fresh owned result. There is no defect - the adjudicator
should APPROVE. Paired with the off-by-one canary, it checks the panel does NOT
reject a correctly-bounded loop that merely resembles the buggy one.

```rust
/// Double every element of `data`, returning a new vector.
pub fn doubled(data: &[u64]) -> Vec<u64> {
    let mut out = Vec::with_capacity(data.len());
    for &x in data.iter() {
        out.push(x.saturating_mul(2));
    }
    out
}
```
