# Symbol Index + `symbols` Grounder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a deterministic, tree-sitter-based symbol index and a `symbols` (and `hybrid`) grounder behind the existing `Grounder` port, serving the PRECISE grounding contract, validated against the turbovec default.

**Architecture:** A new feature-gated module `src/grounder/symbols/` projects the code tree into a per-language symbol model (definitions + references + a name-level, per-language cross-reference graph). Extraction is one function over an INJECTED `(grammar, tag query)` pair (tree-sitter tags); a static registry maps extensions to grammars; the index persists under `.rigger/` with construction-deterministic serialization; a `Symbols` grounder ranks name matches over incidental text. It plugs into `grounder_for` / `select_grounder` alongside `grep` / `turbovec` / `nop`. Blast-radius and conductor wiring are spec 16, OUT OF SCOPE here.

**Tech Stack:** Rust; `tree-sitter` + `tree-sitter-tags` + per-language grammar crates; `serde` with `BTreeMap`/`BTreeSet` for deterministic persistence; the existing `grounder::Grounder` trait (`ground(&self, query, k) -> Vec<Ref>`, `reindex(&self, src_dir, files)`), `grounder::Ref { file: String, line: u32, text: String }`.

## Global Constraints

- Hyphens, not em dashes, everywhere (a gate checks the diff; U+2014 fails it).
- No references to any external tool or project in code, comments, or commit messages.
- New event types: NONE in this spec.
- The domain model is tree-sitter-free: no `tree_sitter::` type appears in the persisted/queried model API (spans are plain integers, `kind` is a rigger enum). tree-sitter lives ONLY in `extract.rs`.
- Determinism by construction: the persisted model uses `BTreeMap`/`BTreeSet`/sorted `Vec`, NEVER `HashMap`/`HashSet` in anything serialized; file content hashing is line-ending-normalized.
- The grammar set is behind a `symbols` cargo feature, carried in `default` (so a normal `cargo build` has it), like `turbovec`. BOTH lanes must compile and pass: `cargo build` (default) and `cargo build --no-default-features`, and the same for `cargo test`.
- Selection never silently degrades: `defaults.grounder: symbols` on a binary built without the `symbols` feature is a LOUD error (mirror `turbovec_feature_missing_error`), not a silent grep fallback. Per-file grep fallback happens ONLY inside an already-active `symbols` grounder, for a file whose language is not registered or does not parse.
- The shipped default is UNCHANGED (`symbols` is opt-in via config) until the replay gate versus turbovec earns the switch.

---

### Task 0: Add the `symbols` feature + tree-sitter dependencies

**Files:**
- Modify: `Cargo.toml` (`[features]` and `[dependencies]`)

**Interfaces:**
- Produces: a `symbols` cargo feature gating `dep:tree-sitter`, `dep:tree-sitter-tags`, and the five grammar crates; `symbols` added to `default`.

- [ ] **Step 1: Add the feature + deps**

In `Cargo.toml`, add `symbols` to the default set and define it, and add the optional deps. ALL grammar crates must be ABI-compatible with the pinned `tree-sitter` version - resolve compatible versions together (a mismatched grammar ABI is a compile/link error).

```toml
# in [features]
default = ["turbovec", "symbols"]
# Deterministic, dependency-light STRUCTURAL grounding: tree-sitter parsers + the tags
# mechanism, compiled in (no runtime-loaded artifacts). Feature-gated so a minimal build
# drops it, exactly as `turbovec` is.
symbols = [
    "dep:tree-sitter", "dep:tree-sitter-tags",
    "dep:tree-sitter-rust", "dep:tree-sitter-c-sharp",
    "dep:tree-sitter-javascript", "dep:tree-sitter-typescript",
    "dep:tree-sitter-go", "dep:tree-sitter-python",
]

# in [dependencies] (all optional, all pinned to versions ABI-compatible with tree-sitter)
tree-sitter = { version = "0.24", optional = true }
tree-sitter-tags = { version = "0.24", optional = true }
tree-sitter-rust = { version = "0.23", optional = true }
tree-sitter-c-sharp = { version = "0.23", optional = true }
tree-sitter-javascript = { version = "0.23", optional = true }
tree-sitter-typescript = { version = "0.23", optional = true }
tree-sitter-go = { version = "0.23", optional = true }
tree-sitter-python = { version = "0.23", optional = true }
```

- [ ] **Step 2: Verify both lanes build**

Run: `cargo build --no-default-features && cargo build`
Expected: both succeed. If a grammar crate fails to link, adjust its version to the one matching `tree-sitter`'s ABI (check each grammar crate's `tree-sitter` version requirement), then rebuild.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build(symbols): add the symbols feature + tree-sitter deps"
```

---

## UNIT 1 - the parser-free data model + tags extraction (Rust)

### Task 1: The symbol data model (parser-free, deterministic)

**Files:**
- Create: `src/grounder/symbols/mod.rs`
- Create: `src/grounder/symbols/model.rs`
- Modify: `src/grounder/mod.rs` (add `#[cfg(feature = "symbols")] pub mod symbols;`)

**Interfaces:**
- Produces: `symbols::model::{Kind, Def, SymRef, FileSymbols, SymbolIndex}`; `SymbolIndex::insert_file(rel_path: String, fs: FileSymbols)`; `SymbolIndex::definitions_named(name: &str) -> Vec<&Def>`; `SymbolIndex::references_named(name: &str, lang: Lang) -> Vec<(&str, &SymRef)>` (per-language scoped).

- [ ] **Step 1: Write the failing test**

In `src/grounder/symbols/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_and_references_are_name_indexed_and_language_scoped() {
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "a.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def { kind: Kind::Function, name: "parse".into(), line: 3 }],
                refs: vec![SymRef { name: "parse".into(), line: 9 }],
            },
        );
        idx.insert_file(
            "b.py".into(),
            FileSymbols {
                lang: Lang::Python,
                defs: vec![Def { kind: Kind::Function, name: "parse".into(), line: 1 }],
                refs: vec![],
            },
        );
        // Name lookup finds both definitions of `parse`.
        assert_eq!(idx.definitions_named("parse").len(), 2);
        // References are LANGUAGE-SCOPED: a Rust `parse` reference never links the Python def.
        let rs_refs = idx.references_named("parse", Lang::Rust);
        assert_eq!(rs_refs.len(), 1);
        assert_eq!(rs_refs[0].0, "a.rs");
        assert_eq!(idx.references_named("parse", Lang::Python).len(), 0);
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test --no-default-features symbols::model 2>&1 | tail -5` (the model is NOT feature-gated logic; keep the module always-compiled but gate only the tree-sitter parts - see Task 2. Put `#[cfg(feature = "symbols")] pub mod symbols;` in mod.rs, and run under default features instead:)
Run: `cargo test symbols::model::tests::definitions_and_references -v 2>&1 | tail -8`
Expected: FAIL (types not defined).

- [ ] **Step 3: Implement the model**

In `src/grounder/symbols/mod.rs`:

```rust
//! The symbol index: a projection of the code tree into definitions, references, and a
//! name-level, per-language cross-reference graph (architecture 5.5). Parser-free model
//! (`model`); tree-sitter lives only in `extract`.
pub mod model;
pub mod extract;
pub mod registry;
pub mod store;
pub mod grounder;
pub mod hybrid;
```

In `src/grounder/mod.rs` add near the other module declarations:

```rust
#[cfg(feature = "symbols")]
pub mod symbols;
```

In `src/grounder/symbols/model.rs`:

```rust
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// The languages the registry can extract. A rigger-owned enum so the model never names a
/// tree-sitter type; per-language scoping keys the cross-reference graph on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Lang { Rust, CSharp, Js, Ts, Go, Python }

/// The kind of a definition. A rigger enum, not a tree-sitter syntax-type id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind { Function, Method, Type, Trait, Impl, Module, Constant, Other }

/// A definition site: kind, name, and 1-based line (a plain integer span, never a
/// `tree_sitter::Range`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Def { pub kind: Kind, pub name: String, pub line: u32 }

/// A reference site: the referenced name and its 1-based line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymRef { pub name: String, pub line: u32 }

/// One file's extracted symbols.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSymbols { pub lang: Lang, pub defs: Vec<Def>, pub refs: Vec<SymRef> }

/// The whole-project index. Deterministic containers only (`BTreeMap`): iterating it for
/// serialization is stable across processes, unlike a `HashMap`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIndex {
    /// rel-path -> that file's symbols.
    files: BTreeMap<String, FileSymbols>,
}

impl SymbolIndex {
    pub fn insert_file(&mut self, rel_path: String, fs: FileSymbols) {
        self.files.insert(rel_path, fs);
    }
    /// Every definition whose name equals `name`, across languages.
    pub fn definitions_named(&self, name: &str) -> Vec<&Def> {
        self.files.values().flat_map(|f| f.defs.iter()).filter(|d| d.name == name).collect()
    }
    /// (file, reference) pairs referencing `name`, SCOPED to `lang` (a Rust reference never
    /// links a Python definition - the cross-language collision fix, 5.5.2).
    pub fn references_named(&self, name: &str, lang: Lang) -> Vec<(&str, &SymRef)> {
        self.files.iter()
            .filter(|(_, f)| f.lang == lang)
            .flat_map(|(p, f)| f.refs.iter().map(move |r| (p.as_str(), r)))
            .filter(|(_, r)| r.name == name)
            .collect()
    }
    pub fn files(&self) -> &BTreeMap<String, FileSymbols> { &self.files }
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::model::tests::definitions_and_references -v 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/mod.rs src/grounder/symbols/mod.rs src/grounder/symbols/model.rs
git commit -m "feat(symbols): parser-free, per-language-scoped symbol data model (spec 15, unit 1)"
```

### Task 2: Tags extraction over an injected (grammar, tag query) - Rust

**Files:**
- Create: `src/grounder/symbols/extract.rs`

**Interfaces:**
- Consumes: `model::{FileSymbols, Def, SymRef, Kind, Lang}`.
- Produces: `extract::extract(source: &str, lang: Lang, ts_language: &tree_sitter::Language, tags_query: &str) -> Result<FileSymbols, String>` - the ONE extraction path, grammar injected.

- [ ] **Step 1: Write the failing test**

In `src/grounder/symbols/extract.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::{Kind, Lang};

    #[test]
    fn extracts_a_rust_definition_and_a_reference() {
        let src = "fn parse(x: u8) -> u8 { x }\nfn caller() { parse(1); }\n";
        let lang = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &lang, tree_sitter_rust::TAGS_QUERY).unwrap();
        // The definition `parse` is found with its kind and 1-based line.
        assert!(fs.defs.iter().any(|d| d.name == "parse" && d.kind == Kind::Function && d.line == 1));
        // The call site `parse(1)` on line 2 is a reference.
        assert!(fs.refs.iter().any(|r| r.name == "parse" && r.line == 2));
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::extract::tests::extracts_a_rust -v 2>&1 | tail -8`
Expected: FAIL (`extract` not defined). NOTE: if `tree_sitter_rust::TAGS_QUERY` or `LANGUAGE` differs in the resolved crate version, run `cargo doc -p tree-sitter-rust --no-deps --open` and use the exported query constant + language fn it actually provides; carry a local `.scm` if the crate ships none.

- [ ] **Step 3: Implement extraction (feature-gated)**

In `src/grounder/symbols/extract.rs` (the whole file is `#[cfg(feature = "symbols")]` via the module gate on `symbols`):

```rust
use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef};
use tree_sitter_tags::{TagsConfiguration, TagsContext};

/// Map a tag's syntax-type NAME (from the grammar's tags query, e.g. "function",
/// "method", "struct", "class", "interface", "trait", "module", "constant") to a rigger
/// `Kind`. Unknown names fold to `Other` - the model stays grammar-agnostic.
fn kind_of(syntax_type: &str) -> Kind {
    match syntax_type {
        "function" => Kind::Function,
        "method" => Kind::Method,
        "struct" | "class" | "enum" | "type" | "interface" => Kind::Type,
        "trait" => Kind::Trait,
        "impl" => Kind::Impl,
        "module" | "namespace" => Kind::Module,
        "constant" => Kind::Constant,
        _ => Kind::Other,
    }
}

/// Extract one file's definitions and references by running the INJECTED grammar's tag
/// query over `source` (5.5.3). The only function that touches tree-sitter; everything
/// downstream sees `FileSymbols` (parser-free).
pub fn extract(
    source: &str,
    lang: Lang,
    ts_language: &tree_sitter::Language,
    tags_query: &str,
) -> Result<FileSymbols, String> {
    let config = TagsConfiguration::new(ts_language.clone(), tags_query, "")
        .map_err(|e| format!("symbols: tags config: {e}"))?;
    let mut ctx = TagsContext::new();
    let (tags, _) = ctx
        .generate_tags(&config, source.as_bytes(), None)
        .map_err(|e| format!("symbols: generate tags: {e}"))?;
    let mut defs = Vec::new();
    let mut refs = Vec::new();
    for tag in tags {
        let tag = tag.map_err(|e| format!("symbols: tag: {e}"))?;
        let name = source
            .get(tag.name_range.start..tag.name_range.end)
            .unwrap_or_default()
            .to_string();
        // 1-based line from the byte range's row.
        let line = (tag.span.start.row + 1) as u32;
        let syntax = config.syntax_type_name(tag.syntax_type_id);
        if tag.is_definition {
            defs.push(Def { kind: kind_of(syntax), name, line });
        } else {
            refs.push(SymRef { name, line });
        }
    }
    Ok(FileSymbols { lang, defs, refs })
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::extract::tests::extracts_a_rust -v 2>&1 | tail -8`
Expected: PASS. (If a field name like `tag.span`/`tag.name_range`/`tag.is_definition` differs in the resolved `tree-sitter-tags` version, adjust to the actual `Tag` struct fields - confirm via `cargo doc -p tree-sitter-tags --no-deps`.)

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/extract.rs
git commit -m "feat(symbols): tags extraction over an injected grammar, Rust (spec 15, unit 1)"
```

### Task 3: High-fan-out suppression on the cross-reference graph

**Files:**
- Modify: `src/grounder/symbols/model.rs`

**Interfaces:**
- Produces: `SymbolIndex::is_hub(name: &str, percentile: f64) -> bool` and `SymbolIndex::reference_degree(name: &str) -> usize` - the per-repo fan-out data spec 16 consumes.

- [ ] **Step 1: Write the failing test**

Add to `model.rs` tests:

```rust
#[test]
fn hub_symbols_are_flagged_by_repo_relative_degree() {
    let mut idx = SymbolIndex::default();
    // `new` is referenced in many files (a hub); `apply_damage` in one.
    for i in 0..20 {
        idx.insert_file(format!("f{i}.rs"), FileSymbols {
            lang: Lang::Rust, defs: vec![],
            refs: vec![SymRef { name: "new".into(), line: 1 }],
        });
    }
    idx.insert_file("combat.rs".into(), FileSymbols {
        lang: Lang::Rust, defs: vec![Def { kind: Kind::Function, name: "apply_damage".into(), line: 1 }],
        refs: vec![SymRef { name: "apply_damage".into(), line: 2 }],
    });
    assert_eq!(idx.reference_degree("new"), 20);
    // At the 90th percentile of the degree distribution, `new` is a hub, `apply_damage` is not.
    assert!(idx.is_hub("new", 0.90));
    assert!(!idx.is_hub("apply_damage", 0.90));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::model::tests::hub_symbols -v 2>&1 | tail -6`
Expected: FAIL (`reference_degree`/`is_hub` not defined).

- [ ] **Step 3: Implement**

Add to `impl SymbolIndex` in `model.rs`:

```rust
/// How many references (across all files) name `name`.
pub fn reference_degree(&self, name: &str) -> usize {
    self.files.values().flat_map(|f| f.refs.iter()).filter(|r| r.name == name).count()
}

/// Whether `name` is a HUB - its reference degree is at or above the `percentile` of the
/// repo's own reference-degree distribution (a relative threshold, not an absolute magic
/// number a monorepo would blow past, 5.5.2). Spec 16 treats a hub as conflict-with-all.
pub fn is_hub(&self, name: &str, percentile: f64) -> bool {
    let mut degrees: Vec<usize> = {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for r in self.files.values().flat_map(|f| f.refs.iter()) {
            *counts.entry(r.name.as_str()).or_insert(0) += 1;
        }
        counts.into_values().collect()
    };
    if degrees.is_empty() { return false; }
    degrees.sort_unstable();
    let cutoff_idx = ((degrees.len() as f64 - 1.0) * percentile).floor() as usize;
    let cutoff = degrees[cutoff_idx];
    self.reference_degree(name) >= cutoff && cutoff > 0
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::model::tests::hub_symbols -v 2>&1 | tail -6`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/model.rs
git commit -m "feat(symbols): repo-relative hub-symbol fan-out scoring (spec 15, unit 1)"
```

---

## UNIT 2 - the grammar registry + five languages

### Task 4: The extension -> (grammar, tag query) registry + auto-detect

**Files:**
- Create: `src/grounder/symbols/registry.rs`

**Interfaces:**
- Consumes: `model::Lang`; `tree_sitter::Language`.
- Produces: `registry::LanguageEntry { lang: Lang, language: tree_sitter::Language, tags_query: &'static str }`; `registry::for_extension(ext: &str) -> Option<LanguageEntry>`; `registry::for_path(path: &str, override_lang: Option<Lang>) -> Option<LanguageEntry>`.

- [ ] **Step 1: Write the failing test**

In `src/grounder/symbols/registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::Lang;

    #[test]
    fn auto_detects_all_five_languages_by_extension() {
        assert_eq!(for_extension("rs").unwrap().lang, Lang::Rust);
        assert_eq!(for_extension("cs").unwrap().lang, Lang::CSharp);
        assert_eq!(for_extension("ts").unwrap().lang, Lang::Ts);
        assert_eq!(for_extension("tsx").unwrap().lang, Lang::Ts);
        assert_eq!(for_extension("js").unwrap().lang, Lang::Js);
        assert_eq!(for_extension("mjs").unwrap().lang, Lang::Js);
        assert_eq!(for_extension("go").unwrap().lang, Lang::Go);
        assert_eq!(for_extension("py").unwrap().lang, Lang::Python);
        assert!(for_extension("txt").is_none());
    }

    #[test]
    fn language_override_wins_over_extension() {
        // A `.txt` with an explicit override still resolves.
        assert_eq!(for_path("notes.txt", Some(Lang::Rust)).unwrap().lang, Lang::Rust);
        // No override + unknown extension: none.
        assert!(for_path("notes.txt", None).is_none());
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::registry::tests -v 2>&1 | tail -8`
Expected: FAIL.

- [ ] **Step 3: Implement the registry**

In `src/grounder/symbols/registry.rs`:

```rust
use crate::grounder::symbols::model::Lang;

/// A registered language: the rigger `Lang`, its tree-sitter grammar, and the tag query
/// the extractor runs. Adding a language is adding an entry here - no bespoke code.
pub struct LanguageEntry {
    pub lang: Lang,
    pub language: tree_sitter::Language,
    pub tags_query: &'static str,
}

/// Resolve the language for a bare extension (no dot), or `None` if unregistered.
pub fn for_extension(ext: &str) -> Option<LanguageEntry> {
    let (lang, language, tags_query): (Lang, tree_sitter::Language, &'static str) = match ext {
        "rs" => (Lang::Rust, tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::TAGS_QUERY),
        "cs" => (Lang::CSharp, tree_sitter_c_sharp::LANGUAGE.into(), tree_sitter_c_sharp::TAGS_QUERY),
        "ts" | "tsx" => (Lang::Ts, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), tree_sitter_typescript::TAGS_QUERY),
        "js" | "mjs" | "cjs" | "jsx" => (Lang::Js, tree_sitter_javascript::LANGUAGE.into(), tree_sitter_javascript::TAGS_QUERY),
        "go" => (Lang::Go, tree_sitter_go::LANGUAGE.into(), tree_sitter_go::TAGS_QUERY),
        "py" => (Lang::Python, tree_sitter_python::LANGUAGE.into(), tree_sitter_python::TAGS_QUERY),
        _ => return None,
    };
    Some(LanguageEntry { lang, language, tags_query })
}

/// Resolve by path, honoring an explicit `--language` override; else auto-detect by extension.
pub fn for_path(path: &str, override_lang: Option<Lang>) -> Option<LanguageEntry> {
    if let Some(l) = override_lang {
        // Map the override back to any registered extension for that language.
        let ext = match l {
            Lang::Rust => "rs", Lang::CSharp => "cs", Lang::Ts => "ts",
            Lang::Js => "js", Lang::Go => "go", Lang::Python => "py",
        };
        return for_extension(ext);
    }
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    for_extension(ext)
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::registry::tests -v 2>&1 | tail -8`
Expected: PASS. (If a grammar exports its tags query under a different constant name than `TAGS_QUERY`, or has no query, use `cargo doc -p <grammar-crate> --no-deps` to find the exported constant, or embed a local `queries/<lang>-tags.scm` via `include_str!`.)

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/registry.rs
git commit -m "feat(symbols): extension->grammar registry, five languages, auto-detect + override (spec 15, unit 2)"
```

### Task 5: Index a whole directory (skip-dirs, per-file, graceful)

**Files:**
- Modify: `src/grounder/symbols/mod.rs` (add `build_index`)
- Test: same file

**Interfaces:**
- Consumes: `grounder::{walk_guarded, SKIP_DIRS}`, `registry::for_path`, `extract::extract`, `model::SymbolIndex`.
- Produces: `symbols::build_index(root: &str, override_lang: Option<Lang>) -> SymbolIndex`.

- [ ] **Step 1: Write the failing test**

Add to `src/grounder/symbols/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::Kind;

    #[test]
    fn build_index_walks_the_tree_and_skips_unparseable_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn parse() {}\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "not code\n").unwrap(); // unregistered -> skipped
        std::fs::write(dir.path().join("c.rs"), "fn (((").unwrap();       // malformed -> partial/empty, no crash
        let idx = build_index(dir.path().to_str().unwrap(), None);
        assert!(idx.definitions_named("parse").iter().any(|d| d.kind == Kind::Function));
        assert!(idx.files().contains_key("a.rs"));
        assert!(!idx.files().contains_key("b.txt"));
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::tests::build_index_walks -v 2>&1 | tail -8`
Expected: FAIL (`build_index` not defined).

- [ ] **Step 3: Implement**

Add to `src/grounder/symbols/mod.rs`:

```rust
use crate::grounder::walk_guarded;
use crate::grounder::symbols::model::{Lang, SymbolIndex};
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::path::Path;

/// Build the index over `root`: walk the tree (shared skip-dirs + cycle guard), and for each
/// file whose extension is registered, extract its symbols. An unregistered extension is
/// skipped; a file that fails to parse yields whatever partial symbols the tags run produced
/// (never a crash). `override_lang` forces a language for every file (the `--language` flag).
pub fn build_index(root: &str, override_lang: Option<Lang>) -> SymbolIndex {
    let mut idx = SymbolIndex::default();
    let mut visited = HashSet::new();
    let _ = walk_guarded(Path::new(root), &mut visited, &mut |path| {
        let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned();
        if let Some(entry) = registry::for_path(&rel, override_lang) {
            if let Ok(src) = std::fs::read_to_string(path) {
                if let Ok(fs) = extract::extract(&src, entry.lang, &entry.language, entry.tags_query) {
                    idx.insert_file(rel, fs);
                }
            }
        }
        ControlFlow::Continue(())
    });
    idx
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::tests::build_index_walks -v 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/mod.rs
git commit -m "feat(symbols): build the index over a tree, graceful per-file (spec 15, unit 2)"
```

---

## UNIT 3 - persistence + incremental freshening (deterministic)

### Task 6: Persist + load with construction-deterministic serialization

**Files:**
- Create: `src/grounder/symbols/store.rs`

**Interfaces:**
- Consumes: `model::SymbolIndex`.
- Produces: `store::{save(idx, dir), load(dir) -> Option<SymbolIndex>, index_path(dir), content_hash(src) -> String}`.

- [ ] **Step 1: Write the failing determinism test (fresh-process)**

In `src/grounder/symbols/store.rs` - the determinism test re-serializes and asserts byte-equality; the CROSS-process guard is enforced by construction (BTreeMap, no HashMap) plus a `tests/cli.rs` fresh-process test in Task 10.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymbolIndex};

    fn sample() -> SymbolIndex {
        let mut idx = SymbolIndex::default();
        idx.insert_file("z.rs".into(), FileSymbols { lang: Lang::Rust,
            defs: vec![Def { kind: Kind::Function, name: "b".into(), line: 2 },
                       Def { kind: Kind::Function, name: "a".into(), line: 1 }], refs: vec![] });
        idx
    }

    #[test]
    fn save_load_roundtrips_and_bytes_are_stable() {
        let dir = tempfile::tempdir().unwrap();
        save(&sample(), dir.path().to_str().unwrap()).unwrap();
        let loaded = load(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(loaded, sample());
        // Re-serializing the same logical index yields byte-identical output (BTreeMap order,
        // not HashMap): write twice, compare.
        let p = index_path(dir.path().to_str().unwrap());
        let first = std::fs::read(&p).unwrap();
        save(&sample(), dir.path().to_str().unwrap()).unwrap();
        let second = std::fs::read(&p).unwrap();
        assert_eq!(first, second, "serialization must be byte-stable");
    }

    #[test]
    fn content_hash_is_line_ending_normalized() {
        assert_eq!(content_hash("a\r\nb\r\n"), content_hash("a\nb\n"));
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::store::tests -v 2>&1 | tail -8`
Expected: FAIL.

- [ ] **Step 3: Implement the store**

In `src/grounder/symbols/store.rs`:

```rust
use crate::grounder::symbols::model::SymbolIndex;
use std::path::{Path, PathBuf};

/// The on-disk index file under the project's grounding dir.
pub fn index_path(dir: &str) -> PathBuf {
    Path::new(dir).join(".rigger").join("symbols").join("index.json")
}

/// A line-ending-normalized content hash (CRLF and LF hash identically) so the same source
/// on Windows and Unix keys the same cache entry.
pub fn content_hash(src: &str) -> String {
    use std::hash::{Hash, Hasher};
    let normalized = src.replace("\r\n", "\n");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Serialize deterministically. `SymbolIndex` is BTreeMap-backed, so serde emits keys in
/// sorted order; we never iterate a `HashMap` here, so the bytes are stable across processes.
pub fn save(idx: &SymbolIndex, dir: &str) -> Result<(), String> {
    let path = index_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("symbols: mkdir: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(idx).map_err(|e| format!("symbols: serialize: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("symbols: write {}: {e}", path.display()))
}

/// Load the persisted index, or `None` when absent/unreadable (cold start).
pub fn load(dir: &str) -> Option<SymbolIndex> {
    let bytes = std::fs::read(index_path(dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}
```

Add the gitignore entry so the derived index is never committed - Modify `.gitignore`:

```gitignore
# Persisted symbol index (spec 15): a rebuildable projection of the tree, per-machine.
.rigger/symbols/
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::store::tests -v 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/store.rs .gitignore
git commit -m "feat(symbols): deterministic persist/load + line-ending-normalized hash (spec 15, unit 3)"
```

### Task 7: Incremental reindex (only changed files)

**Files:**
- Modify: `src/grounder/symbols/mod.rs` (add `reindex_files`)

**Interfaces:**
- Produces: `symbols::reindex_files(root: &str, idx: &mut SymbolIndex, files: &[String], override_lang: Option<Lang>)`.

- [ ] **Step 1: Write the failing test**

Add to `src/grounder/symbols/mod.rs` tests:

```rust
#[test]
fn reindex_replaces_only_the_named_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
    std::fs::write(dir.path().join("b.rs"), "fn two() {}\n").unwrap();
    let mut idx = build_index(root, None);
    // Change only a.rs; reindex it.
    std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
    reindex_files(root, &mut idx, &["a.rs".into()], None);
    assert!(idx.definitions_named("oneprime").iter().count() == 1);
    assert!(idx.definitions_named("one").is_empty());        // old symbol gone from a.rs
    assert!(idx.definitions_named("two").iter().count() == 1); // b.rs untouched
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::tests::reindex_replaces_only -v 2>&1 | tail -6`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `src/grounder/symbols/mod.rs`:

```rust
/// Re-extract ONLY `files` into `idx` (replacing each named file's entry), leaving every
/// other file's symbols intact - the incremental freshening `reindex` calls after integrate.
pub fn reindex_files(root: &str, idx: &mut SymbolIndex, files: &[String], override_lang: Option<Lang>) {
    for rel in files {
        if let Some(entry) = registry::for_path(rel, override_lang) {
            let abs = Path::new(root).join(rel);
            if let Ok(src) = std::fs::read_to_string(&abs) {
                if let Ok(fs) = extract::extract(&src, entry.lang, &entry.language, entry.tags_query) {
                    idx.insert_file(rel.clone(), fs);
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::tests::reindex_replaces_only -v 2>&1 | tail -6`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/mod.rs
git commit -m "feat(symbols): incremental reindex of named files (spec 15, unit 3)"
```

---

## UNIT 4 - the `symbols` grounder + ranking + wiring + validation

### Task 8: The `Symbols` grounder (ranking, precise contract)

**Files:**
- Create: `src/grounder/symbols/grounder.rs`

**Interfaces:**
- Consumes: `grounder::{Grounder, Ref}`, `model::SymbolIndex`, `store`, `build_index`, `reindex_files`.
- Produces: `symbols::grounder::Symbols` implementing `Grounder`; `Symbols::open(root: &str, override_lang: Option<Lang>) -> Symbols` (loads or builds + persists the index).

- [ ] **Step 1: Write the failing test**

In `src/grounder/symbols/grounder.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::Grounder;

    #[test]
    fn ranks_a_definition_above_an_incidental_prose_mention() {
        let dir = tempfile::tempdir().unwrap();
        // combat.rs DEFINES apply_damage; notes.rs only MENTIONS it in a comment.
        std::fs::write(dir.path().join("combat.rs"),
            "fn apply_damage(x: u8) -> u8 { x }\n").unwrap();
        std::fs::write(dir.path().join("notes.rs"),
            "// TODO: think about apply_damage someday\nfn unrelated() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
        let refs = g.ground("apply_damage", 5);
        assert!(!refs.is_empty());
        // The DEFINITION site outranks the prose mention: combat.rs is first.
        assert_eq!(refs[0].file, "combat.rs");
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::grounder::tests::ranks_a_definition -v 2>&1 | tail -8`
Expected: FAIL.

- [ ] **Step 3: Implement the grounder + ranking**

In `src/grounder/symbols/grounder.rs`:

```rust
use crate::grounder::symbols::model::{Lang, SymbolIndex};
use crate::grounder::symbols::{build_index, reindex_files, store};
use crate::grounder::{Grounder, Ref};
use std::sync::Mutex;

/// The `symbols` grounder over the persisted index. Serves the PRECISE contract: ranks a
/// definition-name match above a reference above an incidental prose mention (5.5.6).
pub struct Symbols {
    root: String,
    override_lang: Option<Lang>,
    idx: Mutex<SymbolIndex>,
}

impl Symbols {
    /// Load the persisted index, or build + persist it on a cold start.
    pub fn open(root: &str, override_lang: Option<Lang>) -> Symbols {
        let idx = store::load(root).unwrap_or_else(|| {
            let built = build_index(root, override_lang);
            let _ = store::save(&built, root);
            built
        });
        Symbols { root: root.to_string(), override_lang, idx: Mutex::new(idx) }
    }
}

impl Grounder for Symbols {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        if query.is_empty() || k == 0 { return Vec::new(); }
        let idx = self.idx.lock().unwrap();
        // The query's alphanumeric terms are the symbol candidates.
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| t.len() >= 2)
            .map(|t| t.to_string())
            .collect();
        // Score each (file, line): a DEFINITION whose name is a term = 3; a REFERENCE = 2;
        // (incidental prose is not indexed as a symbol, so it never appears - it ranks 0).
        // Deterministic ordering: higher score first, then file, then line.
        let mut scored: Vec<(u8, String, u32, String)> = Vec::new();
        for term in &terms {
            for d in idx.definitions_named(term) {
                // find which file this def is in
                for (path, fs) in idx.files() {
                    if fs.defs.iter().any(|x| x.name == d.name && x.line == d.line) {
                        scored.push((3, path.clone(), d.line, d.name.clone()));
                    }
                }
            }
            // references across all languages (grounding is precision, cross-lang ok here)
            for l in [Lang::Rust, Lang::CSharp, Lang::Js, Lang::Ts, Lang::Go, Lang::Python] {
                for (path, r) in idx.references_named(term, l) {
                    scored.push((2, path.to_string(), r.line, term.clone()));
                }
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        scored.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2);
        scored.into_iter().take(k)
            .map(|(_, file, line, name)| Ref { file, line, text: name })
            .collect()
    }

    fn reindex(&self, _src_dir: &str, files: &[String]) {
        let mut idx = self.idx.lock().unwrap();
        reindex_files(&self.root, &mut idx, files, self.override_lang);
        let _ = store::save(&idx, &self.root);
    }
}
```

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::grounder::tests::ranks_a_definition -v 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/grounder.rs
git commit -m "feat(symbols): the Symbols grounder + definition-over-reference-over-prose ranking (spec 15, unit 4)"
```

### Task 9: Wire `symbols` into selection (loud when feature-off)

**Files:**
- Modify: `src/grounder/mod.rs` (`grounder_for`: `symbols` -> loud error when feature-off)
- Modify: `src/main.rs` (`select_grounder`: `symbols` -> `Symbols` when feature on)
- Test: `src/grounder/mod.rs`

**Interfaces:**
- Consumes: `symbols::grounder::Symbols` (feature on); `resolves_to_turbovec` pattern.
- Produces: `defaults.grounder: symbols` selects the `Symbols` grounder (feature on) or fails loudly (feature off).

- [ ] **Step 1: Write the failing test (feature-independent branch)**

Add to `src/grounder/mod.rs` tests:

```rust
#[test]
fn symbols_without_the_feature_is_a_loud_error_not_a_grep_fallback() {
    // The feature-INDEPENDENT resolver (this fn) never returns a Symbols grounder; when the
    // feature is off it must error loudly (like turbovec), never silently degrade to grep.
    let err = grounder_for("symbols", ".").unwrap_err();
    assert!(err.to_lowercase().contains("symbols"));
    assert!(err.contains("feature"));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test --no-default-features symbols_without_the_feature -v 2>&1 | tail -6`
Expected: FAIL (currently returns "unknown grounder").

- [ ] **Step 3: Implement the wiring**

In `src/grounder/mod.rs`, extend `grounder_for` and add a message fn:

```rust
/// The loud error when `symbols` is configured but the binary lacks the `symbols` feature.
/// Selection never silently degrades to grep (the same rule as turbovec).
pub fn symbols_feature_missing_error() -> String {
    "grounder \"symbols\" is configured but this binary was built without the symbols \
     feature; rebuild with the default features, or set `defaults.grounder: grep` \
     explicitly".to_string()
}
```

In `grounder_for`, add a match arm before the `other =>` catch-all:

```rust
        "symbols" => Err(symbols_feature_missing_error()),
```

In `src/main.rs::select_grounder` (the feature-aware resolver), add - guarded - before delegating to `grounder_for`:

```rust
    #[cfg(feature = "symbols")]
    if name.trim().eq_ignore_ascii_case("symbols") {
        return Ok(Box::new(rigger::grounder::symbols::grounder::Symbols::open(root, None)));
    }
```

(Match the exact shape of `select_grounder` in `src/main.rs`; it already branches feature-gated grounders before calling `grounder::grounder_for`.)

- [ ] **Step 4: Run both lanes**

Run: `cargo test --no-default-features symbols_without_the_feature -v 2>&1 | tail -6` -> PASS (loud error).
Run: `cargo build` (default, symbols on) -> selection returns `Symbols`. Add a default-features test that `select_grounder("symbols", tmp)` yields a working grounder.
Expected: both green.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/mod.rs src/main.rs
git commit -m "feat(symbols): wire symbols into selection, loud when feature-off (spec 15, unit 4)"
```

### Task 10: Cross-process determinism test (the hash-seed hazard)

**Files:**
- Modify: `tests/cli.rs`

**Interfaces:**
- Consumes: the installed/`CARGO_BIN_EXE_rigger` binary; `rigger reindex`/`rigger ground` or a new `rigger symbols-index` debug command if needed.

- [ ] **Step 1: Write the failing test**

Add to `tests/cli.rs` a test that builds the index in TWO separate processes over the same tree and asserts the persisted `index.json` is byte-identical - this is the check the in-process test cannot make (Rust `HashMap` seed randomization only differs across processes).

```rust
#[cfg(feature = "symbols")]
#[test]
fn symbol_index_is_byte_identical_across_processes() {
    let dir = temp_project();
    let root = dir.path();
    std::fs::write(root.join(".rigger").join("workflow.yml"), "defaults:\n  grounder: symbols\nstages: {}\n").unwrap();
    std::fs::write(root.join("m.rs"), "fn a(){} fn b(){} fn c(){}\n").unwrap();
    // Two independent processes ground the tree (each builds + persists the index).
    let _ = run_rigger(root, &["ground", "a", "1"]);
    let first = std::fs::read(root.join(".rigger").join("symbols").join("index.json")).unwrap();
    std::fs::remove_file(root.join(".rigger").join("symbols").join("index.json")).unwrap();
    let _ = run_rigger(root, &["ground", "a", "1"]);
    let second = std::fs::read(root.join(".rigger").join("symbols").join("index.json")).unwrap();
    assert_eq!(first, second, "the index must be byte-identical across processes");
}
```

- [ ] **Step 2: Run it to see it fail (or pass)**

Run: `cargo test --test cli symbol_index_is_byte_identical -v 2>&1 | tail -8`
Expected: PASS if the model is BTreeMap-clean; if it FAILS, a `HashMap`/`HashSet` leaked into the serialized path - find and replace it with `BTreeMap`/`BTreeSet`.

- [ ] **Step 3: Commit**

```bash
git add tests/cli.rs
git commit -m "test(symbols): cross-process byte-identical index (determinism, spec 15, unit 3)"
```

### Task 11: Replay validation vs turbovec (the switch gate)

**Files:**
- Create: `docs/superpowers/plans/notes/symbols-replay-eval.md` (the methodology + the recorded numbers)

**Interfaces:**
- Consumes: `rigger replay <run> --against <config-rev>` (spec 13 unit 2).

- [ ] **Step 1: Document + run the comparison**

The grounder's acceptance is empirical against the TURBOVEC default. On a recorded run, run:

```bash
# Baseline is the run as recorded (turbovec). Candidate is a config rev with
# defaults.grounder: symbols. `rigger replay` re-drives the recorded trajectory under the
# candidate config in an isolated namespace and prints the baseline-vs-candidate stats diff.
rigger replay latest --against <git-rev-with-symbols-config>
```

Record first-pass yield and escalation for turbovec (baseline) vs symbols and hybrid (candidate) in the notes file. The shipped `defaults.grounder` stays turbovec until symbols/hybrid **meets or exceeds turbovec by the stated margin** on this diff - grep is only a floor. This is a gate on flipping the default, not a code change; DO NOT change `defaults.grounder` in this spec.

- [ ] **Step 2: Commit the recorded methodology + numbers**

```bash
git add docs/superpowers/plans/notes/symbols-replay-eval.md
git commit -m "docs(symbols): replay-vs-turbovec eval methodology + baseline numbers (spec 15, unit 4)"
```

---

## UNIT 5 - the `hybrid` grounder

### Task 12: The `hybrid` grounder (symbols + turbovec)

**Files:**
- Create: `src/grounder/symbols/hybrid.rs`
- Modify: `src/main.rs` (`select_grounder`: `hybrid`)

**Interfaces:**
- Consumes: `symbols::grounder::Symbols`; the existing turbovec grounder (`grounder::turbovec::Turbovec`); `grounder::{Grounder, Ref}`.
- Produces: `symbols::hybrid::Hybrid` implementing `Grounder`; selected by `defaults.grounder: hybrid` when both features are on; degrades to `Symbols` when turbovec is absent.

- [ ] **Step 1: Write the failing test**

In `src/grounder/symbols/hybrid.rs`:

```rust
#[cfg(all(test, feature = "turbovec"))]
mod tests {
    use super::*;
    use crate::grounder::Grounder;

    #[test]
    fn hybrid_ranks_structural_matches_ahead_of_semantic_only_hits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("combat.rs"), "fn apply_damage(x: u8) -> u8 { x }\n").unwrap();
        let g = Hybrid::open(dir.path().to_str().unwrap(), None);
        let refs = g.ground("apply_damage", 5);
        // The structural definition is present and first (structure ranks ahead of a purely
        // semantic hit that shares no name).
        assert_eq!(refs[0].file, "combat.rs");
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test symbols::hybrid::tests::hybrid_ranks -v 2>&1 | tail -8`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `src/grounder/symbols/hybrid.rs`:

```rust
use crate::grounder::symbols::grounder::Symbols;
use crate::grounder::symbols::model::Lang;
use crate::grounder::{Grounder, Ref};

/// `hybrid`: structural symbol matches (from `Symbols`) ranked FIRST, then turbovec's
/// semantic hits fill the recall a name match misses. Feature-gated with turbovec; absent
/// turbovec, `Hybrid::open` is exactly the `Symbols` grounder (see `select_grounder`).
pub struct Hybrid {
    symbols: Symbols,
    #[cfg(feature = "turbovec")]
    vector: crate::grounder::turbovec::Turbovec,
}

impl Hybrid {
    #[cfg(feature = "turbovec")]
    pub fn open(root: &str, override_lang: Option<Lang>) -> Hybrid {
        Hybrid {
            symbols: Symbols::open(root, override_lang),
            vector: crate::grounder::turbovec::Turbovec::new(root)
                .unwrap_or_else(|_| crate::grounder::turbovec::Turbovec::new(root).expect("turbovec")),
        }
    }
}

#[cfg(feature = "turbovec")]
impl Grounder for Hybrid {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        let mut out = self.symbols.ground(query, k);            // structural first
        if out.len() < k {
            for r in self.vector.ground(query, k) {            // semantic fills the rest
                if !out.iter().any(|o| o.file == r.file && o.line == r.line) {
                    out.push(r);
                    if out.len() >= k { break; }
                }
            }
        }
        out
    }
    fn reindex(&self, src_dir: &str, files: &[String]) {
        self.symbols.reindex(src_dir, files);
        self.vector.reindex(src_dir, files);
    }
}
```

In `src/main.rs::select_grounder`, add (feature-gated):

```rust
    #[cfg(all(feature = "symbols", feature = "turbovec"))]
    if name.trim().eq_ignore_ascii_case("hybrid") {
        return Ok(Box::new(rigger::grounder::symbols::hybrid::Hybrid::open(root, None)));
    }
    // With symbols but NOT turbovec, `hybrid` degrades to the symbols grounder.
    #[cfg(all(feature = "symbols", not(feature = "turbovec")))]
    if name.trim().eq_ignore_ascii_case("hybrid") {
        return Ok(Box::new(rigger::grounder::symbols::grounder::Symbols::open(root, None)));
    }
```

(Confirm `Turbovec::new`'s exact constructor signature in `src/grounder/turbovec.rs` and match it.)

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test symbols::hybrid::tests::hybrid_ranks -v 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/grounder/symbols/hybrid.rs src/main.rs
git commit -m "feat(symbols): the hybrid grounder (symbols + turbovec), degrades to symbols (spec 15, unit 5)"
```

---

### Task 13: Full gate - both lanes green + fmt + clippy

- [ ] **Step 1: Run the full gate**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings                 # default (symbols on)
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test                                                # default features
cargo test --no-default-features
```

Expected: all green. Fix any em-dash the style gate would catch (hyphens only).

- [ ] **Step 2: Final commit if fmt changed anything**

```bash
git add -A && git commit -m "chore(symbols): fmt + clippy clean, both lanes green (spec 15)"
```

---

## Self-Review

- **Spec coverage:** Unit 1 -> Tasks 1-3 (model, extraction, fan-out) + Task 5 (build). Unit 2 -> Task 4 (registry, five languages, auto-detect, override) + Task 5 (skip/graceful). Unit 3 -> Tasks 6-7 (persist, determinism, incremental) + Task 10 (cross-process determinism). Unit 4 -> Tasks 8-9, 11 (grounder, ranking, wiring, replay-vs-turbovec). Unit 5 -> Task 12 (hybrid). Done-when 5 criteria all covered. Global constraints (tree-sitter-free model, no-hash-ordered-containers, feature-gating, loud selection, default unchanged) each pinned by a task.
- **Placeholder scan:** none - every code step carries real code; the two "confirm the crate API against the resolved version" notes are actionable verification steps, not deferrals.
- **Type consistency:** `FileSymbols`/`Def`/`SymRef`/`Kind`/`Lang`/`SymbolIndex` are defined in Task 1 and used unchanged in 2-12; `extract(source, lang, ts_language, tags_query)` signature is stable across Tasks 2, 5, 7; `Symbols::open`/`Hybrid::open(root, override_lang)` consistent.
