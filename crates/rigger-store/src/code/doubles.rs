//! Test doubles for the code store's interfaces: a toy extractor and a grouping pass that count
//! their calls, and a tree source held in memory. Compiled for this crate's tests and, through
//! the `doubles` feature, for the tests of crates that implement a tree source.

use crate::code::dir_source::{content_id, tree_id};
use rigger_domain::code_facts::{Def, FileFacts, Mention, MentionKind, Scope, Tip};
use rigger_domain::code_view::{
    Cluster, CodeView, Entry, Extract, Group, Groupings, TreeSource, WorktreeDiff,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A toy language, one fact a line: `def NAME` defines `NAME` inside the previous definition if
/// it is indented, `call NAME` calls it from the definition above, and `use KIND NAME` is a
/// reference tagged `KIND`. A file under `tests/` is test scope. It records every file it reads.
pub struct ToyExtract {
    fingerprint: u64,
    read: Mutex<Vec<(String, Vec<u8>)>>,
}

impl ToyExtract {
    pub fn new(fingerprint: u64) -> Self {
        Self {
            fingerprint,
            read: Mutex::new(Vec::new()),
        }
    }

    /// How many files it extracted.
    pub fn calls(&self) -> usize {
        self.read.lock().map_or(0, |r| r.len())
    }

    /// The paths it extracted, sorted.
    pub fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .read
            .lock()
            .map(|r| r.iter().map(|(p, _)| p.clone()).collect())
            .unwrap_or_default();
        paths.sort();
        paths
    }

    /// The bytes it was handed for `path`.
    pub fn bytes_of(&self, path: &str) -> Option<Vec<u8>> {
        let read = self.read.lock().ok()?;
        read.iter().find(|(p, _)| p == path).map(|(_, b)| b.clone())
    }
}

impl Extract for ToyExtract {
    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
        if let Ok(mut read) = self.read.lock() {
            read.push((entry.path.clone(), bytes.to_vec()));
        }
        let mut facts = FileFacts {
            path: entry.path.clone(),
            blob: entry.blob.clone(),
            lang: "toy".into(),
            scope: if entry.path.starts_with("tests/") {
                Scope::Test
            } else {
                Scope::Product
            },
            ..Default::default()
        };
        let mut top: Option<u32> = None;
        for (n, line) in String::from_utf8_lossy(bytes).lines().enumerate() {
            let line_no = n as u32 + 1;
            let nested = line.starts_with(' ');
            let words: Vec<&str> = line.split_whitespace().collect();
            let enclosing = facts.defs.len().checked_sub(1).map(|i| i as u32);
            match words.as_slice() {
                ["def", name] => {
                    let parent = if nested { top } else { None };
                    let local = match parent {
                        Some(p) => format!("{}::{name}", facts.defs[p as usize].local),
                        None => (*name).to_owned(),
                    };
                    if !nested {
                        top = Some(facts.defs.len() as u32);
                    }
                    facts.defs.push(Def {
                        local,
                        leaf: (*name).to_owned(),
                        kind: "fn".into(),
                        line: line_no,
                        end_line: line_no,
                        parent,
                        test: facts.scope == Scope::Test,
                    });
                }
                ["call", name] | ["use", _, name] => facts.mentions.push(Mention {
                    enclosing,
                    name: (*name).to_owned(),
                    leaf: name.rsplit("::").next().unwrap_or(name).to_owned(),
                    kind: match words.as_slice() {
                        ["use", kind, _] => MentionKind::Tagged((*kind).to_owned()),
                        _ => MentionKind::Call,
                    },
                    line: line_no,
                    test: facts.scope == Scope::Test,
                }),
                _ => {}
            }
        }
        facts
    }
}

/// A grouping pass that puts every definition in one community and counts its calls.
#[derive(Default)]
pub struct CountingGroup {
    calls: AtomicUsize,
}

impl CountingGroup {
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Group for CountingGroup {
    fn group(&self, view: &dyn CodeView) -> Groupings {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let members: Vec<String> = view
            .files()
            .filter_map(|f| view.file(f.path))
            .flat_map(|facts| {
                facts
                    .defs
                    .iter()
                    .map(|d| format!("{}::{}", facts.path, d.local))
                    .collect::<Vec<_>>()
            })
            .collect();
        Groupings {
            communities: vec![Cluster {
                id: "community/1/0".into(),
                label: "every definition".into(),
                resolution: 1.0,
                hash: content_id(members.join("\n").as_bytes()),
                members,
            }],
            concepts: Vec::new(),
        }
    }
}

/// A tree source held in memory: each commit names a tree of (path, text) files, and the heads
/// asked of it and the blobs it hands out are counted.
#[derive(Default)]
pub struct MemSource {
    head: Mutex<Option<Tip>>,
    trees: Mutex<HashMap<String, Vec<Entry>>>,
    blobs: Mutex<HashMap<String, Vec<u8>>>,
    blobs_read: AtomicUsize,
    heads_asked: AtomicUsize,
}

impl MemSource {
    /// Checks out a commit holding `files`, and returns its tip.
    pub fn commit(&self, commit: &str, files: &[(&str, &str)]) -> Tip {
        let mut entries: Vec<Entry> = files
            .iter()
            .map(|(path, text)| {
                let blob = content_id(text.as_bytes());
                if let Ok(mut blobs) = self.blobs.lock() {
                    blobs.insert(blob.clone(), text.as_bytes().to_vec());
                }
                Entry {
                    path: (*path).to_owned(),
                    blob,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let tip = Tip {
            head: "main".into(),
            commit: Some(commit.to_owned()),
            tree: tree_id(&entries),
        };
        if let Ok(mut trees) = self.trees.lock() {
            trees.insert(tip.tree.clone(), entries);
        }
        if let Ok(mut head) = self.head.lock() {
            *head = Some(tip.clone());
        }
        tip
    }

    /// How many blobs it handed out.
    pub fn blobs_read(&self) -> usize {
        self.blobs_read.load(Ordering::SeqCst)
    }

    /// How many times its head was asked for.
    pub fn heads_asked(&self) -> usize {
        self.heads_asked.load(Ordering::SeqCst)
    }
}

impl TreeSource for MemSource {
    fn head(&self) -> Result<Tip, String> {
        self.heads_asked.fetch_add(1, Ordering::SeqCst);
        self.head
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| "nothing committed".to_string())
    }

    fn still_at(&self, tip: &Tip) -> bool {
        self.head
            .lock()
            .is_ok_and(|head| head.as_ref() == Some(tip))
    }

    fn list(&self, tree: &str) -> Result<Vec<Entry>, String> {
        self.trees
            .lock()
            .map_err(|e| e.to_string())?
            .get(tree)
            .cloned()
            .ok_or_else(|| format!("no tree {tree}"))
    }

    fn changed_against_worktree(&self, _: &str, _: &Path) -> Result<WorktreeDiff, String> {
        Err("a memory source has no worktree".into())
    }

    fn read_blobs(
        &self,
        blobs: &[String],
        sink: &mut dyn FnMut(&str, &[u8]),
    ) -> Result<(), String> {
        let held = self.blobs.lock().map_err(|e| e.to_string())?;
        for blob in blobs {
            let bytes = held.get(blob).ok_or_else(|| format!("no blob {blob}"))?;
            self.blobs_read.fetch_add(1, Ordering::SeqCst);
            sink(blob, bytes);
        }
        Ok(())
    }
}
