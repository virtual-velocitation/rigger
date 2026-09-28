//! THE BOUNDARY GATE: Clean Architecture made mechanical over the whole workspace. Every crate
//! sits in one ring of the workspace plan (entities and ports innermost, the composition root
//! outermost) and three rules hold on the checked-out tree:
//!
//! 1. DEPENDENCY DIRECTION. A crate's `[dependencies]` / `[build-dependencies]` (including
//!    target-specific tables) name only workspace crates in its own ring or an inner one. Dev
//!    dependencies are exempt: a test is its own composition root, the same reason test code
//!    may construct adapters below.
//! 2. ADAPTERS ARE WIRED ONLY IN THE COMPOSITION ROOT. An adapter constructor (a store opener,
//!    a graph opener, an agent driver) is named in production code only inside the composition
//!    root or inside that adapter's own files. Anything else reaches past a port.
//! 3. THE PRINCIPLE LINTS CARRY NO EXEMPTION. No item opts out of a lint the root manifest
//!    denies.
//!
//! Rules 1 and 2 each carry an allowlist of today's offenders that may only shrink: an entry that no
//! longer matches an offender fails the suite until it is deleted, so the list is the follow-up
//! work queue and never outlives the work.

mod common;
use common::repo::{collect_rs_files, production_part, repo_root, table_lines};

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "common/source_audit.rs"]
mod source_audit;
use source_audit::{cfg_test_ranges, enclosing_fn_line, fn_sig_lines, in_ranges, write_file};

// ---------------------------------------------------------------------------------------------
// Rule 1: dependency direction
// ---------------------------------------------------------------------------------------------

/// The ring of every workspace crate, innermost first: 1 entities, use-case rules and ports;
/// 2 application; 3 adapters; 4 delivery; 5 the composition root. A crate may depend only on
/// crates whose ring is less than or equal to its own.
const RINGS: &[(&str, u8)] = &[
    ("rigger-domain", 1),
    ("rigger-ports", 1),
    ("rigger-conductor", 2),
    ("rigger-store-sqlite", 3),
    ("rigger-graph-sqlite", 3),
    ("rigger-store-segments", 3),
    ("rigger-graph-mmap", 3),
    ("rigger-driver", 3),
    ("rigger-worktree-git", 3),
    ("rigger-process", 3),
    ("rigger-gates-shell", 3),
    ("rigger-grounder", 3),
    ("rigger-dash", 4),
    ("rigger-console", 4),
    ("console-core", 4),
    ("rigger", 5),
];

/// Outward edges that exist today, `(from, to)`, each deleted by the extraction commit named
/// beside it.
const EDGE_ALLOWLIST: &[(&str, &str)] = &[
    ("console-core", "rigger"), // removed by: split: extract rigger-console
];

/// One workspace crate: its package name and the workspace crates it depends on (normal and
/// build dependencies only).
struct Member {
    name: String,
    deps: Vec<String>,
}

/// The quoted strings in `text`, in order.
fn quoted(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The member directories `[workspace] members` names under `root`, a trailing `/*` expanded
/// to every child directory holding a `Cargo.toml`.
fn member_dirs(root: &Path) -> Vec<PathBuf> {
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("read the root manifest");
    let table = table_lines(&manifest, "workspace").join("\n");
    let Some(start) = table.find("members") else {
        return vec![root.to_path_buf()];
    };
    let list = &table[start..table[start..].find(']').map_or(table.len(), |e| start + e)];
    let mut dirs = Vec::new();
    for member in quoted(list) {
        match member.strip_suffix("/*") {
            Some(parent) => {
                let mut children: Vec<PathBuf> = fs::read_dir(root.join(parent))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.join("Cargo.toml").is_file())
                    .collect();
                children.sort();
                dirs.extend(children);
            }
            None => dirs.push(root.join(member)),
        }
    }
    dirs
}

/// Whether a manifest table header names a normal or build dependency table (a target-specific
/// one included); dev dependencies are exempt.
fn is_dependency_table(header: &str) -> bool {
    ["dependencies", "build-dependencies"]
        .iter()
        .any(|t| header == *t || header.ends_with(&format!(".{t}")))
}

/// The package name and every dependency key (renames resolved through `package = "..."`) the
/// manifest at `dir` declares in its normal and build dependency tables.
fn read_member(dir: &Path) -> (String, Vec<String>) {
    let manifest = fs::read_to_string(dir.join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("read {}/Cargo.toml: {e}", dir.display()));
    let name = table_lines(&manifest, "package")
        .iter()
        .find(|l| l.trim_start().starts_with("name"))
        .and_then(|l| quoted(l).into_iter().next())
        .unwrap_or_else(|| panic!("{}/Cargo.toml names no package", dir.display()));
    let mut deps = Vec::new();
    let mut header = String::new();
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            header = line.trim_matches(|c| c == '[' || c == ']').to_string();
            if let Some(dep) = header.strip_prefix("dependencies.") {
                deps.push(dep.to_string());
            }
            continue;
        }
        if !is_dependency_table(&header) || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().split('.').next().unwrap_or_default().to_string();
        let renamed = value
            .split_once("package")
            .and_then(|(_, rest)| quoted(rest).into_iter().next());
        deps.push(renamed.unwrap_or(key));
    }
    (name, deps)
}

/// Every workspace member under `root`, its dependencies narrowed to workspace crates.
fn workspace(root: &Path) -> Vec<Member> {
    let raw: Vec<(String, Vec<String>)> =
        member_dirs(root).iter().map(|d| read_member(d)).collect();
    let names: Vec<String> = raw.iter().map(|(n, _)| n.clone()).collect();
    raw.into_iter()
        .map(|(name, deps)| Member {
            name,
            deps: deps.into_iter().filter(|d| names.contains(d)).collect(),
        })
        .collect()
}

/// Every dependency-direction violation in the workspace at `root`, each a sentence naming the
/// fix: an unknown crate (with the line to add to [`RINGS`]), or an outward edge.
fn direction_violations(
    root: &Path,
    rings: &[(&str, u8)],
    allowed: &[(&str, &str)],
) -> Vec<String> {
    let ring: BTreeMap<&str, u8> = rings.iter().copied().collect();
    let mut out = Vec::new();
    for member in workspace(root) {
        let Some(&from) = ring.get(member.name.as_str()) else {
            out.push(format!(
                "crate `{}` has no ring; add `(\"{}\", <ring>),` to RINGS in tests/boundary_audit.rs \
                 per the workspace plan's ring table",
                member.name, member.name
            ));
            continue;
        };
        for dep in &member.deps {
            let to = ring.get(dep.as_str()).copied().unwrap_or(u8::MAX);
            if to > from && !allowed.contains(&(member.name.as_str(), dep.as_str())) {
                out.push(format!(
                    "`{}` (ring {from}) depends on `{dep}` (ring {to}): dependencies point inward \
                     only; move the shared item inward or invert the dependency through a port",
                    member.name
                ));
            }
        }
    }
    out
}

#[test]
fn every_workspace_crate_depends_only_inward() {
    let violations = direction_violations(&repo_root(), RINGS, EDGE_ALLOWLIST);
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn every_allowlisted_outward_edge_still_exists() {
    let root = repo_root();
    let stale: Vec<String> = EDGE_ALLOWLIST
        .iter()
        .filter(|&&(from, to)| {
            !workspace(&root)
                .iter()
                .any(|m| m.name == from && m.deps.iter().any(|d| d == to))
        })
        .map(|(from, to)| format!("({from:?}, {to:?})"))
        .collect();
    assert!(
        stale.is_empty(),
        "EDGE_ALLOWLIST entries whose edge is gone - delete them: {}",
        stale.join(", ")
    );
}

#[test]
fn an_outward_edge_and_an_unknown_crate_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(
        root,
        "Cargo.toml",
        "[package]\nname = \"rigger\"\n\n[workspace]\nmembers = [\".\", \"crates/*\"]\n",
    );
    write_file(
        root,
        "crates/domain/Cargo.toml",
        "[package]\nname = \"rigger-domain\"\n\n[dependencies]\nserde = \"1\"\n\
         conductor = { path = \"../conductor\", package = \"rigger-conductor\" }\n\n\
         [dev-dependencies]\nrigger = { path = \"../..\" }\n",
    );
    write_file(
        root,
        "crates/conductor/Cargo.toml",
        "[package]\nname = \"rigger-conductor\"\n\n[target.'cfg(unix)'.dependencies]\n\
         rigger-domain = { path = \"../domain\" }\n",
    );
    write_file(
        root,
        "crates/stray/Cargo.toml",
        "[package]\nname = \"rigger-stray\"\n",
    );
    let violations = direction_violations(root, RINGS, &[]);
    assert_eq!(violations.len(), 2, "{violations:#?}");
    assert!(
        violations[0].contains("`rigger-domain` (ring 1) depends on `rigger-conductor` (ring 2)")
    );
    assert!(violations[1].contains("add `(\"rigger-stray\", <ring>),` to RINGS"));
    assert!(direction_violations(root, RINGS, &[("rigger-domain", "rigger-conductor")]).len() == 1);
}

// ---------------------------------------------------------------------------------------------
// Rule 1b: domain items still outside the domain crate
// ---------------------------------------------------------------------------------------------

/// Domain-responsibility items still in the root crate, `(file, item, lesson)`: each one's code,
/// or a type it cannot be separated from, reaches for an adapter (the clock, a process, the
/// filesystem, an adapter's type), so it waits for a port. The lesson records the port plus the
/// adapter move that lets it join `rigger-domain`.
const DEFERRED_DOMAIN_ITEMS: &[(&str, &str, &str)] = &[
    (
        "src/config.rs",
        "Config",
        "lesson-split-domain-config-validate",
    ),
    (
        "src/config.rs",
        "lint_gating_verdict_lines",
        "lesson-split-domain-config-validate",
    ),
    (
        "src/config.rs",
        "unbounded_wall_clock_advisory",
        "lesson-split-domain-config-validate",
    ),
    (
        "src/ingest.rs",
        "key_batch",
        "lesson-split-domain-ingest-key-batch",
    ),
    (
        "src/blast_radius_eval.rs",
        "corpus_gates",
        "lesson-split-domain-blast-radius-eval",
    ),
    (
        "src/conductor.rs",
        "validate_acyclic",
        "lesson-split-domain-conductor-error",
    ),
    (
        "src/conductor.rs",
        "assert_no_ungated_fanout_unit",
        "lesson-split-domain-conductor-error",
    ),
    (
        "crates/rigger-gates-shell/src/gate.rs",
        "Runner",
        "lesson-split-domain-gate-runner-port",
    ),
];

/// The domain crate's sources.
const DOMAIN_SRC: &str = "crates/rigger-domain/src";

/// Whether `text` declares an item named `item`.
fn declares(text: &str, item: &str) -> bool {
    regex::Regex::new(&format!(
        r"\b(struct|enum|trait|type|fn|const|static|mod)\s+{item}\b"
    ))
    .unwrap()
    .is_match(text)
}

/// Every [`DEFERRED_DOMAIN_ITEMS`]-shaped entry under `root` that no longer describes a deferred
/// item: its file or item is gone, or the item now lives in one of the `homes` crate sources it
/// waits to join.
fn stale_deferred_items(
    root: &Path,
    homes: &[&str],
    entries: &[(&str, &str, &str)],
) -> Vec<String> {
    let home_text: Vec<(&str, String)> = homes
        .iter()
        .flat_map(|home| {
            let mut files = Vec::new();
            collect_rs_files(&root.join(home), &mut files);
            files
                .into_iter()
                .map(move |p| (*home, fs::read_to_string(p).unwrap_or_default()))
        })
        .collect();
    entries
        .iter()
        .filter_map(|&(file, item, lesson)| {
            let text = fs::read_to_string(root.join(file)).unwrap_or_default();
            if !declares(&text, item) {
                Some(format!(
                    "({file:?}, {item:?}, {lesson:?}): `{item}` is no longer declared in {file}; \
                     delete the entry"
                ))
            } else if let Some((home, _)) = home_text.iter().find(|(_, t)| declares(t, item)) {
                Some(format!(
                    "({file:?}, {item:?}, {lesson:?}): `{item}` is now declared in {home}; \
                     finish the move and delete the entry"
                ))
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Rule 1c: inward-dependency violations inside the domain crate
// ---------------------------------------------------------------------------------------------

/// Items placed in `rigger-domain` by their responsibility that still reach for something the
/// domain must not know, `(file, item, what it reaches for, lesson)`. The lesson records the port
/// that removes the reach; the entry goes when the item stops declaring it at that location.
const DOMAIN_VIOLATIONS: &[(&str, &str, &str, &str)] = &[(
    "crates/rigger-domain/src/eventstore.rs",
    "mint",
    "the clock (SystemTime::now) and entropy (uuid::Uuid::new_v4)",
    "lesson-split-domain-event-mint-clock-id",
)];

/// Every [`DOMAIN_VIOLATIONS`]-shaped entry under `root` whose item is no longer declared in its
/// file, or whose file is outside the `homes` crate sources the list covers.
fn stale_violations(
    root: &Path,
    homes: &[&str],
    entries: &[(&str, &str, &str, &str)],
) -> Vec<String> {
    entries
        .iter()
        .filter_map(|&(file, item, reach, lesson)| {
            let text = fs::read_to_string(root.join(file)).unwrap_or_default();
            if !homes.iter().any(|home| file.starts_with(home)) {
                Some(format!(
                    "({file:?}, {item:?}, {reach:?}, {lesson:?}): {file} is not in {}",
                    homes.join(", ")
                ))
            } else if !declares(&text, item) {
                Some(format!(
                    "({file:?}, {item:?}, {reach:?}, {lesson:?}): `{item}` is no longer declared \
                     in {file}; delete the entry"
                ))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn every_split_allowlist_entry_is_still_live() {
    let root = repo_root();
    let mut stale = stale_deferred_items(&root, &[DOMAIN_SRC], DEFERRED_DOMAIN_ITEMS);
    stale.extend(stale_violations(&root, &[DOMAIN_SRC], DOMAIN_VIOLATIONS));
    stale.extend(stale_deferred_items(
        &root,
        ADAPTER_SRCS,
        DEFERRED_ADAPTER_ITEMS,
    ));
    stale.extend(stale_violations(&root, ADAPTER_SRCS, ADAPTER_VIOLATIONS));
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}

#[test]
fn a_domain_allowlist_entry_that_moved_or_vanished_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(
        root,
        "src/ledger.rs",
        "pub struct RunState;\npub fn fold() {}\n",
    );
    write_file(
        root,
        "crates/rigger-domain/src/ledger.rs",
        "pub fn fold() {}\nimpl RunState {\n    fn mint() {}\n}\n",
    );
    let deferred = [
        ("src/ledger.rs", "RunState", "l1"),
        ("src/ledger.rs", "fold", "l2"),
        ("src/ledger.rs", "Gone", "l3"),
    ];
    let stale = stale_deferred_items(root, &[DOMAIN_SRC], &deferred);
    assert_eq!(stale.len(), 2, "{stale:#?}");
    assert!(stale[0].contains("`fold` is now declared in crates/rigger-domain/src"));
    assert!(stale[1].contains("`Gone` is no longer declared in src/ledger.rs"));
    let violations = [
        (
            "crates/rigger-domain/src/ledger.rs",
            "mint",
            "the clock",
            "l4",
        ),
        (
            "crates/rigger-domain/src/ledger.rs",
            "gone",
            "the clock",
            "l5",
        ),
        ("src/ledger.rs", "fold", "the clock", "l6"),
    ];
    let stale = stale_violations(root, &[DOMAIN_SRC], &violations);
    assert_eq!(stale.len(), 2, "{stale:#?}");
    assert!(stale[0].contains("`gone` is no longer declared"));
    assert!(stale[1].contains("is not in crates/rigger-domain/src"));
}

// ---------------------------------------------------------------------------------------------
// Rule 1d: adapter items still outside their adapter crate, or reaching outward inside it
// ---------------------------------------------------------------------------------------------

/// The adapter crates' sources.
const ADAPTER_SRCS: &[&str] = &[
    "crates/rigger-store-sqlite/src",
    "crates/rigger-graph-sqlite/src",
    "crates/rigger-process/src",
    "crates/rigger-worktree-git/src",
    "crates/rigger-gates-shell/src",
    "crates/rigger-driver/src",
    "crates/rigger-grounder/src",
];

/// Items the workspace plan assigns to an adapter crate that still live in the root crate,
/// `(file, item, lesson)`: each one's code names a type from a crate that does not exist yet (the
/// conductor, a driver, the grounder), so it cannot compile in the adapter crate. The lesson
/// records what it reaches for and the extraction that lets it move; the entry goes when the item
/// leaves the file or lands in an adapter crate.
const DEFERRED_ADAPTER_ITEMS: &[(&str, &str, &str)] = &[
    (
        "src/canary_store.rs",
        "CanaryItem",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "load_corpus",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "corpus_hash",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "ModelPins",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "apply_model_pins",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "CanaryReport",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "record_header",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "run_canary",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/canary_store.rs",
        "default_jobs",
        "lesson-split-store-canary-runner",
    ),
    (
        "src/worktree.rs",
        "scratch_root",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "scratch_root_with",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "sweep_orphan_scratch_roots",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "scratch_root_path",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "scratch_root_path_with",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "cache_scratch_root_from",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "scratch_root_from_env",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/worktree.rs",
        "scratch_root_path_from_env",
        "lesson-split-worktree-scratch-root-reach",
    ),
    (
        "src/grounder/workflowdef.rs",
        "workflow_doc",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "stage_id",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "gate_id",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "agent_id",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "ConceptTuple",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "LinkTuple",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "reviewers_of",
        "lesson-split-grounder-workflowdef-config-store",
    ),
    (
        "src/grounder/workflowdef.rs",
        "light_reviewers_of",
        "lesson-split-grounder-workflowdef-config-store",
    ),
];

/// Items placed in an adapter crate by their responsibility that still reach outward for
/// something an adapter must not know (the conductor, a driver, the dash, the CLI),
/// `(file, item, what it reaches for, lesson)`. The lesson records the move that removes the
/// reach; the entry goes when the item stops declaring it at that location.
const ADAPTER_VIOLATIONS: &[(&str, &str, &str, &str)] = &[
    (
        "crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs",
        "apply_review_finding",
        "the conductor's META_SPAWN meta key, through a dev-dependency on the root crate",
        "lesson-split-graph-sqlite-test-meta-spawn",
    ),
    (
        "crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs",
        "a_blast_radius_computed_event_folds_to_nothing_idempotently",
        "metrics' TYPE_BLAST_RADIUS_COMPUTED, named through the conductor, through a \
         dev-dependency on the root crate",
        "lesson-split-graph-sqlite-test-blast-radius-type",
    ),
    (
        "crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs",
        "consumers_read_the_log_not_the_dropped_machinery_nodes",
        "metrics::project, through a dev-dependency on the root crate",
        "lesson-split-graph-sqlite-test-metrics-project",
    ),
    (
        "crates/rigger-driver/src/driver/replay.rs",
        "tests",
        "the conductor's run loop, Deps, park recognition and meta/event keys, the root Config and \
         the gate runner, through a dev-dependency on the root crate",
        "lesson-split-driver-replay-tests-drive-the-conductor",
    ),
    (
        "crates/rigger-driver/src/lib.rs",
        "conductor_fixtures",
        "the shared conductor and config fixtures' conductor, Config and gate vocabulary, through a \
         dev-dependency on the root crate",
        "lesson-split-driver-fixtures-name-the-conductor",
    ),
];

// ---------------------------------------------------------------------------------------------
// Rule 2: adapters are wired only in the composition root
// ---------------------------------------------------------------------------------------------

/// The composition root: the one place production code constructs adapters.
const COMPOSITION_ROOT: &[&str] = &["src/main.rs", "src/cli/"];

/// An adapter family: the constructor shape that builds it, and the files that ARE the adapter
/// (an adapter may use its own constructor internally).
struct Adapter {
    family: &'static str,
    constructor: &'static str,
    home: &'static [&'static str],
}

const ADAPTERS: &[Adapter] = &[
    Adapter {
        family: "sqlite opener",
        constructor: r"\bopen_connection\(",
        home: &["crates/rigger-store-sqlite/", "crates/rigger-graph-sqlite/"],
    },
    Adapter {
        family: "event store",
        constructor: r"\bStore::open\(",
        home: &["crates/rigger-store-sqlite/"],
    },
    Adapter {
        family: "graph projection",
        constructor: r"\bProjector::open\(",
        home: &["crates/rigger-graph-sqlite/"],
    },
    Adapter {
        family: "agent driver",
        constructor: r"\b(ReplayDriver::new|Driver::new|Driver::default)\(|\bDriver \{",
        home: &["crates/rigger-driver/"],
    },
];

/// Production adapter construction outside the composition root, `file:item` (`item` is the
/// enclosing function, or `<top level>`) with the adapter family it reaches for. Each entry
/// carries the LessonLearned that records the port-plus-adapter move that deletes it.
const CONSTRUCTOR_ALLOWLIST: &[(&str, &str)] = &[];

/// The pinned size of [`CONSTRUCTOR_ALLOWLIST`]: it may only fall.
const CONSTRUCTOR_ALLOWLIST_PIN: usize = 0;

/// Every production `.rs` file under `root`'s `src/` and each `crates/*/src/`, as
/// `(repo-relative path, text)`.
fn production_sources(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    collect_rs_files(&root.join("crates"), &mut files);
    files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, p)
        })
        .filter(|(rel, _)| rel.starts_with("src/") || rel.split('/').nth(2) == Some("src"))
        .map(|(rel, p)| {
            let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {rel}: {e}"));
            (rel, text)
        })
        .collect()
}

/// The name of the function declared on `line` (the identifier after `fn `).
fn fn_name(line: &str) -> String {
    let after = line.split("fn ").nth(1).unwrap_or_default();
    after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Every production adapter construction under `root` outside the composition root and the
/// adapter's own files, as `(file:item, family)`, sorted and deduplicated.
fn misplaced_constructors(root: &Path) -> Vec<(String, &'static str)> {
    let patterns: Vec<(regex::Regex, &Adapter)> = ADAPTERS
        .iter()
        .map(|a| (regex::Regex::new(a.constructor).unwrap(), a))
        .collect();
    let mut hits = Vec::new();
    for (rel, text) in production_sources(root) {
        if COMPOSITION_ROOT.iter().any(|r| rel.starts_with(r)) {
            continue;
        }
        let lines: Vec<&str> = production_part(&text).lines().collect();
        let tests = cfg_test_ranges(&lines);
        let sigs = fn_sig_lines(&lines, &tests);
        for (i, line) in lines.iter().enumerate() {
            if in_ranges(i, &tests) || line.trim_start().starts_with("//") {
                continue;
            }
            for (re, adapter) in &patterns {
                if re.is_match(line) && !adapter.home.iter().any(|h| rel.starts_with(h)) {
                    let item = enclosing_fn_line(&lines, &sigs, i)
                        .map_or_else(|| "<top level>".to_string(), |s| fn_name(lines[s]));
                    hits.push((format!("{rel}:{item}"), adapter.family));
                }
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

#[test]
fn adapters_are_constructed_only_in_the_composition_root() {
    let allowed: Vec<&str> = CONSTRUCTOR_ALLOWLIST
        .iter()
        .map(|(site, _)| *site)
        .collect();
    let found = misplaced_constructors(&repo_root());
    let new: Vec<String> = found
        .iter()
        .filter(|(site, _)| !allowed.contains(&site.as_str()))
        .map(|(site, family)| format!("{site} constructs the {family} adapter"))
        .collect();
    assert!(
        new.is_empty(),
        "production code constructs an adapter outside the composition root ({}); take the \
         port as a parameter and construct the adapter in the root:\n{}",
        COMPOSITION_ROOT.join(", "),
        new.join("\n")
    );
    let stale: Vec<&str> = allowed
        .iter()
        .copied()
        .filter(|site| !found.iter().any(|(s, _)| s == site))
        .collect();
    assert!(
        stale.is_empty(),
        "CONSTRUCTOR_ALLOWLIST entries that no longer construct an adapter - delete them and \
         lower CONSTRUCTOR_ALLOWLIST_PIN: {stale:?}"
    );
    assert_eq!(
        CONSTRUCTOR_ALLOWLIST.len(),
        CONSTRUCTOR_ALLOWLIST_PIN,
        "CONSTRUCTOR_ALLOWLIST only shrinks: a new offender is fixed with a port, never listed"
    );
}

#[test]
fn a_constructor_outside_the_root_is_reported_by_file_and_item() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(
        root,
        "src/conductor.rs",
        "fn wave() {\n    let s = Store::open(path);\n}\n\n#[cfg(test)]\nmod tests {\n    \
         fn t() {\n        let s = Store::open(\":memory:\");\n    }\n}\n",
    );
    write_file(
        root,
        "src/main.rs",
        "fn main() {\n    let s = Store::open(path);\n}\n",
    );
    write_file(
        root,
        "crates/rigger-store-sqlite/src/eventstore/sqlite.rs",
        "fn open() {\n    let c = open_connection(path);\n}\n",
    );
    write_file(
        root,
        "crates/rigger-dash/src/lib.rs",
        "fn serve() {\n    let g = Projector::open(p, q);\n}\n",
    );
    assert_eq!(
        misplaced_constructors(root),
        vec![
            (
                "crates/rigger-dash/src/lib.rs:serve".to_string(),
                "graph projection"
            ),
            ("src/conductor.rs:wave".to_string(), "event store"),
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// Rule 3: the principle lints carry no exemption
// ---------------------------------------------------------------------------------------------

/// The lints the root manifest's `[workspace.lints.clippy]` denies. Clippy is their gate; this
/// rule only keeps an item from opting out of one with an `allow` or `expect` attribute.
const PRINCIPLE_LINTS: &[&str] = &["large_enum_variant", "module_inception"];

/// Every `allow`/`expect` of a [`PRINCIPLE_LINTS`] lint over every `.rs` file under `root`'s
/// `src/`, `crates/` and `tests/`, as `file:line: lint`.
fn lint_exemptions(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for dir in ["src", "crates", "tests"] {
        collect_rs_files(&root.join(dir), &mut files);
    }
    let mut out = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        for (i, line) in text.lines().enumerate() {
            for lint in PRINCIPLE_LINTS {
                if ["allow", "expect"]
                    .iter()
                    .any(|attr| line.contains(&format!("{attr}(clippy::{lint})")))
                {
                    out.push(format!("{rel}:{}: exempts `{lint}`; fix the item", i + 1));
                }
            }
        }
    }
    out
}

#[test]
fn principle_lints_carry_no_exemption() {
    let problems = lint_exemptions(&repo_root());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
