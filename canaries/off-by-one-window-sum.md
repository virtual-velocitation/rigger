---
id: off-by-one-window-sum
defect_class: off-by-one
planted: true
anchor: src/window.rs
expected_verdict: reject
expected_tier: adversary
---
A sliding-window sum. The loop bound reads one element PAST the end of `data`
(`0..=data.len()` visits index `data.len()`, which is out of bounds), so the
last iteration indexes past the slice and panics at runtime.

```rust
/// Sum every length-`w` window of `data`, returning one sum per window start.
pub fn window_sums(data: &[u64], w: usize) -> Vec<u64> {
    let mut out = Vec::new();
    // BUG: the window START must stop at `data.len() - w`, and the inner index
    // must stay `< data.len()`. `0..=data.len()` runs one past the end.
    for start in 0..=data.len() {
        let mut sum = 0;
        for i in start..start + w {
            sum += data[i]; // panics on the final `start`, and whenever start + w > len
        }
        out.push(sum);
    }
    out
}
```
