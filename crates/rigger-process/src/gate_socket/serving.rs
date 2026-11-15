//! What the gate channel's adapters share: the serving loop's two waits, the refusal a second
//! driver meets, the driver's answer to one connection and the verb's exchange over one, each over
//! any byte stream the adapter connects.

use std::io;
use std::path::Path;
use std::time::Duration;

use rigger_domain::gate_channel::{GateReply, GateRequest};

/// How long the serving loop waits, between looks for a connection, before it looks for a close.
pub(super) const ACCEPT_POLL: Duration = Duration::from_millis(10);

/// How long the driver waits for a connected client's request line before giving it up, so a
/// client that connects and sends nothing cannot hold the serving loop open.
pub(super) const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The `AddrInUse` refusal a bind meets where a live driver already serves gate requests.
pub(super) fn served_already(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::AddrInUse,
        format!(
            "a driver already serves gate requests at {}",
            path.display()
        ),
    )
}

/// Read one request line from `stream`, answer it, and write the reply line back. A request that
/// does not parse is refused; a client gone before the reply is written is not an error.
pub(super) fn answer_one(
    stream: impl io::Read + io::Write,
    answer: &(dyn Fn(GateRequest) -> GateReply + Sync),
) {
    use std::io::BufRead;
    let mut line = String::new();
    let mut reader = io::BufReader::new(stream);
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let reply = match rigger_domain::gate_channel::from_line::<GateRequest>(&line) {
        Ok(request) => answer(request),
        Err(reason) => GateReply::Refused { reason },
    };
    let _ = reader
        .get_mut()
        .write_all(rigger_domain::gate_channel::to_line(&reply).as_bytes());
}

/// Send `request` over `stream`, connected to a driver's end of the gate channel, and read the
/// driver's reply: `None` when the connection closes before any driver read the request. On a
/// unix socket, a listener that closes with the connection still queued - a driver shutting down,
/// or a socket a process forking at that moment holds a little longer - breaks the pipe the
/// request is written to, or resets the connection once it was written; a driver that closes a
/// connection with the request unread resets it too. No driver took the request then, exactly as
/// when nothing listens. A named pipe whose server is gone before the request is written breaks
/// the pipe the same way, but one closed with the request unread ends the reply as one closed
/// after reading it does. A driver that read the request and closed without replying is the end of
/// file this answers as an error.
pub(super) fn exchange(
    mut stream: impl io::Read + io::Write,
    request: &GateRequest,
) -> io::Result<Option<GateReply>> {
    use std::io::BufRead;
    let mut line = String::new();
    let sent = stream
        .write_all(rigger_domain::gate_channel::to_line(request).as_bytes())
        .and_then(|()| io::BufReader::new(stream).read_line(&mut line));
    match sent {
        Ok(_) => {}
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
            ) =>
        {
            return Ok(None)
        }
        Err(e) => return Err(e),
    }
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
