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
}
