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

/// Grounder returns up to k locations relevant to a query.
pub trait Grounder: Send + Sync {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref>;

    /// Re-index the given files after a unit integrates, so the next agent grounds
    /// on the accepted code (the `symbols` grounder freshens the changed files). The
    /// default is a no-op - grep re-reads the tree each time and needs no index.
    fn reindex(&self, _src_dir: &str, _files: &[String]) {}

    /// The two-view blast radius of `query` (architecture 5.5.1, spec 16). The DEFAULT impl - the
    /// one a grep / nop grounder inherits - returns this grounder's OWN top-`k` radius
    /// (the distinct files it grounds, in ground order) as BOTH views. So a non-symbols grounder's
    /// blast radius is EXACTLY its grep/top-k radius: `precise == safe`, no extra work. This is what keeps unit 3's symbols-inactive `grounded_seed`
    /// (which reads `precise`) byte-for-byte unchanged - it is the same `ground(query, k)` file set
    /// it produces today. Only the `symbols` grounder overrides this to union the structural
    /// cross-reference graph with an uncapped grep.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        let mut files: Vec<String> = Vec::new();
        for r in self.ground(query, k) {
            if !files.contains(&r.file) {
                files.push(r.file);
            }
        }
        BlastRadius {
            precise: files.clone(),
            safe: files,
        }
    }

    /// A provenance stamp for the `BlastRadiusComputed` audit event (spec 16 unit 3,
    /// architecture 5.5.9): the index content-hash + grammar / tag-query version that
    /// produced this grounder's radii, so a recorded radius reconstructs which index state
    /// grounded it and staleness is answerable ("why the full panel?"). It is ALSO the
    /// structural-active signal unit 3's conductor keys the audit off: the DEFAULT (grep /
    /// nop - no structural cross-reference index) returns an EMPTY stamp, so the
    /// conductor emits NO audit event and drives NO retention metric on that path, keeping the
    /// shipped default byte-for-byte unchanged. Only the `symbols` grounder overrides this to a
    /// non-empty `<index-content-hash>/<grammar-tags-version>` stamp.
    fn index_stamp(&self) -> String {
        String::new()
    }

    /// The RANKED-BY-INTENT page (spec 92 criterion 3, "the top page is deduplicated by
    /// entity"): up to `k` DISTINCT entities for `query`, each the entity's best-ranked
    /// location plus its degree. The DEFAULT (grep / nop, and any grounder with no
    /// entity/reference concept) is simply `ground`'s own top-`k` rows, each wrapped with
    /// degree 0 (unknown) and NO dedup - never a silent behavior change for a non-structural
    /// grounder (mirroring the [`Self::blast_radius`] / [`Self::index_stamp`] default
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
