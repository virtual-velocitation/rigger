//! Project-skill fixtures: the paths under a project's `.rigger/skills/` that `rigger setup`
//! reads for its own copies of the shipped skills.

use std::path::Path;

/// Make the root-relative `rel` under `root` a path `rigger setup` cannot read for the project's
/// skill copies - a directory where it reads a copy's `SKILL.md` (`dir`), else a plain file where
/// it lists `.rigger/skills` - and return the error that read gives.
pub fn seed_unreadable_project_skill(root: &Path, rel: &str, dir: bool) -> std::io::Error {
    let path = root.join(rel);
    if dir {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::read(&path).unwrap_err()
    } else {
        super::write_file(&path, b"not a directory\n");
        std::fs::read_dir(&path).unwrap_err()
    }
}
