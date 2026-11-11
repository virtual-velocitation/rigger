//! The one reply budget: the single place a combined lookup's reply is laid out and bounded.
//! Every section is cut to its own bound here and says when it was cut and how much it dropped,
//! and the whole reply is held under [`REPLY_LIMIT`] characters, below the host's limit on one
//! tool result, so a reply is never truncated by the host where the reader cannot see it.

use std::fmt::Write as _;

use super::graph::{Answered, Entity, Site, TermGraph};
use super::prose::Ranked;
use super::text::TermText;

/// The most characters a whole reply holds.
pub(crate) const REPLY_LIMIT: usize = 48_000;
/// The characters the graph section shares among its terms.
pub(crate) const GRAPH_CHARS: usize = 24_000;
/// The most entities one term's graph answer reads.
pub(crate) const GRAPH_ENTITIES: usize = 8;
/// The most callers, or callees, one entity lists.
pub(crate) const EDGES_SHOWN: usize = 10;
/// The most matching lines the text section keeps from one file for one term.
pub(crate) const TEXT_PER_FILE: usize = 5;
/// The most matching lines the text section keeps, shared among its terms.
pub(crate) const TEXT_TOTAL: usize = 50;
/// The characters the text section shares among its terms.
pub(crate) const TEXT_CHARS: usize = 16_000;
/// The most ranked prose results.
pub(crate) const PROSE_TOP: usize = 10;
/// The most characters of one matching line.
pub(crate) const LINE_CHARS: usize = 200;
/// The most characters of one prose snippet.
pub(crate) const SNIPPET_CHARS: usize = 200;

/// The characters a reply cut to [`REPLY_LIMIT`] keeps free for the note stating the cut.
pub(crate) const NOTE_ROOM: usize = 200;

/// What the graph section says after its label, of every answer it gives.
pub(crate) const GRAPH_FILTERED: &str = "Filtered, not worktree-true: a hit in a file that \
differs in your tree is withheld; the text section answers from your tree.";

/// The reply to `terms`: the graph, text and prose sections, each answered or stating why not.
pub(crate) fn reply(
    terms: &[&str],
    graph: Result<Answered, String>,
    text: Result<Vec<TermText>, String>,
    prose: Result<Ranked, String>,
) -> String {
    let mut out = format!("# lookup: {}\n", terms.join(", "));
    out.push_str("\n## graph\n");
    match graph {
        Ok(answered) => graph_section(&mut out, terms, &answered),
        Err(reason) => not_answered(&mut out, &reason),
    }
    out.push_str("\n## text\n");
    match text {
        Ok(found) => text_section(&mut out, terms, &found),
        Err(reason) => not_answered(&mut out, &reason),
    }
    out.push_str("\n## prose\n");
    match prose {
        Ok(ranked) => prose_section(&mut out, &ranked),
        Err(reason) => not_answered(&mut out, &reason),
    }
    held_under_limit(out)
}

/// A section that could not be answered, and why.
fn not_answered(out: &mut String, reason: &str) {
    let _ = writeln!(out, "not answered: {reason}");
}

/// `total` split among `n` shares as evenly as it goes, the first shares taking the remainder.
fn shares(total: usize, n: usize) -> impl Iterator<Item = usize> {
    (0..n).map(move |i| total / n + usize::from(i < total % n))
}

/// The graph's label, then each term's graph answer within its share of [`GRAPH_CHARS`].
fn graph_section(out: &mut String, terms: &[&str], answered: &Answered) {
    let _ = writeln!(out, "{}\n{GRAPH_FILTERED}", answered.label);
    for ((term, answer), share) in terms
        .iter()
        .zip(&answered.terms)
        .zip(shares(GRAPH_CHARS, terms.len()))
    {
        let _ = writeln!(out, "\n### {term}");
        match answer {
            TermGraph::Missing { nearest } if nearest.is_empty() => {
                let _ = writeln!(out, "no entity named {term}; the graph holds no near name");
            }
            TermGraph::Missing { nearest } => {
                let _ = writeln!(
                    out,
                    "no entity named {term}; nearest: {}",
                    nearest.join(", ")
                );
            }
            TermGraph::Found {
                tier,
                entities,
                reached,
                unread,
            } => {
                let _ = writeln!(out, "{tier} match: {}", entities_counted(*reached));
                let mut used = 0;
                let mut shown = 0;
                for block in entities.iter().map(entity_block) {
                    used += block.chars().count();
                    if used > share {
                        break;
                    }
                    shown += 1;
                    out.push_str(&block);
                }
                if shown < entities.len() {
                    let _ = writeln!(
                        out,
                        "truncated: {} dropped to the graph section's size bound",
                        entities_counted(entities.len() - shown)
                    );
                }
                if *unread > 0 {
                    let _ = writeln!(
                        out,
                        "truncated: {} not read past the first {GRAPH_ENTITIES} shown; pass a fuller name or path to narrow",
                        entities_counted(*unread)
                    );
                }
            }
        }
    }
}

/// `n entity` or `n entities`.
fn entities_counted(n: usize) -> String {
    let noun = if n == 1 { "entity" } else { "entities" };
    format!("{n} {noun}")
}

/// One entity's lines: its site, then its callers, callees and referencing files.
fn entity_block(entity: &Entity) -> String {
    let mut block = format!("- {}\n", site_line(&entity.site));
    edge_line(&mut block, "called by", &entity.callers);
    edge_line(&mut block, "calls", &entity.callees);
    if !entity.referenced_in.is_empty() {
        let shown = entity.referenced_in.len().min(EDGES_SHOWN);
        let _ = write!(
            block,
            "  referenced in: {}",
            entity.referenced_in[..shown].join(", ")
        );
        more(&mut block, entity.referenced_in.len() - shown);
    }
    block
}

/// `  <label>: <site>; <site> (+N more)` when `sites` holds any.
fn edge_line(block: &mut String, label: &str, sites: &[Site]) {
    if sites.is_empty() {
        return;
    }
    let shown = sites.len().min(EDGES_SHOWN);
    let listed: Vec<String> = sites[..shown].iter().map(site_line).collect();
    let _ = write!(block, "  {label}: {}", listed.join("; "));
    more(block, sites.len() - shown);
}

/// ` (+N more)` when `dropped` is non-zero, and the line's end.
fn more(block: &mut String, dropped: usize) {
    if dropped > 0 {
        let _ = write!(block, " (+{dropped} more)");
    }
    block.push('\n');
}

/// `file:line id (kind)`, the line left out when unknown and the file when there is none.
fn site_line(site: &Site) -> String {
    match &site.file {
        Some((file, 0)) => format!("{file} {} ({})", site.id, site.kind),
        Some((file, line)) => format!("{file}:{line} {} ({})", site.id, site.kind),
        None => format!("{} ({})", site.id, site.kind),
    }
}

/// Each term's matching lines within its share of [`TEXT_TOTAL`].
fn text_section(out: &mut String, terms: &[&str], found: &[TermText]) {
    let _ = writeln!(
        out,
        "A literal, smart-case scan of your working tree as it stands, .gitignore honoured."
    );
    let line_shares = shares(TEXT_TOTAL, terms.len());
    let char_shares = shares(TEXT_CHARS, terms.len());
    for (((term, text), lines), chars) in terms.iter().zip(found).zip(line_shares).zip(char_shares)
    {
        let _ = writeln!(out, "\n### {term}");
        if text.matched == 0 {
            let _ = writeln!(out, "no line of your tree holds {term}");
            continue;
        }
        let mut used = 0;
        let mut shown = 0;
        for hit in text.hits.iter().take(lines) {
            let line = format!(
                "{}:{}: {}\n",
                hit.file,
                hit.line,
                clipped(&hit.text, LINE_CHARS)
            );
            used += line.chars().count();
            if used > chars {
                break;
            }
            out.push_str(&line);
            shown += 1;
        }
        if shown < text.matched {
            let _ = writeln!(
                out,
                "truncated: {shown} of {} matching lines shown ({} dropped; at most {TEXT_PER_FILE} per file, {TEXT_TOTAL} lines and {TEXT_CHARS} characters across the terms)",
                text.matched,
                text.matched - shown
            );
        }
    }
}

/// The ranked prose, each result labelled `prose:<kind>`.
fn prose_section(out: &mut String, ranked: &Ranked) {
    let _ = writeln!(
        out,
        "Prose and the graph's records ranked by relevance to all the terms together."
    );
    if let Some(reason) = &ranked.records_left_out {
        let _ = writeln!(out, "the graph's records are left out: {reason}");
    }
    if ranked.hits.is_empty() {
        let _ = writeln!(out, "no prose matches");
        return;
    }
    for hit in &ranked.hits {
        let title = if hit.title.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", clipped(&hit.title, LINE_CHARS))
        };
        let _ = writeln!(
            out,
            "- prose:{} {}{title}: {}",
            hit.kind,
            hit.place,
            clipped(&hit.snippet, SNIPPET_CHARS)
        );
    }
    if ranked.hits.len() < ranked.matched {
        let _ = writeln!(
            out,
            "truncated: top {} of {} matching sections shown ({} dropped)",
            ranked.hits.len(),
            ranked.matched,
            ranked.matched - ranked.hits.len()
        );
    }
}

/// `text`'s first `limit` characters, marked when cut.
fn clipped(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((at, _)) => format!("{}...", &text[..at]),
        None => text.to_string(),
    }
}

/// `reply` held under [`REPLY_LIMIT`]: cut at the last whole line that fits, with a stated note
/// of how much was cut.
pub(super) fn held_under_limit(reply: String) -> String {
    if reply.chars().count() <= REPLY_LIMIT {
        return reply;
    }
    let total_lines = reply.lines().count();
    let room = REPLY_LIMIT - NOTE_ROOM;
    let mut kept = String::with_capacity(reply.len().min(room * 4));
    let (mut lines, mut chars) = (0, 0);
    for line in reply.lines() {
        chars += line.chars().count() + 1;
        if chars > room {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
        lines += 1;
    }
    let _ = writeln!(
        kept,
        "\ntruncated: the reply reached its {REPLY_LIMIT}-character bound; {} of {} lines dropped. Pass fewer terms.",
        total_lines - lines,
        total_lines
    );
    kept
}
