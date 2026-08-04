//! Spec 56, criterion 3 - DOCUMENT INTEGRITY of the front-door architecture
//! document.
//!
//! `docs/architecture.md` is the top of the documentation tree, so its STRUCTURAL
//! integrity - not just the accuracy of what it claims - is load-bearing: a stray em
//! dash trips the diff gate, a link to a moved file strands the reader, and a deep-dive
//! addendum that is never linked (or linked twice) breaks the "this document orients and
//! links; the addenda are the deep dives" contract the campaign set. This test pins the
//! three integrity properties the criterion names, so a future edit that breaks any of
//! them fails RED at `cargo test` time instead of shipping a structurally broken document:
//!
//!   1. PURE ASCII HYPHENS. The document must contain no unicode dash characters
//!      (U+2012 figure dash, U+2013 en dash, U+2014 em dash, U+2015 horizontal bar,
//!      U+2212 minus sign) - only the ASCII hyphen-minus `-`. This is the same rule the
//!      campaign's diff gate enforces (a U+2014 fails it); pinning it in `cargo test`
//!      catches a re-introduction before it reaches the gate.
//!   2. EVERY INTRA-REPO LINK RESOLVES. Every inline-markdown link the document carries
//!      to a file inside this repository must resolve to a file that exists. External
//!      links (`http(s)://`, `mailto:`, ...) and pure in-page anchors (`#section`) are
//!      not repo-file links and are out of scope.
//!   3. EACH ARCHITECTURE ADDENDUM LINKED EXACTLY ONCE. The document is the orientation
//!      layer over the deep-dive addenda; every `docs/architecture-addendum-*.md` that
//!      exists on disk must be linked from the document exactly once - not zero times (a
//!      stranded deep dive) and not twice (a duplicated cross-link). Discovering the
//!      addenda from the filesystem keeps the pin honest: a newly added addendum that the
//!      document forgets to link fails this test.
//!
//! OWNERSHIP BOUNDARY.
//! This criterion (c3) OWNS the document's STRUCTURAL INTEGRITY pins. It does NOT own
//! PRESENT-TENSE ACCURACY (criterion 1, `architecture_current_surface.rs`) or STALENESS
//! (criterion 2, `architecture_staleness.rs`): nothing here asserts WHAT the document
//! says, only that its characters are ASCII, its links resolve, and its addendum
//! cross-links are complete and unduplicated.
//!
//! Like the criterion-1 and criterion-2 pins, this test reads the committed
//! `docs/architecture.md` and enumerates the committed `docs/` addenda (resolved from
//! `CARGO_MANIFEST_DIR`, so it does not depend on the process CWD), parses a text file,
//! and touches no backend symbol. It is deliberately NOT feature-gated: it runs
//! identically in both feature lanes.

use std::path::PathBuf;

/// The `docs/` directory, resolved from the manifest dir so the test does not depend on
/// the process CWD (integration tests may run from anywhere).
fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs")
}

/// The committed architecture document.
fn architecture_text() -> String {
    let path = docs_dir().join("architecture.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The unicode dash characters the document must never carry - only the ASCII
/// hyphen-minus `-` is allowed. Each entry pairs the character with its Unicode name so a
/// failure report says exactly which dash slipped in.
const UNICODE_DASHES: &[(char, &str)] = &[
    ('\u{2012}', "U+2012 FIGURE DASH"),
    ('\u{2013}', "U+2013 EN DASH"),
    ('\u{2014}', "U+2014 EM DASH"),
    ('\u{2015}', "U+2015 HORIZONTAL BAR"),
    ('\u{2212}', "U+2212 MINUS SIGN"),
];

/// Every inline-markdown link/image destination in `text`: the `dest` of each `](dest)`
/// occurrence, which covers both `[label](dest)` links and `![alt](dest)` images. Any
/// `"title"` suffix and surrounding `<...>` angle brackets are stripped, so the returned
/// string is the bare destination. `](` and `)` are ASCII, so every slice boundary lands
/// on a char boundary even when the destination text around it is multi-byte.
fn link_destinations(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b')' {
                j += 1;
            }
            if j < bytes.len() {
                let dest = clean_destination(&text[start..j]);
                if !dest.is_empty() {
                    out.push(dest);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The bare destination of a markdown link body: leading/trailing whitespace trimmed, an
/// optional `"title"` (or `'title'`) suffix dropped by taking the text up to the first
/// whitespace, and surrounding `<...>` angle brackets removed.
fn clean_destination(raw: &str) -> String {
    let first = raw.split_whitespace().next().unwrap_or("");
    first
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

/// Whether `dest` is a link to a file inside this repository (as opposed to an external
/// URL or a pure in-page anchor). External schemes and protocol-relative links are out of
/// scope, as are pure fragments.
fn is_intra_repo_link(dest: &str) -> bool {
    if dest.starts_with('#') || dest.starts_with("//") || dest.contains("://") {
        return false;
    }
    let lower = dest.to_ascii_lowercase();
    !["mailto:", "tel:"].iter().any(|s| lower.starts_with(s))
}

/// Resolve an intra-repo link destination to a path on disk. A leading `/` is
/// repo-root-absolute (resolved from the manifest dir); anything else is relative to the
/// document's own directory (`docs/`). Any `#fragment` suffix is dropped first.
fn resolve_intra_repo(dest: &str) -> PathBuf {
    let path_part = dest.split('#').next().unwrap_or(dest);
    match path_part.strip_prefix('/') {
        Some(rest) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rest),
        None => docs_dir().join(path_part),
    }
}

/// Every `docs/architecture-addendum-*.md` file that exists on disk, sorted for a stable
/// failure report.
fn addendum_files() -> Vec<PathBuf> {
    let mut addenda: Vec<PathBuf> = std::fs::read_dir(docs_dir())
        .expect("docs/ must be readable")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("architecture-addendum-") && name.ends_with(".md")
                })
        })
        .collect();
    addenda.sort();
    addenda
}

#[test]
fn architecture_uses_only_ascii_hyphens() {
    let text = architecture_text();

    let offenders: Vec<String> = text
        .lines()
        .enumerate()
        .flat_map(|(lineno, line)| {
            line.chars().enumerate().filter_map(move |(col, ch)| {
                UNICODE_DASHES
                    .iter()
                    .find(|(dash, _)| *dash == ch)
                    .map(|(_, name)| {
                        format!(
                            "line {}, char {}: {} in: {}",
                            lineno + 1,
                            col + 1,
                            name,
                            line
                        )
                    })
            })
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "docs/architecture.md must use only the ASCII hyphen-minus `-`, never a unicode \
         dash (spec 56, criterion 3 - document integrity). The campaign's diff gate fails \
         on a U+2014; this pin catches any of U+2012/2013/2014/2015/2212 before it reaches \
         the gate. Unicode dashes still present in the document: {offenders:#?}"
    );
}

#[test]
fn architecture_intra_repo_links_all_resolve() {
    let broken: Vec<String> = link_destinations(&architecture_text())
        .into_iter()
        .filter(|dest| is_intra_repo_link(dest))
        .filter_map(|dest| {
            let resolved = resolve_intra_repo(&dest);
            (!resolved.exists()).then(|| format!("{dest:?} -> {} (missing)", resolved.display()))
        })
        .collect();

    assert!(
        broken.is_empty(),
        "every intra-repo link docs/architecture.md carries must resolve to a file that \
         exists (spec 56, criterion 3 - document integrity). A link to a moved or deleted \
         file strands the reader. Links that do not resolve: {broken:#?}"
    );
}

#[test]
fn architecture_links_each_addendum_exactly_once() {
    let text = architecture_text();

    // The canonical on-disk paths every intra-repo link that resolves points at. The
    // addenda paths are also canonicalized before comparison, so a link and its target
    // match regardless of how the relative path spells the route to the same file.
    let linked: Vec<PathBuf> = link_destinations(&text)
        .into_iter()
        .filter(|dest| is_intra_repo_link(dest))
        .filter_map(|dest| resolve_intra_repo(&dest).canonicalize().ok())
        .collect();

    let addenda = addendum_files();
    assert!(
        !addenda.is_empty(),
        "expected docs/architecture-addendum-*.md deep-dive files to exist - the front-door \
         document is the orientation layer over them"
    );

    let wrong: Vec<String> = addenda
        .iter()
        .filter_map(|addendum| {
            let canonical = addendum.canonicalize().ok()?;
            let count = linked.iter().filter(|path| **path == canonical).count();
            (count != 1).then(|| {
                format!(
                    "{}: linked {count} time(s), expected exactly 1",
                    addendum.file_name().unwrap_or_default().to_string_lossy()
                )
            })
        })
        .collect();

    assert!(
        wrong.is_empty(),
        "docs/architecture.md must link each architecture addendum exactly once (spec 56, \
         criterion 3 - document integrity): it is the orientation layer over the deep-dive \
         addenda, so every docs/architecture-addendum-*.md must be reachable from it - never \
         stranded (zero links) and never duplicated (two links). Addenda with the wrong link \
         count: {wrong:#?}"
    );
}
