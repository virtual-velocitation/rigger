//! Language-marker fixtures: the oracle spec 120's MARKERS rule reads a shipped text with, shared
//! by the binary's unit tests and the integration suites that read the same text at a boundary.

/// Every language marker `text` spells under spec 120's MARKERS rule, the word markers first
/// and then the substring markers, each in list order. The text is lower-cased; a word marker
/// hits when it, or it followed by `s`, equals a token (a run of letters, digits and `_`), or,
/// unless it is a whole-token marker, equals any `_`-separated part of one; a syntax or
/// phrase marker hits as a substring.
pub fn language_marker_hits(text: &str) -> Vec<&'static str> {
    const PART_WORDS: &[&str] = &[
        "fn", "crate", "impl", "trait", "match", "matches", "cargo", "clippy", "rustfmt", "wasm",
        "rs", "rust", "rustdoc",
    ];
    const WHOLE_TOKEN_WORDS: &[&str] = &["create_dir_all", "for_extension"];
    const SUBSTRINGS: &[&str] = &[
        "#[",
        "//!",
        "cargo.toml",
        "the sdet lens",
        "the architecture lens",
        "feature lane",
    ];
    let text = text.to_lowercase();
    let tokens: Vec<&str> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .collect();
    let word_hits = |marker: &str, by_part: bool| {
        tokens.iter().any(|token| {
            *token == marker
                || token.strip_suffix('s') == Some(marker)
                || (by_part && token.split('_').any(|part| part == marker))
        })
    };
    PART_WORDS
        .iter()
        .copied()
        .filter(|marker| word_hits(marker, true))
        .chain(
            WHOLE_TOKEN_WORDS
                .iter()
                .copied()
                .filter(|marker| word_hits(marker, false)),
        )
        .chain(
            SUBSTRINGS
                .iter()
                .copied()
                .filter(|marker| text.contains(marker)),
        )
        .collect()
}
