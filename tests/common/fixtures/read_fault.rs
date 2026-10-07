//! THE READ FAULT: the one fault a test arms to prove that a reader of the tree hands no bytes,
//! or that a sink fails, on a regular in-scope file it cannot read.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Arm THE READ FAULT on the regular file `file`: strip every permission bit, so this process's
/// uid cannot read it, and answer whether a read of it now fails. A uid that reads a file
/// whatever its mode (uid 0) answers `false`, and the calling test ends there: the fault cannot
/// be armed, so there is nothing left to assert.
pub fn arm_read_fault(file: &Path) -> bool {
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000))
        .expect("strip the file's permission bits");
    std::fs::read(file).is_err()
}
