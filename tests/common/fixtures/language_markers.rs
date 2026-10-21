//! Language-marker fixtures: the oracle spec 120's MARKERS rule reads a shipped text with, shared
//! by the binary's unit tests and the integration suites that read the same text at a boundary,
//! and the identifier character class it splits on, which the source audits and the suites that
//! read an identifier off a source line share too.

/// Every language marker `text` spells under spec 120's MARKERS rule, the word markers first
/// and then the substring markers, each in list order. The text is lower-cased; a word marker
/// hits when it, or it followed by `s`, equals a token (a run of letters, digits and `_`) or any
/// `_`-separated part of one, so a marker holding `_` hits only as a whole token, since no part
/// holds `_`; a syntax or phrase marker hits as a substring.
pub fn language_marker_hits(text: &str) -> Vec<&'static str> {
    const WORDS: &[&str] = &[
        "fn",
        "crate",
        "impl",
        "trait",
        "match",
        "matches",
        "cargo",
        "clippy",
        "rustfmt",
        "wasm",
        "rs",
        "rust",
        "rustdoc",
        "create_dir_all",
        "for_extension",
    ];
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
        .split(|c: char| !is_ident_char(c))
        .filter(|t| !t.is_empty())
        .collect();
    let word_hits = |marker: &str| {
        tokens.iter().any(|token| {
            *token == marker
                || token.strip_suffix('s') == Some(marker)
                || token.split('_').any(|part| part == marker)
        })
    };
    WORDS
        .iter()
        .copied()
        .filter(|marker| word_hits(marker))
        .chain(
            SUBSTRINGS
                .iter()
                .copied()
                .filter(|marker| text.contains(marker)),
        )
        .collect()
}

/// Whether `c` is an identifier character, a letter, a digit or `_`: the word-token class the
/// marker oracle splits a text on and the source audits read an identifier with, so e.g.
/// `fnv1a_64` is never mistaken for the `fn` keyword.
pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
