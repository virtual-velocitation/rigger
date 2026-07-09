---
id: resource-leak-scratch-file
defect_class: resource-leak
planted: true
anchor: src/scratch.rs
expected_verdict: reject
expected_tier: lens
---
Writes a payload through a temporary scratch file, then renames it into place.
The cleanup (`remove_file`) runs only on the happy path: every `?` early-return
between creating the scratch file and the rename LEAKS it on disk. A run that
fails partway (a full disk, a permission error) leaves an orphaned
`*.scratch` file behind every time.

```rust
use std::fs;
use std::io::Write;
use std::path::Path;

/// Atomically write `payload` to `dest` via a scratch file.
pub fn write_atomic(dest: &Path, payload: &[u8]) -> std::io::Result<()> {
    let scratch = dest.with_extension("scratch");
    let mut f = fs::File::create(&scratch)?;
    // BUG: if either of these `?`s returns early, `scratch` is never removed -
    // the error path leaks the temp file. A guard (or remove-on-drop) is needed.
    f.write_all(payload)?;
    f.sync_all()?;
    fs::rename(&scratch, dest)?;
    fs::remove_file(&scratch).ok(); // only reached on success (rename consumed it anyway)
    Ok(())
}
```
