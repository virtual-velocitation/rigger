//! The code-side interfaces: a stamped view of the tree's facts ([`CodeView`]), the source the
//! tree is read from ([`TreeSource`]), the extraction and grouping passes a store is given
//! ([`Extract`], [`Group`]), the filter a reader applies ([`ViewFilter`]) and the in-memory view
//! ([`FactsView`]). A store holds no language knowledge: extraction and grouping are passed in.

use crate::code_facts::{FileFacts, Half, MentionKind, ModuleDecl, Scope, Stamp, Tip};
use crate::resolve::ProofCall;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// A file in a view, without its facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileRef<'a> {
    pub path: &'a str,
    pub lang: &'a str,
    pub half: Half,
    pub scope: Scope,
    /// Whether the file holds design or workflow facts: concepts, links, nodes or relations.
    pub intent: bool,
    /// Whether the file declares a module held in another file.
    pub modules: bool,
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

/// The hidden paths a tree's facts still cover: the workflow definition rigger reads.
pub const ALWAYS_IN_SCOPE: &[&str] = &[".rigger/workflow.yml"];

/// Whether the file at `path` (relative to the tree root, `/`-separated) is one a tree's facts
/// cover: no component of it is hidden (a dot-name), or it is one of [`ALWAYS_IN_SCOPE`]. Every
/// tree source's listing is read through this, so a repository and a plain directory holding the
/// same tree yield the same files.
pub fn in_scope(path: &str) -> bool {
    !path.split('/').any(|part| part.starts_with('.')) || ALWAYS_IN_SCOPE.contains(&path)
}

/// One tree entry: a file and its content id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub blob: String,
}

/// How a worktree differs from a tree, path by path as git's status names each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorktreeDiff {
    /// Paths the tree holds whose content the worktree changed.
    pub modified: Vec<String>,
    /// Paths the tree holds that the worktree deleted.
    pub deleted: Vec<String>,
    /// Paths the worktree holds that the tree does not.
    pub untracked: Vec<String>,
}

impl WorktreeDiff {
    /// The files that differ, less those outside the tree's scope ([`in_scope`]): rigger's own
    /// state holds no fact, so it never counts as a difference.
    pub fn files_in_scope(&self) -> BTreeSet<&str> {
        (self.masked()).chain(self.read_from_worktree()).collect()
    }

    /// The tree's files in scope whose facts no longer describe the worktree: the modified and
    /// the deleted.
    pub fn masked(&self) -> impl Iterator<Item = &str> {
        Self::scoped(self.modified.iter().chain(&self.deleted))
    }

    /// The files in scope whose facts are read from the worktree's bytes: the modified and the
    /// untracked.
    pub fn read_from_worktree(&self) -> impl Iterator<Item = &str> {
        Self::scoped(self.modified.iter().chain(&self.untracked))
    }

    fn scoped<'d>(paths: impl Iterator<Item = &'d String>) -> impl Iterator<Item = &'d str> {
        paths.map(String::as_str).filter(|f| in_scope(f))
    }
}

/// What the view a head serves describes of the reader's worktree (6.1, 13): the ONE withholding
/// rule every reader of a view applies (invariant 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compared {
    /// The view describes the worktree: the tree's facts with this many of the worktree's
    /// differing files ([`WorktreeDiff::files_in_scope`]) taken over them; none when the worktree
    /// matches the tree.
    Overlaid(usize),
    /// The worktree cannot be compared with the tree (its stamp commit is not in the worktree's
    /// object store, or its index or a differing file cannot be read), or no view was read: the
    /// answers describe the tree alone, so every code fact is withheld.
    Uncomparable,
}

impl Compared {
    /// What the view of a tree overlaid with a worktree's differing files describes, given how
    /// the worktree differs from the tree (`diff`), or why it cannot be compared.
    pub fn of(diff: Result<&WorktreeDiff, &str>) -> Compared {
        match diff {
            Ok(diff) => Compared::Overlaid(diff.files_in_scope().len()),
            Err(_) => Compared::Uncomparable,
        }
    }

    /// Whether a reader withholds every code fact of the view.
    pub fn withholds_code(self) -> bool {
        self == Compared::Uncomparable
    }
}

/// The code view of the head a reader in a worktree reads, and what it describes of that
/// worktree.
pub struct Head {
    /// The head's facts with the worktree's differing files overlaid ([`Compared::Overlaid`]),
    /// or the head's alone when the worktree cannot be compared.
    pub view: Box<dyn CodeView>,
    /// What the view describes of the worktree.
    pub compared: Compared,
    /// Why the view is behind the tree it was opened for, when it is.
    pub behind: Option<String>,
}

/// Opens the code view of the head for a reader (7.3). The composition root implements it. A
/// reader that lives across merges opens once per read, so each answer describes the store as it
/// stands then.
pub trait OpenCodeView: Send + Sync {
    /// The view of the head as it stands now, compared with `worktree`, or why it cannot be read.
    fn head(&self, worktree: &Path) -> Result<Head, String>;
}

/// One tree's facts, stamped with the tree they describe.
pub trait CodeView: Send + Sync {
    fn stamp(&self) -> &Stamp;
    /// Every file, in path order.
    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_>;
    /// The facts of the file at `path`; borrowed when the view holds them in memory, built when
    /// it reads them from bytes.
    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>>;
    /// The facts of the file at `path` as far as a read of its design and workflow facts needs:
    /// its path, half, scope, concepts, links, nodes and relations are the file's, and its
    /// definitions, mentions and modules may be left out. By default the whole facts; a view that
    /// reads facts from bytes decodes these alone.
    fn intent_facts(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.file(path)
    }
    /// The modules the file at `path` declares. By default read off the file's whole facts; a
    /// view that reads facts from bytes decodes these alone.
    fn modules(&self, path: &str) -> Option<Cow<'_, [ModuleDecl]>> {
        Some(match self.file(path)? {
            Cow::Borrowed(facts) => Cow::Borrowed(&facts.modules[..]),
            Cow::Owned(facts) => Cow::Owned(facts.modules),
        })
    }
    /// Every definition whose leaf is `leaf`, in path then source order.
    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_>;
    /// Every mention whose leaf is `leaf`, in path then source order.
    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_>;
    fn groupings(&self) -> &Groupings;
    /// Every distinct leaf a definition carries, in order: the names a reader resolves a term
    /// against before it reads the definitions of each through [`Self::defs_named`]. By default
    /// read off every file's facts; a view that indexes its definitions by leaf lists its index.
    fn def_leaves(&self) -> Vec<String> {
        let mut leaves = std::collections::BTreeSet::new();
        for f in self.files() {
            if let Some(facts) = self.file(f.path) {
                leaves.extend(facts.defs.iter().map(|d| d.leaf.clone()));
            }
        }
        leaves.into_iter().collect()
    }

    /// What the store settled for this view at advance, when the view holds it: a reader then
    /// reads its rows instead of computing them ([`crate::resolve::Resolver`] computes them for a
    /// view that does not).
    fn settled(&self) -> Option<&dyn Settled> {
        None
    }
}

/// The proof calls a [`Settled`] read selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proofs<'a> {
    /// Every proof call.
    All,
    /// The calls proving the definition with this id.
    Of(&'a str),
    /// The calls in the file at this path.
    In(&'a str),
}

/// What a store computes from the tree alone when it advances, so a read takes rows rather than
/// computing them: each one is what [`crate::resolve::Resolver`] computes over the same view.
pub trait Settled: Sync {
    /// Every test file, by its Test scope or the test-module closure, in path order.
    fn test_files(&self) -> Vec<&str>;
    /// The proof calls `of` selects, in proof order ([`crate::resolve::Resolver::proof_calls`]).
    fn proofs(&self, of: Proofs<'_>) -> Vec<ProofCall>;
}

/// Where a tree is read from: a git repository, or a directory with no git.
pub trait TreeSource {
    /// The checked-out commit and its tree.
    fn head(&self) -> Result<Tip, String>;
    /// Whether the checkout is still at `tip`'s branch and commit, when the source can tell
    /// without the work [`Self::head`] does; false when it cannot tell, which sends the caller to
    /// [`Self::head`].
    fn still_at(&self, _tip: &Tip) -> bool {
        false
    }
    /// Every file of `tree`.
    fn list(&self, tree: &str) -> Result<Vec<Entry>, String>;
    /// How `worktree` differs from `tree`.
    fn changed_against_worktree(&self, tree: &str, worktree: &Path)
        -> Result<WorktreeDiff, String>;
    /// Hands each of `paths` (as [`Self::changed_against_worktree`] names them) to `sink` as
    /// (path, bytes), read from `worktree` as this source reads a file into a tree: the bytes a
    /// commit of the worktree's content would hold there. A path it cannot read is an error.
    fn read_worktree(
        &self,
        worktree: &Path,
        paths: &[&str],
        sink: &mut dyn FnMut(&str, &[u8]),
    ) -> Result<(), String>;
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
    /// Changes whenever the groupings computed for a view can change; a snapshot grouped under
    /// another generation is not trusted.
    fn generation(&self) -> u64;
    fn group(&self, view: &dyn CodeView) -> Groupings;
}

/// Advances one project's code store to the head its operator checkout has checked out. The
/// conductor calls it right after it lands a merge, so the cost of describing the new tip falls on
/// the merge rather than on the next reader. The project is bound where the interface is built.
pub trait AdvanceCodeStore: Send + Sync {
    /// Advances the store to the head, waiting for a writer in flight; an error says why the
    /// store still describes an older tip.
    fn advance_to_head(&self) -> Result<(), String>;
}

/// Whether test code is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tests {
    #[default]
    Exclude,
    Include,
    Only,
}

impl Tests {
    /// Whether code that is test code (`test`) or product code is shown under this setting.
    pub fn admits(self, test: bool) -> bool {
        match self {
            Tests::Exclude => !test,
            Tests::Include => true,
            Tests::Only => test,
        }
    }
}

/// The one parser of the `tests` setting every reader takes (the dash's `tests=`, the MCP tools'
/// `tests`, the CLI's `--tests`): `exclude`, `include` or `only`.
impl std::str::FromStr for Tests {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "exclude" => Ok(Tests::Exclude),
            "include" => Ok(Tests::Include),
            "only" => Ok(Tests::Only),
            other => Err(format!(
                "`tests` is one of exclude, include or only, not {other:?}"
            )),
        }
    }
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

    /// This view with `groupings` as the groupings it holds.
    pub fn with_groupings(mut self, groupings: Groupings) -> Self {
        self.groupings = groupings;
        self
    }

    fn file_ref(file: &FileFacts) -> FileRef<'_> {
        FileRef {
            path: &file.path,
            lang: &file.lang,
            half: file.half,
            scope: file.scope,
            intent: file.holds_intent(),
            modules: !file.modules.is_empty(),
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

    fn def_leaves(&self) -> Vec<String> {
        let mut leaves: Vec<String> = self.defs.keys().cloned().collect();
        leaves.sort_unstable();
        leaves
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_facts::{Def, Mention};

    #[test]
    fn in_scope_skips_hidden_paths_but_the_workflow_definition() {
        let workflow = format!(
            "{}/{}",
            crate::config::RIGGER_DIR,
            crate::config::WORKFLOW_FILE
        );
        assert_eq!(ALWAYS_IN_SCOPE, [workflow.as_str()]);
        for (path, scoped) in [
            ("src/a.rs", true),
            ("README.md", true),
            (".gitignore", false),
            (".github/workflows/ci.yml", false),
            ("src/.cache/x.rs", false),
            (".rigger/workflow.yml", true),
            (".rigger/store/CURRENT", false),
            ("docs/.rigger/workflow.yml", false),
            ("a.b/c.rs", true),
        ] {
            assert_eq!(in_scope(path), scoped, "{path}");
        }
    }

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

    fn diff(modified: &[&str], deleted: &[&str], untracked: &[&str]) -> WorktreeDiff {
        let owned = |paths: &[&str]| paths.iter().map(|f| f.to_string()).collect();
        WorktreeDiff {
            modified: owned(modified),
            deleted: owned(deleted),
            untracked: owned(untracked),
        }
    }

    /// Each row: how a worktree differs, the files whose tree facts it masks and the files whose
    /// facts are read from the worktree.
    #[test]
    fn a_diff_masks_the_modified_and_deleted_and_reads_the_modified_and_untracked() {
        let cases = [
            ("agreeing", diff(&[], &[], &[]), vec![], vec![]),
            (
                "one of each",
                diff(&["src/m.rs"], &["src/d.rs"], &["src/u.rs"]),
                vec!["src/m.rs", "src/d.rs"],
                vec!["src/m.rs", "src/u.rs"],
            ),
            (
                "rigger's own state is out of scope",
                diff(&[".rigger/events.db"], &[".gitignore"], &[".rigger/x"]),
                vec![],
                vec![],
            ),
        ];
        for (name, diff, masked, read) in cases {
            assert_eq!(diff.masked().collect::<Vec<_>>(), masked, "{name}");
            assert_eq!(
                diff.read_from_worktree().collect::<Vec<_>>(),
                read,
                "{name}"
            );
        }
    }

    /// Each row: what a view describes of a worktree and whether a reader withholds its code.
    #[test]
    fn only_an_uncomparable_worktree_withholds_code() {
        for (compared, withholds) in [
            (Compared::Overlaid(0), false),
            (Compared::Overlaid(3), false),
            (Compared::Uncomparable, true),
        ] {
            assert_eq!(compared.withholds_code(), withholds, "{compared:?}");
        }
    }

    /// Each row: how a worktree differs and the files that count as differing.
    #[test]
    fn a_diff_counts_only_the_files_the_tree_covers() {
        let cases = [
            ("no difference", diff(&[], &[], &[]), vec![]),
            (
                "modified, deleted and untracked together",
                diff(&["src/b.rs"], &["src/c.rs"], &["src/a.rs"]),
                vec!["src/a.rs", "src/b.rs", "src/c.rs"],
            ),
            (
                "a file in two lists counts once",
                diff(&["src/a.rs"], &[], &["src/a.rs"]),
                vec!["src/a.rs"],
            ),
            (
                "rigger's own state is out of scope",
                diff(
                    &[".rigger/events.db"],
                    &[],
                    &[".rigger/store/code/CURRENT", "README.md"],
                ),
                vec!["README.md"],
            ),
            (
                "the workflow definition is in scope",
                diff(&[".rigger/workflow.yml"], &[], &[]),
                vec![".rigger/workflow.yml"],
            ),
        ];
        for (name, diff, expected) in cases {
            let files: Vec<&str> = diff.files_in_scope().into_iter().collect();
            assert_eq!(files, expected, "{name}");
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

    /// Each row: a `tests` setting as a reader spells it and what it parses to.
    #[test]
    fn tests_setting_parses_one_way_for_every_reader() {
        let cases = [
            ("exclude", Ok(Tests::Exclude)),
            ("include", Ok(Tests::Include)),
            ("only", Ok(Tests::Only)),
            (
                "Include",
                Err("`tests` is one of exclude, include or only, not \"Include\"".to_string()),
            ),
            (
                "",
                Err("`tests` is one of exclude, include or only, not \"\"".to_string()),
            ),
        ];
        for (spelled, expected) in cases {
            assert_eq!(spelled.parse::<Tests>(), expected, "{spelled:?}");
        }
    }
}
