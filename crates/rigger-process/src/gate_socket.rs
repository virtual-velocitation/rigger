//! The gate request channel's adapters (signed verdicts): the driver serves
//! [`GateChannel`] on a Unix domain socket readable and writable by its owner only, and
//! `rigger gate` asks over it with [`ask`]. One request and one reply per connection, each one
//! JSON line. Both ends find the socket with [`socket_path`]: one path per repo in a private
//! per-user directory under the runtime directory, named by a digest of the canonical repo root
//! so its length never grows with the repo's path. Windows has no adapter yet: there both ends
//! answer an `Unsupported` error naming the named pipe it would take, which the verb reports as
//! its error.

use std::io;
use std::path::{Path, PathBuf};

use rigger_domain::gate_channel::{GateChannel, GateReply, GateRequest};

/// How many hex digits of the canonical repo root's SHA-256 name its socket.
#[cfg(unix)]
const DIGEST_HEX: usize = 16;

/// The gate socket path for the project rooted at `repo_root`, the one rule the driver binds by
/// and `rigger gate` asks by: [`socket_path_under`] the runtime directory, `$XDG_RUNTIME_DIR`
/// when it is set to an absolute path and the OS temp directory otherwise.
pub fn socket_path(repo_root: &Path) -> io::Result<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    socket_path_under(&runtime, repo_root)
}

/// The gate socket path for the project rooted at `repo_root` under the runtime directory
/// `runtime_dir`: `<runtime_dir>/rigger-<uid>/<first 16 hex of SHA-256 of the canonical repo
/// root>.sock`, the per-host control-path convention. Every path to one repo names one socket,
/// and the path is as long as the runtime directory plus a fixed tail. Errors when the repo root
/// does not resolve.
#[cfg(unix)]
pub fn socket_path_under(runtime_dir: &Path, repo_root: &Path) -> io::Result<PathBuf> {
    use std::fmt::Write;
    use std::os::unix::ffi::OsStrExt;
    let root = repo_root.canonicalize()?;
    let digest = ring::digest::digest(&ring::digest::SHA256, root.as_os_str().as_bytes());
    let name = digest.as_ref()[..DIGEST_HEX / 2]
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        });
    Ok(owner_dir(runtime_dir).join(format!("{name}.sock")))
}

/// No adapter on this OS: always the `Unsupported` error.
#[cfg(not(unix))]
pub fn socket_path_under(_runtime_dir: &Path, _repo_root: &Path) -> io::Result<PathBuf> {
    Err(unsupported())
}

/// The calling user's own directory under `runtime_dir`, `rigger-<uid>`.
#[cfg(unix)]
fn owner_dir(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(format!("rigger-{}", rustix::process::getuid().as_raw()))
}

/// Refuse a socket whose directory is not this user's alone: one another user owns, or can
/// enter, could hold a socket that user placed. A directory not there yet is no refusal.
#[cfg(unix)]
fn check_private(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = match dir.symlink_metadata() {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let ours = meta.is_dir() && meta.uid() == rustix::process::getuid().as_raw();
    if ours && meta.mode() & 0o077 == 0 {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "the gate socket directory {} is not this user's alone (a directory owned by this \
             user with mode 0700 is)",
            dir.display()
        ),
    ))
}

/// Make `dir` this user's private directory: created with mode 0700 when absent, refused by
/// [`check_private`] when it is there but not private.
#[cfg(unix)]
fn private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => check_private(dir),
        Err(e) => Err(e),
    }
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
    /// Bind the socket at `path`, readable and writable by its owner only, in a directory that is
    /// the owner's alone ([`private_dir`]). A file there that no process answers (a socket a dead
    /// driver left) is reclaimed; one a live driver answers is refused with `AddrInUse`. The
    /// socket is bound under a temporary name no longer than its own, restricted, then moved into
    /// place, so no other user can connect while it is still open to them. A path longer than
    /// the OS allows a socket is `InvalidInput`.
    pub fn bind(path: &Path) -> io::Result<GateSocket> {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::{UnixListener, UnixStream};
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            private_dir(dir)?;
        }
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

/// The temporary name a socket is bound under before it is moved to `path`: this process's and
/// this bind's numbers, shorter than any socket name [`socket_path_under`] gives, so a path that
/// fits the OS limit never fails at its staging name.
#[cfg(unix)]
fn staging_path(path: &Path) -> PathBuf {
    static BINDS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let bind = BINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    path.with_file_name(format!(".{:x}.{bind:x}", std::process::id()))
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
/// driver serves there (no socket file, or one nothing listens on). A socket in a directory that
/// is not this user's alone is refused with `PermissionDenied` ([`check_private`]), and a path
/// longer than the OS allows a socket is `InvalidInput`.
#[cfg(unix)]
pub fn ask(path: &Path, request: &GateRequest) -> io::Result<Option<GateReply>> {
    use std::io::{BufRead, Write};
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        check_private(dir)?;
    }
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
        let (runtime, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let path = socket_path_under(runtime.path(), repo.path()).unwrap();
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
        let (runtime, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let path = socket_path_under(runtime.path(), repo.path()).unwrap();
        assert_eq!(ask(&path, &request()).unwrap(), None, "no socket file");
        private_dir(path.parent().unwrap()).unwrap();
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
