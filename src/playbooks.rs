//! The post-run distiller: it folds a run's `LessonLearned` events into a
//! deduplicated, trigger-scoped **playbook pool** under `.rigger/playbooks/`
//! (spec 13b, unit 2).
//!
//! A playbook is one distilled lesson rendered in rigger's native agent-file shape
//! (YAML frontmatter + a markdown body, the exact `.rigger/agents/<id>.md` format
//! `config::split_frontmatter` parses). Its frontmatter carries the TRIGGER PREDICATE -
//! the blast-radius files the lesson is `about` - so the injector can rank a playbook by
//! how much its trigger scope overlaps an agent's grounded seed (the relevance ordering
//! `conductor::write_capped_lessons` applies to the lessons slice).
//!
//! The pool is a **rebuildable projection of the event log**, never hand-edited state:
//! [`rebuild`] clears the rigger-managed files and re-derives every playbook from the
//! `LessonLearned` stream, so `rigger playbooks --rebuild` reconstructs it deterministically
//! from the same events the graph is projected from. No new event type is introduced - the
//! distiller only READS the existing [`TYPE_LESSON_LEARNED`] stream.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use crate::contextgraph::TYPE_LESSON_LEARNED;
use crate::eventstore::Event;

/// The subdirectory (under a project's `.rigger/`) the playbook pool lives in.
pub const POOL_SUBDIR: &str = "playbooks";

/// FNV-1a/64 over `bytes` with the SAME fixed constants as `main::fnv1a_64` and
/// `conductor::input_digest`, so the crate keeps ONE stable-hash idiom: a playbook's slug
/// is identical across processes, machines, and builds (unlike `DefaultHasher`), which is
/// what makes the pool a reproducible projection - the same lesson text always rebuilds to
/// the same file name.
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

/// One distilled playbook: a deduplicated lesson, the blast-radius files that trigger it,
/// and how many `LessonLearned` events folded into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playbook {
    /// The stable slug (`playbook-<16 hex>` of the summary) that names the pool file and
    /// the frontmatter `id`. Deriving it from the summary makes the SAME lesson dedup to
    /// the SAME playbook across runs.
    pub id: String,
    /// The distilled lesson body (the deduplicated `LessonLearned` summary).
    pub summary: String,
    /// The TRIGGER PREDICATE: the sorted union of every folded lesson's `about` files, the
    /// blast radius the injector overlaps against an agent's grounded seed to rank relevance.
    pub triggers: Vec<String>,
    /// How many `LessonLearned` events collapsed into this one playbook (>= 1).
    pub lessons: usize,
}

/// One lesson event's payload. A LOCAL decode of the stable [`TYPE_LESSON_LEARNED`] shape
/// (`{id, summary, about}`); the distiller needs only the text and its trigger scope.
#[derive(serde::Deserialize)]
struct LessonEvent {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}

/// Fold the `LessonLearned` events into the deduplicated playbook pool: lessons carrying the
/// SAME (trimmed) summary collapse into ONE playbook whose trigger scope is the UNION of
/// their `about` files and whose `lessons` count is how many folded. Non-lesson events and
/// empty-summary lessons are skipped. Keyed and returned in deterministic (summary-sorted)
/// order so a rebuild is byte-reproducible from the log.
pub fn distill(events: &[Event]) -> Vec<Playbook> {
    // summary -> (union of trigger files, folded count).
    let mut folded: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    for e in events {
        if e.type_ != TYPE_LESSON_LEARNED {
            continue;
        }
        let Ok(l) = serde_json::from_slice::<LessonEvent>(&e.data) else {
            continue;
        };
        let summary = l.summary.trim().to_string();
        if summary.is_empty() {
            continue;
        }
        let entry = folded.entry(summary).or_default();
        for f in l.about {
            let f = f.trim();
            if !f.is_empty() {
                entry.0.insert(f.to_string());
            }
        }
        entry.1 += 1;
    }
    folded
        .into_iter()
        .map(|(summary, (triggers, lessons))| Playbook {
            id: format!("playbook-{:016x}", fnv1a_64(summary.as_bytes())),
            summary,
            triggers: triggers.into_iter().collect(),
            lessons,
        })
        .collect()
}

/// A playbook's YAML frontmatter fields, serialized into the native agent-file header.
#[derive(Serialize)]
struct Frontmatter<'a> {
    id: &'a str,
    triggers: &'a [String],
    lessons: usize,
}

/// Render a playbook as a native agent-file (`---\n<yaml>\n---\n<body>`): YAML frontmatter
/// carrying the id, the trigger predicate, and the fold count, then the lesson body. The
/// output round-trips through [`crate::config::split_frontmatter`], the same parser the
/// `.rigger/agents/*.md` definitions use.
pub fn render(pb: &Playbook) -> String {
    let fm = Frontmatter {
        id: &pb.id,
        triggers: &pb.triggers,
        lessons: pb.lessons,
    };
    // serde_yaml emits the mapping with a trailing newline, so the closing delimiter and
    // body attach directly. An unserializable struct is impossible here (all fields are
    // owned primitives), so the fallback is inert.
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    format!("---\n{yaml}---\n{}\n", pb.summary)
}

/// Reconstruct the playbook pool at `dir` from the `LessonLearned` events: distill them,
/// CLEAR the rigger-managed pool files (every `*.md` under `dir`), then write one file per
/// playbook. Because the pool is a pure projection, clearing first is what lets a lesson that
/// no longer exists in the log drop out of the pool - the rebuild is the whole pool, not a
/// merge. Returns the distilled playbooks it wrote.
pub fn rebuild(events: &[Event], dir: &Path) -> io::Result<Vec<Playbook>> {
    let playbooks = distill(events);
    fs::create_dir_all(dir)?;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|x| x.to_str()) == Some("md") {
            fs::remove_file(&path)?;
        }
    }
    for pb in &playbooks {
        fs::write(dir.join(format!("{}.md", pb.id)), render(pb))?;
    }
    Ok(playbooks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lesson(summary: &str, about: &[&str]) -> Event {
        Event::new(
            TYPE_LESSON_LEARNED,
            serde_json::to_vec(&json!({
                "id": format!("lesson-{summary}"),
                "summary": summary,
                "about": about,
            }))
            .unwrap(),
        )
    }

    #[test]
    fn distill_dedups_by_summary_and_unions_trigger_scope() {
        // spec 13b unit 2 / d13b-u2i-playbooks-module: the distiller folds LessonLearned
        // into a DEDUPLICATED pool - two lessons with the same text collapse to one playbook
        // whose trigger scope is the UNION of their `about` files and whose count is 2. A
        // distinct lesson stays its own playbook; a non-lesson event and an empty-summary
        // lesson contribute nothing.
        let events = vec![
            lesson("guard the checked add", &["a.rs"]),
            lesson("guard the checked add", &["b.rs", "a.rs"]),
            lesson("close the scratch file", &["c.rs"]),
            lesson("", &["d.rs"]), // empty summary: skipped
            Event::new("SomethingElse", b"{}".to_vec()), // non-lesson: skipped
        ];
        let pool = distill(&events);
        assert_eq!(
            pool.len(),
            2,
            "two distinct summaries -> two playbooks; got {pool:?}"
        );

        let add = pool
            .iter()
            .find(|p| p.summary == "guard the checked add")
            .expect("the deduplicated add playbook");
        assert_eq!(
            add.lessons, 2,
            "both add lessons must fold into one playbook"
        );
        assert_eq!(
            add.triggers,
            vec!["a.rs".to_string(), "b.rs".to_string()],
            "the trigger scope is the sorted UNION of both lessons' about files"
        );

        let close = pool
            .iter()
            .find(|p| p.summary == "close the scratch file")
            .expect("the distinct scratch-file playbook");
        assert_eq!(close.lessons, 1);
        assert_eq!(close.triggers, vec!["c.rs".to_string()]);

        // The slug is a stable projection of the summary (same text -> same id).
        let redistilled = distill(&events);
        let add_again = redistilled
            .iter()
            .find(|p| p.summary == "guard the checked add")
            .unwrap();
        assert_eq!(
            add.id, add_again.id,
            "the same lesson text distills to the same slug"
        );
        assert!(add.id.starts_with("playbook-"));
    }

    #[test]
    fn render_is_a_native_agent_file_that_round_trips() {
        // The pool files are rigger's native agent-file shape, so they parse with the SAME
        // frontmatter splitter the .rigger/agents/*.md definitions use, and the frontmatter
        // carries the trigger predicate + fold count while the body is the lesson verbatim.
        let pb = Playbook {
            id: "playbook-000000000000002a".to_string(),
            summary: "never disable a feature to unblock".to_string(),
            triggers: vec!["conductor.rs".to_string(), "main.rs".to_string()],
            lessons: 3,
        };
        let rendered = render(&pb);
        let (front, body) =
            crate::config::split_frontmatter(&rendered).expect("renders as native frontmatter");

        #[derive(serde::Deserialize)]
        struct FrontOwned {
            id: String,
            triggers: Vec<String>,
            lessons: usize,
        }
        let parsed: FrontOwned = serde_yaml::from_str(front).expect("frontmatter is valid YAML");
        assert_eq!(parsed.id, pb.id);
        assert_eq!(
            parsed.triggers, pb.triggers,
            "trigger predicate rides the frontmatter"
        );
        assert_eq!(parsed.lessons, 3, "the fold count rides the frontmatter");
        assert_eq!(
            body.trim_end(),
            pb.summary,
            "the body is the lesson verbatim"
        );
    }

    #[test]
    fn rebuild_reconstructs_the_pool_from_the_log_and_clears_stale() {
        // The pool is a rebuildable PROJECTION, never hand-edited state: rebuild clears the
        // managed files and re-derives the whole pool, so a stale playbook whose lesson is
        // gone from the log drops out, and re-running over the SAME log is idempotent.
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join(POOL_SUBDIR);
        fs::create_dir_all(&pool_dir).unwrap();
        // A stale, hand-left playbook file that no current lesson justifies.
        fs::write(
            pool_dir.join("playbook-stale.md"),
            "---\nid: playbook-stale\n---\ngone\n",
        )
        .unwrap();

        let full = vec![
            lesson("first lesson", &["a.rs"]),
            lesson("second lesson", &["b.rs"]),
        ];
        let written = rebuild(&full, &pool_dir).unwrap();
        assert_eq!(written.len(), 2);
        assert!(
            !pool_dir.join("playbook-stale.md").exists(),
            "rebuild must clear a stale managed file the current log does not justify"
        );
        for pb in &written {
            let ondisk = fs::read_to_string(pool_dir.join(format!("{}.md", pb.id))).unwrap();
            assert_eq!(
                ondisk,
                render(pb),
                "the on-disk file is the rendered playbook"
            );
        }

        // Idempotent: rebuilding the same log yields the same file set.
        let again = rebuild(&full, &pool_dir).unwrap();
        assert_eq!(
            again, written,
            "rebuild over the same log reproduces the pool"
        );
        let md_count = fs::read_dir(&pool_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .and_then(|x| x.to_str())
                    == Some("md")
            })
            .count();
        assert_eq!(
            md_count, 2,
            "no duplicate or leftover files after a re-rebuild"
        );

        // A SHRUNK log drops the playbook it no longer justifies.
        let shrunk = vec![lesson("first lesson", &["a.rs"])];
        let after = rebuild(&shrunk, &pool_dir).unwrap();
        assert_eq!(
            after.len(),
            1,
            "the pool projects only the surviving lesson"
        );
        assert_eq!(after[0].summary, "first lesson");
        let md_count = fs::read_dir(&pool_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .and_then(|x| x.to_str())
                    == Some("md")
            })
            .count();
        assert_eq!(md_count, 1, "the dropped playbook's file is gone");
    }
}
