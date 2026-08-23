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
/// Built on [`line_criterion`]'s block-boundary walk - the ONE place that walk lives -
/// rather than re-deriving the same count/open/indent state machine a second time.
fn sub_bullet_criteria(text: &str) -> std::collections::BTreeMap<usize, String> {
    let owners = line_criterion(text);
    let mut out = std::collections::BTreeMap::new();
    for (line, owner) in text.lines().zip(owners) {
        let Some(idx) = owner else { continue };
        if checkbox_text(line).is_some() {
            // The checkbox's own line (or a nested checkbox, which owns itself) - not a
            // sub-bullet under a parent.
            continue;
        }
        if let Some(bullet) = plain_bullet_text(line.trim_start()) {
            out.entry(idx).or_insert(bullet);
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

// ---------------------------------------------------------------------------------------
// Spec 66 (unit c3): the mechanical subset of the planning-field-guide recipe as a lint -
// ownership (F1), open dispositions (F4), and hygiene (the diff gate's em-dash rule) - the
// three classes `spec_shape_advisories` above does NOT cover. These reuse the SAME parsing
// primitives (`extract_criteria`, `checkbox_text`) rather than a second parser, and
// [`spec_lint_advisories`] is the ONE combined surface `cmd_validate` calls for the full
// pre-launch lint (see decision d-u66c3-lint-mechanism).
// ---------------------------------------------------------------------------------------

/// One discipline advisory from the mechanical spec lint: which field-guide class it maps
/// to (`"F1 ownership"`, `"F4 disposition"`, or `"hygiene"` for the diff gate's em-dash
/// rule, which has no catalog entry of its own), the 1-based Done-when criterion it is
/// tied to (`None` when the offending text sits outside any checkbox - Design prose,
/// Global constraints, or Notes), and a human detail. Advisory only: `rigger validate`
/// never fails on one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintAdvisory {
    pub class: &'static str,
    pub criterion: Option<usize>,
    pub detail: String,
}

impl std::fmt::Display for LintAdvisory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.criterion {
            Some(n) => write!(f, "{} (criterion {n}): {}", self.class, self.detail),
            None => write!(f, "{}: {}", self.class, self.detail),
        }
    }
}

/// F1 ownership (`docs/handbook/planning-field-guide.md`): the #1 recurring plan-critique
/// killer is a criterion whose concern is silently claimed by two units, because neither
/// carries an ownership sentence a reviewer (or a planner copying criteria verbatim) can
/// check against its neighbors. At three-plus Done-when criteria, a checkbox carrying
/// neither "OWNS" nor "owner" is flagged a twin-risk. Silent below three criteria: an
/// ownership collision needs at least two OTHER criteria to collide with. Scans each
/// criterion's FULL block via [`criterion_blocks`] - never just [`extract_criteria`]'s
/// first-physical-line text - so an OWNS sentence on a wrapped continuation line (this
/// repo's own standard Done-when convention) is found.
pub fn ownership_advisories(text: &str) -> Vec<LintAdvisory> {
    let criteria = extract_criteria(text);
    if criteria.len() < 3 {
        return Vec::new();
    }
    let blocks = criterion_blocks(text);
    (1..=criteria.len())
        .filter(|i| !blocks.get(i).is_some_and(|b| carries_owner_sentence(b)))
        .map(|i| LintAdvisory {
            class: "F1 ownership",
            criterion: Some(i),
            detail: "no OWNS/owner sentence; among three-plus criteria this is a twin-risk \
                     - a reviewer cannot tell this concern is not already claimed by a \
                     neighbor"
                .to_string(),
        })
        .collect()
}

/// Map from 1-based criterion index to the FULL text of that criterion's block - the
/// checkbox's own text plus every line inside its block ([`line_criterion`]'s boundary),
/// joined with spaces. Unlike [`extract_criteria`] (first physical line only), this
/// recovers text that sits on a wrapped continuation line or a sub-bullet, so a check like
/// [`carries_owner_sentence`] sees the whole criterion, not just its opening line.
fn criterion_blocks(text: &str) -> std::collections::BTreeMap<usize, String> {
    let owners = line_criterion(text);
    let mut out: std::collections::BTreeMap<usize, String> = std::collections::BTreeMap::new();
    for (line, owner) in text.lines().zip(owners) {
        let Some(idx) = owner else { continue };
        let body = checkbox_text(line).unwrap_or_else(|| line.trim());
        let entry = out.entry(idx).or_default();
        if !entry.is_empty() {
            entry.push(' ');
        }
        entry.push_str(body);
    }
    out
}

fn carries_owner_sentence(criterion: &str) -> bool {
    let lower = criterion.to_lowercase();
    find_word_across_hyphen(&lower, "owns").is_some() || affirmative_owner_occurs(&lower)
}

/// True when `lower` (already lowercased) contains a standalone "owner" occurrence that is
/// NOT the "owner" consumed by a "no owner" denial phrase elsewhere in the block. Round-4's
/// `denies_ownership` vetoed the WHOLE block the instant any of "no owner"/"ownerless"/"not
/// owned" appeared anywhere in it, even when a genuine, unrelated "OWNS"/"owner" sentence
/// sat elsewhere in the same block - exactly this unit's own governing spec (specs/66's
/// criterion 3 affirmatively OWNS its lint surface while separately describing "an
/// ownerless criterion" as fixture prose for the test it specifies). An affirmative match
/// anywhere in the block must win; only the SPECIFIC "owner" word a "no owner" phrase
/// consumes is excluded, never the block as a whole. "ownerless" and "not owned" need no
/// equivalent exclusion bookkeeping: neither ever produces a standalone "owner" match in
/// the first place ("ownerless" fuses "owner" straight into "less" with no boundary between
/// them, and "owned" is simply a different word from "owner"), so a block whose only owner
/// mention is one of those two phrases already fails this scan with no special-casing.
/// Matched with [`find_word_across_hyphen`] (not [`find_word`]) so a legitimate hyphenated
/// compound like "co-owner" still registers as a real ownership claim - the hyphen as
/// word-forming rule exists for phrase-boundary hedges like "self-worth considering", not
/// for a compound ownership noun.
fn affirmative_owner_occurs(lower: &str) -> bool {
    let denied = denied_owner_positions(lower);
    let mut start = 0;
    while let Some(rel) = find_word_across_hyphen(&lower[start..], "owner") {
        let pos = start + rel;
        if !denied.contains(&pos) {
            return true;
        }
        start = pos + "owner".len();
    }
    false
}

/// Byte positions, within `lower`, of the "owner" word that belongs to a "no owner" denial
/// phrase - excluded from [`affirmative_owner_occurs`]'s count so the denial suppresses
/// only the occurrence it actually consumes, not every "owner" in the block.
fn denied_owner_positions(lower: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(rel) = find_word_across_hyphen(&lower[start..], "no owner") {
        let pos = start + rel;
        out.push(pos + "no ".len());
        start = pos + "no owner".len();
    }
    out
}

/// F4 open dispositions (`docs/handbook/planning-field-guide.md`): "removed" and "ignored"
/// are different verdicts on the same files, and an unresolved disposition left in prose
/// is a rejection loop waiting to happen - the implementer picks one reading, a reviewer
/// picks the other. The countermeasure is grepping a draft for its own draft-smell
/// phrases; this lint runs that grep mechanically for the three the field guide names:
/// "worth considering", "either ... or", and "could instead". Scanned in PROSE only: the
/// `## Notes` section (and any deeper subsection under it) is the spec's explicit-deferral
/// home, so a hit there is exempt; a fenced code block, a backtick-quoted inline code span,
/// or a double-quoted span (the field guide's own convention for NAMING these exact
/// phrases, e.g. this spec's own Design bullet) can carry any of these words as literal
/// quoted text, so all three are excluded too - quoted or named text can never
/// false-positive.
pub fn disposition_advisories(text: &str) -> Vec<LintAdvisory> {
    let notes = notes_section_lines(text);
    let fenced = fenced_code_lines(text);
    let owners = line_criterion(text);
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    // Scan LOGICAL PARAGRAPHS, not physical lines: this repo hard-wraps prose, so a
    // hedge split across a wrap ("either X\nor Y") is one sentence to a reader and must
    // be one haystack to the lint (the same block-join reasoning `criterion_blocks`
    // applies for F1). A paragraph is a maximal run of unmasked, non-empty lines whose
    // continuations do not START a new structural element (bullet, heading, table row,
    // fence); paragraphs are DISJOINT, so a hedge is reported exactly once, attributed
    // to the paragraph's first line's criterion owner.
    let mut i = 0;
    while i < lines.len() {
        if notes[i] || fenced[i] || lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        let start = i;
        let mut joined = strip_inline_code(lines[i]);
        i += 1;
        while i < lines.len()
            && !notes[i]
            && !fenced[i]
            && !lines[i].trim().is_empty()
            && !starts_new_element(lines[i])
        {
            joined.push(' ');
            joined.push_str(&strip_inline_code(lines[i]));
            i += 1;
        }
        if let Some(phrase) = disposition_smell(&joined) {
            out.push(LintAdvisory {
                class: "F4 disposition",
                criterion: owners[start],
                detail: format!(
                    "open disposition (\"{phrase}\") outside Notes; decide it in Design or \
                     move it to Notes as an explicit deferral"
                ),
            });
        }
    }
    out
}

/// True when `line` begins a NEW structural element rather than continuing the previous
/// line's hard-wrapped sentence: a bullet (`- ` / `* `), a checkbox item, a heading, a
/// table row, or a fence opener. The paragraph joiner in [`disposition_advisories`]
/// breaks on these so two adjacent bullets never merge into one false haystack.
fn starts_new_element(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with('#')
        || t.starts_with('|')
        || t.starts_with("```")
}

/// The first draft-smell phrase `prose` contains, case-insensitively, or `None`.
fn disposition_smell(prose: &str) -> Option<&'static str> {
    let lower = prose.to_lowercase();
    if find_word(&lower, "worth considering").is_some() {
        return Some("worth considering");
    }
    if lower.contains("could instead") {
        return Some("could instead");
    }
    if either_or_hedge(&lower) {
        return Some("either ... or");
    }
    None
}

/// True when `lower` (already lowercased) contains a genuine, unresolved "either ... or"
/// hedge: a standalone "either" paired with a standalone "or" IN THE SAME CLAUSE (see
/// [`clause_end`]) that is not a decided-disposition sentence (see
/// [`is_decided_disposition`]). Both halves are matched as a STANDALONE word ([`find_word`]),
/// never a bare substring: "either" is itself a substring of "neither" (a "neither ... or"
/// sentence must never be misread as this pairing), and "or" is itself a substring of
/// ordinary words like "original", "order", or "orphan".
///
/// Scans every standalone "either" on the line, not only the first: "either" also has an
/// ordinary, non-disjunctive sense ("one of the two", e.g. specs/68's own "cannot bypass
/// either surface"), and stopping at the first occurrence would let that earlier,
/// non-disjunctive use shadow a real disjunction later on the same line - or, before the
/// per-clause bound below existed, wrongly pair with a faraway, unrelated standalone "or" in
/// a LATER clause (specs/68 criterion 1's "cannot bypass either surface; ... installs,
/// replaces, or modifies ..." - a live false-fire this exact shape produced,
/// `adj-u66c3-r5-reject-selfclean-live-violation`'s remedy plus a corpus-wide sweep found).
fn either_or_hedge(lower: &str) -> bool {
    let mut start = 0;
    while let Some(rel) = find_word(&lower[start..], "either") {
        let pos = start + rel;
        let clause = &lower[pos..clause_end(lower, pos)];
        if find_word(clause, "or").is_some() && !is_decided_disposition(lower, pos) {
            return true;
        }
        start = pos + "either".len();
    }
    false
}

/// The byte position, within `lower`, of the end of the grammatical clause that starts at
/// `from` - the next `.` or `;` after `from`, or `lower.len()` when neither appears. Bounds
/// the "either ... or" pairing search to ONE clause, the field guide's own countermeasure
/// describing a single disjunctive clause, not any two occurrences of the words anywhere on
/// a physical line regardless of how many unrelated sentences separate them.
fn clause_end(lower: &str, from: usize) -> usize {
    lower[from..]
        .find(['.', ';'])
        .map_or(lower.len(), |rel| from + rel)
}

/// True when the nearest word before the "either" match at `either_pos` (byte offset
/// into `lower`) is an unnegated "satisfied" - the field guide's own decided-disposition
/// idiom. "Satisfied either by A or by B" names two concrete, already-accepted satisfaction
/// paths (e.g. specs/68's Global constraints and every Done-when criterion: "each may be
/// satisfied either by fresh implementation or by independently re-verifying
/// already-integrated code ..., evidence bar = ..."); unlike a bare hedge ("Either the
/// daemon retries or it escalates"), it never poses an unresolved question about which
/// outcome occurs - both named paths are decided-acceptable. Round 5's REJECT
/// (`adj-u66c3-r5-reject-selfclean-live-violation`) found `disposition_smell` false-firing
/// on exactly this idiom, 5 times, on real committed spec prose.
///
/// The match is TOKEN-BASED, generalizing the round-6 rejects instead of patching a
/// seventh literal: the nearest WORD before "either" - punctuation between them ignored,
/// so the comma-separated form "satisfied, either by A or by B" reads as the same decided
/// idiom - must be "satisfied" exactly (a fused negation like "unsatisfied" is a
/// different token and stays an open hedge). NEGATION SCOPE: a negator among the two
/// tokens before "satisfied" ("not satisfied either", "not yet satisfied either")
/// cancels the exemption - an explicitly negated satisfaction is an OPEN question, the
/// exact opposite of a decided one.
fn is_decided_disposition(lower: &str, either_pos: usize) -> bool {
    let words = trailing_words(lower, either_pos, 3);
    if words.first() != Some(&"satisfied") {
        return false;
    }
    const NEGATORS: [&str; 6] = ["not", "never", "cannot", "no", "neither", "nor"];
    !words[1..].iter().any(|w| NEGATORS.contains(w))
}

/// The last `n` whole words (alphanumeric/hyphen runs) of `lower[..pos]`, nearest first.
/// The token walk behind [`is_decided_disposition`]: punctuation and whitespace between
/// words carry no meaning here, only the words themselves and their order.
fn trailing_words(lower: &str, pos: usize, n: usize) -> Vec<&str> {
    lower[..pos]
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty())
        .rev()
        .take(n)
        .collect()
}

/// The byte position of `word` as a STANDALONE word inside `haystack` (a character that is
/// neither alphanumeric nor a hyphen, or absent, on both sides), or `None`. Guards a plain
/// substring search from matching a fragment of a larger word - e.g. "either" inside
/// "neither", or "worth" as the tail of the hyphenated compound "self-worth". A hyphen
/// counts as WORD-FORMING (not a boundary) so a hyphenated compound is treated as one
/// token, the same way a reader parses it - otherwise "self-worth considering" would
/// misread its trailing "worth" as the standalone word this function exists to isolate.
fn find_word(haystack: &str, word: &str) -> Option<usize> {
    find_word_boundary(haystack, word, true)
}

/// Same search as [`find_word`], but a hyphen counts as a plain BOUNDARY, not a
/// word-forming character - so a legitimate hyphenated compound like "co-owner" splits
/// into separate tokens, letting a real ownership claim buried in it still match. Used
/// only where the hyphen-word-forming rule would wrongly hide a genuine standalone word
/// (owner/owns matching); [`find_word`]'s hyphen-word-forming rule exists for
/// phrase-boundary hedges like "self-worth considering", a different concern.
fn find_word_across_hyphen(haystack: &str, word: &str) -> Option<usize> {
    find_word_boundary(haystack, word, false)
}

/// Shared word-boundary walker behind [`find_word`] and [`find_word_across_hyphen`] - the
/// one substring-with-boundary-check algorithm in this file, parameterized on whether a
/// hyphen counts as word-forming rather than duplicated per caller.
fn find_word_boundary(haystack: &str, word: &str, hyphen_is_word_char: bool) -> Option<usize> {
    let is_word_char = |c: char| c.is_alphanumeric() || (hyphen_is_word_char && c == '-');
    let mut start = 0;
    while let Some(rel) = haystack[start..].find(word) {
        let pos = start + rel;
        let before_ok = haystack[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = haystack[pos + word.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok {
            return Some(pos);
        }
        start = pos + word.len();
    }
    None
}

/// Hygiene: a U+2014 em dash anywhere in the document - inside a criterion, Design prose,
/// or Notes alike, with NO exemption - is flagged now rather than discovered only when the
/// diff gate rejects the committed spec later (`.rigger/workflow.yml`'s em-dash check).
pub fn hygiene_advisories(text: &str) -> Vec<LintAdvisory> {
    let owners = line_criterion(text);
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains('\u{2014}'))
        .map(|(i, _)| LintAdvisory {
            class: "hygiene",
            criterion: owners[i],
            detail: "contains a U+2014 em dash; the diff gate rejects it - use a hyphen or \
                     rewrite the sentence"
                .to_string(),
        })
        .collect()
}

/// The full mechanical spec lint (spec 66): every shape advisory ([`spec_shape_advisories`]
/// above, mapped through with its ORIGINAL wording preserved verbatim so every existing
/// caller/test pinned to that wording keeps matching), plus ownership, open-disposition,
/// and hygiene advisories. This is the ONE surface `cmd_validate` calls for the pre-launch
/// spec lint - never a second, parallel aggregation.
pub fn spec_lint_advisories(text: &str) -> Vec<LintAdvisory> {
    let mut out: Vec<LintAdvisory> = spec_shape_advisories(text)
        .into_iter()
        .map(|a| LintAdvisory {
            class: match a.rule {
                ShapeRule::MultiBehavior => "F2 bundling",
                ShapeRule::SubBulletAsUnit | ShapeRule::OverLong => "F6 copyability",
            },
            // The criterion number is already named inside `detail` (ShapeAdvisory's own
            // Display), so it is not duplicated here.
            criterion: None,
            detail: a.to_string(),
        })
        .collect();
    out.extend(ownership_advisories(text));
    out.extend(disposition_advisories(text));
    out.extend(hygiene_advisories(text));
    out
}

/// For each line (0-based, aligned with `text.lines()`), whether it falls inside a
/// `## Notes` (or deeper) section - the section runs from that heading to the next heading
/// at the SAME OR SHALLOWER level, or to end of file.
fn notes_section_lines(text: &str) -> Vec<bool> {
    let mut out = Vec::with_capacity(text.lines().count());
    let mut notes_level: Option<usize> = None;
    for line in text.lines() {
        if let Some(level) = heading_level(line) {
            if let Some(nl) = notes_level {
                if level <= nl {
                    notes_level = None;
                }
            }
            let title = line.trim_start()[level..].trim();
            if title.to_lowercase().starts_with("notes") {
                notes_level = Some(level);
            }
        }
        out.push(notes_level.is_some());
    }
    out
}

/// The heading level (number of leading `#`s) of a markdown ATX heading line, or `None`.
fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        Some(hashes)
    } else {
        None
    }
}

/// For each line (0-based), whether it sits inside a fenced code block. The ` ``` `
/// delimiter lines themselves count as inside, so a phrase on the fence line is never
/// flagged.
fn fenced_code_lines(text: &str) -> Vec<bool> {
    let mut out = Vec::with_capacity(text.lines().count());
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            out.push(true);
        } else {
            out.push(fenced);
        }
    }
    out
}

/// `line` with every backtick-delimited OR double-quote-delimited span blanked to spaces
/// (never dropped, so word boundaries around the span cannot merge two words into a new
/// one), so a prose scan cannot match text quoted as code, nor a phrase NAMED in double
/// quotes - the field guide's own convention for listing its exact smell phrases (e.g.
/// specs/66's own Design bullet, which quotes all three: it must not trip its own lint).
/// Both delimiters share ONE toggle, keyed on the delimiter that opened the current span,
/// so a stray unmatched mark blanks the rest of the line rather than letting the other
/// delimiter re-open inside it.
fn strip_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut open: Option<char> = None;
    for ch in line.chars() {
        match open {
            Some(delim) if ch == delim => {
                open = None;
                out.push(' ');
            }
            Some(_) => out.push(' '),
            None if ch == '`' || ch == '"' => {
                open = Some(ch);
                out.push(' ');
            }
            None => out.push(ch),
        }
    }
    out
}

/// For each line (0-based), the 1-based Done-when criterion whose checkbox block it falls
/// inside (the checkbox's own line, or a more-indented line directly under it), or `None`
/// for a line outside any checkbox (headings, Design/Notes/Global-constraints prose, blank
/// lines). THE single block-boundary walk over the document: [`sub_bullet_criteria`] and
/// [`criterion_blocks`] are both built on this, rather than each re-deriving their own
/// count/open/indent state machine.
fn line_criterion(text: &str) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(text.lines().count());
    let mut count = 0usize;
    let mut open: Option<usize> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if checkbox_text(line).is_some() {
            count += 1;
            open = Some(indent);
            out.push(Some(count));
        } else if trimmed.is_empty() {
            out.push(None);
        } else if let Some(cb_indent) = open {
            if indent > cb_indent {
                out.push(Some(count));
            } else {
                open = None;
                out.push(None);
            }
        } else {
            out.push(None);
        }
    }
    out
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

    // -----------------------------------------------------------------------------------
    // Spec 66, unit c3: the mechanical planning-field-guide recipe as a lint - ownership
    // (F1), open dispositions (F4), and hygiene (em dash) - reusing extract_criteria /
    // checkbox_text (no second parser), surfaced through spec_lint_advisories.
    // -----------------------------------------------------------------------------------

    /// F1 ownership: below three criteria, the ownership check stays silent even when
    /// NONE of the criteria carry an OWNS/owner sentence - a collision needs at least two
    /// other criteria to collide with.
    #[test]
    fn ownership_check_is_silent_below_three_criteria() {
        let text = "## Done when\n\n\
            - [ ] the store passes the contract suite\n\
            - [ ] the graph projector supersedes an older decision\n";
        assert!(
            ownership_advisories(text).is_empty(),
            "two criteria must never draw an ownership advisory; got: {:?}",
            ownership_advisories(text)
        );
    }

    /// F1 ownership: at three-plus criteria, a checkbox with no "OWNS"/"owner" sentence is
    /// flagged `F1 ownership` naming the right criterion; a sibling that DOES carry one
    /// stays silent.
    #[test]
    fn ownership_check_flags_the_criterion_missing_an_owns_sentence() {
        let text = "## Done when\n\n\
            - [ ] the daemon writes a pidfile. This criterion OWNS the pidfile write.\n\
            - [ ] the store passes the contract suite\n\
            - [ ] the graph supersedes an older decision. This criterion OWNS the supersede path.\n";
        let advisories = ownership_advisories(text);
        let hit = advisories
            .iter()
            .find(|a| a.class == "F1 ownership")
            .expect("the ownerless criterion must be flagged F1 ownership");
        assert_eq!(hit.criterion, Some(2), "criterion 2 is the ownerless one");
        assert!(
            !advisories
                .iter()
                .any(|a| a.criterion == Some(1) || a.criterion == Some(3)),
            "criteria carrying an OWNS sentence must stay silent; got: {advisories:?}"
        );
    }

    /// F1 ownership: "owner" (not just "OWNS") also counts as an ownership sentence.
    #[test]
    fn ownership_check_accepts_the_word_owner() {
        let text = "## Done when\n\n\
            - [ ] the daemon writes a pidfile; no clear owner is named otherwise\n\
            - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
            - [ ] the graph supersedes an older decision. This criterion OWNS the supersede path.\n";
        assert!(
            !ownership_advisories(text)
                .iter()
                .any(|a| a.criterion == Some(1)),
            "the word owner must satisfy the ownership check"
        );
    }

    /// F1 ownership: an OWNS sentence on a WRAPPED CONTINUATION line (this repo's own
    /// standard Done-when convention - see specs/66-ship-the-planning-discipline.md's own
    /// Done-when list) must satisfy the check, not only one on the checkbox's first
    /// physical line. `extract_criteria`/`checkbox_text` only ever see that first line, so
    /// the ownership check must scan the criterion's FULL block (via `line_criterion`'s
    /// block boundary), not `extract_criteria`'s truncated text.
    #[test]
    fn ownership_check_finds_an_owns_sentence_on_a_wrapped_continuation_line() {
        let text = "## Done when\n\n\
            - [ ] the daemon writes a pidfile that is mode 0644 and readable only by the\n\
            \x20\x20service account. This criterion OWNS the pidfile permissions.\n\
            - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
            - [ ] the graph supersedes an older decision. This criterion OWNS the supersede \
            path.\n";
        assert!(
            !ownership_advisories(text)
                .iter()
                .any(|a| a.criterion == Some(1)),
            "criterion 1's OWNS sentence sits on a wrapped continuation line, not the \
             checkbox's first physical line - it must still satisfy the ownership check; \
             got: {:?}",
            ownership_advisories(text)
        );
    }

    /// `criterion_blocks` must join a checkbox's continuation lines with a SEPARATING
    /// SPACE, not concatenate them directly - a dropped separator can accidentally weld
    /// two words across a line break into one that spuriously satisfies
    /// `carries_owner_sentence`'s substring check (e.g. "own" + "ership" -> "ownership",
    /// which contains "owner").
    #[test]
    fn ownership_check_does_not_let_a_dropped_word_boundary_fake_an_owns_sentence() {
        let text = "## Done when\n\n\
            - [ ] the widget adopts a new own\n\
            \x20\x20ership model for the config\n\
            - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
            - [ ] the graph supersedes an older decision. This criterion OWNS the supersede \
            path.\n";
        assert!(
            ownership_advisories(text)
                .iter()
                .any(|a| a.criterion == Some(1)),
            "criterion 1 has no real OWNS/owner sentence - \"own\" and \"ership\" sit on \
             separate lines and must NOT be welded into a false \"ownership\" match; got: \
             {:?}",
            ownership_advisories(text)
        );
    }

    /// The sibling of the test above, closing the same defect class with a fixture the
    /// welded word "ownership" cannot exercise: "own" welded straight to "ership" still
    /// fails `find_word_across_hyphen`'s OWN after-boundary check (the trailing "ship"
    /// keeps it from matching standalone "owner"), so that fixture cannot tell a correct
    /// join from a dropped-separator weld apart. Splitting "own" from "er" instead welds
    /// into EXACTLY the five letters "owner" with nothing trailing - a weld
    /// `find_word_across_hyphen`'s boundary check cannot distinguish from a genuine
    /// standalone word, so only the separating space stands between this fixture and a
    /// false ownership claim.
    #[test]
    fn ownership_check_does_not_let_a_dropped_word_boundary_weld_own_and_er_into_owner() {
        let text = "## Done when\n\n\
            - [ ] the widget locks down its own\n\
            \x20\x20er and simpler path through the config\n\
            - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
            - [ ] the graph supersedes an older decision. This criterion OWNS the supersede \
            path.\n";
        assert!(
            ownership_advisories(text)
                .iter()
                .any(|a| a.criterion == Some(1)),
            "criterion 1 has no real OWNS/owner sentence - \"own\" and \"er\" sit on \
             separate lines and must NOT be welded into a false standalone \"owner\" match; \
             got: {:?}",
            ownership_advisories(text)
        );
    }

    /// F1 ownership: an explicit DENIAL of ownership ("no owner", "ownerless", "not
    /// owned") must NOT satisfy `carries_owner_sentence` just because it contains the bare
    /// substring "owner" - a criterion that says ownership has NOT been assigned is exactly
    /// the twin-risk case F1 exists to catch, so it must still be flagged, not read as
    /// carrying an ownership sentence.
    #[test]
    fn ownership_check_flags_a_criterion_that_explicitly_denies_ownership() {
        let text = "## Done when\n\n\
            - [ ] the daemon writes a pidfile. No owner has been assigned to this \
            criterion yet.\n\
            - [ ] the store passes the contract suite. This criterion OWNS the suite.\n\
            - [ ] the graph supersedes an older decision. This criterion OWNS the supersede \
            path.\n";
        let advisories = ownership_advisories(text);
        assert!(
            advisories.iter().any(|a| a.criterion == Some(1)),
            "an explicit ownership denial must still be flagged twin-risk, not misread as \
             carrying an ownership sentence; got: {advisories:?}"
        );
    }

    /// F1 ownership: "ownerless" and "not owned" are also explicit denials, not ownership
    /// sentences.
    #[test]
    fn ownership_check_flags_ownerless_and_not_owned_as_denials() {
        assert!(
            !carries_owner_sentence("this criterion is ownerless for now"),
            "\"ownerless\" is a denial, not an ownership sentence"
        );
        assert!(
            !carries_owner_sentence("the pidfile write is not owned by this criterion"),
            "\"not owned\" is a denial, not an ownership sentence"
        );
        assert!(
            carries_owner_sentence("this criterion OWNS the pidfile write"),
            "a genuine OWNS sentence must still satisfy the check"
        );
    }

    /// F1 ownership: "owns" and "owner" are themselves substrings of ordinary English
    /// words that carry no ownership claim at all - "owns" inside "drowns", "owner" inside
    /// "downer" - so a criterion using one of those unrelated words must not be misread as
    /// carrying a genuine OWNS/owner sentence just because the bare substring happens to
    /// appear inside a larger word. The same defect class already fixed twice in this file
    /// (either/or, then worth-considering); left unapplied here it is the identical corner
    /// cut one function away.
    #[test]
    fn ownership_check_does_not_match_owns_or_owner_inside_an_unrelated_word() {
        assert!(
            !carries_owner_sentence(
                "the retry handler drowns duplicate signals during a backoff storm"
            ),
            "\"drowns\" contains the substring \"owns\" but claims no ownership"
        );
        assert!(
            !carries_owner_sentence("a stale cache entry is a real downer for latency"),
            "\"downer\" contains the substring \"owner\" but claims no ownership"
        );
        assert!(
            carries_owner_sentence("this criterion OWNS the pidfile write"),
            "a genuine standalone OWNS must still satisfy the check"
        );
        assert!(
            carries_owner_sentence("no clear owner is named otherwise"),
            "a genuine standalone owner must still satisfy the check"
        );
    }

    /// Round-4 REJECT remedy (a) (`adj-u66c3-r4-reject-owner-veto-and-compound-hyphen-defects`):
    /// `denies_ownership` vetoed the WHOLE block the instant any denial phrase appeared
    /// anywhere in it, even when a completely separate, genuine "OWNS"/"owner" sentence sat
    /// elsewhere in the same block - exactly the shape of this unit's own governing spec
    /// (specs/66's criterion 3 affirmatively OWNS its lint surface while separately
    /// describing an "ownerless criterion" as fixture prose). An affirmative match
    /// elsewhere in the block must win; only the "owner" consumed by the "no owner" phrase
    /// itself is excluded.
    #[test]
    fn ownership_check_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere() {
        assert!(
            carries_owner_sentence(
                "this criterion OWNS the pre-launch lint surface. the test also builds an \
                 ownerless fixture to prove the check fires on it"
            ),
            "an unrelated \"ownerless\" mention describing a FIXTURE must not veto a real \
             OWNS sentence elsewhere in the same block"
        );
        assert!(
            carries_owner_sentence(
                "the pidfile write is not owned by criterion two. this criterion OWNS the \
                 daemon startup sequence"
            ),
            "an unrelated \"not owned\" mention must not veto a real OWNS sentence \
             elsewhere in the same block"
        );
        assert!(
            !carries_owner_sentence("no owner has been assigned to this criterion yet"),
            "a block whose ONLY owner mention is itself the \"no owner\" denial phrase must \
             still be flagged twin-risk, not read as carrying an ownership sentence"
        );
    }

    /// Round-4 REJECT remedy (b): a legitimate hyphenated compound noun like "co-owner"
    /// must still register as a real ownership claim. `find_word`'s hyphen-as-word-forming
    /// rule (added to fix "self-worth considering") exists for phrase-boundary hedges, not
    /// for a compound ownership noun - a genuine ownership claim buried in a hyphenated
    /// compound must not be silenced.
    #[test]
    fn ownership_check_recognizes_owner_inside_a_hyphenated_compound() {
        assert!(
            carries_owner_sentence("the co-owner of this criterion is the widget team"),
            "\"co-owner\" is a real ownership claim; the hyphen must not hide the standalone \
             \"owner\" inside it"
        );
    }

    /// F4 open dispositions: each of the three draft-smell phrases the field guide names
    /// is caught in prose - "worth considering", "could instead", and an "either ... or"
    /// pairing.
    #[test]
    fn disposition_check_catches_each_draft_smell_phrase() {
        let text = "## Design\n\n\
            The retry policy is worth considering for a future revision.\n\n\
            We could instead retry indefinitely.\n\n\
            Either the daemon retries or it escalates immediately.\n";
        let advisories = disposition_advisories(text);
        assert!(
            advisories
                .iter()
                .any(|a| a.detail.contains("worth considering")),
            "must catch \"worth considering\"; got: {advisories:?}"
        );
        assert!(
            advisories
                .iter()
                .any(|a| a.detail.contains("could instead")),
            "must catch \"could instead\"; got: {advisories:?}"
        );
        assert!(
            advisories.iter().any(|a| a.detail.contains("either")),
            "must catch the either...or pairing; got: {advisories:?}"
        );
        assert!(
            advisories.iter().all(|a| a.class == "F4 disposition"),
            "every disposition advisory carries the F4 disposition class; got: {advisories:?}"
        );
    }

    /// F4 open dispositions: "either" is a substring of "neither", so a sentence using
    /// "neither ... or" must NOT be misread as the "either ... or" draft-smell pairing
    /// just because "either" appears as a fragment of "neither".
    #[test]
    fn disposition_check_does_not_match_either_inside_neither() {
        let text = "## Design\n\nThis works in neither case A or case B.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "\"neither\" contains \"either\" as a substring; that must not false-fire the \
             either...or pairing; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// F4 open dispositions: "or" is itself a substring of ordinary words - "original",
    /// "order", "orphan" - so a standalone "either" earlier on the line must not make a
    /// LATER, unrelated "or"-prefixed word false-fire as the disjunction's second half. The
    /// fixture sentence has a real standalone "either" but no real disjunction at all.
    #[test]
    fn disposition_check_does_not_match_or_inside_a_later_word() {
        let text = "## Design\n\n\
            Either approach works well; the original design remains valid throughout.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "\"original\" contains \" or\" as a substring; that must not false-fire the \
             either...or pairing when there is no standalone \"or\" on the line; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// F4 open dispositions: "worth considering" must be matched as a STANDALONE phrase
    /// the same way either/or already are - a hyphenated compound noun like "self-worth"
    /// immediately followed by "considering" is ordinary prose, not the hedging
    /// disposition the check exists to catch, and must not false-fire just because the
    /// bare substring "worth considering" happens to appear across the hyphen boundary.
    #[test]
    fn disposition_check_does_not_match_worth_considering_across_a_hyphenated_compound() {
        let text = "## Design\n\n\
            A fair price reflects self-worth considering every relevant factor.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "\"self-worth\" is a hyphenated compound noun; its trailing \"worth\" followed \
             by \"considering\" must not false-fire the worth-considering draft-smell \
             phrase; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// F4 open dispositions: a smell phrase NAMED in double quotes - the field guide's own
    /// convention for listing the exact phrases it watches for (see specs/66's Design
    /// bullet, which quotes all three) - must not false-positive, the same as a backtick
    /// code span already does not.
    #[test]
    fn disposition_check_ignores_a_phrase_named_in_double_quotes() {
        let text = "## Design\n\nThe lint watches for draft-smell phrases: \"worth \
            considering\", \"either ... or\", \"could instead\".\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "a phrase NAMED in double quotes (not used as open prose) must not \
             false-positive; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// The double-quote/backtick exemption must properly CLOSE at its matching delimiter
    /// and resume normal scanning afterward - it must not blank the rest of the line once
    /// a span opens, or a genuine draft-smell phrase appearing later on the SAME line
    /// (outside the span) would be missed.
    #[test]
    fn disposition_check_resumes_scanning_after_a_quoted_span_closes_on_the_same_line() {
        let text = "## Design\n\nUse `example` here; this is worth considering separately.\n";
        assert!(
            disposition_advisories(text)
                .iter()
                .any(|a| a.detail.contains("worth considering")),
            "scanning must resume after the backtick span closes, catching the later \
             smell phrase on the same line; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// Round-5 REJECT remedy (`adj-u66c3-r5-reject-selfclean-live-violation`,
    /// `adv-u66c3-r5-f4-either-or-false-fires-on-a-decided-disposition-rule`): "satisfied
    /// either by A or by B" is the field guide's own decided-disposition idiom (specs/68's
    /// Global constraints and all four Done-when criteria use it verbatim) - it names two
    /// concrete, already-accepted satisfaction paths, not an open question about which
    /// outcome occurs, so it must never trip F4.
    #[test]
    fn disposition_check_does_not_match_a_satisfied_either_or_decided_disposition() {
        let text = "## Global constraints\n\n\
            - Disposition for criteria 1-4: each may be satisfied either by fresh \
            implementation or by independently re-verifying already-integrated code at the \
            run's base commit - the evidence bar for the re-verify path is rerunning that \
            criterion's own pinned tests plus both feature lanes.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "a \"satisfied either ... or ...\" decided-disposition sentence must not \
             false-fire F4; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// The decided-disposition exemption is scoped to the "satisfied either" idiom
    /// specifically, not to "either ... or" in general - a genuine unresolved hedge sitting
    /// right beside a decided one on a different line must still be flagged, proving the fix
    /// is not a blanket F4 suppression.
    #[test]
    fn disposition_check_still_flags_an_unresolved_hedge_beside_a_decided_disposition() {
        let text = "## Design\n\n\
            Disposition: satisfied either by A or by B, evidence bar named per path.\n\n\
            Separately, either the daemon retries or it escalates immediately - undecided.\n";
        let advisories = disposition_advisories(text);
        assert_eq!(
            advisories.len(),
            1,
            "only the genuine unresolved hedge must be flagged, not the decided sentence; \
             got: {advisories:?}"
        );
        assert!(advisories[0].detail.contains("either"));
    }

    /// The "satisfied" governing word must be the STANDALONE word immediately preceding
    /// "either" - a negated form like "unsatisfied either ... or ..." is a different word
    /// (word-boundary check, not a bare suffix match) and must still be read as an open
    /// hedge, not silently swallowed by the decided-disposition exemption.
    #[test]
    fn disposition_check_does_not_exempt_unsatisfied_either_or() {
        let text = "## Design\n\nthe criterion remains unsatisfied either by retry or by \
            escalation, undecided.\n";
        assert!(
            !disposition_advisories(text).is_empty(),
            "\"unsatisfied\" is a different word from \"satisfied\" - the exemption must not \
             match a bare suffix; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// Corpus-wide sweep residual (found reproducing round 5's fix on the real specs/68
    /// file, not named individually in the REJECT verdict): "either" also has an ordinary,
    /// non-disjunctive sense ("one of the two") with no "or" of its own - specs/68
    /// criterion 1's "cannot bypass either surface" - followed, in a LATER unrelated clause
    /// on the same physical line, by a genuine standalone "or" ("installs, replaces, or
    /// modifies"). Before the either...or pairing was bounded to one clause, this unrelated
    /// pair false-fired F4. It must not.
    #[test]
    fn disposition_check_does_not_pair_a_non_disjunctive_either_with_a_faraway_unrelated_or() {
        let text = "## Design\n\nan entry cannot bypass either surface; a test also proves \
            an agent never installs, replaces, or modifies the operator's binary.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "\"either surface\" has no \"or\" of its own; a faraway, unrelated \"or\" in a \
             later clause must not be misread as its pair; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// The clause bound must not swallow a GENUINE disjunction that follows a
    /// non-disjunctive "either" earlier on the same line - the scan must keep looking past
    /// the first, non-paired "either" rather than stopping there.
    #[test]
    fn disposition_check_finds_a_genuine_hedge_after_an_earlier_non_disjunctive_either() {
        let text = "## Design\n\nan entry cannot bypass either surface; either the daemon \
            retries or it escalates, undecided.\n";
        let advisories = disposition_advisories(text);
        assert!(
            advisories.iter().any(|a| a.detail.contains("either")),
            "a genuine hedge later on the line must still be caught even though an earlier, \
             non-disjunctive \"either\" precedes it; got: {advisories:?}"
        );
    }

    /// A bare "either ... or" hedge with no governing "satisfied" word at all is unaffected
    /// by the exemption and still flags - the baseline case the exemption must not weaken.
    #[test]
    fn disposition_check_still_flags_a_bare_either_or_with_no_satisfied_word() {
        let text = "## Design\n\nreindex either retires or re-points to the symbol index, \
            whichever the surviving command surface makes honest.\n";
        assert!(
            !disposition_advisories(text).is_empty(),
            "a bare either...or hedge with no \"satisfied\" governing word must still be \
             flagged; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// The double-quote exemption must track its OPENING delimiter all the way to the
    /// matching CLOSE, not merely blank a couple of characters after the opening mark -
    /// padding right after the quote (before the phrase itself starts) must not let the
    /// phrase later in the same span leak into the prose scan.
    #[test]
    fn disposition_check_ignores_a_wide_double_quoted_span() {
        let text =
            "## Design\n\nThe phrase is named here: \" worth considering \" as an example.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "the whole double-quoted span must be exempt regardless of its width; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// F4 open dispositions: a smell phrase inside the `## Notes` section is an explicit,
    /// intentional deferral - it must NOT be flagged.
    #[test]
    fn disposition_check_is_silent_inside_notes() {
        let text = "## Design\n\nsettled prose, nothing open.\n\n\
            ## Notes (non-criteria)\n\n\
            - worth considering for a later revision, deliberately deferred here.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "a smell phrase inside Notes is an explicit deferral, not an open disposition; \
             got: {:?}",
            disposition_advisories(text)
        );
    }

    /// F4 open dispositions: the Notes exemption ends at the next heading - prose AFTER
    /// Notes is scanned again.
    #[test]
    fn disposition_check_resumes_scanning_after_notes_ends() {
        let text = "## Notes (non-criteria)\n\nworth considering, deferred.\n\n\
            ## Global constraints\n\nit is worth considering here too.\n";
        let advisories = disposition_advisories(text);
        assert_eq!(
            advisories.len(),
            1,
            "only the occurrence AFTER Notes ends must be flagged; got: {advisories:?}"
        );
    }

    /// F4 open dispositions: a smell phrase quoted inside a fenced code block or an inline
    /// code span must never false-positive.
    #[test]
    fn disposition_check_ignores_fenced_and_inline_code() {
        let text = "## Design\n\n\
            ```\nworth considering as literal example text\n```\n\n\
            The config carries `either this or that` as a literal token, unrelated prose.\n";
        assert!(
            disposition_advisories(text).is_empty(),
            "quoted code must never false-positive; got: {:?}",
            disposition_advisories(text)
        );
    }

    /// F4 open dispositions: a smell phrase inside a Done-when checkbox is attributed to
    /// that criterion.
    #[test]
    fn disposition_check_attributes_a_hit_inside_a_criterion() {
        let text = "## Done when\n\n\
            - [ ] the store passes the contract suite\n\
            - [ ] either the recovery path retries or it escalates immediately\n";
        let advisories = disposition_advisories(text);
        let hit = advisories
            .iter()
            .find(|a| a.class == "F4 disposition")
            .expect("the either...or checkbox must be flagged");
        assert_eq!(hit.criterion, Some(2));
    }

    /// `starts_new_element` recognizes EACH of its five prefix kinds independently
    /// (`d-u66c3-mutation-starts-new-element-gap`): every existing `disposition_advisories`
    /// scenario only ever exercises a "- " dash bullet (the only bullet mark this repo's own
    /// Done-when checkboxes use) or a fence line (which the paragraph loop's separate
    /// `fenced[i]` check already excludes regardless of what this function returns for it),
    /// so mutation testing surfaced three surviving `||` -> `&&` mutants spanning the '#',
    /// '|', and "```" arms of the boolean chain - none of the five-term `||` chain's terms
    /// beyond the first "- "/"* " pair was ever independently proven true on its own. Each
    /// assertion below fixes exactly one term true with the other four false, which
    /// (boolean algebra, since a line's first character satisfies at most one of these five
    /// prefixes) forces every possible single-`||`-mutated-to-`&&` variant of the chain to
    /// disagree with the correct `true` result for at least one assertion here.
    #[test]
    fn starts_new_element_recognizes_every_prefix_kind_independently() {
        assert!(starts_new_element("- a dash bullet"), "dash bullet");
        assert!(starts_new_element("* a star bullet"), "star bullet");
        assert!(starts_new_element("# a heading"), "heading");
        assert!(starts_new_element("| a table row |"), "table row");
        assert!(starts_new_element("```a fence opener"), "fence opener");
        assert!(
            !starts_new_element("plain prose with none of the five prefixes"),
            "plain prose must not start a new element"
        );
    }

    /// Hygiene: a U+2014 em dash is flagged anywhere in the document - Design prose here,
    /// with NO criterion attribution since it sits outside any checkbox.
    #[test]
    fn hygiene_check_flags_an_em_dash_in_prose() {
        let text = "## Design\n\nthe daemon starts \u{2014} then it writes a pidfile.\n";
        let advisories = hygiene_advisories(text);
        let hit = advisories
            .iter()
            .find(|a| a.class == "hygiene")
            .expect("a U+2014 em dash in prose must be flagged");
        assert_eq!(hit.criterion, None);
    }

    /// Hygiene: an em dash INSIDE a Done-when checkbox is attributed to that criterion -
    /// no exemption for criteria, Notes, or code, unlike the disposition check.
    #[test]
    fn hygiene_check_attributes_a_hit_inside_a_criterion() {
        let text = "## Done when\n\n\
            - [ ] the store passes the contract suite\n\
            - [ ] the report renders a summary line \u{2014} appended at the end\n";
        let advisories = hygiene_advisories(text);
        let hit = advisories
            .iter()
            .find(|a| a.class == "hygiene")
            .expect("the em dash inside criterion 2 must be flagged");
        assert_eq!(hit.criterion, Some(2));
    }

    /// `line_criterion` attributes an INDENTED CONTINUATION line to its checkbox even when
    /// the checkbox's own line is much LONGER than the continuation line - pinning genuine
    /// indent arithmetic (`line.len() - trimmed.len()`) rather than a formula that happens
    /// to agree with real indentation only when longer lines are also more indented (e.g.
    /// `line.len() + trimmed.len()` would invert the block boundary here).
    #[test]
    fn line_criterion_attributes_a_short_continuation_under_a_long_checkbox() {
        let text = "- [ ] the store passes the full end-to-end contract suite across every \
                     adapter\n\
                     \x20\x20a short tail\n";
        assert_eq!(line_criterion(text), vec![Some(1), Some(1)]);
    }

    /// `line_criterion` closes a checkbox's block on a DEDENT TO THE SAME indent, not only
    /// on an outdent - pinning the strict `>` comparison (not `>=`) so a plain line back at
    /// the checkbox's own margin is prose, not the block's continuation.
    #[test]
    fn line_criterion_closes_the_block_on_a_same_indent_line() {
        let text = "- [ ] the store passes the contract suite\n\
                     a line back at the same margin\n";
        assert_eq!(line_criterion(text), vec![Some(1), None]);
    }

    /// A markdown heading carries at most six `#`s (CommonMark); seven or more - even
    /// followed by a space, which otherwise looks exactly like a heading - is NOT one, so
    /// it can never open (or close) a `## Notes` section by accident.
    #[test]
    fn heading_level_rejects_more_than_six_hashes() {
        assert_eq!(heading_level("####### Notes"), None);
    }

    /// Hygiene: a document with no em dash draws no hygiene advisory.
    #[test]
    fn hygiene_check_is_silent_without_an_em_dash() {
        let text = "## Design\n\nplain hyphen-only prose, no unicode dash at all.\n";
        assert!(hygiene_advisories(text).is_empty());
    }

    /// spec_lint_advisories is the ONE combined surface `cmd_validate` calls: on a fixture
    /// carrying all four Done-when-c3 defect kinds (a multi-behavior criterion, an
    /// ownerless criterion among three-plus, a disposition smell, and an em dash), it
    /// reports each with a criterion and a field-guide class, and stays silent on a clean
    /// fixture.
    #[test]
    fn spec_lint_advisories_reports_every_defect_with_its_criterion_and_class() {
        let text = "# Widget\n\n## Done when\n\n\
            - [ ] the daemon starts on boot, and it writes a pidfile, and it rotates the log \
            nightly. This criterion OWNS the startup sequence.\n\
            - [ ] the store passes the contract suite\n\
            - [ ] either the recovery path retries or it escalates immediately. This \
            criterion OWNS the recovery path.\n\
            - [ ] the report renders a trailing summary line \u{2014} appended at the end. \
            This criterion OWNS the summary render.\n";
        let advisories = spec_lint_advisories(text);

        let shape_hit = advisories
            .iter()
            .find(|a| a.detail.contains("multi-behavior"))
            .expect("criterion 1's multi-behavior defect must be reported");
        assert_eq!(shape_hit.class, "F2 bundling");

        let ownership_hit = advisories
            .iter()
            .find(|a| a.class == "F1 ownership")
            .expect("criterion 2's missing OWNS sentence must be reported");
        assert_eq!(ownership_hit.criterion, Some(2));

        let disposition_hit = advisories
            .iter()
            .find(|a| a.class == "F4 disposition")
            .expect("criterion 3's either...or smell must be reported");
        assert_eq!(disposition_hit.criterion, Some(3));

        let hygiene_hit = advisories
            .iter()
            .find(|a| a.class == "hygiene")
            .expect("criterion 4's em dash must be reported");
        assert_eq!(hygiene_hit.criterion, Some(4));

        // Every advisory prints with SOME class label (never blank).
        for a in &advisories {
            assert!(!a.class.is_empty());
            assert!(!a.to_string().is_empty());
        }
    }

    /// spec_lint_advisories reports a clean fixture (three-plus criteria, each carrying an
    /// OWNS sentence, single-behavior, no smells, no em dash) as fully clean.
    #[test]
    fn spec_lint_advisories_is_silent_on_a_clean_fixture() {
        let text = "# Widget\n\n## Done when\n\n\
            - [ ] the store passes the contract suite. This criterion OWNS the contract \
            coverage.\n\
            - [ ] the graph projector supersedes an older decision. This criterion OWNS the \
            supersede path.\n\
            - [ ] the conductor integrates an approved unit. This criterion OWNS the \
            integration step.\n";
        assert!(
            spec_lint_advisories(text).is_empty(),
            "a fully clean fixture must draw no advisory; got: {:?}",
            spec_lint_advisories(text)
        );
    }

    /// Determinism: calling spec_lint_advisories twice on the same text yields identical
    /// output (no map iteration / unordered collection anywhere in the pipeline).
    #[test]
    fn spec_lint_advisories_is_deterministic() {
        let text = "## Done when\n\n\
            - [ ] the daemon writes a pidfile, and it rotates the log, and it archives it\n\
            - [ ] the store passes the contract suite\n\
            - [ ] either it retries or it escalates \u{2014} immediately\n";
        assert_eq!(spec_lint_advisories(text), spec_lint_advisories(text));
    }
}
