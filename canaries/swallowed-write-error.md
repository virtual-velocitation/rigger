---
id: swallowed-write-error
defect_class: swallowed-error
planted: true
anchor: src/persist.rs
expected_verdict: reject
expected_tier: lens
---
Persists a record and reports success unconditionally. The durable write's
`Result` is discarded with `let _ =`, so a failed flush (disk full, closed
handle) is SWALLOWED and the function returns `Ok(())` anyway - the caller
believes the record is durable when it is not. A critical write's error must
propagate.

```rust
use std::io::Write;

/// Append `record` to the open log and report whether it was durably written.
pub fn persist(log: &mut impl Write, record: &[u8]) -> std::io::Result<()> {
    // BUG: the write result is thrown away. A short write or an I/O error is
    // silently lost and the function still returns Ok - a phantom-durability
    // bug. This must be `log.write_all(record)?; log.flush()?;`.
    let _ = log.write_all(record);
    let _ = log.flush();
    Ok(())
}
```
