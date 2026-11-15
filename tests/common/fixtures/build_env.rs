//! The declared build environment's shared rows (spec 125 criterion 4): the declaration the
//! criterion names, the `build` block that declares a set of variables, in its typed and its
//! workflow-file form, and every `build.env` entry the one check refuses with the exact refusal
//! it names. It names only the domain crate, so the gate adapter's, the config adapter's, the
//! conductor's and the root crate's tests read one copy.

/// The criterion's declaration: `B` over the site's build cache, `J` the jobs cap, `L` a literal.
pub const DECLARED_BJL: [(&str, &str); 3] =
    [("B", "{build_cache}/x"), ("J", "{jobs}"), ("L", "lit")];

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
/// itself, an unknown placeholder and a stray brace.
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
