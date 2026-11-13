//! Where a directory's git repository lies and what its checkout holds, read from the files git
//! keeps, as git itself reads them to set a repository up, rather than asked of a git process.
//! Every layout git finds a checkout in is read: a `.git` directory or a symlink to one, and a
//! `.git` file naming its administrative directory by an absolute or a relative path (a linked
//! worktree, a git directory kept apart, a submodule), under per-worktree configuration, a work
//! tree configured where the `.git` lies, and the ceilings and device rule the environment sets
//! for git's walk. What the files cannot answer as git does is left to git, so every answer is
//! the one git gives: a bare repository, a work tree configured elsewhere, a configuration git
//! refuses, a path not spelled in UTF-8, a repository format or extension git may refuse, a
//! `.git` directory naming a common directory, a repository this user does not own, an OS whose
//! ownership is not read here, or refs held in a reftable. A directory lies in no repository only when a walk up from it, stopping where git's
//! own walk stops, meets neither a `.git` entry nor a bare repository; git refusing anywhere else
//! is a failure, never that none is there.

use crate::worktree::{git_failure, git_output, GitOutput};
use rigger_process::file_trust::{device_of, owned_by_this_user};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The directories git's walk up from a directory never enters.
const CEILINGS: &str = "GIT_CEILING_DIRECTORIES";

/// Whether git's walk up from a directory crosses devices.
const ACROSS_DEVICES: &str = "GIT_DISCOVERY_ACROSS_FILESYSTEM";

/// The repository a directory lies in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    /// The physical top of the worktree the directory lies in.
    pub toplevel: PathBuf,
    /// The worktree's administrative directory, physical: the `.git` directory, or the one a
    /// `.git` file names.
    pub git_dir: PathBuf,
    /// The directory every worktree of the repository shares, physical.
    pub common_dir: PathBuf,
    /// The physical directory's path below `toplevel`.
    prefix: PathBuf,
    /// How git shows `common_dir`.
    shown: CommonShown,
    /// The main checkout git records for the repository ([`Format::main_checkout`]).
    main: Option<PathBuf>,
}

/// How `git rev-parse --git-common-dir` shows a repository's common directory, which follows
/// from how git set the repository up.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CommonShown {
    /// Relative to the directory asked from: git found a `.git` directory and works from the top
    /// of the checkout, where it names it `.git`.
    Relative,
    /// `.git` at the top of the checkout and absolute below it: git found a `.git` directory
    /// under a configured work tree, which it takes to its physical path when the directory
    /// asked from lies below the top.
    RelativeAtTop,
    /// Absolute: a `.git` file names the administrative directory, which git takes to its
    /// physical path.
    Absolute,
    /// As git showed it when it set the repository up itself ([`discover`]).
    Asked(String),
}

/// What `git rev-parse` answers about where a directory's repository lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// `--show-toplevel`: the physical top of the worktree.
    Toplevel,
    /// `--git-common-dir`: the common directory, relative to the directory asked from in a
    /// checkout holding a `.git` directory and absolute where a `.git` file names it.
    CommonDir,
    /// `--show-prefix`: the directory's path below the top of its worktree, each part followed
    /// by `/`; empty at the top.
    Prefix,
    /// `--absolute-git-dir`: the worktree's administrative directory, which holds its index.
    GitDir,
}

impl Location {
    fn flag(self) -> &'static str {
        match self {
            Location::Toplevel => "--show-toplevel",
            Location::CommonDir => "--git-common-dir",
            Location::Prefix => "--show-prefix",
            Location::GitDir => "--absolute-git-dir",
        }
    }
}

impl Repository {
    /// What `git rev-parse` answers for `ask` from the directory this repository was read from.
    pub fn answer(&self, ask: Location) -> String {
        let at_top = self.prefix.as_os_str().is_empty();
        match ask {
            Location::Toplevel => self.toplevel.to_string_lossy().into_owned(),
            Location::CommonDir => match &self.shown {
                CommonShown::Relative => {
                    format!("{}.git", "../".repeat(self.prefix.components().count()))
                }
                CommonShown::RelativeAtTop if at_top => ".git".to_owned(),
                CommonShown::RelativeAtTop | CommonShown::Absolute => {
                    self.common_dir.to_string_lossy().into_owned()
                }
                CommonShown::Asked(shown) => shown.clone(),
            },
            Location::Prefix => self
                .prefix
                .iter()
                .map(|part| format!("{}/", part.to_string_lossy()))
                .collect(),
            Location::GitDir => self.git_dir.to_string_lossy().into_owned(),
        }
    }

    /// The physical top of the main checkout git records for the repository: none where git
    /// records none, as for a git directory kept apart, a bare repository or a `.git`
    /// configured bare ([`Format::main_checkout`]).
    pub fn main_checkout(&self) -> Option<&Path> {
        self.main.as_deref()
    }

    /// The directory the repository's store and the tree it reads lie at: the main checkout git
    /// records ([`Repository::main_checkout`]), the one root every worktree of the repository
    /// shares, and where git records none, the worktree's own top.
    pub fn main_root(&self) -> PathBuf {
        self.main.clone().unwrap_or_else(|| self.toplevel.clone())
    }

    /// The branch and commit the `HEAD` of the root's checkout names ([`Repository::main_root`]),
    /// which the store is read at: the common directory's, the main checkout's, where git records
    /// one, and otherwise the worktree's own. Read from git's files
    /// ([`Repository::head_with`]): None where they leave it to git.
    pub fn main_head(&self) -> Option<(String, Option<String>)> {
        let admin = match self.main {
            Some(_) => &self.common_dir,
            None => &self.git_dir,
        };
        self.head_with(admin, &mut |path| std::fs::read_to_string(path))
    }

    /// The branch and commit the `HEAD` in `admin`, one of the repository's administrative
    /// directories, names, each of git's files read through `read`: the short name of the branch
    /// (`HEAD` when detached) and the commit it is at (none on a branch with no commit yet), as
    /// `git symbolic-ref --short -q HEAD` and `git rev-parse --verify -q HEAD^{commit}` answer
    /// them. None when the files leave it to git: refs in a reftable, a `HEAD` naming a ref
    /// outside the branches, a branch name another ref also spells (which git would not shorten),
    /// or a symbolic branch ref. A ref is read as git reads it: loose first, and only where it is
    /// not loose from a `packed-refs` read after that. `git pack-refs` writes `packed-refs` before
    /// it deletes the loose refs it packs, so a ref a pack moves between the two reads is in the
    /// later one.
    fn head_with(
        &self,
        admin: &Path,
        read: &mut dyn FnMut(&Path) -> std::io::Result<String>,
    ) -> Option<(String, Option<String>)> {
        if self.common_dir.join("reftable").exists() {
            return None;
        }
        let head = read(&admin.join("HEAD")).ok()?;
        let head = head.trim_end_matches(['\n', '\r']);
        let Some(full) = head.strip_prefix("ref: ") else {
            return is_object_id(head).then(|| ("HEAD".to_owned(), Some(head.to_owned())));
        };
        let name = full.strip_prefix("refs/heads/")?;
        let others = [
            name.to_owned(),
            format!("refs/{name}"),
            format!("refs/tags/{name}"),
            format!("refs/remotes/{name}"),
            format!("refs/remotes/{name}/HEAD"),
        ];
        if others.iter().any(|other| self.has_loose_ref(admin, other)) {
            return None;
        }
        let packed = self.packed_refs(read)?;
        if others
            .iter()
            .any(|other| packed_ref(&packed, other).is_some())
        {
            return None;
        }
        let commit = match read(&self.common_dir.join(full)) {
            Ok(loose) => {
                let loose = loose.trim_end_matches(['\n', '\r']);
                Some(is_object_id(loose).then(|| loose.to_owned())?)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                packed_ref(&self.packed_refs(read)?, full).map(str::to_owned)
            }
            Err(_) => return None,
        };
        Some((name.to_owned(), commit))
    }

    /// Whether a ref named `name` lies loose in `admin`, the administrative directory whose
    /// `HEAD` is read, or in the common directory.
    fn has_loose_ref(&self, admin: &Path, name: &str) -> bool {
        admin.join(name).is_file() || self.common_dir.join(name).is_file()
    }

    /// The text of the common directory's `packed-refs`, read through `read`: empty when there
    /// is none, None when it cannot be read.
    fn packed_refs(
        &self,
        read: &mut dyn FnMut(&Path) -> std::io::Result<String>,
    ) -> Option<String> {
        match read(&self.common_dir.join("packed-refs")) {
            Ok(packed) => Some(packed),
            Err(e) if e.kind() == ErrorKind::NotFound => Some(String::new()),
            Err(_) => None,
        }
    }
}

/// Where git's walk up from a directory stops, as its environment sets it.
#[derive(Debug, Default)]
struct Reach {
    /// The directories the walk never enters ([`CEILINGS`]): its absolute entries, each resolved
    /// to its physical path up to the first empty entry and taken as written after it, as git
    /// takes them. An entry that cannot be resolved is none.
    ceilings: Vec<PathBuf>,
    /// Whether the walk crosses devices ([`ACROSS_DEVICES`], read as git reads a boolean).
    across_devices: bool,
}

impl Reach {
    /// The reach this process's environment sets, as git reads it: None when git refuses its
    /// [`ACROSS_DEVICES`] value ([`git_bool`]).
    fn from_env() -> Option<Self> {
        let across_devices = match std::env::var_os(ACROSS_DEVICES) {
            Some(value) => git_bool(Some(value.to_str()?.as_bytes()))?,
            None => false,
        };
        Some(Self::of(std::env::var_os(CEILINGS), across_devices))
    }

    /// The longest reach this process's environment may set: [`Reach::from_env`], crossing
    /// devices where it gives none.
    fn longest_from_env() -> Self {
        Self::from_env().unwrap_or_else(|| Self::of(std::env::var_os(CEILINGS), true))
    }

    /// The reach a [`CEILINGS`] value and an [`ACROSS_DEVICES`] setting give.
    fn of(ceilings: Option<OsString>, across_devices: bool) -> Self {
        let mut resolved = true;
        let ceilings = ceilings
            .iter()
            .flat_map(std::env::split_paths)
            .filter_map(|entry| {
                if entry.as_os_str().is_empty() {
                    resolved = false;
                    None
                } else if !entry.is_absolute() {
                    None
                } else if resolved {
                    std::fs::canonicalize(entry).ok()
                } else {
                    Some(entry)
                }
            })
            .collect();
        Self {
            ceilings,
            across_devices,
        }
    }
}

/// What a walk up from a directory, stopping where git's own walk stops, meets first.
#[derive(Debug, PartialEq, Eq)]
enum Met {
    /// A `.git` directory, or a `.git` file when `file`, each itself or the target of a symlink,
    /// in `toplevel`, `prefix` below which the start lies.
    Dot {
        toplevel: PathBuf,
        prefix: PathBuf,
        file: bool,
    },
    /// What git may take for a repository and this reading does not follow: a `.git` that is
    /// neither a file nor a directory, a symlink to nothing, a bare repository's shape, or an
    /// entry it cannot read.
    Unread,
    /// Nothing: the walk reached a ceiling, a device boundary or the root, or the start does
    /// not exist.
    Nothing,
}

/// Walks up from `start` as git does: from its physical path, never into a ceiling of `reach`
/// that is a proper ancestor, and never onto another device unless `reach` crosses devices
/// (a device this OS does not report is never a boundary, which can only lengthen the walk).
fn walk(start: &Path, reach: &Reach) -> Met {
    // An empty `start` is the current directory, as `git -C ""` takes it.
    let start = if start.as_os_str().is_empty() {
        Path::new(".")
    } else {
        start
    };
    let physical = match std::fs::canonicalize(start) {
        Ok(physical) => physical,
        Err(e) if e.kind() == ErrorKind::NotFound => return Met::Nothing,
        Err(_) => return Met::Unread,
    };
    let ceiling = reach
        .ceilings
        .iter()
        .filter(|ceiling| physical.starts_with(ceiling) && physical != **ceiling)
        .map(|ceiling| ceiling.components().count())
        .max();
    let device = if reach.across_devices {
        None
    } else {
        device_of(&physical)
    };
    for dir in physical.ancestors() {
        if ceiling.is_some_and(|depth| dir.components().count() <= depth) {
            return Met::Nothing;
        }
        if let Some(device) = device {
            match device_of(dir) {
                Some(here) if here == device => {}
                Some(_) => return Met::Nothing,
                None => return Met::Unread,
            }
        }
        let dot = dir.join(".git");
        match std::fs::symlink_metadata(&dot) {
            Ok(_) => {
                // git reads a `.git` symlink as what it points to.
                let Ok(meta) = std::fs::metadata(&dot) else {
                    return Met::Unread;
                };
                let Ok(prefix) = physical.strip_prefix(dir) else {
                    return Met::Unread;
                };
                if !meta.is_dir() && !meta.is_file() {
                    return Met::Unread;
                }
                return Met::Dot {
                    toplevel: dir.to_path_buf(),
                    prefix: prefix.to_path_buf(),
                    file: meta.is_file(),
                };
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return Met::Unread,
        }
        if dir.join("HEAD").exists() && dir.join("objects").is_dir() {
            return Met::Unread;
        }
    }
    Met::Nothing
}

/// The repository `start` lies in, read from git's files: `Some(Some(_))` when found,
/// `Some(None)` when `start` lies in none, and `None` when the files leave the answer to git.
pub fn read_repository(start: &Path) -> Option<Option<Repository>> {
    read_repository_within(start, &Reach::from_env()?)
}

/// [`read_repository`], git's walk up from `start` stopping where `reach` stops it.
fn read_repository_within(start: &Path, reach: &Reach) -> Option<Option<Repository>> {
    match walk(start, reach) {
        Met::Dot {
            toplevel,
            prefix,
            file,
        } => checkout(&toplevel, prefix, file).map(Some),
        Met::Unread => None,
        Met::Nothing => Some(None),
    }
}

/// The checkout at `toplevel`, whose `.git` is a file naming its administrative directory when
/// `file` and that directory itself otherwise, `prefix` below which the start lies, set up as git
/// sets it up: None where the files leave it to git. The administrative directory is the one the
/// `.git` file names, relative to the `.git` file unless absolute, and the common directory the
/// one its `commondir` names, relative to it unless absolute, else itself; both physical. Both
/// hold what git requires of a repository, a `HEAD` in the one and `objects` and `refs` in the
/// other. The configuration git reads the format from is the common directory's, and its
/// `core.bare` and `core.worktree` apply only where no `commondir` is named or per-worktree
/// configuration is on, which the worktree's `config.worktree` then overrides. A checkout set up
/// bare, or with a work tree other than `toplevel`, is left to git. The main checkout is the one
/// the common directory's configuration records ([`Format::main_checkout`]).
fn checkout(toplevel: &Path, prefix: PathBuf, file: bool) -> Option<Repository> {
    let dot = toplevel.join(".git");
    let git_dir = if file {
        let text = std::fs::read(&dot).ok()?;
        let named = path_of(line_of(text.strip_prefix(b"gitdir: ")?))?;
        std::fs::canonicalize(toplevel.join(named)).ok()?
    } else {
        std::fs::canonicalize(&dot).ok()?
    };
    // A `.git` directory naming a common directory is no layout git makes.
    let common_dir = match std::fs::read(git_dir.join("commondir")) {
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Ok(named) if file => {
            Some(std::fs::canonicalize(git_dir.join(path_of(line_of(&named))?)).ok()?)
        }
        _ => return None,
    };
    let shared = common_dir.is_some();
    let common_dir = common_dir.unwrap_or_else(|| git_dir.clone());
    let repository = has_head(&git_dir)
        && common_dir.join("objects").is_dir()
        && common_dir.join("refs").is_dir();
    if !repository {
        return None;
    }
    let common = read_format(&std::fs::read(common_dir.join("config")).ok()?)?;
    let main = common.clone().main_checkout(&common_dir)?;
    let format = common.overlaid(&git_dir)?;
    let (bare, worktree) = if !shared || format.worktree_config {
        (format.bare, format.worktree)
    } else {
        (None, None)
    };
    if bare == Some(true) {
        return None;
    }
    if let Some(worktree) = &worktree {
        if std::fs::canonicalize(git_dir.join(worktree)).ok()? != toplevel {
            return None;
        }
    }
    let shown = match (file, worktree.is_some()) {
        (true, _) => CommonShown::Absolute,
        (false, true) => CommonShown::RelativeAtTop,
        (false, false) => CommonShown::Relative,
    };
    owned(&[toplevel, &dot, &git_dir, &common_dir]).then(|| Repository {
        toplevel: toplevel.to_path_buf(),
        git_dir,
        common_dir,
        prefix,
        shown,
        main,
    })
}

/// `text` without the line ends git trims from a file naming a path.
fn line_of(text: &[u8]) -> &[u8] {
    let end = text
        .iter()
        .rposition(|&c| c != b'\n' && c != b'\r')
        .map_or(0, |last| last + 1);
    &text[..end]
}

/// The path `text` spells: None when it is empty or not UTF-8.
fn path_of(text: &[u8]) -> Option<PathBuf> {
    let text = std::str::from_utf8(text).ok()?;
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// Whether `git_dir` holds a `HEAD` git takes for a repository's: a file naming a ref under
/// `refs/` or holding an object id.
fn has_head(git_dir: &Path) -> bool {
    let head = git_dir.join("HEAD");
    std::fs::symlink_metadata(&head).is_ok_and(|meta| meta.is_file())
        && std::fs::read_to_string(&head).is_ok_and(|text| {
            let text = text.trim_end_matches(['\n', '\r']);
            text.strip_prefix("ref: ")
                .map_or_else(|| is_object_id(text), |name| name.starts_with("refs/"))
        })
}

/// What a repository's configuration tells git about where its checkout lies ([`read_format`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Format {
    /// Whether `extensions.worktreeConfig` is on: the worktree's own `config.worktree` is read,
    /// and the `core.bare` and `core.worktree` it settles apply to it.
    worktree_config: bool,
    /// `core.bare`, where set.
    bare: Option<bool>,
    /// `core.worktree`, where set.
    worktree: Option<PathBuf>,
}

impl Format {
    /// Take the entry `name`, a lowercased configuration name, set to `value`, where it is
    /// `core.bare` or `core.worktree`: None where git refuses its value or it names a path not
    /// spelled in UTF-8.
    fn take(&mut self, name: &str, value: Option<Vec<u8>>) -> Option<()> {
        match name {
            "core.bare" => self.bare = Some(git_bool(value.as_deref())?),
            "core.worktree" => self.worktree = Some(path_of(&value?)?),
            _ => {}
        }
        Some(())
    }

    /// The format a configuration's `entries` give, whatever its version and extensions:
    /// [`Format::take`] for each, and `extensions.worktreeConfig`. None where git refuses a value
    /// it reads.
    fn of(entries: Vec<(String, Option<Vec<u8>>)>) -> Option<Self> {
        let mut format = Self::default();
        for (name, value) in entries {
            if name == "extensions.worktreeconfig" {
                format.worktree_config = git_bool(value.as_deref())?;
            } else {
                format.take(&name, value)?;
            }
        }
        Some(format)
    }

    /// This format, a common directory's, as the worktree whose administrative directory is
    /// `admin` reads it: under per-worktree configuration with `admin`'s `config.worktree` read
    /// after it. None where that file cannot be read or git refuses it.
    fn overlaid(mut self, admin: &Path) -> Option<Self> {
        if !self.worktree_config {
            return Some(self);
        }
        match std::fs::read(admin.join("config.worktree")) {
            Ok(text) => {
                for (name, value) in config_entries(&text)? {
                    self.take(&name, value)?;
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        Some(self)
    }

    /// The main checkout git records in this format, the physical common directory
    /// `common_dir`'s, read as git answers `git config` asked from the common directory (with its
    /// own `config.worktree`, [`Format::overlaid`]): none where `core.bare` is true; the physical
    /// directory `core.worktree` names, taken from the common directory, where it is set; the
    /// common directory's parent where the common directory is named `.git`; and none anywhere
    /// else, as for a git directory kept apart, a bare repository or a `.git` that is a symlink to
    /// a directory of another name, where `git worktree list` names the common directory itself
    /// as the main worktree or marks it bare. None where the configuration cannot be read or the
    /// directory `core.worktree` names does not resolve.
    fn main_checkout(self, common_dir: &Path) -> Option<Option<PathBuf>> {
        let format = self.overlaid(common_dir)?;
        if format.bare == Some(true) {
            return Some(None);
        }
        if let Some(worktree) = &format.worktree {
            return Some(Some(std::fs::canonicalize(common_dir.join(worktree)).ok()?));
        }
        let dot_git = common_dir.file_name() == Some(".git".as_ref());
        let main = common_dir.parent().filter(|_| dot_git);
        Some(main.map(Path::to_path_buf))
    }
}

/// The format a repository's configuration `text` gives, as git reads it to set the repository
/// up: None where git may refuse it, as for a format version other than 0 or 1, none at all, or
/// an extension that version cannot carry or git may not know, or where [`Format::take`] leaves
/// a value to git.
fn read_format(text: &[u8]) -> Option<Format> {
    let entries = config_entries(text)?;
    let mut version = None;
    for (name, value) in &entries {
        if name == "core.repositoryformatversion" {
            version = Some(git_int(value.as_deref()?)?);
        }
    }
    let version_1 = match version? {
        0 => false,
        1 => true,
        _ => return None,
    };
    for (name, value) in &entries {
        let Some(extension) = name.strip_prefix("extensions.") else {
            continue;
        };
        match extension {
            "noop" => {}
            "preciousobjects" | "worktreeconfig" => {
                git_bool(value.as_deref())?;
            }
            "partialclone" => {
                value.as_ref()?;
            }
            "noop-v1" if version_1 => {}
            "objectformat" | "compatobjectformat"
                if version_1 && matches!(value.as_deref(), Some(b"sha1" | b"sha256")) => {}
            _ => return None,
        }
    }
    Format::of(entries)
}

/// Every entry of a configuration file's `text`, in order, as the newest git reads a file, the
/// rule git 2.45.0 set: the name `section.key` or `section.subsection.key` (a key before any
/// section stands alone), lowercased but for a subsection; the value none for a key written
/// without `=`, and white space inside it kept as written ([`Source::value`]). None where git
/// refuses the file. An older git reads each tab or lone carriage return inside an unquoted
/// value as a space.
fn config_entries(text: &[u8]) -> Option<Vec<(String, Option<Vec<u8>>)>> {
    let mut source = Source {
        text: text.strip_prefix(b"\xef\xbb\xbf").unwrap_or(text),
        at: 0,
        ended: false,
    };
    let (mut entries, mut section, mut comment) = (Vec::new(), None::<String>, false);
    loop {
        let c = source.next();
        match c {
            b'\n' if source.ended => return Some(entries),
            b'\n' => comment = false,
            _ if comment || is_space(c) => {}
            b'#' | b';' => comment = true,
            b'[' => section = Some(source.section()?),
            _ if c.is_ascii_alphabetic() => {
                let (key, value) = source.entry(c)?;
                let name = match &section {
                    Some(section) => format!("{section}.{key}"),
                    None => key,
                };
                entries.push((name, value));
            }
            _ => return None,
        }
    }
}

/// A configuration file's text read a byte at a time as git's own parser reads it: `\r\n` as
/// `\n`, and the end as a `\n` that sets `ended`.
struct Source<'t> {
    text: &'t [u8],
    at: usize,
    ended: bool,
}

impl Source<'_> {
    fn next(&mut self) -> u8 {
        let Some(&c) = self.text.get(self.at) else {
            self.ended = true;
            return b'\n';
        };
        self.at += 1;
        if c == b'\r' && self.text.get(self.at) == Some(&b'\n') {
            self.at += 1;
            return b'\n';
        }
        c
    }

    /// A section header's name, read from after its `[`: lowercased, and for a header naming a
    /// subsection in quotes followed by `.` and the subsection as written. None where git
    /// refuses the header.
    fn section(&mut self) -> Option<String> {
        let mut name = String::new();
        loop {
            match self.next() {
                b']' if !name.is_empty() => return Some(name),
                c if is_space(c) => return self.subsection(name, c),
                c if is_key_char(c) || c == b'.' => name.push(char::from(c.to_ascii_lowercase())),
                _ => return None,
            }
        }
    }

    /// The section `name` followed by `.` and the quoted subsection a header names, read from
    /// `c`, the white space after the name. None where git refuses the header.
    fn subsection(&mut self, mut name: String, mut c: u8) -> Option<String> {
        while is_space(c) {
            if c == b'\n' {
                return None;
            }
            c = self.next();
        }
        if c != b'"' {
            return None;
        }
        let mut subsection = Vec::new();
        loop {
            let mut c = self.next();
            if c == b'"' {
                break;
            }
            if c == b'\\' {
                c = self.next();
            }
            if c == b'\n' {
                return None;
            }
            subsection.push(c);
        }
        if self.next() != b']' {
            return None;
        }
        name.push('.');
        name.push_str(&String::from_utf8_lossy(&subsection));
        Some(name)
    }

    /// A key beginning with `first`, lowercased, and its value: none for a key written without
    /// `=`. None where git refuses the line.
    fn entry(&mut self, first: u8) -> Option<(String, Option<Vec<u8>>)> {
        let mut key = String::from(char::from(first.to_ascii_lowercase()));
        let mut c = self.next();
        while is_key_char(c) {
            key.push(char::from(c.to_ascii_lowercase()));
            c = self.next();
        }
        while c == b' ' || c == b'\t' {
            c = self.next();
        }
        match c {
            b'\n' => Some((key, None)),
            b'=' => Some((key, Some(self.value()?))),
            _ => None,
        }
    }

    /// A value, read from after its `=` to the end of its line: quotes kept out, escapes and a
    /// line continued by `\` taken as git takes them, a comment outside quotes dropped, white
    /// space outside quotes dropped at either end (before a comment too) and kept as written
    /// inside, a run before a line continued included. None where git refuses it.
    fn value(&mut self) -> Option<Vec<u8>> {
        let (mut value, mut quoted, mut comment, mut spaces) =
            (Vec::new(), false, false, Vec::new());
        loop {
            let c = self.next();
            match c {
                b'\n' if quoted => return None,
                b'\n' => return Some(value),
                _ if comment => {}
                _ if is_space(c) && !quoted => {
                    if !value.is_empty() {
                        spaces.push(c);
                    }
                }
                b'#' | b';' if !quoted => comment = true,
                _ => {
                    value.append(&mut spaces);
                    match c {
                        b'\\' => value.push(match self.next() {
                            b'\n' => continue,
                            b't' => b'\t',
                            b'b' => 8,
                            b'n' => b'\n',
                            c @ (b'\\' | b'"') => c,
                            _ => return None,
                        }),
                        b'"' => quoted = !quoted,
                        _ => value.push(c),
                    }
                }
            }
        }
    }
}

/// Whether git's configuration parser takes `c` for white space.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// Whether `c` may stand in a configuration section or key name.
fn is_key_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// A value read as git reads a boolean: true for a key written without one, a word git takes
/// for true or false in any case, or an integer, true unless zero ([`git_int`]). None for any
/// other value, which git refuses.
fn git_bool(value: Option<&[u8]>) -> Option<bool> {
    let Some(value) = value else {
        return Some(true);
    };
    match value.to_ascii_lowercase().as_slice() {
        b"true" | b"yes" | b"on" => Some(true),
        b"" | b"false" | b"no" | b"off" => Some(false),
        _ => git_int(value).map(|n| n != 0),
    }
}

/// `value` read as the newest git reads an integer, the rule git 2.50.0 set: after any white
/// space and an optional sign, digits in hex after `0x`, in octal after a leading `0`, else in
/// decimal, then optionally a unit `k`, `m` or `g` in any case, multiplying by 1024 for each
/// step, the whole within `i32::MIN..=i32::MAX`. None for any other value, which git refuses.
/// An older git refuses `i32::MIN` too, however it is written (`-2147483648`, `-2097152k`,
/// `-2048m`, `-2g`).
fn git_int(value: &[u8]) -> Option<i32> {
    let start = value
        .iter()
        .position(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))?;
    let mut rest = &value[start..];
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'-' | b'+')) {
        rest = &rest[1..];
    }
    let hex = rest.len() > 2
        && rest[0] == b'0'
        && matches!(rest[1], b'x' | b'X')
        && rest[2].is_ascii_hexdigit();
    let (radix, digits) = match rest {
        _ if hex => (16, &rest[2..]),
        [b'0', ..] => (8, rest),
        _ => (10, rest),
    };
    let count = digits
        .iter()
        .take_while(|c| char::from(**c).is_digit(radix))
        .count();
    if count == 0 {
        return None;
    }
    let mut magnitude: i64 = 0;
    for c in &digits[..count] {
        let digit = char::from(*c).to_digit(radix)?;
        magnitude = magnitude
            .checked_mul(i64::from(radix))?
            .checked_add(i64::from(digit))?;
    }
    let factor: i64 = match digits[count..].to_ascii_lowercase().as_slice() {
        b"" => 1,
        b"k" => 1 << 10,
        b"m" => 1 << 20,
        b"g" => 1 << 30,
        _ => return None,
    };
    let signed = if negative { -magnitude } else { magnitude };
    i32::try_from(signed.checked_mul(factor)?).ok()
}

/// Whether this user owns every one of `paths`, as git requires before it trusts a repository.
fn owned(paths: &[&Path]) -> bool {
    paths
        .iter()
        .all(|path| owned_by_this_user(path) == Some(true))
}

/// The object id `packed` (a `packed-refs` file's text) holds for the ref `name`.
fn packed_ref<'p>(packed: &'p str, name: &str) -> Option<&'p str> {
    packed
        .lines()
        .filter(|line| !line.starts_with(['#', '^']))
        .find_map(|line| {
            let (id, rest) = line.split_once(' ')?;
            (rest == name).then_some(id)
        })
}

/// Whether `text` is a full object id: forty (SHA-1) or sixty-four (SHA-256) lowercase hex
/// digits.
fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `git -C <start> rev-parse <ask>`'s trimmed answer: read from git's files where they answer it
/// ([`read_repository`]), else asked of git. None when `start` lies in no repository;
/// a failure naming git's refusal when git refuses where a walk up from `start` meets a `.git`
/// entry or a bare repository ([`walk`]), as in a worktree whose administrative directory is
/// gone or a repository this user does not own.
pub fn rev_parse(start: &Path, ask: Location) -> Result<Option<String>, String> {
    if let Some(found) = read_repository(start) {
        return Ok(found.map(|repo| repo.answer(ask)));
    }
    let args = ["rev-parse", ask.flag()];
    let out = asked(
        start,
        &args,
        git_output(start, &args, GitOutput::Stdout),
        || walk(start, &Reach::longest_from_env()),
    )?;
    Ok(out.map(|out| String::from_utf8_lossy(&out).trim().to_owned()))
}

/// The repository `start` lies in, discovered once for every answer a caller needs of it: read
/// from git's files where they answer ([`read_repository`]), else asked of git in one `git
/// rev-parse` naming the top, the administrative and the common directory and the prefix. None
/// when `start` lies in no repository; a failure where git refuses inside one, as [`rev_parse`]
/// fails, or answers a path its four lines cannot carry (one holding a line break, or not spelled
/// in UTF-8).
pub fn discover(start: &Path) -> Result<Option<Repository>, String> {
    if let Some(found) = read_repository(start) {
        return Ok(found);
    }
    let asks = [
        Location::Toplevel,
        Location::GitDir,
        Location::CommonDir,
        Location::Prefix,
    ];
    let args: Vec<&str> = std::iter::once("rev-parse")
        .chain(asks.map(Location::flag))
        .collect();
    let Some(out) = asked(
        start,
        &args,
        git_output(start, &args, GitOutput::Stdout),
        || walk(start, &Reach::longest_from_env()),
    )?
    else {
        return Ok(None);
    };
    repository_asked(start, &out).map(Some).map_err(|why| {
        let said = String::from_utf8_lossy(&out);
        git_failure(&args, start, &format!("{why}: {said:?}")).0
    })
}

/// The repository git's answer `out` to [`discover`]'s `rev-parse` from `start` names: its four
/// lines, each path taken to its physical path and the common directory, which git names relative
/// to `start` where it names it relatively, joined onto `start`; and the main checkout its common
/// directory's configuration records ([`Format::main_checkout`]). A failure where the answer is
/// not four lines of UTF-8 naming directories that exist, or that configuration cannot be read.
fn repository_asked(start: &Path, out: &[u8]) -> Result<Repository, &'static str> {
    const NO_REPOSITORY: &str = "answered no repository";
    let text = std::str::from_utf8(out)
        .ok()
        .and_then(|text| text.strip_suffix('\n'))
        .ok_or(NO_REPOSITORY)?;
    let [toplevel, git_dir, common, prefix] = text.split('\n').collect::<Vec<_>>()[..] else {
        return Err(NO_REPOSITORY);
    };
    // An empty `start` is the current directory, as `git -C ""` takes it.
    let start = if start.as_os_str().is_empty() {
        Path::new(".")
    } else {
        start
    };
    let physical = |path: &Path| std::fs::canonicalize(path).map_err(|_| NO_REPOSITORY);
    let common_dir = physical(&start.join(common))?;
    let main = std::fs::read(common_dir.join("config"))
        .ok()
        .and_then(|text| Format::of(config_entries(&text)?))
        .and_then(|format| format.main_checkout(&common_dir))
        .ok_or("answered a repository whose recorded main checkout cannot be read")?;
    Ok(Repository {
        toplevel: physical(Path::new(toplevel))?,
        git_dir: physical(Path::new(git_dir))?,
        common_dir,
        prefix: PathBuf::from(prefix.trim_end_matches('/')),
        shown: CommonShown::Asked(common.to_owned()),
        main,
    })
}

/// What git's run of `args` from `start` answers ([`rev_parse`], [`discover`]): its output when
/// it ran and succeeded; when it refused or could not start, none where a walk up from `start`
/// (`met`) meets nothing, else a failure naming the command, `start` and git's cause.
fn asked(
    start: &Path,
    args: &[&str],
    ran: Result<Vec<u8>, Vec<u8>>,
    met: impl FnOnce() -> Met,
) -> Result<Option<Vec<u8>>, String> {
    match ran {
        Ok(out) => Ok(Some(out)),
        Err(_) if met() == Met::Nothing => Ok(None),
        Err(why) => Err(git_failure(args, start, String::from_utf8_lossy(&why).trim()).0),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::{git_ok, git_ok_with_identity, git_out, init_repo, run_git};

    /// The branch and commit of the checkout `start` lies in, read from git's files through `read`
    /// ([`Repository::head_with`]): None where they leave the repository ([`read_repository`]) or
    /// its head to git.
    fn read_head_with(
        start: &Path,
        read: &mut dyn FnMut(&Path) -> std::io::Result<String>,
    ) -> Option<(String, Option<String>)> {
        let repo = read_repository(start)??;
        repo.head_with(&repo.git_dir, read)
    }

    /// [`read_head_with`], each file read as it stands.
    fn read_head(start: &Path) -> Option<(String, Option<String>)> {
        read_head_with(start, &mut |path| std::fs::read_to_string(path))
    }

    /// git's own answer to `rev-parse <ask>` from `dir`, none when it refuses.
    fn git_says(dir: &Path, ask: Location) -> Option<String> {
        let out = run_git(dir, &["rev-parse", ask.flag()]);
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    /// git's own answer to which branch and commit `dir`'s checkout is at.
    fn git_head(dir: &Path) -> (String, Option<String>) {
        let answer = |args: &[&str]| {
            let out = run_git(dir, args);
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        };
        (
            answer(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_else(|| "HEAD".into()),
            answer(&["rev-parse", "--verify", "-q", "HEAD^{commit}"]),
        )
    }

    /// A main checkout `main` with a nested and an outside linked worktree, a branch with no
    /// commit, a detached worktree, packed refs, symlinks into each, a linked worktree whose
    /// administrative directory is gone (`pruned`) and a `.git` directory holding nothing
    /// (`headless`), and every other layout git finds a checkout in ([`layouts`]); every path
    /// the tables below name lies under the returned directory.
    fn shapes() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let main = root.join("main");
        std::fs::create_dir_all(main.join("a/b")).unwrap();
        init_repo(&main);
        std::fs::create_dir_all(main.join(".rigger/tmp")).unwrap();
        git_ok(
            &main,
            &["worktree", "add", "-q", ".rigger/tmp/wt", "-b", "nested"],
        );
        git_ok(
            &main,
            &["worktree", "add", "-q", "../linked", "-b", "outside"],
        );
        std::fs::create_dir_all(root.join("linked/s/t")).unwrap();
        git_ok(&main, &["worktree", "add", "-q", "--detach", "../detached"]);
        git_ok(&main, &["branch", "packed"]);
        git_ok(&main, &["pack-refs", "--all"]);
        git_ok(&main, &["worktree", "add", "-q", "../on-packed", "packed"]);
        git_ok(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "--orphan",
                "-b",
                "unborn",
                "../unborn",
            ],
        );
        git_ok(&main, &["tag", "outside"]);
        git_ok(
            &main,
            &["worktree", "add", "-q", "../pruned", "-b", "pruned"],
        );
        std::fs::remove_dir_all(main.join(".git/worktrees/pruned")).unwrap();
        std::fs::create_dir_all(root.join("headless/.git")).unwrap();
        std::fs::create_dir_all(root.join("plain")).unwrap();
        for (link, target) in [
            ("link-a", "main/a"),
            ("link-ls", "linked/s"),
            ("mainlink", "main"),
        ] {
            std::os::unix::fs::symlink(root.join(target), root.join(link)).unwrap();
        }
        layouts(root);
        dir
    }

    /// Beside `main` under `root`: a linked worktree whose `.git` names its administrative
    /// directory relatively (`relative`), a checkout whose git directory lies apart and a linked
    /// worktree of it (`apart`, `apart-linked`), a submodule and a linked worktree of it
    /// (`outer/sub`, `sub-linked`), a checkout whose `.git` is a symlink and a linked worktree of
    /// it (`dotlink`, `dotlink-linked`), a sparse checkout (`sparse`), a main checkout and a
    /// linked worktree under per-worktree configuration (`wtc`, `wtc-linked`), a bare
    /// repository's linked worktree (`bare-wt`), a configuration including a file and naming
    /// worktrees in a remote (`configured`), a work tree configured where its `.git` lies
    /// (`selfwt`), a `bare` value written quoted beside a comment (`quoted`), a format version
    /// written in hex and a `bare` value as a number with a unit (`suffixed`), a linked worktree
    /// of a `.git` configured bare (`baremain-wt`); and what git alone sets up: a work tree
    /// configured elsewhere (`elsewhere`), a `.git` configured bare (`baremain`), a configuration
    /// naming no format version (`noversion`), a format 0 naming an extension git ignores there
    /// (`ext0`), and a `.git` directory naming a common directory (`named`).
    fn layouts(root: &Path) {
        let main = root.join("main");
        git_ok(
            &main,
            &["worktree", "add", "-q", "../relative", "-b", "relative"],
        );
        std::fs::write(
            root.join("relative/.git"),
            "gitdir: ../main/.git/worktrees/relative\n",
        )
        .unwrap();
        git_ok(
            root,
            &["init", "-q", "--separate-git-dir", "apart.git", "apart"],
        );
        git_ok_with_identity(root.join("apart"), &["commit", "--allow-empty", "-qm", "i"]);
        std::fs::create_dir_all(root.join("apart/x")).unwrap();
        git_ok(
            root.join("apart"),
            &["worktree", "add", "-q", "../apart-linked", "-b", "apartl"],
        );
        let outer = root.join("outer");
        std::fs::create_dir_all(&outer).unwrap();
        init_repo(&outer);
        let add = ["submodule", "add", "-q", main.to_str().unwrap(), "sub"];
        git_ok(
            &outer,
            &[&["-c", "protocol.file.allow=always"][..], &add].concat(),
        );
        std::fs::create_dir_all(outer.join("sub/x")).unwrap();
        git_ok(
            outer.join("sub"),
            &["worktree", "add", "-q", "../../sub-linked", "-b", "subl"],
        );
        for repo in ["dotlink", "sparse", "wtc", "configured", "selfwt"] {
            std::fs::create_dir_all(root.join(repo).join("d")).unwrap();
            init_repo(root.join(repo));
        }
        std::fs::rename(root.join("dotlink/.git"), root.join("dotlink.git")).unwrap();
        std::os::unix::fs::symlink(root.join("dotlink.git"), root.join("dotlink/.git")).unwrap();
        git_ok(
            root.join("dotlink"),
            &["worktree", "add", "-q", "../dotlink-linked", "-b", "dotl"],
        );
        git_ok(
            root.join("sparse"),
            &["sparse-checkout", "set", "--no-cone", "/*"],
        );
        git_ok(
            root.join("wtc"),
            &["config", "extensions.worktreeConfig", "true"],
        );
        std::fs::write(
            root.join("wtc/.git/config.worktree"),
            "[core]\n\tbare = false\n",
        )
        .unwrap();
        git_ok(
            root.join("wtc"),
            &["worktree", "add", "-q", "../wtc-linked", "-b", "wtcl"],
        );
        git_ok(root, &["clone", "-q", "--bare", "main", "bare.git"]);
        git_ok(
            root.join("bare.git"),
            &["worktree", "add", "-q", "../bare-wt", "-b", "bwt"],
        );
        std::fs::write(
            root.join("configured/.git/extra"),
            "[core]\n\tautocrlf = false\n",
        )
        .unwrap();
        let configured = root.join("configured");
        git_ok(&configured, &["config", "include.path", "extra"]);
        git_ok(
            &configured,
            &["config", "remote.origin.url", "/up/worktrees/origin"],
        );
        let selfwt = root.join("selfwt");
        git_ok(
            &selfwt,
            &["config", "core.worktree", selfwt.to_str().unwrap()],
        );
        for (repo, key, value) in [
            ("elsewhere", "core.worktree", root.join("plain")),
            ("baremain", "core.bare", PathBuf::from("true")),
        ] {
            std::fs::create_dir_all(root.join(repo)).unwrap();
            init_repo(root.join(repo));
            git_ok(root.join(repo), &["config", key, value.to_str().unwrap()]);
        }
        git_ok(
            root.join("baremain"),
            &["worktree", "add", "-q", "../baremain-wt", "-b", "bmwt"],
        );
        for (repo, bare, version) in [
            ("quoted", "\"false\" ; quoted", "0"),
            ("suffixed", "0k", "0x0"),
        ] {
            std::fs::create_dir_all(root.join(repo)).unwrap();
            init_repo(root.join(repo));
            let config = root.join(repo).join(".git/config");
            let text = std::fs::read_to_string(&config).unwrap();
            let text = text
                .replace("bare = false", &format!("bare = {bare}"))
                .replace(
                    "repositoryformatversion = 0",
                    &format!("repositoryformatversion = {version}"),
                );
            std::fs::write(&config, text).unwrap();
        }
        for (repo, args) in [
            (
                "noversion",
                &["config", "--unset", "core.repositoryformatversion"][..],
            ),
            ("ext0", &["config", "extensions.unheardof", "true"][..]),
        ] {
            std::fs::create_dir_all(root.join(repo).join("d")).unwrap();
            init_repo(root.join(repo));
            git_ok(root.join(repo), args);
        }
        git_ok(&main, &["worktree", "add", "-q", "../named", "-b", "named"]);
        std::fs::remove_file(root.join("named/.git")).unwrap();
        std::fs::rename(main.join(".git/worktrees/named"), root.join("named/.git")).unwrap();
        let common = format!("{}\n", main.join(".git").display());
        std::fs::write(root.join("named/.git/commondir"), common).unwrap();
        std::fs::create_dir_all(root.join("named/d")).unwrap();
    }

    /// [`rev_parse`]'s answer from `start` for `ask`, checked against git's: git's own answer
    /// when it gives one, and when it refuses, a failure where a repository may lie (`inside`)
    /// and none where none does.
    fn assert_rev_parse_agrees(start: &Path, ask: Location, inside: bool, what: &str) {
        let asked = rev_parse(start, ask);
        match git_says(start, ask) {
            Some(answer) => assert_eq!(asked, Ok(Some(answer)), "{what}: {ask:?}"),
            None if inside => assert!(asked.is_err(), "{what}: {ask:?}: {asked:?}"),
            None => assert_eq!(asked, Ok(None), "{what}: {ask:?}"),
        }
    }

    /// [`discover`]'s repository from `start`, checked against git: every answer git gives for
    /// `asks` where it gives them all, and where it refuses one, a failure where a repository may
    /// lie (`inside`) and none where none does.
    fn assert_discovery_agrees(start: &Path, asks: &[Location], inside: bool, what: &str) {
        let discovered = discover(start);
        let said: Option<Vec<String>> = asks.iter().map(|ask| git_says(start, *ask)).collect();
        match said {
            Some(said) => {
                let repo = discovered
                    .unwrap()
                    .unwrap_or_else(|| panic!("{what}: none found"));
                for (ask, said) in asks.iter().zip(said) {
                    assert_eq!(repo.answer(*ask), said, "{what}: discovered {ask:?}");
                }
            }
            None if inside => assert!(discovered.is_err(), "{what}: {discovered:?}"),
            None => assert_eq!(discovered, Ok(None), "{what}"),
        }
    }

    /// Each row: a directory under the shapes, whether its answers are read from git's files
    /// rather than left to git, and whether a walk up from it meets a repository. Every answer
    /// read must be the one git gives, and where git refuses, a refusal inside a repository is a
    /// failure, never none.
    #[test]
    fn the_repository_read_from_files_is_the_one_git_names() {
        let dir = shapes();
        let cases: &[(&str, bool, bool)] = &[
            ("main", true, true),
            ("main/a", true, true),
            ("main/a/b", true, true),
            ("main/.rigger/tmp/wt", true, true),
            ("linked", true, true),
            ("linked/s/t", true, true),
            ("detached", true, true),
            ("on-packed", true, true),
            ("unborn", true, true),
            ("link-a", true, true),
            ("link-ls", true, true),
            ("mainlink/a", true, true),
            ("mainlink/.rigger/tmp/wt", true, true),
            ("relative", true, true),
            ("apart", true, true),
            ("apart/x", true, true),
            ("apart-linked", true, true),
            ("outer", true, true),
            ("outer/sub", true, true),
            ("outer/sub/x", true, true),
            ("sub-linked", true, true),
            ("dotlink", true, true),
            ("dotlink/d", true, true),
            ("dotlink-linked", true, true),
            ("sparse/d", true, true),
            ("wtc/d", true, true),
            ("wtc-linked", true, true),
            ("bare-wt", true, true),
            ("baremain-wt", true, true),
            ("configured/d", true, true),
            ("selfwt", true, true),
            ("selfwt/d", true, true),
            ("quoted", true, true),
            ("suffixed", true, true),
            ("main/.git", false, true),
            ("main/.git/refs", false, true),
            ("missing", true, false),
            ("pruned", false, true),
            ("headless", false, true),
            ("bare.git", false, true),
            ("elsewhere", false, true),
            ("baremain", false, true),
            ("noversion", false, true),
            ("noversion/d", false, true),
            ("ext0/d", false, true),
            ("named", false, true),
            ("named/d", false, true),
        ];
        let asks = [
            Location::Toplevel,
            Location::CommonDir,
            Location::Prefix,
            Location::GitDir,
        ];
        for (path, read, inside) in cases {
            let start = dir.path().join(path);
            let found = read_repository(&start);
            assert_eq!(found.is_some(), *read, "{path}: read from files");
            let met = walk(&start, &Reach::default());
            assert_eq!(met != Met::Nothing, *inside, "{path}: {met:?}");
            for ask in asks {
                let from_files = found.as_ref().map(|r| r.as_ref().map(|r| r.answer(ask)));
                if let Some(answer) = from_files {
                    assert_eq!(answer, git_says(&start, ask), "{path}: {ask:?}");
                }
                assert_rev_parse_agrees(&start, ask, *inside, path);
            }
            assert_discovery_agrees(&start, &asks, *inside, path);
        }
        // An empty start is the directory this process runs in, which git takes `-C ""` to mean.
        let (here, empty) = (
            read_repository(Path::new(".")),
            read_repository(Path::new("")),
        );
        for ask in asks {
            let answer = |found: &Option<Option<Repository>>| {
                found.as_ref().map(|r| r.as_ref().map(|r| r.answer(ask)))
            };
            assert_eq!(answer(&empty), answer(&here), "an empty start: {ask:?}");
            assert_eq!(
                rev_parse(Path::new(""), ask),
                rev_parse(Path::new("."), ask),
                "an empty start: {ask:?}"
            );
            let inside = walk(Path::new(""), &Reach::default()) != Met::Nothing;
            assert_rev_parse_agrees(Path::new(""), ask, inside, "an empty start");
        }
        let outside = tempfile::tempdir().unwrap();
        if git_says(outside.path(), Location::Toplevel).is_none() {
            assert_eq!(read_repository(outside.path()), Some(None));
            for ask in asks {
                assert_eq!(rev_parse(outside.path(), ask), Ok(None), "outside: {ask:?}");
            }
        }
    }

    /// Each row: what git's run answered, what a walk up from the directory met, and what that
    /// answers: git's output where it ran, and where it refused or could not start, none only
    /// where the walk met nothing, else a failure naming the command, the directory and the cause.
    #[test]
    fn a_refusal_is_none_only_where_the_walk_meets_nothing() {
        let start = Path::new("/r");
        let args = ["rev-parse", "--git-common-dir"];
        let linked = Met::Dot {
            toplevel: start.to_path_buf(),
            prefix: PathBuf::new(),
            file: true,
        };
        let failure = |why: &str| Err(format!("git rev-parse --git-common-dir in /r: {why}"));
        type Ran = Result<Vec<u8>, Vec<u8>>;
        type Answer = Result<Option<Vec<u8>>, String>;
        let cases: [(Ran, Met, Answer); 5] = [
            (
                Ok(b".git\n".to_vec()),
                Met::Unread,
                Ok(Some(b".git\n".to_vec())),
            ),
            (
                Err(b"fatal: not a git repository\n".to_vec()),
                Met::Nothing,
                Ok(None),
            ),
            (Err(b"could not start".to_vec()), Met::Nothing, Ok(None)),
            (
                Err(b"fatal: not a git repository\n".to_vec()),
                linked,
                failure("fatal: not a git repository"),
            ),
            (
                Err(b"could not start".to_vec()),
                Met::Unread,
                failure("could not start"),
            ),
        ];
        for (ran, met, answer) in cases {
            assert_eq!(asked(start, &args, ran, || met), answer);
        }
    }

    /// Each row: a directory under the shapes, the entries of `GIT_CEILING_DIRECTORIES` (each
    /// under the shapes, but for an empty entry and a relative one), and whether a repository is
    /// found from it under them. A walk up from the directory meets one exactly when git does:
    /// git never enters a ceiling that is a proper ancestor, resolves an entry to its physical
    /// path up to the first empty entry, and ignores a relative entry; and the repository read
    /// from the files under them is the one git names.
    #[test]
    fn the_walk_stops_at_the_ceilings_git_stops_at() {
        let dir = shapes();
        let at = |path: &str| dir.path().join(path).into_os_string();
        let cases: Vec<(&str, Vec<OsString>, bool)> = vec![
            ("main/a/b", vec![at("main")], false),
            ("main/a/b", vec![at("main/a")], false),
            ("linked/s/t", vec![at("linked/s")], false),
            ("main", vec![at("main")], true),
            ("main/a", vec![at("")], true),
            ("main/a", vec![at("mainlink")], false),
            ("main/a", vec![OsString::new(), at("mainlink")], true),
            ("main/a/b", vec![OsString::from("main")], true),
        ];
        for (path, entries, found) in cases {
            let start = dir.path().join(path);
            let value = std::env::join_paths(&entries).unwrap();
            let out = crate::subprocess::git_in(&start)
                .env(CEILINGS, &value)
                .args(["rev-parse", Location::Toplevel.flag()])
                .output()
                .unwrap();
            let git_finds = out
                .status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned());
            let reach = Reach::of(Some(value.clone()), false);
            let met = walk(&start, &reach);
            assert_eq!(
                (met != Met::Nothing, git_finds.is_some()),
                (found, found),
                "{path} under {value:?}: {met:?}"
            );
            let read = read_repository_within(&start, &reach)
                .map(|found| found.map(|repo| repo.answer(Location::Toplevel)));
            assert_eq!(read, Some(git_finds), "{path} under {value:?}: read");
        }
    }

    /// Each row: a checkout, and whether its branch and commit are read from git's files. Every
    /// one read is the one git names; `outside`, which a tag also spells, is left to git.
    #[test]
    fn the_head_read_from_files_is_the_one_git_names() {
        let dir = shapes();
        let cases: &[(&str, bool)] = &[
            ("main", true),
            ("main/a", true),
            ("main/.rigger/tmp/wt", true),
            ("linked/s/t", false),
            ("detached", true),
            ("on-packed", true),
            ("unborn", true),
            ("relative", true),
            ("apart/x", true),
            ("outer/sub/x", true),
            ("dotlink/d", true),
            ("sparse/d", true),
            ("wtc-linked", true),
            ("bare-wt", true),
            ("configured/d", true),
            ("selfwt/d", true),
            ("main/.git", false),
        ];
        for (path, read) in cases {
            let start = dir.path().join(path);
            let head = read_head(&start);
            assert_eq!(head.is_some(), *read, "{path}: read from files");
            if let Some(head) = head {
                assert_eq!(head, git_head(&start), "{path}");
            }
        }
        let unborn = read_head(&dir.path().join("unborn")).unwrap();
        assert_eq!(unborn, ("unborn".to_owned(), None));
        let packed = read_head(&dir.path().join("on-packed")).unwrap();
        assert_eq!(
            packed.1,
            Some(git_out(dir.path().join("main"), &["rev-parse", "packed"]))
        );
        // An empty start is the directory this process runs in, which git takes `-C ""` to mean.
        let empty = read_head(Path::new(""));
        assert_eq!(empty, read_head(Path::new(".")), "an empty start");
        if let Some(head) = empty {
            assert_eq!(head, git_head(Path::new("")), "an empty start");
        }
    }

    /// The main checkout git records for the repository `start` lies in, from git's own answers
    /// asked from the physical path of its common directory (`git rev-parse --git-common-dir`):
    /// none where `git config --type=bool --get core.bare` answers `true`, the path `git config
    /// --get core.worktree` answers, taken from the common directory, where it answers one, the
    /// common directory's parent where it is named `.git`, and none anywhere else.
    fn git_records_main(start: &Path) -> Option<PathBuf> {
        let common = git_says(start, Location::CommonDir).unwrap();
        let common = std::fs::canonicalize(start.join(common)).unwrap();
        let config = |args: &[&str]| {
            let out = run_git(&common, &[&["config"][..], args].concat());
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        };
        if config(&["--type=bool", "--get", "core.bare"]).as_deref() == Some("true") {
            return None;
        }
        if let Some(worktree) = config(&["--get", "core.worktree"]) {
            return Some(std::fs::canonicalize(common.join(worktree)).unwrap());
        }
        (common.file_name() == Some(".git".as_ref())).then(|| common.parent().unwrap().into())
    }

    /// Each row: a directory under the shapes, the root its repository's store lies at, and the
    /// directory whose `HEAD` names the head that store is read at. The repository discovered
    /// from it, from git's files or of git, names that root and that head: the main checkout's
    /// where git records one ([`git_records_main`]), whichever worktree it is discovered from,
    /// and otherwise the worktree's own top and head.
    #[test]
    fn the_main_root_and_head_are_the_main_checkout_git_records() {
        let dir = shapes();
        let cases: &[(&str, &str, &str)] = &[
            ("main", "main", "main"),
            ("main/a/b", "main", "main"),
            ("main/.rigger/tmp/wt", "main", "main"),
            ("linked/s/t", "main", "main"),
            ("detached", "main", "main"),
            ("relative", "main", "main"),
            ("named/d", "main", "main"),
            ("apart/x", "apart", "apart"),
            ("apart-linked", "apart-linked", "apart-linked"),
            ("outer/sub/x", "outer/sub", "outer/sub"),
            ("sub-linked", "outer/sub", "outer/sub"),
            ("dotlink/d", "dotlink", "dotlink"),
            ("dotlink-linked", "dotlink-linked", "dotlink-linked"),
            ("wtc-linked", "wtc", "wtc"),
            ("bare-wt", "bare-wt", "bare-wt"),
            ("baremain-wt", "baremain-wt", "baremain-wt"),
            ("noversion/d", "noversion", "noversion"),
            ("ext0/d", "ext0", "ext0"),
            ("elsewhere", "plain", "elsewhere"),
        ];
        for (path, root, head_of) in cases {
            let start = dir.path().join(path);
            let repo = discover(&start).unwrap().unwrap();
            let root = std::fs::canonicalize(dir.path().join(root)).unwrap();
            assert_eq!(repo.main_root(), root, "{path}: the main root");
            let top = git_says(&start, Location::Toplevel).unwrap();
            let recorded = git_records_main(&start).unwrap_or_else(|| PathBuf::from(top));
            assert_eq!(
                root, recorded,
                "{path}: the root is the main checkout git records"
            );
            let head = git_head(&dir.path().join(head_of));
            assert_eq!(repo.main_head(), Some(head), "{path}: the main head");
        }
    }

    /// git's own reading of the configuration file `text`: each entry's name and value, none for
    /// a key written without one; None when git refuses the file.
    fn git_reads(text: &str) -> Option<Vec<(String, Option<Vec<u8>>)>> {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config");
        std::fs::write(&file, text).unwrap();
        let out = run_git(dir.path(), &["config", "-z", "--list", "-f", "config"]);
        let entries = out
            .stdout
            .split(|&c| c == 0)
            .filter(|entry| !entry.is_empty());
        let entries = entries.map(|entry| match entry.iter().position(|&c| c == b'\n') {
            Some(at) => (
                String::from_utf8_lossy(&entry[..at]).into_owned(),
                Some(entry[at + 1..].to_vec()),
            ),
            None => (String::from_utf8_lossy(entry).into_owned(), None),
        });
        out.status.success().then(|| entries.collect())
    }

    /// Each row: a configuration file's text that every git release from 2.43 on reads alike.
    /// Its entries are read as the installed git reads them, and a file git refuses is refused.
    #[test]
    fn a_configuration_is_read_as_git_reads_it() {
        let cases = [
            "[core]\n\trepositoryformatversion = 0\n\tbare = false\n",
            "[Core] Bare\n[core]bare=1\n",
            "k = v\n",
            "[ \"x\"]\nk=1\n",
            "[remote \"o\\\"r]i\\gin\"]\n\turl = a ; b\n\tfetch = x # y\n",
            "[a.B]\nk = \"t\\tx\" y  z \nl = \"\" m  n\n",
            "[s]\nk = a\t \t# c\nl = b \t\r\n",
            "[core]\r\n\tworktree = /a\\\n/b\r\n",
            "\u{feff}[core]\nbare = \"false\"\n",
            "# c\n; d\n\n[core] ; e\n  bare = no\n",
            "[]\nk=1\n",
            "[core]\nbare = \"un\n",
            "[core]\nbare = x\\q\n",
            "[core]\n1k = 2\n",
            "[core]\nk # c\n",
            "[core \"x\n\"]\n",
            "[core x]\n",
            "[core]\n\tbare = 1\\",
        ];
        for text in cases {
            assert_eq!(config_entries(text.as_bytes()), git_reads(text), "{text:?}");
        }
    }

    /// Each row: a configuration file's text that git releases read differently, and its entries
    /// as the newest release reads them, which this reader follows. From git 2.45.0 white space
    /// inside an unquoted value (a tab or a lone carriage return, a run of them before a line
    /// continued) is kept as written; an older git reads each such character as a space.
    #[test]
    fn a_configuration_read_differently_by_release_is_read_as_the_newest_git_reads_it() {
        type Entries = &'static [(&'static str, &'static [u8])];
        let cases: &[(&str, Entries)] = &[
            (
                "[a.B]\nk = \"t\\tx\" y  z \nl = \"\" m\t n\n",
                &[("a.b.k", b"t\tx y  z"), ("a.b.l", b"m\t n")],
            ),
            ("[s]\nk = a\rb\n", &[("s.k", b"a\rb")]),
            ("[s]\nk = a\t\\\n\tb\n", &[("s.k", b"a\t\tb")]),
        ];
        for (text, entries) in cases {
            let entries = entries
                .iter()
                .map(|(name, value)| ((*name).to_owned(), Some(value.to_vec())))
                .collect();
            assert_eq!(config_entries(text.as_bytes()), Some(entries), "{text:?}");
        }
    }

    /// Each row: a configuration value that every git release from 2.43 on reads alike as a
    /// boolean, and how git reads it, none where git refuses it: a word, or an integer in any base
    /// git reads, with any unit, within git's range at either end. The installed git reads it so.
    #[test]
    fn a_boolean_is_read_as_git_reads_it() {
        let cases: &[(Option<&str>, Option<bool>)] = &[
            (None, Some(true)),
            (Some(""), Some(false)),
            (Some("TRUE"), Some(true)),
            (Some("Off"), Some(false)),
            (Some("yes"), Some(true)),
            (Some("0"), Some(false)),
            (Some("-2"), Some(true)),
            (Some("+0"), Some(false)),
            (Some("0k"), Some(false)),
            (Some("1K"), Some(true)),
            (Some("0x1"), Some(true)),
            (Some("-0x10"), Some(true)),
            (Some("010"), Some(true)),
            (Some("08"), None),
            (Some("0x"), None),
            (Some("1 k"), None),
            (Some("2147483647"), Some(true)),
            (Some("-2147483647"), Some(true)),
            (Some("2147483648"), None),
            (Some("-2147483649"), None),
            (Some("2097151k"), Some(true)),
            (Some("-2097151k"), Some(true)),
            (Some("2097152k"), None),
            (Some("-2097153k"), None),
            (Some("2047m"), Some(true)),
            (Some("-2047m"), Some(true)),
            (Some("2048m"), None),
            (Some("-2049m"), None),
            (Some("1g"), Some(true)),
            (Some("-1g"), Some(true)),
            (Some("2g"), None),
            (Some("-3g"), None),
            (Some("+"), None),
            (Some("maybe"), None),
        ];
        for &(value, read) in cases {
            assert_eq!(git_bool(value.map(str::as_bytes)), read, "{value:?}");
            let dir = tempfile::tempdir().unwrap();
            let line = value.map_or("k".to_owned(), |value| format!("k = {value}"));
            std::fs::write(dir.path().join("config"), format!("[s]\n{line}\n")).unwrap();
            let out = run_git(dir.path(), &["config", "-f", "config", "--bool", "s.k"]);
            let git = out
                .status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim() == "true");
            assert_eq!(git, read, "git: {value:?}");
        }
    }

    /// Each row: a value git releases read differently as an integer, and so as a boolean, and
    /// the integer the newest release reads, which this reader follows. From git 2.50.0 an
    /// integer reaches down to `i32::MIN`, however it is written; an older git refuses it.
    #[test]
    fn a_boolean_read_differently_by_release_is_read_as_the_newest_git_reads_it() {
        let cases: &[(&str, i32)] = &[
            ("-2147483648", i32::MIN),
            ("-0x80000000", i32::MIN),
            ("-2097152k", i32::MIN),
            ("-2048M", i32::MIN),
            ("-2g", i32::MIN),
        ];
        for &(value, read) in cases {
            assert_eq!(git_int(value.as_bytes()), Some(read), "{value:?}");
            assert_eq!(
                git_bool(Some(value.as_bytes())),
                Some(read != 0),
                "{value:?}"
            );
        }
    }

    /// Where a `git pack-refs --all`, which packs every loose ref and deletes it, lands among
    /// [`read_head_with`]'s reads.
    #[derive(Clone, Copy)]
    enum Pack {
        /// Just before the branch's loose ref is read.
        BeforeTheLooseBranch,
        /// Just after `packed-refs` is read.
        AfterPackedRefs,
    }

    /// A checkout's head read while a `git pack-refs --all` lands at `pack`, with a tag spelling
    /// its branch when `tagged`. A ref is read loose first and only then from a `packed-refs`
    /// read after it, as git reads it, so a pack between the reads hides no ref: the branch keeps
    /// the commit git names, and a tag spelling it still leaves the read to git.
    fn a_pack_between_the_reads_hides_no_ref(pack: Pack, tagged: bool) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let (branch, commit) = git_head(repo);
        if tagged {
            git_ok(repo, &["tag", &branch]);
        }
        let loose = Path::new("refs/heads").join(&branch);
        let mut read = |path: &Path| {
            if matches!(pack, Pack::BeforeTheLooseBranch) && path.ends_with(&loose) {
                git_ok(repo, &["pack-refs", "--all"]);
            }
            let text = std::fs::read_to_string(path);
            if matches!(pack, Pack::AfterPackedRefs) && path.ends_with("packed-refs") {
                git_ok(repo, &["pack-refs", "--all"]);
            }
            text
        };
        let expected = (!tagged).then_some((branch, commit));
        assert_eq!(read_head_with(repo, &mut read), expected);
    }

    crate::test_cases! {
        a_pack_before_the_loose_branch_is_read_leaves_it_at_its_commit:
            a_pack_between_the_reads_hides_no_ref(Pack::BeforeTheLooseBranch, false);
        a_pack_after_packed_refs_is_read_still_sees_a_tag_spelling_the_branch:
            a_pack_between_the_reads_hides_no_ref(Pack::AfterPackedRefs, true);
    }
}
