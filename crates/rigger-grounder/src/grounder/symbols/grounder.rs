//! The `symbols` grounder (spec 15, unit 4): the `Grounder` port over the code store's view of
//! the head, serving the PRECISE grounding contract (architecture 5.5.6). It ranks a DEFINITION
//! whose name matches the query above a mere REFERENCE, above an incidental prose mention
//! (which the view does not hold as a symbol at all, so it never appears). Selection is wired in
//! `grounder_for` / `main::select_grounder`.
//!
//! Every read opens the view of the head as it stands then ([`OpenCodeView`]), so a grounder that
//! lives across merges follows the store as merges advance it, and serves the code files the
//! caller's worktree agrees with the view on: a file that differs is withheld (the code store's
//! honesty rule, invariant 2), and every file is when the worktree cannot be compared.

use crate::facts::DirectoryHead;
use crate::grounder::symbols::model::{percentile_cutoff, GRAMMAR_TAGS_VERSION};
use crate::grounder::{BlastRadius, Grep, Grounded, Grounder, RankedRef, Ref, StampedRadius};
use rigger_domain::code_facts::{Half, MentionKind, Scope, Stamp, Tip};
use rigger_domain::code_view::{
    CodeView, DefRef, FactsView, FileRef, MentionRef, OpenCodeView, Tests, Withheld,
};
use rigger_domain::composed::Label;
use rigger_domain::contextgraph::query::path_leaf;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

/// The percentile of the tree-wide name-ambiguity distribution above which a name counts as
/// AMBIGUOUS for the ranked-by-intent page (spec 92 criterion 3): drawn from the repo's OWN
/// distribution, not an absolute constant a monorepo would blow past, so only the top decile of
/// most-defined names is down-weighted.
const AMBIGUITY_PERCENTILE: f64 = 0.90;

/// The `symbols` grounder over the code store's view of the head. `ground` ranks name matches by
/// the precise contract; every method reads the view afresh, so there is nothing to freshen after a
/// unit integrates: the conductor advances the store after each landed merge.
pub struct Symbols {
    /// The caller's worktree: the tree the text search and the path checks read, and the one the
    /// head's view is compared with.
    root: String,
    heads: Box<dyn OpenCodeView>,
}

impl Symbols {
    /// The grounder for the worktree at `root`, reading the view `heads` opens.
    pub fn over(root: &str, heads: Box<dyn OpenCodeView>) -> Symbols {
        Symbols {
            root: root.to_string(),
            heads,
        }
    }

    /// The grounder of a project with no git, read from the directory at `root`.
    pub fn in_directory(root: &str) -> Symbols {
        Symbols::over(root, Box::new(DirectoryHead::new(Path::new(root))))
    }

    /// `read` over the head's view as it stands now, less the files `root` puts in doubt, with
    /// the label of that same head: ONE open of the store gives both, so the label always
    /// describes the tree the answer came from. A head that cannot be read is a worktree that
    /// cannot be compared and more: every file is withheld, over a view that holds none, so the
    /// radius falls back to the text search, and the label names the failure.
    fn read_labelled<T>(&self, read: impl FnOnce(&Sight<'_>) -> T) -> (T, Label) {
        let (answer, label, _tree) = self.read_opened(read);
        (answer, label)
    }

    /// [`Symbols::read_labelled`] with the id of the tree that same open read ([`UNREAD_TREE`]
    /// when the head cannot be read): the one open every read makes.
    fn read_opened<T>(&self, read: impl FnOnce(&Sight<'_>) -> T) -> (T, Label, String) {
        let worktree = self.root.clone();
        match self.heads.head(Path::new(&self.root)) {
            Ok(head) => {
                let withheld = Withheld::of(head.diff);
                let tip = &head.view.stamp().tip;
                let label = Label::describing(tip, worktree, &withheld, head.behind);
                let tree = tip.tree.clone();
                (read(&Sight::new(&*head.view, withheld)), label, tree)
            }
            Err(reason) => (
                read(&Sight::new(&unread_view(), Withheld::All)),
                Label::unopened(worktree, &reason),
                UNREAD_TREE.to_string(),
            ),
        }
    }

    /// [`Symbols::read_labelled`] without the label.
    fn read<T>(&self, read: impl FnOnce(&Sight<'_>) -> T) -> T {
        self.read_labelled(read).0
    }

    /// [`Grounder::seed`] over one read's `sight`.
    fn seed_in(&self, sight: &Sight<'_>, query: &str, k: usize) -> Vec<String> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        let terms = code_terms(query);
        let mut seed: Vec<String> = Vec::new();
        let mut add = |file: &str| {
            if !seed.iter().any(|s| s == file) {
                seed.push(file.to_string());
            }
        };
        for t in &terms.paths {
            let served: Vec<&str> = sight
                .files()
                .map(|f| f.path)
                .filter(|p| names_path(p, t))
                .collect();
            if served.is_empty() && held_in_tree(&self.root, t) {
                add(t);
            }
            served.into_iter().for_each(&mut add);
        }
        let mut named: Vec<Vec<&str>> = terms
            .names
            .iter()
            .map(|n| definers(sight, &n.name))
            .collect();
        named.sort_by_key(Vec::len);
        named.into_iter().flatten().for_each(&mut add);
        scored_hits(sight, &query_terms(query))
            .into_iter()
            .filter(|h| !h.test)
            .take(k)
            .for_each(|h| add(&h.file));
        seed.truncate(k);
        seed
    }
}

/// The tree id of the view a head that cannot be read stands in for.
const UNREAD_TREE: &str = "unread";

/// The view a head that cannot be read stands in for: it holds no file.
fn unread_view() -> FactsView {
    let tip = Tip {
        head: String::new(),
        commit: None,
        tree: UNREAD_TREE.to_string(),
    };
    FactsView::new(
        Stamp {
            tip,
            fingerprint: 0,
            seq: 0,
        },
        Vec::new(),
    )
}

/// What one read sees: the head's view, less the files its worktree puts in doubt. Only code-half
/// files are served: the design and workflow halves hold no symbols.
struct Sight<'v> {
    view: &'v dyn CodeView,
    withheld: Withheld,
    /// Whether this sight serves the withheld code files instead: what a read counts as withheld.
    inverted: bool,
}

impl<'v> Sight<'v> {
    fn new(view: &'v dyn CodeView, withheld: Withheld) -> Self {
        Sight {
            view,
            withheld,
            inverted: false,
        }
    }

    /// The code files this sight withholds, served as a sight of their own, so a read counts the
    /// hits it withheld by the same pass that finds the hits it serves.
    fn withheld_part(&self) -> Sight<'v> {
        Sight {
            view: self.view,
            withheld: self.withheld.clone(),
            inverted: true,
        }
    }

    /// Whether the file at `path` is withheld from this read.
    fn withholds(&self, path: &str) -> bool {
        self.withheld.withholds_file(path)
    }

    fn serves(&self, file: &FileRef<'_>) -> bool {
        file.half == Half::Code && self.withholds(file.path) == self.inverted
    }

    /// Whether the radius must text-search the names the view resolves: some file is withheld, so
    /// a reference in it may be one the view does not hold.
    fn has_withheld_files(&self) -> bool {
        !self.withheld.withholds_nothing()
    }

    /// The served files, in path order.
    fn files(&self) -> impl Iterator<Item = FileRef<'v>> + '_ {
        let view = self.view;
        view.files().filter(move |f| self.serves(f))
    }

    /// The served definitions named `leaf`.
    fn defs_named(&self, leaf: &str) -> impl Iterator<Item = DefRef<'v>> + '_ {
        let view = self.view;
        view.defs_named(leaf).filter(move |d| self.serves(&d.file))
    }

    /// The served call references to `leaf`: no tagged reference (spec 117) counts, ranks or
    /// places a file.
    fn calls_named(&self, leaf: &str) -> impl Iterator<Item = MentionRef<'v>> + '_ {
        let view = self.view;
        view.mentions_named(leaf)
            .filter(move |m| m.kind == MentionKind::Call && self.serves(&m.file))
    }

    /// How many definitions named `name` the served files of `lang` hold, and with `count_refs`
    /// the call references to it too.
    fn occurrences(&self, name: &str, lang: &str, count_refs: bool) -> usize {
        let defs = self
            .defs_named(name)
            .filter(|d| d.file.lang == lang)
            .count();
        if !count_refs {
            return defs;
        }
        defs + self
            .calls_named(name)
            .filter(|m| m.file.lang == lang)
            .count()
    }

    /// How many call references name `name` WITHIN `lang` - the per-language fan-out degree an
    /// entity row reports. SCOPED to `lang`: the cross-reference graph is per-language (5.5.2), so
    /// the degree over it is too. A name that over-links in another language never inflates this
    /// count (a Python `parse` leaves the Rust `parse` degree untouched).
    fn reference_degree(&self, name: &str, lang: &str) -> usize {
        self.calls_named(name)
            .filter(|m| m.file.lang == lang)
            .count()
    }
}

/// The query's symbol-candidate terms for [`Symbols::ground`] (the prompt seed): the
/// alphanumeric/underscore runs of at least TWO Unicode characters, so `apply_damage` stays ONE
/// term and single-character noise is dropped. The filter counts CHARACTERS (`chars().count()`),
/// not bytes, so a single multibyte alphanumeric character (an accented letter, a CJK ideograph)
/// is dropped exactly like an ASCII single char rather than surviving on its 2-3 byte length. The
/// prompt seed reads the WHOLE criterion, prose included; the blast radius never does - it takes
/// its terms from [`code_terms`], the code a criterion names.
fn query_terms(query: &str) -> Vec<&str> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        // Count CHARACTERS, not bytes: `t.len()` is the byte length, so a single MULTIBYTE
        // alphanumeric char (an accented letter, a CJK ideograph, the micro sign) is 2-3 bytes
        // and a `len() >= 2` filter would keep it as a term - letting a degenerate single-char
        // query slip past the empty-terms fail-safe and fall through to an uncapped whole-repo
        // grep. `chars().count() >= 2` drops EVERY one-character token uniformly, ASCII or not.
        .filter(|t| t.chars().count() >= 2)
        .collect()
}

/// What a criterion NAMES AS CODE, which is all its blast radius grounds on (never its prose):
/// the symbols it `names` and the file `paths` it names. A criterion that names no code has
/// neither, and a span that is not a name of code (a multi-word phrase such as a command line, a
/// directory fragment) contributes nothing.
#[derive(Debug, Default, PartialEq)]
struct CodeTerms {
    names: Vec<Named>,
    paths: Vec<String>,
}

/// A symbol a criterion names: the `text` as written (`a::b`, `store.open`) and the `name` the
/// index matches (its last `::` or `.` segment). A name the index defines is grounded
/// structurally; one it does not (a string-literal event name, a constant prefix, a name nothing
/// defines yet) falls back to a whole-identifier text search for `text`.
#[derive(Debug, PartialEq)]
struct Named {
    text: String,
    name: String,
}

/// Files the text search never matches: spec files no unit edits, so a mention there is never a
/// conflict between two units.
const TEXT_SEARCH_SKIPS: &[&str] = &["specs/"];

/// Language keywords a code span may carry (`pub fn`, `&mut self`) that name no code of the
/// project, so they are never terms.
const KEYWORDS: &[&str] = &["fn", "pub", "mut", "self", "let", "use", "mod", "impl"];

/// The file extensions that make a dotted term a PATH (`ingest.rs`) rather than a member access
/// (`store.open`).
const FILE_EXTENSIONS: &[&str] = &[
    "rs", "md", "toml", "yml", "yaml", "json", "sh", "py", "ts", "js", "txt", "lock", "html",
    "css", "db",
];

/// The code a criterion names ([`CodeTerms`]). Each code span (backticked text) and each bare
/// token that is identifier-shaped ([`identifier_shaped`]) or path-shaped ([`is_path`])
/// contributes; a plain prose word never does. Generic and call arguments (`<T>`, `(x)`), surrounding punctuation and trailing `:`, `!`
/// or `?` are stripped, and keywords dropped. A span left with ONE piece is a path when it looks
/// like one (contains `/`, starts with `.`, or ends in a file extension), else a symbol when it is
/// an identifier or a `::`/`.` path of identifiers; anything else names no code. A span left
/// with SEVERAL pieces (`rigger validate`, a command line) is prose about the code, not a name of
/// it, so it contributes nothing.
fn code_terms(query: &str) -> CodeTerms {
    let mut terms = CodeTerms::default();
    for (i, part) in query.split('`').enumerate() {
        if i % 2 == 1 {
            terms.add_span(part);
            continue;
        }
        for word in part.split_whitespace() {
            let stripped = strip_groups(word);
            let piece = clean_piece(&stripped);
            if identifier_shaped(piece) || is_path(piece) {
                terms.add_piece(piece);
            }
        }
    }
    terms
}

impl CodeTerms {
    fn is_empty(&self) -> bool {
        self.names.is_empty() && self.paths.is_empty()
    }

    fn add_span(&mut self, span: &str) {
        let stripped = strip_groups(span);
        let pieces: Vec<&str> = stripped
            .split_whitespace()
            .map(clean_piece)
            .filter(|p| meaningful(p) && !KEYWORDS.contains(p))
            .collect();
        if let [one] = pieces.as_slice() {
            self.add_piece(one);
        }
    }

    fn add_piece(&mut self, piece: &str) {
        if !meaningful(piece) || KEYWORDS.contains(&piece) {
            return;
        }
        if is_path(piece) {
            push_new(&mut self.paths, piece);
        } else if piece
            .split("::")
            .flat_map(|s| s.split('.'))
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_'))
            && !self.names.iter().any(|n| n.text == piece)
        {
            self.names.push(Named {
                text: piece.to_string(),
                name: path_leaf(piece).to_string(),
            });
        }
    }
}

fn push_new(terms: &mut Vec<String>, term: &str) {
    if !terms.iter().any(|t| t == term) {
        terms.push(term.to_string());
    }
}

/// Two characters, one of them alphanumeric.
fn meaningful(piece: &str) -> bool {
    piece.chars().count() >= 2 && piece.chars().any(char::is_alphanumeric)
}

/// A term reads as a file path when it has a `/`, starts with `.` (`.gitignore`) or ends in a
/// known file extension. It grounds only when the tree holds that file.
fn is_path(term: &str) -> bool {
    term.contains('/')
        || term.starts_with('.')
        || term
            .rsplit_once('.')
            .is_some_and(|(stem, ext)| !stem.is_empty() && FILE_EXTENSIONS.contains(&ext))
}

/// `text` with every bracketed `<...>` and `(...)` group removed (nesting included), so
/// `a::b<T>` reads `a::b` and `foo(x)` reads `foo`. An unmatched closer is kept.
fn strip_groups(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// A piece without its surrounding punctuation or a trailing `:`, `!`, `?` or `.`; a leading `.`
/// stays, so `.rigger/workflow.yml` keeps its name.
fn clean_piece(piece: &str) -> &str {
    piece
        .trim_matches(|c: char| {
            matches!(
                c,
                ',' | ';' | ':' | '!' | '?' | '&' | '*' | '"' | '\'' | '(' | ')' | '[' | ']'
            )
        })
        .trim_end_matches('.')
}

/// Whether the code file at `path` is the one a criterion's path `term` names: the same path,
/// or one ending in `/term` (a criterion may spell a path from any directory down).
fn names_path(path: &str, term: &str) -> bool {
    path == term || path.ends_with(&format!("/{term}"))
}

/// Whether a path `term` the index does not hold facts for (`.gitignore`) is a file the tree holds. An
/// absolute or `..` path, which would escape the tree, is never read.
fn held_in_tree(root: &str, term: &str) -> bool {
    !Path::new(term).is_absolute()
        && !term.split('/').any(|c| c == "..")
        && Path::new(root).join(term).is_file()
}

/// The files that define `name`, product code before test code, each group in path order.
fn definers<'v>(sight: &Sight<'v>, name: &str) -> Vec<&'v str> {
    let mut files: BTreeMap<&'v str, bool> = BTreeMap::new();
    for d in sight.defs_named(name) {
        let test = files
            .entry(d.file.path)
            .or_insert(d.file.scope == Scope::Test);
        *test |= d.test;
    }
    let mut files: Vec<(&'v str, bool)> = files.into_iter().collect();
    files.sort_by_key(|(_, test)| *test);
    files.into_iter().map(|(path, _)| path).collect()
}

/// Whether a bare prose token reads as code: it contains `_` or `::`, or mixes case with an
/// uppercase letter past its first character (`camelCase`, `CamelCase`) - never an ALL-CAPS or
/// Capitalized prose word.
fn identifier_shaped(token: &str) -> bool {
    token.contains('_')
        || token.contains("::")
        || (token.chars().any(char::is_lowercase) && token.chars().skip(1).any(char::is_uppercase))
}

/// Every distinct `(name, language)` pair's DISTINCT-DEFINITION count over the served files (spec
/// 92 criterion 3 remediation, adj-u92c3-verdict-reject): the genuine tree-wide AMBIGUITY signal -
/// "how many different things could this name mean, WITHIN this language" - independent of how
/// often any ONE of those definitions is called. A name defined exactly once (within its own
/// language) is entirely unambiguous no matter how many places reference it (`criterion_stable_id`:
/// 1 definition, 37 references in this very repo, must read as a confident match); a name defined
/// many times over in unrelated places (`run`, `new`, `parse`) is genuinely ambiguous regardless
/// of reference volume. This is the ONE authority [`Symbols::has_strong_match`] gates its cutoff
/// on - never the raw definition-plus-call-reference occurrence volume
/// ([`Sight::occurrences`]), which conflates one entity's own popularity with tree-wide name
/// ambiguity. Its cutoff is [`crate::grounder::symbols::model::percentile_cutoff`]. Keyed by
/// `(name, language)`, never a bare name (spec 92 criterion 3 remediation round 5,
/// adj-u92c3-r4-verdict-reject): a bare-name key sums an entity's ambiguity across every language
/// sharing that name.
fn ambiguity_map(sight: &Sight<'_>) -> BTreeMap<(String, String), usize> {
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for file in sight.files() {
        let Some(facts) = sight.view.file(file.path) else {
            continue;
        };
        for d in &facts.defs {
            *counts
                .entry((d.leaf.clone(), file.lang.to_string()))
                .or_insert(0) += 1;
        }
    }
    counts
}

/// Whether a location's match is a DEFINITION or a REFERENCE of the query - the existing
/// lexical tier ([`ScoredHit::lexical`]: 3 for a definition, 2 for a reference) is derived from
/// this, and [`Symbols::ground_ranked`] uses it to decide how a hit's entity should be keyed
/// (a definition is its own entity by construction; a reference is absorbed into its sole
/// definer, when unambiguous).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HitKind {
    Def,
    Ref,
}

/// One (file, line) location scored against the query's terms (spec 92 criterion 3, the
/// scorer): `tier` - EXACT (2, a term equals the entity's name) beats CONTAINS (1, a term
/// merely occurs within a longer name), the Design decision "a query token that equals an
/// entity's name beats a token that merely occurs in it"; `commonness` - the MATCHED ENTITY's
/// OWN tree-wide occurrence count ([`Sight::occurrences`]), looked up by the entity's OWN
/// resolved name rather than whichever query term matched it (ascending: rarer ranks first, the
/// inverse-document-frequency ordering, without needing floating point - spec 92 criterion 3
/// remediation round 2, adv-u92c3r2-contains-tier-commonness-collapses-to-spurious-zero: a
/// CONTAINS-tier term is by definition a substring, so keying this off the term itself missed
/// every CONTAINS hit and silently handed it the artificial rarest score); `lexical` - the
/// EXISTING definition-over-reference tier, the Design's final tiebreaker ("then by the existing
/// lexical score").
///
/// Gap 104 puts three keys ahead of that order, counting DISTINCT query terms case-insensitively.
/// `name_coverage` - the terms the entity's name holds - ranks first, so a query that names an
/// entity by its shape (`impl EventStore for` -> the impl blocks named `impl EventStore for
/// Store`) finds it above a name that holds one incidental word (`for`). `test` - the location is
/// test code (a test-marked definition or reference, or a file whose scope is Test) - ranks
/// next, so a test double never outranks the product code it stands in for. `coverage` - the
/// terms the name OR the file path holds - ranks third, so the words that name the module narrow
/// the match (`sqlite store open` -> `open_connection` in `store/sqlite.rs` above `reopen_window`
/// elsewhere).
struct ScoredHit {
    name_coverage: usize,
    test: bool,
    coverage: usize,
    tier: u8,
    commonness: usize,
    lexical: u8,
    file: String,
    line: u32,
    name: String,
    lang: String,
    kind: HitKind,
}

impl ScoredHit {
    /// The `(name, language)` key that identifies this hit's matched entity - the ONE pairing every
    /// cross-language scoping lookup in this module uses, so a same-named entity in an unrelated
    /// language can never be looked up, absorbed into, or pooled with this one (spec 92 criterion
    /// 3 remediation round 5, adj-u92c3-r4-verdict-reject).
    fn entity(&self) -> (&str, &str) {
        (&self.name, &self.lang)
    }

    /// The ONE ranking order, best first: name coverage, then product before test code, then
    /// name-and-path coverage, then tier, then rarest commonness, then definition over reference,
    /// then file and line (a total, deterministic order). Both the per-location collapse and the
    /// final sort in [`scored_hits`] compare by it, so the two can never disagree.
    fn rank(&self) -> impl Ord + '_ {
        (
            std::cmp::Reverse(self.name_coverage),
            self.test,
            std::cmp::Reverse(self.coverage),
            std::cmp::Reverse(self.tier),
            self.commonness,
            std::cmp::Reverse(self.lexical),
            self.file.as_str(),
            self.line,
        )
    }
}

/// Score every (file, line) definition/call-reference location of the served files against
/// `terms` (a tagged reference, spec 117, never a location), ranked by [`ScoredHit::rank`] - the
/// ONE scored, sorted pass both [`Symbols::ground`] and [`Symbols::ground_ranked`] consume, so the
/// two views can never disagree on ranking. A location that matches more than one term (or both an
/// EXACT and a CONTAINS candidate) keeps its BEST tier - commonness is a property of the matched
/// entity's own name, so it never varies by which term matched - mirroring the
/// def-and-ref-at-one-line collapse the prior single-pass scorer already made.
///
/// The one pass that reads every served file's facts: a CONTAINS-tier term is a substring of a
/// name, so no lookup by name finds it.
fn scored_hits(sight: &Sight<'_>, terms: &[&str]) -> Vec<ScoredHit> {
    let lower_terms: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    let mut memo: HashMap<(String, String), usize> = HashMap::new();
    let mut best: BTreeMap<(String, u32), ScoredHit> = BTreeMap::new();
    for file in sight.files() {
        let Some(facts) = sight.view.file(file.path) else {
            continue;
        };
        let lower_path = file.path.to_lowercase();
        let test_file = file.scope == Scope::Test;
        let mut score_one = |name: &str, line: u32, kind: HitKind, lexical: u8, test: bool| {
            // The best TIER any query term gives this name: an EXACT match (some term equals the
            // name) always wins over a CONTAINS match (some term merely occurs within it).
            // `commonness` is the MATCHED ENTITY's own tree-wide occurrence count - `(name,
            // language)`, never the raw query term - so it is the same value regardless of which
            // term matched (rounds 2 and 4, adv-u92c3r2/r4: keying on the term, or on the name
            // alone, mis-scored CONTAINS hits and pooled languages).
            let mut hit_tier = 0u8;
            for t in terms {
                let tier = if name == *t {
                    2u8
                } else if name.contains(t) {
                    1u8
                } else {
                    0u8
                };
                hit_tier = hit_tier.max(tier);
            }
            if hit_tier == 0 {
                return;
            }
            let lower_name = name.to_lowercase();
            let in_name = |t: &&String| lower_name.contains(t.as_str());
            let commonness = *memo
                .entry((name.to_string(), file.lang.to_string()))
                .or_insert_with(|| sight.occurrences(name, file.lang, true));
            let hit = ScoredHit {
                name_coverage: lower_terms.iter().filter(in_name).count(),
                test: test || test_file,
                coverage: lower_terms
                    .iter()
                    .filter(|t| in_name(t) || lower_path.contains(t.as_str()))
                    .count(),
                tier: hit_tier,
                commonness,
                lexical,
                file: file.path.to_string(),
                line,
                name: name.to_string(),
                lang: file.lang.to_string(),
                kind,
            };
            // One row per (file, line): the better-ranked candidate keeps the slot - an impl
            // block's definition, matching every word of an impl-shaped query, over the trait
            // reference on its own header line.
            match best.entry((file.path.to_string(), line)) {
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if hit.rank() < slot.get().rank() {
                        slot.insert(hit);
                    }
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(hit);
                }
            }
        };
        for d in &facts.defs {
            score_one(&d.leaf, d.line, HitKind::Def, 3, d.test);
        }
        for m in facts
            .mentions
            .iter()
            .filter(|m| m.kind == MentionKind::Call)
        {
            score_one(&m.leaf, m.line, HitKind::Ref, 2, m.test);
        }
    }
    let mut hits: Vec<ScoredHit> = best.into_values().collect();
    hits.sort_by(|a, b| a.rank().cmp(&b.rank()));
    hits
}

impl Grounder for Symbols {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        self.ground_labelled(query, k, Tests::Include).rows
    }

    fn ground_labelled(&self, query: &str, k: usize, tests: Tests) -> Grounded<Vec<Ref>> {
        let none = Grounded {
            rows: Vec::new(),
            label: None,
            tests_hidden: 0,
        };
        if query.is_empty() || k == 0 {
            return none;
        }
        // The query's alphanumeric/underscore terms are the symbol candidates (so `apply_damage`
        // stays one term). Single-character terms are dropped as noise. This is the prompt seed's
        // whole-text match; `blast_radius` grounds on `code_terms` instead.
        let terms = query_terms(query);
        if terms.is_empty() {
            return none;
        }
        // Ranked by intent (spec 92 criterion 3): exact-name match first, then the rarest
        // matching token, then definition-over-reference - NOT deduplicated by entity (that is
        // `ground_ranked`'s page-shape concern), so this stays the SAME row-per-location
        // contract `grounded_seed` (conductor.rs) relies on for prompt-seed file diversity.
        let ((rows, tests_hidden, withheld), mut label) = self.read_labelled(|sight| {
            let hits = scored_hits(sight, &terms);
            let hidden = hidden_test_hits(&hits, tests);
            let rows = hits
                .into_iter()
                .filter(|h| tests.admits(h.test))
                .take(k)
                .map(|h| Ref {
                    file: h.file,
                    line: h.line,
                    text: h.name,
                })
                .collect();
            (rows, hidden, withheld_entities(sight, &terms, tests))
        });
        label.withheld = withheld;
        Grounded {
            rows,
            label: Some(label),
            tests_hidden,
        }
    }

    /// The prompt seed: the code `query` names ([`code_terms`]) resolved to the files that hold
    /// it, then the whole-text match. The files it spells come first (a served file whose path
    /// ends in the term, else the file itself when the tree holds it), then the files that define
    /// each identifier it names - the identifier fewer files define first, so a widely defined
    /// name never crowds out a specific one. The fill is [`Symbols::ground`]'s own top-`k` hit
    /// lines with test code left out: a prose word never matches a test function's sentence name,
    /// while a test file or test function the query names explicitly still seeds. At most `k`
    /// distinct files.
    fn seed(&self, query: &str, k: usize) -> Vec<String> {
        self.read(|sight| self.seed_in(sight, query, k))
    }

    /// The two-view blast radius over the cross-reference graph (architecture 5.5.1, spec 16 unit
    /// 1) - the `symbols` override of the grep-only trait default.
    ///
    /// The query is a criterion, and its terms are the CODE it names ([`code_terms`]: the symbols
    /// and the files of its code spans and identifier-shaped tokens), never its prose words - a
    /// prose word that happens to be a symbol name would pull that symbol's whole neighborhood
    /// in - and never a multi-word span, which is prose about the code rather than a name of it:
    ///
    /// - `precise` (the grounding contract) is the STRUCTURAL view - the files that DEFINE the
    ///   queried symbols (and any file the tree holds that a path term names) ranked ABOVE the
    ///   files whose call references name them (spec 117: a tagged reference places no file) -
    ///   capped at `k`. It is what seeds an agent's prompt, so it favors precision.
    /// - `safe` (the safety contract) is the structural view UNIONed, UNCAPPED, with a
    ///   whole-identifier text search (the EXISTING [`Grep`] walk) for each named symbol the view
    ///   does NOT define - the fallback for references a name index never sees (a string-literal
    ///   event name, a constant prefix, a name nothing defines yet), outside spec files and the
    ///   regenerated audit. A defined name and a path are never text-searched, except that a file
    ///   the worktree holds differently from the view is withheld from the structural view, so a
    ///   text hit in it is added for every named symbol: a missed reference is the unsafe error.
    /// - A HUB term (a name referenced across much of the tree) fails SAFE through `safe` itself:
    ///   every file that defines it or holds a call reference to it is in the view, never
    ///   truncated, so the partitioning consumer's overlap test keeps the unit apart from exactly
    ///   the units that share one of those files - and pairs it with every unit that shares none.
    ///
    /// Determinism is by construction: the served files come in path order, so both structural
    /// passes are sorted, and the grep-recovered tail of `safe` is explicitly SORTED before it is
    /// appended - grep itself walks the tree in unsorted `read_dir` order, so without the sort
    /// `safe` would be set-deterministic but not order-deterministic. Sorting the tail makes the
    /// whole `safe` ordering reproducible across processes, which is what unit 3 needs to hash the
    /// seed-file list into a stable `BlastRadiusComputed` audit event. A query naming no code -
    /// empty, prose only, a single character (ASCII or multibyte) or all punctuation - returns
    /// empty views (the empty-radius fail-safe the scheduler runs alone and unit 3 routes to the
    /// full panel) WITHOUT running a whole-repo grep, never a partial or a panic. It is
    /// [`Self::blast_radius_stamped`]'s radius: one read gives both. A query WITH code
    /// terms that simply matches nothing is ALSO empty, but that case does run the uncapped search
    /// per unresolved name - it just comes back empty.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        self.blast_radius_stamped(query, k).radius
    }

    /// The prompt seed and the radius with the provenance stamp `<tree-id>/<grammar-tags-version>`
    /// of the one open that grounded both: the tree the view describes unioned with the grammar / tag-query version. The
    /// `symbols` grounder is STRUCTURAL, so the stamp is NON-EMPTY - the signal unit 3's conductor
    /// keys the audit + retention metric off - even for the empty-radius fail-safe, whose open
    /// still names the tree. A change to the tree changes the stamp.
    fn blast_radius_stamped(&self, query: &str, k: usize) -> StampedRadius {
        // The query's terms are the CODE it names ([`code_terms`]), never its prose: a criterion's
        // prose words that happen to be symbol names (`tests`, `run`, `parse`) would otherwise pull
        // every file defining or referencing them into the radius.
        let terms = code_terms(query);
        let ((seed, radius), _label, tree) = self.read_opened(|sight| {
            let seed = self.seed_in(sight, query, k);
            // The empty-terms fail-safe, applied BEFORE any text search: a query naming no code
            // (an empty query, prose only, a single character, all punctuation) grounds to the
            // empty radius - empty precise and empty safe - which the scheduler runs alone and
            // unit 3 routes to the full, unpartitioned panel.
            let radius = if terms.is_empty() {
                BlastRadius::default()
            } else {
                self.radius(sight, &terms, k)
            };
            (seed, radius)
        });
        StampedRadius {
            seed,
            radius,
            stamp: format!("{tree}/{GRAMMAR_TAGS_VERSION}"),
        }
    }

    /// The RANKED-BY-INTENT page (spec 92 criterion 3): [`scored_hits`]'s already-ranked list,
    /// deduplicated to one row per ENTITY - "six call sites of one function occupy one row with
    /// its degree." A DEFINITION is its own entity, keyed by `(name, file, line)` - always
    /// unique by construction, so two DIFFERENT definitions sharing a name (the constraints
    /// walk: "a name defined in two files - `ground` returns both entities as separate rows")
    /// never collapse into one, even when they share a file (six same-named methods on six
    /// structs in one file stay six rows). A REFERENCE - a call reference, the one kind
    /// [`scored_hits`] reads (spec 117) - is absorbed into its SOLE definer's row when the name
    /// has EXACTLY one definition anywhere in the tree; a name with ZERO or
    /// AMBIGUOUS (multiple) definitions is never guessed at - its references stand alone as
    /// their own (unattributed) entity rather than being pinned to one of several candidates.
    /// `degree` is the real [`Sight::reference_degree`] for the entity's name and
    /// language, not an approximate
    /// count of what happened to be visible in this page - EXCEPT on an ambiguous name's own
    /// Def row, which reports 0 rather than the tree-wide count (spec 92 criterion 3
    /// remediation round 6, adj-u92c3-r5-verdict-reject /
    /// adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): that count is exactly
    /// the unattributed reference pool the Standalone row exists to carry, so a Def row that
    /// cannot be pinned to any given reference must never claim it too - see the degree
    /// computation below for the full reasoning.
    fn ground_ranked(&self, query: &str, k: usize) -> Vec<RankedRef> {
        self.read(|sight| ranked_in(sight, &query_terms(query), k, Tests::Include))
    }

    fn ground_ranked_labelled(
        &self,
        query: &str,
        k: usize,
        tests: Tests,
    ) -> Grounded<Option<Vec<RankedRef>>> {
        let terms = query_terms(query);
        let ((rows, tests_hidden, withheld), mut label) = self.read_labelled(|sight| {
            let rows =
                (k == 0 || has_strong(sight, &terms)).then(|| ranked_in(sight, &terms, k, tests));
            let hidden = hidden_test_hits(&scored_hits(sight, &terms), tests);
            (rows, hidden, withheld_entities(sight, &terms, tests))
        });
        label.withheld = withheld;
        Grounded {
            rows,
            label: Some(label),
            tests_hidden,
        }
    }

    /// Whether `query` has at least one match whose MATCHED ENTITY sits AT OR BELOW the repo's
    /// own name-ambiguity distribution's [`AMBIGUITY_PERCENTILE`] cutoff (spec 92 criterion 3:
    /// "a query with no strong token returns the honest 'no entity matches strongly' line
    /// instead of noise"). Two fixes over the first round of this unit (spec 92 criterion 3
    /// remediation, adj-u92c3-verdict-reject):
    ///
    /// 1. Judges the SAME hits [`scored_hits`] (the actual ranker `ground`/`ground_ranked` use)
    ///    produces, rather than re-deriving a second raw lookup keyed on the QUERY TERM. A
    ///    CONTAINS-tier term (`"damage"` matching `apply_damage`) is never itself a name, so that
    ///    second lookup always missed and silently read as "not strong" - an absent-key sentinel
    ///    inversion against `scored_hits`' own rarest-by-default convention. Judging
    ///    [`ScoredHit::name`] (the entity actually matched) instead closes that gap.
    /// 2. Draws its cutoff from [`ambiguity_map`] (distinct-DEFINITION count per `(name,
    ///    language)`) rather than the raw definition-plus-call-reference occurrence count: a
    ///    single-definition, heavily-referenced entity (`criterion_stable_id` - one of this
    ///    repo's own Done-when audit fixtures) is completely unambiguous and must never be
    ///    misclassified as tree-wide-common merely because it is called often.
    /// 3. Scopes BOTH the per-hit ambiguity lookup AND the cutoff distribution itself by the
    ///    hit's OWN language (spec 92 criterion 3 remediation round 5,
    ///    adj-u92c3-r4-verdict-reject / adv-u92c3r4-ambiguity-map-bleeds-across-languages), as
    ///    [`Sight::reference_degree`] scopes its count: a name defined once in Rust and,
    ///    separately, once in an unrelated language is genuinely unambiguous in EACH language
    ///    alone; pooling the two definition counts into one bare-name bucket manufactures a
    ///    tree-wide-ambiguous verdict neither language's own distribution supports.
    ///
    /// A query with no matches at all is not strong either - it has no candidate to be
    /// confident about.
    fn has_strong_match(&self, query: &str, _k: usize) -> bool {
        self.read(|sight| has_strong(sight, &query_terms(query)))
    }
}

/// How many entities the hits of `terms` in the files `sight` withholds name, keyed as the
/// ranked page keys its rows: those its answer under `tests` would have shown or reported hidden,
/// the unit every graph read counts its withheld nodes in.
fn withheld_entities(sight: &Sight<'_>, terms: &[&str], tests: Tests) -> usize {
    let counted = match tests {
        Tests::Only => Tests::Only,
        Tests::Include | Tests::Exclude => Tests::Include,
    };
    ranked_in(&sight.withheld_part(), terms, usize::MAX, counted).len()
}

/// How many of `hits` are test locations `tests` leaves out of an answer: none unless it excludes
/// them.
fn hidden_test_hits(hits: &[ScoredHit], tests: Tests) -> usize {
    if tests == Tests::Exclude {
        hits.iter().filter(|h| h.test).count()
    } else {
        0
    }
}

/// The ranked-by-intent page of `terms` over what `sight` serves ([`Grounder::ground_ranked`]
/// states the contract): empty for no terms or `k == 0`.
fn ranked_in(sight: &Sight<'_>, terms: &[&str], k: usize, tests: Tests) -> Vec<RankedRef> {
    if terms.is_empty() || k == 0 {
        return Vec::new();
    }
    let hits: Vec<ScoredHit> = scored_hits(sight, terms)
        .into_iter()
        .filter(|h| tests.admits(h.test))
        .collect();

    // Every matched (name, language) pair's definition sites, so a REFERENCE hit can look
    // up whether it has a SOLE, SAME-LANGUAGE definer to absorb into. Read over the WHOLE
    // view (not just the hits), since a name's definition is only a "hit" itself when it
    // also matches a term - the absorption question ("how many definitions does this name
    // have, within this language") is independent of that. Keyed by `(name, language)`,
    // never a bare name (spec 92 criterion 3 remediation round 5, adj-u92c3-r4-verdict-
    // reject / adv-u92c3r4-ground-ranked-def-sites-cross-language-absorption): a bare-name
    // key let a Rust reference with NO Rust definition (e.g. a `.unwrap()` call site)
    // resolve through a same-named definition in an unrelated language as if it were that
    // entity's own sole definer - silently absorbing and deduplicating the entire
    // cross-language reference population into ONE foreign row.
    let mut def_sites: HashMap<(String, String), Vec<(String, u32)>> = HashMap::new();

    #[derive(PartialEq, Eq, Hash, Clone)]
    enum EntityKey {
        /// A specific definition site - always a distinct entity.
        Def(String, String, u32),
        /// A `(name, language)` with zero or ambiguous (multiple SAME-LANGUAGE)
        /// definitions - its references stand alone, keyed by name AND language (never a
        /// bare name: a standalone Rust `unwrap` and a standalone Python `unwrap` are
        /// different entities, there is only ever one such row per pair).
        Standalone(String, String),
    }

    let mut seen: HashSet<EntityKey> = HashSet::new();
    let mut out: Vec<RankedRef> = Vec::new();
    for h in hits {
        // How many SAME-LANGUAGE definitions this hit's own name has - independent of
        // which hit kind matched, since a Def hit's own definition is always counted in
        // `def_sites` and a Ref hit's sole-definer lookup below uses the identical key.
        let sites = def_sites
            .entry((h.name.clone(), h.lang.clone()))
            .or_insert_with(|| {
                sight
                    .defs_named(&h.name)
                    .filter(|d| d.file.lang == h.lang)
                    .map(|d| (d.file.path.to_string(), d.line))
                    .collect()
            });
        let def_count = sites.len();
        let key = match h.kind {
            HitKind::Def => EntityKey::Def(h.name.clone(), h.file.clone(), h.line),
            HitKind::Ref => match sites.as_slice() {
                [(file, line)] => EntityKey::Def(h.name.clone(), file.clone(), *line),
                _ => EntityKey::Standalone(h.name.clone(), h.lang.clone()),
            },
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        // An AMBIGUOUS (more than one same-language definition) entity's own Def row
        // reports an attributable degree of 0, never the tree-wide call-reference count
        // (spec 92 criterion 3 remediation round 6, adj-u92c3-r5-verdict-reject /
        // adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): the
        // Standalone row for this same `(name, language)` exists SPECIFICALLY because a
        // reference cannot be pinned to any one of its several candidates, so a Def row
        // must never turn around and claim that identical unattributable pool as its own
        // - the opposite of what creating the Standalone row already decided. An
        // UNAMBIGUOUS (exactly one definer) Def row keeps the real reference degree -
        // every call reference to that name genuinely IS this entity's own, nothing is
        // pooled. A Standalone row always keeps the real reference degree too - it IS the
        // pooled, unattributed call-reference count (no tagged reference counts, spec
        // 117), by construction of why it exists at all.
        let degree = match key {
            EntityKey::Def(..) if def_count > 1 => 0,
            _ => sight.reference_degree(&h.name, &h.lang),
        };
        out.push(RankedRef {
            loc: Ref {
                file: h.file,
                line: h.line,
                text: h.name,
            },
            degree,
        });
        if out.len() >= k {
            break;
        }
    }
    out
}

/// Whether `terms` have a match over what `sight` serves whose entity sits at or below the
/// name-ambiguity cutoff ([`Grounder::has_strong_match`] states the contract): false for no terms.
fn has_strong(sight: &Sight<'_>, terms: &[&str]) -> bool {
    if terms.is_empty() {
        return false;
    }
    let hits = scored_hits(sight, terms);
    if hits.is_empty() {
        return false;
    }
    let ambiguity = ambiguity_map(sight);
    // The ambiguity cutoff, drawn SEPARATELY per language from that language's OWN
    // distinct-definition distribution - never one pooled cutoff across every language
    // present (5.5.2). A language absent from `ambiguity` (no definition anywhere in it)
    // falls back to 0: nothing has EVER been observed as ambiguous there, so every hit in
    // that language trivially clears the cutoff, matching the `unwrap_or(0)` below for
    // every such hit regardless.
    let mut cutoffs: BTreeMap<&str, usize> = BTreeMap::new();
    for lang in hits
        .iter()
        .map(|h| h.lang.as_str())
        .collect::<BTreeSet<_>>()
    {
        let mut degrees: Vec<usize> = ambiguity
            .iter()
            .filter_map(|((_, l), &count)| (l == lang).then_some(count))
            .collect();
        let cutoff = if degrees.is_empty() {
            0
        } else {
            percentile_cutoff(&mut degrees, AMBIGUITY_PERCENTILE)
        };
        cutoffs.insert(lang, cutoff);
    }
    hits.iter().any(|h| {
        let cutoff = cutoffs.get(h.lang.as_str()).copied().unwrap_or(0);
        let (name, lang) = h.entity();
        let ambiguous = ambiguity
            .get(&(name.to_string(), lang.to_string()))
            .copied()
            .unwrap_or(0);
        ambiguous <= cutoff
    })
}

impl Symbols {
    /// The two views of the radius of `terms` over what `sight` serves ([`Grounder::blast_radius`]
    /// states the contract).
    fn radius(&self, sight: &Sight<'_>, terms: &CodeTerms, k: usize) -> BlastRadius {
        let names: Vec<&str> = terms.names.iter().map(|n| n.name.as_str()).collect();

        // The STRUCTURAL view, ranked (definer files, then referencer files not already a
        // definer), each group in path order. A file the criterion names counts as a definer; a
        // symbol it names matches by name.
        let mut defined: HashSet<&str> = HashSet::new();
        let mut found_paths: HashSet<&str> = HashSet::new();
        let mut definers: BTreeSet<&str> = BTreeSet::new();
        for file in sight.files() {
            for t in terms.paths.iter().filter(|t| names_path(file.path, t)) {
                found_paths.insert(t.as_str());
                definers.insert(file.path);
            }
        }
        for name in &names {
            for d in sight.defs_named(name) {
                defined.insert(*name);
                definers.insert(d.file.path);
            }
        }
        let referencers: BTreeSet<&str> = names
            .iter()
            .flat_map(|name| sight.calls_named(name))
            .map(|m| m.file.path)
            .collect();
        // A named path the view holds no facts for (`.gitignore`) still grounds on its own file
        // when the tree holds it; a path the tree does not hold grounds nothing.
        let unheld: Vec<&str> = terms
            .paths
            .iter()
            .map(String::as_str)
            .filter(|t| !found_paths.contains(t) && held_in_tree(&self.root, t))
            .collect();
        // Ranked: every definer file first, then each referencer that is not also a definer.
        let structural: Vec<String> = definers
            .iter()
            .copied()
            .chain(unheld)
            .chain(referencers.difference(&definers).copied())
            .map(str::to_string)
            .collect();

        // The SAFE view: the FULL (untruncated) structural set UNIONed with an UNCAPPED text search
        // for each name the view does NOT define - the fallback for the references a name index
        // misses (a string-literal event name, a constant prefix, a name nothing defines yet). A
        // name the view resolves is grounded structurally alone: text-searching it would put
        // every file mentioning a common type name in the radius. A path is never text-searched.
        // The search matches whole identifiers only and skips [`TEXT_SEARCH_SKIPS`]. This clone
        // happens BEFORE `precise` is capped, so the safe view is never bounded by `k`; do not
        // reorder the truncation below it. A `seen` set keeps the dedup O(lines) rather than
        // O(lines * files): the search yields one hit per matching LINE and the safe view is
        // uncapped.
        let mut safe = structural.clone();
        let mut seen: HashSet<String> = safe.iter().cloned().collect();
        let grep = Grep {
            root: self.root.clone(),
        };
        // The grep-recovered tail: the files grep matched that the structural view missed. Grep
        // walks the tree in unsorted `read_dir` order, so collect the tail and SORT it before
        // appending, which makes the whole `safe` ordering reproducible across processes (unit 3
        // hashes the seed-file list into a `BlastRadiusComputed` event that must be cross-process
        // byte-identical). Sorting a set of distinct paths (not the raw grep hits) keeps this
        // O(tail log tail), not per-line.
        let mut grep_tail: Vec<String> = Vec::new();
        for n in &terms.names {
            let resolved = defined.contains(n.name.as_str());
            // A resolved name is searched only for the files the worktree withholds from the view.
            if resolved && !sight.has_withheld_files() {
                continue;
            }
            for r in grep.ground_identifier(&n.text, usize::MAX) {
                if (resolved && !sight.withholds(&r.file))
                    || TEXT_SEARCH_SKIPS
                        .iter()
                        .any(|skip| r.file.starts_with(skip))
                {
                    continue;
                }
                if seen.insert(r.file.clone()) {
                    grep_tail.push(r.file);
                }
            }
        }
        grep_tail.sort();
        safe.extend(grep_tail);

        // The precise view is the ranked structural set capped at `k`; the safe view stays uncapped.
        let mut precise = structural;
        precise.truncate(k);
        BlastRadius { precise, safe }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::FactsExtractor;
    use crate::grounder::Grounder;
    use rigger_domain::code_facts::{Def, FileFacts, Mention, Stamp, Tip};
    use rigger_domain::code_view::{Entry, Extract, Head, Tests, WorktreeDiff};
    use rigger_domain::composed::Compared;

    /// The `symbols` grounder is STRUCTURAL, so its radius stamp (unit 3's audit provenance +
    /// the structural-active signal the conductor keys the `BlastRadiusComputed` audit off) is
    /// NON-EMPTY, shaped `<index-content-hash>/<grammar-tags-version>`, and CHANGES with the
    /// index content - so a recorded radius reconstructs which index generation grounded it.
    #[test]
    fn radius_stamp_is_nonempty_and_tracks_index_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());
        let stamp = g.blast_radius_stamped("`foo`", 8).stamp;
        assert!(
            !stamp.is_empty(),
            "symbols is structural: the audit stamp must be non-empty so the conductor emits the audit"
        );
        assert!(
            stamp.contains('/')
                && stamp.ends_with(crate::grounder::symbols::model::GRAMMAR_TAGS_VERSION),
            "the stamp is <index-content-hash>/<grammar-tags-version>; got {stamp:?}"
        );
        // A DIFFERENT index (different symbols) yields a different content-hash half, so a radius
        // recorded under one index generation is distinguishable from one under another.
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::write(dir2.path().join("b.rs"), "fn bar() {}\nfn baz() {}\n").unwrap();
        let g2 = Symbols::in_directory(dir2.path().to_str().unwrap());
        assert_ne!(
            g.blast_radius_stamped("`foo`", 8).stamp,
            g2.blast_radius_stamped("`foo`", 8).stamp,
            "distinct index content must yield a distinct provenance stamp"
        );
        // DETERMINISM (the load-bearing provenance/replay property): the SAME tree opened by a
        // SECOND grounder must yield the SAME stamp - the compact `BTreeMap` serialization is
        // sorted-key stable, so a radius replayed in another process reconstructs to the same
        // index generation. A `to_string` -> `to_vec` drift, or a non-deterministic index order,
        // would trip this.
        let dir3 = tempfile::tempdir().unwrap();
        std::fs::write(dir3.path().join("a.rs"), "fn foo() {}\n").unwrap();
        let g3a = Symbols::in_directory(dir3.path().to_str().unwrap());
        let g3b = Symbols::in_directory(dir3.path().to_str().unwrap());
        assert_eq!(
            g3a.blast_radius_stamped("`foo`", 8).stamp,
            g3b.blast_radius_stamped("`foo`", 8).stamp,
            "the same tree opened twice must yield the SAME stamp (cross-process determinism)"
        );
    }

    #[test]
    fn ranks_a_definition_above_an_incidental_prose_mention() {
        let dir = tempfile::tempdir().unwrap();
        // combat.rs DEFINES apply_damage; notes.rs only MENTIONS it in a comment (prose).
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(x: u8) -> u8 { x }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("notes.rs"),
            "// TODO: think about apply_damage someday\nfn unrelated() {}\n",
        )
        .unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());
        let refs = g.ground("apply_damage", 5);
        assert!(!refs.is_empty(), "the definition must be grounded");
        // The DEFINITION site outranks the prose mention: combat.rs is first, and the prose
        // mention (a comment, not a symbol) never appears at all.
        assert_eq!(refs[0].file, "combat.rs");
        assert!(
            !refs.iter().any(|r| r.file == "notes.rs"),
            "an incidental prose mention is not indexed as a symbol and must not be grounded; got {refs:?}"
        );
    }

    /// Spec 16 unit 1, the criterion-1 fixture: `apply_damage` is defined in one file, called (a
    /// real symbol reference the graph links) in another, and mentioned ONLY in a COMMENT in a
    /// third. The index resolves the name, so its radius is the structural set - the definer
    /// ranked above the call site - and the comment mention, which no unit edits through the
    /// name, stays out of both views (the text search is the fallback for spans the index cannot
    /// resolve, `blast_radius_text_searches_only_the_spans_the_index_does_not_resolve`).
    #[test]
    fn a_resolved_name_grounds_on_its_definer_and_call_sites_never_on_a_comment_mention() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(x: u8) -> u8 { x }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("caller.rs"),
            "fn go() { apply_damage(1); }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("notes.rs"),
            "// apply_damage is discussed but never called here\n",
        )
        .unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let br = g.blast_radius("apply_damage", 8);
        // The precise view IS the structural cross-reference graph: the definer and the real call
        // site, ranked - and NOT the comment-only mention (which is no symbol).
        assert!(
            br.precise.contains(&"combat.rs".to_string()),
            "the definer must be in the precise view; got {br:?}"
        );
        assert!(
            br.precise.contains(&"caller.rs".to_string()),
            "the real call site must be in the precise view; got {br:?}"
        );
        assert!(
            !br.precise.contains(&"notes.rs".to_string()),
            "a comment-only mention is not a symbol; the precise view must exclude it; got {br:?}"
        );
        // The definer ranks ABOVE the referencer in the precise view.
        let combat_at = br.precise.iter().position(|f| f == "combat.rs").unwrap();
        let caller_at = br.precise.iter().position(|f| f == "caller.rs").unwrap();
        assert!(
            combat_at < caller_at,
            "the definer must rank above the referencer; got {br:?}"
        );
        // The name resolves, so the comment mention is never text-searched into the safe view.
        assert!(
            !br.safe.contains(&"notes.rs".to_string()),
            "a resolved name's safe view carries no comment-only mention; got {br:?}"
        );
        // And it is a strict superset of the precise (structural) view.
        for f in &br.precise {
            assert!(
                br.safe.contains(f),
                "safe must be a superset of precise; missing {f} in {br:?}"
            );
        }
    }

    /// A criterion grounds its blast radius on the CODE it names, never on its prose: the query
    /// terms are its code spans plus identifier-shaped bare tokens, so the prose words `tests`,
    /// `run` and `parse` - which happen to be symbol names here - pull in nothing, and the radius
    /// is `reclaim_space`'s alone. A criterion naming no code at all grounds on nothing (the empty
    /// radius, which the scheduler serializes).
    #[test]
    fn blast_radius_grounds_a_criterion_on_its_code_spans_not_its_prose() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("reclaim.rs"), "fn reclaim_space() {}\n").unwrap();
        std::fs::write(
            dir.path().join("caller.rs"),
            "fn go() { reclaim_space(); }\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("tests.rs"), "fn tests() {}\n").unwrap();
        std::fs::write(
            dir.path().join("run.rs"),
            "fn run() { parse(); tests(); }\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("parse.rs"), "fn parse() {}\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let br = g.blast_radius(
            "a test proves the run reclaims space: `reclaim_space` frees what tests and parse \
             leave behind, asserted in the run's tests",
            8,
        );
        let mut safe = br.safe.clone();
        safe.sort();
        assert_eq!(
            safe,
            vec!["caller.rs".to_string(), "reclaim.rs".to_string()],
            "the radius is the named span's alone, never the prose words'; got {br:?}"
        );

        let prose = g.blast_radius("the run parses its tests and reclaims one of them", 8);
        assert_eq!(
            prose,
            BlastRadius::default(),
            "a criterion naming no code grounds on nothing; got {prose:?}"
        );
    }

    /// Over a tree holding `files`, `query`'s safe view is exactly `expected`.
    fn assert_safe(files: &[(&str, &str)], query: &str, expected: &[&str], why: &str) {
        let (_tree, g) = grounder_over(files);
        let br = g.blast_radius(query, 8);
        assert_eq!(br.safe, expected, "{why}; got {br:?}");
    }

    /// Over a tree holding `files`, every one of `queries` grounds nothing - the empty radius a
    /// criterion naming no code gets.
    fn assert_grounds_nothing(files: &[(&str, &str)], queries: &[&str]) {
        let (_tree, g) = grounder_over(files);
        for query in queries {
            let br = g.blast_radius(query, 8);
            assert_eq!(
                br,
                BlastRadius::default(),
                "{query} grounds nothing; got {br:?}"
            );
        }
    }

    /// A span naming only language keywords (`&mut self`, `pub fn`) names no code at all.
    #[test]
    fn blast_radius_never_grounds_on_language_keywords() {
        assert_grounds_nothing(
            &[("check.rs", "pub fn validate(&mut self) {}\n")],
            &["the method takes `&mut self` and is `pub fn`"],
        );
    }

    /// A span naming a path the tree holds grounds on that file itself, indexed or not (a
    /// `.gitignore` the symbol index never parses), and never on the files that mention the path.
    #[test]
    fn blast_radius_grounds_a_path_the_tree_holds_on_that_file_alone() {
        assert_safe(
            &[
                (".gitignore", "target/\n"),
                ("notes.rs", "// see .gitignore for the build dir\n"),
            ],
            "`.gitignore` lists the build dir",
            &[".gitignore"],
            "a held path grounds on its own file alone",
        );
    }

    /// A span of several words (`rigger validate`) is prose about the code, not a name of it, so
    /// it grounds nothing - not on the files holding the phrase, nor on its words.
    #[test]
    fn blast_radius_a_multi_word_span_grounds_nothing() {
        assert_grounds_nothing(
            &[
                ("verb.rs", "// run rigger validate first\n"),
                ("check.rs", "fn validate() {}\n"),
            ],
            &["`rigger validate` refuses a drifted spec"],
        );
    }

    /// A path the tree does not hold (`graph.db`, a runtime artifact), a directory fragment
    /// (`src/`) and an absolute path (which would escape the tree) name no file of the tree, so
    /// they ground nothing - never the files mentioning them, never a file outside the tree.
    #[test]
    fn blast_radius_a_path_the_tree_does_not_hold_grounds_nothing() {
        let outside = tempfile::NamedTempFile::new().unwrap();
        let absolute = format!("`{}` is read", outside.path().display());
        assert_grounds_nothing(
            &[("src/open.rs", "// opens graph.db under src/ here\n")],
            &[
                "`graph.db` holds the graph",
                "code lives in `src/`",
                &absolute,
            ],
        );
    }

    /// A span ending in `_` names a FAMILY by its prefix (`TYPE_` for the `TYPE_*` constants): the
    /// text search matches it with the left identifier boundary only, so it finds every member
    /// and never a name that merely contains the prefix.
    #[test]
    fn blast_radius_text_search_matches_a_trailing_underscore_span_as_a_prefix() {
        assert_safe(
            &[
                ("consts.rs", "const KIND: &str = TYPE_RUN_STARTED;\n"),
                ("other.rs", "const K: &str = MY_TYPE_X;\n"),
            ],
            "the `TYPE_` constants",
            &["consts.rs"],
            "a trailing-underscore span matches its family by prefix",
        );
    }

    /// The text search matches a span only on identifier boundaries: `gc` matches where it stands
    /// as its own word, never inside `logcat`.
    #[test]
    fn blast_radius_text_search_matches_a_span_only_on_identifier_boundaries() {
        assert_safe(
            &[
                ("word.rs", "// run gc now\n"),
                ("inside.rs", "// logcat output\n"),
            ],
            "`gc` reclaims space",
            &["word.rs"],
            "a span matches only as a whole identifier",
        );
    }

    /// A span the index resolves to a definition grounds on its structural set alone, never on
    /// every file mentioning the word; a span the index cannot resolve (a string-literal event
    /// name) still falls back to the text search.
    #[test]
    fn blast_radius_text_searches_only_the_spans_the_index_does_not_resolve() {
        let files = [
            ("def.rs", "pub struct Store;\n"),
            ("prose.rs", "// the Store is only mentioned here\n"),
            ("events.rs", "const STARTED: &str = \"RunStarted\";\n"),
        ];
        assert_safe(
            &files,
            "`Store` reclaims space",
            &["def.rs"],
            "a resolved span grounds on its structural set, not its mentions",
        );
        assert_safe(
            &files,
            "`RunStarted` opens a run",
            &["events.rs"],
            "an unresolved span falls back to the text search",
        );
    }

    /// The text search never matches a spec file, which no unit edits. Everything else stays in
    /// scope: tests, the handbook and `docs/audit/`.
    #[test]
    fn blast_radius_text_search_skips_specs_alone() {
        assert_safe(
            &[
                ("code.rs", "const K: &str = \"unindexed_name\";\n"),
                ("specs/107-x.md", "uses `unindexed_name`\n"),
                (
                    "docs/audit/catalog.json",
                    "{\"name\": \"unindexed_name\"}\n",
                ),
                ("docs/audit/report.md", "unindexed_name\n"),
                ("docs/handbook/page.md", "unindexed_name\n"),
                ("tests/cli.rs", "// unindexed_name\n"),
            ],
            "`unindexed_name` is read",
            &[
                "code.rs",
                "docs/audit/catalog.json",
                "docs/audit/report.md",
                "docs/handbook/page.md",
                "tests/cli.rs",
            ],
            "specs are never text-search hits",
        );
    }

    /// A span keeps the name it carries through generic and call arguments, trailing marks and
    /// member access: `spawn_unit<T>`, `finish(x)`, `parked:` and `store.open()` each match the
    /// symbol they name, and a dotted term is a path only when it ends in a file extension.
    #[test]
    fn blast_radius_reads_the_name_inside_generics_calls_marks_and_member_access() {
        let dir = tempfile::tempdir().unwrap();
        for (file, def) in [
            ("generic.rs", "spawn_unit"),
            ("call.rs", "finish"),
            ("mark.rs", "parked"),
            ("member.rs", "open"),
        ] {
            std::fs::write(dir.path().join(file), format!("fn {def}() {{}}\n")).unwrap();
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());
        for (query, file) in [
            ("`spawn_unit<T>`", "generic.rs"),
            ("`finish(x)`", "call.rs"),
            ("`parked:`", "mark.rs"),
            ("`store.open()`", "member.rs"),
            ("`call.rs`", "call.rs"),
        ] {
            let br = g.blast_radius(query, 8);
            assert!(
                br.safe.contains(&file.to_string()),
                "{query} names the code in {file}; got {br:?}"
            );
        }
        // A store file (`graph.db`) is a path, never the symbol `db`.
        let store_dir = tempfile::tempdir().unwrap();
        std::fs::write(store_dir.path().join("handle.rs"), "fn db() {}\n").unwrap();
        let g = Symbols::in_directory(store_dir.path().to_str().unwrap());
        let br = g.blast_radius("`graph.db`", 8);
        assert!(
            !br.safe.contains(&"handle.rs".to_string()),
            "a .db file name never grounds on a symbol named db; got {br:?}"
        );
    }

    /// A HUB symbol (a name referenced across many files) fails SAFE through its radius, never by
    /// truncating it and never by a conflict-with-everything flag: the safe view carries EVERY file
    /// of the hub's neighborhood (even past the `k` cap), so the overlap test keeps it apart from
    /// exactly the units that share one of those files and pairs it with every unit that shares
    /// none.
    #[test]
    fn a_hub_symbol_s_safe_view_carries_its_whole_neighborhood_untruncated() {
        let dir = tempfile::tempdir().unwrap();
        // `spawn` is referenced across many files (a hub).
        std::fs::write(dir.path().join("def.rs"), "fn spawn() {}\n").unwrap();
        let mut expected: Vec<String> = vec!["def.rs".to_string()];
        for i in 0..12 {
            let name = format!("f{i}.rs");
            std::fs::write(dir.path().join(&name), "fn c() { spawn(); }\n").unwrap();
            expected.push(name);
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        // A SMALL cap proves the safe view is uncapped: there are 13 `spawn` files, more than k=8.
        let br = g.blast_radius("`spawn`", 8);
        for f in &expected {
            assert!(
                br.safe.contains(f),
                "the hub safe view must not truncate {f}; got {br:?}"
            );
        }
        assert!(
            br.safe.len() >= expected.len(),
            "the safe view is uncapped: all {} hub files must be present, not capped at 8; got {} in {br:?}",
            expected.len(),
            br.safe.len()
        );
    }

    /// The blast-radius fail-safe paths (spec 16 unit 1): an empty query and a no-match query each
    /// return EMPTY views (unit 3 routes an empty radius to the full,
    /// unpartitioned panel). A `k=0` cap collapses the PRECISE view to empty, but the SAFE view is
    /// UNCAPPED by design - it still carries the full safe radius (addendum 2.4) so the
    /// partitioning consumer can never under-include just because the prompt budget was zero.
    #[test]
    fn blast_radius_empty_and_no_match_are_the_empty_failsafe_and_k0_keeps_safe_uncapped() {
        let dir = tempfile::tempdir().unwrap();
        // A definer and a real referencer of `parse`, so the radius is non-empty for a real query.
        std::fs::write(dir.path().join("def.rs"), "fn parse() {}\n").unwrap();
        std::fs::write(dir.path().join("call.rs"), "fn run() { parse(); }\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        // Empty query -> no terms -> empty views.
        let empty = g.blast_radius("", 8);
        assert!(
            empty.precise.is_empty() && empty.safe.is_empty(),
            "an empty query is the empty fail-safe: both views empty; got {empty:?}"
        );

        // A name that appears nowhere -> nothing structural AND nothing grep -> empty views.
        let none = g.blast_radius("nonexistent_symbol_zzz", 8);
        assert!(
            none.precise.is_empty() && none.safe.is_empty(),
            "a no-match query is the empty fail-safe: both views empty; got {none:?}"
        );

        // k=0 caps the PRECISE view to empty; the SAFE view is uncapped and still carries the radius.
        let k0 = g.blast_radius("`parse`", 0);
        assert!(
            k0.precise.is_empty(),
            "k=0 caps the precise view to empty; got {k0:?}"
        );
        assert!(
            k0.safe.contains(&"def.rs".to_string()) && k0.safe.contains(&"call.rs".to_string()),
            "the safe view is uncapped even at k=0 - it carries the full radius; got {k0:?}"
        );
    }

    /// Spec 17 criterion 2 (plan17-c2-blast-radius-empty-terms-guard): a DEGENERATE query - one
    /// whose only tokens are dropped by the character-count term filter (a single character, ASCII
    /// OR multibyte, or all punctuation) - must yield an EMPTY blast radius, IDENTICAL in emptiness
    /// to `ground` on the same query. The multibyte arm is the teeth: a byte-length filter keeps a
    /// 2-byte character as a term and greps the whole tree, so counting CHARACTERS is what closes
    /// the fail-safe for the full Unicode single-character class.
    /// This is distinct from the empty-QUERY case the sibling test already covers: the
    /// query here is non-empty, but every term is filtered out. Before the guard, `ground`
    /// early-returned on empty terms while `blast_radius` fell through to an UNBOUNDED whole-repo
    /// grep (`grep.ground(query, usize::MAX)`), so `precise` was empty while `safe` covered almost
    /// the entire tree - forcing the full review panel and corrupting the retention metric. The
    /// single shared `query_terms` authority makes the two agree: no terms -> the empty fail-safe
    /// radius, BEFORE grep is ever run.
    #[test]
    fn blast_radius_on_a_degenerate_terms_query_is_empty_not_a_whole_repo_grep() {
        let dir = tempfile::tempdir().unwrap();
        // Every file contains the letter `a` AND the multibyte char U+00E9 (`\u{e9}`, an accented
        // `e`, TWO UTF-8 bytes but ONE Unicode character), so an UNGUARDED single-character grep
        // for either would match every file - the whole-repo blow-up the guard exists to prevent.
        // The `\u{e9}` escape keeps the source ASCII while the runtime string is the real 2-byte
        // character, which is exactly the class the byte-length filter used to miss.
        std::fs::write(dir.path().join("alpha.rs"), "fn parse() {} // \u{e9}\n").unwrap();
        std::fs::write(
            dir.path().join("beta.rs"),
            "fn draw() { parse(); } // \u{e9}\n",
        )
        .unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        // A single-character query: its only token is one CHARACTER, dropped by the char-count term
        // filter, so it has NO symbol terms. `ground` returns nothing...
        assert!(
            g.ground("a", 8).is_empty(),
            "a degenerate single-char query grounds to nothing"
        );
        // ...and `blast_radius` must be the IDENTICAL empty radius - not a whole-repo grep for "a".
        let one_char = g.blast_radius("a", 8);
        assert_eq!(
            one_char,
            BlastRadius::default(),
            "a degenerate single-char query is the empty fail-safe radius (empty precise, empty \
             safe), not an unbounded whole-repo grep; got {one_char:?}"
        );

        // A single MULTIBYTE character (U+00E9: 2 UTF-8 bytes, but still ONE Unicode character).
        // The term filter must drop it by CHARACTER count, not byte length - a byte-length `>= 2`
        // filter would keep this 2-byte token, miss the empty-terms guard, and fall through to the
        // uncapped whole-repo grep (which matches BOTH files here since each contains the char).
        // `ground` returns nothing (no symbol named `\u{e9}`)...
        let multibyte = "\u{e9}";
        assert!(
            g.ground(multibyte, 8).is_empty(),
            "a degenerate single-MULTIBYTE-char query grounds to nothing"
        );
        // ...and `blast_radius` must be the IDENTICAL empty radius, NOT a whole-repo grep for the
        // 2-byte character. This arm is RED under a byte-length filter (safe = both files) and
        // GREEN once the filter counts CHARACTERS.
        let one_multibyte = g.blast_radius(multibyte, 8);
        assert_eq!(
            one_multibyte,
            BlastRadius::default(),
            "a single multibyte char (2 bytes, ONE char) is the empty fail-safe radius, not an \
             unbounded whole-repo grep for the substring; got {one_multibyte:?}"
        );

        // An all-punctuation query splits into only empty/dropped tokens - the same empty radius,
        // again matching `ground`.
        assert!(
            g.ground("!!!", 8).is_empty(),
            "an all-punctuation query grounds to nothing"
        );
        let punct = g.blast_radius("!!!", 8);
        assert_eq!(
            punct,
            BlastRadius::default(),
            "an all-punctuation query is the empty fail-safe radius, not a grep set; got {punct:?}"
        );
    }

    /// Spec 16 unit 1, the cross-language over-inclusion fixture (sdet-u16-1-structural-view-cross
    /// -language): the structural referencer scan iterates `files()` and matches refs BY NAME across
    /// ALL languages, mirroring `ground`'s own cross-language matching. So a query for a Rust symbol
    /// pulls in a Python file that references the same name - the OVER-inclusion (safe) direction,
    /// which is correct by construction (definers/referencers are deliberately cross-language for
    /// grounding recall). This pins that a Python
    /// referencer of a Rust-defined name lands in the precise AND safe views, and that safe stays a
    /// superset of precise. Gated behind the `symbols` feature like every test here (real parsing).
    #[test]
    fn structural_view_is_cross_language_a_python_referencer_of_a_rust_symbol_is_included() {
        let dir = tempfile::tempdir().unwrap();
        // Rust DEFINES `render`; Python CALLS `render` (a real symbol reference, in another
        // language) and mentions it in a comment only.
        std::fs::write(dir.path().join("view.rs"), "fn render() {}\n").unwrap();
        std::fs::write(dir.path().join("client.py"), "def draw():\n    render()\n").unwrap();
        std::fs::write(
            dir.path().join("notes.py"),
            "# render is described in prose only, never called\n",
        )
        .unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let br = g.blast_radius("`render`", 8);
        // The Rust definer is present (the precise structural view).
        assert!(
            br.precise.contains(&"view.rs".to_string()),
            "the Rust definer must be in the precise view; got {br:?}"
        );
        // The Python CALL SITE is a cross-language structural referencer, pulled into precise.
        assert!(
            br.precise.contains(&"client.py".to_string()),
            "the cross-language Python referencer must be in the precise structural view (over-inclusion); got {br:?}"
        );
        // The comment-only Python mention is no symbol, and the resolved name is never
        // text-searched, so it stays out of the safe view.
        assert!(
            !br.safe.contains(&"notes.py".to_string()),
            "a resolved name's safe view carries no comment-only mention; got {br:?}"
        );
        // Safe is a superset of precise.
        for f in &br.precise {
            assert!(
                br.safe.contains(f),
                "safe must be a superset of precise; missing {f} in {br:?}"
            );
        }
    }

    /// Over a tree holding `files`, `query`'s prompt seed at cap `k` is exactly `expected`.
    fn assert_seed(files: &[(&str, &str)], query: &str, k: usize, expected: &[&str], why: &str) {
        let (_tree, g) = grounder_over(files);
        assert_eq!(g.seed(query, k), expected, "{why}");
    }

    crate::test_cases! {
        /// A backticked identifier seeds its definition file ahead of a name that merely covers
        /// more of the criterion's prose words.
        seed_puts_a_named_identifier_s_definition_file_before_any_prose_match: assert_seed(
            &[
                ("src/conductor.rs", "fn write_lookup_pointer() {}\n"),
                ("src/pointer_names.rs", "fn names_the_tools() {}\n"),
            ],
            "`write_lookup_pointer` names the tools",
            8,
            &["src/conductor.rs", "src/pointer_names.rs"],
            "the named definition comes first, the prose match fills after it",
        );
        /// A path the criterion spells out, bare in its prose, seeds that file first.
        seed_resolves_a_path_the_criterion_spells_bare_to_that_file: assert_seed(
            &[
                ("crates/dash/src/mcpserver.rs", "fn tool_list() {}\n"),
                ("src/listing.rs", "fn lists_every_tool() {}\n"),
            ],
            "crates/dash/src/mcpserver.rs lists every tool",
            8,
            &["crates/dash/src/mcpserver.rs", "src/listing.rs"],
            "the spelled path comes first, the prose match fills after it",
        );
        /// A test file the criterion names explicitly still seeds: only the prose fill skips test
        /// code.
        seed_takes_a_test_file_the_criterion_names_explicitly: assert_seed(
            &[("tests/guard.rs", "fn awk_is_held() {}\n")],
            "`tests/guard.rs` holds the awk case",
            8,
            &["tests/guard.rs"],
            "a named test path seeds its file",
        );
        /// Prose words never match a test function's sentence name, in a test directory or in a
        /// test module of a product file.
        seed_never_matches_prose_against_test_function_names: assert_seed(
            &[
                ("tests/notes.rs", "fn the_rebuild_owed_note_speaks_only_for_a_graph_db() {}\n"),
                ("src/note.rs", "fn rebuild_note() {}\n"),
                (
                    "src/lib.rs",
                    "#[cfg(test)]\nmod tests {\n    fn the_note_speaks_for_the_graph() {}\n}\n",
                ),
            ],
            "the rebuild owed note speaks only for a graph db",
            8,
            &["src/note.rs"],
            "the prose fill holds product code alone",
        );
        /// The named code fills the cap before any prose match does.
        seed_caps_at_k_with_the_named_code_first: assert_seed(
            &[
                ("a_prose.rs", "fn wires_every_spawn_gate() {}\n"),
                ("z.rs", "fn arm() {}\n"),
            ],
            "`arm` wires every spawn gate",
            1,
            &["z.rs"],
            "a cap of one keeps the named definition alone",
        );
        /// Among named identifiers, the one fewer files define seeds first, so a widely defined
        /// name cannot crowd out a specific one.
        seed_puts_a_rarely_defined_name_before_a_widely_defined_one: assert_seed(
            &[
                ("r1.rs", "fn run() {}\n"),
                ("r2.rs", "fn run() {}\n"),
                ("r3.rs", "fn run() {}\n"),
                ("combat.rs", "fn apply_damage() {}\n"),
            ],
            "`run` before `apply_damage`",
            2,
            &["combat.rs", "r1.rs"],
            "the one-definer name seeds before the three-definer name",
        );
    }

    /// A `Symbols` grounder over a fresh root holding `files` (`(name, source)` pairs); the
    /// root lives as long as the returned dir.
    fn grounder_over(files: &[(&str, &str)]) -> (tempfile::TempDir, Symbols) {
        let dir = tempfile::tempdir().unwrap();
        for (name, source) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());
        (dir, g)
    }

    /// The tree behind the gap-104 ranking tests: a trait, two product impls of it, two test doubles
    /// implementing it (one in a `#[cfg(test)]` module, one under a `tests/` directory), the one
    /// store opener every impl calls, an unrelated impl in the trait's own file, and unrelated
    /// names that merely contain a query word (`for`, `open`) in files that sort first.
    fn store_tree() -> (tempfile::TempDir, Symbols) {
        grounder_over(&[
            (
                "domain/eventstore.rs",
                "pub trait EventStore { fn append(&self); }\nstruct Guard;\nimpl Drop for Guard { fn drop(&mut self) {} }\n",
            ),
            (
                "tests/doubles.rs",
                "struct Double;\nimpl EventStore for Double { fn append(&self) {} }\n",
            ),
            (
                "store/sqlite.rs",
                "pub struct Store;\npub fn open_connection() {}\nimpl EventStore for Store { fn append(&self) { open_connection(); } }\n",
            ),
            (
                "store/kurrentdb.rs",
                "pub struct Store;\nimpl EventStore for Store { fn append(&self) { open_connection(); } }\n",
            ),
            (
                "conductor.rs",
                "#[cfg(test)]\nmod tests {\n    struct Fake;\n    impl EventStore for Fake { fn append(&self) {} }\n}\n",
            ),
            (
                "a_console/lib.rs",
                "fn malformed_input_for_op() {}\nfn formatter_for_display() {}\nfn reopen_window() {}\n",
            ),
        ])
    }

    /// The row position of the first ranked entity named `text` in `file`, failing loudly when
    /// the page does not carry it.
    fn position_of(ranked: &[RankedRef], file: &str, text: &str) -> usize {
        ranked
            .iter()
            .position(|r| r.loc.file == file && r.loc.text == text)
            .unwrap_or_else(|| panic!("no row {file}: {text} in {ranked:?}"))
    }

    /// Gap 104: `impl EventStore for` names the impl sites - each impl block ranks above names
    /// that merely contain `for`, and a product impl ranks above a test double.
    #[test]
    fn ground_ranked_puts_the_impl_sites_of_an_impl_shaped_query_above_incidental_word_matches() {
        let (_dir, g) = store_tree();
        let ranked = g.ground_ranked("impl EventStore for", 10);
        let sqlite = position_of(&ranked, "store/sqlite.rs", "impl EventStore for Store");
        let kurrent = position_of(&ranked, "store/kurrentdb.rs", "impl EventStore for Store");
        let double = position_of(&ranked, "conductor.rs", "impl EventStore for Fake");
        let tests_dir_double =
            position_of(&ranked, "tests/doubles.rs", "impl EventStore for Double");
        assert!(
            sqlite.max(kurrent) < double.min(tests_dir_double),
            "a product impl ranks above a test double, in a test module or a tests directory; \
             got {ranked:?}"
        );
        let other_impl = position_of(&ranked, "domain/eventstore.rs", "impl Drop for Guard");
        assert!(
            other_impl > double.max(tests_dir_double),
            "an impl of another trait matches the query's words only through its file path, so \
             it ranks below every impl the query names; got {ranked:?}"
        );
        for incidental in ["malformed_input_for_op", "formatter_for_display"] {
            if let Some(p) = ranked.iter().position(|r| r.loc.text == incidental) {
                assert!(
                    p > double,
                    "{incidental} merely contains `for`, so it ranks below every impl site; got {ranked:?}"
                );
            }
        }
    }

    /// Gap 104: a query naming an entity by its name AND its module path (`sqlite store open`)
    /// finds the entity in that module first, above a rarer name that matches one word alone.
    #[test]
    fn ground_ranked_puts_the_entity_matching_the_most_query_words_across_name_and_path_first() {
        let (_dir, g) = store_tree();
        let ranked = g.ground_ranked("sqlite store open", 3);
        assert_eq!(
            (ranked[0].loc.file.as_str(), ranked[0].loc.text.as_str()),
            ("store/sqlite.rs", "open_connection"),
            "got {ranked:?}"
        );
    }

    #[test]
    fn a_reference_ranks_below_a_definition_of_the_same_name() {
        // def.rs DEFINES parse; call.rs REFERENCES it (a call site is a real symbol reference).
        let (_dir, g) = grounder_over(&[
            ("def.rs", "fn parse() {}\n"),
            ("call.rs", "fn run() { parse(); }\n"),
        ]);
        let refs = g.ground("parse", 5);
        assert_eq!(
            refs[0].file, "def.rs",
            "the definition outranks the reference; got {refs:?}"
        );
    }

    #[test]
    fn a_self_referential_definition_grounds_once_at_its_highest_score() {
        // A recursive `fn parse() { parse(); }` is BOTH a definition and a reference of `parse`,
        // and the def name and the self-call share a line. The location must ground ONCE (at the
        // definition score), not as a duplicate def-row plus ref-row.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("r.rs"), "fn parse() { parse(); }\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());
        let refs = g.ground("parse", 8);
        let at_line_1: Vec<_> = refs
            .iter()
            .filter(|r| r.file == "r.rs" && r.line == 1)
            .collect();
        assert_eq!(
            at_line_1.len(),
            1,
            "the def+ref at one location must collapse to a single grounded ref; got {refs:?}"
        );
    }

    /// Spec 92 criterion 3 remediation round 5 (adj-u92c3-r4-verdict-reject,
    /// adv-u92c3r4-ground-ranked-def-sites-cross-language-absorption): `ground_ranked`'s
    /// entity-resolution (`def_sites`) must scope by `(name, Lang)`, never a bare name. JS
    /// defines the ONLY tree-wide definition of `unwrap`; five UNRELATED Rust files each
    /// reference that same bare name (mirroring Rust's own `.unwrap()` call sites, which are
    /// pure references with no local Rust definition). Before this fix, every Rust reference
    /// absorbed into the JS definition's entity (the sole bare-name def site) and collapsed to
    /// ONE row - erasing the entire Rust reference population rather than surfacing it as its
    /// own (unattributed) in-language entity, exactly the live `rigger ground unwrap 50` repro
    /// against this repo's own tree.
    #[test]
    fn ground_ranked_keeps_a_cross_language_reference_as_its_own_in_language_entity_not_absorbed_into_a_foreign_definition(
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shim.mjs"), "function unwrap() {}\n").unwrap();
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("r{i}.rs")),
                "fn go() { unwrap(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let ranked = g.ground_ranked("unwrap", 50);
        assert!(
            ranked.iter().any(|r| r.loc.file == "shim.mjs"),
            "the JS definition must still surface its own row; got {ranked:?}"
        );
        let rust_row = ranked
            .iter()
            .find(|r| r.loc.file.ends_with(".rs"))
            .unwrap_or_else(|| {
                panic!(
                    "the Rust reference population must surface its OWN row, never be absorbed \
                 into a same-named foreign-language definition; got {ranked:?}"
                )
            });
        assert_eq!(
            rust_row.degree, 5,
            "the Rust entity's degree must be its own in-language reference count (5), never \
             the JS definition's; got {ranked:?}"
        );
        assert_eq!(
            ranked.len(),
            2,
            "a same-named foreign-language definition must never collapse the Rust reference \
             population into the JS row - exactly 2 rows (the JS def, the Rust standalone \
             entity); got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3 remediation round 5 (adj-u92c3-r4-verdict-reject,
    /// adv-u92c3r4-ambiguity-map-bleeds-across-languages): `ambiguity_map` (and therefore
    /// `has_strong_match`'s cutoff) must scope by `(name, Lang)`, never a bare name.
    /// `collide` is defined EXACTLY ONCE in Rust (genuinely unambiguous within Rust alone) and,
    /// separately, EXACTLY ONCE in Python (also genuinely unambiguous within Python alone).
    /// Pooling the two languages' definition counts into one bare-name bucket manufactures a
    /// tree-wide ambiguity of 2 - an outlier against the nine Rust filler names' baseline of 1
    /// each - so the old bare-name-keyed cutoff wrongly read Rust's own unambiguous `collide` as
    /// tree-wide-common and answered "no entity matches strongly". Scoped per-language, Rust's
    /// own ambiguity distribution is nine names at 1 plus `collide` at 1: no outlier, so
    /// `collide` correctly reads as a confident, unambiguous match.
    #[test]
    fn has_strong_match_scopes_definition_ambiguity_by_language_a_cross_language_pool_never_taints_an_unambiguous_in_language_definition(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let filler: String = (0..9).map(|i| format!("fn filler{i}() {{}}\n")).collect();
        std::fs::write(dir.path().join("filler.rs"), filler).unwrap();
        std::fs::write(dir.path().join("combat.rs"), "fn collide() {}\n").unwrap();
        std::fs::write(dir.path().join("other.py"), "def collide():\n    pass\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        assert!(
            g.has_strong_match("collide", 8),
            "collide has exactly ONE definition within Rust and, separately, exactly one \
             within Python - each language's OWN ambiguity is 1, genuinely unambiguous. \
             Pooling both languages' definition counts into one bare-name bucket must never \
             manufacture a false tree-wide-ambiguous verdict for either language's own \
             genuinely unambiguous entity."
        );
    }

    #[test]
    fn empty_query_or_zero_k_grounds_nothing() {
        let (_dir, g) = grounder_over(&[("a.rs", "fn parse() {}\n")]);
        assert!(g.ground("", 5).is_empty());
        assert!(g.ground("parse", 0).is_empty());
    }

    /// Every read opens the view of the head afresh, so the grounder follows the tree as its files
    /// change and are deleted: a new symbol grounds, the replaced one is gone, an untouched file
    /// stands, and a deleted file stops grounding.
    #[test]
    fn grounding_follows_the_tree_as_files_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // Named so the OLD and NEW symbol share no substring relationship (spec 92 criterion 3's
        // CONTAINS-tier matching would otherwise still weakly ground "one" to a renamed
        // "oneprime" - correct recall, but the wrong reason for THIS assertion to pass).
        std::fs::write(dir.path().join("a.rs"), "fn alpha_orig() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn gamma_orig() {}\n").unwrap();
        let g = Symbols::in_directory(root);
        assert!(!g.ground("alpha_orig", 5).is_empty());

        std::fs::write(dir.path().join("a.rs"), "fn beta_fresh() {}\n").unwrap();
        assert!(
            !g.ground("beta_fresh", 5).is_empty(),
            "the new symbol must ground"
        );
        assert!(
            g.ground("alpha_orig", 5).is_empty(),
            "the replaced symbol must be gone"
        );
        assert!(
            !g.ground("gamma_orig", 5).is_empty(),
            "the untouched file must stand"
        );

        std::fs::remove_file(dir.path().join("a.rs")).unwrap();
        assert!(
            g.ground("beta_fresh", 5).is_empty(),
            "a deleted file's symbol must not keep grounding"
        );
        assert!(
            !g.ground("gamma_orig", 5).is_empty(),
            "the surviving file must still ground"
        );
    }

    /// Spec 92 criterion 3 (RANKED BY INTENT), the tier decision: "a query token that equals
    /// an entity's name beats a token that merely occurs in it." `run` EXACTLY names one
    /// definition; `run_all` only CONTAINS the token. Both are equally rare (each defined
    /// exactly once, nowhere else), so tier is the only thing that can decide the order - not
    /// commonness, not the def/ref lexical tier (both hits are definitions).
    #[test]
    fn ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token() {
        // Named so a plain file-order tiebreak (with tier ignored) would put the CONTAINS
        // match first - only real tier precedence can make the exact match win here.
        let (_dir, g) = grounder_over(&[
            ("zzz_exact.rs", "fn run() {}\n"),
            ("aaa_contains.rs", "fn run_all() {}\n"),
        ]);

        let refs = g.ground("run", 5);
        assert_eq!(
            refs[0].file, "zzz_exact.rs",
            "an exact-name match must outrank a name that merely contains the token; got {refs:?}"
        );
        assert_eq!(refs[0].text, "run");
    }

    /// Spec 92 criterion 3, the audit's own failure shape (docs/audit/2026-09-graph-vs-grep.md
    /// question 3: "dash run-awareness API routes" - `ground` returned "six `run` definitions
    /// in conductor.rs", ranking failed). SIX tree-wide `run` definitions must not drown out a
    /// rare, specific `dash` match, even though both share the SAME exact-match tier and the
    /// SAME definition lexical tier - only the inverse-document-frequency-style commonness
    /// weighting can tell them apart.
    #[test]
    fn ground_gives_a_rare_exact_match_priority_over_six_tree_wide_definitions_of_another_term() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(dir.path().join(format!("r{i}.rs")), "fn run() {}\n").unwrap();
        }
        // Named so a plain file-order tiebreak (with commonness ignored) would put a `run`
        // hit first - only real inverse-document-frequency weighting can make `dash` win.
        std::fs::write(dir.path().join("zzz_dash.rs"), "fn dash() {}\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let refs = g.ground("dash run", 10);
        assert_eq!(
            refs[0].text, "dash",
            "the rare token must rank above the six tree-wide `run` hits; got {refs:?}"
        );
        assert_eq!(refs[0].file, "zzz_dash.rs");
    }

    /// Spec 92 criterion 3 remediation round 2 (adv-u92c3r2-contains-tier-commonness-collapses
    /// -to-spurious-zero, UPHELD by review): `scored_hits`' CONTAINS-tier commonness must be the
    /// MATCHED ENTITY's OWN tree-wide occurrence count, never the raw query term's. A
    /// CONTAINS-tier term is by definition a substring, so it is almost never itself an indexed
    /// name; keying `commonness_map` on the term (instead of the entity it resolved to) silently
    /// collapsed every CONTAINS hit to the artificial rarest score (0), drowning a genuinely rare
    /// entity under unrelated ones that merely happen to share a common substring.
    ///
    /// `cfg_one`..`cfg_four` are four unrelated entities (each defined once, referenced twice -
    /// own commonness 3) sharing the substring "cfg", which is never itself a standalone name.
    /// `zorble_alone` (defined once, referenced nowhere - own commonness 1) is genuinely rarer.
    /// `zorble_alone`'s definition is placed LAST (the highest line number) in the fixture file
    /// deliberately: under the old bug every CONTAINS hit ties at commonness 0, so the sort falls
    /// through to line order and puts `zorble_alone` LAST, not first - only scoring commonness
    /// off the matched entity's own name can put the genuinely rare one first.
    #[test]
    fn ground_ranked_puts_a_genuinely_rare_contains_tier_entity_above_common_ones_sharing_its_substring(
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("entities.rs"),
            "fn cfg_one() {}\nfn cfg_two() {}\nfn cfg_three() {}\nfn cfg_four() {}\nfn zorble_alone() {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("callers.rs"),
            "fn go() {\n    cfg_one();\n    cfg_one();\n    cfg_two();\n    cfg_two();\n    cfg_three();\n    cfg_three();\n    cfg_four();\n    cfg_four();\n}\n",
        )
        .unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        // Neither "cfg" nor "zorble" is ever itself a standalone name in this fixture - every
        // match below is CONTAINS-tier, never EXACT, squarely exercising the scorer's
        // CONTAINS-tier commonness lookup.
        let ranked = g.ground_ranked("cfg zorble", 10);
        assert_eq!(
            ranked.len(),
            5,
            "five distinct entities must each collapse to one row; got {ranked:?}"
        );
        assert_eq!(
            ranked[0].loc.text, "zorble_alone",
            "the genuinely rare entity (own commonness 1) must outrank four entities that share \
             its query substring but are each themselves more common (own commonness 3) - CONTAINS \
             -tier commonness keyed on the raw query term ties all five at an artificial zero and \
             falls through to line order (which would rank zorble_alone LAST, by construction of \
             this fixture); got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3, "the top page is deduplicated by entity, so six call sites of one
    /// function occupy one row with its degree." A definition plus every one of its call
    /// sites (scattered across DIFFERENT files) must collapse to a SINGLE [`RankedRef`],
    /// carrying the real reference degree.
    #[test]
    fn ground_ranked_dedupes_a_functions_many_call_sites_into_one_row_with_its_degree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("def.rs"), "fn apply_damage() {}\n").unwrap();
        for i in 0..3 {
            std::fs::write(
                dir.path().join(format!("caller{i}.rs")),
                "fn go() { apply_damage(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let ranked = g.ground_ranked("apply_damage", 5);
        assert_eq!(
            ranked.len(),
            1,
            "the definition and its three call sites must collapse into ONE entity row; got {ranked:?}"
        );
        assert_eq!(ranked[0].loc.file, "def.rs");
        assert_eq!(
            ranked[0].degree, 3,
            "the row's degree must be the reference count across every call site; got {ranked:?}"
        );
    }

    /// Spec 92 constraints walk: "a name defined in two files - `ground` returns both entities
    /// as separate rows." The entity dedup is per DEFINITION, never per bare name - two
    /// distinct `run` definitions must not collapse into one just because they share a name.
    #[test]
    fn ground_ranked_keeps_two_definitions_of_the_same_name_as_separate_rows() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn run() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn run() {}\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let ranked = g.ground_ranked("run", 5);
        let files: Vec<&str> = ranked.iter().map(|r| r.loc.file.as_str()).collect();
        assert!(
            files.contains(&"a.rs") && files.contains(&"b.rs"),
            "two distinct definitions of the same name must both appear as separate rows; got {ranked:?}"
        );
        assert_eq!(
            ranked.len(),
            2,
            "no more than the two real definitions - never collapsed, never duplicated; got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3 remediation round 6 (adj-u92c3-r5-verdict-reject /
    /// adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): an AMBIGUOUS name's
    /// own Def rows must never claim the whole unattributed reference pool as their own
    /// degree. Two same-language definitions of one name, PLUS real call sites, reproduces the
    /// reject's own empirical repro (two Rust defs of `run` + five Rust refs -> three rows,
    /// every row printing the same pooled degree) - here with a fixture name so the assertion
    /// is exact and stable rather than depending on this crate's own real `run` population.
    /// Every prior test either used zero-reference ambiguous defs
    /// (`ground_ranked_keeps_two_definitions_of_the_same_name_as_separate_rows`) or an
    /// unambiguous single def WITH references
    /// (`ground_ranked_dedupes_a_functions_many_call_sites_into_one_row_with_its_degree`) -
    /// never an ambiguous def combined with real references, the exact combination the reject
    /// named as untested.
    #[test]
    fn ground_ranked_gives_ambiguous_definitions_zero_degree_and_pools_the_real_count_on_the_standalone_row(
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn dup_name() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn dup_name() {}\n").unwrap();
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("caller{i}.rs")),
                "fn go() { dup_name(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        let ranked = g.ground_ranked("dup_name", 10);
        assert_eq!(
            ranked.len(),
            3,
            "two ambiguous Def rows plus one pooled Standalone reference row; got {ranked:?}"
        );
        let def_rows: Vec<_> = ranked
            .iter()
            .filter(|r| r.loc.file == "a.rs" || r.loc.file == "b.rs")
            .collect();
        assert_eq!(
            def_rows.len(),
            2,
            "both ambiguous definitions must still appear as separate rows; got {ranked:?}"
        );
        for row in &def_rows {
            assert_eq!(
                row.degree, 0,
                "an ambiguous definition's own attributable degree is unknown - the code has \
                 already decided (by creating a Standalone row at all) that its references \
                 cannot be pinned to one candidate, so a Def row must never claim the whole \
                 unattributed pool as its own; got {ranked:?}"
            );
        }
        let standalone = ranked
            .iter()
            .find(|r| r.loc.file != "a.rs" && r.loc.file != "b.rs")
            .unwrap_or_else(|| {
                panic!("expected a pooled Standalone reference row; got {ranked:?}")
            });
        assert_eq!(
            standalone.degree, 5,
            "the Standalone row alone carries the real aggregate unattributed call-reference \
             count - genuinely different from the ambiguous Def rows' degree, never a shared \
             pooled number; got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3, "a query with no strong token returns the honest 'no entity
    /// matches strongly' line instead of noise." A query that only matches a tree-wide-common
    /// (AMBIGUOUS - many distinct definitions) token is not strong; a rare, specific match is;
    /// a query matching nothing at all is not strong either (it has no data to be noisy about,
    /// but it is not a confident hit). `run` is genuinely ambiguous - twelve UNRELATED
    /// definitions, so a match could mean any of them; `apply_damage` is defined exactly once.
    /// Reference VOLUME plays no part here (spec 92 criterion 3 remediation, the
    /// adj-u92c3-verdict-reject fix): a name referenced constantly but defined exactly once is
    /// NOT ambiguous, only a name with multiple candidate definitions is - see
    /// `has_strong_match_is_true_for_a_single_definition_referenced_many_times` below for that
    /// half of the contract.
    #[test]
    fn has_strong_match_is_false_when_every_matching_token_is_tree_wide_common() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..12 {
            std::fs::write(dir.path().join(format!("f{i}.rs")), "fn run() {}\n").unwrap();
        }
        std::fs::write(dir.path().join("combat.rs"), "fn apply_damage() {}\n").unwrap();
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        assert!(
            !g.has_strong_match("run", 8),
            "a query that only matches a name with many distinct (ambiguous) definitions must \
             not be a strong match"
        );
        assert!(
            g.has_strong_match("apply_damage", 8),
            "a rare, specific match is strong"
        );
        assert!(
            !g.has_strong_match("nonexistent_symbol_zzz", 8),
            "a query with no matches at all is not a strong match either"
        );
    }

    /// Spec 92 criterion 3 remediation (adj-u92c3-verdict-reject): the reject's own live repro
    /// against the real project tree - `criterion_stable_id` (1 definition, 37 references) and
    /// another single-definition entity (1 definition, 93 references) both wrongly printed "no
    /// entity matches strongly". A single-definition entity is UNAMBIGUOUS no matter how many
    /// places call it; the old cutoff conflated one entity's own reference VOLUME with tree-wide
    /// name AMBIGUITY (how many DISTINCT definitions share the name). This fixture mirrors that
    /// shape at a scale that clears `AMBIGUITY_PERCENTILE` under the OLD (broken) raw
    /// occurrence-count cutoff, proving the fix measures ambiguity, not popularity.
    #[test]
    fn has_strong_match_is_true_for_a_single_definition_referenced_many_times() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("def.rs"), "fn widely_called() {}\n").unwrap();
        for i in 0..40 {
            std::fs::write(
                dir.path().join(format!("caller{i}.rs")),
                "fn go() { widely_called(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::in_directory(dir.path().to_str().unwrap());

        assert!(
            g.has_strong_match("widely_called", 8),
            "a single-definition entity must read as strong regardless of how many places \
             reference it - reference volume is not ambiguity"
        );
    }

    /// Spec 92 criterion 3 remediation (adj-u92c3-verdict-reject, the CONTAINS-tier
    /// sentinel-inversion gap): a query term that is never ITSELF an indexed name (only a
    /// substring of one) must still read as strong when it resolves to a real, unambiguous
    /// entity - `has_strong_match` must judge the MATCHED ENTITY's ambiguity
    /// (`scored_hits`' resolved `name`), not do a second raw exact-key lookup on the query
    /// term itself, which finds nothing and used to silently read as "not strong".
    #[test]
    fn has_strong_match_is_true_for_a_contains_tier_match_of_an_unambiguous_entity() {
        let (_dir, g) = grounder_over(&[("combat.rs", "fn apply_damage() {}\n")]);

        assert!(
            g.has_strong_match("damage", 8),
            "\"damage\" only CONTAINS-matches the single-definition apply_damage; it must read \
             as strong, not silently fail through the honest no-match line"
        );
    }

    /// A stamp for a view built in memory.
    fn stamp() -> Stamp {
        Stamp {
            tip: Tip {
                head: "main".into(),
                commit: None,
                tree: "fixture-tree".into(),
            },
            fingerprint: 0,
            seq: 1,
        }
    }

    /// The head a test double hands out: the same files every read, compared with a worktree that
    /// differs as `diff` says. No file is on disk.
    struct Fixed {
        files: Vec<FileFacts>,
        diff: Result<WorktreeDiff, String>,
    }

    impl OpenCodeView for Fixed {
        fn head(&self, _worktree: &Path) -> Result<Head, String> {
            Ok(Head {
                view: Box::new(FactsView::new(stamp(), self.files.clone())),
                diff: self.diff.clone(),
                behind: None,
            })
        }
    }

    /// A fully-parsed code file at `path`, in language `lang` and `scope`.
    fn code_file(
        path: &str,
        lang: &str,
        scope: Scope,
        defs: Vec<Def>,
        refs: Vec<Mention>,
    ) -> FileFacts {
        FileFacts {
            path: path.into(),
            lang: lang.into(),
            scope,
            defs,
            mentions: refs,
            ..FileFacts::default()
        }
    }

    /// Test-only fixture: a one-line definition at `line`, real fields elsewhere immaterial to
    /// [`scored_hits`]'s tie-break (`test` at its inert default).
    fn hand_def(name: &str, line: u32) -> Def {
        Def {
            local: name.to_string(),
            leaf: name.to_string(),
            kind: "fn".to_string(),
            line,
            end_line: line,
            parent: None,
            test: false,
        }
    }

    /// Test-only fixture: a call reference at `line`, unattributed (`enclosing: None`) and not
    /// test evidence - the fields [`scored_hits`] itself never reads.
    fn hand_ref(name: &str, line: u32) -> Mention {
        Mention {
            enclosing: None,
            name: name.to_string(),
            leaf: name.to_string(),
            kind: MentionKind::Call,
            line,
            test: false,
        }
    }

    /// A view of `files`, compared with a worktree that differs from it nowhere.
    fn view_of(files: Vec<FileFacts>) -> FactsView {
        FactsView::new(stamp(), files)
    }

    fn agreeing(view: &FactsView) -> Sight<'_> {
        Sight::new(view, Withheld::of(Ok(WorktreeDiff::default())))
    }

    /// A one-file `fixture.rs` Rust view of `defs` and `refs`.
    fn fixture_view(defs: Vec<Def>, refs: Vec<Mention>) -> FactsView {
        view_of(vec![code_file(
            "fixture.rs",
            "rust",
            Scope::Product,
            defs,
            refs,
        )])
    }

    /// DEV-ONLY CODE MAKES NO NODE (spec 116 criterion 1), at ranking: [`scored_hits`] marks
    /// every location of a Test-scope file (`*_test.go` among them) as test, though none carries
    /// a test marking of its own, and leaves its production sibling's unmarked.
    #[test]
    fn scored_hits_marks_a_test_scope_file_location_as_test_and_its_sibling_as_production() {
        let view = view_of(
            [("pkg/x.go", Scope::Product), ("pkg/x_test.go", Scope::Test)]
                .map(|(path, scope)| {
                    code_file(
                        path,
                        "go",
                        scope,
                        vec![hand_def("Widget", 1)],
                        vec![hand_ref("Widget", 2)],
                    )
                })
                .into(),
        );
        let marked: Vec<(String, u32, bool)> = scored_hits(&agreeing(&view), &["Widget"])
            .into_iter()
            .map(|h| (h.file, h.line, h.test))
            .collect();
        assert_eq!(
            marked,
            vec![
                ("pkg/x.go".to_string(), 1, false),
                ("pkg/x.go".to_string(), 2, false),
                ("pkg/x_test.go".to_string(), 1, true),
                ("pkg/x_test.go".to_string(), 2, true),
            ]
        );
    }

    /// The name `hits` slots at `fixture.rs`'s `line`.
    fn slot_at(hits: &[ScoredHit], line: u32) -> Option<&str> {
        hits.iter()
            .find(|h| h.line == line && h.file == "fixture.rs")
            .map(|h| h.name.as_str())
    }

    /// Over a `fixture.rs` view of `defs` and `refs`, scoring `terms` slots each `(line, name,
    /// why)` case's `name` at its `line`.
    fn assert_slots(
        defs: Vec<Def>,
        refs: Vec<Mention>,
        terms: &[&str],
        expected: &[(u32, &str, &str)],
    ) {
        let view = fixture_view(defs, refs);
        let hits = scored_hits(&agreeing(&view), terms);
        for (line, name, why) in expected {
            let slot = slot_at(&hits, *line);
            assert_eq!(slot, Some(*name), "{why}; got {slot:?}");
        }
    }

    crate::test_cases! {
        /// [`scored_hits`]' own tie-break, `let better = ..` (grounder.rs, three clauses over one
        /// (file, line) location's accumulated best candidate): clause 1 is `hit_tier >
        /// slot.tier` - a STRICTLY higher tier always wins, no matter how the two candidates
        /// compare on commonness or lexical kind. Two hand-built candidates collide at the SAME
        /// (file, line) in each cluster below (the only way [`scored_hits`]'
        /// `best.entry(..).and_modify(..)` tie-break ever runs at all): a definition is always
        /// scored before any reference in the SAME file ([`scored_hits`]'s own `for d in &fs.defs
        /// { .. } for r in fs.calls() { .. }`), so the definition is always the initial slot
        /// (`or_insert`) and the reference is always the challenger (`and_modify`) whenever both
        /// match at one location.
        scored_hits_a_strictly_higher_tier_always_wins_over_a_worse_commonness_and_lexical: assert_slots(
            vec![
                // Cluster A (line 10): the SLOT is a CONTAINS-tier (1) definition that is
                // both COMMON (occurs 4x tree-wide, via the 3 extra refs below) and, being a
                // definition, lexically favored (3) - every OTHER clause would keep it. Only
                // the strictly-higher-tier challenger below can dislodge it.
                hand_def("walrus_partial_match_hook", 10),
                // Cluster B (line 20): the SLOT is an EXACT-tier (2) definition that is RARE
                // (occurs once). The challenger below matches the SAME tier but is common
                // (4x) - equal tier must NOT let a worse-commonness challenger win.
                hand_def("penguin_rare_def", 20),
            ],
            vec![
                // Cluster A's challenger: EXACT tier (2, via the term below), strictly
                // higher than the slot's CONTAINS tier (1) - must win despite its worse
                // (rarer-favoring-the-slot) commonness and worse (reference) lexical kind.
                hand_ref("narwhal_full_match_hook", 10),
                // Cluster B's challenger: SAME tier (2) as the slot, but common (4x) -
                // clause 1 alone must never promote it; the slot must stay.
                hand_ref("octopus_common_ref", 20),
                // Inflate "walrus_partial_match_hook"'s tree-wide commonness to 4 (the
                // definition above plus these three), so cluster A's slot is common, not
                // rare - proving the win is from tier alone, not incidental rarity.
                hand_ref("walrus_partial_match_hook", 200),
                hand_ref("walrus_partial_match_hook", 201),
                hand_ref("walrus_partial_match_hook", 202),
                // Inflate "octopus_common_ref" to 4 occurrences, so cluster B's challenger
                // is common (worse), not merely tied on commonness too.
                hand_ref("octopus_common_ref", 210),
                hand_ref("octopus_common_ref", 211),
                hand_ref("octopus_common_ref", 212),
            ],
            &[
                "partial_match",           // CONTAINS-tier term for the cluster-A slot
                "narwhal_full_match_hook", // EXACT-tier term for the cluster-A challenger
                "penguin_rare_def",        // EXACT-tier term for the cluster-B slot
                "octopus_common_ref",      // EXACT-tier term for the cluster-B challenger
            ],
            &[
                (
                    10,
                    "narwhal_full_match_hook",
                    "a strictly higher tier must win regardless of commonness/lexical",
                ),
                (
                    20,
                    "penguin_rare_def",
                    "an EQUAL tier must never be promoted by clause 1 alone, however common the \
             challenger; the rarer-and-already-slotted definition must stay",
                ),
            ],
        );
        /// Clause 2, `hit_tier == slot.tier && hit_commonness < slot.commonness`: at a TIED tier,
        /// the STRICTLY rarer candidate wins outright - even against a worse (reference) lexical
        /// kind - and a merely EQUAL commonness must never itself promote: there is no third clause
        /// falling back to [`ScoredHit::lexical`] within a slot (proven separately - the definition
        /// wins a same-tier, same-commonness tie by scoring order alone).
        scored_hits_breaks_a_tier_tie_by_strict_rarity_never_by_an_equal_commonness: assert_slots(
            vec![
                // Cluster C (line 30): the SLOT is a common (4x) definition; the challenger
                // below is a strictly RARER (1x) reference at the same tier - it must win
                // despite its worse lexical kind, proving commonness outranks lexical.
                hand_def("quokka_common_def", 30),
                // Cluster D (line 40): SLOT and challenger have EQUAL commonness (1x each)
                // at the same tier - clause 2's `<` must stay false on a tie, never `<=`.
                hand_def("echidna_equal_def", 40),
            ],
            vec![
                hand_ref("wombat_rare_ref", 30),
                hand_ref("platypus_equal_ref", 40),
                // Inflate "quokka_common_def" to 4 occurrences (common, worse).
                hand_ref("quokka_common_def", 220),
                hand_ref("quokka_common_def", 221),
                hand_ref("quokka_common_def", 222),
            ],
            &[
                "quokka_common_def",
                "wombat_rare_ref",
                "echidna_equal_def",
                "platypus_equal_ref",
            ],
            &[
                (
                    30,
                    "wombat_rare_ref",
                    "a strictly rarer same-tier challenger must win over a common slot, even as a \
             reference against a definition",
                ),
                (
                    40,
                    "echidna_equal_def",
                    "an EQUAL commonness must never itself promote a reference over an already-slotted, \
             same-tier definition",
                ),
            ],
        );
        /// A tied tier AND tied commonness never promotes the challenger, however their
        /// [`ScoredHit::lexical`] kinds compare - `better`'s two clauses stop at commonness, with no
        /// third clause falling back to `lexical` within a slot. Since every definition in a file is
        /// always scored before any reference in it, a definition can never challenge an
        /// already-slotted reference; only a same-tier, same-commonness REFERENCE can ever challenge
        /// an already-slotted DEFINITION (and does not promote it), or a second definition can
        /// challenge a first one at the SAME line (and does not promote it either) - the
        /// FIRST-inserted candidate always stays, by scoring order alone.
        scored_hits_lexical_never_promotes_a_tied_reference_or_a_tied_second_definition: assert_slots(
            vec![
                // Cluster E (line 50): TWO definitions at the same tier and commonness (1x
                // each) - "bandicoot" is scored first (Vec order) and must stay the slot;
                // `better`'s commonness clause must stay `<`, never `<=`, so an equal
                // second definition never promotes over the first.
                hand_def("bandicoot_alpha_def", 50),
                hand_def("dingo_beta_def", 50),
            ],
            vec![],
            &    ["bandicoot_alpha_def", "dingo_beta_def"],
            &[
                (
                    50,
                    "bandicoot_alpha_def",
                    "tied tier, tied commonness, tied (definition) lexical: the first-scored definition \
             must stay the slot",
                ),
            ],
        );
    }

    /// The files a worktree holds differently from the view's tree, as the honesty check reports
    /// them.
    fn differing(changed: &[&str], untracked: &[&str]) -> Result<WorktreeDiff, String> {
        let owned = |files: &[&str]| files.iter().map(|f| f.to_string()).collect();
        Ok(WorktreeDiff {
            changed: owned(changed),
            untracked: owned(untracked),
        })
    }

    /// Grounding reads the code store's view, not the disk: the files exist only in the view the
    /// grounder is handed, and what it serves is that view less the files the caller's worktree
    /// puts in doubt (a differing file; every file when the worktree cannot be compared). Each row:
    /// the worktree's difference, `ground`'s hits as `file:line:name`, and the blast radius's
    /// precise view.
    #[test]
    fn symbol_grounding_reads_the_code_view() {
        let files = vec![
            code_file(
                "src/combat.rs",
                "rust",
                Scope::Product,
                vec![hand_def("apply_damage", 7)],
                vec![],
            ),
            code_file(
                "src/caller.rs",
                "rust",
                Scope::Product,
                vec![hand_def("go", 1)],
                vec![hand_ref("apply_damage", 2)],
            ),
        ];
        let both_files = vec![
            "src/combat.rs:7:apply_damage",
            "src/caller.rs:2:apply_damage",
        ];
        let cases = [
            (
                "a worktree that agrees with the view",
                differing(&[], &[]),
                both_files.clone(),
                vec!["src/combat.rs", "src/caller.rs"],
            ),
            (
                "a file outside the tree's scope that differs withholds nothing",
                differing(&[], &[".rigger/events.db"]),
                both_files,
                vec!["src/combat.rs", "src/caller.rs"],
            ),
            (
                "a differing definer is withheld",
                differing(&["src/combat.rs"], &[]),
                vec!["src/caller.rs:2:apply_damage"],
                vec!["src/caller.rs"],
            ),
            (
                "a differing caller is withheld",
                differing(&[], &["src/caller.rs"]),
                vec!["src/combat.rs:7:apply_damage"],
                vec!["src/combat.rs"],
            ),
            (
                "a worktree that cannot be compared withholds every file",
                Err("the commit is not in the worktree".to_string()),
                vec![],
                vec![],
            ),
        ];
        for (name, diff, ground, precise) in cases {
            let worktree = tempfile::tempdir().unwrap();
            let g = Symbols::over(
                worktree.path().to_str().unwrap(),
                Box::new(Fixed {
                    files: files.clone(),
                    diff,
                }),
            );
            let hits: Vec<String> = g
                .ground("apply_damage", 8)
                .into_iter()
                .map(|r| format!("{}:{}:{}", r.file, r.line, r.text))
                .collect();
            assert_eq!(hits, ground, "{name}: ground");
            assert_eq!(
                g.blast_radius("`apply_damage`", 8).precise,
                precise,
                "{name}: blast radius"
            );
        }
    }

    /// A store that cannot be opened withholds everything, like a worktree that cannot be compared,
    /// and says why: nothing is served from the view, the radius falls back to the text search
    /// (the safe over-approximation), and the label names the failure. Never an empty answer
    /// indistinguishable from "no hit".
    #[test]
    fn grounding_answers_state_an_unopenable_store() {
        struct Unopenable;
        impl OpenCodeView for Unopenable {
            fn head(&self, _worktree: &Path) -> Result<Head, String> {
                Err("the code store is locked".to_string())
            }
        }
        let worktree = tempfile::tempdir().unwrap();
        std::fs::write(worktree.path().join("def.rs"), "fn apply_damage() {}\n").unwrap();
        std::fs::write(
            worktree.path().join("caller.rs"),
            "fn go() { apply_damage(); }\n",
        )
        .unwrap();
        let g = Symbols::over(worktree.path().to_str().unwrap(), Box::new(Unopenable));

        let Grounded {
            rows: hits, label, ..
        } = g.ground_labelled("apply_damage", 8, Tests::Include);
        assert!(hits.is_empty(), "nothing is served");
        let radius = g.blast_radius("`apply_damage`", 8);
        assert!(radius.precise.is_empty(), "no structural view");
        let mut safe = radius.safe;
        safe.sort();
        assert_eq!(
            safe,
            ["caller.rs", "def.rs"],
            "the text search answers in place of the view"
        );
        let label = label.expect("the symbols grounder labels its answers");
        assert_eq!(label.compared, Compared::Uncomparable);
        assert!(
            label.to_string().contains("the code store is locked"),
            "the label names why: {label}"
        );
    }

    /// Grounding rows follow the `tests` filter: `exclude` keeps product rows, `include` ranks
    /// every row (product first), `only` keeps the tests. Each row: the filter and the files of
    /// the rows it answers for `widget`, defined in `src/w.rs` and called by a test in
    /// `tests/t.rs`.
    #[test]
    fn grounding_rows_follow_the_tests_filter() {
        let worktree = tempfile::tempdir().unwrap();
        let files = vec![
            code_file(
                "src/w.rs",
                "rust",
                Scope::Product,
                vec![hand_def("widget", 1)],
                Vec::new(),
            ),
            code_file(
                "tests/t.rs",
                "rust",
                Scope::Test,
                vec![hand_def("widget_works", 1)],
                Vec::new(),
            ),
        ];
        let g = Symbols::over(
            worktree.path().to_str().unwrap(),
            Box::new(Fixed {
                files,
                diff: Ok(WorktreeDiff::default()),
            }),
        );
        let cases: [(Tests, &[&str]); 3] = [
            (Tests::Exclude, &["src/w.rs"]),
            (Tests::Include, &["src/w.rs", "tests/t.rs"]),
            (Tests::Only, &["tests/t.rs"]),
        ];
        for (tests, expected) in cases {
            let grounded = g.ground_labelled("widget", 8, tests);
            let rows = grounded.rows;
            let found: Vec<&str> = rows.iter().map(|r| r.file.as_str()).collect();
            assert_eq!(found, expected, "{tests:?}");
            let ranked = g.ground_ranked_labelled("widget", 8, tests);
            let page = ranked.rows.unwrap_or_default();
            let page: Vec<&str> = page.iter().map(|r| r.loc.file.as_str()).collect();
            assert_eq!(page, expected, "{tests:?}: the ranked page follows it");
            let hidden = usize::from(tests == Tests::Exclude);
            assert_eq!(grounded.tests_hidden, hidden, "{tests:?}");
            assert_eq!(
                ranked.tests_hidden, hidden,
                "{tests:?}: the ranked page counts it"
            );
        }
    }

    /// A store that advances on every open, naming the open's number in its tree id and its one
    /// file (beside one test file), so a second open shows as an answer from another tree and as
    /// a count above one.
    struct Advancing(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    impl OpenCodeView for Advancing {
        fn head(&self, _worktree: &Path) -> Result<Head, String> {
            let open = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let mut at = stamp();
            at.tip.tree = format!("tree-{open}");
            let file = code_file(
                &format!("open{open}.rs"),
                "rust",
                Scope::Product,
                vec![hand_def("apply_damage", 1)],
                vec![],
            );
            // One test location beside it, which the excluding answer counts, not lists.
            let test = code_file(
                &format!("open{open}_test.rs"),
                "rust",
                Scope::Test,
                vec![hand_def("apply_damage_works", 1)],
                vec![],
            );
            Ok(Head {
                view: Box::new(FactsView::new(at, vec![file, test])),
                diff: Ok(WorktreeDiff::default()),
                behind: None,
            })
        }
    }

    /// A labelled answer comes from ONE open of the store: the label describes the tree the rows
    /// came from even when the store advances between opens. The double advances on every open,
    /// naming the open's number in its tree id and its one file, so a second open shows as a
    /// label and rows from different trees, and as a count above one.
    #[test]
    fn a_labelled_answer_opens_the_store_once_and_describes_the_tree_it_read() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        type Answer = fn(&Symbols) -> (Vec<String>, Option<Label>, usize);
        let ground: Answer = |g| {
            let answer = g.ground_labelled("apply_damage", 8, Tests::Exclude);
            (
                answer.rows.into_iter().map(|r| r.file).collect(),
                answer.label,
                answer.tests_hidden,
            )
        };
        let ranked: Answer = |g| {
            let answer = g.ground_ranked_labelled("apply_damage", 8, Tests::Exclude);
            (
                answer
                    .rows
                    .unwrap_or_default()
                    .into_iter()
                    .map(|r| r.loc.file)
                    .collect(),
                answer.label,
                answer.tests_hidden,
            )
        };
        for (name, answer) in [
            ("ground_labelled", ground),
            ("ground_ranked_labelled", ranked),
        ] {
            let opens = Arc::new(AtomicUsize::new(0));
            let g = Symbols::over("W", Box::new(Advancing(opens.clone())));
            let (files, label, hidden) = answer(&g);
            assert_eq!(files, ["open1.rs"], "{name}: the rows read the first open");
            assert_eq!(hidden, 1, "{name}: the count comes from that open too");
            assert_eq!(
                label.expect("labelled").to_string(),
                "describes tree-1",
                "{name}: the label describes that same open"
            );
            assert_eq!(
                opens.load(Ordering::SeqCst),
                1,
                "{name}: one answer, one open"
            );
        }
    }

    /// The prompt seed, the radius and its audit stamp come from ONE open of the store: a store
    /// advanced between two opens never records a seed and a radius of different trees, or
    /// stamps either with a tree it was not grounded on.
    #[test]
    fn a_radius_is_stamped_with_the_tree_of_the_open_that_grounded_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let opens = Arc::new(AtomicUsize::new(0));
        let g = Symbols::over("W", Box::new(Advancing(opens.clone())));
        let stamped = g.blast_radius_stamped("`apply_damage`", 8);
        assert_eq!(stamped.seed, ["open1.rs"], "the seed reads the first open");
        assert!(
            stamped.radius.precise.contains(&"open1.rs".to_string()),
            "the radius reads the first open: {:?}",
            stamped.radius
        );
        assert_eq!(
            stamped.stamp,
            format!("tree-1/{GRAMMAR_TAGS_VERSION}"),
            "the stamp names that same open's tree"
        );
        assert_eq!(
            opens.load(Ordering::SeqCst),
            1,
            "one seed and radius, one open"
        );
    }

    /// Every answer's label states what was withheld, and counts the hits of that answer the
    /// differing files withheld: the entities it would have shown or reported hidden, a definition
    /// and its calls one, in the same read.
    /// The view holds `alpha` defined and called in `src/a.rs` and the test `alpha_test` in
    /// `tests/t.rs`; the query is `alpha`. Each row: the worktree's difference, the filter, the
    /// label's text after `describes <tree>` (the same on the plain and the ranked page) and the
    /// test hits reported hidden.
    #[test]
    fn grounding_labels_count_the_hits_they_withheld() {
        let files = vec![
            code_file(
                "src/a.rs",
                "rust",
                Scope::Product,
                vec![hand_def("alpha", 1)],
                vec![hand_ref("alpha", 5)],
            ),
            code_file(
                "tests/t.rs",
                "rust",
                Scope::Test,
                vec![hand_def("alpha_test", 1)],
                vec![],
            ),
        ];
        let a = &["src/a.rs"][..];
        let t = &["tests/t.rs"][..];
        let cases = [
            ("agreeing", differing(&[], &[]), Tests::Exclude, "", 1),
            (
                "the product file differs",
                differing(a, &[]),
                Tests::Exclude,
                "; 1 file in W differs; 1 withheld",
                1,
            ),
            (
                "an untracked differing file holds no hit",
                differing(a, &["src/b.rs"]),
                Tests::Include,
                "; 2 files in W differ; 1 withheld",
                0,
            ),
            (
                "a differing test file's hit is withheld, not hidden",
                differing(t, &[]),
                Tests::Exclude,
                "; 1 file in W differs; 1 withheld",
                0,
            ),
            (
                "a product hit the test view would not show is not counted",
                differing(a, &[]),
                Tests::Only,
                "; 1 file in W differs",
                0,
            ),
            (
                "the test view counts its withheld test hit",
                differing(t, &[]),
                Tests::Only,
                "; 1 file in W differs; 1 withheld",
                0,
            ),
            (
                "not comparable",
                Err("no such commit".to_string()),
                Tests::Include,
                "; W cannot be compared; 2 withheld",
                0,
            ),
        ];
        for (name, diff, tests, after_tree, hidden) in cases {
            let g = Symbols::over(
                "W",
                Box::new(Fixed {
                    files: files.clone(),
                    diff,
                }),
            );
            let expected = format!("describes {}{after_tree}", stamp().tip.tree);
            let plain = g.ground_labelled("alpha", 8, tests);
            assert_eq!(
                plain.label.expect("labelled").to_string(),
                expected,
                "{name}"
            );
            assert_eq!(plain.tests_hidden, hidden, "{name}: hidden");
            let ranked = g.ground_ranked_labelled("alpha", 8, tests);
            assert_eq!(
                ranked.label.expect("labelled").to_string(),
                expected,
                "{name}: ranked"
            );
            assert_eq!(ranked.tests_hidden, hidden, "{name}: ranked hidden");
        }
    }

    /// A file the worktree holds differently from the view is withheld from the structural view,
    /// so the safe view adds the files the text search finds each named symbol in among them: a
    /// missed reference is the unsafe error. Each row: the worktree's difference and the safe view
    /// of `spawn`, defined in the view's `def.rs`; the worktree's `edited.rs` calls it.
    #[test]
    fn a_differing_file_s_text_hits_join_the_safe_view() {
        let worktree = tempfile::tempdir().unwrap();
        std::fs::write(worktree.path().join("edited.rs"), "fn go() { spawn(); }\n").unwrap();
        let files = vec![code_file(
            "def.rs",
            "rust",
            Scope::Product,
            vec![hand_def("spawn", 1)],
            vec![],
        )];
        let cases = [
            (
                "a worktree that agrees with the view: a resolved name is not text-searched",
                differing(&[], &[]),
                vec!["def.rs"],
            ),
            (
                "a differing file that names the symbol joins the safe view",
                differing(&[], &["edited.rs"]),
                vec!["def.rs", "edited.rs"],
            ),
            (
                "a differing file that does not name it stays out",
                differing(&[], &["other.rs"]),
                vec!["def.rs"],
            ),
        ];
        for (name, diff, safe) in cases {
            let g = Symbols::over(
                worktree.path().to_str().unwrap(),
                Box::new(Fixed {
                    files: files.clone(),
                    diff,
                }),
            );
            let br = g.blast_radius("`spawn`", 8);
            assert_eq!(br.safe, safe, "{name}");
            assert_eq!(br.precise, vec!["def.rs"], "{name}: precise");
        }
    }

    /// The reference degree counts every served file's call references to a name within one
    /// language: a name called in many files reports them all, and a name over-linked in another
    /// language never inflates the degree (the cross-language collision per-language scoping
    /// exists to prevent, 5.5.2). Each row: the files, then (name, language, degree) cases.
    #[test]
    fn reference_degree_counts_call_references_within_one_language() {
        let called = |path: String, lang: &str, name: &str| {
            code_file(&path, lang, Scope::Product, vec![], vec![hand_ref(name, 1)])
        };
        let many = |lang: &str, ext: &str, name: &str, n: usize| -> Vec<FileFacts> {
            (0..n)
                .map(|i| called(format!("{lang}/f{i}.{ext}"), lang, name))
                .collect()
        };
        let mut shared_name = many("python", "py", "parse", 20);
        shared_name.push(called("a.rs".into(), "rust", "parse"));
        shared_name.extend(many("rust", "rs", "new", 10));
        let mut defined_and_called = many("rust", "rs", "new", 20);
        defined_and_called.push(code_file(
            "combat.rs",
            "rust",
            Scope::Product,
            vec![hand_def("apply_damage", 1)],
            vec![hand_ref("apply_damage", 2)],
        ));
        let cases = [
            (
                "a name called in many files",
                defined_and_called,
                vec![
                    ("new", "rust", 20),
                    ("apply_damage", "rust", 1),
                    ("absent", "rust", 0),
                ],
            ),
            (
                "a name over-linked in another language",
                shared_name,
                vec![
                    ("parse", "rust", 1),
                    ("parse", "python", 20),
                    ("new", "rust", 10),
                ],
            ),
        ];
        for (name, files, degrees) in cases {
            let view = view_of(files);
            let sight = agreeing(&view);
            for (leaf, lang, degree) in degrees {
                assert_eq!(sight.reference_degree(leaf, lang), degree, "{name}: {leaf}");
            }
        }
    }

    /// Spec 117 criterion 6: a type reference, a const read and a `use` move no count, rank or
    /// radius - [`Sight::reference_degree`], [`Sight::occurrences`], [`scored_hits`],
    /// [`Symbols::ground_ranked`] and [`Symbols::blast_radius`] count call references alone - over
    /// a view holding one of each beside a call.
    #[test]
    fn tagged_references_move_no_degree_commonness_rank_or_radius() {
        use crate::rust_definition_sources::{
            REFERENCE_KINDS_DEFINER as DEFINER, REFERENCE_KINDS_TREE, REFERENCE_KINDS_USER as USER,
        };
        let files: Vec<FileFacts> = REFERENCE_KINDS_TREE
            .iter()
            .map(|(path, source)| {
                let entry = Entry {
                    path: path.to_string(),
                    blob: format!("blob-of-{path}"),
                };
                FactsExtractor.extract(&entry, source.as_bytes())
            })
            .collect();
        let user = files.iter().find(|f| f.path == USER).unwrap();
        let mut tagged: Vec<(u32, &str, String)> = user
            .mentions
            .iter()
            .filter_map(|m| match &m.kind {
                MentionKind::Tagged(kind) => Some((m, kind)),
                MentionKind::Call => None,
            })
            .map(|(m, kind)| {
                let caller = m
                    .enclosing
                    .map_or("-", |d| user.defs[d as usize].local.as_str());
                (
                    m.line,
                    m.name.as_str(),
                    format!("{} {kind} {} {caller}", m.line, m.name),
                )
            })
            .collect();
        tagged.sort();
        let tagged: Vec<&str> = tagged.iter().map(|(_, _, row)| row.as_str()).collect();
        assert_eq!(
            tagged.join("\n"),
            crate::rust_definition_sources::REFERENCE_KINDS_USER_TAGGED
        );
        let names = ["Kestrel", "QUOTA_CAP", "draw"];
        let view = view_of(files.clone());
        let sight = agreeing(&view);
        assert_eq!(names.map(|n| sight.reference_degree(n, "rust")), [0, 0, 1]);
        assert_eq!(names.map(|n| sight.occurrences(n, "rust", true)), [1, 1, 2]);
        let mut hits: Vec<(String, u32)> = scored_hits(&sight, &names)
            .into_iter()
            .map(|h| (h.file, h.line))
            .collect();
        hits.sort();
        assert_eq!(
            hits,
            [
                (DEFINER.to_string(), 1),
                (DEFINER.to_string(), 2),
                (DEFINER.to_string(), 3),
                (USER.to_string(), 2)
            ]
        );
        let g = Symbols::over(
            "",
            Box::new(Fixed {
                files,
                diff: Ok(WorktreeDiff::default()),
            }),
        );
        let ranked: Vec<(String, u32, usize)> = g
            .ground_ranked("Kestrel draw", 8)
            .into_iter()
            .map(|r| (r.loc.file, r.loc.line, r.degree))
            .collect();
        assert_eq!(ranked, [(DEFINER.into(), 1, 0), (DEFINER.into(), 3, 1)]);
        for (query, radius) in [
            ("`Kestrel` `QUOTA_CAP`", &[DEFINER][..]),
            ("`draw`", &[DEFINER, USER][..]),
        ] {
            let br = g.blast_radius(query, 8);
            let radius: Vec<String> = radius.iter().map(|f| f.to_string()).collect();
            assert_eq!((br.precise, br.safe), (radius.clone(), radius), "{query}");
        }
    }
}
