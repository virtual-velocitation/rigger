//! THE PROCESS-SPAWN INTERFACE: the one place production code constructs a [`Command`].
//!
//! Every process this crate starts - git, the gate shell, the agent CLI, the dash child, a declared
//! extractor - is built here and handed back for the caller to add its own arguments and run. One
//! seam means one place owning how a child is launched: the program a name resolves to on
//! Windows ([`command`], through [`find_executable`]), the working-directory rule every caller
//! shares ([`command_in`]), the host's shell given a project's command line whole
//! ([`shell_command`]), the one open file a child inherits across its exec ([`inherit_on_exec`])
//! and the `git -C <dir>` shape nearly every git call takes, its repository found from `dir`
//! alone ([`git_in`]). Beyond those rules it adds no policy of its own: the caller still picks
//! the arguments, the environment, the stdio and how the child is waited on, and a child is only
//! ever ended through its own spawning handle.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh command for `program`, inheriting this process's environment, working directory
/// and stdio exactly as [`Command::new`] does. On Windows a program name holding no path
/// separator is spawned at the path [`find_executable`] resolves it to over
/// [`SearchPath::ambient`], and as given when that resolves nothing; elsewhere `program` goes
/// to [`Command::new`] unchanged.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let program = program.as_ref();
    let resolved = if cfg!(windows) {
        program
            .to_str()
            .and_then(|name| find_executable(&SearchPath::ambient(), name))
    } else {
        None
    };
    match resolved {
        Some(path) => Command::new(path),
        None => Command::new(program),
    }
}

/// Where a program name is looked up: `dirs`, a `PATH`-style directory list joined by the
/// platform's separator, and `pathext`, a `PATHEXT`-style list of `;`-separated extensions,
/// which only a Windows lookup reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchPath {
    pub dirs: OsString,
    pub pathext: String,
}

impl SearchPath {
    /// This process's own `PATH` and `PATHEXT`, an unset one reading as empty: the one read of
    /// the ambient search path, which a lookup that is handed no search path of its own goes
    /// through.
    pub fn ambient() -> Self {
        Self {
            dirs: std::env::var_os("PATH").unwrap_or_default(),
            pathext: std::env::var("PATHEXT").unwrap_or_default(),
        }
    }
}

/// The first executable regular file named `bin` inside a directory of `search`'s `dirs` (split
/// by [`std::env::split_paths`]), checked in listed order. An empty or relative component is
/// skipped, so every path answered is absolute. Each directory is tried with each of
/// [`lookup_names`] of `bin` over `search`'s `pathext` in turn, under this host's rules: `bin`
/// is a file name, so an empty one, or one holding a path separator (`/`, and on Windows `\`
/// too), names no file in a directory and finds nothing; otherwise it is tried alone, except on
/// Windows, where it is tried instead with each of [`pathext_candidates`]: a `bin` with no
/// extension is never tried bare there, and an empty `pathext` leaves such a `bin` no candidate
/// at all. Symlinks are followed (through [`is_executable_file`]'s `std::fs::metadata`), and a
/// found symlink is answered at its own path.
pub fn find_executable(search: &SearchPath, bin: &str) -> Option<PathBuf> {
    let names = lookup_names(bin, &search.pathext, cfg!(windows));
    std::env::split_paths(&search.dirs)
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| is_executable_file(candidate))
}

/// The file names a lookup of `bin` tries in each directory, under Windows' rules when `windows`
/// holds and unix's otherwise, so every host builds and tests both: none when `bin` is empty or
/// holds a path separator (`/`, and under Windows' rules `\` too), since such a `bin` names no
/// file in a directory; under Windows' rules [`pathext_candidates`] of `bin` over `pathext`; else
/// `bin` alone, `pathext` unread.
fn lookup_names(bin: &str, pathext: &str, windows: bool) -> Vec<String> {
    if bin.is_empty() || bin.contains(|c| c == '/' || (windows && c == '\\')) {
        return Vec::new();
    }
    if windows {
        pathext_candidates(bin, pathext)
    } else {
        vec![bin.to_string()]
    }
}

/// The file names Windows tries for the program `name` over `pathext`, a `PATHEXT`-style list
/// of `;`-separated extensions: a name with no extension is the name with each listed extension
/// appended, in order, an empty entry skipped and the bare name never among them; a name
/// carrying an extension is the name alone. A plain string rule, so every host builds and tests
/// it.
pub fn pathext_candidates(name: &str, pathext: &str) -> Vec<String> {
    if Path::new(name).extension().is_some() {
        return vec![name.to_string()];
    }
    pathext
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{name}{extension}"))
        .collect()
}

/// Whether `path` is a regular file with an execute permission bit set, following symlinks.
#[cfg(unix)]
pub fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Whether `path` is a regular file, following symlinks: a host without unix permission bits
/// reads every regular file as executable.
#[cfg(not(unix))]
pub fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// The directory a child given `dir` runs in: `dir` itself, or this process's own working
/// directory, `.`, when `dir` is empty (the caller-supplied "no directory" convention).
pub fn session_dir(dir: &str) -> &Path {
    Path::new(if dir.is_empty() { "." } else { dir })
}

/// A fresh command for `program` that runs in [`session_dir`] of `dir`.
pub fn command_in(program: impl AsRef<OsStr>, dir: &str) -> Command {
    let mut cmd = command(program);
    cmd.current_dir(session_dir(dir));
    cmd
}

/// The one argument `cmd` reads `run` from on Windows: its switches - `/D`, no AutoRun command;
/// `/S`, the outer quotes stripped and the rest kept as written; `/C`, run it, then end - and
/// `run` in one pair of quotes. A plain string, so every host builds and tests it.
pub fn cmd_argument(run: &str) -> String {
    format!("/D /S /C \"{run}\"")
}

// The one spelling of the host's shell: on Windows `cmd`, handed [`cmd_argument`] as one raw
// argument so no quoting of the host's own reaches the run; on every other OS `sh -c <run>`.
std::cfg_select! {
    windows => {
        fn host_shell(run: &str) -> Command {
            use std::os::windows::process::CommandExt;
            let mut cmd = command("cmd");
            cmd.raw_arg(cmd_argument(run));
            cmd
        }
    }
    _ => {
        fn host_shell(run: &str) -> Command {
            let mut cmd = command("sh");
            cmd.arg("-c").arg(run);
            cmd
        }
    }
}

/// The host's shell given the command line `run` whole, as a project writes one (a declared
/// extractor's `run`, say), running in `dir`: `cmd` and [`cmd_argument`] on Windows, `sh -c
/// <run>` elsewhere. The caller adds the stdio and the environment.
pub fn shell_command(run: &str, dir: &Path) -> Command {
    let mut cmd = host_shell(run);
    cmd.current_dir(dir);
    cmd
}

/// Hands the open `file` to `cmd`'s child across its exec, so the child and every process it
/// starts share the open file, and any lock taken on it, for as long as one of them keeps it.
/// On unix a hook run in the child between fork and exec clears the descriptor's close-on-exec
/// flag there alone: the flag stays set in this process, so a child another thread spawns
/// meanwhile never inherits the file. Elsewhere it does nothing. `file` must stay open until
/// `cmd` has spawned.
#[cfg(unix)]
pub fn inherit_on_exec(cmd: &mut Command, file: &std::fs::File) {
    use std::os::fd::{AsRawFd, BorrowedFd};
    use std::os::unix::process::CommandExt;
    let raw = file.as_raw_fd();
    // SAFETY: the hook runs in the forked child before exec and makes two `fcntl` calls, which
    // allocate nothing, take no lock and are async-signal-safe. `raw` is open in that child:
    // it is forked while the caller still holds `file` open, and a fork copies every descriptor.
    unsafe {
        cmd.pre_exec(move || {
            let fd = BorrowedFd::borrow_raw(raw);
            let flags = rustix::io::fcntl_getfd(fd)?;
            rustix::io::fcntl_setfd(fd, flags.difference(rustix::io::FdFlags::CLOEXEC))?;
            Ok(())
        });
    }
}

/// Hands the open `file` to `cmd`'s child across its exec on unix; on this host it does
/// nothing, and the file stays this process's alone.
#[cfg(not(unix))]
pub fn inherit_on_exec(_cmd: &mut Command, _file: &std::fs::File) {}

/// The variables through which an environment names the repository git operates on, apart from
/// the directory git starts in: the repository itself, its work tree, its index, its object
/// databases and its common directory. A git hook runs with some of them set.
const GIT_REPOSITORY_VARIABLES: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

/// `git -C <dir>`: a git command anchored at `dir` rather than the process's working directory,
/// with none of [`GIT_REPOSITORY_VARIABLES`] passed on, so the repository git finds from `dir` is
/// the one it operates on whatever repository this process's environment names (the convention
/// git documents for a hook that operates on another repository). A caller may still set one of
/// them on the command it is handed.
pub fn git_in(dir: impl AsRef<OsStr>) -> Command {
    let mut cmd = command("git");
    cmd.arg("-C").arg(dir);
    for variable in GIT_REPOSITORY_VARIABLES {
        cmd.env_remove(variable);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd_of(cmd: &Command) -> Option<&std::path::Path> {
        cmd.get_current_dir()
    }

    crate::test_cases! {
        /// An empty directory runs the child in this process's own working directory, `.`.
        command_in_an_empty_dir_runs_the_child_in_this_processs_own:
            assert_eq!(cwd_of(&command_in("sh", "")), Some(std::path::Path::new(".")));
        /// A non-empty directory becomes the child's working directory.
        command_in_a_dir_runs_the_child_there:
            assert_eq!(cwd_of(&command_in("sh", "/x")), Some(std::path::Path::new("/x")));
    }

    /// `git_in` is exactly `git -C <dir>` with every variable naming a repository removed from
    /// the child's environment, and nothing else added.
    #[test]
    fn git_in_anchors_git_at_the_directory() {
        let cmd = git_in("/repo");
        assert_eq!(cmd.get_program(), "git");
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("-C"), OsStr::new("/repo")]);
        assert_eq!(cwd_of(&cmd), None);
        let envs: Vec<(&OsStr, Option<&OsStr>)> = cmd.get_envs().collect();
        let mut removed: Vec<(&OsStr, Option<&OsStr>)> = GIT_REPOSITORY_VARIABLES
            .iter()
            .map(|variable| (OsStr::new(variable), None))
            .collect();
        removed.sort();
        assert_eq!(envs, removed);
    }

    /// `cmd_argument` is cmd's switches - no AutoRun, the outer quotes stripped, run then end -
    /// and the run in one pair of quotes, kept as written whatever it holds, on every host.
    #[test]
    fn cmd_argument_quotes_the_run_after_cmds_switches() {
        let rows = [
            ("cargo test", r#"/D /S /C "cargo test""#),
            (r#"echo "a b" > out"#, r#"/D /S /C "echo "a b" > out""#),
            ("", r#"/D /S /C """#),
        ];
        for (run, argument) in rows {
            assert_eq!(cmd_argument(run), argument, "{run:?}");
        }
    }

    /// `shell_command` is the host's shell given the run whole - `cmd` and its one raw argument
    /// on Windows, `sh -c <run>` elsewhere - in the directory, and nothing else.
    #[test]
    fn shell_command_is_the_hosts_shell_given_the_run_in_the_directory() {
        let run = r#"cargo test "a b""#;
        let dir = std::path::Path::new("/project");
        let cmd = shell_command(run, dir);
        let (program, args) = if cfg!(windows) {
            ("cmd", vec![cmd_argument(run)])
        } else {
            ("sh", vec!["-c".to_string(), run.to_string()])
        };
        assert_eq!(
            (
                cmd.get_program(),
                cmd.get_args().collect::<Vec<_>>(),
                cwd_of(&cmd),
                cmd.get_envs().count()
            ),
            (
                OsStr::new(program),
                args.iter().map(OsStr::new).collect::<Vec<_>>(),
                Some(dir),
                0
            )
        );
    }

    /// On unix, the shell reads the run as a script, so a compound run's every command runs, and
    /// it starts in the directory given.
    #[cfg(unix)]
    #[test]
    fn a_shell_command_runs_its_whole_run_in_its_directory() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let rows = [
            ("pwd -P", dir.to_path_buf()),
            ("cd src && pwd -P", dir.join("src")),
        ];
        for (run, ran_in) in rows {
            let out = shell_command(run, dir).output().expect("run the shell");
            let expected = ran_in.canonicalize().expect("the directory exists");
            assert_eq!(
                (out.status.success(), String::from_utf8_lossy(&out.stdout)),
                (true, format!("{}\n", expected.display()).into()),
                "{run}"
            );
        }
    }

    /// `pathext_candidates` is the Windows by-name rule on every host: a name with no extension
    /// becomes the name with each `PATHEXT` extension, in order, an empty entry skipped and the
    /// bare name never among them; a name carrying an extension is the name alone.
    #[test]
    fn pathext_candidates_extend_a_bare_name_by_each_listed_extension() {
        let rows: [(&str, &str, &[&str]); 5] = [
            ("git", ".COM;.EXE", &["git.COM", "git.EXE"]),
            ("git.exe", ".COM;.EXE", &["git.exe"]),
            ("git", ";.COM;;.EXE;", &["git.COM", "git.EXE"]),
            ("git", "", &[]),
            ("git.exe", "", &["git.exe"]),
        ];
        for (name, pathext, candidates) in rows {
            assert_eq!(
                pathext_candidates(name, pathext),
                candidates,
                "{name:?} over {pathext:?}"
            );
        }
    }

    /// `lookup_names` is each host's by-name rule on every host: an empty name, or one holding a
    /// path separator (`/`, and `\` under Windows' rules alone), has no name to try; under
    /// Windows' rules a name is each of its `PATHEXT` candidates, the bare name never among them;
    /// otherwise it is the name alone, `PATHEXT` unread.
    #[test]
    fn lookup_names_follow_each_hosts_separator_and_extension_rules() {
        let rows: [(&str, &str, bool, &[&str]); 11] = [
            ("git", ".COM;.EXE", false, &["git"]),
            ("git", ".COM;.EXE", true, &["git.COM", "git.EXE"]),
            ("git", "", false, &["git"]),
            ("git", "", true, &[]),
            ("git.exe", ".COM;.EXE", true, &["git.exe"]),
            ("bin/git", ".EXE", false, &[]),
            ("bin/git", ".EXE", true, &[]),
            (r"bin\git", ".EXE", false, &[r"bin\git"]),
            (r"bin\git", ".EXE", true, &[]),
            ("", ".EXE", false, &[]),
            ("", ".EXE", true, &[]),
        ];
        for (bin, pathext, windows, names) in rows {
            assert_eq!(
                lookup_names(bin, pathext, windows),
                names,
                "{bin:?} over {pathext:?}, Windows' rules: {windows}"
            );
        }
    }

    /// Which command a row of the inheritance table marks with [`inherit_on_exec`].
    #[cfg(unix)]
    #[derive(Debug, Clone, Copy)]
    enum Marked {
        /// The shell that writes through the file's descriptor number.
        Writer,
        /// No command.
        Neither,
        /// Another command, run to its end before the writer: the mark is its child's alone.
        Other,
    }

    /// On unix, `inherit_on_exec` hands an open file to the command it marks, and only to it:
    /// the marked shell writes through the file's descriptor number, and an unmarked one finds
    /// that descriptor closed, since every file this process opens closes on exec - even after
    /// another command was marked with the same file and has run, since the mark clears the
    /// flag in that command's child and never in this process.
    #[cfg(unix)]
    #[test]
    fn inherit_on_exec_hands_the_file_to_the_marked_command_only() {
        use std::os::fd::AsRawFd;
        for (marked, outcome) in [
            (Marked::Writer, (true, "hi")),
            (Marked::Neither, (false, "")),
            (Marked::Other, (false, "")),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("held");
            let file = std::fs::File::create(&path).expect("create the held file");
            let mut cmd = shell_command(
                &format!("printf hi >> /dev/fd/{}", file.as_raw_fd()),
                dir.path(),
            );
            match marked {
                Marked::Writer => inherit_on_exec(&mut cmd, &file),
                Marked::Neither => {}
                Marked::Other => {
                    let mut other = shell_command("true", dir.path());
                    inherit_on_exec(&mut other, &file);
                    let status = other.status().expect("run the other shell");
                    assert_eq!(status.code(), Some(0), "the other shell ran");
                }
            }
            let ran = cmd.output().expect("run the shell").status.success();
            let written = std::fs::read_to_string(&path).expect("read the held file");
            assert_eq!((ran, written.as_str()), outcome, "marked: {marked:?}");
        }
    }
}
