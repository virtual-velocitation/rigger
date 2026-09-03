//! Spec 48, criterion 5 - PROJECTIONS STAY LOCAL (the projection boundary).
//!
//! Which event-store backend a project uses is CONFIGURATION resolved by one authority
//! (`resolve_store`), and that authority governs the EVENT LOG only. `graph.db` and
//! `progress.db` are per-machine, rebuildable PROJECTIONS of the log, not the log: they
//! remain embedded sqlite under the local `.rigger/` regardless of the event-store choice.
//! Select the server backend and the shared truth (the log) moves to the server, while each
//! machine keeps projecting it into its OWN local sqlite files. This file pins that boundary
//! two ways:
//!
//!   1. STRUCTURALLY (always on, no server needed) - the projections are opened by the LOCAL
//!      sqlite constructors (`Projector::open` for the graph, `Store::open` for progress), and
//!      a projection path is NEVER routed through `resolve_store` (the configurable, possibly
//!      server-backed event-log resolver) or the server adapter. So no command can ever point a
//!      projection at the server, whatever the event-store selection.
//!   2. AT RUNTIME (against a REACHABLE server booted in a container, gracefully skipped when no
//!      container runtime is reachable) - with ONLY the server configured (`KURRENTDB_CONN`, no
//!      flag), the lightest projection-touching commands still land their projection under the
//!      local `.rigger/` as sqlite while NO local `events.db` is fabricated (the log is the
//!      server's): `graph build` for `graph.db`, and `progress` for `progress.db`.
//!
//! This criterion OWNS the projection boundary. The uniform wiring, the precedence, the secret
//! channel, and verbatim pass-through are pinned by their own criteria; here we only prove that
//! the store choice governs the event LOG and never redirects a local projection.

use std::path::Path;
use std::process::Command;

// =======================================================================================
// Structural boundary: projections open as LOCAL sqlite; the event-log resolver and the
// server adapter never touch a projection path. Always on - no container required.
// =======================================================================================

/// The production source of the CLI composition root (`src/main.rs`), with the trailing
/// `#[cfg(test)] mod tests { ... }` unit-test module stripped. The projection boundary rule
/// governs SHIPPING code: test code legitimately opens throwaway sqlite projections directly,
/// and must not be scanned as a command's construction path.
fn production_main_rs() -> String {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
        .expect("read src/main.rs");
    match src.find("#[cfg(test)]\nmod tests {") {
        // Everything before the unit-test module (which runs to EOF). Falls back to the whole
        // source when the marker is absent, so a future reshaping never makes this scan pass by
        // silently scanning nothing.
        Some(cut) => src[..cut].to_string(),
        None => src,
    }
}

/// The code portion of a production line: the text before any trailing `//` line comment, and
/// empty for a whole-line comment. Prose that MENTIONS a projection beside `resolve_store` is
/// harmless; only CODE that couples them is a defect, so the structural scan looks at code only.
fn code_of(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return "";
    }
    line.split("//").next().unwrap_or(line)
}

#[test]
fn the_event_log_resolver_and_server_adapter_never_touch_a_projection_path() {
    let src = production_main_rs();

    // Every production CODE line that names a projection store file.
    let projection_lines: Vec<(usize, &str)> = src
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, code_of(l)))
        .filter(|(_, code)| code.contains("graph.db") || code.contains("progress.db"))
        .collect();

    // Sanity: if the projections vanished from main.rs entirely, this scan would vacuously pass
    // - so pin that they are actually there to be governed.
    assert!(
        !projection_lines.is_empty(),
        "production main.rs must reference the graph.db / progress.db projections; found none - \
         did the scan break?"
    );

    for (line_no, code) in &projection_lines {
        assert!(
            !code.contains("resolve_store("),
            "line {line_no}: a projection (graph.db / progress.db) must NEVER be routed through \
             `resolve_store` - that resolver is the configurable, possibly server-backed EVENT-LOG \
             authority, and the store choice governs the event log ONLY. A projection stays local \
             sqlite:\n  {}",
            code.trim()
        );
        assert!(
            !code.contains("kurrentdb"),
            "line {line_no}: a projection (graph.db / progress.db) must NEVER be opened against the \
             server adapter - projections are per-machine LOCAL sqlite regardless of the \
             event-store choice:\n  {}",
            code.trim()
        );
    }
}

#[test]
fn the_graph_and_progress_projections_open_via_the_local_sqlite_constructors() {
    let src = production_main_rs();

    // The graph projection is opened by the sqlite `Projector` at the local `.rigger/graph.db`
    // (`db_path` resolves under RIGGER_DIR). If a regression redirected it - e.g. through
    // `resolve_store` - this canonical local construction would disappear.
    assert!(
        src.contains(r#"Projector::open(&db_path("graph.db")"#),
        "the graph projection must be opened by the LOCAL sqlite Projector at .rigger/graph.db \
         (`Projector::open(&db_path(\"graph.db\") ...)`); the canonical local construction is gone"
    );

    // The progress projection is opened by the sqlite `Store` at the local `.rigger/progress.db`.
    assert!(
        src.contains(r#"Store::open(&db_path("progress.db")"#),
        "the progress projection must be opened by the LOCAL sqlite Store at .rigger/progress.db \
         (`Store::open(&db_path(\"progress.db\") ...)`); the canonical local construction is gone"
    );
}

// =======================================================================================
// Runtime boundary: with ONLY the server configured, the projection lands LOCALLY as sqlite
// while the event LOG goes to the server. Gracefully skipped when no container runtime.
// =======================================================================================

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

/// The project identity the binary resolves for `root` (the git top-level basename, or the
/// tracked `.rigger/project.id`) - the identity that namespaces the LOCAL progress projection,
/// so a read-back binds the exact stream the courier's write landed in.
fn store_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Boot a single-node insecure KurrentDB in a container and return (container, conn). Returns
/// `None` - so the caller skips cleanly - when no container runtime is reachable, exactly as the
/// backend-agnostic contract suite and the sibling wiring tests do.
fn start_kurrentdb(
    rt: &tokio::runtime::Runtime,
) -> Option<(
    testcontainers::ContainerAsync<testcontainers::GenericImage>,
    String,
)> {
    use testcontainers::core::{IntoContainerPort, WaitFor};
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{GenericImage, ImageExt};

    let image = GenericImage::new("kurrentplatform/kurrentdb", "latest")
        .with_wait_for(WaitFor::message_on_stdout("IS LEADER"))
        .with_mapped_port(21135, 2113.tcp())
        .with_env_var("KURRENTDB_INSECURE", "true")
        .with_env_var("KURRENTDB_MEM_DB", "true")
        .with_env_var("KURRENTDB_RUN_PROJECTIONS", "None")
        .with_env_var("KURRENTDB_NODE_PORT", "2113");
    let container = match rt.block_on(image.start()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping projection-boundary runtime test (no container runtime?): {e}");
            return None;
        }
    };
    // The server needs a moment past the readiness log line before it accepts gRPC.
    std::thread::sleep(std::time::Duration::from_secs(2));
    Some((
        container,
        "kurrentdb://localhost:21135?tls=false".to_string(),
    ))
}

/// A throwaway project that is its OWN git repo (so the identity is stable and the ingest walk
/// roots at the fixture), configured for the server-backed store purely by `KURRENTDB_CONN`.
fn server_project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    let git = |args: &[&str]| {
        let _ = Command::new("git").args(args).current_dir(root).status();
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    project
}

#[test]
fn graph_build_against_the_server_keeps_graph_db_local_and_the_log_on_the_server() {
    use rigger::contextgraph::sqlite::Projector;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let Some((container, conn)) = start_kurrentdb(&rt) else {
        return; // no container runtime: gracefully skipped
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let project = server_project();
        let root = project.path();
        // A small source file so the default lane has something to parse; the light lane ingests
        // nothing but still CREATES the graph store, so this test asserts the same boundary in
        // both lanes.
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("lib.rs"), "pub fn hello() {}\n").unwrap();

        // Build the graph with ONLY the server configured (no `--eventstore` flag) - the surface a
        // server-backed project uses. The event LOG is resolved through `resolve_store` (-> server);
        // the graph PROJECTION is opened directly as local sqlite.
        let out = common::rigger_courier()
            .args(["graph", "build"])
            .current_dir(root)
            .env("KURRENTDB_CONN", &conn)
            .env("RIGGER_NO_DASH", "1")
            .output()
            .expect("spawn rigger graph build");
        assert!(
            out.status.success(),
            "graph build must succeed against the server: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // The graph PROJECTION landed LOCALLY under `.rigger/` ...
        let graph_db = root.join(".rigger").join("graph.db");
        assert!(
            graph_db.exists(),
            "graph.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - projections are per-machine and stay local"
        );
        // ... and it is a genuine local sqlite projection (it opens through the sqlite Projector).
        Projector::open(graph_db.to_str().unwrap(), "probe")
            .expect("the local graph.db must be a valid sqlite projection");

        // ... while the EVENT LOG went to the SERVER: no local `events.db` was fabricated. This is
        // exactly the boundary - the store choice governs the log, not the projection.
        assert!(
            !root.join(".rigger").join("events.db").exists(),
            "a server-configured `graph build` must NOT create a local .rigger/events.db - the \
             event log is the server's; only the projection is local"
        );
    }));

    let _ = rt.block_on(container.rm());
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn progress_against_the_server_keeps_progress_db_local_and_the_log_on_the_server() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let Some((container, conn)) = start_kurrentdb(&rt) else {
        return; // no container runtime: gracefully skipped
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let project = server_project();
        let root = project.path();

        // A bare `rigger progress` - no `--eventstore` flag - the exact surface a worker uses. It
        // resolves the run store (-> server, read-only, to scope the report) and appends to the
        // SEPARATE progress store, which must stay LOCAL sqlite. `XDG_STATE_HOME` is redirected
        // to a per-call temp dir (spec 62, "couriers count as activity"): `progress` now
        // refreshes the machine-global instance registry too, so an unredirected call here would
        // otherwise seed a phantom, since-deleted-tempdir entry into the operator's real
        // `~/.local/state/rigger/instances`.
        let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
        let out = common::rigger_courier()
            .args([
                "progress",
                "u5-projections/implementer#0",
                "folded a fixture into the local projection",
            ])
            .current_dir(root)
            .env("KURRENTDB_CONN", &conn)
            .env("RIGGER_NO_DASH", "1")
            .env("XDG_STATE_HOME", state.path())
            .output()
            .expect("spawn rigger progress");
        assert!(
            out.status.success(),
            "rigger progress must succeed against the server: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // The progress PROJECTION landed LOCALLY under `.rigger/` ...
        let progress_db = root.join(".rigger").join("progress.db");
        assert!(
            progress_db.exists(),
            "progress.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - the progress store is a local projection, not the shared log"
        );

        // ... and the report is readable back FROM the LOCAL progress store (it never went to the
        // server), proving the write stayed local.
        let backend = Store::open(progress_db.to_str().unwrap())
            .expect("the local progress.db must be a valid sqlite store");
        let store = Namespaced::new(&backend, &store_identity(root));
        let events = store
            .read_stream(rigger::progress::STREAM, 0, Direction::Forward)
            .expect("read the local progress stream");
        assert!(
            events
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("folded a fixture")),
            "the progress report must live in the LOCAL progress.db (found {} event(s)); it must \
             NOT be redirected to the server",
            events.len()
        );

        // ... while the run EVENT LOG went to the SERVER: no local `events.db` was fabricated.
        assert!(
            !root.join(".rigger").join("events.db").exists(),
            "a server-configured `rigger progress` must NOT create a local .rigger/events.db - the \
             run log is the server's; only the progress projection is local"
        );
    }));

    let _ = rt.block_on(container.rm());
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
