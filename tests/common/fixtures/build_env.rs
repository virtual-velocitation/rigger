//! The declared build environment's shared rows (spec 125 criteria 4 and 6): the declaration the
//! criterion names, the `build` block that declares a set of variables, in its typed and its
//! workflow-file form, the Rust gate template set's `env` and the `build.env` a scaffold from that
//! set writes, every `build.env` entry the one check refuses with the exact refusal it names and
//! the `set.yml` line declaring it, the longest `env` name a scaffold writes and loads back with
//! the `set.yml` lines declaring a name of a given length, the `build.env` a scaffold writes of
//! them and the refusal of a set declaring one past it, and every `build` block carrying a
//! retired key with what validation answers for it.
//! It names only the domain crate, so the gate adapter's, the config adapter's, the conductor's
//! and the root crate's tests read one copy.

/// The criterion's declaration: `B` over the site's build cache, `J` the jobs cap, `L` a literal.
pub const DECLARED_BJL: [(&str, &str); 3] =
    [("B", "{build_cache}/x"), ("J", "{jobs}"), ("L", "lit")];

/// The Rust gate template set's `env`, in name order: cargo's job count over `{jobs}` and its
/// build cache over `{build_cache}`.
pub const RUST_SET_ENV: [(&str, &str); 2] = [
    ("CARGO_BUILD_JOBS", "{jobs}"),
    ("CARGO_TARGET_DIR", "{build_cache}"),
];

/// The `build.env` value a scaffold from the Rust gate template set writes under `build:`, byte
/// for byte: [`RUST_SET_ENV`] as a flow map.
pub const RUST_SET_ENV_WRITTEN: &str =
    "{CARGO_BUILD_JOBS: \"{jobs}\", CARGO_TARGET_DIR: \"{build_cache}\"}";

/// The `build` block declaring `pairs` under `build.env`, with `build.jobs` set to `jobs` and
/// every other key at its default.
pub fn build_declaring(pairs: &[(&str, &str)], jobs: u32) -> rigger_domain::config::BuildConfig {
    rigger_domain::config::BuildConfig {
        env: pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        jobs,
        ..Default::default()
    }
}

/// The workflow file's `build` block declaring `pairs` under `build.env` (each name and value a
/// double-quoted scalar), with `build.jobs` set to `jobs` and no build budget, so a gate it runs
/// takes no slot from a machine-wide pool.
pub fn build_block_declaring(pairs: &[(&str, &str)], jobs: u32) -> String {
    let mut block = format!("build:\n  max_concurrent: 0\n  jobs: {jobs}\n");
    if !pairs.is_empty() {
        block.push_str("  env:\n");
        for (name, value) in pairs {
            block.push_str(&format!("    {name:?}: {value:?}\n"));
        }
    }
    block
}

/// One `build.env` entry the check refuses, and the refusal, which names the variable.
pub struct RefusedDeclaration {
    pub name: &'static str,
    pub value: &'static str,
    pub message: &'static str,
}

impl RefusedDeclaration {
    /// The `env:` line of a gate template set's `set.yml` declaring this entry alone, as a flow
    /// map of its name and value, each double-quoted.
    pub fn set_env_line(&self) -> String {
        format!("env: {{{:?}: {:?}}}\n", self.name, self.value)
    }
}

const fn refused(
    name: &'static str,
    value: &'static str,
    message: &'static str,
) -> RefusedDeclaration {
    RefusedDeclaration {
        name,
        value,
        message,
    }
}

/// Every refused `build.env` entry: a name that is not a variable name, each name rigger sets
/// itself and one of them in another letter case, an unknown placeholder and a stray brace.
pub const REFUSED_DECLARATIONS: &[RefusedDeclaration] = &[
    refused(
        "9B",
        "lit",
        "build.env: \"9B\" is not a variable name; a name is a letter or underscore followed by letters, digits and underscores",
    ),
    refused(
        "B-1",
        "lit",
        "build.env: \"B-1\" is not a variable name; a name is a letter or underscore followed by letters, digits and underscores",
    ),
    refused(
        "",
        "lit",
        "build.env: \"\" is not a variable name; a name is a letter or underscore followed by letters, digits and underscores",
    ),
    refused(
        "É",
        "lit",
        "build.env: \"É\" is not a variable name; a name is a letter or underscore followed by letters, digits and underscores",
    ),
    refused(
        "RIGGER_STORE_FENCE_DIR",
        "lit",
        "build.env: RIGGER_STORE_FENCE_DIR is set by rigger itself; declare another name",
    ),
    refused(
        "RIGGER_GATE_SCRATCH",
        "lit",
        "build.env: RIGGER_GATE_SCRATCH is set by rigger itself; declare another name",
    ),
    refused(
        "RIGGER_RUN_BASE",
        "lit",
        "build.env: RIGGER_RUN_BASE is set by rigger itself; declare another name",
    ),
    refused(
        "RIGGER_SPAWN",
        "lit",
        "build.env: RIGGER_SPAWN is set by rigger itself; declare another name",
    ),
    refused(
        "rigger_spawn",
        "lit",
        "build.env: rigger_spawn is set by rigger itself; declare another name",
    ),
    refused(
        "RIGGER_GATE_SOCKET",
        "lit",
        "build.env: RIGGER_GATE_SOCKET is set by rigger itself; declare another name",
    ),
    refused(
        "KURRENTDB_CONN",
        "lit",
        "build.env: KURRENTDB_CONN is set by rigger itself; declare another name",
    ),
    refused(
        "CLAUDE_CODE_OAUTH_TOKEN",
        "lit",
        "build.env: CLAUDE_CODE_OAUTH_TOKEN is set by rigger itself; declare another name",
    ),
    refused(
        "RIGGER_CLAUDE_CODE_OAUTH_TOKEN",
        "lit",
        "build.env: RIGGER_CLAUDE_CODE_OAUTH_TOKEN is set by rigger itself; declare another name",
    ),
    refused(
        "B",
        "{cache}",
        "build.env: B: unknown placeholder {cache} in \"{cache}\"; a value knows {build_cache} and {jobs}, and {{ and }} for a literal brace",
    ),
    refused(
        "B",
        "a{}b",
        "build.env: B: unknown placeholder {} in \"a{}b\"; a value knows {build_cache} and {jobs}, and {{ and }} for a literal brace",
    ),
    refused(
        "J",
        "{JOBS}",
        "build.env: J: unknown placeholder {JOBS} in \"{JOBS}\"; a value knows {build_cache} and {jobs}, and {{ and }} for a literal brace",
    ),
    refused(
        "B",
        "{build_cache",
        "build.env: B: a stray brace in \"{build_cache\"; write {{ or }} for a literal brace",
    ),
    refused(
        "B",
        "a}b",
        "build.env: B: a stray brace in \"a}b\"; write {{ or }} for a literal brace",
    ),
    refused(
        "B",
        "{{build_cache}",
        "build.env: B: a stray brace in \"{{build_cache}\"; write {{ or }} for a literal brace",
    ),
    refused(
        "B",
        "{a{jobs}}",
        "build.env: B: a stray brace in \"{a{jobs}}\"; write {{ or }} for a literal brace",
    ),
    refused(
        "B",
        "}}}",
        "build.env: B: a stray brace in \"}}}\"; write {{ or }} for a literal brace",
    ),
];

/// The length of the longest key YAML reads plain: the longest `env` name a scaffold writes on
/// its `env` line and loads back.
pub const LONGEST_PLAIN_KEY: usize = 1024;

/// The `env:` lines of a gate template set's `set.yml` declaring one variable, valued `x`, whose
/// name is `len` `A`s, spelled as an explicit key, the one spelling YAML reads of a name longer
/// than [`LONGEST_PLAIN_KEY`].
pub fn set_env_line_named(len: usize) -> String {
    format!("env:\n  ? {}\n  : x\n", "A".repeat(len))
}

/// The `build.env` value a scaffold writes under `build:` from the `env` that
/// [`set_env_line_named`] declares, byte for byte: its one variable as a flow map, the name plain.
pub fn env_written_named(len: usize) -> String {
    format!("{{{}: \"x\"}}", "A".repeat(len))
}

/// Why a set whose `env` name is one character past [`LONGEST_PLAIN_KEY`] is refused: the
/// workflow it scaffolds, which writes that name plain on its `env` line `line`, fails to load.
pub fn overlong_name_refusal(line: usize) -> String {
    format!(
        "the workflow it scaffolds fails to load: config: parse workflow: did not find expected \
         ',' or '}}' at line {line} column 1034, while parsing a flow mapping at line {line} \
         column 8"
    )
}

/// The refusal of a workflow still carrying a `build.wrapper` value.
const WRAPPER_RETIRED: &str = "the workflow key `build.wrapper` is retired (rigger exports only what `build.env` declares; declare there each variable your builds need (your build cache over `{build_cache}`, your job count over `{jobs}`)): delete the `wrapper:` line under `build:` from your workflow.yml (config key: build.wrapper)";

/// The refusal of a workflow still carrying a `build.cache_dir` value.
const CACHE_DIR_RETIRED: &str = "the workflow key `build.cache_dir` is retired (rigger exports only what `build.env` declares; declare there each variable your builds need (your build cache over `{build_cache}`, your job count over `{jobs}`)): delete the `cache_dir:` line under `build:` from your workflow.yml (config key: build.cache_dir)";

/// One workflow-file `build` block carrying a retired key, and what validation answers for it:
/// the refusal naming the key and the line to delete, or `None` when the value is empty or
/// whitespace-only and validation passes.
pub struct RetiredKeyRow {
    pub why: &'static str,
    pub block: &'static str,
    pub refusal: Option<&'static str>,
}

const fn retired(
    why: &'static str,
    block: &'static str,
    refusal: Option<&'static str>,
) -> RetiredKeyRow {
    RetiredKeyRow {
        why,
        block,
        refusal,
    }
}

/// Every retired `build` key's value class: `auto`, `off` and a named wrapper are refused, a
/// cache directory is refused, an empty or whitespace-only value of either passes, and a block
/// carrying both keys is refused for `build.wrapper`, whichever line the file writes first.
pub const RETIRED_BUILD_KEYS: &[RetiredKeyRow] = &[
    retired(
        "wrapper auto",
        "build:\n  wrapper: auto\n",
        Some(WRAPPER_RETIRED),
    ),
    retired(
        "wrapper off",
        "build:\n  wrapper: off\n",
        Some(WRAPPER_RETIRED),
    ),
    retired(
        "a named wrapper",
        "build:\n  wrapper: sccache\n",
        Some(WRAPPER_RETIRED),
    ),
    retired("an empty wrapper", "build:\n  wrapper: \"\"\n", None),
    retired(
        "a whitespace-only wrapper",
        "build:\n  wrapper: \"  \"\n",
        None,
    ),
    retired(
        "a cache dir",
        "build:\n  cache_dir: /shared/cache\n",
        Some(CACHE_DIR_RETIRED),
    ),
    retired("an empty cache dir", "build:\n  cache_dir: \"\"\n", None),
    retired(
        "a whitespace-only cache dir",
        "build:\n  cache_dir: \"  \"\n",
        None,
    ),
    retired(
        "both keys, the cache dir written first",
        "build:\n  cache_dir: /shared/cache\n  wrapper: auto\n",
        Some(WRAPPER_RETIRED),
    ),
];
