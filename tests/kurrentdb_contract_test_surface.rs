//! Spec 47, criterion 3 - the KurrentDB contract test surface.
//!
//! Criterion 3 says: "the existing KurrentDB contract test compiles and runs (or
//! gracefully skips without a container runtime) in BOTH lanes." Retiring the
//! build-time `kurrentdb` cargo feature un-gated the adapter module - and with it the
//! backend-agnostic contract test `passes_the_contract` in
//! `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs`, which drives `eventstore::contract::assert_contract`
//! against a real KurrentDB container. That test now compiles into EVERY lane's
//! `cargo test`, and it must keep two properties, neither of which any other test pins:
//!
//!   1. UN-GATED IN EVERY LANE. The adapter file carries no `#[cfg(feature = ...)]`
//!      gate, so both the adapter AND its contract test compile and run in the default
//!      lane and the `--no-default-features` lane alike. The sibling
//!      `kurrentdb_always_available.rs` scan only catches the ONE retired feature name
//!      (`kurrentdb`); a regression that re-gated the contract test behind ANY other
//!      cargo feature (`#[cfg(feature = "integration")]`, say) would silently compile it
//!      out of a lane and slip past that scan. This guard forbids ANY cargo-feature gate
//!      in the adapter file, so the contract test cannot be conditionally compiled out of
//!      either lane again.
//!
//!   2. SKIPS ONLY WITHOUT A CONTAINER RUNTIME. The contract test needs a container runtime;
//!      a CI box without one (the common case) must stay GREEN, and a box with one must never
//!      report green with the test unrun. The test runs through the shared fixture
//!      `with_kurrentdb` (tests/common/fixtures/kurrentdb.rs), which starts its container
//!      through `start_kurrentdb` and hands a failed start to the one decision `start_failure`.
//!      It skips - a stderr notice, `None`, and `with_kurrentdb` RETURNS without running the
//!      test's body - only when no runtime is reachable: DOCKER_HOST unset and no socket where
//!      the client looks for one, or a socket that refuses the connection. Every other failure
//!      (a startup timeout, an image pull, a port binding, a daemon error) fails the test with
//!      its error, and so does every failure once DOCKER_HOST names a runtime, since the test
//!      gate's container snippet exports it exactly when it found one. Both regressions are
//!      SILENT: a skip turned into a panic reds only on the boxes that lack a runtime, and a
//!      failure turned into a skip stays green everywhere with the suites unrun, because the
//!      test runner hides a passing test's stderr. These guards fail at `cargo test` time
//!      instead.
//!
//! Deliberately NOT feature-gated and it touches no backend symbol: it reads the adapter
//! source as text (resolved from `CARGO_MANIFEST_DIR`, so it is CWD-independent) and drives
//! the fixture's decision on the client errors a failed start returns, identically in both
//! feature lanes, a real member of each lane's `cargo test` battery.

mod common;

use common::fixtures::{container_runtime, start_failure, StartFailure};
use common::repo::repo_text;
use testcontainers::bollard::errors::Error as BollardError;
use testcontainers::bollard::{Docker, API_DEFAULT_VERSION};
use testcontainers::core::error::{ClientError, WaitContainerError};
use testcontainers::TestcontainersError;

/// The body of the first `fn <name>` in `src`, from its opening `{` to the matching `}`
/// (brace-balanced, so nested blocks are included). Panics if the function or a balanced
/// body is absent - the guard is worthless if it silently matches nothing.
fn fn_body(src: &str, name: &str) -> String {
    let sig = format!("fn {name}");
    let at = src
        .find(&sig)
        .unwrap_or_else(|| panic!("`{sig}` not found in the adapter source"));
    let open = src[at..]
        .find('{')
        .map(|i| at + i)
        .unwrap_or_else(|| panic!("`{sig}` has no opening brace"));
    let mut depth = 0usize;
    for (idx, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return src[open..=open + idx].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("`{sig}` body has unbalanced braces");
}

/// CRITERION 3, part one - the backend-agnostic contract test is PRESENT and un-gated,
/// so it compiles and runs in BOTH lanes. `passes_the_contract` is the test that runs
/// `eventstore::contract::assert_contract` against a real KurrentDB; deleting it would
/// silently drop the store's only real-backend contract coverage. And the adapter file
/// must carry NO `#[cfg(feature = ...)]` gate: a cargo-feature predicate would compile
/// the adapter (and the contract test with it) OUT of whichever lane lacks that feature,
/// breaking "runs in BOTH lanes". Space-insensitive so `feature="x"` is caught too.
#[test]
fn the_contract_test_is_present_and_never_gated_on_a_cargo_feature() {
    let src = repo_text("crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs");

    assert!(
        src.contains("fn passes_the_contract"),
        "crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs must still define the backend-agnostic contract test \
         `passes_the_contract` (spec 47 criterion 3: the existing contract test compiles and runs \
         in both lanes) - it is gone"
    );

    let squeezed: String = src.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        !squeezed.contains("cfg(feature="),
        "the KurrentDB adapter (crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs) must carry NO `#[cfg(feature = ...)]` \
         gate: the build-time feature is retired (spec 47), so a cargo-feature predicate here would \
         compile the adapter and its `passes_the_contract` contract test OUT of the lane that lacks \
         the feature, breaking 'compiles and runs in BOTH lanes'"
    );
}

/// CRITERION 3, part two - the contract test SKIPS ONLY WITHOUT A CONTAINER RUNTIME. Every
/// KurrentDB-backed test runs through the one shared fixture `with_kurrentdb`, which boots its
/// server through `start_kurrentdb` (tests/common/fixtures/kurrentdb.rs): the start is never
/// force-unwrapped, a failed one goes to `start_failure` with the DOCKER_HOST the process sees,
/// that decision's `Skip` arm is the fixture's ONLY skip - the one no-runtime notice and the one
/// `None` - and its `Fail` arm panics with the start's error. `with_kurrentdb` returns on that
/// `None`, and `passes_the_contract` runs through `with_kurrentdb`. A regression in any of them is
/// SILENT (see the module header), so it is pinned here at `cargo test` time.
#[test]
fn the_contract_test_skips_only_without_a_container_runtime() {
    let squeeze = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let source = squeeze(&repo_text("tests/common/fixtures/kurrentdb.rs"));
    let fixture = squeeze(&fn_body(
        &repo_text("tests/common/fixtures/kurrentdb.rs"),
        "start_kurrentdb",
    ));
    let start_at = fixture.find("image.start()").unwrap_or_else(|| {
        panic!(
            "`start_kurrentdb` must attempt to start a container (`image.start()`) - it is the \
             step that can be absent, and the skip is its failure path; found neither"
        )
    });

    // The start result must be handled, never force-unwrapped: `.expect(` / `.unwrap(`
    // applied to the start would PANIC a runtime-less box instead of skipping it.
    for forced in ["image.start()).unwrap", "image.start()).expect"] {
        assert!(
            !fixture.contains(forced),
            "`start_kurrentdb` must not force-unwrap the container start ({forced}...): a box \
             with no container runtime must SKIP (spec 47 criterion 3), not panic"
        );
    }

    // Within the start's `match`, from the call to the `};` that closes it, the error goes to
    // the one decision with the DOCKER_HOST the process sees; its `Skip` arm prints the
    // no-runtime notice and returns `None`, and its `Fail` arm panics with the error.
    let rest = &fixture[start_at..];
    let window = &rest[..rest.find("};").unwrap_or(rest.len())];
    assert!(
        window.contains(
            "Err(e)=>matchstart_failure(std::env::var(\"DOCKER_HOST\").ok().as_deref(),&e){"
        ),
        "a failed container start must go to the one decision `start_failure`, handed the \
         DOCKER_HOST this process sees and the start's error; the start handling is:\n{window}"
    );
    let (skip, fail) = window
        .split_once("StartFailure::Skip=>")
        .and_then(|(_, arms)| arms.split_once("StartFailure::Fail=>"))
        .unwrap_or_else(|| {
            panic!(
                "the decision's `Skip` arm must come before its `Fail` arm; the start handling \
                 is:\n{window}"
            )
        });
    assert!(
        skip.contains("eprintln!(\"skipping:noKurrentDBcontainer(nocontainerruntimeisreachable)")
            && skip.contains("returnNone;"),
        "the `Skip` arm must print the no-runtime notice and return `None`; it is:\n{skip}"
    );
    assert!(
        fail.contains("panic!(") && fail.contains("{e}"),
        "the `Fail` arm must fail the test with the start's error; it is:\n{fail}"
    );
    for (skip_mark, what) in [("returnNone", "`None`"), ("skipping:", "skip notice")] {
        assert_eq!(
            source.matches(skip_mark).count(),
            1,
            "the decision's `Skip` arm must be the fixture's only {what}: any other is a skip \
             path that does not ask whether a runtime is reachable"
        );
    }

    // The shared lifecycle boots through that start and returns on its `None`, and the contract
    // test runs through that lifecycle.
    let lifecycle = squeeze(&fn_body(
        &repo_text("tests/common/fixtures/kurrentdb.rs"),
        "with_kurrentdb",
    ));
    assert!(
        lifecycle.contains("start_kurrentdb(&rt)else{return;"),
        "`with_kurrentdb` must boot its server through `start_kurrentdb` and return when it yields \
         no container (spec 47 criterion 3); its body is:\n{lifecycle}"
    );
    let contract = squeeze(&fn_body(
        &repo_text("crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs"),
        "passes_the_contract",
    ));
    assert!(
        contract.contains("with_kurrentdb(|"),
        "`passes_the_contract` must run through the shared `with_kurrentdb` fixture, which skips \
         when no container runtime is reachable (spec 47 criterion 3); its body is:\n{contract}"
    );
}

/// The DOCKER_HOST the test gate's container snippet exports when it finds the podman socket.
const GATE_DOCKER_HOST: &str = "unix:///run/user/1000/podman/podman.sock";

/// The two failed starts that mean no container runtime is reachable, each the error the client
/// really returns: no socket where it looks for one (docker's default path, when DOCKER_HOST is
/// unset), and a socket that exists but refuses the connection, which arrives as the failure of
/// a start's first call to the runtime, the container's creation.
fn no_runtime_failures() -> [TestcontainersError; 2] {
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name).to_str().unwrap().to_string();
    let no_socket = Docker::connect_with_unix(&path("absent.sock"), 120, API_DEFAULT_VERSION)
        .expect_err("no socket where nothing created one");
    // The socket file outlives its listener, so the path exists and nothing accepts on it.
    drop(std::os::unix::net::UnixListener::bind(path("refusing.sock")).unwrap());
    let docker = Docker::connect_with_unix(&path("refusing.sock"), 120, API_DEFAULT_VERSION)
        .expect("a client for a socket that exists");
    let refused = container_runtime()
        .block_on(docker.create_container(None, Default::default()))
        .expect_err("a socket nothing listens on refuses the connection");
    [
        TestcontainersError::Client(ClientError::Init(no_socket)),
        TestcontainersError::Client(ClientError::CreateContainer(refused)),
    ]
}

/// Failed starts on a runtime the client reached, one per way a start fails there: the server
/// never reports ready in time, the image does not pull, the port does not bind, the daemon
/// refuses to create the container.
fn reachable_runtime_failures() -> [TestcontainersError; 4] {
    let daemon = |message: &str| BollardError::DockerResponseServerError {
        status_code: 500,
        message: message.to_string(),
    };
    [
        TestcontainersError::WaitContainer(WaitContainerError::StartupTimeout),
        TestcontainersError::Client(ClientError::PullImage {
            descriptor: "kurrentplatform/kurrentdb:latest".to_string(),
            err: daemon("pull access denied"),
        }),
        TestcontainersError::Client(ClientError::StartContainer(daemon(
            "port is already allocated",
        ))),
        TestcontainersError::Client(ClientError::CreateContainer(daemon(
            "no space left on device",
        ))),
    ]
}

/// A start that reached no container runtime skips the test while DOCKER_HOST is unset, or
/// empty, which the test gate's container snippet reads as unset too.
#[test]
fn the_fixture_skips_a_start_that_reaches_no_container_runtime() {
    for error in no_runtime_failures() {
        for unset in [None, Some("")] {
            assert_eq!(
                start_failure(unset, &error),
                StartFailure::Skip,
                "DOCKER_HOST {unset:?}: {error}"
            );
        }
    }
}

/// A start that fails on a runtime the client reached fails the test, never a skip.
#[test]
fn the_fixture_fails_a_start_that_fails_on_a_reachable_runtime() {
    for error in reachable_runtime_failures() {
        assert_eq!(start_failure(None, &error), StartFailure::Fail, "{error}");
    }
}

/// Once DOCKER_HOST names a runtime - the test gate's container snippet exports it exactly when
/// it found one - no failed start skips the test, not even one that reached nothing there.
#[test]
fn the_fixture_never_skips_once_docker_host_names_a_runtime() {
    for error in no_runtime_failures()
        .into_iter()
        .chain(reachable_runtime_failures())
    {
        assert_eq!(
            start_failure(Some(GATE_DOCKER_HOST), &error),
            StartFailure::Fail,
            "{error}"
        );
    }
}
