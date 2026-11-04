//! The Linux keep-awake adapter (spec 105): a logind inhibitor lock, taken over the system bus.

use rigger_domain::sleep_hold::{SleepGuard, Unavailable};
use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};
use zbus::connection::Builder;
use zbus::Connection;

/// The bus name logind owns on the system bus.
const LOGIND: &str = "org.freedesktop.login1";

/// The object path of logind's manager.
const LOGIND_PATH: &str = "/org/freedesktop/login1";

/// The interface of logind's manager, which `Inhibit` belongs to.
const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";

/// The bus name the bus driver itself sends under: an error it sent for a call to logind means
/// logind never answered.
const BUS_DRIVER: &str = "org.freedesktop.DBus";

/// The one bound on an acquire, over connecting to the system bus, its authentication handshake
/// and logind's answer to `Inhibit` together: the D-Bus default method-call timeout.
const DEADLINE: Duration = Duration::from_secs(25);

/// Take a logind `block` inhibitor of sleep and idle by `rigger` for `reason`, over a system-bus
/// connection of its own, within [`DEADLINE`]. The guard holds the descriptor logind answers
/// with, and logind ends the inhibitor when the guard drops it.
pub(super) fn acquire(reason: &str) -> Result<SleepGuard, Unavailable> {
    let bus = Builder::system().map_err(no_system_bus)?;
    acquire_on(bus, reason, DEADLINE)
}

/// Take the inhibitor over the connection `bus` builds, answering unavailable, timed out, once
/// `deadline` passes wherever the acquire then stands: connecting, authenticating or awaiting
/// logind's answer. The connection runs on a current-thread runtime of this acquire's own, which
/// the calling thread drives; the connection and the reply drop inside it, and the runtime is
/// shut down before this returns, so the descriptor the guard holds is the one copy the process
/// keeps. The shutdown joins every thread the runtime started, waiting no longer than what
/// remains of `deadline`: past it, a thread still running, one blocked in a connect the deadline
/// cut short among them, is left to end on its own rather than hold the acquire past its bound.
fn acquire_on(
    bus: Builder<'_>,
    reason: &str,
    deadline: Duration,
) -> Result<SleepGuard, Unavailable> {
    let started = Instant::now();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| unavailable(e.into()))?;
    let taken = runtime.block_on(async {
        let inhibitor = async {
            let bus = bus.build().await.map_err(no_system_bus)?;
            inhibit(&bus, reason).await.map_err(unavailable)
        };
        tokio::time::timeout(deadline, inhibitor)
            .await
            .unwrap_or_else(|_| {
                Err(unavailable(
                    std::io::Error::from(std::io::ErrorKind::TimedOut).into(),
                ))
            })
    });
    runtime.shutdown_timeout(deadline.saturating_sub(started.elapsed()));
    let descriptor = taken?;
    Ok(SleepGuard::new(move || drop(descriptor)))
}

/// Ask logind for the inhibitor, without the interactive-authorization flag, so no
/// authorization prompt opens mid-run. The descriptor is zvariant's close-on-exec duplicate of
/// the one the reply carries, so a child spawned while the guard stands never inherits it; the
/// reply, and the original with it, drops on return. zbus receives that original without
/// close-on-exec, so a child another thread spawns between its receipt and this return inherits
/// it and holds the inhibitor past the guard: a caller acquires before it starts any thread that
/// spawns a child.
async fn inhibit(bus: &Connection, reason: &str) -> zbus::Result<OwnedFd> {
    let reply = bus
        .call_method(
            Some(LOGIND),
            LOGIND_PATH,
            Some(LOGIND_MANAGER),
            "Inhibit",
            &("sleep:idle", "rigger", reason, "block"),
        )
        .await?;
    let descriptor: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
    Ok(descriptor.into())
}

/// Unavailable because no connection to the system bus opens: none is reachable, or the bus
/// refused the connection.
fn no_system_bus(error: zbus::Error) -> Unavailable {
    Unavailable(format!("cannot connect to the system bus: {error}"))
}

/// Unavailable because `Inhibit` failed, classified by the error reply's sender, never its name:
/// an error the bus driver sent means logind never answered (no process owns or provides its
/// name, its activation failed, or the bus's policy refused the call); an error any other sender
/// sent is logind's refusal; a failure that is no error reply (no answer, or an answer with no
/// descriptor) is neither.
fn unavailable(error: zbus::Error) -> Unavailable {
    match &error {
        zbus::Error::MethodError(_, _, reply)
            if reply
                .header()
                .sender()
                .is_some_and(|sender| sender.as_str() == BUS_DRIVER) =>
        {
            Unavailable(format!("logind was not reached on the system bus: {error}"))
        }
        zbus::Error::MethodError(..) => {
            Unavailable(format!("logind refused the inhibitor: {error}"))
        }
        _ => Unavailable(format!("cannot take a logind inhibitor: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{acquire_on, no_system_bus, unavailable};
    use rigger_domain::sleep_hold::Unavailable;
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener};
    use std::pin::Pin;
    use std::time::Duration;
    use zbus::export::futures_core::Stream;

    /// An error reply named `name` to an `Inhibit` call, sent by `sender` and carrying `detail`
    /// where each is given, as zbus hands it to the caller.
    fn error_reply(sender: Option<&str>, name: &str, detail: Option<&str>) -> zbus::Error {
        let call = zbus::Message::method_call("/org/freedesktop/login1", "Inhibit")
            .unwrap()
            .build(&())
            .unwrap();
        let mut reply = zbus::Message::error(&call.header(), name).unwrap();
        if let Some(sender) = sender {
            reply = reply.sender(sender).unwrap();
        }
        let reply = match detail {
            Some(detail) => reply.build(&detail),
            None => reply.build(&()),
        };
        zbus::Error::from(reply.unwrap())
    }

    /// One call as a bus delivers it: destination, path, interface and member, and the inhibitor
    /// its body asks for, by what, who, why and mode.
    type Call = (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        (String, String, String, String),
    );

    /// A fake logind on the private bus `listener` listens on, serving the one connection it
    /// accepts as the bus itself: it answers the connection's `Hello` and answers no other call,
    /// handing back each one it received once the connection closes.
    fn silent_logind(listener: UnixListener) -> std::thread::JoinHandle<Vec<Call>> {
        std::thread::spawn(move || {
            let (stream, _) = listener
                .accept()
                .expect("the adapter connects to the private bus");
            stream
                .set_nonblocking(true)
                .expect("the accepted stream turns non-blocking");
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the fake bus's runtime");
            runtime.block_on(async {
                let stream = tokio::net::UnixStream::from_std(stream).expect("the accepted stream");
                let mut messages = zbus::connection::Builder::unix_stream(stream)
                    .server(zbus::Guid::generate())
                    .expect("a bus-side connection")
                    .p2p()
                    .build_message_stream()
                    .await
                    .expect("the adapter completes the handshake");
                let bus = zbus::Connection::from(&messages);
                let mut calls = Vec::new();
                while let Some(Ok(message)) =
                    std::future::poll_fn(|cx| Pin::new(&mut messages).poll_next(cx)).await
                {
                    let header = message.header();
                    if header.member().is_some_and(|member| member == "Hello") {
                        bus.reply(&header, &":1.1")
                            .await
                            .expect("the fake bus answers Hello");
                        continue;
                    }
                    calls.push((
                        header.destination().map(|name| name.to_string()),
                        header.path().map(|path| path.to_string()),
                        header.interface().map(|name| name.to_string()),
                        header.member().map(|name| name.to_string()),
                        message
                            .body()
                            .deserialize()
                            .expect("the call asks for an inhibitor"),
                    ));
                }
                calls
            })
        })
    }

    /// Where logind takes the adapter's `Inhibit` and never answers it, the acquire answers
    /// unavailable, timed out, once its deadline passes: over a private bus whose fake logind
    /// answers the bus `Hello` and nothing more, `Inhibit` reaches the fake with the adapter's
    /// arguments and the answer is the timed-out failure.
    #[test]
    fn an_inhibit_logind_never_answers_is_answered_unavailable_at_the_deadline() {
        let name = format!(
            "rigger-keep-awake-silent-logind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let address = SocketAddr::from_abstract_name(name.as_bytes()).expect("an abstract name");
        let logind =
            silent_logind(UnixListener::bind_addr(&address).expect("the private bus listens"));
        let bus = zbus::connection::Builder::address(format!("unix:abstract={name}").as_str())
            .expect("the private bus's address");
        assert_eq!(
            acquire_on(bus, "a reason", Duration::from_secs(5)).err(),
            Some(Unavailable(
                "cannot take a logind inhibitor: I/O error: timed out".to_string()
            )),
            "logind never answers Inhibit: the adapter's answer at its deadline"
        );
        let inhibit: Call = (
            Some("org.freedesktop.login1".to_string()),
            Some("/org/freedesktop/login1".to_string()),
            Some("org.freedesktop.login1.Manager".to_string()),
            Some("Inhibit".to_string()),
            (
                "sleep:idle".to_string(),
                "rigger".to_string(),
                "a reason".to_string(),
                "block".to_string(),
            ),
        );
        assert_eq!(
            logind.join().expect("the fake logind ends"),
            vec![inhibit],
            "the calls the fake logind received, none of them answered"
        );
    }

    /// Each way taking the inhibitor fails is answered unavailable with the spelling that names
    /// it, on every host: a bus the adapter cannot connect to; an error the bus driver sends,
    /// since logind was never reached, whatever its name, the bus's policy refusing the call
    /// included; an error any other sender sends, logind's refusal, whatever its name and with or
    /// without a detail; and a failure that is no error reply at all.
    #[test]
    fn each_failure_is_answered_unavailable_naming_it() {
        let driver = Some("org.freedesktop.DBus");
        let logind = Some(":1.14");
        let rows: Vec<(&str, Unavailable, &str)> = vec![
            (
                "the system bus cannot be reached",
                no_system_bus(zbus::Error::Address("no such socket".to_string())),
                "cannot connect to the system bus: address error: no such socket",
            ),
            (
                "the bus driver: no process owns or provides logind's name",
                unavailable(error_reply(
                    driver,
                    "org.freedesktop.DBus.Error.ServiceUnknown",
                    Some("The name org.freedesktop.login1 was not provided by any .service files"),
                )),
                "logind was not reached on the system bus: \
                 org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.login1 was \
                 not provided by any .service files",
            ),
            (
                "the bus driver: logind's activation failed",
                unavailable(error_reply(
                    driver,
                    "org.freedesktop.DBus.Error.Spawn.ChildExited",
                    Some("Process org.freedesktop.login1 exited with status 1"),
                )),
                "logind was not reached on the system bus: \
                 org.freedesktop.DBus.Error.Spawn.ChildExited: Process org.freedesktop.login1 \
                 exited with status 1",
            ),
            (
                "the bus driver: its policy refuses the call to logind",
                unavailable(error_reply(
                    driver,
                    "org.freedesktop.DBus.Error.AccessDenied",
                    Some("Rejected send message"),
                )),
                "logind was not reached on the system bus: org.freedesktop.DBus.Error.AccessDenied: \
                 Rejected send message",
            ),
            (
                "logind denies the lock",
                unavailable(error_reply(
                    logind,
                    "org.freedesktop.DBus.Error.AccessDenied",
                    Some("Permission denied"),
                )),
                "logind refused the inhibitor: org.freedesktop.DBus.Error.AccessDenied: \
                 Permission denied",
            ),
            (
                "logind refuses with no detail",
                unavailable(error_reply(
                    logind,
                    "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
                    None,
                )),
                "logind refused the inhibitor: \
                 org.freedesktop.DBus.Error.InteractiveAuthorizationRequired: no details",
            ),
            (
                "a sender other than the bus driver, under a name the driver also sends",
                unavailable(error_reply(
                    logind,
                    "org.freedesktop.DBus.Error.ServiceUnknown",
                    Some("unknown"),
                )),
                "logind refused the inhibitor: org.freedesktop.DBus.Error.ServiceUnknown: unknown",
            ),
            (
                "an error reply naming no sender",
                unavailable(error_reply(
                    None,
                    "org.freedesktop.DBus.Error.AccessDenied",
                    Some("Permission denied"),
                )),
                "logind refused the inhibitor: org.freedesktop.DBus.Error.AccessDenied: \
                 Permission denied",
            ),
            (
                "no error reply: the reply carries no descriptor",
                unavailable(zbus::Error::InvalidReply),
                "cannot take a logind inhibitor: Invalid D-Bus method reply",
            ),
        ];
        for (what, answered, expected) in rows {
            assert_eq!(answered, Unavailable(expected.to_string()), "{what}");
        }
    }
}
