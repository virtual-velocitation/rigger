//! The grounding port (workspace split): the `Grounder` trait every grounder implements and the
//! plain values it returns - a location, a ranked entity row and the two-view blast radius. It
//! names only these values; the grounders live in `rigger-grounder`, and the root `rigger` crate
//! re-exports every item under its historical `rigger::grounder` path.

/// A relevant location: a file, a line, and a snippet.
#[derive(Clone, Debug)]
pub struct Ref {
    pub file: String,
    pub line: u32,
    pub text: String,
}

/// One ranked, entity-deduplicated row of the RANKED-BY-INTENT page (spec 92 criterion 3): the
/// entity's best-ranked location, plus `degree` - how many locations across the tree define or
/// reference it, so "six call sites of one function occupy one row with its degree" (the
/// Design decision) is a single [`RankedRef`] rather than six separate [`Ref`]s. `degree` is 0
/// for a non-structural grounder (grep / nop), which has no reference graph to count; only the
/// `symbols` grounder computes a real degree. `degree` is ALSO 0 for a row that is one of
/// several AMBIGUOUS (multiple same-language) definitions of one name (spec 92 criterion 3
/// remediation round 6, adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): its
/// own attributable reference count is unknown by construction, since the grounder has already
/// decided (by emitting a separate Standalone row for the same name) that no reference can be
/// pinned to any one of the several candidates - that unattributed pool belongs to the
/// Standalone row alone, never duplicated onto every ambiguous Def row.
#[derive(Clone, Debug)]
pub struct RankedRef {
    pub loc: Ref,
    pub degree: usize,
}

/// The two-view blast radius of a query (architecture 5.5.1, spec 16 unit 1). Blast-radius has
/// OPPOSITE error costs for its two consumers, so it delivers TWO views over the same query:
///
/// - `precise` - the ranked, capped view (definers ranked above referencers) that seeds an
///   agent's prompt. A spurious extra file here merely wastes a little context, so precision
///   is what it optimizes for.
/// - `safe` - the uncapped SAFE view the conductor schedules and routes review tiers by; what it
///   holds is stated once, in `docs/architecture-addendum-context-management.md` section 2.4.
///   [`radii_conflict`] keeps two units apart only when their file sets SHARE a file, so a MISSED
///   reference could co-schedule two conflicting units; over-inclusion is the safe error. A HUB
///   symbol (a name referenced across much of the tree) needs no flag of its own: its whole
///   neighborhood is in `safe`, so the overlap test keeps it apart from exactly the units that
///   share one of those files.
///
/// [`radii_conflict`]: crate::metrics::radii_conflict
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlastRadius {
    /// The precise / ranked view: definer files first, then referencer files, capped at `k`.
    pub precise: Vec<String>,
    /// The safe view, uncapped and always holding `precise` (addendum 2.4 states the rest).
    pub safe: Vec<String>,
}

/// A unit's prompt seed and blast radius with the provenance stamp of the one read that grounded
/// both ([`Grounder::blast_radius_stamped`]); an empty stamp marks a grounder with no structural
/// index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StampedRadius {
    /// The prompt seed ([`Grounder::seed`]) of the read that grounded `radius`.
    pub seed: Vec<String>,
    pub radius: BlastRadius,
    pub stamp: String,
}

/// Grounder returns up to k locations relevant to a query.
/// A grounder's answer: the rows, the honesty label they travel with, and how many test hits the
/// `tests` filter left out of them, all from one read of the code store so they describe one tree.
/// The count is none unless the filter excludes tests, and for a grounder that cannot tell test
/// code from product code.
#[derive(Clone, Debug, PartialEq)]
pub struct Grounded<T> {
    pub rows: T,
    pub label: Option<crate::composed::Label>,
    pub tests_hidden: usize,
}

pub trait Grounder: Send + Sync {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref>;

    /// The prompt seed of `query`: at most `k` distinct files an agent is grounded on. The
    /// DEFAULT - the one a grep / nop grounder inherits - is the distinct files of its own
    /// `ground(query, k)`, in ground order. The `symbols` grounder overrides it to start from the
    /// code the query names, resolved to the files that define it.
    fn seed(&self, query: &str, k: usize) -> Vec<String> {
        distinct_files(self.ground(query, k))
    }

    /// The two-view blast radius of `query` (architecture 5.5.1, spec 16). The DEFAULT impl - the
    /// one a grep / nop grounder inherits - returns this grounder's OWN top-`k` radius
    /// (the distinct files it grounds, in ground order) as BOTH views. So a non-symbols grounder's
    /// blast radius is EXACTLY its grep/top-k radius: `precise == safe`, no extra work. This is what keeps unit 3's symbols-inactive `grounded_seed`
    /// (which reads `precise`) byte-for-byte unchanged - it is the same `ground(query, k)` file set
    /// it produces today. Only the `symbols` grounder overrides this to union the structural
    /// cross-reference graph with an uncapped grep.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        let files = distinct_files(self.ground(query, k));
        BlastRadius {
            precise: files.clone(),
            safe: files,
        }
    }

    /// [`Self::seed`] and [`Self::blast_radius`] with the provenance stamp of the read that
    /// grounded them, for the `BlastRadiusComputed` audit event (spec 16 unit 3, architecture
    /// 5.5.9): the tree id + grammar / tag-query version that produced THIS radius, so a recorded
    /// radius reconstructs which tree grounded it and staleness is answerable ("why the full
    /// panel?"). The seed, the radius and the stamp come from one read, never several, so a store
    /// advanced between two reads cannot record a seed and a radius of different trees, or stamp
    /// either with a tree it was not grounded on. A non-empty stamp is ALSO the
    /// structural-active signal the conductor keys the audit off: the DEFAULT (grep / nop - no
    /// structural cross-reference index) stamps EMPTY, so the conductor emits NO audit event and
    /// drives NO retention metric on that path. Only the `symbols` grounder overrides this to a
    /// non-empty `<tree-id>/<grammar-tags-version>` stamp.
    fn blast_radius_stamped(&self, query: &str, k: usize) -> StampedRadius {
        StampedRadius {
            seed: self.seed(query, k),
            radius: self.blast_radius(query, k),
            stamp: String::new(),
        }
    }

    /// [`Self::ground`] with the honesty label (spec 6.1) its answer travels with: the tree the
    /// rows describe and what the caller's worktree puts in doubt, or why nothing could be read.
    /// A grounder that answers from the code store's view of the head overrides it to read that
    /// view ONCE, so the label describes exactly the tree the rows came from. The DEFAULT
    /// (grep / nop) reads the worktree itself, so it has nothing to state.
    ///
    /// `tests` filters the rows by test-ness; a grounder that cannot tell test code from product
    /// code (the DEFAULT) answers all rows whatever it is.
    fn ground_labelled(
        &self,
        query: &str,
        k: usize,
        tests: crate::code_view::Tests,
    ) -> Grounded<Vec<Ref>> {
        let _ = tests;
        Grounded {
            rows: self.ground(query, k),
            label: None,
            tests_hidden: 0,
        }
    }

    /// The ranked page `rigger ground` prints with its label: `None` rows when `k > 0` and no
    /// entity matches strongly ([`Self::has_strong_match`]), else [`Self::ground_ranked`]'s page.
    /// Overridden, like [`Self::ground_labelled`], to read the view once.
    fn ground_ranked_labelled(
        &self,
        query: &str,
        k: usize,
        tests: crate::code_view::Tests,
    ) -> Grounded<Option<Vec<RankedRef>>> {
        let _ = tests;
        let rows =
            (k == 0 || self.has_strong_match(query, k)).then(|| self.ground_ranked(query, k));
        Grounded {
            rows,
            label: None,
            tests_hidden: 0,
        }
    }

    /// The RANKED-BY-INTENT page (spec 92 criterion 3, "the top page is deduplicated by
    /// entity"): up to `k` DISTINCT entities for `query`, each the entity's best-ranked
    /// location plus its degree. The DEFAULT (grep / nop, and any grounder with no
    /// entity/reference concept) is simply `ground`'s own top-`k` rows, each wrapped with
    /// degree 0 (unknown) and NO dedup - never a silent behavior change for a non-structural
    /// grounder (mirroring the [`Self::blast_radius`] / [`Self::blast_radius_stamped`] default
    /// pattern). Only the `symbols` grounder overrides this with real ranking, entity dedup,
    /// and a real degree.
    fn ground_ranked(&self, query: &str, k: usize) -> Vec<RankedRef> {
        self.ground(query, k)
            .into_iter()
            .map(|loc| RankedRef { loc, degree: 0 })
            .collect()
    }

    /// Whether `query` has at least one matching entity that is NOT tree-wide-common (spec 92
    /// criterion 3, "a query with no strong token returns the honest 'no entity matches
    /// strongly' line instead of noise") - the gate a caller checks BEFORE printing
    /// [`Self::ground_ranked`]'s page. The DEFAULT (grep / nop) has no commonness concept, so
    /// every query is trivially "strong" - never suppressed, matching today's behavior. Only
    /// the `symbols` grounder overrides this with the real repo-relative commonness check.
    fn has_strong_match(&self, _query: &str, _k: usize) -> bool {
        true
    }
}

/// The distinct files of `refs`, in their order.
fn distinct_files(refs: Vec<Ref>) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for r in refs {
        if !files.contains(&r.file) {
            files.push(r.file);
        }
    }
    files
}
