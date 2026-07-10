//! Spec 16 unit 2 - the partitioning + routing SAFETY EVAL, the quantified go/no-go gate
//! that authorizes unit 3 wiring `blast_radius` into the conductor (architecture 5.5.8).
//!
//! This is a GATE, not a shipped surface: the whole module lives under `#[cfg(test)]` (it is
//! declared `#[cfg(test)] mod` in `lib.rs`). It adds NO runtime API and NO new event - the
//! RUNTIME wave-level parallelism-retention warn-metric derivable from `BlastRadiusComputed`
//! is unit 3's; unit 2 owns only the PRE-SHIP validation and its red-on-regression assertions,
//! so `grounded_seed` and every production surface stay byte-for-byte unchanged.
//!
//! Two arms with different jobs (5.5.8):
//!
//! - Arm (a), the IMPLEMENTATION-INVARIANT guard: on an adversarial corpus (macro, trait
//!   object, re-export, reflection, common-name) the safe view MUST be a superset of grep. It
//!   is a REGRESSION guard, not a discovery mechanism: because `safe = structural ∪ grep`
//!   (5.5.1) it can only go RED if the union is built WRONG (it intersects, or drops the grep
//!   side). The corpus deliberately carries grep-only mentions (a reflection string a symbol
//!   index never indexes, a macro body) so a `safe = structural` or `safe = structural ∩ grep`
//!   mutation actually drops a file and trips the superset check.
//!
//! - Arm (b), the real QUANTIFIED go/no-go on a PINNED polyglot repo: a parallelism-retention
//!   gate (units stay co-schedulable versus the grep baseline; median safe-radius bounded) and
//!   a tier-routing-distribution check (the light/full split under symbols must not collapse to
//!   all-full). Both bounds are MEASURED on the pinned fixture and FROZEN as red-on-regression
//!   assertions rather than hand-chosen (adv-quant-bound-unpinned).
//!
//! The co-scheduling measure REUSES [`crate::conductor::partition_by_blast_radius`] - the ONE
//! partitioner authority the run itself uses - for the file-set grouping, so THAT dimension is
//! the ACTUAL partition, not a parallel re-implementation. The serialize->own-singleton-batch
//! dimension, by contrast, IS a model of unit 3's not-yet-written wiring (see [`partition`]): the
//! production `partition_by_blast_radius` has no serialize concept, so this gate holds unit 3 to
//! wiring `serialize` with EXACTLY these own-singleton-batch semantics - it stays valid only if
//! unit 3 does. It deliberately does NOT drive the production `route_review_tier`: that
//! router's size `threshold` is the un-retuned `8`, and against an UNCAPPED safe radius every
//! unit would clear it and the split would collapse to all-full - a deadlock that is unit-3's
//! re-tune to fix (adv-tier-threshold-coupling). The tier arm instead models a
//! width-distribution-tuned threshold to prove such a split is FEASIBLE.

// Only the ONE partitioner authority is needed by the always-compiled metric core. The
// symbols-specific imports (`Grep`, `Grounder`, `BlastRadius`, `HashSet`, the `GROUND_K` cap
// and `safe_superset_violations`) live in the feature-gated `corpus_gates` module below, so the
// symbols-OFF lane carries no dead code.
use crate::conductor::partition_by_blast_radius;

/// One unit's blast radius for the eval: its id and the file set partitioning keys on.
#[derive(Clone)]
struct UnitRadius {
    unit: String,
    /// The safe file set (what the conductor partitions and routes tiers by).
    files: Vec<String>,
    /// Whether this radius must SERIALIZE (conflict-with-everything) because its query is a hub
    /// (unit 1's `BlastRadius::serialize`). The grep baseline never serializes; the symbols
    /// subject does when a queried name is a hub.
    serialize: bool,
}

/// The units co-schedulable under a partition: those landing in a batch with at least one peer
/// (batch size >= 2). A singleton batch is a serialized / unshared unit and contributes zero.
fn co_schedulable(batches: &[Vec<String>]) -> usize {
    batches
        .iter()
        .filter(|b| b.len() >= 2)
        .map(|b| b.len())
        .sum()
}

/// Partition the units the way the conductor will once unit 3 honors the two-view contract: a
/// SERIALIZE radius (a hub) takes its OWN batch (conflict-with-everything), and every other
/// radius is grouped by file-set disjointness through the ONE existing
/// [`partition_by_blast_radius`] authority. This is the eval's MODEL of the partition (unit 3
/// owns the wiring); it re-implements nothing - the file-set grouping is the production
/// partitioner, and the serialize->own-batch rule is unit 1's `serialize` contract verbatim.
fn partition(units: &[UnitRadius]) -> Vec<Vec<String>> {
    let shareable: Vec<(String, Vec<String>)> = units
        .iter()
        .filter(|u| !u.serialize)
        .map(|u| (u.unit.clone(), u.files.clone()))
        .collect();
    let mut batches = partition_by_blast_radius(&shareable);
    // Each serialized (hub) unit conflicts with everything: it never co-schedules, so it lands
    // in its own singleton batch rather than joining the shareable partition.
    for u in units.iter().filter(|u| u.serialize) {
        batches.push(vec![u.unit.clone()]);
    }
    batches
}

/// Parallelism retention of the `subject` partition versus the `baseline`: the share of the
/// baseline's co-schedulable units that STAY co-schedulable under the subject. `1.0` when the
/// baseline co-schedules nothing (there was no parallelism to retain), so the ratio is never a
/// divide-by-zero and a corpus that cannot parallelize under grep cannot manufacture a
/// spurious pass.
fn parallelism_retention(subject: &[UnitRadius], baseline: &[UnitRadius]) -> f64 {
    let base = co_schedulable(&partition(baseline));
    if base == 0 {
        return 1.0;
    }
    co_schedulable(&partition(subject)) as f64 / base as f64
}

/// The median file-count over the radii - the integer LOWER median (element `(n-1)/2` of the
/// sorted widths), a deterministic median with no float averaging. Lower is more partitionable.
fn median_width(radii: &[UnitRadius]) -> usize {
    let mut widths: Vec<usize> = radii.iter().map(|u| u.files.len()).collect();
    widths.sort_unstable();
    if widths.is_empty() {
        0
    } else {
        widths[(widths.len() - 1) / 2]
    }
}

/// A tier size `threshold` RE-TUNED to a width distribution: the nearest-rank `percentile` value
/// of the sorted widths (the degree at 0-based rank `floor(N * percentile)`, clamped to the top).
/// This is the TIER-ROUTING split point over the radius-WIDTH distribution - a distinct concern
/// from `model::is_hub`, which detects a strict high-degree OUTLIER over the reference-DEGREE
/// distribution (`floor((N - 1) * percentile)` with a strict `>`). The two used to share the
/// `floor(N * percentile)` rule, but hub detection was retuned to an outlier definition so it does
/// not degenerate on a long tail; this tier threshold keeps the nearest-rank percentile because it
/// wants a split POINT, not outlier detection. Unit 3 owns the PRODUCTION re-tune; unit 2 uses this
/// only to demonstrate that a distribution-tuned threshold keeps the light/full split non-degenerate
/// where the un-retuned absolute `8` collapses it.
fn width_threshold(widths: &[usize], percentile: f64) -> usize {
    if widths.is_empty() {
        return 0;
    }
    let mut w = widths.to_vec();
    w.sort_unstable();
    let idx = ((w.len() as f64) * percentile).floor() as usize;
    w[idx.min(w.len() - 1)]
}

/// The share of units the SIZE signal alone routes to the FULL panel: widths strictly greater
/// than `threshold`. `1.0` is the collapse-to-all-full the re-tune exists to prevent; `0.0` is
/// an all-light size signal.
fn full_fraction(widths: &[usize], threshold: usize) -> f64 {
    if widths.is_empty() {
        return 0.0;
    }
    let full = widths.iter().filter(|&&w| w > threshold).count();
    full as f64 / widths.len() as f64
}

#[cfg(test)]
mod pure_metric_tests {
    //! Direct unit tests of the eval's pure metric logic - grounder-agnostic, so they run in
    //! BOTH feature lanes (no `symbols` feature, no tree-sitter). They pin the metrics on
    //! synthetic radii so the corpus arms below rest on measured-correct primitives.
    use super::*;

    fn u(unit: &str, files: &[&str], serialize: bool) -> UnitRadius {
        UnitRadius {
            unit: unit.to_string(),
            files: files.iter().map(|s| s.to_string()).collect(),
            serialize,
        }
    }

    #[test]
    fn co_schedulable_counts_only_units_with_a_peer() {
        // One batch of 3 (all disjoint) => 3 co-schedulable; a singleton contributes 0.
        let disjoint = [
            u("a", &["a.rs"], false),
            u("b", &["b.rs"], false),
            u("c", &["c.rs"], false),
        ];
        assert_eq!(co_schedulable(&partition(&disjoint)), 3);
        // Two units sharing a file cannot co-schedule: they split into two singleton batches.
        let overlap = [u("a", &["x.rs"], false), u("b", &["x.rs"], false)];
        assert_eq!(co_schedulable(&partition(&overlap)), 0);
    }

    #[test]
    fn a_serialized_hub_takes_its_own_batch_and_never_co_schedules() {
        // Three disjoint units co-schedule (3); flip one to serialize and it drops to its own
        // batch, so only the remaining two co-schedule.
        let mut units = [
            u("a", &["a.rs"], false),
            u("b", &["b.rs"], false),
            u("hub", &["h.rs"], false),
        ];
        assert_eq!(co_schedulable(&partition(&units)), 3);
        units[2].serialize = true;
        assert_eq!(
            co_schedulable(&partition(&units)),
            2,
            "a serialized hub must not co-schedule; the other two still do"
        );
    }

    #[test]
    fn parallelism_retention_is_the_share_of_baseline_parallelism_kept() {
        // Baseline: three disjoint units, none serialized => co_schedulable 3.
        let baseline = [
            u("a", &["a.rs"], false),
            u("b", &["b.rs"], false),
            u("c", &["c.rs"], false),
        ];
        // Subject: same file sets but one serializes => co_schedulable 2 => retention 2/3.
        let subject = [
            u("a", &["a.rs"], false),
            u("b", &["b.rs"], false),
            u("c", &["c.rs"], true),
        ];
        let r = parallelism_retention(&subject, &baseline);
        assert!(
            (r - 2.0 / 3.0).abs() < 1e-9,
            "retention must be 2/3, got {r}"
        );
        // Identical partitions retain everything.
        assert!((parallelism_retention(&baseline, &baseline) - 1.0).abs() < 1e-9);
        // An empty baseline (no parallelism to retain) is 1.0, never a divide-by-zero.
        assert_eq!(parallelism_retention(&subject, &[]), 1.0);
    }

    #[test]
    fn width_threshold_is_the_tier_width_nearest_rank_percentile() {
        // The tier-routing width split point is nearest-rank floor(N * percentile): [1, 20] at the
        // 90th percentile picks 20 (floor(2 * 0.9) = 1 => index 1), not 1. This is deliberately a
        // DIFFERENT rule than model::is_hub's outlier cutoff (which uses floor((N - 1) * percentile)
        // with a strict `>` so it does not degenerate on a long tail); width_threshold wants a split
        // POINT over the width distribution, not high-degree outlier detection.
        assert_eq!(width_threshold(&[1, 20], 0.90), 20);
        // Order-independent (it sorts).
        assert_eq!(width_threshold(&[20, 1], 0.90), 20);
        // width_threshold is nearest-rank floor(n*p): index floor(6*0.5)=3 of the sorted spread
        // is 12, the UPPER-middle element (11 is the lower-middle). This is deliberately a
        // DIFFERENT median than median_width's lower median (n-1)/2=11; arm_b pins BOTH (tuned
        // threshold 12 AND median radius 11), so the two formulas must NOT be reconciled.
        assert_eq!(width_threshold(&[9, 10, 11, 12, 13, 15], 0.50), 12);
        assert_eq!(width_threshold(&[], 0.90), 0);
    }

    #[test]
    fn full_fraction_spans_all_light_to_collapse() {
        // Everything over the threshold => collapse to all-full (1.0).
        assert_eq!(full_fraction(&[9, 10, 11], 8), 1.0);
        // Nothing over => all-light (0.0). The comparison is STRICTLY greater, so ties are light.
        assert_eq!(full_fraction(&[8, 8, 8], 8), 0.0);
        // A genuine split.
        assert_eq!(full_fraction(&[9, 10, 11, 12, 13, 15], 12), 2.0 / 6.0);
        assert_eq!(full_fraction(&[], 8), 0.0);
    }

    #[test]
    fn median_width_is_the_deterministic_lower_median() {
        assert_eq!(median_width(&[u("a", &["a.rs", "b.rs"], false)]), 2);
        assert_eq!(
            median_width(&[
                u("a", &["1"], false),
                u("b", &["1", "2", "3"], false),
                u("c", &["1", "2"], false),
            ]),
            2
        );
        // Even count: the LOWER median (n-1)/2 picks the lower of the two middles. For widths
        // [1,2,3,4] that is index 1 => 2, where the UPPER median n/2 would be index 2 => 3. This
        // pins the lower-vs-upper choice in BOTH feature lanes: a `(n-1)/2 -> n/2` mutation trips
        // HERE (a grounder-agnostic pure-metric test), not only in the symbols-gated arm_b.
        assert_eq!(
            median_width(&[
                u("a", &["1"], false),
                u("b", &["1", "2"], false),
                u("c", &["1", "2", "3"], false),
                u("d", &["1", "2", "3", "4"], false),
            ]),
            2
        );
        assert_eq!(median_width(&[]), 0);
    }
}

// ===========================================================================================
// Arm (a) + arm (b): the corpus gates. These construct the real `Symbols` grounder (tree-sitter
// parsing), so they are confined to the `symbols` feature exactly like every other structural
// test (d16-both-lanes-gate). With the feature OFF the pure-metric tests above still run, so the
// no-default lane stays green and still exercises the metric logic.
// ===========================================================================================

#[cfg(test)]
#[cfg(feature = "symbols")]
mod corpus_gates {
    use super::*;
    use crate::grounder::symbols::grounder::Symbols;
    use crate::grounder::{BlastRadius, Grep, Grounder};
    use std::collections::HashSet;
    use std::path::Path;

    /// The distinct-file cap `grounded_seed` grounds at today (conductor.rs `grounded_seed`). It
    /// bounds the SUBJECT symbols view at the production cap - though the symbols safe view is
    /// uncapped by construction regardless, so `k` only bounds the precise seed. The grep
    /// BASELINE, by contrast, runs UNCAPPED (`usize::MAX`) in `measure`: grep's trait-default
    /// `blast_radius` returns `ground(q, k)` for both views, so a capped baseline would be a
    /// read_dir-order nondeterministic WHICH-`k` slice, unfit for a frozen gate
    /// (d16-u2-retention-isolates-serialize-cost). Arm (a) likewise runs grep uncapped.
    const GROUND_K: usize = 8;

    /// The queries whose `subject` safe view is NOT a superset of grep's UNCAPPED radius - the
    /// arm-(a) invariant violations (empty = pass). Grep runs uncapped (`usize::MAX`) so the
    /// check is against the FULL grep radius, not a top-k slice; the safe view is
    /// `blast_radius(q).safe`.
    fn safe_superset_violations(
        subject: &dyn Grounder,
        grep: &Grep,
        queries: &[&str],
    ) -> Vec<String> {
        let mut bad = Vec::new();
        for &q in queries {
            let safe: HashSet<String> =
                subject.blast_radius(q, GROUND_K).safe.into_iter().collect();
            let grep_files: HashSet<String> = grep
                .ground(q, usize::MAX)
                .into_iter()
                .map(|r| r.file)
                .collect();
            if !grep_files.is_subset(&safe) {
                bad.push(q.to_string());
            }
        }
        bad
    }

    /// Build the adversarial corpus for arm (a): one small Rust repo whose queries each name a
    /// symbol a NAME-LEVEL index can miss (a macro body, dynamic dispatch, a re-export, a
    /// reflection string, an over-linking common name), and where grep-only mentions are
    /// present so the superset check has teeth. Returns the queries to check.
    fn build_adversarial_corpus(root: &Path) -> Vec<&'static str> {
        let write = |name: &str, body: &str| std::fs::write(root.join(name), body).unwrap();

        // common-name: `new` is defined on two types and over-links to every like-named def.
        write(
            "common_a.rs",
            "pub struct A;\nimpl A { pub fn new() -> A { A } }\n",
        );
        write(
            "common_b.rs",
            "pub struct B;\nimpl B { pub fn new() -> B { B } }\n",
        );

        // macro: `render` is called only inside a macro body, which a name-level tags query can
        // fail to index as a reference - but grep matches the substring in macros.rs.
        write("widget.rs", "fn render() {}\n");
        write(
            "macros.rs",
            "macro_rules! paint { ($x:expr) => { render(); $x }; }\nfn go() { paint!(0); }\n",
        );

        // trait object: `draw` is invoked through `&dyn Shape`, a dynamic dispatch no
        // name-resolution links; grep recovers the call site.
        write(
            "painter.rs",
            "trait Shape { fn draw(&self); }\nfn paint(s: &dyn Shape) { s.draw(); }\n",
        );

        // re-export: `helper` is defined in an inner module and re-exported with `pub use`.
        write(
            "reexport.rs",
            "mod inner { pub fn helper() {} }\npub use inner::helper;\n",
        );

        // reflection: `compute` is invoked ONLY by a string literal - never a symbol reference,
        // so the structural graph cannot see reflect.rs; grep matches the string. This is the
        // load-bearing grep-only mention that gives the whole arm its teeth.
        write("compute_impl.rs", "fn compute() {}\n");
        write(
            "reflect.rs",
            "fn invoke(_name: &str) {}\nfn boot() { invoke(\"compute\"); }\n",
        );

        vec!["new", "render", "draw", "helper", "compute"]
    }

    /// Arm (a) - the implementation-invariant guard. On the adversarial corpus the safe view is
    /// a superset of grep for EVERY query, and the grep-only reflection mention is recovered by
    /// the union (present in `safe`, absent from the precise structural view). RED only if the
    /// `structural ∪ grep` union is built wrong.
    #[test]
    fn arm_a_safe_view_is_a_grep_superset_on_the_adversarial_corpus() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let queries = build_adversarial_corpus(root);
        let symbols = Symbols::open(root.to_str().unwrap(), None);
        let grep = Grep {
            root: root.to_str().unwrap().to_string(),
        };

        // The corpus must be non-vacuous: grep matches at least one file for every query, so an
        // empty safe view could never pass the superset check by default.
        for &q in &queries {
            assert!(
                !grep.ground(q, usize::MAX).is_empty(),
                "adversarial query {q:?} must match at least one file under grep"
            );
        }

        // The invariant: safe ⊇ grep for every query. This is the regression guard - it can
        // only fail if the union drops or intersects the grep side.
        let violations = safe_superset_violations(&symbols, &grep, &queries);
        assert!(
            violations.is_empty(),
            "the safe view must be a superset of grep on every adversarial query; \
             the union under-includes for: {violations:?}"
        );

        // Teeth: the reflection string is a grep-only file the structural graph cannot index.
        // It MUST be recovered into `safe` yet be ABSENT from the precise structural view - so a
        // `safe = structural` (drop grep) or `safe = structural ∩ grep` (intersect) mutation
        // drops reflect.rs and trips the superset check above.
        let compute = symbols.blast_radius("compute", GROUND_K);
        assert!(
            compute.safe.contains(&"reflect.rs".to_string()),
            "the safe union must recover the reflection string mention grep matches; got {compute:?}"
        );
        assert!(
            !compute.precise.contains(&"reflect.rs".to_string()),
            "a reflection string is no symbol reference; the precise structural view must miss \
             reflect.rs (which is exactly why the grep union is load-bearing); got {compute:?}"
        );
        // And the real definition IS in both views (the structural graph does see the def).
        assert!(compute.precise.contains(&"compute_impl.rs".to_string()));

        // Second cross-file teeth class: `render` is CALLED only inside a macro body. The
        // name-level tags query parses the macro_rules transcriber as an unresolved token tree,
        // so the precise structural view does NOT link macros.rs; grep matches the `render`
        // substring there, so the union MUST recover it into safe. A `safe = structural`
        // (drop-grep) or `safe = structural ∩ grep` (intersect) mutation drops macros.rs and
        // trips the superset guard for this class too. (The remaining three classes -
        // new/common-name, draw/trait-object, helper/re-export - keep the definition and the
        // hard-to-resolve reference in the SAME file, so precise == safe and they carry no
        // drop-grep teeth; only the reflection and macro classes are load-bearing here.)
        let render = symbols.blast_radius("render", GROUND_K);
        assert!(
            render.safe.contains(&"macros.rs".to_string()),
            "the safe union must recover the macro-body call grep matches in macros.rs; got {render:?}"
        );
        assert!(
            !render.precise.contains(&"macros.rs".to_string()),
            "a macro-body call is not a resolved reference; the precise structural view must miss \
             macros.rs (which is exactly why the grep union is load-bearing for the macro class); \
             got {render:?}"
        );
        // And the real definition IS in both views (the structural graph does see the def).
        assert!(render.precise.contains(&"widget.rs".to_string()));
    }

    /// The unit ids and queries of the PINNED polyglot repo (arm b). Each entry is
    /// `(unit id, query, definition file, definition source, reference degree)`. The definitions
    /// span five languages (Rust, Go, C#, Python, TypeScript) so radii are genuinely polyglot;
    /// all REFERENCES are Rust so the per-language degree distribution is controlled and
    /// non-degenerate (six distinct referenced names), with exactly one hub (`audit`, the top
    /// decile). Degrees are chosen so every safe radius exceeds the un-retuned `8` (proving the
    /// naive tier threshold collapses) with a spread that a distribution-tuned threshold splits.
    fn pinned_polyglot_units() -> Vec<(
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        usize,
    )> {
        vec![
            (
                "combat",
                "apply_damage",
                "combat.rs",
                "pub fn apply_damage() {}\n",
                8,
            ),
            (
                "physics",
                "simulate",
                "physics.go",
                "package main\nfunc simulate() {}\n",
                9,
            ),
            (
                "auth",
                "authenticate",
                "auth.cs",
                "class Auth { void authenticate() {} }\n",
                10,
            ),
            (
                "config",
                "parse_config",
                "config.py",
                "def parse_config():\n    pass\n",
                11,
            ),
            (
                "render",
                "render",
                "render.ts",
                "export function render() {}\n",
                12,
            ),
            ("audit", "audit", "logger.rs", "pub fn audit() {}\n", 14),
        ]
    }

    /// Materialize the pinned polyglot repo: each unit's definition file (in its language) plus
    /// `degree` dedicated Rust referencer files that each reference ONLY that unit's name. The
    /// referencer files are per-unit disjoint, so under grep every unit is co-schedulable (all
    /// radii disjoint) - retention then measures purely the parallelism the hub serialize costs.
    fn build_pinned_polyglot_repo(root: &Path) {
        for (unit, query, def_file, def_src, degree) in pinned_polyglot_units() {
            std::fs::write(root.join(def_file), def_src).unwrap();
            for i in 0..degree {
                // A unique-per-file caller that references exactly this unit's name. The `caller`
                // definition is single-purpose noise: a definition never counts toward the
                // reference-degree distribution and no query names it.
                std::fs::write(
                    root.join(format!("{unit}_ref_{i}.rs")),
                    format!("fn caller() {{ {query}(); }}\n"),
                )
                .unwrap();
            }
        }
    }

    /// Compute the per-unit radii under the grep baseline and under the symbols safe view. The
    /// baseline is grep's own `blast_radius` run UNCAPPED (its full radius, never serializing);
    /// the subject is the symbols safe superset with its hub serialize verdict.
    fn measure(root: &Path) -> (Vec<UnitRadius>, Vec<UnitRadius>) {
        let symbols = Symbols::open(root.to_str().unwrap(), None);
        let grep = Grep {
            root: root.to_str().unwrap().to_string(),
        };
        let mut baseline = Vec::new();
        let mut subject = Vec::new();
        for (unit, query, ..) in pinned_polyglot_units() {
            // The grep BASELINE runs UNCAPPED (usize::MAX), matching arm-a's uncapped grep and
            // honoring d16-u2-retention-isolates-serialize-cost: grep's trait-default blast_radius
            // returns ground(q, k) for both views, so a capped baseline would be a read_dir-order
            // nondeterministic WHICH-k slice, unfit for a frozen gate. Uncapped, the baseline IS
            // the full grep radius, so it equals the safe superset at the file-set level and the
            // ONLY thing that can differ is the hub serialize verdict.
            let g: BlastRadius = grep.blast_radius(query, usize::MAX);
            baseline.push(UnitRadius {
                unit: unit.to_string(),
                files: g.safe,
                serialize: g.serialize,
            });
            let s: BlastRadius = symbols.blast_radius(query, GROUND_K);
            subject.push(UnitRadius {
                unit: unit.to_string(),
                files: s.safe,
                serialize: s.serialize,
            });
        }
        (baseline, subject)
    }

    /// Arm (b) - the quantified go/no-go on the pinned polyglot repo.
    ///
    /// The bounds below are MEASURED on this fixture and FROZEN as red-on-regression assertions
    /// (adv-quant-bound-unpinned), not hand-chosen. The reasoning for each frozen bound is in
    /// its assertion. Because the safe view is a superset of grep by construction, this arm
    /// proves VALUE (retained parallelism, a non-collapsed tier split), never safety.
    #[test]
    fn arm_b_partitioning_and_routing_retention_gate_on_the_pinned_polyglot_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        build_pinned_polyglot_repo(root);
        let (baseline, subject) = measure(root);

        // Exactly one unit - `audit`, the top-decile hub - serializes; the polyglot corpus is
        // non-degenerate (six distinct referenced names) so this is a genuine percentile
        // outlier, not the thin-distribution artifact that flags a lone name
        // (adv-u16-1rr-degenerate-perlang-distribution-over-serializes). If a mis-tune flagged a
        // second unit, retention drops below the floor below and this count trips first.
        let serialized: Vec<&str> = subject
            .iter()
            .filter(|u| u.serialize)
            .map(|u| u.unit.as_str())
            .collect();
        assert_eq!(
            serialized,
            vec!["audit"],
            "exactly the hub unit must serialize on the pinned corpus"
        );
        assert!(
            !baseline.iter().any(|u| u.serialize),
            "the grep baseline never serializes"
        );

        // The safe view is a file-set superset of the grep baseline for every unit, and because
        // the baseline runs UNCAPPED (measure passes usize::MAX) it IS the full grep radius; with
        // structural ⊆ grep on this corpus that makes safe == the baseline grep radius EXACTLY
        // (both are widths 9..=15). So the ONLY thing that can cost parallelism is the hub
        // serialize - which is what retention measures.
        for (b, s) in baseline.iter().zip(subject.iter()) {
            let bset: HashSet<&String> = b.files.iter().collect();
            let sset: HashSet<&String> = s.files.iter().collect();
            assert!(
                bset.is_subset(&sset),
                "unit {}: safe must be a superset of the grep baseline radius",
                s.unit
            );
        }

        // --- Parallelism-retention gate -----------------------------------------------------
        let base_co = co_schedulable(&partition(&baseline));
        let subj_co = co_schedulable(&partition(&subject));
        let retention = parallelism_retention(&subject, &baseline);
        // MEASURED on this fixture: grep co-schedules all six units (all radii disjoint) and the
        // symbols view keeps five - only the hub serializes. retention = 5/6 ≈ 0.833.
        assert_eq!(
            base_co, 6,
            "grep baseline co-schedules all six disjoint units"
        );
        assert_eq!(
            subj_co, 5,
            "symbols keeps five co-schedulable (audit serializes)"
        );
        // FROZEN floor 0.80: it sits between the healthy 0.833 and the first regression - a
        // SECOND unit serializing drops retention to 4/6 = 0.667, well under 0.80. So the floor
        // is a meaningful red line calibrated to the corpus, not an arbitrary number.
        const MIN_RETENTION: f64 = 0.80;
        assert!(
            retention >= MIN_RETENTION,
            "parallelism retention {retention:.3} fell below the frozen floor {MIN_RETENTION} \
             (measured 0.833 on the pinned corpus; a regression here means the safe view \
             serializes or over-includes more than the pinned hub)"
        );

        // MEASURED median safe-radius is 11; FROZEN ceiling 12 (measured + 1) is red-on-regression
        // against radii that explode past the pinned distribution.
        let median = median_width(&subject);
        const MAX_MEDIAN_RADIUS: usize = 12;
        assert_eq!(median, 11, "the pinned median safe-radius is 11");
        assert!(
            median <= MAX_MEDIAN_RADIUS,
            "median safe-radius {median} exceeded the frozen ceiling {MAX_MEDIAN_RADIUS}"
        );

        // --- Tier-routing-distribution check ------------------------------------------------
        // The size signal must not collapse to all-full. The widths span 9..=15, so the
        // UN-RETUNED absolute threshold 8 routes EVERY unit full (1.0) - exactly the deadlock
        // that forbids driving the production `route_review_tier` here (adv-tier-threshold-
        // coupling). A threshold RE-TUNED to the width distribution restores a real split.
        let widths: Vec<usize> = subject.iter().map(|u| u.files.len()).collect();
        let naive_full = full_fraction(&widths, 8);
        assert_eq!(
            naive_full, 1.0,
            "the un-retuned threshold 8 collapses the uncapped safe widths to all-full - the \
             deadlock unit 2 must not gate on"
        );
        let tuned = width_threshold(&widths, 0.50);
        let tuned_full = full_fraction(&widths, tuned);
        // MEASURED: tuned threshold 12 routes 2 of 6 units full (1/3). FROZEN as a non-degenerate
        // split - strictly between all-light and all-full - proving unit 3's re-tune is feasible.
        assert_eq!(tuned, 12, "the distribution-tuned (median) threshold is 12");
        assert!(
            tuned_full > 0.0 && tuned_full < 1.0,
            "a distribution-tuned threshold must yield a non-degenerate light/full split, got \
             {tuned_full}"
        );
        assert!(
            tuned_full < naive_full,
            "re-tuning must strictly reduce the full-panel share versus the collapsed naive \
             threshold ({tuned_full} !< {naive_full})"
        );
    }

    /// The hapax-heavy (degree-one-dominated) referenced-name counts arm-c materializes: a long
    /// tail of `TAIL_NAMES` single-reference names plus two genuine high-degree outliers. This is
    /// the realistic long-tail distribution `model::is_hub` must not degenerate on - the shape the
    /// all-distinct-degree arm-b corpus never exercises.
    const TAIL_NAMES: usize = 20;
    const HUB_DEGREE: usize = 10;

    /// Materialize a hapax-heavy Rust repo: `TAIL_NAMES` names each referenced exactly once
    /// (degree 1) and two names (`hub_a`, `hub_b`) each referenced `HUB_DEGREE` times. Every
    /// referencer file references exactly one name, so the per-language degree distribution is a
    /// long tail of 1s with two genuine outliers. Returns the full name list (tail then hubs).
    fn build_hapax_heavy_repo(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for i in 0..TAIL_NAMES {
            let name = format!("hapax_{i}");
            std::fs::write(
                root.join(format!("tail_{i}.rs")),
                format!("fn caller() {{ {name}(); }}\n"),
            )
            .unwrap();
            names.push(name);
        }
        for hub in ["hub_a", "hub_b"] {
            for i in 0..HUB_DEGREE {
                std::fs::write(
                    root.join(format!("{hub}_ref_{i}.rs")),
                    format!("fn caller() {{ {hub}(); }}\n"),
                )
                .unwrap();
            }
            names.push(hub.to_string());
        }
        names
    }

    /// Arm (c) - the hapax-heavy red-on-regression arm that guards `model::is_hub`'s robustness
    /// through the real grounder. On a degree-one-dominated distribution the hub check (which
    /// drives `BlastRadius::serialize`) must flag ONLY the genuine high-degree outliers, never
    /// nearly every referenced name. The old nearest-rank `floor(N * percentile)` cutoff with the
    /// `>=` comparison collapses to 1 on this long tail and flags EVERY name as a hub - so every
    /// unit serializes and parallelism-retention collapses to near zero. This arm goes RED for
    /// exactly that over-serializing cutoff, while the all-distinct-degree arm-b corpus (which
    /// never enters the long-tail failure mode) still passes.
    #[test]
    fn arm_c_hub_detection_does_not_over_serialize_a_hapax_heavy_distribution() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let names = build_hapax_heavy_repo(root);
        let symbols = Symbols::open(root.to_str().unwrap(), None);

        // A single-name query's serialize verdict IS its hub verdict (serialize fires if any query
        // term is a hub in any present language). Collect every name the hub check serializes.
        let serialized: Vec<String> = names
            .iter()
            .filter(|n| symbols.blast_radius(n, GROUND_K).serialize)
            .cloned()
            .collect();

        // The two genuine outliers serialize; a degree-1 tail name never does.
        assert!(
            symbols.blast_radius("hub_a", GROUND_K).serialize,
            "the genuine high-degree outlier hub_a must serialize"
        );
        assert!(
            symbols.blast_radius("hub_b", GROUND_K).serialize,
            "the genuine high-degree outlier hub_b must serialize"
        );
        assert!(
            !symbols.blast_radius("hapax_0", GROUND_K).serialize,
            "a degree-1 tail name is not a hub and must not serialize"
        );

        // RED-ON-REGRESSION: an over-serializing cutoff floods this long tail, flagging nearly
        // every name. A robust cutoff flags only the genuine outliers, so the serialized fraction
        // stays near the top decile, never approaching all.
        let fraction = serialized.len() as f64 / names.len() as f64;
        assert!(
            fraction <= 0.20,
            "hub detection over-serialized a hapax-heavy distribution: {}/{} = {fraction:.3} names \
             flagged (an over-serializing cutoff floods a degree-one-dominated distribution); a \
             robust cutoff flags only the genuine high-degree outliers",
            serialized.len(),
            names.len()
        );
        assert_eq!(
            serialized,
            vec!["hub_a".to_string(), "hub_b".to_string()],
            "only the two genuine high-degree outliers serialize on a degree-one-dominated \
             distribution; got {serialized:?}"
        );
    }
}
