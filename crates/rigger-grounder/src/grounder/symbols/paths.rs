//! Project path rules the code store's extractor reads off a path alone: whether a path names
//! dev-only code, and which files an out-of-line `mod` declaration names.

use crate::grounder::symbols::model::{Def, Lang};
use crate::grounder::symbols::registry::for_path;

/// DEV-ONLY PATHS (spec 116 criterion 1): whether `path` (a '/'-separated, project-relative
/// path) names dev-only code by where it lives. The ONE path predicate the graph and ranking
/// share: the code store's extractor ([`crate::facts`]) gives such a file the test scope,
/// [`crate::grounder::design::extract`]'s link pass drops a link to it, and the grounder's
/// ranking marks its every location test. Case-sensitive, read off the path alone, in two parts:
///
/// - Ungated, in every language: a DIRECTORY component (every segment except the file's own
///   name, [`dir_of`]) `tests`, `benches`, `examples` or `__tests__`. So `tests/foo.rs` and
///   `crate/benches/b.rs` are dev-only while `src/testsuite.rs` and `src/tests.rs` are not.
/// - Gated on the language [`for_path`] gives the path's own extension (a dotfile such as
///   `.cs` has none): Go `*_test.go`; Python `test_*.py` and `*_test.py`; JS and TS `*.test.*`
///   and `*.spec.*`; C# a directory component ending `Test` or `Tests` (its last dot-separated
///   segment ending so, neither suffix holding a `.`). A file whose extension registers no
///   language, one indexed under a `--language` override included, matches the ungated part
///   only.
pub(crate) fn is_dev_only(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lang = for_path(path, None).map(|entry| entry.lang);
    let dev_dir = |dir: &str| {
        matches!(dir, "tests" | "benches" | "examples" | "__tests__")
            || (lang == Some(Lang::CSharp) && (dir.ends_with("Test") || dir.ends_with("Tests")))
    };
    dir_of(path).split('/').any(dev_dir)
        || match lang {
            Some(Lang::Go) => name.ends_with("_test.go"),
            Some(Lang::Python) => name.starts_with("test_") || name.ends_with("_test.py"),
            Some(Lang::Js | Lang::Ts) => name.contains(".test.") || name.contains(".spec."),
            Some(Lang::Rust | Lang::CSharp) | None => false,
        }
}

/// The directory component of a project-relative `path` (every segment except the file's own
/// name) - the ONE place this repo splits a path this way, shared by [`is_dev_only`],
/// [`module_dir`] and [`module_candidates`]' `#[path]`-override branch, so the
/// split-off-the-last-`/` idiom exists exactly once.
fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// `rest` under the directory `base`: `<base>/<rest>`, or `rest` alone when `base` is empty (the
/// project root) - the ONE place a module path joins a directory, shared by [`module_dir`] and
/// [`module_candidates`].
fn join_under(base: &str, rest: &str) -> String {
    if base.is_empty() {
        rest.to_string()
    } else {
        format!("{base}/{rest}")
    }
}

/// Resolves `.` and `..` components, at ANY position, in a `/`-separated LOGICAL path string - the
/// ONE place [`module_candidates`]' `#[path]`-override branch normalizes a raw attribute
/// value joined onto a directory before a caller matches it against the code files it holds. Those
/// paths are always the project's own already-clean relative paths (never containing a literal `.`
/// or `..` segment), so `std::fs::canonicalize` does not apply here - there is no real filesystem
/// to resolve against at every intermediate step, only a string key to compute, matching rustc's
/// own purely lexical handling of a `#[path]` value. A `.` segment is dropped; a `..` segment pops
/// the most recently pushed real segment off the stack.
///
/// Round 9 (ADJUDICATION `adj-u86c1-r8-verdict-reject`, upheld finding
/// `adv-u86c1-r8-overcount-dotdot-silently-excludes-an-unrelated-real-file`): a `..` reached once
/// the stack is already EMPTY - an override whose `..` count exceeds the declaring directory's own
/// depth - is an IRRECOVERABLE overflow, not a harmless one to keep walking past. `Vec::pop` on an
/// empty stack is Rust's own silent no-op with no signal to the caller; continuing the walk after
/// one and joining whatever segments remain computes a string that is syntactically a valid
/// relative path but semantically WRONG (it claims to walk back further than the declaring file's
/// own tree allows) - and nothing stops that wrong-but-plausible string from coincidentally
/// matching a real, UNRELATED file's key, at which point the index lookup over the candidates finds
/// a match and silently drops that unrelated file's whole entity set from the graph. So this now
/// returns `None` the instant a `..` has nothing real left to pop, letting the caller name no
/// candidate rather than one that only LOOKS like a real path.
fn normalize_logical_path(path: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            real => stack.push(real),
        }
    }
    Some(stack.join("/"))
}

/// Round 9 (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`,
/// NORMALIZATION rule): whether a raw `#[path]` override value `p` is a shape
/// [`normalize_logical_path`] must never be asked to join onto any base directory at all - an
/// OS-absolute path, a URI scheme, or a Windows drive letter, none of which a real project's
/// code file paths (always `/`-separated, repo-relative, drive-and-scheme-free keys) could ever
/// hold. Verified against real rustc before this was written: `#[path = "/etc/hostname"]` reads
/// `/etc/hostname` directly, an OS-absolute path used AS-IS rather than joined onto anything.
/// Checked on the RAW override, before joining onto the base directory: joining first and only
/// then normalizing would let a leading `/` on `p` silently vanish via the SAME
/// `"" | "." => {}` rule that drops an ordinary `.` segment (an empty split segment either way),
/// reproducing the identical wrong-but-plausible-collapsed-string failure
/// [`normalize_logical_path`]'s own `..`-overflow guard closes, through a different front door -
/// most sharply for a ROOT-level declaring file, whose empty base directory lets the join
/// degenerate to `p` completely unchanged.
fn path_override_names_an_unresolvable_shape(p: &str) -> bool {
    if p.starts_with('/') {
        return true;
    }
    if p.contains("://") {
        return true;
    }
    let bytes = p.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Round 7 (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): the MODULE DIRECTORY a
/// declaring file `path`'s own out-of-line children resolve under, read off the file's name
/// alone, never simply `path`'s own directory (the round-6 bug,
/// `sdet-u86c1-r6-out-of-line-mod-resolution-uses-declaring-files-directory-not-rusts-own-module-
/// nesting-path`). A file named `mod.rs`, `lib.rs` or `main.rs` owns its OWN directory: its
/// `mod name;` children are same-directory siblings. Every OTHER file is taken as a leaf submodule
/// whose children sit in a NEW subdirectory named after its own stem (`foo.rs` -> `foo/`).
/// The rule reads the file's name alone, while rustc decides by whether the file is a crate root,
/// a `mod.rs`, or loaded through `#[path]`, so the two part ways on some files. Among them: a
/// crate root of another name (`src/bin/tool.rs`, `build.rs`) and a `#[path]`-loaded file not
/// named `mod.rs`, `lib.rs` or `main.rs`, both of which this rule puts one directory deeper than
/// rustc; and a submodule file named `lib.rs` or `main.rs` (`src/sub/main.rs`), which this rule
/// gives its own directory where rustc uses its stem.
fn module_dir(path: &str) -> String {
    let dir = dir_of(path);
    let base = path.rsplit('/').next().unwrap_or(path);
    if base == "mod.rs" || base == "lib.rs" || base == "main.rs" {
        dir.to_string()
    } else {
        join_under(dir, base.strip_suffix(".rs").unwrap_or(base))
    }
}

/// The candidate files this resolver names for the `mod` declaration `d` in `declaring_path`, in
/// order, the first a caller holds winning (spec 117 criterion 8): a pure function of the
/// declaring path and the declaration, reading no index, so a caller matches them against
/// whatever files it holds. It applies Rust's file-per-module convention as the rules below
/// spell it, and it departs from rustc on some declarations. Among them: every file
/// [`module_dir`]'s name-only rule places differently from rustc; an enclosing inline mod carrying
/// its own `#[path]`, which contributes its name to the chain, not its path; a raw-identifier
/// declaration (`mod r#match;` names `r#match.rs` where rustc reads `match.rs`); a path given
/// through `#[cfg_attr(.., path = "..")]`, which is not taken as an override; and both
/// `<name>.rs` and `<name>/mod.rs` present, which rustc rejects (E0761) and where the first
/// candidate wins here. Round 9
/// (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`) sets out the
/// `#[path]`-override rules this code applies as one enumerated list:
/// 1. A `#[path = ".."]` override on the declaration ([`Def::path_override`], captured
///    structurally at extraction time) takes precedence over the convention entirely, naming one
///    file or none. BASE DIRECTORY: when the declaration sits at the declaring file's own top
///    level (the overwhelmingly common case), the base is the declaring file's own DIRECTORY
///    ([`dir_of`]) - never [`module_dir`], since `#[path]` is rustc's own escape hatch FROM the
///    file-per-module convention and is, at the file's top level, unconditionally
///    directory-of-file-relative regardless of whether the declaring file is itself a
///    directory-style module. When the declaration is instead nested inside one or more INLINE
///    `mod outer { .. }` blocks ([`Def::enclosing_inline_module_path`]), the base is the
///    declaring file's own MODULE directory ([`module_dir`]) with the enclosing inline chain
///    appended - one directory component per inline module, outermost first - verified against
///    real rustc for a plain inline parent such as `mod outer`
///    (`Def::enclosing_inline_module_path`'s doc gives examples where the chain departs).
///    ABSOLUTE / SCHEME REJECTION: an override that is itself an OS-absolute path, a URI scheme,
///    or a Windows drive letter
///    ([`path_override_names_an_unresolvable_shape`]) names no file, joined onto nothing - rustc
///    uses an absolute `#[path]` as-is, never relative to any Rust-side directory, so it can never
///    name a key a project-relative index holds; left unguarded, a root-level declaring file's
///    EMPTY base directory would let the join degenerate to the raw override unchanged, and its
///    leading `/` would then silently drop during normalization (the SAME rule that drops an
///    ordinary `.` segment), coincidentally colliding with an unrelated real file exactly like the
///    `..`-overflow defect below. NORMALIZATION: the joined `<base>/<override>` string is then
///    [`normalize_logical_path`]'d (round 8, `sdet-u86c1-r7-path-override-dotdot-unresolved` /
///    `adv-u86c1-r7-dot-slash-override-also-unresolved-not-just-dotdot`): a raw `#[path]` value
///    may itself contain `.` or `..` segments at any position (rustc resolves those purely
///    lexically too), and an index's keys are always already-clean, so an unnormalized join would
///    silently match no key for such a value. Round 9
///    (`adv-u86c1-r8-overcount-dotdot-silently-excludes-an-unrelated-real-file`): normalization can
///    itself fail - a `..` count exceeding the declaring directory's own depth - and that failure
///    names no file rather than a collapsed-but-wrong string that might coincidentally name a
///    real, unrelated file.
/// 2. Otherwise the convention's two, under the declaring file's [`module_dir`] with the enclosing
///    inline-mod chain appended, as the override nested in one takes it, so `mod outer { mod x; }`
///    in `src/lib.rs` names `src/outer/x.rs` first: the flat `<dir>/<name>.rs`, then the
///    directory-module form `<dir>/<name>/mod.rs`.
pub(crate) fn module_candidates(declaring_path: &str, d: &Def) -> Vec<String> {
    let module_dir = module_dir(declaring_path);
    let inline_dir = match &d.enclosing_inline_module_path {
        Some(chain) => join_under(&module_dir, chain),
        None => module_dir,
    };
    match &d.path_override {
        Some(p) if path_override_names_an_unresolvable_shape(p) => Vec::new(),
        Some(p) => {
            let base = match d.enclosing_inline_module_path {
                Some(_) => inline_dir.as_str(),
                None => dir_of(declaring_path),
            };
            normalize_logical_path(&join_under(base, p))
                .into_iter()
                .collect()
        }
        None => [format!("{}.rs", d.name), format!("{}/mod.rs", d.name)]
            .map(|leaf| join_under(&inline_dir, &leaf))
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use crate::grounder::symbols::model::{Def, FileSymbols};

    /// A crate root at the walk's root: a declaring path with no directory.
    const ROOT_DECLARING_FILE: &str = "lib.rs";

    /// A declaring file owning no module directory, so its own stem is its module directory.
    const NESTED_DECLARING_FILE: &str = "src/foo.rs";

    /// The one out-of-line `mod` declaration `symbols` holds.
    fn out_of_line_declaration(symbols: &FileSymbols) -> &Def {
        symbols
            .defs
            .iter()
            .find(|d| d.is_out_of_line_module)
            .expect("the source declares a module out of line")
    }

    /// Spec 117 criterion 8: the candidates [`super::module_candidates`] names, in order, for each
    /// declaration the table holds, read off the declaring path and the declaration alone - a
    /// `#[path]` override's one file under the declaring file's directory, or its module
    /// directory and inline chain when nested, none for an absolute or over-climbing override;
    /// else `x.rs` then `x/mod.rs` under the module directory and the enclosing inline-mod chain.
    #[test]
    fn module_candidates_name_the_candidates_of_each_tabled_declaration_in_order() {
        use crate::graph_fixtures::{DECLARING_FILE, DIR_MODULE_FILE, MODULE_FILE};
        use crate::symbol_index::{ABSOLUTE_PATH_MOD_X, INLINE_MOD_X, MOD_X, PATH_MOD_X};
        let cases: [(&str, &str, &[&str]); 10] = [
            (DECLARING_FILE, PATH_MOD_X, &["src/gen/real.rs"]),
            (
                ROOT_DECLARING_FILE,
                "#[path = \"real.rs\"]\nmod x;\n",
                &["real.rs"],
            ),
            (
                DECLARING_FILE,
                "mod outer {\n    #[path = \"real.rs\"]\n    mod x;\n}\n",
                &["src/outer/real.rs"],
            ),
            (DECLARING_FILE, ABSOLUTE_PATH_MOD_X, &[]),
            (DECLARING_FILE, "#[path = \"../../x.rs\"]\nmod x;\n", &[]),
            (DECLARING_FILE, MOD_X, &[MODULE_FILE, DIR_MODULE_FILE]),
            (
                NESTED_DECLARING_FILE,
                MOD_X,
                &["src/foo/x.rs", "src/foo/x/mod.rs"],
            ),
            (ROOT_DECLARING_FILE, MOD_X, &["x.rs", "x/mod.rs"]),
            (
                DECLARING_FILE,
                INLINE_MOD_X,
                &["src/outer/x.rs", "src/outer/x/mod.rs"],
            ),
            (
                ROOT_DECLARING_FILE,
                INLINE_MOD_X,
                &["outer/x.rs", "outer/x/mod.rs"],
            ),
        ];
        for (declaring, src, candidates) in cases {
            let symbols = crate::symbol_index::extracted("rs", src);
            assert_eq!(
                super::module_candidates(declaring, out_of_line_declaration(&symbols)),
                candidates,
                "{declaring}: {src}"
            );
        }
    }

    /// Spec 117 criterion 8: a declaring file whose name lacks `.rs` (one indexed as Rust under a
    /// `--language` override) keeps its whole name as its module directory's last component, so
    /// `src/foo` names the candidates `src/foo.rs` does.
    #[test]
    fn module_candidates_of_a_declaring_file_without_the_rust_extension_keep_its_whole_name() {
        let symbols = crate::symbol_index::extracted("rs", crate::symbol_index::MOD_X);
        let declaration = &symbols.defs[0];
        assert_eq!(
            super::module_candidates("src/foo", declaration),
            super::module_candidates(NESTED_DECLARING_FILE, declaration)
        );
    }
}
