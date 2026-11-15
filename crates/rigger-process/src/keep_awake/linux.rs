//! The Linux keep-awake adapter (spec 105): a logind inhibitor lock, taken over the system bus,
//! and in a graphical session the desktop portal's inhibitor besides (spec 125), taken over the
//! session bus, so the desktop holds the session awake too.

use rigger_domain::sleep_hold::{SleepGuard, Unavailable};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::fd::OwnedFd;
use std::sync::mpsc;
use std::thread::JoinHandle;
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
pub const BUS_DRIVER: &str = "org.freedesktop.DBus";

/// The bus name the desktop portal owns on the session bus.
const PORTAL: &str = "org.freedesktop.portal.Desktop";

/// The object path of the desktop portal.
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// The portal's interface that `Inhibit` belongs to.
const PORTAL_INHIBIT: &str = "org.freedesktop.portal.Inhibit";

/// The portal's `Inhibit` flag that inhibits suspending the session.
const SUSPEND: u32 = 4;

/// The portal's `Inhibit` flag that inhibits the session going idle.
const IDLE: u32 = 8;

/// The one bound on an acquire, over connecting to the system bus, its authentication handshake
/// and logind's answer to `Inhibit` together, and, in a graphical session, connecting to the
/// session bus and the portal's answer after them: the D-Bus default method-call timeout.
const DEADLINE: Duration = Duration::from_secs(25);

/// Take a logind `block` inhibitor of sleep and idle by `rigger` for `reason`, over a system-bus
/// connection of its own, and in a graphical session the desktop portal's inhibitor besides, over
/// a session-bus connection of its own, within [`DEADLINE`]. The guard holds the descriptor
/// logind answers with, and logind ends the inhibitor when the guard drops it; the guard closes
/// the portal's connection as it drops, and the portal ends its inhibitor with that connection.
pub(super) fn acquire(reason: &str) -> Result<SleepGuard, Unavailable> {
    let bus = Builder::system().map_err(no_system_bus)?;
    let graphical = graphical_session(
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        std::env::var_os("DISPLAY").as_deref(),
    );
    acquire_on(bus, graphical, Builder::session, reason, DEADLINE)
}

/// Whether the process runs in a graphical session: a Wayland or an X display named, by the
/// value `wayland_display` of `WAYLAND_DISPLAY` or `display` of `DISPLAY` being non-empty.
fn graphical_session(wayland_display: Option<&OsStr>, display: Option<&OsStr>) -> bool {
    [wayland_display, display]
        .into_iter()
        .flatten()
        .any(|value| !value.is_empty())
}

/// Take the inhibitor over the connection `bus` builds, answering unavailable, timed out, once
/// `deadline` passes wherever the acquire then stands: connecting, authenticating or awaiting
/// logind's answer. The connection runs on a current-thread runtime of this acquire's own, which
/// the calling thread drives; the connection and the reply drop inside it, and the runtime is
/// shut down before this returns, so the descriptor the guard holds is the one copy the process
/// keeps. The shutdown joins every thread the runtime started, waiting no longer than what
/// remains of `deadline`: past it, a thread still running, one blocked in a connect the deadline
/// cut short among them, is left to end on its own rather than hold the acquire past its bound.
///
/// Once logind grants, and only when `graphical`, the acquire also asks the desktop portal for its
/// inhibitor over the session-bus connection `session` builds, within what remains of `deadline`
/// (see [`PortalInhibitor::take`]). The portal thread is the one thread a granted acquire leaves
/// standing while its guard lives: dropping the guard closes the session-bus connection and joins
/// that thread before logind's descriptor drops. A portal that is absent, refuses or does not
/// answer in time leaves logind's inhibitor alone in force, and nothing is printed.
fn acquire_on(
    bus: Builder<'_>,
    graphical: bool,
    session: impl FnOnce() -> zbus::Result<Builder<'static>>,
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
    let portal = graphical
        .then(session)
        .and_then(Result::ok)
        .and_then(|session| {
            PortalInhibitor::take(session, reason, deadline.saturating_sub(started.elapsed()))
        });
    Ok(SleepGuard::new(move || {
        if let Some(portal) = portal {
            portal.release();
        }
        drop(descriptor);
    }))
}

/// The desktop portal's inhibitor: the thread holding the session-bus connection it lives on, and
/// the sender whose drop tells that thread to close the connection.
struct PortalInhibitor {
    release: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

impl PortalInhibitor {
    /// Ask the portal for its inhibitor over the connection `session` builds, on a thread of its
    /// own, answering within `within`: the inhibitor when the portal grants it, none when no thread
    /// starts, no connection opens, or the portal refuses or does not answer in time. The thread
    /// drives that connection on a current-thread runtime of its own that keeps no idle thread, so
    /// the blocking thread connecting runs on ends with its connect. When the portal grants, the
    /// thread keeps the connection, and the runtime with it, until [`PortalInhibitor::release`];
    /// otherwise it drops both before this returns, so a refusal leaves no connection standing.
    fn take(session: Builder<'static>, reason: &str, within: Duration) -> Option<Self> {
        let (granted, answer) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let reason = reason.to_string();
        let thread = std::thread::Builder::new()
            .name("rigger-keep-awake-portal".to_string())
            .spawn(move || hold_portal_inhibitor(session, &reason, within, &granted, &released))
            .ok()?;
        if answer.recv() == Ok(true) {
            Some(Self { release, thread })
        } else {
            let _ = thread.join();
            None
        }
    }

    /// Close the portal's connection, ending its inhibitor, and join the thread that held it.
    fn release(self) {
        drop(self.release);
        let _ = self.thread.join();
    }
}

/// The portal thread: connect through `session` and ask the portal for its inhibitor, within
/// `within`, telling `granted` whether it granted it; once it did, hold the connection until
/// `released` reports its sender dropped. The connection drops inside the runtime, and the runtime
/// shuts down, waiting for the threads it started no longer than `within`, before the thread ends.
fn hold_portal_inhibitor(
    session: Builder<'static>,
    reason: &str,
    within: Duration,
    granted: &mpsc::Sender<bool>,
    released: &mpsc::Receiver<()>,
) {
    let started = Instant::now();
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_keep_alive(Duration::ZERO)
        .build()
    else {
        return;
    };
    let held = runtime.block_on(async {
        let inhibitor = async {
            let bus = session.build().await?;
            portal_inhibit(&bus, reason).await?;
            Ok::<_, zbus::Error>(bus)
        };
        tokio::time::timeout(within, inhibitor).await.ok()?.ok()
    });
    let _ = granted.send(held.is_some());
    let bound = match held {
        Some(bus) => {
            let _ = released.recv();
            runtime.block_on(async move { drop(bus) });
            within
        }
        None => within.saturating_sub(started.elapsed()),
    };
    runtime.shutdown_timeout(bound);
}

/// Ask the desktop portal to inhibit suspending the session and its going idle, for `reason`,
/// with no parent window. The portal's method return grants the inhibitor, which then stands until
/// this connection closes; an error reply refuses it.
async fn portal_inhibit(bus: &Connection, reason: &str) -> zbus::Result<()> {
    let options = HashMap::from([("reason", zbus::zvariant::Value::from(reason))]);
    bus.call_method(
        Some(PORTAL),
        PORTAL_PATH,
        Some(PORTAL_INHIBIT),
        "Inhibit",
        &("", SUSPEND | IDLE, options),
    )
    .await?;
    Ok(())
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
    use super::{acquire_on, graphical_session, no_system_bus, unavailable};
    use crate::test_support::{
        client_end, fake_bus, logind_inhibit, pending, portal_inhibit, private_bus, Answer, Call,
        FakeBus, LogindInhibit, PortalInhibit,
    };
    use rigger_domain::sleep_hold::Unavailable;
    use std::ffi::OsStr;
    use std::io::ErrorKind;
    use std::time::Duration;
    use zbus::connection::Builder;

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

    /// Where logind takes the adapter's `Inhibit` and never answers it, the acquire answers
    /// unavailable, timed out, once its deadline passes: over a private bus whose fake logind
    /// answers the bus `Hello` and nothing more, `Inhibit` reaches the fake with the adapter's
    /// arguments and the answer is the timed-out failure. The portal is asked only once logind
    /// grants, so the acquire makes no connection to it, a graphical session's though it is.
    #[test]
    fn an_inhibit_logind_never_answers_is_answered_unavailable_at_the_deadline() {
        let (listener, address) = private_bus("silent-logind");
        let logind = fake_bus::<LogindInhibit>(listener, Answer::Nothing);
        let bus = Builder::address(address.as_str()).expect("the private bus's address");
        let (portal, portal_address) = private_bus("unasked-portal");
        assert_eq!(
            acquire_on(
                bus,
                true,
                || Builder::address(portal_address.as_str()),
                "a reason",
                Duration::from_secs(5)
            )
            .err(),
            Some(Unavailable(
                "cannot take a logind inhibitor: I/O error: timed out".to_string()
            )),
            "logind never answers Inhibit: the adapter's answer at its deadline"
        );
        assert_eq!(
            logind.calls.join().expect("the fake logind ends"),
            vec![logind_inhibit("a reason")],
            "the calls the fake logind received, none of them answered"
        );
        assert_eq!(
            pending(&portal),
            Err(ErrorKind::WouldBlock),
            "the connections to the portal of an acquire logind never granted"
        );
    }

    /// In a graphical session, once logind grants, the acquire asks the desktop portal for its
    /// inhibitor over a session-bus connection of its own, with one `Inhibit` of suspend and idle
    /// (flags 12) giving the reason, and whatever the portal does the acquire stands granted. The
    /// portal grants only when the request its `Inhibit` returns responds 0: that connection is
    /// open while the guard stands and closed once it drops. One whose request responds anything
    /// else, or does not respond by the deadline, one the portal refuses, and one it never answers
    /// by the deadline, is closed before the acquire returns; and with no portal listening there
    /// is no connection to make. Logind is asked once in every row.
    #[test]
    fn a_graphical_session_asks_the_portal_and_any_answer_leaves_the_acquire_granted() {
        // What, how the portal answers (none: no portal listens), its connection while the guard
        // stands and once it dropped, and the calls the portal received.
        type Row<'a> = (
            &'a str,
            Option<Answer>,
            Option<(&'a str, &'a str)>,
            Option<Vec<Call<PortalInhibit>>>,
        );
        let rows: Vec<Row> = vec![
            (
                "the portal grants: its request responds 0",
                Some(Answer::Handle(Some(0))),
                Some(("open", "closed")),
                Some(vec![portal_inhibit("a reason")]),
            ),
            (
                "the portal's request responds 1",
                Some(Answer::Handle(Some(1))),
                Some(("closed", "closed")),
                Some(vec![portal_inhibit("a reason")]),
            ),
            (
                "the portal's request does not respond by the deadline",
                Some(Answer::Handle(None)),
                Some(("closed", "closed")),
                Some(vec![portal_inhibit("a reason")]),
            ),
            (
                "the portal refuses",
                Some(Answer::Refusal),
                Some(("closed", "closed")),
                Some(vec![portal_inhibit("a reason")]),
            ),
            (
                "the portal never answers by the deadline",
                Some(Answer::Nothing),
                Some(("closed", "closed")),
                Some(vec![portal_inhibit("a reason")]),
            ),
            ("no portal listens", None, None, None),
        ];
        for (what, answer, connection, portal_calls) in rows {
            let (listener, address) = private_bus("granting-logind");
            let logind = fake_bus::<LogindInhibit>(listener, Answer::Descriptor);
            let (listener, portal_address) = private_bus("portal");
            let portal = answer.map(|answer| fake_bus::<PortalInhibit>(listener, answer));
            let held = acquire_on(
                Builder::address(address.as_str()).expect("the private logind's address"),
                true,
                || Builder::address(portal_address.as_str()),
                "a reason",
                Duration::from_secs(5),
            );
            let answered = held.as_ref().err().cloned();
            let (end, calls) = match portal {
                Some(FakeBus { end, calls }) => (
                    Some(end.recv().expect("the adapter connects to the portal")),
                    Some(calls),
                ),
                None => (None, None),
            };
            let while_held = end.as_ref().map(client_end);
            drop(held);
            let dropped = end.as_ref().map(client_end);
            assert_eq!(
                (answered, while_held.zip(dropped)),
                (
                    None,
                    connection.map(|(held, dropped)| (held.to_string(), dropped.to_string()))
                ),
                "{what}: the acquire's answer, and the portal's connection while its guard stood \
                 and once it dropped"
            );
            assert_eq!(
                calls.map(|calls| calls.join().expect("the fake portal ends")),
                portal_calls,
                "{what}: the calls the fake portal received"
            );
            assert_eq!(
                logind.calls.join().expect("the fake logind ends"),
                vec![logind_inhibit("a reason")],
                "{what}: the calls the fake logind received"
            );
        }
    }

    /// With no graphical session, the acquire takes logind's inhibitor alone and makes no
    /// connection to the session bus: the portal's private bus has no connection waiting, while
    /// the guard stands or once it dropped.
    #[test]
    fn with_no_graphical_session_the_acquire_makes_no_portal_connection() {
        let (listener, address) = private_bus("granting-logind");
        let logind = fake_bus::<LogindInhibit>(listener, Answer::Descriptor);
        let (portal, portal_address) = private_bus("unasked-portal");
        let held = acquire_on(
            Builder::address(address.as_str()).expect("the private logind's address"),
            false,
            || Builder::address(portal_address.as_str()),
            "a reason",
            Duration::from_secs(5),
        );
        let while_held = (held.as_ref().err().cloned(), pending(&portal));
        drop(held);
        assert_eq!(
            (while_held, pending(&portal)),
            ((None, Err(ErrorKind::WouldBlock)), Err(ErrorKind::WouldBlock)),
            "the acquire's answer and the connections waiting on the portal while its guard stood, \
             and those waiting once it dropped"
        );
        assert_eq!(
            logind.calls.join().expect("the fake logind ends"),
            vec![logind_inhibit("a reason")],
            "the calls the fake logind received"
        );
    }

    /// A graphical session is one with a Wayland or an X display named: `WAYLAND_DISPLAY` or
    /// `DISPLAY` non-empty, each of them alone or both; neither set, or each set empty, is none.
    #[test]
    fn a_graphical_session_is_a_display_named_non_empty() {
        let rows: [(Option<&str>, Option<&str>, bool); 9] = [
            (None, None, false),
            (Some(""), None, false),
            (None, Some(""), false),
            (Some(""), Some(""), false),
            (Some("wayland-0"), None, true),
            (None, Some(":1"), true),
            (Some("wayland-0"), Some(""), true),
            (Some(""), Some(":1"), true),
            (Some("wayland-0"), Some(":1"), true),
        ];
        for (wayland_display, display, expected) in rows {
            assert_eq!(
                graphical_session(wayland_display.map(OsStr::new), display.map(OsStr::new)),
                expected,
                "WAYLAND_DISPLAY {wayland_display:?}, DISPLAY {display:?}"
            );
        }
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
