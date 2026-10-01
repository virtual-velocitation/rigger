//! A throwaway KurrentDB server in a container, for every test that drives the KurrentDB
//! adapter against a real one: the adapter's own contract test and the root suites'
//! server-backed wiring tests.

use rigger::eventstore::kurrentdb::Store;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use testcontainers::bollard::models::PortBinding;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The server's gRPC port inside its container.
const KURRENTDB_PORT: u16 = 2113;

/// Run `body` against a throwaway KurrentDB server ([`start_kurrentdb`]), handed its connection
/// string, then remove the server's container whatever `body` did - on the runtime that started
/// it, since a container handle dropped outside its runtime aborts the test binary and leaves the
/// container running - and re-raise a panic `body` raised, so a failed assertion still fails the
/// test. When no container runtime is reachable `body` never runs and the test passes as skipped.
pub fn with_kurrentdb(body: impl FnOnce(&str)) {
    let rt = container_runtime();
    let Some((container, conn)) = start_kurrentdb(&rt) else {
        return; // no container runtime: gracefully skipped
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&conn)));
    let _ = rt.block_on(container.rm());
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// The runtime [`with_kurrentdb`] starts and removes its server's container on.
pub fn container_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("a runtime to drive the container")
}

/// Boot a single-node, insecure, in-memory KurrentDB and return it with a connection string
/// the adapter already accepts. The host side of the port is whichever one the container
/// runtime picks, read back from the container, so runs that overlap (a mutation sweep's
/// parallel copies, two units testing at once, two tests of one binary) never compete for
/// one. Returns `None` - after saying why on stderr, so the caller skips cleanly - when no
/// container runtime is reachable.
fn start_kurrentdb(rt: &tokio::runtime::Runtime) -> Option<(ContainerAsync<GenericImage>, String)> {
    let image = GenericImage::new("kurrentplatform/kurrentdb", "latest")
        .with_exposed_port(KURRENTDB_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stdout("IS LEADER"))
        .with_env_var("KURRENTDB_INSECURE", "true")
        .with_env_var("KURRENTDB_MEM_DB", "true")
        .with_env_var("KURRENTDB_RUN_PROJECTIONS", "None")
        .with_env_var("KURRENTDB_NODE_PORT", KURRENTDB_PORT.to_string())
        // An empty host port lets the runtime pick one. The address is explicit because podman
        // reports a picked port's address as empty unless one was asked for, and testcontainers
        // reads back only a binding whose address parses.
        .with_host_config_modifier(|host| {
            host.publish_all_ports = Some(false);
            host.port_bindings = Some(HashMap::from([(
                KURRENTDB_PORT.tcp().to_string(),
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(String::new()),
                }]),
            )]));
        });
    let container = match rt.block_on(image.start()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping: no KurrentDB container (no container runtime?): {e}");
            return None;
        }
    };
    match rt
        .block_on(container.get_host_port_ipv4(KURRENTDB_PORT))
        .map_err(|e| format!("the server's port was not published: {e}"))
        .and_then(|port| ready(format!("kurrentdb://localhost:{port}?tls=false")))
    {
        Ok(conn) => Some((container, conn)),
        Err(why) => {
            let _ = rt.block_on(container.rm());
            panic!("{why}");
        }
    }
}

/// `conn` once the adapter can connect to it. The readiness log line precedes gRPC accept and
/// the adapter connects eagerly with no retry, so its own connect is polled rather than a fixed
/// grace trusted.
fn ready(conn: String) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Store::open(&conn).is_err() {
        if Instant::now() >= deadline {
            return Err("KurrentDB never became ready".to_string());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(conn)
}
