//! The declarative failure taxonomy (spec 10, unit 2): an ordered set of rules that
//! classify a failure SIGNAL (a process's exit status, terminating signal, and/or
//! captured output) into one of three classes - `infra`, `product`, or `flaky` -
//! carrying a per-rule rerun `limit` and exponential `backoff`. First match wins.
//!
//! This is the SINGLE classification authority the conductor folds its failure
//! decisions from (R1), replacing the infra-vs-product distinction that was hand-coded
//! across the conductor's gate/spawn/reviewer sites. The domain stays framework-free:
//! no conductor, store, or config types leak in - the conductor builds a [`Taxonomy`]
//! from its config and asks it to [`classify`](Taxonomy::classify) a [`Signal`].
//!
//! The shipped [`Taxonomy::default`] preserves spec-07 semantics: recognised transient
//! infrastructure faults classify `infra` (rerun, never a product defect) and every
//! other failure falls through to the `product` catch-all (a real defect that charges a
//! remediation attempt and, at a gate, demotes the autonomy ratchet).

use std::time::Duration;

use regex::Regex;

/// How a failure is classified, and what the loop owes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    /// An infrastructure fault (a broken tool, a transient outage, a dead worker) -
    /// NOT the unit's code. Spec-07 semantics: it never charges the unit a remediation
    /// attempt, and a persistent infra fault at a gate must not demote the ratchet (an
    /// outage should never destroy a gate's earned autonomy).
    Infra,
    /// A genuine product defect: the unit's own code / gates / review failed. It charges
    /// a remediation attempt and, at a gate, demotes the autonomy ratchet - the exact
    /// hand-coded behavior before this taxonomy.
    Product,
    /// A flaky failure: non-deterministic, may pass on rerun. A gate whose failure is
    /// flaky reruns up to the rule's [`FailureRule::limit`]; a MIXED result (it passed
    /// on a rerun) is a pass-with-warning that never demotes the ratchet, while a
    /// consistent all-rerun failure is a real defect (demote + remediate).
    Flaky,
}

impl FailureClass {
    /// The canonical lowercase label, as written in `failure_rules[].class` and stamped
    /// on the events the conductor records.
    pub fn as_str(&self) -> &'static str {
        match self {
            FailureClass::Infra => "infra",
            FailureClass::Product => "product",
            FailureClass::Flaky => "flaky",
        }
    }

    /// Parse a class label. Strict: an unknown label is `None` so config validation can
    /// reject a typo rather than silently defaulting a misclassified failure.
    pub fn parse(s: &str) -> Option<FailureClass> {
        match s {
            "infra" => Some(FailureClass::Infra),
            "product" => Some(FailureClass::Product),
            "flaky" => Some(FailureClass::Flaky),
            _ => None,
        }
    }

    /// Whether a gate failure of this class is rerun before it is believed (the Bazel
    /// flaky-attempts model). A `product` failure is deterministic - it is not rerun;
    /// `infra` and `flaky` failures are transient and rerun up to the rule's limit.
    pub fn reruns(&self) -> bool {
        !matches!(self, FailureClass::Product)
    }

    /// Whether a PERSISTENT failure of this class (one that stayed red across every
    /// rerun) demotes the autonomy ratchet at a gate. A `product` or `flaky` gate that
    /// consistently fails is a real defect and demotes exactly as a gate failure did
    /// before; an `infra` fault never demotes - an outage must not cost a gate its
    /// earned autonomy.
    pub fn demotes_on_persistent_failure(&self) -> bool {
        !matches!(self, FailureClass::Infra)
    }
}

/// Exponential backoff between the reruns of a flaky/infra gate failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Backoff {
    /// The base delay before the FIRST rerun. Zero (the default) means no wait - the
    /// value tests configure so a rerun costs no wall-clock time.
    pub duration: Duration,
    /// The multiplier applied per rerun: rerun `n` waits `duration * factor^n`.
    pub factor: f64,
    /// The cap on the computed delay. Zero means uncapped.
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            duration: Duration::ZERO,
            factor: 1.0,
            max: Duration::ZERO,
        }
    }
}

impl Backoff {
    /// The delay before rerun number `n` (0-based): `duration * factor^n`, capped at
    /// `max` when `max` is non-zero. A zero base duration yields zero regardless of
    /// factor, so a backoff-less rule (and the test default) never sleeps.
    pub fn delay(&self, n: u32) -> Duration {
        if self.duration.is_zero() {
            return Duration::ZERO;
        }
        let scaled = self.duration.as_secs_f64() * self.factor.powi(n as i32);
        let capped = if self.max.is_zero() {
            scaled
        } else {
            scaled.min(self.max.as_secs_f64())
        };
        Duration::from_secs_f64(capped.max(0.0))
    }
}

/// A failure SIGNAL observed at a classification site: the exit status and/or the
/// terminating signal of a process (when known), and its captured output. A gate site
/// supplies the output (the compact gate evidence); a spawn / liveness site can supply
/// the exit status. An absent numeric field simply never matches a rule that pins it.
#[derive(Clone, Debug, Default)]
pub struct Signal {
    pub exit_status: Option<i32>,
    pub signal: Option<i32>,
    pub output: String,
}

impl Signal {
    /// A signal carrying only captured output - the shape a gate site produces (the
    /// gate runner yields compact evidence, not a raw exit code).
    pub fn from_output(output: impl Into<String>) -> Self {
        Signal {
            output: output.into(),
            ..Default::default()
        }
    }
}

/// A rule's match predicate. Every PRESENT field must match (logical AND); an absent
/// field is a wildcard. An all-absent matcher matches every signal - the catch-all a
/// final `product` rule uses to classify anything the earlier rules did not.
#[derive(Clone, Debug)]
pub struct Matcher {
    pub exit_status: Option<i32>,
    pub signal: Option<i32>,
    pub output_regex: Option<Regex>,
}

impl Matcher {
    /// A matcher that matches every signal (all fields wildcard).
    pub fn any() -> Matcher {
        Matcher {
            exit_status: None,
            signal: None,
            output_regex: None,
        }
    }

    /// Whether this predicate matches `sig`: every present field must match; a present
    /// numeric field requires the signal carry that exact value (an absent signal value
    /// never matches a pinned rule), and a present `output_regex` must find a match in
    /// the signal's output.
    pub fn matches(&self, sig: &Signal) -> bool {
        if let Some(code) = self.exit_status {
            if sig.exit_status != Some(code) {
                return false;
            }
        }
        if let Some(s) = self.signal {
            if sig.signal != Some(s) {
                return false;
            }
        }
        if let Some(re) = &self.output_regex {
            if !re.is_match(&sig.output) {
                return false;
            }
        }
        true
    }
}

/// One ordered failure rule: what to match, how to classify it, and - for a rerunnable
/// class at a gate - how many times to rerun and with what backoff.
#[derive(Clone, Debug)]
pub struct FailureRule {
    pub matcher: Matcher,
    pub class: FailureClass,
    /// The rerun budget (the Bazel flaky-attempts count): how many additional times a
    /// matching gate failure is rerun before it is believed. 0 = never rerun.
    pub limit: u32,
    pub backoff: Backoff,
}

/// The ordered rule set, matched FIRST-WINS. The conductor builds one from its config
/// (or takes the shipped [`Taxonomy::default`]) and folds every gate-failure decision
/// through it.
#[derive(Clone, Debug)]
pub struct Taxonomy {
    rules: Vec<FailureRule>,
}

impl Taxonomy {
    /// Build a taxonomy from an explicit, ordered rule list.
    pub fn new(rules: Vec<FailureRule>) -> Taxonomy {
        Taxonomy { rules }
    }

    /// Whether the rule set is empty (no rules configured). An empty taxonomy classifies
    /// nothing, so every caller falls back to its own default (for a gate, a plain
    /// product failure - today's behavior).
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The rules, in match order (for inspection / testing).
    pub fn rules(&self) -> &[FailureRule] {
        &self.rules
    }

    /// Classify a signal by the FIRST matching rule. `None` when no rule matches, which
    /// the caller treats as its own default (a gate treats an unmatched failure as a
    /// plain product defect: no rerun, demote + remediate).
    pub fn classify(&self, sig: &Signal) -> Option<&FailureRule> {
        self.rules.iter().find(|r| r.matcher.matches(sig))
    }
}

impl Default for Taxonomy {
    /// The shipped defaults preserving spec-07 semantics. Recognised transient
    /// infrastructure faults - out of disk, a refused/reset connection, DNS failure,
    /// a raw `ECONNRESET`/`ETIMEDOUT` - classify `infra` (reran, never charged a product
    /// defect); everything else falls through to the `product` catch-all, exactly the
    /// hand-coded infra-vs-product split the conductor shipped before this taxonomy.
    /// The patterns are deliberately narrow and unambiguous so a real product gate
    /// failure is never mistaken for infra.
    fn default() -> Taxonomy {
        let infra = Regex::new(
            "(?i)(no space left on device\
            |resource temporarily unavailable\
            |connection refused\
            |connection reset by peer\
            |could not resolve host\
            |temporary failure in name resolution\
            |econnreset\
            |etimedout)",
        )
        .expect("the shipped default infra regex is a valid pattern");
        Taxonomy::new(vec![
            FailureRule {
                matcher: Matcher {
                    exit_status: None,
                    signal: None,
                    output_regex: Some(infra),
                },
                class: FailureClass::Infra,
                limit: 2,
                backoff: Backoff {
                    duration: Duration::from_secs(1),
                    factor: 2.0,
                    max: Duration::from_secs(30),
                },
            },
            FailureRule {
                matcher: Matcher::any(),
                class: FailureClass::Product,
                limit: 0,
                backoff: Backoff::default(),
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_parses_and_round_trips_its_label() {
        for c in [
            FailureClass::Infra,
            FailureClass::Product,
            FailureClass::Flaky,
        ] {
            assert_eq!(FailureClass::parse(c.as_str()), Some(c));
        }
        assert_eq!(
            FailureClass::parse("bogus"),
            None,
            "an unknown class label is rejected, not silently defaulted"
        );
    }

    #[test]
    fn class_governs_rerun_and_demotion() {
        // product: deterministic - never rerun, demotes on failure (today's behavior).
        assert!(!FailureClass::Product.reruns());
        assert!(FailureClass::Product.demotes_on_persistent_failure());
        // flaky: rerun, but a consistent all-rerun failure is a real defect (demotes).
        assert!(FailureClass::Flaky.reruns());
        assert!(FailureClass::Flaky.demotes_on_persistent_failure());
        // infra: rerun, and never demotes even when persistent (an outage must not cost
        // the gate its earned autonomy).
        assert!(FailureClass::Infra.reruns());
        assert!(!FailureClass::Infra.demotes_on_persistent_failure());
    }

    #[test]
    fn matcher_ands_present_fields_and_wildcards_absent_ones() {
        let m = Matcher {
            exit_status: Some(101),
            signal: None,
            output_regex: Some(Regex::new("boom").unwrap()),
        };
        // Both present fields must match.
        assert!(m.matches(&Signal {
            exit_status: Some(101),
            signal: Some(9),
            output: "kaboom!".into(),
        }));
        // Wrong exit status: no match even though the regex matches.
        assert!(!m.matches(&Signal {
            exit_status: Some(1),
            output: "boom".into(),
            ..Default::default()
        }));
        // Right exit status, output missing the pattern: no match.
        assert!(!m.matches(&Signal {
            exit_status: Some(101),
            output: "quiet".into(),
            ..Default::default()
        }));
        // A pinned numeric field never matches a signal that lacks that value.
        assert!(!m.matches(&Signal {
            exit_status: None,
            output: "boom".into(),
            ..Default::default()
        }));
    }

    #[test]
    fn any_matcher_matches_every_signal() {
        let any = Matcher::any();
        assert!(any.matches(&Signal::default()));
        assert!(any.matches(&Signal::from_output("whatever")));
        assert!(any.matches(&Signal {
            exit_status: Some(137),
            signal: Some(9),
            output: "killed".into(),
        }));
    }

    #[test]
    fn classify_is_first_match_wins() {
        let tax = Taxonomy::new(vec![
            FailureRule {
                matcher: Matcher {
                    exit_status: None,
                    signal: None,
                    output_regex: Some(Regex::new("flake").unwrap()),
                },
                class: FailureClass::Flaky,
                limit: 3,
                backoff: Backoff::default(),
            },
            FailureRule {
                matcher: Matcher::any(),
                class: FailureClass::Product,
                limit: 0,
                backoff: Backoff::default(),
            },
        ]);
        // The first rule wins for a matching signal even though the catch-all also would.
        let hit = tax
            .classify(&Signal::from_output("intermittent flake here"))
            .unwrap();
        assert_eq!(hit.class, FailureClass::Flaky);
        assert_eq!(hit.limit, 3);
        // A non-flake signal falls through to the product catch-all.
        assert_eq!(
            tax.classify(&Signal::from_output("assertion failed"))
                .unwrap()
                .class,
            FailureClass::Product
        );
    }

    #[test]
    fn empty_taxonomy_classifies_nothing() {
        let tax = Taxonomy::new(vec![]);
        assert!(tax.is_empty());
        assert!(tax.classify(&Signal::from_output("anything")).is_none());
    }

    #[test]
    fn default_taxonomy_preserves_spec07_infra_vs_product() {
        let tax = Taxonomy::default();
        // A recognised transient infra fault classifies infra (reran, never a defect).
        let infra = tax
            .classify(&Signal::from_output(
                "error: No space left on device (os error 28)",
            ))
            .expect("an infra fault matches the default infra rule");
        assert_eq!(infra.class, FailureClass::Infra);
        assert!(infra.limit >= 1, "an infra fault is rerun");
        assert!(!infra.backoff.duration.is_zero(), "infra reruns back off");
        // An ordinary product gate failure falls through to the product catch-all: no
        // rerun (today's behavior), which is what keeps every existing gate test green.
        let product = tax
            .classify(&Signal::from_output(
                "FAIL\nassertion `left == right` failed",
            ))
            .expect("the catch-all classifies any failure");
        assert_eq!(product.class, FailureClass::Product);
        assert_eq!(product.limit, 0, "a plain product failure is never reran");
        assert!(!product.class.reruns());
    }

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let b = Backoff {
            duration: Duration::from_secs(1),
            factor: 2.0,
            max: Duration::from_secs(5),
        };
        assert_eq!(b.delay(0), Duration::from_secs(1));
        assert_eq!(b.delay(1), Duration::from_secs(2));
        assert_eq!(b.delay(2), Duration::from_secs(4));
        // 1 * 2^3 = 8s, capped at the 5s max.
        assert_eq!(b.delay(3), Duration::from_secs(5));
        // A zero base never sleeps, whatever the factor - the test/no-backoff default.
        let none = Backoff::default();
        assert!(none.delay(0).is_zero());
        assert!(none.delay(5).is_zero());
    }
}
