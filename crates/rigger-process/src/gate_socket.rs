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
    fn asking_with_no_listener_answers_none_and_a_stale_socket_is_reclaimed_but_a_live_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gate.sock");
        assert_eq!(ask(&path, &request()).unwrap(), None, "no socket file");
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "a listener dropped without unlinking leaves its file");
        assert_eq!(ask(&path, &request()).unwrap(), None, "a stale socket file");
        let live = GateSocket::bind(&path).expect("a stale socket is reclaimed");
        let again = GateSocket::bind(&path).err().expect("a live socket is not taken");
        assert_eq!(again.kind(), std::io::ErrorKind::AddrInUse);
        drop(live);
    }
}
