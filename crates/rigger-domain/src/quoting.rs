//! Single-quoted literals, for the languages rigger renders a value into.

/// `text` between single quotes, each `'` it holds spelled `escaped_quote`: the spelling the
/// language reading the literal takes for a quote inside one.
pub fn single_quoted(text: &str, escaped_quote: &str) -> String {
    format!("'{}'", text.replace('\'', escaped_quote))
}
