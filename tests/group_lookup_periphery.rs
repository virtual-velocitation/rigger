//! Periphery (contract / API / integration) tests for spec 101 criterion 3: THE LATEST GENERATION
//! IS A GROUP LOOKUP. Every keyed derived event carries its batch identity as its `group`, and
//! both ingest sinks - the run's `rigger step` and `rigger graph build` - ask the store's group
//! lookup for an identity's latest recorded generation instead of reading the stream.
//!
//! The inside-out proofs count a step's reads over a `:memory:` store inside the crate and pin
//! each backend's lookup through the contract suite. What they are structurally blind to, and
//! what this file drives, is the shipped composition:
//!
//! 1. THE TWO SINKS ACROSS PROCESSES. A `rigger graph build` records a generation, a later
//!    `rigger step` - another process, another sink - must answer it through the lookup and
//!    re-append nothing for it, and must append exactly the batches that moved.
//! 2. THE UPGRADE. A log recorded before the stamp carries keys but no group. The first ingest
//!    over it re-emits the live index once, stamped, and every later ingest (by either sink)
//!    answers from the stamp and appends nothing.
//! 3. THE NAMESPACE. The product stores many projects in one `events.db` behind a project
//!    namespace; one project's recording of an identity must never answer another project's
//!    lookup of the same identity. Drivable in both feature lanes, since it needs no walk.

mod common;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision, GroupHead, META_GROUP};
use rigger::ingest::{keyed_derived_event, latest_generation, META_REPLAY_KEY};
use std::collections::BTreeMap;

/// The two ingest sinks over a real tree: the walk that mints the batches is `symbols`-gated, so
/// in the light lane there is no batch to ingest and nothing here to drive.
#[cfg(feature = "symbols")]
mod ingest_sinks {
    use super::*;
    use common::cli::{
        identified_git_project, init_event_log, read_run_events, run_rigger, step_line,
        with_run_store, write_workflow,
    };
    use common::git::run_git;
    use std::path::Path;

    /// The derived index events of `events`, in log order.
    fn derived(events: &[Event]) -> Vec<Event> {
        events
            .iter()
            .filter(|e| rigger::ingest::is_derived_index_type(&e.type_))
            .cloned()
            .collect()
    }

    /// Every derived event appended to `root`'s run stream since it held `before` events.
    fn derived_since(root: &Path, before: usize) -> Vec<Event> {
        derived(&read_run_events(root)[before..])
    }

    /// Each event's `name` metadata entry, in order (empty when it carries none): its replay key
    /// under [`META_REPLAY_KEY`], its group under [`META_GROUP`].
    fn meta_of(events: &[Event], name: &str) -> Vec<String> {
        events
            .iter()
            .map(|e| e.meta.get(name).cloned().unwrap_or_default())
            .collect()
    }

    /// What the SHIPPED walk emits for the tree at `root` as it stands: each batch's identity, its
    /// generation and its keys, in walk order - the writer both sinks drive, never a spelling this
    /// test invented.
    fn walk(root: &Path) -> Vec<(String, String, Vec<String>)> {
        let mut batches = Vec::new();
        rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |keyed, _| {
            let (identity, generation) = rigger::ingest::derived_key_parts(&keyed[0].0).unwrap();
            batches.push((
                identity.to_string(),
                generation.to_string(),
                keyed.iter().map(|(k, _)| k.clone()).collect(),
            ));
        });
        batches
    }

    /// The keys of the walked batches whose identity is one of `identities`, concatenated in walk
    /// order - exactly what an ingest that re-emits those batches whole appends.
    fn keys_of(batches: &[(String, String, Vec<String>)], identities: &[&str]) -> Vec<String> {
        batches
            .iter()
            .filter(|(identity, _, _)| identities.contains(&identity.as_str()))
            .flat_map(|(_, _, keys)| keys.clone())
            .collect()
    }

    /// The identity each key names, in order: the group every appended event must carry.
    fn identities_of(keys: &[String]) -> Vec<String> {
        keys.iter()
            .map(|k| rigger::ingest::derived_key_parts(k).unwrap().0.to_string())
            .collect()
    }

    /// The generation the store's group lookup answers for each walked identity, in walk order.
    fn answered(root: &Path, batches: &[(String, String, Vec<String>)]) -> Vec<Option<String>> {
        with_run_store(root, |store| {
            batches
                .iter()
                .map(|(identity, _, _)| {
                    latest_generation(store, rigger::conductor::STREAM, identity).unwrap()
                })
                .collect()
        })
    }

    /// Write `files` under `root` and commit the tree, so the project stands at that content.
    fn tree(root: &Path, files: &[(&str, &str)]) {
        for (file, body) in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let _ = run_git(root, &["add", "-A"]);
        let _ = run_git(root, &["commit", "-q", "-m", "tree"]);
    }

    /// A project that is its own git repo, carrying a one-stage workflow whose spawn a step parks (so
    /// a step builds a prompt, and so walks and ingests the tree), the store the binary appends to,
    /// and a real design doc that never changes, so the untouched half covers both ingest prefixes.
    fn ingestable_project() -> tempfile::TempDir {
        let dir = identified_git_project();
        let root = dir.path();
        std::fs::write(
            root.join(".gitignore"),
            format!("{}/\n", rigger::config::RIGGER_DIR),
        )
        .unwrap();
        write_workflow(root, "");
        init_event_log(root);
        tree(
            root,
            &[(
                "specs/sample.md",
                include_str!("../specs/29c-unified-traversal-tiers.md"),
            )],
        );
        dir
    }

    /// A raw connection of its own to `root`'s events file, for planting what the store's API
    /// never writes.
    fn events_db(root: &Path) -> rusqlite::Connection {
        rusqlite::Connection::open(common::cli::rigger_file(root, "events.db")).unwrap()
    }

    /// Run `sql` against `root`'s events file on a raw connection of its own.
    fn execute(root: &Path, sql: &str) {
        events_db(root).execute_batch(sql).unwrap();
    }

    /// Set the type of the one recording grouped under `identity` to the SQL literal `type_sql`,
    /// asserting exactly one row carries that group.
    fn set_recording_type(root: &Path, identity: &str, type_sql: &str) {
        let rows = events_db(root)
            .execute(
                &format!(
                    "UPDATE events SET type = {type_sql} WHERE json_extract(meta, '$.group') = ?1"
                ),
                [identity],
            )
            .unwrap();
        assert_eq!(rows, 1, "exactly one recording is grouped under {identity}");
    }

    /// Record a stale generation of `identity`'s batch, then make that recording unreadable to the
    /// group lookup (its type is not text), so the store cannot answer `identity`'s lookup. Returns
    /// the run stream's length once the recording is appended, before it is made unreadable.
    fn plant_unreadable_recording(root: &Path, identity: &str) -> usize {
        with_run_store(root, |store| {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[keyed_derived_event(
                        Event::new(
                            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                            b"{}".to_vec(),
                        ),
                        &format!("{identity}@stale#0"),
                    )],
                )
                .unwrap();
        });
        let recorded = read_run_events(root).len();
        set_recording_type(root, identity, "X'FF'");
        recorded
    }

    /// Give `identity`'s planted recording back its text type, so the log reads back whole.
    fn restore_recording(root: &Path, identity: &str) {
        set_recording_type(
            root,
            identity,
            &format!("'{}'", rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED),
        );
    }

    /// `rigger graph build` in `root`, which must succeed.
    fn graph_build(root: &Path, what: &str) {
        let (out, err, ok) = run_rigger(root, &["graph", "build"]);
        assert!(
            ok,
            "{what}: graph build must succeed; stderr: {err}; stdout: {out}"
        );
    }

    const UNCHANGED: &str = "src/unchanged.rs";
    const CHANGED: &str = "src/changed.rs";
    const REVERTED: &str = "src/reverted.rs";

    /// GIVEN a project whose `unchanged.rs`, `changed.rs` and `reverted.rs` a `rigger graph build`
    /// recorded, and whose `reverted.rs` a second build recorded at a newer generation,
    /// WHEN `changed.rs` is edited, `reverted.rs` goes back to its first content and a `rigger step`
    /// ingests the tree,
    /// THEN the step appends exactly the changed and reverted files' batches, whole and in walk order,
    /// each event grouped by its batch identity; the reverted batch re-emits keys the log already
    /// records; the store's group lookup then answers every walked identity with the tree's current
    /// generation; and a second `rigger step` - a fresh process meeting every identity for the first
    /// time - appends no derived event at all.
    ///
    /// The unchanged file and the design doc were recorded by the OTHER sink, so the step sparing them
    /// is the group lookup answering across sinks and processes, which no in-crate test can see.
    #[test]
    fn a_step_over_an_unchanged_a_changed_and_a_reverted_file_appends_exactly_the_moved_batches_grouped(
    ) {
        let dir = ingestable_project();
        let root = dir.path();
        tree(
            root,
            &[
                (UNCHANGED, "pub fn kept() {}\n"),
                (CHANGED, "pub fn before() {}\n"),
                (REVERTED, "pub fn first() {}\n"),
            ],
        );
        let first = walk(root);
        graph_build(root, "the first build");
        tree(root, &[(REVERTED, "pub fn second() {}\n")]);
        let second = walk(root);
        graph_build(root, "the build that moves the reverted file on");
        tree(
            root,
            &[
                (CHANGED, "pub fn after() {}\n"),
                (REVERTED, "pub fn first() {}\n"),
            ],
        );
        let now = walk(root);
        let generation = |batches: &[(String, String, Vec<String>)], file: &str| {
            let identity = format!("gc/{file}");
            batches
                .iter()
                .find(|(id, _, _)| *id == identity)
                .unwrap_or_else(|| panic!("the walk emits {identity}"))
                .1
                .clone()
        };
        assert_eq!(generation(&now, REVERTED), generation(&first, REVERTED));
        assert_ne!(generation(&now, REVERTED), generation(&second, REVERTED));
        assert_ne!(generation(&now, CHANGED), generation(&first, CHANGED));
        assert_eq!(generation(&now, UNCHANGED), generation(&first, UNCHANGED));
        assert_eq!(
            answered(root, &now)
                .iter()
                .zip(&now)
                .filter(|(answer, (_, g, _))| answer.as_deref() == Some(g.as_str()))
                .count(),
            now.len() - 2,
            "before the step, every identity but the changed and the reverted one is recorded at its \
             current generation"
        );

        let before = read_run_events(root).len();
        step_line(root, "the step that ingests the moved tree");
        let appended = derived_since(root, before);
        let moved = keys_of(&now, &[&format!("gc/{CHANGED}"), &format!("gc/{REVERTED}")]);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            moved,
            "the step appends the changed and reverted batches, whole, and no other derived event"
        );
        assert_eq!(
            meta_of(&appended, META_GROUP),
            identities_of(&moved),
            "every appended event carries its batch identity as its group"
        );
        assert_eq!(
            keys_of(&now, &[&format!("gc/{REVERTED}")]),
            keys_of(&first, &[&format!("gc/{REVERTED}")]),
            "the reverted batch re-emits keys the log already records, because they are no longer \
             its latest generation"
        );
        assert_eq!(
            answered(root, &now),
            now.iter()
                .map(|(_, g, _)| Some(g.clone()))
                .collect::<Vec<_>>(),
            "after the step the group lookup answers every walked identity at its current generation"
        );

        let settled = read_run_events(root).len();
        step_line(root, "the step over the settled tree");
        assert_eq!(
            meta_of(&derived_since(root, settled), META_REPLAY_KEY),
            Vec::<String>::new(),
            "a later step's first-sight lookups answer every batch as recorded, so it appends nothing"
        );
    }

    /// GIVEN a project whose store holds a recording of `src/broken.rs`'s batch that the group
    /// lookup cannot read back (its newest member's type is not text),
    /// WHEN `rigger graph build` walks the tree,
    /// THEN the build FAILS rather than reporting success: an unanswered lookup is never read as
    /// "already recorded", so the build names the store's error, prints no ingested-count line, and
    /// appends nothing for that batch - while every other batch of the walk, whose lookup did answer,
    /// is appended whole, grouped, in walk order.
    #[test]
    fn a_graph_build_whose_recorded_generation_is_unreadable_fails_and_appends_nothing_for_that_batch(
    ) {
        const BROKEN: &str = "src/broken.rs";
        let dir = ingestable_project();
        let root = dir.path();
        tree(
            root,
            &[
                (UNCHANGED, "pub fn kept() {}\n"),
                (BROKEN, "pub fn broken() {}\n"),
            ],
        );
        let now = walk(root);
        let broken = format!("gc/{BROKEN}");
        let before = plant_unreadable_recording(root, &broken);

        let (out, err, ok) = run_rigger(root, &["graph", "build"]);
        assert!(
            !ok,
            "a build whose lookup cannot be answered must fail; stdout: {out}; stderr: {err}"
        );
        assert!(
            !out.contains("graph build: ingested"),
            "a failed build reports no ingested count; stdout: {out}"
        );
        assert!(
            err.contains("Invalid column type Blob"),
            "the build names the store's error; stderr: {err}"
        );

        // Restore the recording's type so the log reads back; the build has already run.
        restore_recording(root, &broken);
        let others: Vec<String> = now
            .iter()
            .filter(|(identity, _, _)| *identity != broken)
            .flat_map(|(_, _, keys)| keys.clone())
            .collect();
        assert_eq!(
            others.len() + keys_of(&now, &[&broken]).len(),
            now.iter().map(|(_, _, keys)| keys.len()).sum::<usize>(),
            "sanity: the walk emits the broken file's batch alongside the others"
        );
        assert!(
            !keys_of(&now, &[&broken]).is_empty() && !others.is_empty(),
            "sanity: both halves of the walk are non-empty"
        );
        let appended = derived_since(root, before);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            others,
            "every batch whose lookup answered is appended whole, and the unreadable one is not"
        );
        assert_eq!(meta_of(&appended, META_GROUP), identities_of(&others));
    }

    /// Replace the store's group index with one of the same name that no group lookup can use (a
    /// partial index over no row), so every group lookup fails to plan while every other read -
    /// none of which names that index - still answers. Opening the store keeps it, since the index
    /// the schema creates already exists by that name.
    fn break_group_index(root: &Path) {
        execute(
            root,
            "DROP INDEX idx_events_group; \
             CREATE INDEX idx_events_group ON events(position) WHERE 0",
        );
    }

    /// GIVEN a project whose store cannot answer a group lookup (its group index is unusable) while
    /// every other read still answers,
    /// WHEN a `rigger step` parks a stage and so walks and ingests the tree,
    /// THEN the step FAILS rather than parking its stage: the run sink never reads an unanswered
    /// lookup as "nothing to append", so the step exits non-zero naming the store's error, prints no
    /// step line, and appends no derived event;
    /// AND once the store answers again, the next `rigger step` succeeds and appends the whole walk,
    /// every batch whole, grouped and in walk order - the failed step recorded nothing that spares
    /// a batch it never appended.
    #[test]
    fn a_step_whose_group_lookup_is_unanswered_fails_and_appends_nothing() {
        let dir = ingestable_project();
        let root = dir.path();
        tree(root, &[(UNCHANGED, "pub fn kept() {}\n")]);
        let all: Vec<String> = walk(root)
            .into_iter()
            .flat_map(|(_, _, keys)| keys)
            .collect();
        break_group_index(root);
        let before = read_run_events(root).len();

        let (out, err, ok) = run_rigger(root, &["step"]);
        assert!(
            !ok,
            "a step whose lookup cannot be answered must fail; stdout: {out}; stderr: {err}"
        );
        assert_eq!(out.trim(), "", "a failed step prints no step line");
        assert!(
            err.contains("rigger: conductor: event store: no query solution"),
            "the step fails with the store's lookup error; stderr: {err}"
        );
        assert_eq!(
            meta_of(&derived_since(root, before), META_REPLAY_KEY),
            Vec::<String>::new(),
            "no batch is appended when no lookup answered"
        );

        // Drop the unusable group index, so the next open of the store rebuilds the real one.
        execute(root, "DROP INDEX idx_events_group");
        let failed = read_run_events(root).len();
        step_line(root, "the step once the store answers again");
        let appended = derived_since(root, failed);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            all,
            "the next step appends every batch the failed step could not, whole and in walk order"
        );
        assert_eq!(meta_of(&appended, META_GROUP), identities_of(&all));
    }

    const REFUSED: &str = "src/refused.rs";

    /// The error the store answers an append of a refused batch with.
    const REFUSAL: &str = "the store refuses this batch";

    /// Make the store refuse (with [`REFUSAL`]) any append of an event grouped under `identity`,
    /// while every read and every other append still answers: a real backend failure at the append,
    /// raised by the store itself inside the append's own statement.
    fn refuse_appends_of(root: &Path, identity: &str) {
        execute(
            root,
            &format!(
                "CREATE TRIGGER refuse_group BEFORE INSERT ON events \
                 WHEN json_extract(NEW.meta, '$.group') = '{identity}' \
                 BEGIN SELECT RAISE(ABORT, '{REFUSAL}'); END"
            ),
        );
    }

    /// A tree whose refused file's batch sits BETWEEN other batches of the walk, so a sink that
    /// stopped at the refusal would visibly drop the batches after it. Returns the walk.
    fn tree_with_a_refused_file(root: &Path) -> Vec<(String, String, Vec<String>)> {
        tree(
            root,
            &[
                ("src/after.rs", "pub fn after() {}\n"),
                (REFUSED, "pub fn refused() {}\n"),
                (UNCHANGED, "pub fn kept() {}\n"),
            ],
        );
        let now = walk(root);
        let refused = format!("gc/{REFUSED}");
        let at = now
            .iter()
            .position(|(identity, _, _)| *identity == refused)
            .unwrap_or_else(|| panic!("the walk emits {refused}"));
        assert!(
            at > 0 && at + 1 < now.len(),
            "sanity: the refused batch is neither the walk's first nor its last; walk {:?}",
            now.iter()
                .map(|(identity, _, _)| identity)
                .collect::<Vec<_>>()
        );
        now
    }

    /// Drive one ingest sink - `rigger <args>` - over a project whose store refuses to append
    /// `src/refused.rs`'s batch, a batch the walk emits between others, while every lookup still
    /// answers; then let the store accept appends and drive the same sink again.
    ///
    /// Asserts, of the refused run: it FAILS naming the store's refusal, and every other batch -
    /// the ones after the refusal included - is appended whole, grouped and in walk order. Of the
    /// next run: it succeeds and appends exactly the refused batch, whole and grouped, because the
    /// failed run recorded nothing that spares it. Returns both runs' stdout and the refused
    /// batch's event count, for the sink's own report lines.
    fn refuse_then_accept(args: &[&str]) -> (String, String, usize) {
        let dir = ingestable_project();
        let root = dir.path();
        let now = tree_with_a_refused_file(root);
        let refused = format!("gc/{REFUSED}");
        let refused_keys = keys_of(&now, &[&refused]);
        let others: Vec<String> = now
            .iter()
            .filter(|(identity, _, _)| *identity != refused)
            .flat_map(|(_, _, keys)| keys.clone())
            .collect();
        refuse_appends_of(root, &refused);
        let before = read_run_events(root).len();

        let (refused_out, err, ok) = run_rigger(root, args);
        assert!(
            !ok,
            "rigger {args:?} whose append is refused must fail; stdout: {refused_out}; stderr: {err}"
        );
        assert!(
            err.contains(REFUSAL),
            "rigger {args:?} names the store's refusal; stderr: {err}"
        );
        let appended = derived_since(root, before);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            others,
            "every other batch, the ones after the refusal included, is appended whole in walk order"
        );
        assert_eq!(meta_of(&appended, META_GROUP), identities_of(&others));

        // Let the store record every append again.
        execute(root, "DROP TRIGGER refuse_group");
        let failed = read_run_events(root).len();
        let (next_out, err, ok) = run_rigger(root, args);
        assert!(
            ok,
            "rigger {args:?} once the store accepts appends must succeed; stderr: {err}"
        );
        let appended = derived_since(root, failed);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            refused_keys,
            "the next run appends exactly the batch the store refused, whole"
        );
        assert_eq!(meta_of(&appended, META_GROUP), identities_of(&refused_keys));
        (refused_out, next_out, refused_keys.len())
    }

    /// GIVEN a project whose store refuses to append one file's batch while every lookup answers,
    /// WHEN `rigger graph build` walks the tree,
    /// THEN the build FAILS naming the refusal and prints no ingested-count line, rather than
    /// reporting success over a batch it never recorded, and appends every other batch;
    /// AND once the store accepts appends, the next build appends exactly the refused batch and
    /// reports exactly that count.
    #[test]
    fn a_graph_build_whose_append_is_refused_fails_and_the_next_build_appends_the_refused_batch() {
        let (refused_out, next_out, refused) = refuse_then_accept(&["graph", "build"]);
        assert_eq!(
            refused_out.trim(),
            "",
            "a failed build reports no ingested count"
        );
        assert!(
            next_out.starts_with(&format!(
                "graph build: ingested {refused} code-ingest event(s) into "
            )),
            "the next build reports exactly the refused batch's events; stdout: {next_out}"
        );
    }

    /// GIVEN a project whose store refuses to append one file's batch while every lookup answers,
    /// WHEN a `rigger step` parks a stage and so walks and ingests the tree,
    /// THEN the step FAILS naming the refusal and prints no step line, and appends every other
    /// batch;
    /// AND once the store accepts appends, the next `rigger step` - its first-sight lookup asking a
    /// store that never recorded the batch - appends exactly the refused batch and parks its stage.
    #[test]
    fn a_step_whose_append_is_refused_fails_and_the_next_step_appends_the_refused_batch() {
        let (refused_out, next_out, _) = refuse_then_accept(&["step"]);
        assert_eq!(refused_out.trim(), "", "a failed step prints no step line");
        assert!(
            next_out.starts_with("{\"wave\":[{\"id\":\"a/implementer#0\""),
            "the next step parks its stage; stdout: {next_out}"
        );
    }

    /// GIVEN a log whose derived events were recorded BEFORE the group stamp - each keyed, none
    /// grouped - for exactly the tree the project holds,
    /// WHEN `rigger graph build` runs over it,
    /// THEN the group lookup found no generation for any identity, so the build re-emits the live
    /// index once, every batch whole and in walk order, each event grouped; and a `rigger step` after
    /// it - the other sink, in another process - appends no derived event, because every lookup now
    /// answers the build's stamped recording.
    ///
    /// No migration rewrites a recorded event: the unstamped copies are still in the log afterwards.
    #[test]
    fn a_log_recorded_before_the_group_stamp_re_emits_the_live_index_once_then_answers() {
        let dir = ingestable_project();
        let root = dir.path();
        tree(
            root,
            &[
                (UNCHANGED, "pub fn kept() {}\n"),
                (CHANGED, "pub fn before() {}\n"),
            ],
        );
        let now = walk(root);
        let all: Vec<String> = now.iter().flat_map(|(_, _, keys)| keys.clone()).collect();
        let mut legacy = Vec::new();
        rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |keyed, _| {
            for (key, event) in keyed {
                legacy.push((*event).clone().with_meta(META_REPLAY_KEY, key.as_str()));
            }
        });
        with_run_store(root, |store| {
            store
                .append(rigger::conductor::STREAM, ExpectedRevision::Any, &legacy)
                .unwrap();
        });
        assert_eq!(
            meta_of(&derived(&read_run_events(root)), META_GROUP),
            vec![String::new(); all.len()],
            "sanity: the pre-stamp log carries no group"
        );
        assert_eq!(
            answered(root, &now),
            vec![None; now.len()],
            "a log with no stamp answers no generation for any identity"
        );

        let before = read_run_events(root).len();
        graph_build(root, "the first build over a pre-stamp log");
        let appended = derived_since(root, before);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            all,
            "the first ingest over a pre-stamp log re-emits every batch of the live index, once"
        );
        assert_eq!(meta_of(&appended, META_GROUP), identities_of(&all));
        assert_eq!(
            meta_of(
                &derived(&read_run_events(root))[..all.len()],
                META_REPLAY_KEY
            ),
            all,
            "the unstamped recordings are left in place, never rewritten"
        );
        assert_eq!(
            answered(root, &now),
            now.iter()
                .map(|(_, g, _)| Some(g.clone()))
                .collect::<Vec<_>>()
        );

        let settled = read_run_events(root).len();
        step_line(root, "the step after the stamping build");
        assert_eq!(
            meta_of(&derived_since(root, settled), META_REPLAY_KEY),
            Vec::<String>::new(),
            "the step answers every batch from the build's stamp and appends nothing"
        );
    }

    const SPARED: &str = "src/spared.rs";
    const STALE: &str = "src/stale.rs";
    const FOLLOWED: &str = "src/followed.rs";

    /// A generation no walk of the tree answers.
    const OLD: &str = "0ld";

    /// Drive two ingest sinks, `rigger <first>` then `rigger <second>`, over a project whose log
    /// holds hand-built ledger entries and no walked batch:
    ///
    /// - `src/spared.rs`: an entry at the tree's current generation, its only recording;
    /// - `src/stale.rs`: an entry at a generation the tree no longer holds, its only recording;
    /// - `src/followed.rs`: an entry at the current generation FOLLOWED by a derived row of an old
    ///   one, so its latest recording is a derived row.
    ///
    /// Asserts, of the first sink: it appends every walked batch but the spared file's, whole,
    /// grouped and in walk order - the entry at the current generation answers the sink's check, the
    /// stale entry does not, and the identity whose latest recording is a derived row is answered by
    /// that row as before. Then the group lookup answers every walked identity at its current
    /// generation, the spared one still from its entry; and the second sink - another process -
    /// appends no derived event. Returns both sinks' stdout and the count of events the first
    /// appended, for each sink's own report line.
    fn an_entry_at_the_current_generation_spares_its_batch(
        first: &[&str],
        second: &[&str],
    ) -> (String, String, usize) {
        let dir = ingestable_project();
        let root = dir.path();
        tree(
            root,
            &[
                (FOLLOWED, "pub fn followed() {}\n"),
                (SPARED, "pub fn spared() {}\n"),
                (STALE, "pub fn stale() {}\n"),
            ],
        );
        let now = walk(root);
        let of = |file: &str| {
            let identity = format!("gc/{file}");
            let (_, generation, keys) = now
                .iter()
                .find(|(id, _, _)| *id == identity)
                .unwrap_or_else(|| panic!("the walk emits {identity}"));
            (identity, generation.clone(), keys.len())
        };
        let (spared, spared_generation, spared_events) = of(SPARED);
        let (stale, _, stale_events) = of(STALE);
        let (followed, followed_generation, followed_events) = of(FOLLOWED);
        let entry = |file: &str, generation: &str, n: usize| {
            common::fixtures::generation_ingested("gc", file, generation, "b10b", false).event(n)
        };
        with_run_store(root, |store| {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        entry(SPARED, &spared_generation, spared_events),
                        entry(STALE, OLD, stale_events),
                        entry(FOLLOWED, &followed_generation, followed_events),
                        keyed_derived_event(
                            Event::new(
                                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                                b"{}".to_vec(),
                            ),
                            &format!("{followed}@{OLD}#0"),
                        ),
                    ],
                )
                .unwrap();
        });
        let planted: Vec<Option<String>> = now
            .iter()
            .map(|(identity, generation, _)| {
                if *identity == spared {
                    Some(generation.clone())
                } else if *identity == stale || *identity == followed {
                    Some(OLD.to_string())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            answered(root, &now),
            planted,
            "before any sink runs, the lookup answers the spared entry's generation, the stale \
             entry's, the derived row that follows an entry, and no other identity"
        );
        let others: Vec<String> = now
            .iter()
            .filter(|(identity, _, _)| *identity != spared)
            .flat_map(|(_, _, keys)| keys.clone())
            .collect();
        assert_eq!(
            others.len() + spared_events,
            now.iter().map(|(_, _, keys)| keys.len()).sum::<usize>(),
            "sanity: the walk emits the spared file's batch alongside the others"
        );
        assert_ne!(spared_events, 0, "sanity: the spared batch holds events");

        let before = read_run_events(root).len();
        let (first_out, err, ok) = run_rigger(root, first);
        assert!(
            ok,
            "rigger {first:?} must succeed; stdout: {first_out}; stderr: {err}"
        );
        let appended = derived_since(root, before);
        assert_eq!(
            meta_of(&appended, META_REPLAY_KEY),
            others,
            "the sink appends every batch but the one an entry records at its current generation"
        );
        assert_eq!(meta_of(&appended, META_GROUP), identities_of(&others));
        assert_eq!(
            answered(root, &now),
            now.iter()
                .map(|(_, g, _)| Some(g.clone()))
                .collect::<Vec<_>>(),
            "after the sink every walked identity answers its current generation"
        );
        let head = with_run_store(root, |store| {
            store
                .latest_in_group(rigger::conductor::STREAM, &spared)
                .unwrap()
                .map(|head| head.type_)
        });
        assert_eq!(
            head.as_deref(),
            Some(rigger::retention::TYPE_GENERATION_INGESTED),
            "the spared identity's latest recording is still its entry"
        );

        let settled = read_run_events(root).len();
        let (second_out, err, ok) = run_rigger(root, second);
        assert!(
            ok,
            "rigger {second:?} must succeed; stdout: {second_out}; stderr: {err}"
        );
        assert_eq!(
            meta_of(&derived_since(root, settled), META_REPLAY_KEY),
            Vec::<String>::new(),
            "the other sink answers every batch as recorded, the spared one by its entry"
        );
        (first_out, second_out, others.len())
    }

    /// GIVEN a log holding a ledger entry of one file at the tree's current generation, a stale
    /// entry of another, and a third file whose entry a derived row of an old generation follows,
    /// WHEN `rigger step` ingests the tree,
    /// THEN the step parks its stage and appends every batch but the first file's, and a
    /// `rigger graph build` after it appends nothing and reports nothing ingested.
    #[test]
    fn a_step_spares_the_batch_a_ledger_entry_records_at_its_current_generation() {
        let (step_out, build_out, _) =
            an_entry_at_the_current_generation_spares_its_batch(&["step"], &["graph", "build"]);
        assert!(
            step_out.starts_with("{\"wave\":[{\"id\":\"a/implementer#0\""),
            "the step parks its stage; stdout: {step_out}"
        );
        assert!(
            build_out.starts_with("graph build: ingested 0 code-ingest event(s) into "),
            "the build after it reports nothing ingested; stdout: {build_out}"
        );
    }

    /// GIVEN the same log,
    /// WHEN `rigger graph build` ingests the tree,
    /// THEN the build appends every batch but the first file's and reports exactly that count, and a
    /// `rigger step` after it parks its stage and appends nothing.
    #[test]
    fn a_graph_build_spares_the_batch_a_ledger_entry_records_at_its_current_generation() {
        let (build_out, step_out, appended) =
            an_entry_at_the_current_generation_spares_its_batch(&["graph", "build"], &["step"]);
        assert!(
            build_out.starts_with(&format!(
                "graph build: ingested {appended} code-ingest event(s) into "
            )),
            "the build reports every event but the spared batch's; stdout: {build_out}"
        );
        assert!(
            step_out.starts_with("{\"wave\":[{\"id\":\"a/implementer#0\""),
            "the step after it parks its stage; stdout: {step_out}"
        );
    }
}

/// CONTRACT at the product's store composition, in both feature lanes: two projects share one
/// file-backed `events.db` behind their namespaces, both recording the same batch identity on the
/// same run stream name. Each project's lookup answers ITS OWN newest recording - position, type
/// and metadata - and a third project that recorded nothing answers none, so one project's
/// generation can never suppress another project's ingest.
#[test]
fn the_group_lookup_answers_each_project_namespace_only_its_own_recording() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.db");
    let backend = Store::open(path.to_str().unwrap()).unwrap();
    let stream = rigger::conductor::STREAM;
    let record = |project: &str, generation: &str| {
        Namespaced::new(&backend, project)
            .append(
                stream,
                ExpectedRevision::Any,
                &[keyed_derived_event(
                    Event::new(
                        rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        b"{}".to_vec(),
                    ),
                    &format!("gc/src/a.rs@{generation}#0"),
                )],
            )
            .unwrap()
    };
    record("alpha", "h1");
    let beta_at = record("beta", "h2")
        .placed()
        .map(|(_, p)| p)
        .collect::<Vec<_>>();
    let alpha_at = record("alpha", "h3")
        .placed()
        .map(|(_, p)| p)
        .collect::<Vec<_>>();

    let head = |project: &str| {
        Namespaced::new(&backend, project)
            .latest_in_group(stream, "gc/src/a.rs")
            .unwrap()
    };
    let expected = |position: u64, generation: &str| GroupHead {
        position,
        type_: rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED.to_string(),
        meta: BTreeMap::from([
            (META_GROUP.to_string(), "gc/src/a.rs".to_string()),
            (
                META_REPLAY_KEY.to_string(),
                format!("gc/src/a.rs@{generation}#0"),
            ),
        ]),
    };
    assert_eq!(head("alpha"), Some(expected(alpha_at[0], "h3")));
    assert_eq!(head("beta"), Some(expected(beta_at[0], "h2")));
    assert_eq!(
        head("gamma"),
        None,
        "a project that recorded nothing answers none"
    );

    let generation = |project: &str| {
        latest_generation(&Namespaced::new(&backend, project), stream, "gc/src/a.rs").unwrap()
    };
    assert_eq!(
        [generation("alpha"), generation("beta"), generation("gamma")],
        [Some("h3".to_string()), Some("h2".to_string()), None],
        "the domain reader over each namespace answers that project's own latest generation"
    );
}
