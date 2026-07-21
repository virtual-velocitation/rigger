//! Extract the enumerable acceptance criteria from a spec document - the
//! "Done-when" list the conductor's coverage gate checks every unit against. A
//! spec with none is not loop-ready.

/// ExtractCriteria returns the text of every markdown checkbox item ("- [ ] ...").
pub fn extract_criteria(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(checkbox_text)
        .map(str::to_string)
        .collect()
}

/// The repo-relative, path-like tokens a spec's `criteria` reference (e.g. `src/main.rs`,
/// `crates/foo/src/bar.rs`) - so a run entry can check them against its base ref and refuse
/// an obviously-wrong base before it parks its first unit (spec 18). Deliberately
/// conservative: a token qualifies ONLY when it looks unmistakably like a relative file
/// path (see [`looks_like_repo_path`]), so ordinary prose ("and/or"), option flags
/// (`--base`), type names (`Type::Name`), version numbers (`0.1.0`), and URLs are never
/// mistaken for a path. This asymmetry is intentional - a missed path (false negative) only
/// weakens the wrong-base signal, but a spurious token (false positive) could refuse a run
/// on a CORRECT base, which the spec forbids. Markdown backticks and surrounding
/// punctuation are trimmed. The result preserves first-seen order and is de-duplicated
/// (an ordered `Vec`, no `HashSet`, so it stays deterministic).
pub fn path_tokens(criteria: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for criterion in criteria {
        for raw in criterion.split_whitespace() {
            // Trim the markdown/sentence punctuation that commonly wraps an inline path.
            // Leading and trailing sets differ so a hidden-directory dot (`.github/...`) is
            // preserved while a trailing sentence period (`... src/main.rs.`) is dropped.
            let tok = raw
                .trim_start_matches(['`', '\'', '"', '(', '[', '{', '<'])
                .trim_end_matches(['`', '\'', '"', ')', ']', '}', '>', ',', ';', ':', '.']);
            if looks_like_repo_path(tok) && !out.iter().any(|p| p == tok) {
                out.push(tok.to_string());
            }
        }
    }
    out
}

/// Whether `tok` looks unmistakably like a repo-relative file path. Requires: a path
/// separator (`/`); only path-safe characters (`[A-Za-z0-9._/-]`); no scheme (`://`), so a
/// URL is excluded; no empty, `.`, or `..` path segment; and a final segment carrying a
/// plausible file extension (`name.ext`, where `ext` is 1-10 characters, alphanumeric, and
/// begins with a letter - so a numeric tail like `1.2.3` is not read as an extension). This
/// is the conservative predicate that keeps [`path_tokens`] free of false positives.
fn looks_like_repo_path(tok: &str) -> bool {
    if tok.is_empty() || tok.starts_with('/') || !tok.contains('/') || tok.contains("://") {
        return false;
    }
    if !tok
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
    {
        return false;
    }
    let mut last = "";
    for seg in tok.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return false;
        }
        last = seg;
    }
    match last.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && (1..=10).contains(&ext.len())
                && ext.starts_with(|c: char| c.is_ascii_alphabetic())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }
        None => false,
    }
}

fn checkbox_text(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix('-').or_else(|| rest.strip_prefix('*'))?;
    let rest = rest.trim_start().strip_prefix('[')?;
    let mark = rest.chars().next()?;
    if !matches!(mark, ' ' | 'x' | 'X') {
        return None;
    }
    let rest = rest[mark.len_utf8()..].strip_prefix(']')?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// The single recommendation every spec-shape advisory ends with: keep each Done-when
/// criterion to ONE observable behavior, and move type shapes / structural detail into a
/// non-criteria Notes section. A criterion that packs several behaviors, hides a
/// sub-criterion in an indented bullet, or runs long is exactly the shape a planner
/// paraphrases or truncates when told to copy it verbatim, which then fails the
/// baseline-id match the conductor reconciles proposals against.
pub const SHAPE_RECOMMENDATION: &str =
    "one observable behavior per criterion; put type shapes and detail in a non-criteria \
     Notes section";

/// A criterion longer than this many characters is flagged `over-long`: a verbatim
/// planner copy of a criterion this long is unreliable (it paraphrases or truncates).
const MAX_CRITERION_LEN: usize = 240;

/// Which spec-shape rule an advisory fired on. The lint is ADVISORY only - it never
/// hard-fails - so a heuristic false negative just misses a warning, and the rules are
/// deliberately biased against false positives (a clean single-behavior spec stays
/// silent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeRule {
    /// A checkbox that packs more than one observable behavior.
    MultiBehavior,
    /// A plain indented bullet under a checkbox that reads as a separate criterion.
    SubBulletAsUnit,
    /// A criterion long enough that a verbatim planner copy is unreliable.
    OverLong,
}

impl ShapeRule {
    /// The stable rule name that appears in the advisory (and that callers grep for).
    pub fn name(self) -> &'static str {
        match self {
            ShapeRule::MultiBehavior => "multi-behavior",
            ShapeRule::SubBulletAsUnit => "sub-bullet-as-unit",
            ShapeRule::OverLong => "over-long",
        }
    }
}

/// One heuristic spec-shape advisory: which rule fired, the 1-based criterion it fired
/// on, and a short human reason. Rendered (Display) as
/// `<rule>: criterion <n>: <detail>. Recommendation: <SHAPE_RECOMMENDATION>.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeAdvisory {
    pub rule: ShapeRule,
    pub criterion: usize,
    pub detail: String,
}

impl std::fmt::Display for ShapeAdvisory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: criterion {}: {}. Recommendation: {}.",
            self.rule.name(),
            self.criterion,
            self.detail,
            SHAPE_RECOMMENDATION
        )
    }
}

/// Heuristic spec-shape advisories over a spec document's Done-when criteria - warnings
/// only, NEVER a hard failure (a heuristic must not block a run). Flags three shapes that
/// a planner paraphrases or truncates when told to copy a criterion verbatim:
///   - `multi-behavior`: a checkbox that packs several observable behaviors;
///   - `sub-bullet-as-unit`: a plain indented bullet under a checkbox that reads as its
///     own criterion;
///   - `over-long`: a criterion long enough that a verbatim copy is unreliable.
///
/// Deliberately biased against FALSE POSITIVES so a clean single-behavior spec stays
/// silent: false negatives (an unusual shape it misses) are acceptable. Advisories are
/// returned in document order, grouped by criterion then by rule. Reuses
/// [`extract_criteria`] for the criterion list, so indices align with it.
pub fn spec_shape_advisories(text: &str) -> Vec<ShapeAdvisory> {
    let criteria = extract_criteria(text);
    let sub_bullets = sub_bullet_criteria(text);
    let mut out = Vec::new();
    for (i, criterion) in criteria.iter().enumerate() {
        let n = i + 1;
        if let Some(count) = multi_behavior_coordinators(criterion) {
            out.push(ShapeAdvisory {
                rule: ShapeRule::MultiBehavior,
                criterion: n,
                detail: format!(
                    "packs multiple observable behaviors ({count} clause coordinators)"
                ),
            });
        }
        if let Some(bullet) = sub_bullets.get(&n) {
            out.push(ShapeAdvisory {
                rule: ShapeRule::SubBulletAsUnit,
                criterion: n,
                detail: format!(
                    "an indented sub-bullet reads as a separate criterion (\"{bullet}\")"
                ),
            });
        }
        let len = criterion.chars().count();
        if len > MAX_CRITERION_LEN {
            out.push(ShapeAdvisory {
                rule: ShapeRule::OverLong,
                criterion: n,
                detail: format!(
                    "is {len} characters; a verbatim planner copy of a criterion this long is unreliable"
                ),
            });
        }
    }
    out
}

/// The number of clause coordinators in a criterion when there are ENOUGH to flag it
/// `multi-behavior` (>= 2), else `None`. A coordinator is a comma- or semicolon-joined
/// clause separator - `", and "`, `", then "`, or `"; "` (case-insensitive). One
/// coordinator is often a noun pair, an Oxford list, or a single qualifying clause, so
/// the threshold is TWO independent separators: that reliably marks several observable
/// behaviors stacked in one checkbox while keeping the lint silent on a clean
/// single-behavior criterion (the false positive the Unit-4 criterion forbids).
fn multi_behavior_coordinators(criterion: &str) -> Option<usize> {
    let lower = criterion.to_lowercase();
    let count = [", and ", ", then ", "; "]
        .iter()
        .map(|sep| lower.matches(sep).count())
        .sum::<usize>();
    (count >= 2).then_some(count)
}

/// Map from 1-based criterion index to the text of the FIRST plain indented bullet found
/// directly under that checkbox - a bullet that reads as a separate criterion hidden
/// inside one. A checkbox with none is absent from the map; only the first sub-bullet per
/// checkbox is reported (one advisory per criterion makes the point). A NESTED checkbox
/// is not a sub-bullet: it is its own criterion (`extract_criteria` counts it), so it
/// opens a new scope rather than flagging its parent. Indices align with
/// [`extract_criteria`] because both recognize a checkbox with the same [`checkbox_text`].
fn sub_bullet_criteria(text: &str) -> std::collections::BTreeMap<usize, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut count = 0usize; // 1-based index of the current checkbox
    let mut open: Option<usize> = None; // indent of the currently open checkbox, if any
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if checkbox_text(line).is_some() {
            count += 1;
            open = Some(indent);
        } else if trimmed.is_empty() {
            // A blank line does not close a markdown list item.
            continue;
        } else if let Some(cb_indent) = open {
            if indent > cb_indent {
                // A MORE-indented plain bullet under the checkbox is a hidden
                // sub-criterion; a more-indented non-bullet line is wrapped criterion text.
                if let Some(bullet) = plain_bullet_text(trimmed) {
                    out.entry(count).or_insert(bullet);
                }
            } else {
                // A dedent to or under the checkbox closes the item.
                open = None;
            }
        }
    }
    out
}

/// The text of a plain markdown bullet (`- ` or `* `), leading marker stripped and
/// truncated for a message, or `None` when `trimmed` is not a plain bullet. Callers pass
/// a line already known not to be a checkbox, so no checkbox re-check is needed.
fn plain_bullet_text(trimmed: &str) -> Option<String> {
    let body = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))?
        .trim();
    Some(truncate_for_message(body))
}

/// Truncate a message fragment to a readable length, appending `...` when clipped. Uses a
/// char boundary so multi-byte text is never split mid-codepoint.
fn truncate_for_message(s: &str) -> String {
    const MAX: usize = 60;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{head}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_checkbox_criteria() {
        let text = "# Feature\n\nsome prose\n\n- [ ] the store passes the contract suite\n- [x] the graph supersedes\n* [ ] the conductor integrates\n\n- a plain bullet is ignored\n";
        assert_eq!(
            extract_criteria(text),
            [
                "the store passes the contract suite",
                "the graph supersedes",
                "the conductor integrates",
            ]
        );
    }

    #[test]
    fn empty_when_no_criteria() {
        assert!(extract_criteria("# just prose\n\nno checkboxes").is_empty());
    }

    /// A clean single-behavior spec emits NO spec-shape advisory - the Unit-4 no-false
    /// positive requirement: each criterion is one short observable behavior.
    #[test]
    fn clean_single_behavior_spec_is_silent() {
        let text = "# Feature\n\n## Done when\n\n\
            - [ ] the store passes the contract suite\n\
            - [ ] the graph projector supersedes an older decision\n\
            - [ ] the conductor integrates an approved unit\n";
        assert!(
            spec_shape_advisories(text).is_empty(),
            "a clean single-behavior spec must yield no advisory; got: {:?}",
            spec_shape_advisories(text)
        );
    }

    /// A checkbox that packs several observable behaviors (two or more clause
    /// coordinators) is flagged `multi-behavior`, on the right criterion, with the
    /// recommendation.
    #[test]
    fn multi_behavior_checkbox_is_flagged() {
        let text = "## Done when\n\n\
            - [ ] the store passes the contract suite\n\
            - [ ] the daemon starts on boot, and it writes a pidfile, and it rotates the log nightly\n";
        let advisories = spec_shape_advisories(text);
        let hit = advisories
            .iter()
            .find(|a| a.rule == ShapeRule::MultiBehavior)
            .expect("the two-coordinator checkbox must be flagged multi-behavior");
        assert_eq!(hit.criterion, 2, "it is the SECOND criterion");
        assert!(
            hit.to_string().contains("multi-behavior")
                && hit.to_string().contains(SHAPE_RECOMMENDATION),
            "the advisory names the rule and carries the recommendation; got: {hit}"
        );
        // The clean first criterion must NOT be flagged.
        assert!(
            !advisories.iter().any(|a| a.criterion == 1),
            "the clean single-behavior criterion 1 must stay silent; got: {advisories:?}"
        );
    }

    /// A single clause coordinator is NOT enough to flag `multi-behavior` - the threshold
    /// is two, biased against false positives (a noun pair / Oxford list / single qualifier
    /// carries one coordinator and must stay silent).
    #[test]
    fn a_single_coordinator_does_not_flag_multi_behavior() {
        let text = "## Done when\n\n\
            - [ ] rigger version reports the crate version, and a build-provenance id\n";
        assert!(
            !spec_shape_advisories(text)
                .iter()
                .any(|a| a.rule == ShapeRule::MultiBehavior),
            "one coordinator is below the multi-behavior threshold"
        );
    }

    /// A plain indented bullet directly under a checkbox reads as a separate criterion and
    /// is flagged `sub-bullet-as-unit` on the ENCLOSING checkbox - while a nested checkbox
    /// (its own criterion) does not flag its parent.
    #[test]
    fn indented_sub_bullet_under_a_checkbox_is_flagged() {
        let text = "## Done when\n\n\
            - [ ] the daemon writes a pidfile\n\
            \x20\x20- it is mode 0644\n\
            \x20\x20- it is removed on shutdown\n\
            - [ ] the store passes the contract suite\n";
        let advisories = spec_shape_advisories(text);
        let hit = advisories
            .iter()
            .find(|a| a.rule == ShapeRule::SubBulletAsUnit)
            .expect("the indented plain bullet must be flagged sub-bullet-as-unit");
        assert_eq!(hit.criterion, 1, "the sub-bullet belongs to criterion 1");
        assert!(
            hit.detail.contains("mode 0644"),
            "the advisory names the offending sub-bullet; got: {}",
            hit.detail
        );
        // Criterion 2 has no sub-bullet.
        assert!(
            !advisories
                .iter()
                .any(|a| a.criterion == 2 && a.rule == ShapeRule::SubBulletAsUnit),
            "criterion 2 has no sub-bullet; got: {advisories:?}"
        );
    }

    /// A criterion long enough that a verbatim planner copy is unreliable is flagged
    /// `over-long`, and a short criterion beside it is not.
    #[test]
    fn over_long_criterion_is_flagged() {
        let long = "x".repeat(MAX_CRITERION_LEN + 1);
        let text =
            format!("## Done when\n\n- [ ] the store passes the contract suite\n- [ ] {long}\n");
        let advisories = spec_shape_advisories(&text);
        let hit = advisories
            .iter()
            .find(|a| a.rule == ShapeRule::OverLong)
            .expect("a criterion over the length threshold must be flagged over-long");
        assert_eq!(hit.criterion, 2, "the long criterion is the second");
        assert!(
            !advisories.iter().any(|a| a.criterion == 1),
            "the short criterion 1 must stay silent; got: {advisories:?}"
        );
    }

    #[test]
    fn path_tokens_extracts_relative_file_paths_and_trims_markdown() {
        let criteria = vec![
            "touches `src/main.rs` and crates/foo/src/bar.rs".to_string(),
            "the file src/x/y.rs exports Z".to_string(),
        ];
        assert_eq!(
            path_tokens(&criteria),
            ["src/main.rs", "crates/foo/src/bar.rs", "src/x/y.rs"]
        );
    }

    #[test]
    fn path_tokens_ignores_prose_flags_versions_types_and_urls() {
        let criteria = vec![
            "refuse and/or warn, pass --base <ref>, see https://example.com/x.html".to_string(),
            "a bare word config, a Type::Name, rigger_emit, and version 0.1.0".to_string(),
        ];
        assert!(
            path_tokens(&criteria).is_empty(),
            "no non-path token may be read as a path; got {:?}",
            path_tokens(&criteria)
        );
    }

    #[test]
    fn path_tokens_dedupes_and_preserves_first_seen_order() {
        let criteria = vec![
            "b/two.rs then a/one.rs".to_string(),
            "again a/one.rs and b/two.rs".to_string(),
        ];
        assert_eq!(path_tokens(&criteria), ["b/two.rs", "a/one.rs"]);
    }

    #[test]
    fn path_tokens_drops_trailing_period_but_keeps_a_hidden_directory() {
        let criteria = vec!["adds .github/workflows/ci.yml.".to_string()];
        assert_eq!(path_tokens(&criteria), [".github/workflows/ci.yml"]);
    }

    #[test]
    fn path_tokens_requires_an_alphabetic_extension_and_a_separator() {
        // No separator, or a directory-only / numeric-tail token, never qualifies.
        let criteria = vec![
            "main.rs Cargo.toml".to_string(), // no slash
            "crates/foo/ and foo/1.2.3".to_string(),
        ];
        assert!(
            path_tokens(&criteria).is_empty(),
            "got {:?}",
            path_tokens(&criteria)
        );
    }
}
