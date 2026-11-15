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
use std::path::Path;
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

/// The output file at `path`, made or emptied, readable by its owner alone where the OS says so
/// ([`owner_only`]).
fn output_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    owner_only(&mut options);
    options.open(path)
}

/// Start `program` with `args` in `dir`, detached from this process ([`detach`]), with this
/// process's environment, null stdin and its stdout and stderr both written to the file at
/// `output`, emptied first: the child's pid. The child's handle is waited on by a thread of this
/// process, so a long-lived starter never keeps an ended child as a zombie.
pub fn start<S: AsRef<OsStr>>(
    program: &Path,
    args: &[S],
    dir: &Path,
    output: &Path,
) -> io::Result<u32> {
    let out = output_file(output)?;
    let err = out.try_clone()?;
    let mut cmd = crate::subprocess::command(program);
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    detach(&mut cmd)?;
    let mut child = cmd.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(pid)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

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
