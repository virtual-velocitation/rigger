//! Spec 48, criterion 1 - the SINGLE AUTHORITY behind the event-store port.
//!
//! Which backend a project uses is CONFIGURATION resolved by ONE authority every command
//! shares, never a per-command flag only one command honors. This file pins that two ways:
//!
//!   1. STRUCTURALLY - the sqlite event-log constructor (`Store::open`) appears at exactly
//!      one call site, inside `resolve_store`, so no command can hardcode a second store.
//!   2. AT RUNTIME - a command invoked in a project configured for the server-backed store
//!      resolves THAT store (not a local sqlite file), verified against a real server in a
//!      container and gracefully skipped when no container runtime is reachable.
//!
//! This criterion OWNS the uniform WIRING. The internal precedence among the configuration
//! sources, the secret channel, verbatim pass-through, and the projection boundary are pinned
//! by their own criteria; here we only prove that every command constructs its backend
//! through the one resolver, and that the resolver genuinely reaches a server.

use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------------------
// Structural single-authority: the sqlite event-log constructor lives at exactly one site.
// ---------------------------------------------------------------------------------------

/// The production source of the CLI composition root (`src/main.rs`), with the trailing
/// `#[cfg(test)] mod tests { ... }` unit-test module stripped. The single-authority rule
/// governs SHIPPING code: test code legitimately opens throwaway sqlite stores (`:memory:`,
/// temp files) directly, and must not be counted as a command's construction path.
fn production_main_rs() -> String {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
        .expect("read src/main.rs");
    strip_unit_test_module(&src)
}

/// Everything before the file's `#[cfg(test)]\nmod tests {` marker (the unit tests are the
/// final block of `src/main.rs`, running to EOF). Falls back to the whole source when the
/// marker is absent, so a future reshaping of the tests never makes this scan silently pass.
fn strip_unit_test_module(src: &str) -> String {
    match src.find("#[cfg(test)]\nmod tests {") {
        Some(cut) => src[..cut].to_string(),
        None => src.to_string(),
    }
}

/// The nearest enclosing top-level function for source line `idx` (0-based): the last line
/// at or above it that opens a top-level `fn` (column 0). The event-log construction lives in
/// a top-level function, so this resolves which one owns a given `Store::open`.
fn enclosing_fn(lines: &[&str], idx: usize) -> Option<String> {
    for line in lines[..=idx].iter().rev() {
        for prefix in [
            "fn ",
            "pub fn ",
            "async fn ",
            "pub async fn ",
            "pub(crate) fn ",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// A production line that constructs the sqlite EVENT-LOG backend: it calls `Store::open(`
/// but is neither the qualified server constructor (`kurrentdb::Store::open`) nor a local
/// PROJECTION store (`progress.db` / `progress_db`, the per-machine sibling that stays sqlite
/// regardless of the event-log choice, owned by the projection-boundary criterion).
fn is_sqlite_event_log_open(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    line.contains("Store::open(")
        && !line.contains("kurrentdb::Store::open")
        && !line.to_lowercase().contains("progress")
}

#[test]
fn the_sqlite_event_log_backend_is_constructed_at_exactly_one_call_site() {
    let src = production_main_rs();
    let lines: Vec<&str> = src.lines().collect();
    let sites: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_sqlite_event_log_open(l))
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect();

    assert_eq!(
        sites.len(),
        1,
        "the sqlite event-log constructor (Store::open) must appear at EXACTLY one call site \
         (inside resolve_store); found {}:\n{:#?}",
        sites.len(),
        sites
    );

    let (line_no, _) = &sites[0];
    let owner = enclosing_fn(&lines, line_no - 1);
    // The one sqlite `Store::open` lives in `open_sqlite_store`, the resolver's dedicated
    // sqlite constructor: `resolve_store` boxes it as the port for every command, and the
    // local identity migration - which needs the concrete `Store` for its stream renames -
    // also constructs through it, so the backend is built at a single call site.
    assert_eq!(
        owner.as_deref(),
        Some("open_sqlite_store"),
        "the one sqlite event-log Store::open must live inside `fn open_sqlite_store` (the \
         resolver's sole sqlite constructor), but it is in {owner:?} (line {line_no})"
    );
}

#[test]
fn the_single_resolver_exists_and_the_old_per_command_helper_is_retired() {
    let src = production_main_rs();
    assert!(
        src.contains("fn resolve_store("),
        "resolve_store must be the single event-log backend constructor"
    );
    assert!(
        src.contains("fn open_sqlite_store("),
        "open_sqlite_store must be the resolver's single sqlite constructor"
    );
    // The pre-48 `open_store` helper hardcoded the sqlite adapter and only `run` used it; its
    // survival would mean a second construction path exists beside the resolver.
    assert!(
        !src.contains("fn open_store("),
        "the pre-48 open_store per-command helper must be replaced by resolve_store"
    );
}

// ---------------------------------------------------------------------------------------
// Runtime wiring: a command configured for the server-backed store resolves THAT store.
// ---------------------------------------------------------------------------------------

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::rigger_bin;

/// The project identity the binary resolves for `root` (the git top-level basename, or the
/// tracked `.rigger/project.id`), mirrored here so a read-back of the server binds the exact
/// `proj-<id>-run` stream a courier's write landed in.
fn run_stream_identity(root: &Path) -> String {
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
/// `None` - so the caller skips cleanly - when no container runtime is reachable, exactly as
/// the backend-agnostic contract suite does.
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
        .with_mapped_port(21134, 2113.tcp())
        .with_env_var("KURRENTDB_INSECURE", "true")
        .with_env_var("KURRENTDB_MEM_DB", "true")
        .with_env_var("KURRENTDB_RUN_PROJECTIONS", "None")
        .with_env_var("KURRENTDB_NODE_PORT", "2113");
    let container = match rt.block_on(image.start()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping server-wiring test (no container runtime?): {e}");
            return None;
        }
    };
    // The server needs a moment past the readiness log line before it accepts gRPC.
    std::thread::sleep(std::time::Duration::from_secs(2));
    Some((
        container,
        "kurrentdb://localhost:21134?tls=false".to_string(),
    ))
}

/// Open the server store as a namespaced port, retrying briefly while it finishes coming up
/// (the adapter connects eagerly). Panics only after the deadline, matching the contract test.
fn open_server(conn: &str) -> rigger::eventstore::kurrentdb::Store {
    use rigger::eventstore::kurrentdb::Store;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut store = Store::open(conn);
    while store.is_err() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(500));
        store = Store::open(conn);
    }
    store.expect("KurrentDB never became ready")
}

#[test]
fn a_courier_in_a_project_configured_for_the_server_resolves_the_server_store() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::{Direction, EventStore};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let Some((container, conn)) = start_kurrentdb(&rt) else {
        return; // no container runtime: gracefully skipped
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // A throwaway project, its own git repo (so the identity is stable), configured for the
        // server-backed store purely by the KURRENTDB_CONN environment - no per-command flag.
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let _ = Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status();
        std::fs::create_dir_all(root.join(".rigger")).unwrap();

        // A bare courier - `rigger emit`, no `--eventstore` flag - the exact surface a worker's
        // self-report uses. Before this criterion it wrote to LOCAL sqlite; now it must resolve
        // the shared server the project is configured for.
        let out = Command::new(rigger_bin())
            .args([
                "emit",
                "DecisionMade",
                r#"{"id":"d-server-wire","summary":"resolved through the server","governs":["src/lib.rs"],"supersedes":""}"#,
            ])
            .current_dir(root)
            .env("KURRENTDB_CONN", &conn)
            .env("RIGGER_NO_DASH", "1")
            .output()
            .expect("spawn rigger emit");
        assert!(
            out.status.success(),
            "rigger emit must succeed against the server: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // The event log went to the SERVER, so NO local sqlite event log was fabricated. (The
        // graph projection stays local - that boundary is another criterion's - so we assert
        // specifically on events.db, the event LOG.)
        assert!(
            !root.join(".rigger").join("events.db").exists(),
            "a server-configured courier must NOT create a local .rigger/events.db - that is the \
             state-fracture this criterion closes"
        );

        // And the decision is readable back FROM the server, in the project's own stream.
        let backend = open_server(&conn);
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        let events = store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .expect("read the run stream from the server");
        assert!(
            events
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("d-server-wire")),
            "the courier's DecisionMade must be on the server, proving it resolved the server \
             store (found {} event(s))",
            events.len()
        );
    }));

    let _ = rt.block_on(container.rm());
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn a_server_courier_in_a_nested_worktree_files_under_the_owning_root_identity() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::{Direction, EventStore};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let Some((container, conn)) = start_kurrentdb(&rt) else {
        return; // no container runtime: gracefully skipped
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // A main repo with one commit (so `git worktree add` has a base), configured for the
        // server-backed store purely by the KURRENTDB_CONN environment - no per-command flag.
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            assert!(ok, "git {args:?} must succeed");
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "base",
        ]);

        // A git-LINKED worktree nested under the repo (the deterministic scratch-root shape a run
        // spawns its units into). Its own `git rev-parse --show-toplevel` is the WORKTREE path, so
        // an identity anchored at the process cwd would misfile the write under
        // `proj-<worktree>-run`; the single server-location authority must instead bind to the
        // OWNING root via `main_repo_root` (whose `git-common-dir` resolves to the main repo).
        let nested = root.join(".rigger").join("tmp").join("rigger-wt-nested");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        let ok = Command::new("git")
            .args(["worktree", "add", "-q", nested.to_str().unwrap()])
            .current_dir(root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git worktree add (nested) must succeed");

        // A bare courier - `rigger emit`, no `--eventstore` flag - run FROM the nested worktree,
        // the exact surface a spawned worker's self-report uses inside its unit worktree.
        let out = Command::new(rigger_bin())
            .args([
                "emit",
                "DecisionMade",
                r#"{"id":"d-nested-wt","summary":"filed under the owning root","governs":["src/lib.rs"],"supersedes":""}"#,
            ])
            .current_dir(&nested)
            .env("KURRENTDB_CONN", &conn)
            .env("RIGGER_NO_DASH", "1")
            .output()
            .expect("spawn rigger emit from the nested worktree");
        assert!(
            out.status.success(),
            "rigger emit from a nested worktree must succeed against the server: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // No local sqlite event log is fabricated in EITHER the root or the worktree - the log is
        // the server's.
        assert!(
            !root.join(".rigger").join("events.db").exists(),
            "a server-configured courier must not create a local events.db at the owning root"
        );
        assert!(
            !nested.join(".rigger").join("events.db").exists(),
            "a server-configured courier must not create a local events.db in the worktree"
        );

        let backend = open_server(&conn);

        // The event landed in the OWNING ROOT's run stream - the identity the conductor reads.
        let root_id = run_stream_identity(root);
        let root_store = Namespaced::new(&backend, &root_id);
        let root_events = root_store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .expect("read the owning root's run stream from the server");
        assert!(
            root_events
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("d-nested-wt")),
            "a nested-worktree courier's DecisionMade must land under the OWNING ROOT identity \
             {root_id:?} (proving the server location binds to `main_repo_root`, not the cwd); \
             found {} event(s)",
            root_events.len()
        );

        // And NOT under the worktree's own git identity - the exact `proj-<worktree>-run` misfile a
        // regression to the cwd (`cwd.join(RIGGER_DIR)` instead of `main_repo_root(cwd)`) produces.
        let wt_id = run_stream_identity(&nested);
        assert_ne!(
            wt_id, root_id,
            "the nested worktree must resolve a DISTINCT git top-level identity, else this test \
             cannot discriminate the owning-root binding from the cwd"
        );
        let wt_store = Namespaced::new(&backend, &wt_id);
        let wt_events = wt_store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap_or_default();
        assert!(
            !wt_events
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("d-nested-wt")),
            "the courier must NOT misfile under the worktree identity {wt_id:?} - landing there is \
             the `proj-<worktree>-run` state-fracture the owning-root binding closes"
        );
    }));

    let _ = rt.block_on(container.rm());
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
