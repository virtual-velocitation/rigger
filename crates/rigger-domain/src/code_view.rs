//! The code-side interfaces: a stamped view of the tree's facts ([`CodeView`]), the source the
//! tree is read from ([`TreeSource`]), the extraction and grouping passes a store is given
//! ([`Extract`], [`Group`]), the filter a reader applies ([`ViewFilter`]) and the in-memory view
//! ([`FactsView`]). A store holds no language knowledge: extraction and grouping are passed in.

use crate::code_facts::{FileFacts, Half, MentionKind, Scope, Stamp, Tip};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

/// A file in a view, without its facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileRef<'a> {
    pub path: &'a str,
    pub lang: &'a str,
    pub half: Half,
    pub scope: Scope,
}

/// A definition in a view, with the file it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefRef<'a> {
    pub file: FileRef<'a>,
    /// The definition's index in its file's `defs`.
    pub index: u32,
    pub local: &'a str,
    pub leaf: &'a str,
    pub kind: &'a str,
    pub line: u32,
    pub end_line: u32,
    pub parent: Option<u32>,
    pub test: bool,
}

/// A name-reference in a view, with the file it sits in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionRef<'a> {
    pub file: FileRef<'a>,
    pub enclosing: Option<u32>,
    pub name: &'a str,
    pub leaf: &'a str,
    pub kind: MentionKind,
    pub line: u32,
    pub test: bool,
}

/// One computed cluster: a community of code entities, or a concept realized by documents and
/// the code they govern.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    /// `community/<resolution>/<n>` or `concept/<resolution>/<n>`.
    pub id: String,
    pub label: String,
    /// The grain the pass ran at.
    pub resolution: f64,
    /// The pass's content hash.
    pub hash: String,
    /// The member node ids.
    pub members: Vec<String>,
}

/// The groupings of a view, computed over the production view at the default grain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Groupings {
    pub communities: Vec<Cluster>,
    pub concepts: Vec<Cluster>,
}

/// One tree entry: a file and its content id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub blob: String,
}

/// One path that differs between two trees; `old` is none for an added file and `new` is none
/// for a deleted one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// How a worktree differs from a tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorktreeDiff {
    /// Paths the tree holds that the worktree modified or deleted.
    pub changed: Vec<String>,
    /// Paths the worktree holds that the tree does not.
    pub untracked: Vec<String>,
}

/// One tree's facts, stamped with the tree they describe.
pub trait CodeView: Send + Sync {
    fn stamp(&self) -> &Stamp;
    /// Every file, in path order.
    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_>;
    /// The facts of the file at `path`; borrowed when the view holds them in memory, built when
    /// it reads them from bytes.
    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>>;
    /// Every definition whose leaf is `leaf`, in path then source order.
    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_>;
    /// Every mention whose leaf is `leaf`, in path then source order.
    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_>;
    fn groupings(&self) -> &Groupings;
}

/// Where a tree is read from: a git repository, or a directory with no git.
pub trait TreeSource {
    /// The checked-out commit and its tree.
    fn head(&self) -> Result<Tip, String>;
    /// Every file of `tree`.
    fn list(&self, tree: &str) -> Result<Vec<Entry>, String>;
    /// The paths that differ between two trees.
    fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>, String>;
    /// How `worktree` differs from `tree`.
    fn changed_against_worktree(&self, tree: &str, worktree: &Path)
        -> Result<WorktreeDiff, String>;
    /// Hands each of `blobs` to `sink` as (blob id, bytes).
    fn read_blobs(&self, blobs: &[String], sink: &mut dyn FnMut(&str, &[u8]))
        -> Result<(), String>;
}

/// Reads one file's facts from its path and bytes.
pub trait Extract: Send + Sync {
    /// Changes whenever any extraction output can change; a snapshot of another fingerprint is
    /// not trusted.
    fn fingerprint(&self) -> u64;
    /// The facts of the file `entry` names, read from `bytes`; `path` and `blob` come from `entry`.
    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts;
}

/// Computes a view's groupings.
pub trait Group {
    fn group(&self, view: &dyn CodeView) -> Groupings;
}

/// Whether test code is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tests {
    #[default]
    Exclude,
    Include,
    Only,
}

/// What a reader asks of a view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewFilter {
    pub tests: Tests,
}

/// A [`CodeView`] over facts held in memory.
pub struct FactsView {
    stamp: Stamp,
    files: Vec<FileFacts>,
    /// Leaf to (file index, def index), in path then source order.
    defs: HashMap<String, Vec<(u32, u32)>>,
    /// Leaf to (file index, mention index), in path then source order.
    mentions: HashMap<String, Vec<(u32, u32)>>,
    groupings: Groupings,
}

impl FactsView {
    /// A view of `files` (paths unique), with no groupings.
    pub fn new(stamp: Stamp, mut files: Vec<FileFacts>) -> Self {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut defs: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        let mut mentions: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        for (f, file) in files.iter().enumerate() {
            for (d, def) in file.defs.iter().enumerate() {
                defs.entry(def.leaf.clone())
                    .or_default()
                    .push((f as u32, d as u32));
            }
            for (m, mention) in file.mentions.iter().enumerate() {
                mentions
                    .entry(mention.leaf.clone())
                    .or_default()
                    .push((f as u32, m as u32));
            }
        }
        Self {
            stamp,
            files,
            defs,
            mentions,
            groupings: Groupings::default(),
        }
    }

    fn file_ref(file: &FileFacts) -> FileRef<'_> {
        FileRef {
            path: &file.path,
            lang: &file.lang,
            half: file.half,
            scope: file.scope,
        }
    }
}

impl CodeView for FactsView {
    fn stamp(&self) -> &Stamp {
        &self.stamp
    }

    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
        Box::new(self.files.iter().map(Self::file_ref))
    }

    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        let at = self
            .files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .ok()?;
        Some(Cow::Borrowed(&self.files[at]))
    }

    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
        let rows = self.defs.get(leaf).map_or(&[][..], Vec::as_slice);
        Box::new(rows.iter().map(|&(f, d)| {
            let file = &self.files[f as usize];
            let def = &file.defs[d as usize];
            DefRef {
                file: Self::file_ref(file),
                index: d,
                local: &def.local,
                leaf: &def.leaf,
                kind: &def.kind,
                line: def.line,
                end_line: def.end_line,
                parent: def.parent,
                test: def.test,
            }
        }))
    }

    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
        let rows = self.mentions.get(leaf).map_or(&[][..], Vec::as_slice);
        Box::new(rows.iter().map(|&(f, m)| {
            let file = &self.files[f as usize];
            let mention = &file.mentions[m as usize];
            MentionRef {
                file: Self::file_ref(file),
                enclosing: mention.enclosing,
                name: &mention.name,
                leaf: &mention.leaf,
                kind: mention.kind.clone(),
                line: mention.line,
                test: mention.test,
            }
        }))
    }

    fn groupings(&self) -> &Groupings {
        &self.groupings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_facts::{Def, Mention};

    fn def(local: &str, leaf: &str, test: bool) -> Def {
        Def {
            local: local.into(),
            leaf: leaf.into(),
            kind: "fn".into(),
            line: 1,
            end_line: 2,
            parent: None,
            test,
        }
    }

    fn mention(name: &str, leaf: &str, line: u32, test: bool) -> Mention {
        Mention {
            enclosing: None,
            name: name.into(),
            leaf: leaf.into(),
            kind: MentionKind::Call,
            line,
            test,
        }
    }

    fn file(path: &str, scope: Scope, defs: Vec<Def>, mentions: Vec<Mention>) -> FileFacts {
        FileFacts {
            path: path.into(),
            lang: "rust".into(),
            scope,
            defs,
            mentions,
            ..Default::default()
        }
    }

    /// Each row: the leaf asked for, the definitions it finds as `path::local`, and the mentions
    /// it finds as `path:line:name`, both in path then source order.
    #[test]
    fn facts_view_finds_defs_and_mentions_by_leaf() {
        // Fed out of path order, so the view's own ordering is under test.
        let stamp = Stamp {
            tip: Tip {
                head: "main".into(),
                commit: None,
                tree: "t".into(),
            },
            fingerprint: 0,
            seq: 1,
        };
        let view = FactsView::new(
            stamp,
            vec![
                file(
                    "tests/t.rs",
                    Scope::Test,
                    vec![def("check", "check", true)],
                    vec![mention("bar", "bar", 5, true)],
                ),
                file(
                    "src/b.rs",
                    Scope::Product,
                    vec![def("bar", "bar", false), def("Foo::bar", "bar", false)],
                    vec![mention("Foo::bar", "bar", 9, false)],
                ),
                file(
                    "src/a.rs",
                    Scope::Product,
                    vec![def("inner", "inner", false)],
                    vec![
                        mention("bar", "bar", 3, false),
                        mention("inner", "inner", 4, false),
                    ],
                ),
            ],
        );
        let cases: &[(&str, &[&str], &[&str])] = &[
            (
                "bar",
                &["src/b.rs::bar", "src/b.rs::Foo::bar"],
                &["src/a.rs:3:bar", "src/b.rs:9:Foo::bar", "tests/t.rs:5:bar"],
            ),
            ("inner", &["src/a.rs::inner"], &["src/a.rs:4:inner"]),
            ("check", &["tests/t.rs::check"], &[]),
            ("absent", &[], &[]),
        ];
        for (leaf, defs, mentions) in cases {
            let found_defs: Vec<String> = view
                .defs_named(leaf)
                .map(|d| format!("{}::{}", d.file.path, d.local))
                .collect();
            let found_mentions: Vec<String> = view
                .mentions_named(leaf)
                .map(|m| format!("{}:{}:{}", m.file.path, m.line, m.name))
                .collect();
            assert_eq!(&found_defs, defs, "defs named {leaf}");
            assert_eq!(&found_mentions, mentions, "mentions named {leaf}");
        }
    }

    /// Each row: a filter and the test setting it must carry.
    #[test]
    fn view_filter_defaults_to_excluding_tests() {
        let cases = [
            (
                "the filter's default",
                ViewFilter::default(),
                Tests::Exclude,
            ),
            (
                "the default test setting",
                ViewFilter {
                    tests: Tests::default(),
                },
                Tests::Exclude,
            ),
            (
                "an explicit include",
                ViewFilter {
                    tests: Tests::Include,
                },
                Tests::Include,
            ),
            (
                "an explicit only",
                ViewFilter { tests: Tests::Only },
                Tests::Only,
            ),
        ];
        for (name, filter, expected) in cases {
            assert_eq!(filter.tests, expected, "{name}");
        }
    }
}
