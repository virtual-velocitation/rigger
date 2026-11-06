//! The production-boundary reader: the one rule every test that reads a source tree's production
//! text reads it by. A file under a `tests` directory is no production file. A file a
//! `mod <name>;` declares under an attribute run whose `cfg` requires `test` (`test`, or
//! `all(..)` with an argument that does) is none either, and neither is a file such a file
//! declares in turn; `any(test, ..)` and `not(test)` are production. Every other file is cut
//! where the attribute run of its first `mod <name> {` that requires `test` begins: each
//! `cfg`-gated item before it is production. An attribute run is the outer attributes in front
//! of an item, a multi-line attribute joined into one, with blank and `//` lines between them
//! passed over, so an `#[allow]` or `#[path]` between the `cfg` and the item does not defeat it.

use std::collections::BTreeSet;
use std::path::Path;

/// The production text of each `.rs` file of `files`, paths relative to `root` and
/// `/`-separated, in their order, as `(path, text)`: [`production_boundary`] over the files as
/// read.
pub fn production_files(root: &Path, files: &[String]) -> Vec<(String, String)> {
    production_boundary(
        files
            .iter()
            .filter(|file| file.ends_with(".rs"))
            .map(|file| (file.clone(), super::read_text(&root.join(file))))
            .collect(),
    )
}

/// The production files of `sources`, `(path, text)` pairs with relative `/`-separated paths,
/// in their order, each with its text cut at its test module.
pub fn production_boundary(sources: Vec<(String, String)>) -> Vec<(String, String)> {
    let scans: Vec<Scan> = sources.iter().map(|(_, text)| scan(text)).collect();
    let mut test_only = BTreeSet::new();
    loop {
        let more: Vec<String> = sources
            .iter()
            .zip(&scans)
            .flat_map(|((path, _), scan)| {
                let all = test_only.contains(path);
                scan.declared
                    .iter()
                    .filter(move |declared| all || declared.test)
                    .flat_map(move |declared| declared.files(path))
            })
            .filter(|file| !test_only.contains(file))
            .collect();
        if more.is_empty() {
            break;
        }
        test_only.extend(more);
    }
    sources
        .into_iter()
        .zip(scans)
        .filter(|((path, _), _)| {
            !path.split('/').any(|part| part == "tests") && !test_only.contains(path)
        })
        .map(|((path, text), scan)| {
            let cut = scan.cut.unwrap_or(text.len());
            (path, text[..cut].to_string())
        })
        .collect()
}

/// What one file's text declares and where its production text ends.
struct Scan {
    /// Each `mod <name>;` the file declares above its cut.
    declared: Vec<Declared>,
    /// The byte offset the file's test module's attribute run begins at, if it has one.
    cut: Option<usize>,
}

/// One `mod <name>;` declaration.
struct Declared {
    name: String,
    /// The `#[path]` its attribute run names, if any.
    path: Option<String>,
    /// Whether its attribute run requires `test`.
    test: bool,
}

impl Declared {
    /// The files this declaration, made in the file `from`, may name: its `#[path]` beside
    /// `from`, else `<name>.rs` or `<name>/mod.rs` beside a crate root or `mod.rs` and under
    /// `from`'s stem beside any other file.
    fn files(&self, from: &str) -> Vec<String> {
        let (dir, file) = from.rsplit_once('/').unwrap_or(("", from));
        if let Some(path) = &self.path {
            return vec![normalized(&format!("{dir}/{path}"))];
        }
        let base = if ["lib.rs", "main.rs", "mod.rs", "build.rs"].contains(&file) {
            dir.to_string()
        } else {
            format!("{dir}/{}", file.trim_end_matches(".rs"))
        };
        let name = &self.name;
        vec![
            normalized(&format!("{base}/{name}.rs")),
            normalized(&format!("{base}/{name}/mod.rs")),
        ]
    }
}

/// `path` with every `.` and empty part dropped and each `..` taking back the part before it.
fn normalized(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// Read `text` line by line, collecting each item's attribute run, up to the first
/// `mod <name> {` whose run requires `test`.
fn scan(text: &str) -> Scan {
    let mut declared = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let mut run_start = None;
    let mut open: Option<(bool, String)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        let mut rest = line.trim();
        if let Some((inner, mut joined)) = open.take() {
            joined.push(' ');
            joined.push_str(rest);
            match attribute_end(&joined) {
                Some(_) if inner => {}
                Some(_) => run.push(joined),
                None => open = Some((inner, joined)),
            }
            continue;
        }
        if rest.is_empty() || rest.starts_with("//") {
            continue;
        }
        while rest.starts_with("#[") || rest.starts_with("#![") {
            let inner = rest.starts_with("#![");
            if !inner {
                run_start.get_or_insert(at);
            }
            match attribute_end(rest) {
                Some(end) => {
                    if !inner {
                        run.push(rest[..end].to_string());
                    }
                    rest = rest[end..].trim_start();
                }
                None => {
                    open = Some((inner, rest.to_string()));
                    rest = "";
                }
            }
        }
        if rest.is_empty() {
            continue;
        }
        let test = run.iter().any(|attribute| requires_test(attribute));
        if let Some((name, inline)) = module_item(rest) {
            if inline && test {
                return Scan {
                    declared,
                    cut: Some(run_start.unwrap_or(at)),
                };
            }
            if !inline {
                declared.push(Declared {
                    name,
                    path: run.iter().find_map(|attribute| path_of(attribute)),
                    test,
                });
            }
        }
        run.clear();
        run_start = None;
    }
    Scan {
        declared,
        cut: None,
    }
}

/// The byte offset just past the `]` closing the attribute `text` opens, its brackets balanced
/// outside string literals, or `None` when the attribute runs on past `text`.
fn attribute_end(text: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' if !in_string => depth += 1,
            ']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The name of the module `item` declares, `mod <name>;` or `mod <name> {` behind any
/// visibility, and whether its body is inline.
fn module_item(item: &str) -> Option<(String, bool)> {
    let item = match item.strip_prefix("pub") {
        Some(rest) if rest.starts_with('(') => rest[rest.find(')')? + 1..].trim_start(),
        Some(rest) if rest.starts_with(' ') => rest.trim_start(),
        _ => item,
    };
    let rest = item.strip_prefix("mod ")?.trim_start();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    match rest[name.len()..].trim_start().chars().next() {
        Some(';') => Some((name, false)),
        Some('{') => Some((name, true)),
        _ => None,
    }
}

/// Whether `attribute` is a `cfg` whose predicate requires `test`.
fn requires_test(attribute: &str) -> bool {
    attribute
        .strip_prefix("#[")
        .and_then(|a| a.strip_suffix(']'))
        .and_then(|a| a.trim().strip_prefix("cfg"))
        .and_then(|a| a.trim().strip_prefix('('))
        .and_then(|a| a.strip_suffix(')'))
        .is_some_and(predicate_requires_test)
}

/// Whether the `cfg` predicate `predicate` holds only under `test`: `test` itself, or `all(..)`
/// with an argument that does.
fn predicate_requires_test(predicate: &str) -> bool {
    let predicate = predicate.trim();
    predicate == "test"
        || predicate
            .strip_prefix("all")
            .and_then(|a| a.trim_start().strip_prefix('('))
            .and_then(|a| a.strip_suffix(')'))
            .is_some_and(|arguments| top_level(arguments).any(predicate_requires_test))
}

/// The comma-separated arguments of `list` at its own nesting level, outside string literals.
fn top_level(list: &str) -> impl Iterator<Item = &str> {
    let mut parts = Vec::new();
    let (mut depth, mut in_string, mut from) = (0, false, 0);
    for (at, c) in list.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => {
                parts.push(&list[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[from..]);
    parts.into_iter()
}

/// The path a `#[path = ".."]` attribute names.
fn path_of(attribute: &str) -> Option<String> {
    let value = attribute
        .strip_prefix("#[")?
        .trim_start()
        .strip_prefix("path")?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    Some(value.strip_prefix('"')?.split('"').next()?.to_string())
}
