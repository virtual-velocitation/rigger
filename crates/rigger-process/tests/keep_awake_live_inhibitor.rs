//! Spec 105's Linux keep-awake adapter against the host's system bus, reached through the
//! crate's public keep-awake API.
//!
//! It is the only test in its binary, so no other test in its process spawns a child: the
//! adapter takes its acquire's precondition from its caller, that no thread spawns a child while
//! the acquire runs, and the tests of one binary share a process.

#![cfg(all(target_os = "linux", any(feature = "store", not(feature = "core"))))]

extern crate rigger_process as rigger;

#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;

use host_fixtures::{release, wait_until, waiting_shell};
use rigger_domain::sleep_hold::{SleepHold, Unavailable};
use rigger_process::keep_awake::{OsSleepHold, BUS_DRIVER};
use std::path::Path;
use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::Connection;
use zbus::names::BusName;

/// The bus name logind owns on the system bus.
const LOGIND: &str = "org.freedesktop.login1";

/// One inhibitor as logind's `ListInhibitors` lists it: what, who, why, mode, uid and pid.
type Listed = (String, String, String, String, u32, u32);

/// One inhibitor by what, who, why and mode.
type Inhibitor = (String, String, String, String);

/// Logind's answer to an `Inhibit` this test asks for itself, with the arguments the adapter
/// passes for `reason`.
fn inhibit(bus: &Connection, reason: &str) -> zbus::Result<zbus::Message> {
    bus.call_method(
        Some(LOGIND),
        "/org/freedesktop/login1",
        Some("org.freedesktop.login1.Manager"),
        "Inhibit",
        &("sleep:idle", "rigger", reason, "block"),
    )
}

/// The inhibitors logind lists for `reason`, a reason unique to one run of the test. They are
/// found by the reason, not the uid and pid logind lists: a test run inside a user or PID
/// namespace knows its own ids by other numbers than logind does.
fn inhibitors_for(bus: &Connection, reason: &str) -> Vec<Inhibitor> {
    let listed: Vec<Listed> = bus
        .call_method(
            Some(LOGIND),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "ListInhibitors",
            &(),
        )
        .expect("logind lists its inhibitors")
        .body()
        .deserialize()
        .expect("ListInhibitors answers a(ssssuu)");
    listed
        .into_iter()
        .filter(|(_, _, why, ..)| why == reason)
        .map(|(what, who, why, mode, ..)| (what, who, why, mode))
        .collect()
}

/// The unique name of the process that owns logind's name on the bus, `None` when no process
/// does: asked after an `Inhibit`, which starts an activatable logind, so `None` means no
/// logind runs on the bus, neither started nor startable.
fn logind_owner(bus: &Connection) -> Option<String> {
    let driver = DBusProxy::new(bus).expect("the bus driver answers");
    match driver.get_name_owner(BusName::try_from(LOGIND).unwrap()) {
        Ok(owner) => Some(owner.to_string()),
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => None,
        Err(e) => panic!("GetNameOwner answers an owner or NameHasNoOwner: {e}"),
    }
}

/// Spec 105's Linux adapter against the host's system bus, in whichever of its worlds the host
/// is. Where logind grants the inhibitor, logind lists it as a `sleep:idle` block lock by
/// `rigger` for the reason given while the guard stands, and lists it no more once the guard
/// drops, even while a child spawned under the guard still runs, since the guard holds the one
/// descriptor and it is close-on-exec. Where logind refuses, the adapter answers unavailable
/// naming the refusal logind itself sends this test's own `Inhibit`. Where the bus driver
/// answers `Inhibit` instead, its policy refusing the call while logind runs or no logind
/// running, its name owned by no process once `Inhibit` was asked, the adapter answers that
/// logind was not reached, with the bus driver's own answer to `Inhibit`; and where no
/// connection to the system bus opens, that it cannot connect, with the error opening it.
#[test]
fn the_inhibitor_stands_while_its_guard_does_and_ends_with_it() {
    let reason = &format!(
        "the rigger-process keep-awake test at {} ns",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let held = OsSleepHold.acquire(reason);
    let bus = match Connection::system() {
        Ok(bus) => bus,
        Err(e) => {
            assert_eq!(
                held.err(),
                Some(Unavailable(format!(
                    "cannot connect to the system bus: {e}"
                ))),
                "no connection to the system bus: the adapter's answer"
            );
            return;
        }
    };
    let why = match held {
        Ok(guard) => {
            let ours: Vec<Inhibitor> = vec![(
                "sleep:idle".to_string(),
                "rigger".to_string(),
                reason.to_string(),
                "block".to_string(),
            )];
            assert_eq!(
                inhibitors_for(&bus, reason),
                ours,
                "logind grants: the inhibitors for this reason while the guard stands"
            );
            let child = waiting_shell("true", Path::new("/"));
            drop(guard);
            wait_until(|| inhibitors_for(&bus, reason).is_empty());
            let after = inhibitors_for(&bus, reason);
            release(child);
            assert_eq!(
                after,
                Vec::<Inhibitor>::new(),
                "logind grants: the inhibitors for this reason once the guard dropped, \
                 a child spawned under the guard still running"
            );
            return;
        }
        Err(why) => why,
    };
    let refusal = inhibit(&bus, reason)
        .expect_err("the bus refuses this test's own inhibitor, as it refused the adapter's");
    let zbus::Error::MethodError(_, _, reply) = &refusal else {
        panic!("the bus answered Inhibit with no error reply: {refusal}");
    };
    let sender = reply.header().sender().map(|sender| sender.to_string());
    match logind_owner(&bus) {
        Some(_) if sender.as_deref() == Some(BUS_DRIVER) => {
            assert_eq!(
                why,
                Unavailable(format!(
                    "logind was not reached on the system bus: {refusal}"
                )),
                "the bus's policy refuses the call while logind runs: the adapter's answer"
            );
        }
        Some(logind) => {
            assert_eq!(
                sender,
                Some(logind),
                "logind refuses: the process owning logind's name sent the refusal"
            );
            assert_eq!(
                why,
                Unavailable(format!("logind refused the inhibitor: {refusal}")),
                "logind refuses: the adapter's answer"
            );
        }
        None => {
            assert_eq!(
                sender.as_deref(),
                Some(BUS_DRIVER),
                "no logind: the bus driver answered for the name no process owns"
            );
            assert_eq!(
                why,
                Unavailable(format!(
                    "logind was not reached on the system bus: {refusal}"
                )),
                "no logind: the adapter's answer"
            );
        }
    }
}
