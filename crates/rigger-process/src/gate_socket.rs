//! The gate request channel's adapters (signed verdicts): the driver serves
//! [`GateChannel`] on a Unix domain socket under the project's `.rigger` directory, readable and
//! writable by its owner only, and `rigger gate` asks over it with [`ask`]. One request and one
//! reply per connection, each one JSON line. Windows has no adapter yet: there both ends answer an
//! `Unsupported` error naming the named pipe it would take, which the verb reports as its error.

use std::io;
use std::path::{Path, PathBuf};

use rigger_domain::gate_channel::{GateChannel, GateReply, GateRequest};

/// The gate socket path for the project rooted at `repo_root`, under the runtime directory
/// `runtime_dir`.
pub fn socket_path_under(_runtime_dir: &Path, _repo_root: &Path) -> io::Result<PathBuf> {
    Err(io::Error::other("no socket path rule yet"))
}

/// A bound gate request socket, its file removed when it drops.
pub struct GateSocket {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(unix)]
    listener: std::os::unix::net::UnixListener,
    #[cfg(unix)]
    closed: std::sync::atomic::AtomicBool,
    /// No OS without an adapter can hold a bound socket.
    #[cfg(not(unix))]
    _unbindable: std::convert::Infallible,
}

/// How long the driver waits for a connected client's request line before giving it up, so a
/// client that connects and sends nothing cannot hold the serving loop open.
#[cfg(unix)]
const REQUEST_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(unix)]
impl GateSocket {
    /// Bind the socket at `path`, readable and writable by its owner only. A file there that no
    /// process answers (a socket a dead driver left) is reclaimed; one a live driver answers is
    /// refused with `AddrInUse`. The socket is bound under a temporary name, restricted, then
    /// moved into place, so no other user can connect while it is still open to them.
    pub fn bind(path: &Path) -> io::Result<GateSocket> {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::{UnixListener, UnixStream};
        if path.symlink_metadata().is_ok() {
            if UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!(
                        "a driver already serves gate requests at {}",
                        path.display()
                    ),
                ));
            }
            std::fs::remove_file(path)?;
        }
        let staging = staging_path(path);
        let _ = std::fs::remove_file(&staging);
        let listener = UnixListener::bind(&staging)?;
        let placed = std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o600))
            .and_then(|()| std::fs::rename(&staging, path));
        if let Err(e) = placed {
            let _ = std::fs::remove_file(&staging);
            return Err(e);
        }
        Ok(GateSocket {
            path: path.to_path_buf(),
            listener,
            closed: std::sync::atomic::AtomicBool::new(false),
        })
    }
}

/// The temporary name a socket is bound under before it is moved to `path`.
#[cfg(unix)]
fn staging_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{}", std::process::id()))
}

#[cfg(unix)]
impl GateChannel for GateSocket {
    fn serve(&self, answer: &(dyn Fn(GateRequest) -> GateReply + Sync)) -> io::Result<()> {
        use std::sync::atomic::Ordering;
        std::thread::scope(|s| {
            for stream in self.listener.incoming() {
                if self.closed.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else {
                    continue;
                };
                s.spawn(move || answer_one(stream, answer));
            }
        });
        Ok(())
    }

    fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        // Wake the accept the serving loop blocks in, so it sees the flag.
        let _ = std::os::unix::net::UnixStream::connect(&self.path);
    }
}

/// Read one request line from `stream`, answer it, and write the reply line back. A request that
/// does not parse is refused; a client gone before the reply is written is not an error.
#[cfg(unix)]
fn answer_one(
    stream: std::os::unix::net::UnixStream,
    answer: &(dyn Fn(GateRequest) -> GateReply + Sync),
) {
    use std::io::{BufRead, Write};
    let _ = stream.set_read_timeout(Some(REQUEST_READ_TIMEOUT));
    let mut line = String::new();
    let Ok(mut reader) = stream.try_clone().map(io::BufReader::new) else {
        return;
    };
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let reply = match rigger_domain::gate_channel::from_line::<GateRequest>(&line) {
        Ok(request) => answer(request),
        Err(reason) => GateReply::Refused { reason },
    };
    let mut writer = stream;
    let _ = writer.write_all(rigger_domain::gate_channel::to_line(&reply).as_bytes());
}

#[cfg(unix)]
impl Drop for GateSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Ask the driver serving the socket at `path` to answer `request`: its reply, or `None` when no
/// driver serves there (no socket file, or one nothing listens on).
#[cfg(unix)]
pub fn ask(path: &Path, request: &GateRequest) -> io::Result<Option<GateReply>> {
    use std::io::{BufRead, Write};
    let mut stream = match std::os::unix::net::UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    stream.write_all(rigger_domain::gate_channel::to_line(request).as_bytes())?;
    let mut line = String::new();
    io::BufReader::new(stream).read_line(&mut line)?;
    if line.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the driver closed the gate channel without replying",
        ));
    }
    rigger_domain::gate_channel::from_line(&line)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The error every gate channel end answers on an OS with no adapter.
#[cfg(not(unix))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "the gate request channel has no adapter for this OS yet (it would be a named pipe)",
    )
}

#[cfg(not(unix))]
impl GateSocket {
    /// No adapter on this OS: always the `Unsupported` error.
    pub fn bind(_path: &Path) -> io::Result<GateSocket> {
        Err(unsupported())
    }
}

#[cfg(not(unix))]
impl GateChannel for GateSocket {
    fn serve(&self, _answer: &(dyn Fn(GateRequest) -> GateReply + Sync)) -> io::Result<()> {
        Err(unsupported())
    }
    fn close(&self) {}
}

/// No adapter on this OS: always the `Unsupported` error.
#[cfg(not(unix))]
pub fn ask(_path: &Path, _request: &GateRequest) -> io::Result<Option<GateReply>> {
    Err(unsupported())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn request() -> GateRequest {
        GateRequest {
            dir: "/work/unit".into(),
            gate: "g".into(),
        }
    }

    /// THE SOCKET IS THE RUN'S OWN, ANSWERED IN TURN: a bound socket is a file only its owner
    /// can open, a request sent over it reaches the answer and its reply comes back, closing ends
    /// the serving loop, and dropping the socket removes its file.
    #[test]
    fn a_bound_socket_is_owner_only_answers_a_request_and_closing_and_dropping_end_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gate.sock");
        let socket = GateSocket::bind(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "only the owner may open the socket");
        let answer = |req: GateRequest| GateReply::Refused {
            reason: format!("asked {} in {}", req.gate, req.dir),
        };
        let reply = std::thread::scope(|s| {
            let serving = s.spawn(|| socket.serve(&answer));
            let reply = ask(&path, &request()).unwrap();
            socket.close();
            serving.join().unwrap().unwrap();
            reply
        });
        assert_eq!(
            reply,
            Some(GateReply::Refused {
                reason: "asked g in /work/unit".into()
            })
        );
        drop(socket);
        assert!(!path.exists(), "dropping the socket removes its file");
    }

    /// NO DRIVER, NO ANSWER: with no socket file, or a file no process listens on, asking
    /// answers `None`; binding over a stale file reclaims it, and binding over a live one is
    /// refused.
    #[test]
    fn asking_with_no_listener_answers_none_and_a_stale_socket_is_reclaimed_but_a_live_one_is_not()
    {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gate.sock");
        assert_eq!(ask(&path, &request()).unwrap(), None, "no socket file");
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(
            path.exists(),
            "a listener dropped without unlinking leaves its file"
        );
        assert_eq!(ask(&path, &request()).unwrap(), None, "a stale socket file");
        let live = GateSocket::bind(&path).expect("a stale socket is reclaimed");
        let again = GateSocket::bind(&path)
            .err()
            .expect("a live socket is not taken");
        assert_eq!(again.kind(), std::io::ErrorKind::AddrInUse);
        drop(live);
    }

    /// ONE SOCKET PER REPO, NEVER OUT OF PATH: GIVEN two repo paths under one runtime directory,
    /// THEN they share a socket path exactly when they name the same repo (a path through `..`
    /// included), the socket sits in the owner's `rigger-<uid>` directory, and its length is the
    /// runtime directory's plus a fixed tail however long the repo path is.
    #[test]
    fn the_socket_path_is_one_per_repo_under_the_owners_directory_with_a_bounded_length() {
        let runtime = tempfile::tempdir().unwrap();
        let repos = tempfile::tempdir().unwrap();
        let deep = repos.path().join("d".repeat(200)).join("e".repeat(200));
        for dir in ["a", "b", "a/sub"] {
            std::fs::create_dir_all(repos.path().join(dir)).unwrap();
        }
        std::fs::create_dir_all(&deep).unwrap();
        let path = |p: &Path| socket_path_under(runtime.path(), p).unwrap();
        let (a, b, a_via_sub) = (
            repos.path().join("a"),
            repos.path().join("b"),
            repos.path().join("a/sub/.."),
        );
        let owner_dir = runtime
            .path()
            .join(format!("rigger-{}", rustix::process::getuid().as_raw()));
        let tail = "/".len() + 16 + ".sock".len();
        for (left, right, same) in [
            (&a, &a, true),
            (&a, &a_via_sub, true),
            (&a, &b, false),
            (&a, &deep, false),
        ] {
            let (l, r) = (path(left), path(right));
            assert_eq!(l == r, same, "{} vs {}", left.display(), right.display());
            for p in [&l, &r] {
                assert_eq!(p.parent(), Some(owner_dir.as_path()), "{}", p.display());
                assert_eq!(
                    p.as_os_str().len(),
                    owner_dir.as_os_str().len() + tail,
                    "{}",
                    p.display()
                );
                assert_eq!(p.extension().and_then(|e| e.to_str()), Some("sock"));
            }
        }
    }

    /// THE OWNER'S DIRECTORY IS PRIVATE: GIVEN no `rigger-<uid>` directory, WHEN the socket binds,
    /// THEN the directory is made readable by its owner only; GIVEN one others can enter, THEN
    /// binding and asking are refused rather than trusting a socket another user could place.
    #[test]
    fn binding_makes_the_owner_directory_private_and_a_shared_one_is_refused() {
        let runtime = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let path = socket_path_under(runtime.path(), repo.path()).unwrap();
        let owner_dir = path.parent().unwrap().to_path_buf();
        let socket = GateSocket::bind(&path).unwrap();
        let mode = std::fs::metadata(&owner_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "only the owner may enter its directory");
        drop(socket);
        std::fs::set_permissions(&owner_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (end, err) in [
            ("bind", GateSocket::bind(&path).err()),
            ("ask", ask(&path, &request()).err()),
        ] {
            let err = err.unwrap_or_else(|| panic!("{end}: a shared directory is refused"));
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{end}: {err}");
        }
    }

    /// A PATH TOO LONG FOR A SOCKET IS NAMED AS SUCH: GIVEN a runtime directory so long the
    /// socket path overflows the OS limit, THEN binding and asking both answer `InvalidInput`,
    /// the cause the verb reports before it falls back to an information-only run.
    #[test]
    fn a_socket_path_longer_than_the_os_allows_is_invalid_input_at_both_ends() {
        let runtime = tempfile::tempdir().unwrap();
        let long = runtime.path().join("r".repeat(120));
        std::fs::create_dir_all(&long).unwrap();
        let repo = tempfile::tempdir().unwrap();
        let path = socket_path_under(&long, repo.path()).unwrap();
        for (end, err) in [
            ("bind", GateSocket::bind(&path).err()),
            ("ask", ask(&path, &request()).err()),
        ] {
            let err = err.unwrap_or_else(|| panic!("{end}: an over-long path fails"));
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{end}: {err}");
        }
    }
}
