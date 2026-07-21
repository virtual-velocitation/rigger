//! The sleep-phase consolidation distiller (spec 27): it folds a project's
//! OLDER-THAN-CURRENT-RUN `DecisionMade` and `ReviewFinding` events into
//! deduplicated, per-file **digest nodes** under a rebuildable pool, so grounding
//! stays lean over months without a manual `reset --runs`.
//!
//! It is modeled directly on [`crate::playbooks`], which already consolidates
//! `LessonLearned` into a rebuildable projection. The shape is mirrored but INVERTED:
//! where a playbook keys by lesson text and unions the trigger files, a digest keys by
//! FILE and consolidates every stale finding/decision about that file into one node.
//!
//! The pool is a **rebuildable projection of the event log**, never hand-edited state:
//! [`rebuild`] clears the rigger-managed files and re-derives every digest from the
//! event stream, so the pool reconstructs deterministically from the same events the
//! graph is projected from. Like `playbooks.rs`, the distiller introduces NO new event
//! type - it only READS the existing [`TYPE_DECISION_MADE`] / [`TYPE_REVIEW_FINDING`]
//! stream, and it DELETES nothing, so the raw events stay retrievable via `rigger peers`.
//!
//! Scope is by RUN BOUNDARY, reusing the single attribution authority
//! ([`crate::run::run_attribution`] + [`crate::run::current_run_id`]) that `reset --runs`
//! and the `rigger peers` LIVE/HISTORICAL labels already use: only findings/decisions
//! OLDER than the current run consolidate; the current run's items stay raw. This is the
//! AUTOMATIC form of what `reset --runs` does by hand. `LessonLearned` is OUT of scope -
//! `playbooks.rs` remains the authority for lessons, which are preserved untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_REVIEW_FINDING};
use crate::eventstore::Event;
use crate::run::RunOf;

/// The subdirectory (under a project's `.rigger/`) the digest pool lives in.
pub const POOL_SUBDIR: &str = "digests";

/// FNV-1a/64 over `bytes` with the SAME fixed constants as `playbooks::fnv1a_64`,
/// `main::fnv1a_64`, and `conductor::input_digest`, so the crate keeps ONE stable-hash
/// idiom: a digest's slug is identical across processes, machines, and builds (unlike
/// `DefaultHasher`), which is what makes the pool a reproducible projection - the same
/// file + summary always rebuilds to the same file name.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Normalize a summary for the fold's dedup: trim, then collapse every internal run of
/// ASCII whitespace to a single space, so two stale findings whose text differs only by
/// incidental reflowing/indentation consolidate into ONE digest line. Kept LOCAL to this
/// projection (like `playbooks`'s own local trim) rather than reaching into the private
/// `conductor::normalize_ws`, whose concern is the distinct criterion-supersede match.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One distilled digest: every OLDER-THAN-CURRENT-RUN finding/decision about ONE file,
/// consolidated into a single node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    /// The stable slug (`digest-<16 hex>` of the file + summary) that names the pool file
    /// and the frontmatter `id`. Deriving it from the (file, summary) projection makes the
    /// SAME consolidated content rebuild to the SAME digest file across runs and machines.
    pub id: String,
    /// The trigger file this digest consolidates the stale findings/decisions ABOUT.
    pub file: String,
    /// The per-file consolidated summary: the distinct normalized summaries of every folded
    /// finding/decision about [`Digest::file`], sorted and joined one per line (dedup by
    /// normalized text, so a repeated summary appears once).
    pub summary: String,
    /// The contributing run ids: the sorted set of runs whose findings/decisions folded into
    /// this digest. A pre-boundary item (recorded before any `RunStarted`) contributes no run
    /// id, so it counts toward [`Digest::count`] without adding a run.
    pub runs: Vec<String>,
    /// How many `DecisionMade`/`ReviewFinding` events about this file folded into the digest
    /// (>= 1), counting each contributing event - including duplicate-summary ones the dedup
    /// collapsed - exactly as `playbooks` counts folded lessons.
    pub count: usize,
}

/// One `DecisionMade` event's payload. A LOCAL decode of the stable [`TYPE_DECISION_MADE`]
/// shape; the distiller needs only the summary text and the files it `governs`.
#[derive(serde::Deserialize)]
struct DecisionEvent {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    governs: Vec<String>,
}

/// One `ReviewFinding` event's payload. A LOCAL decode of the stable [`TYPE_REVIEW_FINDING`]
/// shape; the distiller needs only the summary text and the files it is `about`.
#[derive(serde::Deserialize)]
struct FindingEvent {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}

/// Fold the OLDER-THAN-CURRENT-RUN `DecisionMade`/`ReviewFinding` events into per-file
/// digests: every stale finding/decision ABOUT a file collapses into ONE digest for that
/// file, whose summary is the deduplicated union of their normalized texts, whose `runs`
/// is the set of contributing run ids, and whose `count` is how many events folded.
///
/// The old-vs-current partition reuses the single run-boundary authority
/// ([`crate::run::run_attribution`] against [`crate::run::current_run_id`]): an event
/// folds only when its attribution is NOT live in the current run, so the current run's
/// raw items are left un-consolidated. `LessonLearned`, lifecycle, and empty-summary /
/// no-file events contribute nothing. Keyed and returned in deterministic (file-sorted)
/// order so a rebuild is byte-reproducible from the log.
pub fn distill(events: &[Event]) -> Vec<Digest> {
    let active = crate::run::current_run_id(events);
    let attribution = crate::run::run_attribution(events);
    // file -> (distinct normalized summaries, contributing run ids, folded event count).
    let mut folded: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>, usize)> = BTreeMap::new();
    for (i, e) in events.iter().enumerate() {
        let (raw_summary, files) = match e.type_.as_str() {
            TYPE_DECISION_MADE => {
                let Ok(d) = serde_json::from_slice::<DecisionEvent>(&e.data) else {
                    continue;
                };
                (d.summary, d.governs)
            }
            TYPE_REVIEW_FINDING => {
                let Ok(f) = serde_json::from_slice::<FindingEvent>(&e.data) else {
                    continue;
                };
                (f.summary, f.about)
            }
            // LessonLearned, RunStarted, and every lifecycle event are out of scope.
            _ => continue,
        };
        // Only OLDER-THAN-CURRENT-RUN items consolidate; a current-run item stays raw. The
        // partition reuses the single run-boundary authority (run_attribution) rather than a
        // second inline RunStarted scan: fold iff the event is NOT live in the active run.
        let Some(run_of) = attribution.get(&i) else {
            continue;
        };
        if run_of.is_live(active.as_deref()) {
            continue;
        }
        let summary = normalize(&raw_summary);
        if summary.is_empty() {
            continue;
        }
        // A pre-boundary item (before any RunStarted) belongs to no run: it still folds by
        // AGE but contributes no run id.
        let run_id = match run_of {
            RunOf::Run(r) => Some(r.clone()),
            _ => None,
        };
        // Dedup this event's file list FIRST, so a governs/about list that names the SAME
        // file twice folds the event ONCE for that file: count = number of EVENTS about the
        // file, exactly as playbooks::distill increments once per event OUTSIDE its file
        // loop. A BTreeSet also keeps the per-event fold order deterministic.
        let files: BTreeSet<&str> = files
            .iter()
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
            .collect();
        for f in files {
            let entry = folded.entry(f.to_string()).or_default();
            entry.0.insert(summary.clone());
            if let Some(r) = &run_id {
                entry.1.insert(r.clone());
            }
            entry.2 += 1;
        }
    }
    folded
        .into_iter()
        .map(|(file, (summaries, runs, count))| {
            let summary = summaries.into_iter().collect::<Vec<_>>().join("\n");
            let id = format!(
                "digest-{:016x}",
                fnv1a_64(format!("{file}\n{summary}").as_bytes())
            );
            Digest {
                id,
                file,
                summary,
                runs: runs.into_iter().collect(),
                count,
            }
        })
        .collect()
}

/// A digest's YAML frontmatter fields, serialized into the native agent-file header.
#[derive(Serialize)]
struct Frontmatter<'a> {
    id: &'a str,
    file: &'a str,
    runs: &'a [String],
    count: usize,
}

/// Render a digest as a native agent-file (`---\n<yaml>\n---\n<body>`): YAML frontmatter
/// carrying the id, the trigger file, the contributing runs, and the fold count, then the
/// consolidated summary as the body. The output round-trips through
/// [`crate::config::split_frontmatter`], the same parser the `.rigger/agents/*.md`
/// definitions use.
pub fn render(d: &Digest) -> String {
    let fm = Frontmatter {
        id: &d.id,
        file: &d.file,
        runs: &d.runs,
        count: d.count,
    };
    // serde_yaml emits the mapping with a trailing newline, so the closing delimiter and
    // body attach directly. An unserializable struct is impossible here (all fields are
    // owned primitives), so the fallback is inert.
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    format!("---\n{yaml}---\n{}\n", d.summary)
}

/// Reconstruct the digest pool at `dir` from the event stream: distill the stale
/// findings/decisions, CLEAR the rigger-managed pool files (every `*.md` under `dir`), then
/// write one file per digest. Because the pool is a pure projection, clearing first is what
/// lets a digest the current log no longer justifies drop out - the rebuild is the whole
/// pool, not a merge. The raw events are never touched, so they stay retrievable via
/// `rigger peers`. Returns the distilled digests it wrote.
pub fn rebuild(events: &[Event], dir: &Path) -> io::Result<Vec<Digest>> {
    let digests = distill(events);
    fs::create_dir_all(dir)?;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|x| x.to_str()) == Some("md") {
            fs::remove_file(&path)?;
        }
    }
    for d in &digests {
        fs::write(dir.join(format!("{}.md", d.id)), render(d))?;
    }
    Ok(digests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::TYPE_RUN_STARTED;
    use serde_json::json;

    fn run_started(run: &str) -> Event {
        Event::new(
            TYPE_RUN_STARTED,
            serde_json::to_vec(&json!({ "run": run })).unwrap(),
        )
    }

    fn decision(summary: &str, governs: &[&str]) -> Event {
        Event::new(
            TYPE_DECISION_MADE,
            serde_json::to_vec(&json!({
                "id": format!("d-{summary}"),
                "summary": summary,
                "governs": governs,
            }))
            .unwrap(),
        )
    }

    fn finding(summary: &str, about: &[&str]) -> Event {
        Event::new(
            TYPE_REVIEW_FINDING,
            serde_json::to_vec(&json!({
                "id": format!("f-{summary}"),
                "summary": summary,
                "about": about,
            }))
            .unwrap(),
        )
    }

    #[test]
    fn distill_folds_older_than_current_run_findings_and_decisions_per_file() {
        // spec 27 criterion 1 (this unit OWNS the older-than-current-run per-file fold):
        // given a log with an OLD run A and a CURRENT run B, A's findings/decisions ABOUT
        // file F fold into ONE digest for F (and G's into its own), while B's current-run
        // items are left RAW and un-consolidated (no digest, not merged into F).
        let events = vec![
            run_started("A"),
            decision("guard the checked add", &["src/f.rs"]),
            finding("missing null check", &["src/f.rs"]),
            // Same normalized summary as the first decision (extra whitespace): dedups to
            // one line yet still counts as a folded event.
            decision("guard   the checked   add", &["src/f.rs"]),
            decision("use a BTreeMap", &["src/g.rs"]),
            // A decision governing TWO files contributes to BOTH digests.
            finding("both files leak", &["src/f.rs", "src/g.rs"]),
            run_started("B"),
            // Current run: MUST stay raw / un-consolidated.
            decision("current work on f", &["src/f.rs"]),
            finding("current finding on g", &["src/g.rs"]),
        ];

        let digests = distill(&events);
        assert_eq!(
            digests.len(),
            2,
            "only the OLD run folds -> one digest per stale file (F, G); got {digests:?}"
        );

        let f = digests
            .iter()
            .find(|d| d.file == "src/f.rs")
            .expect("a digest consolidating the stale F items");
        // Three stale events touched F: the two guard decisions (dedup to one line) + the
        // null-check finding + the two-file leak finding = four contributing events.
        assert_eq!(
            f.count, 4,
            "every stale F event folds into the count, including the dedup-collapsed duplicate"
        );
        assert!(
            f.summary.contains("guard the checked add"),
            "the fold keeps the decision text"
        );
        assert!(
            f.summary.contains("missing null check") && f.summary.contains("both files leak"),
            "the fold consolidates every stale F finding/decision"
        );
        // dedup-by-normalized-summary: the two guard decisions differ only in internal
        // whitespace, so normalize collapses them to ONE line. F therefore has exactly
        // three distinct summary lines (guard / null-check / leak). This reddens if the
        // whitespace-collapse dedup breaks (a trim-only normalize leaves four lines) - a
        // substring `matches(...).count()==1` could not catch that, since the 3-space
        // variant never matches the single-space substring.
        assert_eq!(
            f.summary.lines().count(),
            3,
            "the whitespace-variant duplicate collapses: F folds to three distinct lines"
        );
        assert!(
            !f.summary.contains("current work on f"),
            "the CURRENT run's F decision stays raw - it must NOT fold into the digest"
        );
        assert_eq!(
            f.runs,
            vec!["A".to_string()],
            "F's stale items all belong to run A"
        );
        assert!(f.id.starts_with("digest-"), "stable digest slug");

        let g = digests
            .iter()
            .find(|d| d.file == "src/g.rs")
            .expect("a digest consolidating the stale G items");
        // The BTreeMap decision + the two-file leak finding = two stale G events.
        assert_eq!(g.count, 2);
        assert!(
            g.summary.contains("use a BTreeMap") && g.summary.contains("both files leak"),
            "G's digest consolidates only its own stale items"
        );
        assert!(
            !g.summary.contains("current finding on g"),
            "the CURRENT run's G finding stays raw"
        );
        assert_eq!(g.runs, vec!["A".to_string()]);
    }

    #[test]
    fn lessons_are_preserved_outside_the_distiller_and_left_to_playbooks() {
        // spec 27 criterion 3 (this unit OWNS lesson-preservation; it does NOT own the fold
        // [c1] or raw-retrievability [c2]): a `LessonLearned` is OUTSIDE the distiller's
        // scope. `distill` matches only DecisionMade/ReviewFinding by TYPE (the `_ =>
        // continue` arm), so a lesson - even an OLD one about the SAME file a real digest
        // consolidates - is NEVER folded into a digest, and `playbooks.rs` stays the sole
        // authority that consolidates it (d27-plan-lessons). Two anti-vacuous probes pin the
        // by-TYPE exclusion: (a) a lesson about a file with NO decision/finding produces NO
        // digest at all (a type-inclusion regression would mint one); (b) a lesson about file
        // F, which DOES have a real digest, is absent from F's summary and never inflates
        // F's count. A lesson is `RunOf::Lesson` (never live), so the AGE filter would let it
        // fold - only the TYPE match keeps it out, which is exactly what these probes guard.
        use crate::contextgraph::TYPE_LESSON_LEARNED;
        let lesson = |summary: &str, about: &[&str]| {
            Event::new(
                TYPE_LESSON_LEARNED,
                serde_json::to_vec(&json!({
                    "id": format!("lesson-{summary}"),
                    "summary": summary,
                    "about": about,
                }))
                .unwrap(),
            )
        };

        let events = vec![
            run_started("A"),
            // OLD lesson about F - the SAME file a real digest will consolidate.
            lesson("always close the scratch file", &["src/f.rs"]),
            // OLD decision + finding about F: these DO fold into F's digest.
            decision("guard the checked add", &["src/f.rs"]),
            finding("missing null check", &["src/f.rs"]),
            // OLD lesson about a file NO decision/finding touches: it must mint NO digest.
            lesson("prefer a BTreeMap for determinism", &["src/lesson_only.rs"]),
            run_started("B"),
            // Current-run decision: stays raw (owned by c1, here only as realistic context).
            decision("current work on f", &["src/f.rs"]),
        ];

        let digests = distill(&events);

        // (a) Exactly ONE digest - for F, from the decision + finding. The two OLD lessons
        // mint NOTHING: a lesson is excluded by TYPE, so `src/lesson_only.rs` (touched ONLY
        // by a lesson) gets no digest. A type-inclusion regression mints a second digest here.
        assert_eq!(
            digests.len(),
            1,
            "only the decision/finding fold; the lessons mint no digest; got {digests:?}"
        );
        assert!(
            digests.iter().all(|d| d.file != "src/lesson_only.rs"),
            "a file touched ONLY by a lesson must produce NO digest - lessons are out of scope"
        );

        let f = digests
            .iter()
            .find(|d| d.file == "src/f.rs")
            .expect("F's digest consolidates its decision + finding");
        // (b) F's digest folds exactly the two non-lesson events; the OLD lesson about F is
        // neither counted nor summarized. Were lessons folded by type, count would be 3 and
        // the lesson text would appear in the summary.
        assert_eq!(
            f.count, 2,
            "only the decision + finding fold into F; the lesson about F does NOT inflate count"
        );
        assert!(
            !f.summary.contains("always close the scratch file"),
            "the lesson text must NOT appear in F's digest - lessons are excluded by TYPE"
        );
        assert!(
            f.summary.contains("guard the checked add") && f.summary.contains("missing null check"),
            "F's digest still consolidates its real decision + finding"
        );

        // Preservation / authority: the SAME log the distiller just processed still yields
        // BOTH lessons through `playbooks::distill` - `playbooks.rs` remains the sole
        // authority for lessons, and the distiller left the lesson stream untouched.
        let playbooks = crate::playbooks::distill(&events);
        assert_eq!(
            playbooks.len(),
            2,
            "both distinct lessons survive into the playbook pool; got {playbooks:?}"
        );
        assert!(
            playbooks
                .iter()
                .any(|p| p.summary == "always close the scratch file"
                    && p.triggers == vec!["src/f.rs".to_string()]),
            "the F lesson is consolidated by playbooks, not by the distiller"
        );
        assert!(
            playbooks
                .iter()
                .any(|p| p.summary == "prefer a BTreeMap for determinism"),
            "the lesson-only-file lesson is consolidated by playbooks, not the distiller"
        );

        // The distiller's mutation authority is the digest POOL alone: `rebuild` writes
        // digest files and NEVER the lesson stream, so lessons are untouched end-to-end.
        // After a rebuild, no digest file on disk carries lesson text, and playbooks still
        // distills BOTH lessons from the same (unchanged) log.
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join(POOL_SUBDIR);
        let written = rebuild(&events, &pool_dir).unwrap();
        assert_eq!(
            written, digests,
            "rebuild writes exactly the distilled digests - no lesson-derived node"
        );
        for entry in fs::read_dir(&pool_dir).unwrap() {
            let body = fs::read_to_string(entry.unwrap().path()).unwrap();
            assert!(
                !body.contains("always close the scratch file")
                    && !body.contains("prefer a BTreeMap for determinism"),
                "no digest file on disk carries lesson text"
            );
        }
        assert_eq!(
            crate::playbooks::distill(&events).len(),
            2,
            "the lesson stream is untouched by the distiller's rebuild - playbooks still folds both"
        );
    }

    #[test]
    fn distill_counts_a_file_repeated_within_one_event_once() {
        // A single stale event whose governs/about names the SAME file twice is still
        // ONE event ABOUT that file: it must add ONE to the file's fold count, never two.
        // count = number of EVENTS folded (d27-u1-digest-shape), mirroring
        // playbooks::distill, which increments once per event OUTSIDE its file loop - so a
        // sloppy duplicate entry in one event's file list never inflates the digest count.
        let events = vec![
            run_started("A"),
            decision("guard the checked add", &["src/h.rs", "src/h.rs"]),
            run_started("B"),
        ];

        let digests = distill(&events);
        assert_eq!(
            digests.len(),
            1,
            "one stale file -> exactly one digest; got {digests:?}"
        );
        let h = &digests[0];
        assert_eq!(h.file, "src/h.rs");
        assert_eq!(
            h.count, 1,
            "a file named twice in ONE event counts that event ONCE, not twice"
        );
        assert_eq!(
            h.summary.lines().count(),
            1,
            "the single decision folds to exactly one summary line"
        );
        assert_eq!(h.runs, vec!["A".to_string()]);
    }

    /// The frontmatter of a rendered digest, decoded back through the SAME native-agent-file
    /// header shape [`render`] emits, so a round-trip proof reconstructs every field.
    #[derive(serde::Deserialize)]
    struct ParsedFrontmatter {
        id: String,
        file: String,
        runs: Vec<String>,
        count: usize,
    }

    #[test]
    fn distill_partitions_old_vs_current_by_the_latest_run_boundary() {
        // spec 27 criterion 4 (this unit OWNS run-boundary scoping): the old-vs-current split
        // is keyed on the LATEST `RunStarted` boundary, using the SAME `run_attribution` +
        // `is_live` authority `reset --runs` prunes by. The distinguishing property vs c1 (which
        // only had a single old run A and current run B): with THREE runs A, B, C where C is
        // current, a MIDDLE run B is STILL old and folds - only C (the latest boundary's run)
        // stays raw. A pre-boundary item (recorded before ANY `RunStarted`) is dead-run noise
        // `reset --runs` drops, so it folds too, by AGE, contributing no run id.
        let events = vec![
            // Pre-boundary (before any RunStarted): historical, folds with no run id.
            decision("legacy note", &["src/f.rs"]),
            run_started("A"),
            decision("run A alpha", &["src/f.rs"]),
            finding("run A beta", &["src/f.rs"]),
            run_started("B"),
            // B is NOT the current run (C is) -> B is OLD and MUST fold.
            decision("run B gamma", &["src/f.rs"]),
            run_started("C"),
            // Only the LATEST run's items stay raw / un-consolidated.
            decision("current C delta", &["src/f.rs"]),
        ];

        let digests = distill(&events);
        assert_eq!(
            digests.len(),
            1,
            "only the LATEST run C is current; every older item folds into F's one digest; got {digests:?}"
        );
        let f = &digests[0];
        assert_eq!(f.file, "src/f.rs");
        // The load-bearing run-boundary assertion: BOTH prior runs A and B fold because the
        // partition is at the LATEST boundary (C). If the split keyed on the FIRST boundary or
        // treated any started run as live, B (or A) would be excluded and this would redden.
        assert_eq!(
            f.runs,
            vec!["A".to_string(), "B".to_string()],
            "both non-current runs A and B are OLD relative to the latest boundary C and fold; \
             the pre-boundary legacy item contributes no run id"
        );
        // Membership follows the same boundary: pre-boundary + A(2) + B(1) fold; C is excluded.
        assert_eq!(
            f.count, 4,
            "the pre-boundary legacy item and both prior runs fold; the current run C does not"
        );
        assert!(
            f.summary.contains("legacy note")
                && f.summary.contains("run A alpha")
                && f.summary.contains("run A beta")
                && f.summary.contains("run B gamma"),
            "every OLDER-than-latest-boundary item consolidates"
        );
        assert!(
            !f.summary.contains("current C delta"),
            "the CURRENT (latest-boundary) run's item stays raw - it must NOT fold"
        );
    }

    #[test]
    fn distill_output_is_deterministic_regardless_of_input_order() {
        // spec 27 criterion 4 (this unit OWNS determinism): identical input yields byte-identical
        // digest output, BY CONSTRUCTION (BTreeMap file-key + BTreeSet summaries/runs -> sorted
        // Vec), so the rendered pool is INDEPENDENT of the order the stale events arrived in.
        // Two logs carry the SAME stale items about the SAME files, scrambled differently.
        let ordered = vec![
            run_started("A"),
            finding("delta bug", &["src/z.rs"]),
            decision("gamma point", &["src/a.rs"]),
            decision("beta note", &["src/z.rs"]),
            finding("alpha issue", &["src/a.rs"]),
            run_started("B"),
        ];
        let scrambled = vec![
            run_started("A"),
            finding("alpha issue", &["src/a.rs"]),
            finding("delta bug", &["src/z.rs"]),
            decision("beta note", &["src/z.rs"]),
            decision("gamma point", &["src/a.rs"]),
            run_started("B"),
        ];

        let render_pool =
            |evs: &[Event]| distill(evs).iter().map(render).collect::<Vec<_>>().join("");
        let from_ordered = render_pool(&ordered);
        let from_scrambled = render_pool(&scrambled);
        // The headline determinism proof: input order does not leak into the output. A HashMap
        // file-key or an insertion-ordered Vec of summaries would make these two differ.
        assert_eq!(
            from_ordered, from_scrambled,
            "reordered stale input yields byte-identical digest output (determinism by construction)"
        );
        // Identical input is byte-identical too (the literal criterion wording).
        assert_eq!(from_ordered, render_pool(&ordered));

        // The by-construction sort is directly observable, and directly load-bearing: `ordered`
        // inserts z.rs before a.rs and, within a.rs, "gamma" before "alpha", so a non-sorting
        // container would emit the reverse order here.
        let digests = distill(&ordered);
        assert_eq!(
            digests.iter().map(|d| d.file.clone()).collect::<Vec<_>>(),
            vec!["src/a.rs".to_string(), "src/z.rs".to_string()],
            "digests emit in deterministic file-sorted order (a.rs before z.rs)"
        );
        let a = digests.iter().find(|d| d.file == "src/a.rs").unwrap();
        assert_eq!(
            a.summary, "alpha issue\ngamma point",
            "summary lines emit in deterministic sorted order (alpha before gamma)"
        );
        let z = digests.iter().find(|d| d.file == "src/z.rs").unwrap();
        assert_eq!(z.summary, "beta note\ndelta bug");
    }

    #[test]
    fn a_rendered_digest_and_its_rebuilt_pool_file_round_trip_through_split_frontmatter() {
        // spec 27 criterion 4 (this unit OWNS the deterministic pool FORMAT, per the
        // adj-u27-1 carry-forward that named c4 for determinism): render()'s output - and the
        // rebuild() pool it writes to disk - must be a WELL-FORMED native agent-file that parses
        // back through the SAME `config::split_frontmatter` seam the `.rigger/agents/*.md`
        // definitions use, reconstructing every frontmatter field (id/file/runs/count) and the
        // consolidated summary body. A folded summary that itself contains a bare `---` line must
        // NOT mis-split: the FIRST closing `\n---` is always the frontmatter's, never a dashes
        // line inside the body.
        let events = vec![
            run_started("A"),
            decision("guard the checked add", &["src/f.rs"]),
            // A folded summary whose own text is a dashes line: the render output must still
            // split at the frontmatter delimiter, not here.
            finding("--- section break follows", &["src/f.rs"]),
            decision("use a BTreeMap", &["src/g.rs"]),
            run_started("B"),
        ];
        let digests = distill(&events);
        assert!(!digests.is_empty(), "there are stale items to render");

        for d in &digests {
            let rendered = render(d);
            let (front, body) = crate::config::split_frontmatter(&rendered)
                .expect("a rendered digest is a well-formed frontmatter document");
            // The body is EXACTLY the consolidated summary plus render's single trailing newline,
            // even when the summary contains a `---` line (the closing delimiter came first).
            assert_eq!(
                body,
                format!("{}\n", d.summary),
                "the body round-trips to the consolidated summary"
            );
            let fm: ParsedFrontmatter =
                serde_yaml::from_str(front).expect("the frontmatter parses back as YAML");
            assert_eq!(fm.id, d.id, "id round-trips");
            assert_eq!(fm.file, d.file, "trigger file round-trips");
            assert_eq!(fm.runs, d.runs, "contributing runs round-trip");
            assert_eq!(fm.count, d.count, "fold count round-trips");
        }

        // Drive rebuild() (the on-disk projection) through the same guarantee: rebuilding the
        // SAME log into two separate pools writes byte-identical files, each equal to render(d)
        // and each round-tripping through split_frontmatter. This is "byte-identical digest
        // output" in its most literal, on-disk form.
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let written1 = rebuild(&events, dir1.path()).unwrap();
        let written2 = rebuild(&events, dir2.path()).unwrap();
        assert_eq!(
            written1, written2,
            "rebuild is deterministic: the same log yields the same digests"
        );
        for d in &written1 {
            let bytes1 = fs::read_to_string(dir1.path().join(format!("{}.md", d.id))).unwrap();
            let bytes2 = fs::read_to_string(dir2.path().join(format!("{}.md", d.id))).unwrap();
            assert_eq!(
                bytes1, bytes2,
                "rebuild writes byte-identical pool files across runs"
            );
            assert_eq!(
                bytes1,
                render(d),
                "the on-disk pool file is exactly the rendered digest"
            );
            let (_front, body) = crate::config::split_frontmatter(&bytes1)
                .expect("the on-disk pool file round-trips through split_frontmatter");
            assert_eq!(body, format!("{}\n", d.summary));
        }
    }
}
