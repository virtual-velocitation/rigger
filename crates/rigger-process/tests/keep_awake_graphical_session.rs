//! Spec 125's Linux keep-awake adapter in a graphical session, reached through the crate's public
//! keep-awake API with a fake bus behind every address it reads: the host's acquire takes logind's
//! inhibitor on the bus `DBUS_SYSTEM_BUS_ADDRESS` names and, when `WAYLAND_DISPLAY` or `DISPLAY`
//! names a display, asks the desktop portal for its inhibitor on the bus
//! `DBUS_SESSION_BUS_ADDRESS` names.
//!
//! It is the only test in its binary because it sets those four variables in the process
//! environment, which the tests of one binary share.

#![cfg(all(target_os = "linux", any(feature = "store", not(feature = "core"))))]

#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/keep_awake.rs"]
mod keep_awake_fixtures;

use keep_awake_fixtures::{
    client_end, connected_end, fake_bus, logind_inhibit, pending, portal_inhibit, private_bus,
    Answer, Call, FakeBus, LogindInhibit, PortalInhibit, Request,
};
use rigger_domain::sleep_hold::SleepHold;
use rigger_process::keep_awake::OsSleepHold;
use std::io::ErrorKind;
use std::os::unix::net::UnixListener;
use std::time::Duration;

/// The reason every acquire of this test gives.
const REASON: &str = "the rigger-process graphical-session test";

/// The connections waiting on `listener`, a private bus no fake serves: none, or one a probe
/// takes.
fn waiting(listener: &UnixListener) -> String {
    match pending(listener) {
        Ok(()) => "one waits".to_string(),
        Err(ErrorKind::WouldBlock) => "none".to_string(),
        Err(kind) => format!("{kind:?}"),
    }
}

/// The host's acquire asks the desktop portal for its inhibitor exactly when the environment
/// names a display, and whatever the portal answers it stands granted: with `WAYLAND_DISPLAY` or
/// `DISPLAY` non-empty, it makes one `Inhibit` of suspend and idle (flags 12) giving the reason on
/// the session bus, the connection of a portal that grants (its request responds 0) open while the
/// guard stands and closed once it dropped, that of one that refuses closed before the acquire
/// returned; with both unset, or both empty, no connection waits on the session bus. Logind is
/// asked once on the system bus in every row.
#[test]
fn the_acquire_asks_the_desktop_portal_exactly_when_the_environment_names_a_display() {
    // What, `WAYLAND_DISPLAY` and `DISPLAY` (none: unset), how the fake portal answers (none: no
    // fake serves the session bus, which is only bound), the session bus's connection while the
    // guard stands and once it dropped, and the calls the fake portal received.
    type Row<'a> = (
        &'a str,
        Option<&'a str>,
        Option<&'a str>,
        Option<Answer>,
        (&'a str, &'a str),
        Option<Vec<Call<PortalInhibit>>>,
    );
    let rows: Vec<Row> = vec![
        (
            "a Wayland display, the portal grants",
            Some("wayland-0"),
            None,
            Some(Answer::Handle(Request::Responds(0))),
            ("open", "closed"),
            Some(vec![portal_inhibit(REASON)]),
        ),
        (
            "an X display, the portal grants",
            None,
            Some(":1"),
            Some(Answer::Handle(Request::Responds(0))),
            ("open", "closed"),
            Some(vec![portal_inhibit(REASON)]),
        ),
        (
            "an X display beside an empty Wayland one, the portal refuses",
            Some(""),
            Some(":1"),
            Some(Answer::Refusal),
            ("closed", "closed"),
            Some(vec![portal_inhibit(REASON)]),
        ),
        (
            "neither display set",
            None,
            None,
            None,
            ("none", "none"),
            None,
        ),
        (
            "both displays set empty",
            Some(""),
            Some(""),
            None,
            ("none", "none"),
            None,
        ),
    ];
    for (what, wayland_display, display, answer, connection, portal_calls) in rows {
        let (logind_listener, logind_address) = private_bus("logind");
        let (portal_listener, portal_address) = private_bus("portal");
        std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", &logind_address);
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &portal_address);
        for (name, value) in [("WAYLAND_DISPLAY", wayland_display), ("DISPLAY", display)] {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        let logind = fake_bus::<LogindInhibit>(logind_listener, Answer::Descriptor(Duration::ZERO));
        let portal = match answer {
            Some(answer) => Ok(fake_bus::<PortalInhibit>(portal_listener, answer)),
            None => Err(portal_listener),
        };
        let held = OsSleepHold.acquire(REASON);
        let answered = held.as_ref().err().cloned();
        let (probe, calls): (Box<dyn Fn() -> String>, _) = match portal {
            Ok(FakeBus { end, calls }) => {
                let end = connected_end(
                    &end,
                    &format!("{what}: the acquire connects to the session bus"),
                );
                (Box::new(move || client_end(&end)), Some(calls))
            }
            Err(listener) => (Box::new(move || waiting(&listener)), None),
        };
        let while_held = probe();
        drop(held);
        let dropped = probe();
        assert_eq!(
            (answered, (while_held, dropped)),
            (None, (connection.0.to_string(), connection.1.to_string())),
            "{what}: the acquire's answer, and the session bus's connection while its guard stood \
             and once it dropped"
        );
        assert_eq!(
            calls.map(|calls| calls.join().expect("the fake portal ends")),
            portal_calls,
            "{what}: the calls the fake portal received"
        );
        assert_eq!(
            logind.calls.join().expect("the fake logind ends"),
            vec![logind_inhibit(REASON)],
            "{what}: the calls the fake logind received"
        );
    }
}
