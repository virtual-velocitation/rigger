//! THE DETACHED START: the host's facility for starting a process that outlives the one that
//! started it, through the native implementation of the OS the binary is built for. A rigger
//! invocation that finds a run whose driver died starts the driver anew through it, in the
//! background, and ends at once; the started driver must not end with the invocation, its
//! terminal or its session's hook.
//!
//! The child is built through the one process-spawn interface ([`crate::subprocess::command`]),
//! its stdin null and its stdout and stderr both written to one output file the starter names,
//! and is never signalled: its handle is waited on by a thread of the starting process, so a
//! child that ends first is reaped there, and one that outlives the starter is reaped by the OS
//! like any orphan.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The environment knob that selects how a rigger invocation relaunches a run whose driver died:
/// unset or empty, through [`start`]; `record:<path>`, by appending one `relaunch <binary> <args>`
/// line to `<path>` and starting nothing, the recording a test reads; any other value names no
/// launcher.
pub const RELAUNCH_ENV: &str = "RIGGER_RELAUNCH";

// The one spelling of the OS set: each OS with a native way to detach a child binds `detach`,
// which readies a command so the child it starts is in no group the starter's terminal or session
// reaches, and every other OS binds the `_` arm's, which refuses.
std::cfg_select! {
    unix => {
        /// In a process group of its own (`setpgid(0, 0)` in the child), so the starter's
        /// terminal's interrupt and hangup, sent to the starter's group, never reach it.
        fn detach(cmd: &mut Command) -> io::Result<()> {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
            Ok(())
        }
    }
    windows => {
        /// With no console (`DETACHED_PROCESS`) and in a process group of its own
        /// (`CREATE_NEW_PROCESS_GROUP`), so the starter's console closing, or its Ctrl-C or
        /// Ctrl-Break, never reaches it.
        fn detach(cmd: &mut Command) -> io::Result<()> {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            Ok(())
        }
    }
    _ => {
        /// The OSes with no way to detach a child: refused, naming the OS.
        fn detach(_cmd: &mut Command) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("no detached start for {}", std::env::consts::OS),
            ))
        }
    }
}

// The one spelling of who may read the output file: on a unix, its owner alone (mode `0600`, set
// as the file is made); every other OS gives the file the access its directory grants.
std::cfg_select! {
    unix => {
        fn owner_only(options: &mut OpenOptions) {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
    }
    _ => {
        fn owner_only(_options: &mut OpenOptions) {}
    }
}

// The one spelling of how an OS names a running binary that was replaced: linux and android name
// it by its old path with the kernel's `" (deleted)"` marker added, which is how an upgrade by
// rename leaves a running one named, and every other OS names it by its path as it stands.
std::cfg_select! {
    any(target_os = "linux", target_os = "android") => {
        /// The path a running binary named as `exe` stands at, the path a later start of it
        /// runs: `exe` itself when it is a file, else `exe` without the kernel's marker.
        pub fn standing_path(exe: &Path) -> PathBuf {
            if exe.is_file() {
                exe.to_path_buf()
            } else {
                crate::reap::strip_deleted_suffix(exe)
            }
        }
    }
    _ => {
        /// The path a running binary named as `exe` stands at, the path a later start of it
        /// runs: `exe` itself.
        pub fn standing_path(exe: &Path) -> PathBuf {
            exe.to_path_buf()
        }
    }
}

/// The output file at `path`, made or emptied, readable by its owner alone where the OS says so
/// ([`owner_only`]).
fn output_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    owner_only(&mut options);
    options.open(path)
}

/// Start `program` with `args` in `dir`, detached from this process ([`detach`]), with this
/// process's environment but for `env`, each variable of which is set to its value or, given
/// none, removed, null stdin and its stdout and stderr both written to the file at `output`,
/// emptied first: the child's pid. The child's handle is waited on by a thread of this process,
/// so a long-lived starter never keeps an ended child as a zombie.
pub fn start<S, K, V>(
    program: &Path,
    args: &[S],
    env: impl IntoIterator<Item = (K, Option<V>)>,
    dir: &Path,
    output: &Path,
) -> io::Result<u32>
where
    S: AsRef<OsStr>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let out = output_file(output)?;
    let err = out.try_clone()?;
    let mut cmd = crate::subprocess::command(program);
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    for (var, value) in env {
        match value {
            Some(value) => cmd.env(var, value),
            None => cmd.env_remove(var),
        };
    }
    detach(&mut cmd)?;
    let mut child = cmd.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(pid)
}

#[cfg(test)]
mod standing_path_tests {
    use super::*;

    /// A binary replaced by rename while it runs stands at its path without the OS's marker;
    /// any other path, one that is a file under the marker's spelling among them, stands as it is.
    #[test]
    fn a_binary_replaced_while_it_runs_stands_at_its_path_without_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("rigger");
        std::fs::write(&installed, "the new build").unwrap();
        let named_so = dir.path().join("tool (deleted)");
        std::fs::write(&named_so, "a binary with that name").unwrap();
        let marked = PathBuf::from(format!("{} (deleted)", installed.display()));
        let marks_a_replaced_binary = cfg!(any(target_os = "linux", target_os = "android"));
        let cases = [
            (
                "a replaced binary",
                marked.clone(),
                if marks_a_replaced_binary {
                    installed.clone()
                } else {
                    marked.clone()
                },
            ),
            ("a running binary", installed.clone(), installed.clone()),
            (
                "a file under the marker's spelling",
                named_so.clone(),
                named_so.clone(),
            ),
        ];
        for (case, exe, stands) in cases {
            assert_eq!(standing_path(&exe), stands, "{case}");
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// No variable set or removed on the child.
    const NO_ENV: [(&str, Option<&str>); 0] = [];

    /// A started child's environment is the starter's but for the variables it was given: each
    /// given a value is set to it, and each given none is removed, even one the starter has.
    #[test]
    fn a_started_child_has_the_variables_it_was_given_set_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen");
        let output = dir.path().join("out");
        // A variable this process has that the shell neither needs nor sets itself.
        let had = std::env::vars_os()
            .filter_map(|(var, _)| var.into_string().ok())
            .find(|var| {
                !matches!(
                    var.as_str(),
                    "PATH" | "PWD" | "IFS" | "PS1" | "PS2" | "PS4" | "OLDPWD"
                ) && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .expect("this process has a variable");
        let script = r#"eval "removed=\${$2-unset}"; printf '%s|%s\n' "${RIGGER_DETACHED_GIVEN-unset}" "$removed" > "$1""#;
        start(
            Path::new("/bin/sh"),
            &["-c", script, "sh", seen.to_str().unwrap(), &had],
            [
                ("RIGGER_DETACHED_GIVEN", Some("a value")),
                (had.as_str(), None),
            ],
            dir.path(),
            &output,
        )
        .unwrap();
        assert!(
            crate::host_fixtures::wait_until_for(400, || {
                std::fs::read_to_string(&seen).is_ok_and(|s| s.ends_with('\n'))
            }),
            "the child ran"
        );
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap(),
            "a value|unset\n",
            "a variable given a value is set on the child, one given none ({had}) is removed"
        );
    }

    /// A started child runs in the directory it was given and leads a process group of its own,
    /// so the starter's terminal never reaches it, and its stdout and stderr both land in the
    /// output file, emptied first and readable by its owner alone; it ends on its own, when the
    /// test lets it.
    #[test]
    fn a_started_child_runs_in_its_directory_in_a_group_of_its_own() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let go = dir.path().join("go");
        let output = dir.path().join("out");
        let script = r#"echo said; echo refused >&2; pwd -P > here; while [ ! -e "$1" ]; do sleep 0.05; done"#;
        let pid = start(
            Path::new("sh"),
            &["-c", script, "sh", go.to_str().unwrap()],
            NO_ENV,
            dir.path(),
            &output,
        )
        .unwrap();
        let here = dir.path().join("here");
        let wrote = crate::host_fixtures::wait_until_for(400, || {
            std::fs::read_to_string(&here).is_ok_and(|s| s.ends_with('\n'))
        });
        let group = rustix::process::Pid::from_raw(pid as i32)
            .and_then(|p| rustix::process::getpgid(Some(p)).ok());
        std::fs::write(&go, "").unwrap();
        assert!(wrote, "the child ran");
        assert_eq!(
            std::fs::read_to_string(&here).unwrap().trim_end(),
            dir.path().canonicalize().unwrap().to_str().unwrap(),
            "the child ran in the directory it was given"
        );
        assert_eq!(
            group
                .map(rustix::process::Pid::as_raw_nonzero)
                .map(|p| p.get()),
            Some(pid as i32),
            "the child leads a process group of its own"
        );
        assert_eq!(
            (
                std::fs::read_to_string(&output).unwrap(),
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            ),
            ("said\nrefused\n".to_string(), 0o600),
            "the child's stdout and stderr land in the output file, its owner's alone"
        );
        std::fs::write(&output, "an earlier start's output").unwrap();
        let again = dir.path().join("again");
        start(
            Path::new("sh"),
            &["-c", "touch \"$1\"", "sh", again.to_str().unwrap()],
            NO_ENV,
            dir.path(),
            &output,
        )
        .unwrap();
        assert!(
            crate::host_fixtures::wait_until_for(400, || again.exists()),
            "the second child ran"
        );
        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            "",
            "a later start empties the output file before its child writes"
        );
    }
}
