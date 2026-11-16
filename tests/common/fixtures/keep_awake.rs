//! The fake buses the Linux keep-awake adapter's tests serve on private abstract sockets: a fake
//! logind and a fake desktop portal, the probes of a client's connection to one, and the calls
//! the adapter makes to each. `rigger-process` mounts this file twice: in its unit tests, where the
//! adapter's test module reads it, and in its graphical-session suite, which drives the adapter
//! through the crate's public API. It names no crate path, so both mount it alike. Linux alone, as
//! abstract socket names are.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::pin::Pin;
use std::sync::mpsc;
use std::thread::JoinHandle;
use zbus::connection::Builder;
use zbus::export::futures_core::Stream;
use zbus::export::serde::de::DeserializeOwned;
use zbus::zvariant::{Fd, ObjectPath, OwnedValue, Str, Type};

/// One call as a bus delivers it: destination, path, interface and member, and its body.
pub type Call<B> = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    B,
);

/// The body of a call to logind's `Inhibit`: the inhibitor it asks for, by what, who, why and
/// mode.
pub type LogindInhibit = (String, String, String, String);

/// The body of a call to the portal's `Inhibit`: the parent window, the flags and the options
/// of the inhibitor it asks for.
pub type PortalInhibit = (String, u32, HashMap<String, OwnedValue>);

/// The bus name the bus driver answers under: a call addressed to it is a call to the bus itself.
const BUS_DRIVER: &str = "org.freedesktop.DBus";

/// The unique name a fake bus gives the one client it serves.
const CLIENT: &str = ":1.1";

/// How a fake bus answers each call other than a call to the bus itself.
#[derive(Clone, Copy)]
pub enum Answer {
    /// It answers none.
    Nothing,
    /// With a descriptor, as logind grants its inhibitor.
    Descriptor,
    /// With a request handle, as the portal answers its `Inhibit`, and then the request's
    /// `Response` signal carrying the code given, as the portal's backend ends the request: 0
    /// when it registered the inhibitor, any other code when it did not; no `Response` at all
    /// for none.
    Handle(Option<u32>),
    /// With an error, refusing the call.
    Refusal,
}

/// A fake bus serving the one connection its listener accepts.
pub struct FakeBus<B> {
    /// The bus's own end of that connection, once accepted, to probe the client's end through.
    pub end: mpsc::Receiver<UnixStream>,
    /// The calls the bus received, handed back once the client closes the connection.
    pub calls: JoinHandle<Vec<Call<B>>>,
}

/// A private bus for `role`, listening under an abstract name of this process and moment
/// alone, and the address a client connects to it by.
pub fn private_bus(role: &str) -> (UnixListener, String) {
    let name = format!(
        "rigger-keep-awake-{role}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let address = SocketAddr::from_abstract_name(name.as_bytes()).expect("an abstract name");
    (
        UnixListener::bind_addr(&address).expect("the private bus listens"),
        format!("unix:abstract={name}"),
    )
}

/// A fake bus on `listener`, serving the one connection it accepts as the bus itself: it
/// answers each call to the bus itself, the connection's `Hello` with [`CLIENT`] and any other
/// (the `AddMatch` and `RemoveMatch` of a subscription to signals) with nothing, records every
/// other call with its body read as `B` and answers it as `answer` says, and hands back the
/// calls it recorded once the connection closes.
pub fn fake_bus<B>(listener: UnixListener, answer: Answer) -> FakeBus<B>
where
    B: Type + DeserializeOwned + Send + 'static,
{
    let (accepted, end) = mpsc::channel();
    let calls = std::thread::spawn(move || {
        let (stream, _) = listener
            .accept()
            .expect("the adapter connects to the private bus");
        // A test that probes no end has dropped its receiver.
        let _ = accepted.send(stream.try_clone().expect("the bus's end duplicates"));
        stream
            .set_nonblocking(true)
            .expect("the accepted stream turns non-blocking");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the fake bus's runtime");
        runtime.block_on(async {
            let stream = tokio::net::UnixStream::from_std(stream).expect("the accepted stream");
            let mut messages = Builder::unix_stream(stream)
                .server(zbus::Guid::generate())
                .expect("a bus-side connection")
                .p2p()
                .build_message_stream()
                .await
                .expect("the adapter completes the handshake");
            let bus = zbus::Connection::from(&messages);
            let granted = std::fs::File::open("/dev/null").expect("a descriptor to grant");
            let mut calls = Vec::new();
            while let Some(Ok(message)) =
                std::future::poll_fn(|cx| Pin::new(&mut messages).poll_next(cx)).await
            {
                let header = message.header();
                if header
                    .destination()
                    .is_some_and(|name| name.as_str() == BUS_DRIVER)
                {
                    let answered = if header.member().is_some_and(|member| member == "Hello") {
                        bus.reply(&header, &CLIENT).await
                    } else {
                        bus.reply(&header, &()).await
                    };
                    answered.expect("the fake bus answers the call to the bus itself");
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
                        .expect("the call's body has the type the bus serves"),
                ));
                let answered = match answer {
                    Answer::Nothing => continue,
                    Answer::Descriptor => bus.reply(&header, &Fd::from(&granted)).await,
                    Answer::Handle(response) => {
                        let handle = ObjectPath::from_static_str_unchecked(
                            "/org/freedesktop/portal/desktop/request/1_1/rigger",
                        );
                        bus.reply(&header, &handle)
                            .await
                            .expect("the fake portal returns the request's handle");
                        match response {
                            Some(code) => {
                                bus.emit_signal(
                                    Some(CLIENT),
                                    handle,
                                    "org.freedesktop.portal.Request",
                                    "Response",
                                    &(code, HashMap::<String, OwnedValue>::new()),
                                )
                                .await
                            }
                            None => Ok(()),
                        }
                    }
                    Answer::Refusal => {
                        bus.reply_error(
                            &header,
                            "org.freedesktop.DBus.Error.AccessDenied",
                            &"the inhibitor is refused",
                        )
                        .await
                    }
                };
                answered.expect("the fake bus answers the call");
            }
            calls
        })
    });
    FakeBus { end, calls }
}

/// The bus's own end of the connection a client makes to the fake bus [`FakeBus::end`] hands it
/// from, once the client connects within 5 seconds; a panic with the message `connects` when it
/// never does, so a client that never connects fails its test instead of hanging it.
pub fn connected_end(end: &mpsc::Receiver<UnixStream>, connects: &str) -> UnixStream {
    end.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| panic!("{connects}"))
}

/// The client's end of the connection whose bus end is `end`: open, or closed once the client
/// closed it, having read all the bus sent it (an orderly end) or not (the kernel resets the
/// bus's end then, as a client closing with a signal or a reply still unread does). The probe
/// peeks without waiting, so it consumes nothing the fake bus reads.
pub fn client_end(end: &UnixStream) -> String {
    let mut byte = 0u8;
    // SAFETY: the buffer is the one byte `byte` holds, and the descriptor is `end`'s, borrowed
    // for the call.
    let peeked = unsafe {
        libc::recv(
            end.as_raw_fd(),
            (&raw mut byte).cast(),
            1,
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    let error = std::io::Error::last_os_error();
    match peeked {
        0 => "closed".to_string(),
        -1 if error.kind() == ErrorKind::ConnectionReset => "closed".to_string(),
        -1 if error.kind() == ErrorKind::WouldBlock => "open".to_string(),
        _ => format!("peeked {peeked}: {error}"),
    }
}

/// Whether a client's connection waits on `listener`, which a probe takes without waiting.
pub fn pending(listener: &UnixListener) -> Result<(), ErrorKind> {
    listener
        .set_nonblocking(true)
        .expect("the listener turns non-blocking");
    listener.accept().map(|_| ()).map_err(|error| error.kind())
}

/// The adapter's call to logind's `Inhibit` for `reason`.
pub fn logind_inhibit(reason: &str) -> Call<LogindInhibit> {
    (
        Some("org.freedesktop.login1".to_string()),
        Some("/org/freedesktop/login1".to_string()),
        Some("org.freedesktop.login1.Manager".to_string()),
        Some("Inhibit".to_string()),
        (
            "sleep:idle".to_string(),
            "rigger".to_string(),
            reason.to_string(),
            "block".to_string(),
        ),
    )
}

/// The adapter's call to the portal's `Inhibit` for `reason`: no parent window, the flags of
/// suspend (4) and idle (8), and the reason as its one option.
pub fn portal_inhibit(reason: &str) -> Call<PortalInhibit> {
    (
        Some("org.freedesktop.portal.Desktop".to_string()),
        Some("/org/freedesktop/portal/desktop".to_string()),
        Some("org.freedesktop.portal.Inhibit".to_string()),
        Some("Inhibit".to_string()),
        (
            String::new(),
            12,
            HashMap::from([("reason".to_string(), OwnedValue::from(Str::from(reason)))]),
        ),
    )
}
