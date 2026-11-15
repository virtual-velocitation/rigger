//! The gate request channel's adapters (signed verdicts): the driver serves [`GateChannel`] on a
//! channel only its owner can reach, and `rigger gate` asks over it with [`ask`]. One request and
//! one reply per connection, each one JSON line. The driver binds the channel [`socket_path`]
//! names, one per repo, named by a digest of the canonical repo root so its length never grows
//! with the repo's path: on unix a Unix domain socket readable and writable by its owner only, in
//! a private per-user directory under the runtime directory, and on Windows the named pipe
//! [`pipe_name`] gives, open to its owner and to local clients only. The driver hands that address
//! to every spawn as [`GATE_SOCKET_ENV`], and the verb asks there, deriving the same address itself
//! only when the variable is unset, as in the operator's own shell ([`socket_to_ask`]). An OS with
//! neither adapter answers an `Unsupported` error at both ends, which the verb reports as its
//! error.
//!
//! [`GateChannel`]: rigger_domain::gate_channel::GateChannel

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use rigger_domain::gate_channel::GATE_SOCKET_ENV;

std::cfg_select! {
    unix => {
        mod serving;
        mod unix;
        pub use unix::{ask, socket_path, socket_path_under, GateSocket};
    }
    windows => {
        mod serving;
        mod windows;
        pub use windows::{ask, socket_path, GateSocket};
    }
    _ => {
        /// The OSes with no adapter, the `_` arm of the dispatch above: every end of the gate
        /// channel answers the `Unsupported` error.
        mod unsupported {
            use std::io;
            use std::path::{Path, PathBuf};

            use rigger_domain::gate_channel::{GateChannel, GateReply, GateRequest};

            /// The error every gate channel end answers on an OS with no adapter.
            fn unsupported() -> io::Error {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "the gate request channel has no adapter for this OS",
                )
            }

            /// No adapter on this OS: always the `Unsupported` error.
            pub fn socket_path(_repo_root: &Path) -> io::Result<PathBuf> {
                Err(unsupported())
            }

            /// No adapter on this OS: always the `Unsupported` error.
            pub fn socket_path_under(_runtime_dir: &Path, _repo_root: &Path) -> io::Result<PathBuf> {
                Err(unsupported())
            }

            /// No OS without an adapter can hold a bound channel.
            pub struct GateSocket {
                _unbindable: std::convert::Infallible,
            }

            impl GateSocket {
                /// No adapter on this OS: always the `Unsupported` error.
                pub fn bind(_path: &Path) -> io::Result<GateSocket> {
                    Err(unsupported())
                }
            }

            impl GateChannel for GateSocket {
                fn serve(
                    &self,
                    _answer: &(dyn Fn(GateRequest) -> GateReply + Sync),
                ) -> io::Result<()> {
                    Err(unsupported())
                }
                fn close(&self) {}
                fn address(&self) -> Option<String> {
                    None
                }
            }

            /// No adapter on this OS: always the `Unsupported` error.
            pub fn ask(_path: &Path, _request: &GateRequest) -> io::Result<Option<GateReply>> {
                Err(unsupported())
            }
        }
        pub use unsupported::{ask, socket_path, socket_path_under, GateSocket};
    }
}

/// How many hex digits of the canonical repo root's SHA-256 name its gate channel.
const DIGEST_HEX: usize = 16;

/// The first [`DIGEST_HEX`] hex digits of the SHA-256 of `root_bytes`, a canonical repo root's
/// bytes: the name its gate channel carries on every OS.
fn root_digest(root_bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = ring::digest::digest(&ring::digest::SHA256, root_bytes);
    digest.as_ref()[..DIGEST_HEX / 2]
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The Windows named pipe for the canonical repo root whose bytes are `root_bytes`:
/// `\\.\pipe\rigger-gate-` and [`root_digest`], the digits the unix socket's file name carries.
pub fn pipe_name(root_bytes: &[u8]) -> String {
    format!(r"\\.\pipe\rigger-gate-{}", root_digest(root_bytes))
}

/// The gate channel `rigger gate` asks for the project rooted at `repo_root`:
/// [`addressed_or_derived`] over [`GATE_SOCKET_ENV`].
pub fn socket_to_ask(repo_root: &Path) -> io::Result<PathBuf> {
    addressed_or_derived(std::env::var_os(GATE_SOCKET_ENV), repo_root)
}

/// The channel at `addressed`, the value of [`GATE_SOCKET_ENV`] a serving driver hands its spawns,
/// when it is set; [`socket_path`] for `repo_root` when it is unset or empty, the operator's own
/// shell. A relative value is refused.
pub fn addressed_or_derived(addressed: Option<OsString>, repo_root: &Path) -> io::Result<PathBuf> {
    let Some(addressed) = addressed.filter(|value| !value.is_empty()) else {
        return socket_path(repo_root);
    };
    let path = PathBuf::from(addressed);
    if path.is_absolute() {
        return Ok(path);
    }
    // Not `InvalidInput`, the over-long kind the verb runs the gate for information on: a
    // relative address is a broken environment, so the verb refuses rather than guess.
    Err(io::Error::other(format!(
        "{GATE_SOCKET_ENV} is {}, a relative path; it must be the absolute path of the driver's \
         gate socket",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE PIPE IS NAMED BY THE ROOT'S DIGEST ON EVERY HOST: `pipe_name` over a root's bytes is
    /// `\\.\pipe\rigger-gate-` followed by the first 16 hex digits of their SHA-256, the digits
    /// the unix socket name carries for those bytes; two roots give two names.
    #[test]
    fn the_pipe_name_is_the_pipe_prefix_and_the_roots_sixteen_digits_and_two_roots_give_two() {
        // Each row's digits are the first 16 of the hex SHA-256 `sha256sum` prints for the bytes.
        let rows: [(&[u8], &str); 2] = [
            (b"/home/dev/project", "1afbf223bb0b58ba"),
            (br"\\?\C:\Users\dev\project", "e7671d9d51a2dc60"),
        ];
        let names: Vec<String> = rows.iter().map(|(root, _)| pipe_name(root)).collect();
        for ((root, digits), name) in rows.iter().zip(&names) {
            assert_eq!(
                *name,
                format!(r"\\.\pipe\rigger-gate-{digits}"),
                "{}",
                String::from_utf8_lossy(root)
            );
        }
        assert_ne!(names[0], names[1], "two roots give two names");
    }

    /// THE PIPE CARRIES THE SOCKET'S DIGITS: GIVEN a repo on a unix host, THEN the pipe name over
    /// its canonical root's bytes ends in the digits its socket file is named by.
    #[cfg(unix)]
    #[test]
    fn the_pipe_name_carries_the_digits_the_unix_socket_name_carries_for_the_same_root() {
        use std::os::unix::ffi::OsStrExt;
        let (runtime, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let root = repo.path().canonicalize().unwrap();
        let socket = socket_path_under(runtime.path(), repo.path()).unwrap();
        assert_eq!(
            pipe_name(root.as_os_str().as_bytes())
                .strip_prefix(r"\\.\pipe\rigger-gate-")
                .map(|digits| format!("{digits}.sock")),
            socket
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            "the pipe's digits and the socket's name for {}",
            root.display()
        );
    }

    /// THE DRIVER'S ADDRESS WINS: GIVEN `RIGGER_GATE_SOCKET` set to an absolute path, THEN the
    /// verb asks there whatever the runtime directory derives; unset or empty, it derives the
    /// path as the operator's own shell does; set to a relative path, it is refused naming the
    /// variable, and the refusal is never the over-long kind the verb runs for information on.
    #[cfg(unix)]
    #[test]
    fn an_absolute_address_wins_over_the_derivation_unset_derives_and_a_relative_one_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        let derived = socket_path(repo.path()).unwrap();
        let addressed = Path::new("/elsewhere/gate.sock");
        for (why, value, want) in [
            (
                "absolute",
                Some(addressed.into()),
                Ok(addressed.to_path_buf()),
            ),
            ("unset", None, Ok(derived.clone())),
            ("empty", Some(OsString::new()), Ok(derived.clone())),
            ("relative", Some("run/gate.sock".into()), Err(())),
        ] {
            let got = addressed_or_derived(value, repo.path());
            match (got, want) {
                (Ok(path), Ok(want)) => assert_eq!(path, want, "{why}"),
                (Err(e), Err(())) => {
                    assert_ne!(e.kind(), io::ErrorKind::InvalidInput, "{why}: {e}");
                    let said = e.to_string();
                    assert!(
                        said.contains(GATE_SOCKET_ENV)
                            && said.contains("run/gate.sock")
                            && said.contains("absolute"),
                        "{why}: the refusal names the variable, its value and the rule: {said}"
                    );
                }
                (got, want) => panic!("{why}: got {got:?}, want {want:?}"),
            }
        }
    }
}
