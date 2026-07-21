//! The design-intent extraction pass (spec 29b): classify a doc into design-intent concepts and
//! scan source files for `# WHY:` / `# NOTE:` rationale. It lowers raw doc bytes into the
//! parser-free [`DesignConcept`] model that [`super::events`] serializes into
//! `DocConceptExtracted` events. Feature-gated behind `symbols` (mirroring the code extractor): the
//! always-compiled fold folds the events, but PRODUCING them is extraction and stays out of the
//! light lane.
//!
//! Criterion 1 owns the four design-intent node KINDS and the baseline kind-classification that
//! maps a design-intent doc to one of them ([`extract_concepts`]). Criterion 2 extends the same
//! per-file pass with the design-intent LINKS ([`extract_links`]): a doc's inline-code code-path
//! mentions and markdown citations, and a source file's rationale sites, lowered into the
//! [`DesignLink`]s that [`super::events`] serializes into `DocLinkExtracted` events. Criterion 3 adds
//! the design-vs-usage SCOPE gate ([`is_usage_doc`]): a markdown doc written for END USERS - how to
//! DRIVE the tool - is out of scope and yields neither concepts nor links, so no event ever emits and
//! no node or edge ever folds for it, while a design/architecture doc is ingested. The gate is the
//! ONE scope authority, consulted by BOTH public entry points on the emit side; a source file's
//! inline `# WHY:` rationale is design intent wherever the code lives and is never gated.

use std::collections::BTreeSet;

use crate::grounder::design::model::{ConceptKind, DesignConcept, DesignLink, LinkRel};

/// Extract every design-intent concept from one file at relative path `path` with `contents`.
///
/// - A markdown doc (`.md` / `.markdown`) is classified into ONE design-intent kind by its path /
///   heading and emitted as a whole-doc node; a `design-doc` additionally emits one node per `##`
///   section, so "a reference-architecture doc becomes design-doc nodeS" and a later criterion can
///   link the SECTION that designed a subsystem.
/// - Any other (source) file is scanned for `WHY:` / `NOTE:` rationale comments, each emitted as a
///   `rationale` node keyed by its source site.
///
/// Deterministic by construction: the concepts are a pure function of `path` + `contents`, so
/// identical input yields identical concepts (the emit sorts them into a byte-stable order).
pub fn extract_concepts(path: &str, contents: &str) -> Vec<DesignConcept> {
    if is_markdown(path) {
        // Scope gate (spec 29b criterion 3): only design/architecture knowledge is ingested. A doc
        // written for END USERS - how to DRIVE the tool - is out of scope and yields ZERO concepts,
        // so nothing ever emits and no node ever folds for it. The gate is on the EMIT side and
        // wraps the c1 classification: a usage doc simply produces nothing to lower into events.
        if is_usage_doc(path, contents) {
            return Vec::new();
        }
        doc_concepts(path, contents)
    } else {
        // A source file is never a usage doc - inline `# WHY:` rationale is design intent wherever
        // the code lives - so it is always scanned for rationale (a file with none yields nothing).
        rationale_concepts(path, contents)
    }
}

/// Extract every design-intent LINK from one file at relative path `path` with `contents` (spec 29b
/// criterion 2). Extends the per-file pass with the design-intent EDGES:
///
/// - a markdown design-intent doc yields, from its whole-doc node, a kind-specific design->code link
///   for each inline-code CODE path it mentions - `design-doc --SPECIFIES--> code`,
///   `arch-decision --CONSTRAINS--> code`, `handbook-rule --GOVERNS--> code` (reusing `GOVERNS`) -
///   and, for a `design-doc`, a `references` link for each markdown link / citation target
///   (doc->doc or doc->code);
/// - a source file yields one `rationale --explains--> code` link per `# WHY:` / `# NOTE:` site, to
///   the file it annotates (the same `<file>#L<line>` id [`extract_concepts`] gives the rationale
///   node, so the edge lands on that node).
///
/// Deterministic and deduplicated: the links are a pure function of `path` + `contents`, gathered
/// into a `BTreeSet` so identical input yields the identical link set in a stable order (the emit
/// re-sorts into a byte-stable event order). Only the doc's OWN design intent links out - the inline
/// code carrier means "this doc is about this code", the markdown-link carrier means "this doc cites
/// that resource"; fenced code EXAMPLES are skipped, so a code block demonstrating a path is never
/// mistaken for the doc specifying it.
pub fn extract_links(path: &str, contents: &str) -> Vec<DesignLink> {
    let mut links: BTreeSet<DesignLink> = BTreeSet::new();
    if is_markdown(path) {
        // Scope gate (spec 29b criterion 3): the SAME design-vs-usage authority that gates
        // `extract_concepts`. A usage doc yields ZERO links too, so no `DocLinkExtracted` ever emits
        // and no spurious design-intent edge - nor the bare-artifact endpoint node its fold would
        // ensure - ever lands, so "a usage doc produces no nodes" holds across the WHOLE emit
        // surface, not just the concepts. One scope authority, consulted at both entry points.
        if is_usage_doc(path, contents) {
            return Vec::new();
        }
        doc_links(path, contents, &mut links);
    } else {
        rationale_links(path, contents, &mut links);
    }
    links.into_iter().collect()
}

/// Whether a markdown doc is an END-USER usage doc (how to DRIVE the tool) rather than
/// design/architecture intent, and so out of scope (spec 29b criterion 3). Usage docs describe
/// usage, not design, and would add noise to code-grounded traversal, so the extraction emits
/// nothing for one - neither a concept nor a link, so no event and no node or edge ever folds.
///
/// Design intent is the DEFAULT: the reference architecture, `architecture.md`, the addenda,
/// load-bearing decisions, and spec-shape / loop-discipline rules are all design intent, and
/// dropping a real design doc would defeat the spec, so only a doc carrying a clear end-user usage
/// signal AND no design-intent signal is gated out. Three layers guard against a false drop:
///
/// 1. An UNAMBIGUOUS design-structural doc (the reference architecture / an addendum / an ADR /
///    decision / a spec / the `design-intent-gaps` ledger) is design by construction and is never
///    gated, even when its path or heading carries a usage word (an ADR about the install flow, an
///    addendum whose title mentions usage).
/// 2. A handbook holds BOTH loop-discipline rules (design) AND end-user guides (usage), so a
///    handbook path is not a blanket structural override (layer 1). Instead, WITHIN a handbook path
///    the decision is CONTENT-aware: a doc whose content carries a loop-discipline / spec-shape rule
///    signal (`discipline`, `spec shape`, a `load-bearing` decision, `blast-radius` isolation, an
///    `invariant`, `fail-closed` review) is a rule doc and stays in, folding a `handbook-rule` node.
///    So the real repo file `docs/handbook/using-rigger.md` - an operating-discipline doc whose path
///    opens with `using-` and heading with "Using rigger" - stays in on its rule CONTENT (the drop
///    is NOT keyed on the `using-` filename prefix), while a PURE end-user guide under the same
///    handbook path (a usage signal, no rule content) still gates out at layer 3.
/// 3. Otherwise a clear end-user usage signal in the path or the first heading gates the doc out.
///
/// Scope (in / out) and the c1 kind classification (which of the four design kinds) are DISTINCT
/// concerns with one authority each; this predicate never assigns a kind, and `classify_doc` never
/// decides scope. The content signals here recognize design INTENT for the scope decision (whether
/// to gate), not a doc's KIND - they never re-derive `classify_doc`.
fn is_usage_doc(path: &str, contents: &str) -> bool {
    let p = path.to_lowercase();
    let heading = first_heading(contents).unwrap_or_default().to_lowercase();
    // Layer 1: an unambiguous structural design doc is never usage.
    if is_structural_design_doc(&p, &heading) {
        return false;
    }
    // Layer 2: within a handbook path, a doc whose content carries a loop-discipline / spec-shape
    // rule signal is a design rule doc and is never gated, even when its name carries a usage word.
    // Scoped to the handbook path (where design rules and end-user guides genuinely coexist) so the
    // scope decision elsewhere is unchanged - a plain usage doc that mentions a design word in prose
    // is not silently kept.
    if is_handbook_path(&p) && carries_design_rule_signal(contents) {
        return false;
    }
    // Layer 3: else a clear end-user usage signal in the path or first heading gates it out.
    USAGE_PATH_SIGNALS.iter().any(|s| p.contains(s))
        || USAGE_HEADING_SIGNALS.iter().any(|s| heading.contains(s))
}

/// Whether `path_lower` lives under a handbook - the one doc tree that mixes design rules with
/// end-user guides, so its scope decision is resolved by content (layer 2 of [`is_usage_doc`])
/// rather than by the path alone.
fn is_handbook_path(path_lower: &str) -> bool {
    path_lower.contains("handbook")
}

/// Whether the doc's content marks it as a loop-discipline / spec-shape RULE doc - an
/// operating-discipline document rather than an end-user guide. Keyed on the doc's CONTENT (heading
/// and body), never its filename, so a rule doc under a handbook path stays in scope even when its
/// name carries a usage word, while a pure end-user guide (no rule content) does not. The signals
/// are design vocabulary that a plain install / quick-start / FAQ guide would not carry, so this
/// recognizes design intent without false-keeping an actual usage doc.
fn carries_design_rule_signal(contents: &str) -> bool {
    let c = contents.to_lowercase();
    DESIGN_RULE_CONTENT_SIGNALS.iter().any(|s| c.contains(s))
}

/// Whether the path / first heading mark a doc as unambiguous design intent by its STRUCTURE - the
/// reference architecture, an addendum, an ADR / decision, a spec, or the `design-intent-gaps`
/// ledger. Such a doc is never gated as usage even if it carries a usage word. Deliberately narrower
/// than `classify_doc`'s signals: `handbook` / `rule` are excluded as PATH overrides, because those
/// dirs mix design rules with end-user guides. A rule doc under such a path is instead kept by the
/// CONTENT-aware layer ([`carries_design_rule_signal`]), so a pure end-user guide there still gates
/// out while a loop-discipline rule doc stays in.
fn is_structural_design_doc(path_lower: &str, heading_lower: &str) -> bool {
    STRUCTURAL_DESIGN_PATH_SIGNALS
        .iter()
        .any(|s| path_lower.contains(s))
        || heading_lower.contains("reference architecture")
        || heading_lower.starts_with("adr")
}

/// Path substrings that mark a doc as unambiguous design intent (layer 1 of the scope gate). Each is
/// a directory / filename marker (a `specs/` dir, a `spec-` prefix), not a bare word, so an ordinary
/// term (`inspection`, `specific`) never trips the design override.
const STRUCTURAL_DESIGN_PATH_SIGNALS: [&str; 8] = [
    "architecture",
    "addendum",
    "/adr",
    "adr-",
    "design-intent",
    "decision",
    "specs/",
    "spec-",
];

/// Content substrings that mark a doc as a loop-discipline / spec-shape RULE doc (layer 2 of the
/// scope gate, applied within a handbook path). Each is design vocabulary an end-user install /
/// quick-start / FAQ guide would not carry, so matching one keeps a real rule doc in scope without
/// false-keeping an actual usage doc. Matched against the whole (lowercased) doc content, heading
/// and body alike, so the signal is content-aware and never keyed on the filename.
const DESIGN_RULE_CONTENT_SIGNALS: [&str; 9] = [
    "discipline",
    "spec shape",
    "spec-shape",
    "load-bearing",
    "load bearing",
    "blast radius",
    "blast-radius",
    "invariant",
    "fail-closed",
];

/// Path substrings that mark a doc as an end-user usage doc (layer 3 of the scope gate). Precise
/// enough that a design doc's path is unlikely to match; a structural design doc is protected by
/// layer 1 and a rule doc by layer 2 regardless.
const USAGE_PATH_SIGNALS: [&str; 15] = [
    "readme",
    "using-",
    "usage",
    "getting-started",
    "getting_started",
    "quickstart",
    "quick-start",
    "tutorial",
    "how-to",
    "howto",
    "user-guide",
    "user_guide",
    "installation-guide",
    "faq",
    "troubleshooting",
];

/// First-heading substrings that mark a doc as an end-user usage doc (layer 3 of the scope gate).
const USAGE_HEADING_SIGNALS: [&str; 15] = [
    "usage",
    "getting started",
    "quick start",
    "quickstart",
    "tutorial",
    "how to ",
    "how-to",
    "user guide",
    "user's guide",
    "installing",
    "installation",
    "using ",
    "frequently asked",
    "command reference",
    "troubleshooting",
];

/// The design->code and citation links of a markdown design-intent doc. The doc's kind (criterion 1
/// classification) picks the single design->code relation for its inline-code code-path mentions;
/// a `design-doc` additionally emits a `references` link per markdown citation target.
fn doc_links(path: &str, contents: &str, out: &mut BTreeSet<DesignLink>) {
    let kind = classify_doc(path, contents);
    let code_rel = match kind {
        ConceptKind::DesignDoc => LinkRel::Specifies,
        ConceptKind::ArchDecision => LinkRel::Constrains,
        ConceptKind::HandbookRule => LinkRel::Governs,
        // classify_doc never returns Rationale for a markdown doc; stay total, emit no code links.
        ConceptKind::Rationale => return,
    };
    let from = path.to_string();
    for line in unfenced_lines(contents) {
        // Inline-code CODE-path mentions -> the doc's kind-specific design->code relation ("this
        // doc designs / constrains / governs this code").
        for span in inline_code_spans(line) {
            if is_code_path(&span) {
                out.insert(DesignLink {
                    from: from.clone(),
                    rel: code_rel,
                    to: span,
                });
            }
        }
        // Markdown link / citation targets -> `references` (doc->doc or doc->code), from a
        // `design-doc` only (the criterion's FROM kind for `references`).
        if kind == ConceptKind::DesignDoc {
            for target in link_targets(line) {
                if is_repo_path(&target) {
                    out.insert(DesignLink {
                        from: from.clone(),
                        rel: LinkRel::References,
                        to: target,
                    });
                }
            }
        }
    }
}

/// The `explains` links of a source file: one per `# WHY:` / `# NOTE:` rationale site, from the
/// `<file>#L<line>` rationale node (the SAME id [`extract_concepts`] gives it) to the file it
/// annotates - so the rationale's design intent is reachable from its code.
fn rationale_links(path: &str, contents: &str, out: &mut BTreeSet<DesignLink>) {
    for (i, line) in contents.lines().enumerate() {
        if rationale_in_line(line).is_some() {
            out.insert(DesignLink {
                from: format!("{path}#L{}", i + 1),
                rel: LinkRel::Explains,
                to: path.to_string(),
            });
        }
    }
}

/// The lines of a markdown doc OUTSIDE a fenced code block (` ``` ` / `~~~`), and never the fence
/// markers themselves - so a code EXAMPLE that happens to contain a path is not mistaken for the doc
/// specifying that code. Deterministic: a pure function of the contents.
fn unfenced_lines(contents: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in contents.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push(line);
        }
    }
    out
}

/// Every backtick-delimited inline code span on a line, in order (its content, without the
/// backticks). An unterminated span is ignored. The reliable, unambiguous carrier for a path the
/// doc mentions inline.
fn inline_code_spans(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            match chars[i + 1..].iter().position(|&c| c == '`') {
                Some(off) => {
                    let j = i + 1 + off;
                    let span: String = chars[i + 1..j].iter().collect();
                    if !span.is_empty() {
                        out.push(span);
                    }
                    i = j + 1;
                    continue;
                }
                None => break,
            }
        }
        i += 1;
    }
    out
}

/// Every markdown link / image target on a line, in order (the `target` of `[text](target)` or
/// `[text](target "title")`, the leading whitespace-delimited token). The reliable carrier for a
/// doc citation.
fn link_targets(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == '(' {
            match chars[i + 2..].iter().position(|&c| c == ')') {
                Some(off) => {
                    let j = i + 2 + off;
                    let raw: String = chars[i + 2..j].iter().collect();
                    if let Some(target) = raw.split_whitespace().next() {
                        if !target.is_empty() {
                            out.push(target.to_string());
                        }
                    }
                    i = j + 1;
                    continue;
                }
                None => break,
            }
        }
        i += 1;
    }
    out
}

/// Whether `s` (an inline-code span) names a CODE path the doc designs: a repo-relative path with a
/// filename extension, that is NOT a markdown doc (a `.md` mention is a citation, not a design->code
/// link) and NOT a URL. Requiring a `/` and a `.`-bearing final segment keeps a bare identifier or a
/// `foo::bar` module path in inline code from being mistaken for a file.
fn is_code_path(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) || s.contains("://") {
        return false;
    }
    let path = s.split('#').next().unwrap_or(s);
    if !path.contains('/') || is_doc_ext(path) {
        return false;
    }
    let last = path.rsplit('/').next().unwrap_or(path);
    last.contains('.')
}

/// Whether `target` (a markdown link target) is a repo-relative citation path, not an external URL,
/// a `mailto:` / `tel:` link, or an in-page `#anchor`. A doc->doc / doc->code citation.
fn is_repo_path(target: &str) -> bool {
    let s = target.trim();
    if s.is_empty() || s.starts_with('#') {
        return false;
    }
    let lower = s.to_lowercase();
    if lower.contains("://") || lower.starts_with("mailto:") || lower.starts_with("tel:") {
        return false;
    }
    s.contains('/') || s.contains('.')
}

/// Whether `p` (a path, ignoring any `#anchor`) is a markdown doc.
fn is_doc_ext(p: &str) -> bool {
    let p = p.split('#').next().unwrap_or(p).to_lowercase();
    p.ends_with(".md") || p.ends_with(".markdown")
}

/// Whether `path` is a markdown doc (the design-intent doc surface). Everything else is scanned for
/// inline rationale instead.
fn is_markdown(path: &str) -> bool {
    let p = path.to_lowercase();
    p.ends_with(".md") || p.ends_with(".markdown")
}

/// Classify a markdown doc into its design-intent concept(s): a whole-doc node plus, for a
/// `design-doc`, one node per top-level `##` section.
fn doc_concepts(path: &str, contents: &str) -> Vec<DesignConcept> {
    let kind = classify_doc(path, contents);
    let title = first_heading(contents).unwrap_or_else(|| file_stem(path).to_string());
    let mut out = vec![DesignConcept {
        kind,
        id: path.to_string(),
        title,
        doc: path.to_string(),
    }];
    // A reference-architecture / design doc also contributes SECTION nodes - the granular unit an
    // agent reaches ("the RA section that designed this subsystem"). A decision / rule doc is one
    // node (it is atomic design intent, not a sectioned reference document).
    if kind == ConceptKind::DesignDoc {
        for heading in section_headings(contents) {
            out.push(DesignConcept {
                kind: ConceptKind::DesignDoc,
                id: format!("{path}#{}", slug(&heading)),
                title: heading,
                doc: path.to_string(),
            });
        }
    }
    out
}

/// The baseline design-intent kind of a markdown doc (c1). A load-bearing decision / ADR /
/// `design-intent-gaps` entry is an `arch-decision`; a spec-shape / loop-discipline / handbook rule
/// is a `handbook-rule`; everything else - the reference architecture, `architecture.md`, the
/// addenda, and general design docs - is a `design-doc`. Signals are the doc path and its first
/// heading, both deterministic. A later criterion refines this and gates out usage docs.
fn classify_doc(path: &str, contents: &str) -> ConceptKind {
    let p = path.to_lowercase();
    let heading = first_heading(contents).unwrap_or_default().to_lowercase();
    if p.contains("decision")
        || p.contains("/adr")
        || p.contains("adr-")
        || p.contains("design-intent-gap")
        || heading.contains("decision")
        || heading.starts_with("adr")
    {
        return ConceptKind::ArchDecision;
    }
    if p.contains("handbook")
        || p.contains("rule")
        || p.contains("discipline")
        || p.contains("spec-shape")
        || heading.contains("handbook")
        || heading.contains("rule")
        || heading.contains("discipline")
    {
        return ConceptKind::HandbookRule;
    }
    ConceptKind::DesignDoc
}

/// Scan a source file for inline rationale: each `# WHY:` / `# NOTE:` comment line becomes a
/// `rationale` concept keyed by its 1-based source line, so a later criterion can link it to the
/// entity it explains.
fn rationale_concepts(path: &str, contents: &str) -> Vec<DesignConcept> {
    let mut out = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if let Some(text) = rationale_in_line(line) {
            out.push(DesignConcept {
                kind: ConceptKind::Rationale,
                id: format!("{path}#L{}", i + 1),
                title: text,
                doc: path.to_string(),
            });
        }
    }
    out
}

/// The rationale text of a line whose comment body is a `WHY:` / `NOTE:` marker, else `None`. Only
/// a line that is PURELY a comment (a recognized leader at its start) is a rationale, so a `//` in a
/// string literal or URL is never mistaken for one; a trailing comment is left to a later criterion.
fn rationale_in_line(line: &str) -> Option<String> {
    let t = line.trim_start();
    for leader in ["///", "//!", "//", "#", "--"] {
        if let Some(rest) = t.strip_prefix(leader) {
            let rest = rest.trim();
            if rest.starts_with("WHY:") || rest.starts_with("NOTE:") {
                return Some(rest.to_string());
            }
            // A comment line, but not a rationale marker: the leader matched, so stop.
            return None;
        }
    }
    None
}

/// The text of the first markdown `#` H1 heading, if any (the whole-doc node's title).
fn first_heading(contents: &str) -> Option<String> {
    contents
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(|h| h.trim().to_string()))
        .filter(|h| !h.is_empty())
}

/// The text of every top-level `##` H2 heading, in document order (a design-doc's section nodes).
fn section_headings(contents: &str) -> Vec<String> {
    contents
        .lines()
        .filter_map(|l| l.strip_prefix("## ").map(|h| h.trim().to_string()))
        .filter(|h| !h.is_empty())
        .collect()
}

/// A stable, url-safe anchor slug for a heading: lowercased, every run of non-alphanumeric
/// characters collapsed to a single `-`, trimmed. Deterministic, so a section node's id is a pure
/// function of its heading.
fn slug(heading: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in heading.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// The file stem (name without extension) of a `/`-separated relative path - the whole-doc node's
/// fallback title when the doc has no `#` heading.
fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_architecture_doc_becomes_a_design_doc_node_plus_section_nodes() {
        // An RA / architecture doc classifies as design-doc and contributes a whole-doc node plus
        // one node per `##` section - the granular design-doc nodes a later criterion links to code.
        let md =
            "# Reference architecture\n\nintro\n\n## Node taxonomy\n\ntext\n\n## Edge taxonomy\n";
        let cs = extract_concepts("docs/architecture.md", md);
        assert!(cs.iter().all(|c| c.kind == ConceptKind::DesignDoc));
        // The whole-doc node, titled by the H1.
        let whole = cs
            .iter()
            .find(|c| c.id == "docs/architecture.md")
            .expect("a whole-doc design-doc node");
        assert_eq!(whole.title, "Reference architecture");
        assert_eq!(whole.doc, "docs/architecture.md");
        // Two section nodes, id'd by an anchor slug of the heading.
        assert!(cs
            .iter()
            .any(|c| c.id == "docs/architecture.md#node-taxonomy" && c.title == "Node taxonomy"));
        assert!(cs
            .iter()
            .any(|c| c.id == "docs/architecture.md#edge-taxonomy" && c.title == "Edge taxonomy"));
    }

    #[test]
    fn a_load_bearing_decision_doc_becomes_a_single_arch_decision_node() {
        // A decision / ADR doc classifies as arch-decision and is ONE node (atomic design intent,
        // not a sectioned reference doc), even when it has `##` sections.
        let md = "# Ingest code as events\n\n## Context\n\n## Decision\n";
        let cs = extract_concepts("docs/adr/0001-code-as-events.md", md);
        assert_eq!(cs.len(), 1, "a decision doc is one node; got {cs:?}");
        assert_eq!(cs[0].kind, ConceptKind::ArchDecision);
        assert_eq!(cs[0].id, "docs/adr/0001-code-as-events.md");
        assert_eq!(cs[0].title, "Ingest code as events");
    }

    #[test]
    fn a_spec_shape_or_loop_discipline_doc_becomes_a_handbook_rule_node() {
        let md = "# Loop discipline handbook\n\n## One owner per criterion\n";
        let cs = extract_concepts("docs/handbook-rules.md", md);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].kind, ConceptKind::HandbookRule);
        assert_eq!(cs[0].id, "docs/handbook-rules.md");
    }

    #[test]
    fn a_why_comment_in_a_source_file_becomes_a_rationale_node() {
        // A `WHY:` / `NOTE:` comment line becomes a rationale concept keyed by its source line; a
        // plain doc comment and a `//` inside a string literal are NOT rationale.
        let rs = "fn clamp(x: i32) -> i32 {\n    // WHY: damage must never go negative\n    x.max(0)\n}\n/// a plain doc comment\nlet url = \"http://example\"; // trailing not scanned\n";
        let cs = extract_concepts("src/combat.rs", rs);
        assert_eq!(
            cs.len(),
            1,
            "exactly the WHY line is a rationale; got {cs:?}"
        );
        assert_eq!(cs[0].kind, ConceptKind::Rationale);
        assert_eq!(cs[0].id, "src/combat.rs#L2");
        assert_eq!(cs[0].title, "WHY: damage must never go negative");
        assert_eq!(cs[0].doc, "src/combat.rs");
    }

    #[test]
    fn extraction_is_deterministic_identical_input_yields_identical_concepts() {
        let md = "# Title\n\n## A\n\n## B\n";
        assert_eq!(
            extract_concepts("docs/x.md", md),
            extract_concepts("docs/x.md", md)
        );
    }

    #[test]
    fn a_design_doc_specifies_inline_code_paths_and_references_markdown_links() {
        // A design-doc SPECIFIES each CODE path it mentions inline, and REFERENCES each doc / code
        // path it cites via a markdown link. A URL, an in-page anchor, and a bare (non-path)
        // inline-code identifier are all ignored.
        let md = "# Reference architecture\n\n\
                  The projector `src/contextgraph/sqlite.rs` folds `EventLog` into the graph.\n\n\
                  See the [addendum](docs/addendum.md) and the [upstream](https://example/x).\n\
                  Back to [top](#reference-architecture).\n";
        let ls = extract_links("docs/architecture.md", md);
        assert!(ls.contains(&DesignLink {
            from: "docs/architecture.md".to_string(),
            rel: LinkRel::Specifies,
            to: "src/contextgraph/sqlite.rs".to_string(),
        }));
        assert!(ls.contains(&DesignLink {
            from: "docs/architecture.md".to_string(),
            rel: LinkRel::References,
            to: "docs/addendum.md".to_string(),
        }));
        // A bare identifier in inline code is not a path; a URL and an in-page anchor are not repo
        // citations - none of them link.
        assert!(!ls
            .iter()
            .any(|l| l.to == "EventLog" || l.to.contains("://") || l.to.starts_with('#')));
        assert_eq!(
            ls.len(),
            2,
            "exactly the SPECIFIES + references links; got {ls:?}"
        );
    }

    #[test]
    fn an_arch_decision_constrains_and_a_handbook_rule_governs_their_code() {
        // The doc's kind picks the single design->code relation: a decision CONSTRAINS, a rule
        // GOVERNS (reusing the existing GOVERNS relation).
        let dec = extract_links(
            "docs/adr/0001-x.md",
            "# Decision\n\nBinds `src/conductor.rs`.\n",
        );
        assert_eq!(
            dec,
            vec![DesignLink {
                from: "docs/adr/0001-x.md".to_string(),
                rel: LinkRel::Constrains,
                to: "src/conductor.rs".to_string(),
            }]
        );
        let rule = extract_links(
            "docs/handbook.md",
            "# Handbook rule\n\nGoverns `src/spawn.rs`.\n",
        );
        assert_eq!(
            rule,
            vec![DesignLink {
                from: "docs/handbook.md".to_string(),
                rel: LinkRel::Governs,
                to: "src/spawn.rs".to_string(),
            }]
        );
        // A non-design-doc does NOT emit references for its markdown links (references is a
        // design-doc relation); a decision that cites a doc links nothing but its CONSTRAINS.
        let cite = extract_links(
            "docs/adr/0002-y.md",
            "# Decision\n\nSee [other](docs/other.md); binds `src/gate.rs`.\n",
        );
        assert_eq!(cite.len(), 1);
        assert_eq!(cite[0].rel, LinkRel::Constrains);
    }

    #[test]
    fn a_rationale_explains_the_file_it_annotates() {
        // Each `# WHY:` / `# NOTE:` site yields one explains link, from the SAME `<file>#L<line>` id
        // extract_concepts gives the rationale node, to the file it annotates.
        let rs = "fn clamp() {}\n// WHY: damage must never go negative\nlet x = 1;\n";
        let ls = extract_links("src/combat.rs", rs);
        assert_eq!(
            ls,
            vec![DesignLink {
                from: "src/combat.rs#L2".to_string(),
                rel: LinkRel::Explains,
                to: "src/combat.rs".to_string(),
            }]
        );
    }

    #[test]
    fn a_fenced_code_example_path_is_not_mistaken_for_a_specifies_link() {
        // A path inside a fenced code block is an EXAMPLE, not the doc specifying that code; only the
        // prose inline-code mention links.
        let md = "# Reference architecture\n\n\
                  Real: `src/real.rs`.\n\n\
                  ```\nlet p = \"src/example.rs\";\nuse `src/fenced.rs`;\n```\n";
        let ls = extract_links("docs/architecture.md", md);
        assert_eq!(
            ls,
            vec![DesignLink {
                from: "docs/architecture.md".to_string(),
                rel: LinkRel::Specifies,
                to: "src/real.rs".to_string(),
            }],
            "only the unfenced inline-code path links; got {ls:?}"
        );
    }

    #[test]
    fn link_extraction_is_deterministic_and_deduplicated() {
        // Identical input yields the identical link set; a path mentioned twice links once.
        let md = "# Title\n\nUses `src/a.rs` and again `src/a.rs`; also `src/b.rs`.\n";
        let a = extract_links("docs/architecture.md", md);
        assert_eq!(a, extract_links("docs/architecture.md", md));
        assert_eq!(
            a.iter().filter(|l| l.to == "src/a.rs").count(),
            1,
            "a repeated path links exactly once; got {a:?}"
        );
    }

    #[test]
    fn an_end_user_usage_doc_is_out_of_scope_and_yields_no_concepts() {
        // Scope boundary (spec 29b criterion 3): a doc written for END USERS - how to DRIVE the tool
        // - is out of scope and produces NOTHING, so no event is ever emitted and no node ever folds
        // for it. A usage doc is recognized by a clear end-user signal in its PATH or its first
        // HEADING; the signal can appear in either. Each of these is a distinct usage-doc shape.
        let usage: &[(&str, &str)] = &[
            ("README.md", "# Rigger\n\n## Quick start\n\nrun it\n"),
            // A PURE end-user guide under a handbook path (a usage signal, no design-rule content)
            // still gates out. NOTE: the real repo file `docs/handbook/using-rigger.md` is NOT a
            // usage doc - it is an operating-discipline (loop-discipline / spec-shape rule) doc and
            // is KEPT; that keep is pinned by `a_handbook_rule_doc_with_a_usage_word_stays_in_scope`.
            (
                "docs/handbook/quickstart.md",
                "# Quick start\n\ndownload the binary and run the installer\n",
            ),
            ("docs/getting-started.md", "# Getting started\n\ninstall\n"),
            ("docs/usage.md", "# Overview\n\nhow to run\n"),
            ("docs/quickstart.md", "# Overview\n\nsteps\n"),
            ("docs/tutorial-first-run.md", "# First run\n\nsteps\n"),
            ("docs/how-to-configure.md", "# Configure\n\nsteps\n"),
            ("docs/user-guide.md", "# The guide\n\nsteps\n"),
            ("docs/faq.md", "# Questions\n\nanswers\n"),
            ("docs/troubleshooting.md", "# When it breaks\n\nfixes\n"),
            // Signal in the HEADING only (a neutral path).
            ("docs/overview.md", "# Installation\n\ninstall it\n"),
            ("docs/notes.md", "# Command reference\n\nflags\n"),
        ];
        for (path, contents) in usage {
            assert!(
                extract_concepts(path, contents).is_empty(),
                "an end-user usage doc yields zero concepts; {path} did not: {:?}",
                extract_concepts(path, contents)
            );
            // The SAME scope authority gates the link half: a usage doc emits no links either.
            assert!(
                extract_links(path, contents).is_empty(),
                "an end-user usage doc yields zero links; {path} did not: {:?}",
                extract_links(path, contents)
            );
        }
        // The gate drops ONLY usage docs: a design doc alongside them is still ingested.
        assert!(
            !extract_concepts(
                "docs/architecture.md",
                "# Reference architecture\n\n## Nodes\n"
            )
            .is_empty(),
            "a design doc is still ingested"
        );
    }

    #[test]
    fn a_usage_doc_with_inline_code_paths_still_yields_no_links() {
        // Link-half of the scope gate (spec 29b criterion 3), the case the concepts-only gate would
        // miss: a usage doc that mentions a real CODE path in inline code and cites a doc via a
        // markdown link WOULD, ungated, emit a SPECIFIES/references link (and fold its bare-artifact
        // endpoints). The gate must drop it on the link side too, so nothing folds for it.
        let usage = (
            "docs/quickstart.md",
            "# Quick start\n\nRun the binary at `src/main.rs`; see the [readme](README.md).\n",
        );
        assert!(
            extract_links(usage.0, usage.1).is_empty(),
            "a usage doc emits no links even when it names code paths; got {:?}",
            extract_links(usage.0, usage.1)
        );
        // A design doc with the same shape DOES link - so the empty result above is the gate, not an
        // extractor that fails to find the path.
        assert!(
            !extract_links(
                "docs/architecture.md",
                "# Reference architecture\n\nThe entry point is `src/main.rs`.\n"
            )
            .is_empty(),
            "a design doc with an inline code path still links"
        );
    }

    #[test]
    fn a_structural_design_doc_with_a_usage_word_stays_in_scope() {
        // Precedence: an UNAMBIGUOUS design-structural doc (the reference architecture, an addendum,
        // an ADR / decision, a spec, the design-intent-gaps ledger) is design by construction and is
        // NEVER gated out, even when its path or heading carries a usage word. This pins that the
        // scope override beats the usage signal - a load-bearing decision about the install flow, or
        // an addendum whose title mentions usage, must not be silently dropped. It also pins the
        // classify_doc kind precedence (deferred to this criterion): each still folds its own kind.
        let kept: &[(&str, &str, ConceptKind)] = &[
            // An ADR whose subject is the installation flow - "installation" is a usage word.
            (
                "docs/adr/0007-installation-flow.md",
                "# Installation flow decision\n",
                ConceptKind::ArchDecision,
            ),
            // An addendum whose heading mentions usage metering.
            (
                "docs/architecture-addendum-usage-metering.md",
                "# Usage metering\n",
                ConceptKind::DesignDoc,
            ),
            // A spec that happens to describe a tutorial subsystem.
            (
                "specs/40-tutorial-engine.md",
                "# Tutorial engine\n",
                ConceptKind::DesignDoc,
            ),
            // A load-bearing decision doc titled like a how-to.
            (
                "docs/decisions/how-to-shard.md",
                "# How to shard the store\n",
                ConceptKind::ArchDecision,
            ),
        ];
        for (path, contents, want) in kept {
            let cs = extract_concepts(path, contents);
            assert!(
                !cs.is_empty(),
                "a structural design doc is never gated; {path} was dropped"
            );
            assert_eq!(
                cs[0].kind, *want,
                "{path} folds its own kind (scope override composes with classify_doc)"
            );
        }
    }

    #[test]
    fn a_handbook_rule_doc_with_a_usage_word_stays_in_scope() {
        // Content-aware scope for a handbook path (spec 29b criterion 3). A handbook holds BOTH
        // end-user guides (usage) AND loop-discipline / spec-shape rule docs (design), so the
        // scope decision is keyed on CONTENT, never on the filename. This is the REAL repo file
        // `docs/handbook/using-rigger.md` with its real operating-discipline shape: its path opens
        // with "using-" and its heading with "Using rigger", both usage words, yet its content is a
        // loop-discipline / spec-shape rule doc and it MUST stay in scope and fold a `handbook-rule`
        // node - the exact node kind classify_doc gives it and spec 29b criterion 1 mandates. This
        // pins the false-drop the "using-" prefix would otherwise cause.
        let rule_doc = "# Using rigger: the operating discipline\n\nThis chapter is the operating discipline for a rigger run.\n\n## Spec shape\n\nOne observable behavior per criterion.\n\n## Base anchoring\n\nAnchor on the ref you want the work to land on.\n\n## The load-bearing decisions\n\nBlast-radius isolation and fail-closed review keep a run consistent.\n";
        let cs = extract_concepts("docs/handbook/using-rigger.md", rule_doc);
        assert!(
            !cs.is_empty(),
            "the real operating-discipline doc is a rule doc and is never gated as usage; it was dropped"
        );
        assert_eq!(
            cs[0].kind,
            ConceptKind::HandbookRule,
            "the operating-discipline doc folds its own handbook-rule kind; got {cs:?}"
        );
        assert_eq!(cs[0].id, "docs/handbook/using-rigger.md");

        // The discrimination is content-aware, not path-wide: a PURE end-user guide under the same
        // handbook path (a usage signal, no design-rule content) still gates out.
        assert!(
            extract_concepts(
                "docs/handbook/quickstart.md",
                "# Quick start\n\ndownload the binary and run the installer\n"
            )
            .is_empty(),
            "a pure end-user guide under a handbook path is still gated out"
        );
    }

    #[test]
    fn a_source_file_is_never_a_usage_doc_and_its_rationale_stays_in_scope() {
        // The scope gate is a DOC gate: it only ever drops markdown usage docs. A source file is
        // scanned for `# WHY:` rationale regardless of a usage word in its path, because inline
        // rationale is design intent (spec 29b) wherever the code lives.
        let rs = "fn run() {}\n// WHY: usage is metered per call\n";
        let cs = extract_concepts("src/usage_meter.rs", rs);
        assert_eq!(cs.len(), 1, "the rationale is extracted; got {cs:?}");
        assert_eq!(cs[0].kind, ConceptKind::Rationale);
        assert_eq!(cs[0].id, "src/usage_meter.rs#L2");
    }
}
