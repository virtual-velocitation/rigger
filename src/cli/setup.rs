use super::*;

/// `rigger version` (and `rigger --version` / `-V`): print the crate version and the build
/// provenance, so any agent can identify the exact binary without guessing.
pub(crate) fn cmd_version() -> Res {
    println!("{}", version_line());
    Ok(())
}

/// Canonicalize a git remote URL so the ssh, https, and `.git`-suffixed forms of ONE repo
/// all reduce to the SAME string (spec 09): strip the scheme (`https://`, `ssh://`,
/// `git://`) and any `user@` credential, lowercase the host, drop a trailing `.git` and
/// surrounding slashes, and normalize the scp-style `host:path` separator to `/`. So
/// `git@github.com:Acme/Repo.git`, `https://github.com/Acme/Repo.git`, and
/// `ssh://git@github.com/Acme/Repo` all normalize to `github.com/Acme/Repo`, minting one
/// identity. Pure, so the "ssh/https/.git forms mint identical ids" invariant is unit-tested.
fn normalize_origin_url(url: &str) -> String {
    let mut s = url.trim();
    // Strip the scheme (everything up to and including "://").
    if let Some(idx) = s.find("://") {
        s = &s[idx + 3..];
    }
    // Strip any "user@" credential prefix (e.g. the ssh `git@`).
    if let Some(idx) = s.find('@') {
        s = &s[idx + 1..];
    }
    // Split the host from the path on the first ':' (scp-style) or '/'.
    let (host, path) = match s.find([':', '/']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let host = host.to_ascii_lowercase();
    // Drop surrounding slashes and a single trailing `.git` from the path.
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        host
    } else {
        format!("{host}/{path}")
    }
}

/// The `origin` remote URL configured at `root`, or `None` when there is no `origin` remote
/// (or git is unavailable). Read via `git config --get remote.origin.url`, which needs no
/// network and no newer git than the rest of rigger already assumes.
fn origin_url_at(root: &Path) -> Option<String> {
    let out = subprocess::git_in(root)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Mint a fresh durable project id for `root` (spec 09): deterministically from the
/// normalized `origin` URL when a remote exists (so every clone of one repo mints the same
/// id, and the ssh/https/`.git` forms agree), else a random id when there is no remote to
/// anchor on. The result is a compact hex token, safe as a stream-namespace component.
fn mint_project_id(root: &Path) -> String {
    match origin_url_at(root) {
        Some(url) => format!("{:016x}", fnv1a_64(normalize_origin_url(&url).as_bytes())),
        None => uuid::Uuid::new_v4().simple().to_string(),
    }
}

/// What [`init_project`] did, PER ARTIFACT, so `rigger setup` / `rigger init` can
/// narrate exactly what changed and stay a silent no-op on a rerun that changed nothing
/// (spec 05, criterion 4: setup is re-runnable with no surprising output). The summary
/// is built from these fields ([`scaffold_summary_lines`]) so it can never claim a
/// scaffold action that was not performed - a gitignore-only repair reports only the
/// gitignore change (the honest-summary bar the loop already enforced on unit-5).
#[derive(Debug, Default)]
struct ScaffoldReport {
    /// True when this run newly wrote `.rigger/workflow.yml` (it was absent).
    wrote_workflow: bool,
    /// Agent files this run newly wrote (empty when they already existed).
    new_agents: Vec<String>,
    /// Fan-out helper files this run newly wrote under `.claude/agents/` (empty when they
    /// already existed).
    new_helpers: Vec<String>,
    /// True when this run installed or updated the SessionStart hook in
    /// `.claude/settings.json` (false when the hook was already present unchanged).
    wrote_hook: bool,
    /// `.gitignore` patterns this run newly appended (empty when every machine-local
    /// pattern was already ignored or tracked).
    gitignore_added: Vec<String>,
    /// The durable project id this run newly MINTED into `.rigger/project.id` (spec 09),
    /// or `None` when the file already existed and was left untouched.
    minted_id: Option<String>,
    /// True when this run newly wrote `.rigger/instructions/README.md` (it was absent).
    wrote_instructions_readme: bool,
    /// Gate scripts this run newly wrote under `.rigger/gates/` (empty when each already
    /// existed).
    new_gate_files: Vec<String>,
}

impl ScaffoldReport {
    /// True when this run created or modified ANY scaffold artifact. False means the
    /// scaffold was already complete and this run left the tree byte-for-byte identical.
    fn changed(&self) -> bool {
        self.wrote_workflow
            || !self.new_agents.is_empty()
            || !self.new_helpers.is_empty()
            || self.wrote_hook
            || !self.gitignore_added.is_empty()
            || self.minted_id.is_some()
            || self.wrote_instructions_readme
            || !self.new_gate_files.is_empty()
    }
}

/// Scaffold a project idempotently, returning a [`ScaffoldReport`] of what actually
/// changed. Every step is a no-op when its artifact already exists and matches, so a
/// rerun on an initialized project changes nothing and reports `changed: false`.
fn init_project(root: &Path) -> Result<ScaffoldReport, Box<dyn std::error::Error>> {
    // 1. Scaffold .rigger/.
    let rigger_dir = root.join(RIGGER_DIR);
    let agents_dir = rigger_dir.join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    let wrote_workflow = write_if_absent(&rigger_dir.join("workflow.yml"), SCAFFOLD_WORKFLOW)?;
    let instructions_dir = rigger_dir.join("instructions");
    std::fs::create_dir_all(&instructions_dir)?;
    let wrote_instructions_readme = write_if_absent(
        &instructions_dir.join(config_store::INSTRUCTIONS_README),
        SCAFFOLD_INSTRUCTIONS_README,
    )?;
    let gates_dir = rigger_dir.join("gates");
    std::fs::create_dir_all(&gates_dir)?;
    let mut new_gate_files = Vec::new();
    for (file, content) in SCAFFOLD_GATE_FILES {
        if write_if_absent(&gates_dir.join(file), content)? {
            new_gate_files.push(file.to_string());
        }
    }

    // 1b. Mint the durable project identity when absent (spec 09, Gap 20): a tracked
    // `.rigger/project.id` line so the identity survives directory renames and machine
    // moves instead of tracking the volatile directory basename. Deterministic from the
    // normalized `origin` URL when a remote exists (every clone mints the same id), random
    // otherwise. A present file is left untouched (`minted_id` stays `None`), so a rerun
    // never re-mints. A genuine write failure escalates (naming the artifact), never a
    // silent omission - identity is load-bearing.
    let id_path = rigger_dir.join(PROJECT_ID_FILE);
    let minted_id = if id_path.exists() {
        None
    } else {
        let id = mint_project_id(root);
        write_if_absent(&id_path, &format!("{id}\n"))?;
        Some(id)
    };

    // 2. Load the workflow to determine which agents are referenced, then only
    // scaffold those agents. This allows setup to skip scaffolding when the
    // workflow's referenced agents already exist (§05 setup hygiene). A genuine parse
    // failure (most importantly an unrecognized key) escalates as a real `Err` here -
    // never `.unwrap_or_default()` - because folding it into the empty set would make
    // `init_project` treat a broken PRESENT workflow.yml the same as a genuinely ABSENT
    // one, below, and silently re-scaffold the full default fleet over whatever agents the
    // operator had deliberately curated (spec 102's own goal: a config key rigger does not
    // read is an error, never silence).
    let referenced_agents = get_referenced_agent_ids(root)?;

    // If the workflow references agents, scaffold only those. If it references
    // nothing (should not happen with a valid workflow), scaffold all defaults
    // for backward compatibility (empty repo case) - reached now only when
    // `get_referenced_agent_ids` returned the empty set because workflow.yml is
    // genuinely ABSENT, never because a present one failed to parse.
    let agents_to_scaffold: Vec<(&str, &str)> = if referenced_agents.is_empty() {
        SCAFFOLD_AGENTS.to_vec()
    } else {
        SCAFFOLD_AGENTS
            .iter()
            .filter(|(_, content)| {
                // Extract the agent id from the YAML frontmatter (id: xxx)
                if let Ok(def) = rigger::config::parse_agent(content.as_bytes()) {
                    referenced_agents.contains(&def.id)
                } else {
                    // If we can't parse it, skip it to avoid scaffolding invalid agents
                    false
                }
            })
            .copied()
            .collect()
    };

    let mut new_agents = Vec::new();
    for (file, content) in &agents_to_scaffold {
        // Report only NEWLY-written agents; an existing agent is kept silently, so a
        // rerun scaffolds nothing new (the skip-scaffolding hygiene of §05). A genuine
        // write failure escalates (naming the artifact), never a silent omission.
        if write_if_absent(&agents_dir.join(file), content)? {
            new_agents.push(file.to_string());
        }
    }

    // 3. Install the SessionStart hook, merging into any existing settings. Write ONLY
    // when the merge actually changes settings.json, so a rerun (the hook already
    // present) leaves the file - and its mtime - untouched.
    let claude_dir = root.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");
    let existing = std::fs::read(&settings_path).unwrap_or_default();
    let merged = hooks::install_session_start(&existing, hooks::PRIME_COMMAND)?;
    let wrote_hook = merged != existing;
    if wrote_hook {
        std::fs::write(&settings_path, &merged)?;
    }

    // 3b. Install the fan-out helpers the built-in working discipline names, at the
    // `.claude/agents/` path it names them by. An existing helper is kept (the operator may
    // have tuned it), so a rerun writes nothing.
    let helpers_dir = claude_dir.join("agents");
    std::fs::create_dir_all(&helpers_dir)?;
    let mut new_helpers = Vec::new();
    for (file, content) in hooks::HELPER_AGENTS {
        if write_if_absent(&helpers_dir.join(file), content)? {
            new_helpers.push(file.to_string());
        }
    }

    // 4. Write .gitignore entries for machine-local installs, the always-on dash's runtime
    // breadcrumbs, and the per-machine store-connection secret file, when they are not already
    // ignored or tracked. `.claude/` and `.rigger/shim/` are the machine-local installs;
    // `.rigger/dash.url` and `.rigger/dash.marker` are the dash's discoverability breadcrumbs
    // (spec 39) - left untracked-and-not-ignored they get swept into a unit worktree's commit by
    // `git add` and then collide with the live dash's rewrites when the conductor merges the unit
    // ("untracked working tree files would be overwritten"). `.rigger/dash.attempt` (spec 69,
    // round-8 fix) is a THIRD runtime breadcrumb of the identical shape - `ensure_run_dashboard`
    // / `start_run_dashboard` rewrite it on every dash-ensure call exactly as they rewrite the
    // other two - so it collides the same way if left untracked-and-not-ignored, and is ignored
    // here for the same reason. `.rigger/store.conn` is the store resolver's per-machine secret
    // file (spec 48 rung 3): it carries the connection string's credentials, so it is git-ignored
    // BY CONSTRUCTION - a developer's credentials can never ride a committed file. Record WHICH
    // patterns were appended so the summary reports the real gitignore change and nothing it did
    // not do.
    let mut gitignore_added = Vec::new();
    for pattern in [
        ".claude/",
        ".rigger/shim/",
        ".rigger/dash.url",
        ".rigger/dash.marker",
        ".rigger/dash.attempt",
        ".rigger/store.conn",
    ] {
        if write_gitignore_entries(root, pattern)? {
            gitignore_added.push(pattern.to_string());
        }
    }

    Ok(ScaffoldReport {
        wrote_workflow,
        new_agents,
        new_helpers,
        wrote_hook,
        gitignore_added,
        minted_id,
        wrote_instructions_readme,
        new_gate_files,
    })
}

/// Print the empty-repo scaffold pointer: where to get a real starting agent fleet
/// (the agency-agents collection) and how to author agents (the handbook chapter).
/// `rigger init` / `rigger setup` call this ONLY when the default fleet was actually
/// scaffolded this run - per the weave of units 4 and 8, the signal is a non-empty
/// [`ScaffoldReport::new_agents`] (spec 05 done-when line 57, clause 2) - never on a
/// re-run that keeps an existing fleet.
fn print_scaffold_pointer() {
    println!(
        "next: this scaffolded a minimal starter fleet. For a fuller set, clone the \
         agency-agents collection from https://github.com/msitarzewski/agency-agents and \
         import it with `rigger setup --agents <dir>`, or author your own following the \
         handbook chapter at docs/handbook/authoring-agents.md"
    );
}

/// Print the end-of-setup orientation block: the three ways to drive a run, so an operator
/// who just provisioned the project discovers them without grepping the docs (spec 19a unit
/// 2). Names the blessed native `/rigger <spec>` path (chosen from the `/workflows` menu),
/// the read-only dashboard (`rigger dash`, on `127.0.0.1:<dash::DEFAULT_PORT>` - the port is
/// single-sourced from the constant so this line and the fixture that asserts it cannot
/// drift), and `rigger workflow` / `rigger run` as the headless twins that drive the same
/// loop without an editor. Output text only; the dashboard's runtime behavior is spec 19b's.
fn print_orientation() {
    println!("to drive a run, three ways:");
    println!(
        "  /rigger <spec>                 the blessed native path (choose it from /workflows)"
    );
    println!(
        "  rigger dash                    a read-only live dashboard at http://127.0.0.1:{}",
        dash::DEFAULT_PORT
    );
    println!(
        "  rigger workflow / rigger run   the headless twins (the same loop without an editor)"
    );
}

/// Write a .gitignore entry for the given pattern if it is not already an explicit line
/// or a tracked path, returning whether it APPENDED an entry (`true`) or left `.gitignore`
/// untouched (`false`). Idempotent: a rerun finds the exact line already present and is a
/// no-op, so setup never pollutes `.gitignore` with duplicates.
///
/// Deliberately does NOT consult `git check-ignore` to skip a path a BROADER rule already
/// covers. `git check-ignore` resolves ignores against machine-local global sources
/// (`core.excludesFile`, `~/.config/git/ignore`, `.git/info/exclude`), so letting it decide
/// what to append would make the COMMITTED `.gitignore` contingent on the setup-runner's
/// machine: an operator whose global excludes already list `.rigger/` would ship a
/// `.gitignore` MISSING the `.rigger/dash.url` / `.rigger/dash.marker` lines, and a teammate
/// or CI cloning with a clean HOME would then let `git add` sweep the dash breadcrumbs into a
/// unit commit - the exact collision spec 46 criterion 1 exists to prevent. The committed
/// file must be self-contained and portable, so we append the explicit line whenever it is
/// absent. A redundant-but-correct per-file line in a repo whose OWN rules already ignore a
/// broader path (e.g. `.rigger/`) is harmless; the exact-line check above still guarantees
/// idempotency, and the file stays machine-independent.
fn write_gitignore_entries(root: &Path, pattern: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let gitignore_path = root.join(".gitignore");
    let normalized_pattern = pattern.trim_end_matches('/');

    // Already an explicit line in .gitignore: a no-op. This exact-line check is the
    // idempotency guarantee, and it reads ONLY the repo's own committed `.gitignore` (never
    // machine-local global git config), so it holds even OUTSIDE a git repo and never makes
    // the committed file depend on the runner's machine.
    let current = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    if current
        .lines()
        .any(|line| line.trim() == normalized_pattern)
    {
        return Ok(false); // Already in .gitignore
    }

    // Check if the path is tracked in git (it should not be, as .claude/ and .rigger/shim/
    // are machine-local and should never be committed). This is just a safety check.
    let is_tracked = subprocess::command("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.starts_with(&format!("{}/", normalized_pattern)))
        })
        .unwrap_or(false);

    if is_tracked {
        return Ok(false); // Path is tracked, don't ignore it
    }

    // Append to .gitignore
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)?;

    // Add a newline before the entry if the file is not empty and doesn't end with newline
    if !current.is_empty() && !current.ends_with('\n') {
        writeln!(file)?;
    }

    writeln!(file, "{}", normalized_pattern)?;

    Ok(true)
}

/// Get all agent IDs referenced in the workflow at <root>/.rigger/workflow.yml. Returns an
/// empty set ONLY when the file is genuinely ABSENT (the legitimate empty-repo signal
/// `init_project` seeds the full default fleet on); a PRESENT-but-unparseable file - most
/// importantly one carrying an unrecognized key - is a real `Err`, never folded into that
/// same empty set. Routed through [`config_store::load_workflow`], the one canonical
/// `deny_unknown_fields`-honoring parser, rather than a second ad hoc
/// `serde_yaml::from_str::<Workflow>` that would silently accept what that parser rejects.
fn get_referenced_agent_ids(
    root: &Path,
) -> Result<std::collections::HashSet<String>, Box<dyn std::error::Error>> {
    use std::collections::HashSet;

    let workflow_path = root.join(RIGGER_DIR).join("workflow.yml");
    if !workflow_path.exists() {
        return Ok(HashSet::new());
    }

    let workflow = config_store::load_workflow(&workflow_path)?;

    let mut ids = HashSet::new();

    // Add agents from defaults.review
    for agent_id in workflow.defaults.review.agent_ids() {
        ids.insert(agent_id);
    }

    // Add agents from all stages
    for stage in workflow.stages.values() {
        for agent_id in stage.agent_ids() {
            ids.insert(agent_id);
        }
    }

    Ok(ids)
}

/// The per-artifact summary lines for a scaffold run: ONE line for each artifact this
/// run actually (re)wrote, and nothing for artifacts left untouched. This is the single
/// authority for the setup/init summary, so it can never emit a blanket "scaffolded
/// workflow + agents + hook" claim on a run that only repaired one artifact - a
/// gitignore-only repair yields only the gitignore line (spec 05, criterion 4: prints
/// nothing surprising; the honest-summary bar of adj-unit5). Pure so it is unit-testable
/// without capturing stdout.
fn scaffold_summary_lines(report: &ScaffoldReport) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(id) = &report.minted_id {
        lines.push(format!(
            "minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"
        ));
    }
    if report.wrote_workflow {
        lines.push("scaffolded .rigger/workflow.yml".to_string());
    }
    if report.wrote_instructions_readme {
        lines.push("scaffolded .rigger/instructions/README.md".to_string());
    }
    for file in &report.new_gate_files {
        lines.push(format!("scaffolded .rigger/gates/{file}"));
    }
    if !report.new_agents.is_empty() {
        lines.push(format!(
            "scaffolded .rigger/agents/{{{}}}",
            report.new_agents.join(", ")
        ));
    }
    if !report.new_helpers.is_empty() {
        lines.push(format!(
            "scaffolded the fan-out helpers .claude/agents/{{{}}}",
            report.new_helpers.join(", ")
        ));
    }
    if report.wrote_hook {
        lines.push(
            "installed a Claude Code SessionStart hook in .claude/settings.json (it runs \
             `rigger prime`)"
                .to_string(),
        );
    }
    if !report.gitignore_added.is_empty() {
        lines.push(format!(
            "added .gitignore entries so machine-local installs stay untracked: {}",
            report.gitignore_added.join(", ")
        ));
    }
    lines
}

pub(crate) fn cmd_init() -> Res {
    let report = init_project(Path::new("."))?;
    let lines = scaffold_summary_lines(&report);
    if lines.is_empty() {
        // Re-runnable: an already-initialized project is a silent no-op with a plain
        // confirmation, never a re-narration of every file left in place.
        println!("rigger init: already initialized; nothing to scaffold");
    } else {
        for line in lines {
            println!("{line}");
        }
    }
    // The starter-fleet pointer fires exactly when default agents were NEWLY
    // scaffolded (the empty-repo path): units 4 + 8 woven - the per-artifact report's
    // `new_agents` IS the scaffolded-new signal.
    if !report.new_agents.is_empty() {
        print_scaffold_pointer();
    }
    Ok(())
}

/// The git hooks directory for `root`, resolved robustly via `git rev-parse --git-path
/// hooks` (which honors `core.hooksPath` and a worktree's `.git`-file indirection) and
/// falling back to `<root>/.git/hooks` when git cannot be consulted. A relative path git
/// prints is resolved against `root` so the caller gets an absolute-enough path to write to.
fn git_hooks_dir(root: &Path) -> std::path::PathBuf {
    let resolved = subprocess::git_in(root)
        .args(["rev-parse", "--git-path", "hooks"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    match resolved {
        Some(p) => {
            let p = Path::new(&p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                root.join(p)
            }
        }
        None => root.join(".git").join("hooks"),
    }
}

/// Install (or refresh) rigger's docs-checking `pre-commit` hook under `root`,
/// returning [what it did](InstallOutcome). The FS-facing wrapper around the pure
/// [`compose_precommit_bytes`]: it reads the current `pre-commit` (if any) AS BYTES, composes
/// the merged hook, and writes it ONLY when the merge changes something - so a `rigger setup`
/// rerun on an already-installed hook is a true no-op that does not even move the file's mtime
/// (the no-op-when-unchanged discipline of [`install_file_if_changed`], applied to a composer
/// that CHAINS rather than overwrites). The written hook is marked executable so git will
/// run it. Non-destructive by construction: an existing pre-commit hook is preserved - rigger's
/// block is chained in (inserted after the shebang, before the existing body), never clobbered -
/// and reading BYTES rather than a UTF-8 string keeps that guarantee even for a non-UTF-8 hook.
fn install_precommit_hook(root: &Path) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let hooks_dir = git_hooks_dir(root);
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-commit");
    let existed = hook_path.exists();
    // Read the current hook as BYTES so a non-UTF-8 or otherwise unreadable existing hook is
    // preserved and chained rather than clobbered by a fresh script (the compose is byte-level;
    // d24-2-nonutf8-byte-compose-no-clobber).
    let existing = std::fs::read(&hook_path).ok();
    let merged = compose_precommit_bytes(existing.as_deref());
    if existing.as_deref() == Some(merged.as_slice()) {
        return Ok(InstallOutcome::AlreadyCurrent);
    }
    std::fs::write(&hook_path, &merged)?;
    // Mark the hook executable so git runs it. A hook without the execute bit is silently
    // ignored, which would defeat the whole feature.
    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms)?;
    }
    Ok(if existed {
        InstallOutcome::Refreshed
    } else {
        InstallOutcome::Installed
    })
}

/// Provision the per-project JS driver under `<root>/.rigger/shim/`: write the three
/// embedded runtime files (`shim.mjs`, `package.json`, `package-lock.json`) and
/// install their npm dependencies so `node_modules` is ready and `rigger workflow`
/// is zero-setup. Rooted at `root` so it is testable against a temp dir.
///
/// Provisioning is a silent no-op when the shim is already up to date: the three
/// runtime files match the embedded copies AND `node_modules` is present (see
/// [`shim_is_current`]). Skipping then avoids re-touching the files' mtimes and
/// re-running npm on every `rigger setup` (spec 05, criterion 4: setup is re-runnable
/// and changes nothing when nothing drifted). Otherwise the files are (re)written from
/// the embedded copies (so a `rigger` upgrade refreshes the driver to match the binary)
/// and npm install runs: `npm ci` when the lockfile is present (a reproducible, locked
/// install), else `npm install`. A missing `npm` is a CLEAR error (naming the directory
/// it would have installed in), never a silent skip - the user must know the driver is
/// not ready. Returns whether it actually (re)provisioned.
fn provision_shim(root: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let dir = rigger_path(root, SHIM_DIR);
    if shim_is_current(&dir) {
        return Ok(false);
    }
    write_shim_files(root)?;
    run_npm_install(&dir)?;
    Ok(true)
}

/// Whether the provisioned shim in `dir` is up to date: every embedded runtime file is
/// present with byte-identical content AND npm's install is COMPLETE. Used by
/// [`provision_shim`] to make a `rigger setup` rerun a no-op instead of re-writing the
/// files and re-running npm.
///
/// Completeness is gated on `node_modules/.package-lock.json` - the hidden lockfile npm
/// writes as the FINAL step of a successful `npm ci` / `npm install` - not on the mere
/// PRESENCE of a `node_modules` directory. A torn/partial install (an interrupted `npm
/// ci`, which `rm -rf`s `node_modules` then repopulates incrementally) leaves the
/// directory present-but-incomplete and WITHOUT the marker; gating on the marker makes
/// setup re-run npm and SELF-HEAL it rather than treating the broken tree as current and
/// refusing to repair it forever (spec 05, criterion 4: setup is re-runnable).
fn shim_is_current(dir: &Path) -> bool {
    dir.join("node_modules")
        .join(".package-lock.json")
        .is_file()
        && SHIM_FILES.iter().all(|(name, contents)| {
            std::fs::read(dir.join(name))
                .map(|on_disk| on_disk == contents.as_bytes())
                .unwrap_or(false)
        })
}

/// Install the shim's npm dependencies in `dir`. Uses `npm ci` when a
/// `package-lock.json` is present (a clean, lockfile-exact install) and `npm
/// install` otherwise. `npm` not being on PATH is a clear, actionable error naming
/// the directory - the JS driver is unusable without its deps, so this never
/// silently succeeds.
fn run_npm_install(dir: &Path) -> Res {
    let npm = std::env::var("RIGGER_NPM").unwrap_or_else(|_| "npm".to_string());
    let subcmd = if dir.join("package-lock.json").exists() {
        "ci"
    } else {
        "install"
    };
    let status = subprocess::command(&npm)
        .arg(subcmd)
        .current_dir(dir)
        .status()
        .map_err(|e| {
            format!(
                "setup: could not run `{npm} {subcmd}` in {}: {e}. \
                 Is Node's npm installed and on your PATH? The JS driver needs its \
                 dependencies before `rigger workflow` can run.",
                dir.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "setup: `{npm} {subcmd}` failed in {} ({status}); the JS driver's \
             dependencies were not installed",
            dir.display()
        )
        .into());
    }
    Ok(())
}

/// `rigger setup` is the FULL project setup: it does everything `rigger init` does
/// (scaffold `.rigger/` + install the Claude Code hook), installs the native
/// `/rigger` Claude Code workflow at `.claude/workflows/rigger.js`, AND provisions
/// the JS driver (writes the embedded shim runtime into `.rigger/shim/` and runs `npm
/// install`). After it runs the user can drive the loop with the native workflow
/// (`/rigger <spec>`) with zero manual setup; the standalone `rigger workflow` shim
/// remains as a fallback.
pub(crate) fn cmd_setup(args: &[String]) -> Res {
    let opts = parse_setup_args(args)?;
    let root = Path::new(".");
    // Each step is drift-aware and reports whether it changed anything, so setup is
    // safely re-runnable: it refreshes a drifted workflow and reports it, and a rerun
    // on an up-to-date repo changes nothing and prints nothing surprising (spec 05,
    // criterion 4).
    let scaffold = init_project(root)?;
    // Pay the one cold rebuild a `graph.db` owes (spec 101) - folded under an older fold rule, or
    // missing an event the log holds: the only command that rebuilds it, because it is the verb
    // every install already runs.
    let graph_rebuilt = rebuild_owed_graph()?;
    let workflow = install_workflow(root)?;
    // Install EVERY skill in the registry (spec 20, unit 3; spec 68, criterion 1): each a
    // loadable front-door DISTINCT from the `/rigger` workflow, with this repo's project
    // overlay (base branch, specs location) merged into the render. Drift-aware like the
    // workflow, so a rerun on an up-to-date repo changes nothing.
    let skills = install_skills(root)?;
    // Install the docs-regenerating git pre-commit hook (spec 24): on `git commit` it runs
    // `rigger docs` and stages any changed rendered outputs into the SAME commit, so a commit
    // that changes a documented code fact carries its freshly rendered docs. Drift-aware and
    // non-destructive like the installs above - a rerun on an already-installed hook changes
    // nothing, and any pre-existing pre-commit hook is chained, never clobbered.
    let hook = install_precommit_hook(root)?;
    let provisioned = provision_shim(root)?;
    // Register the operator's own MCP lookup surface (spec 92, criterion 4: IN EVERY
    // SESSION'S HAND) - `.mcp.json` gains a `rigger` entry (`rigger mcp`) exposing
    // rigger_peers/rigger_ground/rigger_graph to THIS session - and the PreToolUse hook
    // that bounces a bare source grep toward those tools. Both drift-aware like every
    // install above.
    let mcp_registered = install_operator_mcp(root)?;
    let lookup_hook = install_lookup_hook(root)?;
    // Register `rigger status --line` as the editor's status line command (spec 94, criterion
    // 5: THE STATUSLINE COMMAND) - so the line under the person's conversation and the
    // console's own bottom line are one text. Drift-aware like every install above.
    let status_line = install_status_line(root)?;

    // The --agents import (units 4 + 8 woven) is itself a REQUESTED change: it runs
    // before the silent-no-op check and always reports its outcome, so an import onto
    // an otherwise up-to-date repo is never silently skipped.
    let imported = if let Some(src) = &opts.agents_dir {
        let summary = import_agents(root, src)?;
        println!(
            "imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)",
            summary.imported,
            if summary.imported == 1 {
                "file"
            } else {
                "files"
            },
            src.display(),
            summary.skipped,
        );
        true
    } else {
        false
    };

    let workflow_changed = workflow != InstallOutcome::AlreadyCurrent;
    let skill_changed = skills
        .iter()
        .any(|(_, outcome)| *outcome != InstallOutcome::AlreadyCurrent);
    let hook_changed = hook != InstallOutcome::AlreadyCurrent;
    let mcp_changed = mcp_registered != InstallOutcome::AlreadyCurrent;
    let lookup_hook_changed = lookup_hook != InstallOutcome::AlreadyCurrent;
    let status_line_changed = status_line != InstallOutcome::AlreadyCurrent;
    if !scaffold.changed()
        && !graph_rebuilt
        && !workflow_changed
        && !skill_changed
        && !hook_changed
        && !provisioned
        && !imported
        && !mcp_changed
        && !lookup_hook_changed
        && !status_line_changed
    {
        // A silent no-op: nothing drifted, so there is nothing to report.
        return Ok(());
    }

    // Surface the running binary's version + build provenance (spec 18) whenever setup
    // actually reports a change, so an agent can see which binary just (re)provisioned the
    // project. Printed AFTER the silent-no-op early return above, so a rerun that changed
    // nothing stays silent.
    println!("{}", version_line());

    // Narrate ONLY the scaffold artifacts this run actually (re)wrote - never a blanket
    // claim, so a gitignore-only repair reports the gitignore change alone.
    for line in scaffold_summary_lines(&scaffold) {
        println!("{line}");
    }
    if provisioned {
        println!(
            "provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"
        );
    }
    match workflow {
        InstallOutcome::Installed => println!(
            "installed the /rigger workflow (.claude/workflows/rigger.js) - run it with: /rigger \
             <spec-path>"
        ),
        InstallOutcome::Refreshed => println!(
            "refreshed the drifted /rigger workflow (.claude/workflows/rigger.js) to match this \
             rigger build"
        ),
        InstallOutcome::AlreadyCurrent => {}
    }
    for (name, outcome) in &skills {
        match outcome {
            InstallOutcome::Installed => {
                println!("installed the {name} skill (.claude/skills/{name}/SKILL.md)")
            }
            InstallOutcome::Refreshed => println!(
                "refreshed the drifted {name} skill (.claude/skills/{name}/SKILL.md) to match \
                 this rigger build"
            ),
            InstallOutcome::AlreadyCurrent => {}
        }
    }
    match hook {
        InstallOutcome::Installed => println!(
            "installed the docs pre-commit hook - each commit now regenerates the using-rigger \
             docs and stages any change into that same commit"
        ),
        InstallOutcome::Refreshed => {
            println!("refreshed the docs pre-commit hook to match this rigger build")
        }
        InstallOutcome::AlreadyCurrent => {}
    }
    match mcp_registered {
        InstallOutcome::Installed => println!(
            "registered the rigger MCP server (.mcp.json: rigger mcp) - this session now has \
             rigger_peers/rigger_ground/rigger_graph tools, the same lookups a loop agent gets"
        ),
        InstallOutcome::Refreshed => {
            println!("refreshed the drifted rigger MCP server entry (.mcp.json) to match this rigger build")
        }
        InstallOutcome::AlreadyCurrent => {}
    }
    match lookup_hook {
        InstallOutcome::Installed => println!(
            "installed the graph-first lookup hook - a Grep tool call or `grep` command over \
             src/, tests/, or workflows/ now bounces toward rigger_ground/rigger_graph (end \
             a `grep` command with a `# --literal` comment to proceed anyway)"
        ),
        InstallOutcome::Refreshed => println!(
            "installed the graph-first lookup hook into the existing settings.json - a Grep \
             tool call or `grep` command over src/, tests/, or workflows/ now bounces toward \
             rigger_ground/rigger_graph (end a `grep` command with a `# --literal` comment to \
             proceed anyway)"
        ),
        InstallOutcome::AlreadyCurrent => {}
    }
    match status_line {
        InstallOutcome::Installed => println!(
            "registered the rigger status line command (.claude/settings.json: statusLine -> \
             {}) - the editor's status bar now shows the same line `rigger \
             status` prints first",
            hooks::STATUS_LINE_COMMAND
        ),
        InstallOutcome::Refreshed => println!(
            "refreshed the drifted rigger status line command (.claude/settings.json) to match \
             this rigger build"
        ),
        InstallOutcome::AlreadyCurrent => {}
    }
    // The starter-fleet pointer fires exactly when default agents were NEWLY
    // scaffolded (spec 05 line 57 clause 2): the per-artifact report's `new_agents`
    // is the scaffolded-new signal.
    if !scaffold.new_agents.is_empty() {
        print_scaffold_pointer();
    }
    // The orientation block closes the reported-change path: because it lives after the
    // silent-no-op early return above, a fully up-to-date rerun that changed nothing stays
    // quiet and never re-prints it (spec 05 crit 4: a rerun prints nothing surprising).
    print_orientation();
    Ok(())
}

/// How many events a graph rebuild folds per committed batch: an interrupted rebuild resumes from
/// its last committed batch, and a batch is what it holds in memory at once.
const REBUILD_BATCH: usize = 10_000;

/// Rebuild this project's `graph.db` from the event log when it owes that rebuild (spec 101) -
/// it was folded under an older fold rule, or it misses an event the log holds, which setup finds
/// on every run by reading the file's ledger of folded positions against the log's live selection
/// whether or not a mark says so - saying so, naming the cause, reporting how far along it is, and
/// report whether it did. A rebuild's own unfinished work - the shadow a stopped swap left
/// standing, or the tail past the cursor a swap put in place - is no owed cause: it is finished
/// with no `rebuilding graph.db ...: <cause>` line, still reporting how far along it is, and so
/// rewrites a `graph.db` whose ledger owes nothing. Only a `graph.db` that owes nothing and holds
/// no unfinished rebuild is left untouched; with no `graph.db` there is nothing to rebuild, and the
/// owed mark a removed one left behind is dropped, so the file a later command makes in its place
/// starts owing nothing it recorded. The rebuild folds the log's live selection - the rows `rigger
/// reset --derived` keeps - into a shadow file whose pruned copy replaces `graph.db` in one step
/// ([`Projector::rebuild`]), streaming the log once, resuming an interrupted rebuild from its last
/// committed batch and finishing exactly the tail of one interrupted after its swap.
///
/// Before it reads or writes anything else it takes the rebuild lock on `graph.db.lock`
/// ([`Projector::lock_rebuild`]), making that zero-byte file beside `graph.db` if it is not there,
/// and holds it until the rebuild is paid or found not owed: while another rebuild holds it this
/// setup is refused at once with the one refusal text naming the rebuild in progress, whatever
/// phase that rebuild is in, having opened no graph file and made none.
fn rebuild_owed_graph() -> Result<bool, Box<dyn std::error::Error>> {
    let graph_db = db_path("graph.db");
    if !Path::new(&graph_db).exists() {
        Projector::forget_orphaned_mark(&graph_db)?;
        return Ok(false);
    }
    let held = Projector::lock_rebuild(&graph_db)?;
    // The scaffold may just have minted the durable identity: the log moves to it first (the
    // migration every run driver performs on open), so the rebuild reads the history the legacy
    // namespace still holds.
    migrate_local_identity()?;
    let project = project_identity();
    let graph_error = |e: rigger::eventstore::Error| contextgraph::Error(e.to_string());
    match store_selection(None, None)? {
        StoreSelection::Sqlite => {
            let store = open_sqlite_store(&db_path("events.db"))?;
            let prefix = Namespaced::prefix_for(&project);
            let identity = rigger::ingest::derived_index_identity();
            pay_owed_rebuild(
                &held,
                &project,
                &mut |sink| {
                    store
                        .read_live_positions(
                            &prefix,
                            conductor::STREAM,
                            &identity,
                            REBUILD_BATCH,
                            &mut |positions| {
                                sink(positions).map_err(|e| rigger::eventstore::Error::Backend(e.0))
                            },
                        )
                        .map_err(graph_error)
                },
                &mut |after, sink| {
                    store
                        .read_live_selection(
                            &prefix,
                            conductor::STREAM,
                            &identity,
                            after,
                            REBUILD_BATCH,
                            &mut |events, head| {
                                sink(events, head)
                                    .map_err(|e| rigger::eventstore::Error::Backend(e.0))
                            },
                        )
                        .map_err(graph_error)
                },
            )
        }
        // A server-backed log has no compaction plan (`rigger reset --derived` is sqlite-only), so
        // its live selection is its run stream as it stands: its positions are read through the
        // store port alone, and a rebuild reads each of its events once.
        selection => {
            let backend = resolve_store(&selection, &db_path("events.db"))?;
            let store = Namespaced::new(backend.as_ref(), &project);
            let mut positions =
                contextgraph::sqlite::stream_positions(&store, conductor::STREAM, REBUILD_BATCH);
            let mut source =
                contextgraph::sqlite::stream_source(&store, conductor::STREAM, REBUILD_BATCH);
            pay_owed_rebuild(&held, &project, &mut positions, &mut source)
        }
    }
}

/// Read why the `graph.db` whose rebuild lock is `held` owes its rebuild - its own records, and
/// its ledger against the positions `live` streams ([`Projector::owed_against`]) - say so naming
/// each cause, and pay it by rebuilding from `source`, which also finishes a rebuild's own
/// unfinished work (a standing shadow, or a swapped-in cursor's tail) with no cause to name, since
/// the ledger owes none of it; print how far along the rebuild is, what its run-closure prune
/// removed from the rebuilt graph ([`pruned_line`], as `rigger reset --runs` words its own) and how
/// many events it passed over because the fold rejects their payload; report whether it rebuilt.
fn pay_owed_rebuild(
    held: &contextgraph::sqlite::RebuildLock,
    project: &str,
    live: &mut contextgraph::sqlite::PositionSource,
    source: &mut contextgraph::sqlite::RebuildSource,
) -> Result<bool, Box<dyn std::error::Error>> {
    let causes = Projector::open(held.path(), project)?.owed_against(live)?;
    if !causes.is_empty() {
        println!(
            "rebuilding graph.db from the event log: {}, so the log's live selection is refolded \
             once",
            causes.join(", and ")
        );
    }
    let mut printed = 0;
    let rebuilt = Projector::rebuild(held, project, !causes.is_empty(), source, &mut |at| {
        if let Some(line) = rebuild_progress_line(at, &mut printed) {
            println!("{line}");
        }
    })?;
    if let Some(rebuilt) = rebuilt {
        println!("rebuilt graph.db from the event log");
        println!("{} from the rebuilt graph", pruned_line(&rebuilt.pruned));
        if rebuilt.passed_over > 0 {
            println!(
                "passed over {} event(s) whose payload the fold rejects, recorded as folded",
                rebuilt.passed_over
            );
        }
    }
    Ok(rebuilt.is_some())
}

/// The progress line a graph rebuild prints at `at`, if any: one each time a batch carries it into
/// a tenth of the log beyond `printed` (the last tenth it printed, which this advances), so a long
/// rebuild reports how far along it is in at most ten lines however long the log is. How far is
/// measured in log positions from where this rebuild started, because the rows it folds are a
/// selection of them; a log holding nothing past that start is wholly folded.
fn rebuild_progress_line(
    at: contextgraph::sqlite::RebuildProgress,
    printed: &mut u64,
) -> Option<String> {
    let percent = (at.through.saturating_sub(at.start) * 100)
        .checked_div(at.head.saturating_sub(at.start))
        .unwrap_or(100);
    let tenth = percent / 10;
    if tenth <= *printed {
        return None;
    }
    *printed = tenth;
    Some(format!(
        "rebuilt {} events, through position {} of {} ({percent}%)",
        at.folded, at.through, at.head
    ))
}

/// Install the graph-first lookup hook (spec 92, criterion 4): merges the PreToolUse
/// hook that runs `rigger grep-guard` into `.claude/settings.json`. Drift-aware and
/// non-destructive like every other `rigger setup` install (see
/// [`hooks::install_pretooluse_hook`]): idempotent, and any pre-existing PreToolUse hook
/// (for a different matcher, a different tool, something a person or another tool
/// installed) is preserved untouched.
///
/// The same three-state contract every other `rigger setup` install artifact has:
/// absent settings.json -> `Installed` (a fresh file, our block its first content),
/// an existing settings.json gaining the block (fresh OR foreign content already
/// there) -> `Refreshed` (the FILE existed even though our own array entry did not,
/// mirroring [`install_operator_mcp`]'s `existed` distinction), already carrying the
/// block -> `AlreadyCurrent` (a silent no-op).
fn install_lookup_hook(root: &Path) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    install_into_claude_settings(root, |existing| {
        hooks::install_pretooluse_hook(
            existing,
            hooks::GREP_GUARD_MATCHER,
            hooks::GREP_GUARD_COMMAND,
        )
    })
}

/// Merge an entry into the project's `.claude/settings.json` (creating `.claude/` first) -
/// see [`install_into`].
fn install_into_claude_settings(
    root: &Path,
    merge: impl FnOnce(&[u8]) -> Result<Vec<u8>, hooks::Error>,
) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    let claude_dir = root.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    install_into(&claude_dir.join("settings.json"), merge)
}

/// Idempotently merge an entry into the JSON file at `path`: `merge` maps its current bytes
/// (empty when absent) to the merged ones, and the file is rewritten only when they differ -
/// reporting whether the entry was already current, refreshed in place, or newly installed.
fn install_into(
    path: &Path,
    merge: impl FnOnce(&[u8]) -> Result<Vec<u8>, hooks::Error>,
) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    let existed = path.exists();
    let existing = std::fs::read(path).unwrap_or_default();
    let merged = merge(&existing)?;
    if merged == existing {
        return Ok(InstallOutcome::AlreadyCurrent);
    }
    std::fs::write(path, &merged)?;
    Ok(if existed {
        InstallOutcome::Refreshed
    } else {
        InstallOutcome::Installed
    })
}

/// Register [`hooks::STATUS_LINE_COMMAND`] as the editor's status line (spec 94, criterion 5): merges
/// `.claude/settings.json`'s `statusLine` key via [`hooks::install_status_line`]. Drift-aware
/// and non-destructive like every other `rigger setup` install (see
/// [`install_operator_mcp`]'s identical `existed`/byte-compare shape - `statusLine` has no
/// per-tool namespacing to preserve, so, like the `mcpServers` entry there, rigger owns this
/// key exclusively and self-heals a drifted one unconditionally).
///
/// The same three-state contract every other `rigger setup` install artifact has: absent
/// settings.json -> `Installed`, an existing settings.json gaining or self-healing the key ->
/// `Refreshed`, already carrying the exact command -> `AlreadyCurrent` (a silent no-op).
fn install_status_line(root: &Path) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    install_into_claude_settings(root, |existing| {
        hooks::install_status_line(existing, hooks::STATUS_LINE_COMMAND)
    })
}

/// Install the operator's own MCP lookup surface (spec 92, criterion 4): merges a
/// `rigger` entry (`rigger mcp`, see [`cmd_mcp`]) into `.mcp.json`'s `mcpServers`, so the
/// interactive session gets `rigger_peers`/`rigger_ground`/`rigger_graph` as tools -  the
/// same three lookups a loop agent has - instead of a shell. Drift-aware like every other
/// install (see [`hooks::install_mcp_server`]): a stale entry from an older build
/// self-heals, every OTHER server entry and top-level key in `.mcp.json` survives
/// untouched.
fn install_operator_mcp(root: &Path) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    install_into(&root.join(".mcp.json"), |existing| {
        hooks::install_mcp_server(existing, hooks::MCP_SERVER_NAME, "rigger", &["mcp"])
    })
}

/// Parsed `rigger setup` options. Setup takes no positional arguments; the only
/// flag is `--agents <dir>`, the local directory a starting agent fleet is imported
/// from (spec 05).
#[derive(Debug, Default)]
struct SetupOpts {
    agents_dir: Option<std::path::PathBuf>,
}

/// Parse `rigger setup`'s arguments: only `--agents <dir>` is recognized. An unknown
/// flag or a missing `--agents` value is a clear error rather than a silent skip.
fn parse_setup_args(args: &[String]) -> Result<SetupOpts, Box<dyn std::error::Error>> {
    let mut opts = SetupOpts::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--agents" => {
                let dir = it.next().ok_or(
                    "setup: --agents needs a directory argument (a local checkout of an \
                     agent collection)",
                )?;
                opts.agents_dir = Some(std::path::PathBuf::from(dir));
            }
            other => return Err(format!("setup: unknown argument {other:?}").into()),
        }
    }
    Ok(opts)
}

/// The outcome of an agent import: how many `.md` files were newly written into
/// `.rigger/agents/` and how many were kept untouched because a file of that name
/// already existed (import never overwrites).
#[derive(Debug, Default, PartialEq, Eq)]
struct ImportSummary {
    imported: usize,
    skipped: usize,
}

/// Import a starting agent fleet from a local collection directory into
/// `<root>/.rigger/agents/` (spec 05: offline - no network access in setup; the user
/// clones the collection themselves). For each `.md` file in `src`, the identity
/// frontmatter field is normalized to Rigger's `id:` and the file is copied under its
/// own name into `.rigger/agents/`. A file whose name already exists is KEPT untouched
/// (import never overwrites, so a re-run - or importing over the scaffolded fleet - is
/// safe) and counted as skipped. The result is validated by the SAME `config::load`
/// `rigger validate` runs, so a malformed agent fails the import loudly rather than
/// being written and breaking a later load. Rooted at `root` so it is testable against
/// a temp dir.
fn import_agents(root: &Path, src: &Path) -> Result<ImportSummary, Box<dyn std::error::Error>> {
    let dest = root.join(RIGGER_DIR).join("agents");
    std::fs::create_dir_all(&dest)?;

    // Collect the source `.md` files, SURFACING (never silently dropping) any directory
    // entry that fails to stat - a collection with an unreadable file must fail the
    // import loudly, not import a short count under a success message. Sorted so the log
    // and any first-error are stable across filesystems.
    let mut md_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("setup --agents: cannot read {}: {e}", src.display()))?
    {
        let entry = entry.map_err(|e| {
            format!(
                "setup --agents: reading an entry under {}: {e}",
                src.display()
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) == Some("md") {
            md_files.push(path);
        }
    }
    md_files.sort();

    // The prospective fleet: the agents already on disk plus the ones this import would
    // add. `rigger setup` scaffolds the default fleet before this runs, and a foreign
    // collection can carry an id that collides with a scaffolded agent (or with another
    // file in the same import) under a DIFFERENT filename - past the filename-only
    // overwrite guard, but a duplicate id `config::load` rejects. We therefore validate
    // the whole prospective fleet BEFORE writing anything (below), so a collision aborts
    // the import atomically instead of leaving half the files on disk to brick every
    // later load.
    let mut fleet: Vec<(String, config::AgentDef)> = config_store::read_agents_dir(&dest)
        .map_err(|e| format!("setup --agents: reading the existing fleet: {e}"))?;

    // Pass 1: normalize, parse, and STAGE each file to write - writing nothing yet.
    let mut summary = ImportSummary::default();
    let mut to_write: Vec<(String, String, String)> = Vec::new(); // (name, content, id)
    for path in md_files {
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| {
                format!(
                    "setup --agents: non-UTF-8 file name under {}",
                    src.display()
                )
            })?
            .to_string();
        if dest.join(&name).exists() {
            println!("kept existing .rigger/agents/{name} (import never overwrites)");
            summary.skipped += 1;
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("setup --agents: read {name}: {e}"))?;
        let normalized =
            normalize_identity(&raw).map_err(|e| format!("setup --agents: {name}: {e}"))?;
        // Parse structurally as we stage, so a malformed file's error names it (the same
        // parse the loader uses). The id invariant (non-blank, unique) is enforced once
        // for the whole fleet by `config::index_agents` below - the SAME rule the loader
        // applies, not a second copy of it.
        let parsed = config::parse_agent(normalized.as_bytes())
            .map_err(|e| format!("setup --agents: {name}: {e}"))?;
        let id = parsed.id.clone();
        fleet.push((name.clone(), parsed));
        to_write.push((name, normalized, id));
    }

    // Validate the prospective fleet by the SAME rule `config::load` enforces - a
    // non-blank, unique id per agent - before a single byte is written, so a blank or
    // colliding id fails the import loudly and leaves `.rigger/agents/` untouched.
    config::index_agents(fleet)?;

    // Pass 2: every staged file validated - commit the writes.
    for (name, content, id) in &to_write {
        std::fs::write(dest.join(name), content)
            .map_err(|e| format!("setup --agents: write {name}: {e}"))?;
        println!("imported .rigger/agents/{name} (id: {id})");
        summary.imported += 1;
    }

    // Full referential validation of the resulting project (workflow -> agent
    // references, the review panel, gates) via the same load `rigger validate` runs.
    let root_str = root
        .to_str()
        .ok_or("setup --agents: project root path is not valid UTF-8")?;
    config_store::load(root_str)?;

    Ok(summary)
}

/// Return `content` with the agent's identity frontmatter key normalized to Rigger's
/// `id:`. Collections such as agency-agents / Claude Code sub-agents name the identity
/// field `name:`, while Rigger's [`config::AgentDef`] requires `id:`. If the
/// frontmatter already declares a top-level `id:`, the content is returned unchanged;
/// otherwise the FIRST top-level `name:` key is renamed to `id:`, preserving its value,
/// every other frontmatter line, and the prompt body verbatim. A file with no YAML
/// frontmatter is an error (the same shape the loader rejects).
fn normalize_identity(content: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Parse the frontmatter through the SAME seam the loader uses
    // (`config::split_frontmatter`), not a second private copy of the delimiter logic:
    // `front` is the frontmatter text, `body` the prompt after the closing `---`. A file
    // with no (or unterminated) frontmatter fails here exactly as the loader's parse does.
    let (front, body) = config::split_frontmatter(content)?;

    // A top-level `id:` already present -> nothing to normalize.
    if front.lines().any(|l| top_level_key(l) == Some("id")) {
        return Ok(content.to_string());
    }

    // Rename the FIRST top-level `name:` key to `id:`, preserving its value and every other
    // frontmatter line; the prompt body is reattached verbatim.
    let mut renamed = false;
    let new_front = front
        .lines()
        .map(|line| {
            if !renamed && top_level_key(line) == Some("name") {
                renamed = true;
                let colon = line.find(':').expect("a top-level key implies a colon");
                format!("id{}", &line[colon..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("---\n{new_front}\n---\n{body}"))
}

/// The top-level YAML key a frontmatter line declares, or `None` when the line is
/// blank, indented (a nested value), a comment, or carries no `key:`. Frontmatter is
/// flat, so a non-indented `key:` line is a top-level field.
fn top_level_key(line: &str) -> Option<&str> {
    if line.is_empty() || line.starts_with([' ', '\t']) || line.starts_with('#') {
        return None;
    }
    let (key, _rest) = line.split_once(':')?;
    Some(key.trim_end())
}

/// The first line `rigger prime` prints: the instruction layers in force for this project,
/// counting its `operator` files, so a session starts knowing which law its agents are held to.
fn instructions_in_force_line(operator: usize) -> String {
    let builtin = instructions::BUILTIN
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    let extra = if operator == 0 {
        String::new()
    } else {
        format!(" (+{operator} operator files)")
    };
    format!(
        "{} in force: {builtin}{extra} - see rigger instructions",
        instructions::HEADING
    )
}

/// `rigger prime [<spec>]` - print recent decisions (what the SessionStart hook runs), and,
/// when a spec path is given, the DISCOVERABILITY next step naming the spec lint (spec 66,
/// criterion 5) so an operator or agent about to drive `/rigger <spec>` finds `rigger
/// validate <spec>` instead of stumbling onto it by accident. `args.first()` is the spec
/// path; the bare `rigger prime` the installed hook actually runs carries none, and stays
/// exactly as it was - no lint mention.
pub(crate) fn cmd_prime(args: &[String]) -> Res {
    let spec_path = args.first();
    println!(
        "{}",
        instructions_in_force_line(config_store::load_instructions(Path::new("."))?.len())
    );
    // The one store-location authority every courier resolves through: a session started in a
    // subdirectory or a unit worktree reads the project's own store and identity, never the cwd's.
    let (loc, selection) = match require_store_dir() {
        Ok(found) => found,
        Err(e) if e.downcast_ref::<NoStoreFound>().is_some() => {
            println!("# Rigger: no decisions recorded yet (run `rigger run` to start).");
            if let Some(spec) = spec_path {
                println!("{}", spec_lint_next_step(spec));
            }
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let backend = resolve_store(&selection, &store_file(&loc.dir, "events.db"))?;
    // Every run's decisions BY TYPE on this project's run stream (spec 101): a session start
    // reads the decisions it prints and nothing else - no derived event, no other project's.
    let decisions = Namespaced::new(backend.as_ref(), &loc.identity()).read_stream_typed(
        conductor::STREAM,
        0,
        rigger::eventstore::TypeSelection::Only(&[contextgraph::TYPE_DECISION_MADE]),
    )?;
    println!("# Rigger: recent decisions");
    let mut shown = 0;
    for e in decisions.iter().rev() {
        if let Ok(d) = serde_json::from_slice::<PeerDecision>(&e.data) {
            println!("- {}: {}", d.id, d.summary);
            shown += 1;
            if shown >= 10 {
                break;
            }
        }
    }
    if shown == 0 {
        println!("(none yet)");
    }
    if let Some(spec) = spec_path {
        println!("{}", spec_lint_next_step(spec));
    }
    Ok(())
}

/// Write `content` to `path` only when it does not already exist, returning `Ok(true)`
/// when it WROTE the file and `Ok(false)` when it KEPT an existing one. Keeping is silent
/// (a `rigger setup` / `rigger init` rerun must not narrate every file it left untouched),
/// so the boolean is how callers report only what a run actually created. A genuine write
/// FAILURE is an ERROR naming the artifact, not a swallowed `false`: setup/init must exit
/// nonzero rather than drop an artifact it could not create from the summary while still
/// exiting 0 (an honest-reporting hole on the error path). Only an already-present file is
/// a silent success; a real I/O failure escalates.
fn write_if_absent(path: &Path, content: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(false);
    }
    std::fs::write(path, content)
        .map_err(|e| format!("rigger: could not write {}: {e}", path.display()))?;
    Ok(true)
}

/// The README `rigger init` scaffolds into `.rigger/instructions/`, explaining the operator
/// instruction layer. The loader skips it, so documenting the layer never injects it.
const SCAFFOLD_INSTRUCTIONS_README: &str = "\
# Operator instructions

Every `*.md` file in this directory (except this README) is appended, in filename order,
to the system prompt of every agent rigger spawns: after the agent's persona and the
built-in engineering principles and working discipline, before rigger's communication
discipline. Name files with a numeric prefix (`10-house.md`, `20-team.md`) to control their
order.

Run `rigger instructions` to read exactly what your agents are held to.

These files are part of a run's definition pin: editing one mid-run halts the next step
as a definition drift. Edit them between runs, or continue with `--rebase-definition`.
";

/// The scaffolded workflow (§3.2): a worked plan -> implement pipeline where the
/// review is PER UNIT. It demonstrates the documented shape - a `defaults:` block
/// (autonomy + grounder + the three-tier `review` panel), a reusable `gates:`
/// library, and an implement stage that runs each unit's complete lifecycle
/// (implement -> gates -> three-tier review of THIS unit -> integrate). It loads
/// through `config::load` against the agents scaffolded alongside it.
const SCAFFOLD_WORKFLOW: &str =
    "# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
# review is PER UNIT: each unit implements, three-tier-reviews ITSELF (lenses ->\n\
# adversary -> adjudicator via defaults.review), and integrates in one lifecycle.\n\
# Replace the gate commands with your own.\n\
\n\
defaults:\n  \
autonomy: auto_notify   # manual | auto_notify | silent\n  \
grounder: symbols       # symbols (default; the structural symbol index) | grep | nop\n  \
# The spawn-budget circuit-breaker: the hard cap on agent spawns one unattended\n  \
# run may make. At the cap the breaker emits BudgetExhausted and aborts the run,\n  \
# so a runaway can never spawn unboundedly. NON-ZERO on purpose - 0 = unlimited.\n  \
budget: 60\n  \
# The remediation depth: how many attempts a failed unit gets before it escalates\n  \
# to a human. This is the REFINEMENT-depth knob, not a review-rigor one - raise it\n  \
# to give a subtle unit room to CONVERGE under the full strict review instead of\n  \
# escalating prematurely. It loosens the depth limit, never the review bar. Absent\n  \
# falls back to 3 (the historical default); bounded by `budget` above.\n  \
max_retries: 3\n  \
# How many implement units build at once (spec 102, per-unit pipelining). Each\n  \
# live unit owns its own build cache, and unbounded parallelism can fill disk on\n  \
# a wide fan-out. 0 (the default) is unbounded, unchanged until you set this; 2\n  \
# bounds the worst case to two live per-unit build caches at a time.\n  \
max_parallel_units: 2\n  \
# The three-tier review panel applied to EVERY implement unit. Declared once\n  \
# here, inherited by the implement stage and every planner-proposed unit.\n  \
review:\n    \
lenses: [architecture-reviewer, sdet]   # tier 1: the expert lenses\n    \
adversary: adversary           # tier 2: reviews the lenses and refutes them\n    \
adjudicator: adjudicator   # tier 3: neutral judge; its verdict gates the unit\n\
\n\
# The compilation-cache wrapper (spec 65): `auto` probes PATH for a known wrapper\n\
# (sccache, ccache) and uses it when present, so a machine that already has one\n\
# installed benefits with no further config; `off` disables the shared-cache\n\
# layer entirely. See `rigger validate` for the resolved wrapper/cache dir/budget.\n\
build:\n  \
wrapper: auto\n\
\n\
gates:                    # a reusable library of commands, referenced by name\n  \
build: { run: \"echo build ok; true\", kind: core }\n  \
test:  { run: \"echo test ok; true\",  kind: core }\n  \
lint:  { run: \"echo lint ok; true\",  kind: elevated }\n  \
# The check-in-stage mutation sweep (spec 91): runs ONCE, after every implement\n  \
# unit has integrated - never per implementer round. For a Rust project, run the\n  \
# sweep `rigger init` wrote beside this file: `run: \"sh .rigger/gates/mutation.sh\"`\n  \
# (diff-scoped, in its own memory-bounded scope; its header explains each clause).\n  \
# Declaring a gate under this exact id requires `cargo-mutants` on PATH (rigger\n  \
# validate checks at run start).\n  \
mutation: { run: \"echo mutation ok; true\", kind: core }\n\
# The boundary gate: Clean Architecture made mechanical. Replace with your\n  \
# project's own check that dependencies point inward and adapters are constructed\n  \
# only in the composition root (see this crate's tests/boundary_audit.rs for the\n  \
# worked example). A red boundary gate is non-negotiable: the adjudicator\n  \
# rejects, never balances it against other evidence.\n  \
boundary: { run: \"echo boundary ok; true\", kind: core }\n\
# The audit gate: DRY and YAGNI as a red gate. Replace with your project's own\n  \
# duplication and dead-code check; if it keeps a generated catalog, regenerate it\n  \
# before asserting so a unit is never red on a stale catalog alone.\n  \
audit: { run: \"echo audit ok; true\", kind: core }\n\
# The red-before-green gate: TDD made mechanical. Replace with a check that a\n  \
# unit's first source commit is preceded by (or carries) a test change (see this\n  \
# crate's .rigger/gates/red-before-green.sh for the worked example).\n  \
red-before-green: { run: \"echo red-before-green ok; true\", kind: core }\n\
\n\
stages:\n  \
# The conductor creates one baseline implement unit per acceptance criterion (the\n  \
# deterministic decomposition); this planner REFINES that baseline via UnitProposed.\n  \
# A produces stage decomposes the whole spec, so it has no single coverage criterion\n  \
# - it grounds on the spec's acceptance criteria, not a `coverage` label.\n  \
plan:\n    \
agent: planner\n    \
produces: dag           # refine the spec's unit DAG at runtime\n\
\n  \
# The adversarial plan-critique gate: BEFORE any implementer spawns, the adversary +\n  \
# adjudicator review the PROPOSED unit DAG for the cross-unit hazards per-unit review\n  \
# cannot see: ambiguous mitigation ownership and open dispositions (a shared blast\n  \
# radius is informational only - partition: by-blast-radius serializes it). A reject\n  \
# feeds back to the\n  \
# planner (bounded by max_retries); an approve releases the fan-out. Review-only (no\n  \
# agent) - it critiques the plan, it does not implement.\n  \
plan-critique:\n    \
needs: [plan]\n    \
adversary: adversary        # tier 2: reviews the DAG and refutes it\n    \
adjudicator: adjudicator    # tier 3: its approve/reject gates the fan-out\n\
\n  \
# Each unit implements, three-tier-reviews ITSELF (via defaults.review), and\n  \
# integrates in one lifecycle. A reject or a gate failure feeds back into that\n  \
# same unit's remediation loop; it does NOT integrate until approved + green.\n  \
implement:\n    \
needs: [plan-critique]\n    \
agent: rust-engineer\n    \
strategy: fan-out       # one worker per ready unit, in isolated worktrees\n    \
partition: by-blast-radius\n    \
gates: [build, audit, test, lint, boundary, red-before-green]  # red -> green enforced around the change\n    \
on_pass: merge          # land + reindex + record, per unit, once reviewed\n    \
coverage: \"each unit is implemented, reviews itself, and integrates green\"\n\
\n  \
# 3. Check in ONCE, after every implement unit has integrated (spec 91): a\n  \
# `needs` entry naming the fan-out `implement` TEMPLATE is satisfied exactly when\n  \
# every unit it expanded into has integrated - never per implementer round, and\n  \
# never before every unit has landed. Re-verifies the whole gate suite against\n  \
# the merged tree, then sweeps mutants; one remediation round (max_retries: 2),\n  \
# then integrate or escalate with the accounting already on record.\n  \
checkin:\n    \
needs: [implement]\n    \
agent: rust-engineer\n    \
max_retries: 2          # attempt bound: the sweep, one remediation round, the sweep again\n    \
gates: [build, audit, test, lint, boundary, mutation]\n    \
on_pass: merge\n    \
coverage: \"mutation efficacy of the whole spec diff\"\n";

/// The gate scripts `rigger init` writes into `.rigger/gates/`: this repository's own files,
/// included verbatim so the gates a consumer runs are the ones this repository runs (one home -
/// `tests/principle_gates_wiring.rs` pins each identical). The check-in mutation sweep sources
/// the container runtime snippet from beside itself, so the two ship together.
const SCAFFOLD_GATE_FILES: &[(&str, &str)] = &[
    (
        "mutation.sh",
        include_str!("../../.rigger/gates/mutation.sh"),
    ),
    (
        "container-env.sh",
        include_str!("../../.rigger/gates/container-env.sh"),
    ),
];

/// The agents the scaffolded workflow references - a fresh-repo SEED template, not a
/// frozen canonical fleet. Every entry is referenced by [`SCAFFOLD_WORKFLOW`] and every
/// referenced id is seeded here (the two stay in lockstep so a fresh `rigger init` seeds
/// no stray, unreferenced agent). The ids match this project's own canonical personas
/// (planner, rust-engineer, architecture-reviewer, sdet, adversary, adjudicator); the
/// four generic placeholder personas (implementer, devils-advocate, reviewer.architecture,
/// reviewer.technical) deliberately do NOT appear. Every seed, reviewers included, runs on
/// `opus` and may fan its mechanical work out (`Agent` in `tools`, `recurse: true`) to the
/// `lookup` and `verify` helpers [`hooks::HELPER_AGENTS`] installs; the discipline that
/// governs that fan-out is the built-in instruction layer, never copied into a seed. Each is a
/// markdown-with-frontmatter definition `config::load` parses; filenames are arbitrary, the
/// `id` is what the workflow binds to.
const SCAFFOLD_AGENTS: &[(&str, &str)] = &[
    (
        "planner.md",
        "---\n\
id: planner\n\
model: opus\n\
tools: [Read, Grep, Glob, Agent]\n\
isolation: none\n\
recurse: true\n\
---\n\
You decompose the spec into a DAG of small, independently-verifiable units, one\n\
per acceptance criterion. Emit each as a UnitProposed decision. Do not write code.\n",
    ),
    (
        "rust-engineer.md",
        "---\n\
id: rust-engineer\n\
model: opus\n\
tools: [Read, Edit, Write, Grep, Glob, Bash, Agent]\n\
isolation: worktree\n\
recurse: true\n\
---\n\
You implement ONE fully-specified unit inside your worktree, in idiomatic Rust.\n\
Write the failing test first, confirm RED, implement minimally, confirm GREEN, run\n\
the named gates, commit. Report the final line as JSON: {\"id\",\"pass\",\"evidence\"}.\n",
    ),
    (
        "architecture-reviewer.md",
        "---\n\
id: architecture-reviewer\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash, Agent]\n\
isolation: none\n\
recurse: true\n\
---\n\
You review a diff for architectural defects ONLY. Quote the rule or doc violated.\n\
Name the SOLID principle for each finding.\n\
Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.\n",
    ),
    (
        "sdet.md",
        "---\n\
id: sdet\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash, Agent]\n\
isolation: none\n\
recurse: true\n\
---\n\
You review a diff for correctness, error-handling, test coverage, and idiomatic\n\
defects ONLY. Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.\n\
Confirm the unit's first source commit follows a test commit (red before green).\n",
    ),
    (
        "adversary.md",
        "---\n\
id: adversary\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash, Agent]\n\
isolation: none\n\
recurse: true\n\
---\n\
You are the adversary (tier 2). You run AFTER the lenses and review THEIR findings\n\
AND the diff, trying to PROVE THE LENSES WRONG: hold them to a higher bar, surface\n\
the substantive issues they all missed, and refute lens overreach. You review the\n\
reviews - not a parallel lens - and you do NOT render the final verdict. Default to\n\
skepticism; cite file:line. Record findings with rigger_emit.\n",
    ),
    (
        "adjudicator.md",
        "---\n\
id: adjudicator\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash, Agent]\n\
isolation: none\n\
recurse: true\n\
---\n\
You are the adjudicator (tier 3), the neutral final judge. Weigh the expert lenses\n\
against the adversary and decide who wins. Be neutral in tone but EXTREMELY strict\n\
on design / architecture / ADR adherence: any deviation or cut corner is a reject,\n\
no matter which side flagged it. When you reject, say exactly what must change.\n\
A red `boundary` gate is non-negotiable: reject, never balance it against other evidence.\n\
End\n\
with a single JSON line {\"verdict\":\"approve\"} or {\"verdict\":\"reject\"} - reject\n\
blocks integration no matter what the static gates say.\n",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::git_init_quiet;
    use crate::test_support::tool_available;

    /// Spec 101: a graph rebuild reports its progress in tenths of the log positions it covers -
    /// a line each time a batch carries it into a new tenth, never twice for one tenth - with the
    /// events folded so far, the position reached and the percent, measured from where it started.
    #[test]
    fn a_rebuild_reports_each_tenth_of_the_log_it_reaches_once() {
        use contextgraph::sqlite::RebuildProgress;
        let lines = |start, batches: &[(u64, usize)], head| -> Vec<String> {
            let mut printed = 0;
            batches
                .iter()
                .filter_map(|&(through, folded)| {
                    let at = RebuildProgress {
                        start,
                        through,
                        head,
                        folded,
                    };
                    rebuild_progress_line(at, &mut printed)
                })
                .collect()
        };
        assert_eq!(
            lines(0, &[(1, 1), (5, 3), (6, 4), (7, 5), (11, 8), (20, 15)], 20),
            vec![
                "rebuilt 3 events, through position 5 of 20 (25%)",
                "rebuilt 4 events, through position 6 of 20 (30%)",
                "rebuilt 8 events, through position 11 of 20 (55%)",
                "rebuilt 15 events, through position 20 of 20 (100%)",
            ],
            "below a tenth nothing prints, and a batch inside the tenth already printed prints \
             nothing"
        );
        assert_eq!(
            lines(10, &[(20, 2), (30, 9)], 30),
            vec![
                "rebuilt 2 events, through position 20 of 30 (50%)",
                "rebuilt 9 events, through position 30 of 30 (100%)",
            ],
            "a resumed rebuild measures from where it started"
        );
        assert_eq!(
            lines(7, &[(7, 0)], 7),
            vec!["rebuilt 0 events, through position 7 of 7 (100%)"],
            "a log holding nothing past the start is wholly folded"
        );
    }

    /// spec 24, crit 2 (idempotency + non-clobbering, byte level): a pre-existing pre-commit
    /// hook that is NOT valid UTF-8 (e.g. a compiled/binary hook, or one carrying non-UTF-8
    /// bytes) must NEVER be clobbered by a fresh script (sdet-u24-1r-nonutf8-clobber-persists /
    /// d24-2-nonutf8-byte-compose-no-clobber). `install_precommit_hook` reads the hook as BYTES
    /// and composes at the byte level, so the original bytes are preserved and rigger's block
    /// is chained, and a rerun is a fixed point (no duplicate block).
    #[test]
    fn install_precommit_hook_preserves_a_non_utf8_existing_hook() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // git_hooks_dir falls back to <root>/.git/hooks when git cannot be consulted.
        let hooks = root.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook_path = hooks.join("pre-commit");
        // A valid ASCII shebang line then deliberately non-UTF-8 bytes in the body.
        let mut original: Vec<u8> = b"#!/bin/sh\n".to_vec();
        original.extend_from_slice(&[0xff, 0xfe, b'\n']);
        std::fs::write(&hook_path, &original).unwrap();

        let outcome = install_precommit_hook(root).unwrap();
        assert_eq!(
            outcome,
            InstallOutcome::Refreshed,
            "an existing hook is refreshed, not freshly installed"
        );
        let written = std::fs::read(&hook_path).unwrap();
        assert!(
            written
                .windows(original.len())
                .any(|w| w == original.as_slice())
                || written.windows(3).any(|w| w == [0xff, 0xfe, b'\n']),
            "the original non-UTF-8 hook bytes must be preserved, never clobbered"
        );
        assert!(
            written
                .windows(PRECOMMIT_BEGIN.len())
                .any(|w| w == PRECOMMIT_BEGIN.as_bytes()),
            "rigger's managed block is chained onto the non-UTF-8 hook"
        );

        // Idempotent: re-installing over the chained non-UTF-8 hook changes nothing and never
        // duplicates the block.
        let again = install_precommit_hook(root).unwrap();
        assert_eq!(
            again,
            InstallOutcome::AlreadyCurrent,
            "a rerun over the chained non-UTF-8 hook is a true no-op"
        );
        let rewritten = std::fs::read(&hook_path).unwrap();
        assert_eq!(
            written, rewritten,
            "the non-UTF-8 chained hook is a fixed point"
        );
        let begins = rewritten
            .windows(PRECOMMIT_BEGIN.len())
            .filter(|w| *w == PRECOMMIT_BEGIN.as_bytes())
            .count();
        assert_eq!(begins, 1, "the managed block is never duplicated");
    }

    /// Write the scaffold constants into a temp `.rigger/` (the same bytes
    /// `rigger init` emits) and load them through `config::load`: the scaffold must
    /// be a valid, referentially-complete config demonstrating the full DAG shape.
    #[test]
    fn scaffold_parses_into_a_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let rigger = dir.path().join(RIGGER_DIR);
        let agents = rigger.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(rigger.join("workflow.yml"), SCAFFOLD_WORKFLOW).unwrap();
        for (file, content) in SCAFFOLD_AGENTS {
            std::fs::write(agents.join(file), content).unwrap();
        }

        let cfg = config_store::load(dir.path().to_str().unwrap())
            .expect("the scaffolded config must load and validate");

        // Six CANONICAL agents: planner, rust-engineer, the two reviewer lenses
        // (architecture-reviewer + sdet), the adversary, the adjudicator. Integration is
        // folded into the unit lifecycle (no integrator). None of the four generic
        // placeholder personas is seeded.
        assert_eq!(cfg.agents.len(), 6, "scaffold agent count");
        // Four stages: plan -> plan-critique -> implement -> checkin. The plan-critique
        // gate (spec 10, Unit 1) reviews the proposed DAG before the fan-out releases; the
        // checkin stage (spec 91) runs the mutation sweep once, after every implement unit
        // has integrated.
        assert_eq!(cfg.workflow.stages.len(), 4, "scaffold stage count");
        // Seven gates in the reusable library, including the checkin stage's `mutation` gate,
        // the `boundary` and `audit` gates both unit stages carry and the implement stage's
        // `red-before-green` gate.
        assert_eq!(cfg.workflow.gates.len(), 7, "scaffold gate count");

        // The scaffold exercises the per-unit shape: a producer, the plan-critique gate
        // between plan and implement, a fan-out implement stage that integrates on_pass:
        // merge, and a three-tier review panel declared once on defaults.review.
        let plan = &cfg.workflow.stages["plan"];
        assert_eq!(plan.produces, "dag");
        // The plan-critique gate: review-only (no agent), needs plan, its adversary +
        // adjudicator gate the fan-out.
        let critique = &cfg.workflow.stages["plan-critique"];
        assert!(critique.agent.is_empty(), "the gate implements nothing");
        assert_eq!(critique.needs, ["plan"]);
        assert_eq!(critique.adversary, "adversary");
        assert_eq!(critique.adjudicator, "adjudicator");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        assert_eq!(
            implement.needs,
            ["plan-critique"],
            "the fan-out releases only after the plan-critique gate approves"
        );
        assert_eq!(implement.on_pass, "merge");
        // The checkin stage (spec 91): needs the fan-out implement TEMPLATE (satisfied once
        // every unit it expanded into has integrated - u91c1's generic conductor rule), runs
        // the mutation gate exactly once, remediates once, and integrates on pass.
        let checkin = &cfg.workflow.stages["checkin"];
        assert_eq!(checkin.needs, ["implement"]);
        assert_eq!(
            checkin.max_retries, 2,
            "an ATTEMPT bound like defaults.max_retries: the sweep, one remediation round, \
             the sweep again - a value of 1 escalates on the first miss (spec 91)"
        );
        assert_eq!(
            checkin.gates,
            ["build", "audit", "test", "lint", "boundary", "mutation"],
            "checkin re-verifies the whole gate suite, THEN sweeps mutants"
        );
        assert_eq!(checkin.on_pass, "merge");
        // A placeholder command, like the scaffold's other gates ("Replace the gate
        // commands with your own") - the SHAPE (a `mutation`-id gate the checkin stage
        // lists) is what this test proves, not a live cargo-mutants invocation.
        let mutation_gate = &cfg.workflow.gates["mutation"];
        assert_eq!(mutation_gate.kind, "core");
        let review = &cfg.workflow.defaults.review;
        assert_eq!(
            review.lenses,
            ["architecture-reviewer", "sdet"],
            "tier 1: the two canonical expert lenses"
        );
        assert_eq!(review.adversary, "adversary", "tier 2: refutes the lenses");
        assert_eq!(
            review.adjudicator, "adjudicator",
            "tier 3: the neutral adjudicator gates"
        );
        // The scaffold sets symbols EXPLICITLY (visible, not implicit) - it is the
        // default grounder (the structural symbol index), so a fresh `rigger init`
        // config grounds and reindexes without hitting the retired-grounder error.
        assert_eq!(cfg.workflow.defaults.grounder, "symbols");
        // FIX 3: the scaffold ships a NON-ZERO spawn budget so an unattended `rigger
        // run` cannot spawn unboundedly - 0 would be unlimited.
        assert!(
            cfg.workflow.defaults.budget > 0,
            "the scaffold must ship a non-zero default spawn budget; was {}",
            cfg.workflow.defaults.budget
        );
        assert_eq!(cfg.workflow.defaults.budget, 60, "scaffold default budget");
    }

    #[test]
    fn ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids() {
        let forms = [
            "git@github.com:Acme/Repo.git",
            "https://github.com/Acme/Repo.git",
            "https://github.com/Acme/Repo",
            "ssh://git@github.com/Acme/Repo.git",
            "git://github.com/Acme/Repo.git",
            "https://GitHub.com/Acme/Repo.git/",
        ];
        // Every form canonicalizes to the same normalized URL...
        assert_eq!(normalize_origin_url(forms[0]), "github.com/Acme/Repo");
        for f in forms {
            assert_eq!(
                normalize_origin_url(f),
                "github.com/Acme/Repo",
                "form {f:?} must normalize identically"
            );
        }
        // ...so the derived stable id is identical across all forms.
        let id0 = format!(
            "{:016x}",
            fnv1a_64(normalize_origin_url(forms[0]).as_bytes())
        );
        for f in forms {
            let id = format!("{:016x}", fnv1a_64(normalize_origin_url(f).as_bytes()));
            assert_eq!(id, id0, "form {f:?} must mint the same id");
        }
    }

    #[test]
    fn normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host() {
        assert_ne!(
            normalize_origin_url("git@github.com:Acme/One.git"),
            normalize_origin_url("git@github.com:Acme/Two.git")
        );
        // Host case is normalized; path case is significant (never lowercased).
        assert_eq!(
            normalize_origin_url("https://GITHUB.com/Acme/Repo"),
            normalize_origin_url("https://github.com/Acme/Repo")
        );
        assert_ne!(
            normalize_origin_url("https://github.com/Acme/Repo"),
            normalize_origin_url("https://github.com/acme/repo")
        );
    }

    /// Criterion 4: provisioning the JS driver is a silent no-op when the shim is
    /// already current - the runtime files match the embedded copies and npm's install
    /// is COMPLETE (its `node_modules/.package-lock.json` marker present) - so a `rigger
    /// setup` rerun does not rewrite the files or re-run npm. Faking a complete
    /// `node_modules` lets this assert the short-circuit WITHOUT npm: were the
    /// short-circuit broken, `provision_shim` would run npm and return `true` (or error
    /// when npm is absent), both of which fail this test.
    #[test]
    fn provision_shim_is_a_silent_noop_when_already_current() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();
        assert!(!shim_is_current(&shim), "no node_modules yet: not current");

        // A COMPLETE npm install leaves node_modules/.package-lock.json as its final
        // marker; only then is the shim current.
        let node_modules = shim.join("node_modules");
        std::fs::create_dir_all(&node_modules).unwrap();
        std::fs::write(node_modules.join(".package-lock.json"), "{}").unwrap();
        assert!(
            shim_is_current(&shim),
            "matching runtime files + a COMPLETE node_modules (marker present): current"
        );

        let provisioned = provision_shim(dir.path())
            .expect("a fully-provisioned shim must be a clean no-op (no npm needed)");
        assert!(
            !provisioned,
            "provision_shim must report no work when the shim is already current"
        );

        // A drifted runtime file makes the shim not-current again (an upgrade path).
        std::fs::write(shim.join("shim.mjs"), "// stale shim from an older build\n").unwrap();
        assert!(
            !shim_is_current(&shim),
            "a drifted runtime file must make the shim not-current"
        );
    }

    /// Criterion 4: setup SELF-HEALS a torn/partial shim install. An interrupted `npm
    /// ci` (which `rm -rf`s `node_modules` then repopulates incrementally) leaves a
    /// `node_modules` DIRECTORY that lacks npm's completeness marker
    /// (`node_modules/.package-lock.json`). `shim_is_current` must treat that as NOT
    /// current so the next `rigger setup` re-runs npm and repairs it, rather than
    /// short-circuiting on bare directory presence and permanently refusing to fix a
    /// broken install. Regression-locks adv-u4-shim-torn-install-not-self-healed.
    #[test]
    fn shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();

        // A torn install: node_modules exists (some deps partially unpacked) but the
        // final .package-lock.json marker a COMPLETE install writes is absent.
        std::fs::create_dir_all(shim.join("node_modules").join("some-partial-dep")).unwrap();
        assert!(
            !shim_is_current(&shim),
            "a node_modules dir lacking the .package-lock.json completeness marker is a torn \
             install and must NOT be treated as current"
        );

        // Adding the marker (as a completed npm install would) makes it current again.
        std::fs::write(shim.join("node_modules").join(".package-lock.json"), "{}").unwrap();
        assert!(
            shim_is_current(&shim),
            "once the completeness marker is present the shim is current"
        );
    }

    /// Criterion 4: scaffolding is idempotent. The first `init_project` on an empty
    /// project changes the tree and reports the agents it wrote; a second run finds
    /// everything present and is a silent no-op (`changed: false`, no new agents), so
    /// `rigger setup` / `rigger init` re-run without side effects.
    #[test]
    fn init_project_is_idempotent_reporting_new_work_only_once() {
        let dir = tempfile::tempdir().unwrap();

        let first = init_project(dir.path()).expect("first init scaffolds the project");
        assert!(
            first.changed(),
            "the first init on an empty project must change the tree"
        );
        assert!(
            !first.new_agents.is_empty(),
            "the first init scaffolds the workflow's referenced agents"
        );

        let second = init_project(dir.path()).expect("a rerun must succeed");
        assert!(
            !second.changed(),
            "a rerun on an initialized project must change nothing"
        );
        assert!(
            second.new_agents.is_empty(),
            "a rerun scaffolds no new agents"
        );
    }

    /// Criterion 4 (spec 05): the setup/init summary is HONEST per artifact - it must
    /// never claim a scaffold action it did not perform. On a gitignore-only repair (the
    /// primary Gap-9 upgrade path: `workflow.yml`, the agents, and the hook are all
    /// already present, but a `.gitignore` entry was lost and gets re-appended) the
    /// summary reports ONLY the gitignore change and does NOT emit the false "scaffolded
    /// workflow.yml / agents / installed hook" line. Regression-locks
    /// adv-u4-coarse-changed-summary-lies.
    #[test]
    fn scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair() {
        let dir = tempfile::tempdir().unwrap();

        // First init scaffolds everything AND appends the machine-local .gitignore
        // entries (a non-git temp dir is untracked, so the entries are written).
        let first = init_project(dir.path()).expect("first init scaffolds the project");
        assert!(
            first.wrote_workflow && !first.new_agents.is_empty() && first.wrote_hook,
            "the first init writes workflow.yml, the agents, and the hook"
        );
        assert!(
            !first.gitignore_added.is_empty(),
            "the first init appends the machine-local .gitignore entries"
        );

        // Simulate the Gap-9 upgrade path: only `.gitignore` needs repair; every other
        // scaffold artifact is still present and byte-identical.
        std::fs::remove_file(dir.path().join(".gitignore")).unwrap();

        let repair = init_project(dir.path()).expect("a gitignore-only repair must succeed");
        assert!(
            !repair.wrote_workflow,
            "workflow.yml already exists; it must NOT be reported as scaffolded"
        );
        assert!(
            repair.new_agents.is_empty(),
            "the agents already exist; none are newly written"
        );
        assert!(
            !repair.wrote_hook,
            "the hook is already installed; it must NOT be reported as installed"
        );
        assert!(
            !repair.gitignore_added.is_empty(),
            "the lost .gitignore entries are re-appended - the ONE real change this run made"
        );

        // The summary must report the gitignore change and NOTHING it did not do.
        let lines = scaffold_summary_lines(&repair);
        assert_eq!(
            lines.len(),
            1,
            "a gitignore-only repair reports exactly one line, got: {lines:?}"
        );
        assert!(
            lines[0].contains(".gitignore"),
            "the one line must report the gitignore change, got: {:?}",
            lines[0]
        );
        assert!(
            !lines.iter().any(|l| {
                l.contains("workflow.yml")
                    || l.contains(".rigger/agents/")
                    || l.contains("SessionStart hook")
            }),
            "a gitignore-only repair must not claim it scaffolded the workflow, agents, or \
             hook: {lines:?}"
        );
    }

    /// `rigger init`/`setup` on a fresh consumer repo appends one ignore line per `patterns`
    /// entry (reporting each), and a rerun appends none of them again - no duplicate accrues.
    fn assert_init_gitignores_idempotently(patterns: &[&str]) {
        let dir = tempfile::tempdir().unwrap();
        let first = init_project(dir.path()).expect("first init scaffolds the project");
        let gitignore = dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore).unwrap();
        let second = init_project(dir.path()).expect("a rerun must succeed");
        let after = std::fs::read_to_string(&gitignore).unwrap();
        for pattern in patterns {
            assert!(
                first.gitignore_added.contains(&pattern.to_string()),
                "the first init reports appending the {pattern} ignore pattern, got: {:?}",
                first.gitignore_added
            );
            assert!(
                content.lines().any(|l| l.trim() == *pattern),
                "the written .gitignore ignores {pattern}, got:\n{content}"
            );
            assert!(
                !second.gitignore_added.contains(&pattern.to_string()),
                "a rerun re-appends no {pattern} ignore pattern, got: {:?}",
                second.gitignore_added
            );
            assert_eq!(
                after.lines().filter(|l| l.trim() == *pattern).count(),
                1,
                "exactly one {pattern} ignore line - no duplicate accrued, got:\n{after}"
            );
        }
    }

    rigger::test_cases! {
        /// Spec 46, criterion 1 (CONSUMER GITIGNORE): the always-on dash writes two runtime
        /// breadcrumbs under `.rigger/` - `.rigger/dash.url` and `.rigger/dash.marker`. Left
        /// untracked-and-not-ignored in a consumer's repo they get swept into a unit worktree's
        /// commit by `git add`, then collide with the live dash's rewrites when the conductor
        /// merges the unit (`git merge` aborts with "untracked working tree files would be
        /// overwritten"). So `rigger init`/`setup` must append an ignore line for BOTH, exactly
        /// as it does for the other machine-local installs, and the append must be idempotent -
        /// a second setup adds no duplicate line.
        /// The round-8 `.rigger/dash.attempt` breadcrumb (spec 69) carries the same
        /// collision-with-a-unit-commit risk as the other two.
        init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently:
            assert_init_gitignores_idempotently(&[
            ".rigger/dash.url",
            ".rigger/dash.marker",
            ".rigger/dash.attempt",
        ]);
        /// Spec 48, SECRETS DISCIPLINE: the per-machine connection-string secret file
        /// `.rigger/store.conn` (store resolver rung 3) carries credentials, so `rigger init`/`setup`
        /// must git-ignore it BY CONSTRUCTION - the same scaffold mechanism that ignores the dash
        /// breadcrumbs - so a developer who drops their credentials into it can never commit them, and
        /// the committed project config never requires a secret. The append is idempotent: a second
        /// setup adds no duplicate line.
        init_project_gitignores_the_store_conn_secret_file_idempotently:
            assert_init_gitignores_idempotently(&[".rigger/store.conn"]);
    }

    /// Spec 46, criterion 1 (CONSUMER GITIGNORE), the broad-rule corner: even when a
    /// consumer's OWN committed `.gitignore` already covers both dash breadcrumbs through a
    /// broader rule (`.rigger/`), setup STILL appends the explicit `.rigger/dash.url` and
    /// `.rigger/dash.marker` lines. The committed `.gitignore` must be self-contained and
    /// portable, never contingent on any ignore resolution that could differ per machine, so
    /// the required lines are always present in the artifact shipped to a teammate/CI. The
    /// redundant-but-correct per-file line is harmless; the exact-line idempotency guard still
    /// prevents any duplicate. Proves setup does NOT let a broader ignore rule suppress the
    /// explicit dash lines (the regression a machine-local `git check-ignore` skip introduced).
    #[test]
    fn init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them() {
        let dir = tempfile::tempdir().unwrap();
        git_init_quiet(dir.path());
        // The consumer's own repo already ignores the entire runtime dir through a broad rule.
        std::fs::write(dir.path().join(".gitignore"), ".rigger/\n").unwrap();

        let report = init_project(dir.path()).expect("init must scaffold");
        assert!(
            report
                .gitignore_added
                .contains(&".rigger/dash.url".to_string())
                && report
                    .gitignore_added
                    .contains(&".rigger/dash.marker".to_string())
                && report
                    .gitignore_added
                    .contains(&".rigger/dash.attempt".to_string()),
            "setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}",
            report.gitignore_added
        );

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(
            content.lines().any(|l| l.trim() == ".rigger/dash.url")
                && content.lines().any(|l| l.trim() == ".rigger/dash.marker")
                && content.lines().any(|l| l.trim() == ".rigger/dash.attempt"),
            "all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"
        );

        // Idempotent: a rerun re-appends nothing (the exact lines are already present), so the
        // redundant-but-correct lines never accrue a duplicate.
        let second = init_project(dir.path()).expect("a rerun must succeed");
        assert!(
            !second
                .gitignore_added
                .contains(&".rigger/dash.url".to_string())
                && !second
                    .gitignore_added
                    .contains(&".rigger/dash.marker".to_string())
                && !second
                    .gitignore_added
                    .contains(&".rigger/dash.attempt".to_string()),
            "a rerun re-appends no dash line (exact-line idempotency), got: {:?}",
            second.gitignore_added
        );
    }

    /// Spec 08 item 2: the scaffold seed and the scaffold workflow reference the SAME
    /// canonical persona set - every seeded agent is referenced by the workflow and every
    /// referenced agent is seeded (no stray, unreferenced persona on a fresh-repo init) -
    /// and that set is the canonical six, with NONE of the four generic placeholder
    /// personas. A regression re-seeding a generic stray, or seeding an agent the workflow
    /// does not reference, fails here.
    #[test]
    fn scaffold_agents_and_workflow_reference_the_same_canonical_set() {
        use std::collections::BTreeSet;

        // Every agent id the scaffolded workflow references.
        let wf: config::Workflow =
            serde_yaml::from_str(SCAFFOLD_WORKFLOW).expect("the scaffolded workflow must parse");
        let mut referenced: BTreeSet<String> = wf.defaults.review.agent_ids().into_iter().collect();
        for stage in wf.stages.values() {
            referenced.extend(stage.agent_ids());
        }

        // Every agent id the scaffold seeds.
        let seeded: BTreeSet<String> = SCAFFOLD_AGENTS
            .iter()
            .map(|(_, c)| {
                config::parse_agent(c.as_bytes())
                    .expect("every seeded agent must parse")
                    .id
            })
            .collect();

        assert_eq!(
            seeded, referenced,
            "the seed and the scaffolded workflow must reference the same persona set: \
             seeded={seeded:?} referenced={referenced:?}"
        );

        let canonical: BTreeSet<String> = [
            "planner",
            "rust-engineer",
            "architecture-reviewer",
            "sdet",
            "adversary",
            "adjudicator",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            seeded, canonical,
            "the seed is exactly the canonical persona set"
        );

        // The four generic placeholder personas are gone for good - not a filename, not an
        // id (the strays spec 05/08 removed and must never re-scaffold).
        for stray in [
            "implementer",
            "devils-advocate",
            "reviewer.architecture",
            "reviewer.technical",
        ] {
            assert!(
                !seeded.contains(stray),
                "the generic persona {stray:?} must not be seeded"
            );
            assert!(
                !SCAFFOLD_AGENTS
                    .iter()
                    .any(|(f, _)| *f == format!("{stray}.md")),
                "the generic file {stray}.md must not be seeded"
            );
        }
    }

    /// Spec 08 item 3: the referenced-agent scaffold-skip filter. `init_project` scaffolds
    /// ONLY the seeded agents the workflow references, and skips (never writes) a seeded
    /// agent the workflow does not reference. Driven with a workflow that references just
    /// two of the six seeded agents: exactly those two are written, the other four are not.
    #[test]
    fn init_scaffolds_only_the_workflow_referenced_agents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rigger = root.join(RIGGER_DIR);
        let agents = rigger.join("agents");
        std::fs::create_dir_all(&agents).unwrap();

        // A pre-existing workflow that references only `planner` and `adversary`.
        // `init_project` keeps it (write_if_absent) and scaffolds against ITS references.
        std::fs::write(
            rigger.join("workflow.yml"),
            "stages:\n  plan:\n    agent: planner\n  go:\n    agent: adversary\n",
        )
        .unwrap();

        let report = init_project(root).expect("init must scaffold the referenced agents");

        assert!(
            agents.join("planner.md").exists(),
            "referenced planner seeded"
        );
        assert!(
            agents.join("adversary.md").exists(),
            "referenced adversary seeded"
        );
        for skipped in [
            "rust-engineer.md",
            "architecture-reviewer.md",
            "sdet.md",
            "adjudicator.md",
        ] {
            assert!(
                !agents.join(skipped).exists(),
                "an unreferenced seeded agent must NOT be scaffolded: {skipped}"
            );
        }
        let mut got = report.new_agents.clone();
        got.sort();
        assert_eq!(
            got,
            ["adversary.md", "planner.md"],
            "only the workflow-referenced agents are newly written"
        );
    }

    /// Checkin-round fix: `init_project` must fail loudly on a workflow.yml carrying an
    /// unrecognized key, never silently reinterpret the parse error as the empty-repo
    /// signal and scaffold every default agent over a deliberately curated fleet. A
    /// pre-existing two-agent fleet (mirroring the test above) is left exactly as it was -
    /// no new agent file appears - and `init_project` returns `Err` naming the dotted path.
    #[test]
    fn init_project_errors_loudly_on_an_unknown_key_and_does_not_scaffold_agents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rigger = root.join(RIGGER_DIR);
        let agents = rigger.join("agents");
        std::fs::create_dir_all(&agents).unwrap();

        // A pre-existing curated fleet (only `planner` + `adversary` referenced) whose
        // workflow.yml has since picked up a typo'd key.
        std::fs::write(
            rigger.join("workflow.yml"),
            "stages:\n  plan:\n    agent: planner\n  go:\n    agent: adversary\n  \
             bad:\n    agent: planner\n    gatez: [build]\n",
        )
        .unwrap();
        std::fs::write(
            agents.join("planner.md"),
            "---\nid: planner\n---\nCustom.\n",
        )
        .unwrap();
        std::fs::write(
            agents.join("adversary.md"),
            "---\nid: adversary\n---\nCustom.\n",
        )
        .unwrap();

        let err = init_project(root).expect_err(
            "a workflow.yml carrying an unknown key must fail init, not silently \
                          re-scaffold the default fleet",
        );
        assert!(
            err.to_string().contains("stages.bad.gatez: unknown key"),
            "must name the dotted path of the unrecognized key: {err}"
        );

        for skipped in [
            "rust-engineer.md",
            "architecture-reviewer.md",
            "sdet.md",
            "adjudicator.md",
        ] {
            assert!(
                !agents.join(skipped).exists(),
                "a failed load must NEVER fall back to scaffolding the full default fleet: \
                 {skipped} must not have been written"
            );
        }
    }

    /// Spec 08 item 3: `get_referenced_agent_ids` - the source of truth the scaffold-skip
    /// filter reads - returns exactly the agent ids the workflow references, and an empty
    /// set when there is no workflow (the empty-repo signal `init_project` uses to seed the
    /// full default fleet).
    #[test]
    fn get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rigger = root.join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger).unwrap();
        std::fs::write(rigger.join("workflow.yml"), SCAFFOLD_WORKFLOW).unwrap();

        let ids = get_referenced_agent_ids(root).unwrap();
        let want: std::collections::HashSet<String> = [
            "planner",
            "rust-engineer",
            "architecture-reviewer",
            "sdet",
            "adversary",
            "adjudicator",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            ids, want,
            "the referenced fleet is exactly the scaffolded canonical six"
        );

        let empty = tempfile::tempdir().unwrap();
        assert!(
            get_referenced_agent_ids(empty.path()).unwrap().is_empty(),
            "no workflow.yml yields an empty referenced set (the empty-repo seed signal)"
        );
    }

    /// Checkin-round fix (rejecting `arch-checkin-c3-get-referenced-agent-ids-bypasses-
    /// canonical-parser` / `adv-checkin-uphold-sharpen-arch-agent-ids-silent-swallow`): a
    /// workflow.yml carrying an unrecognized key must fail `get_referenced_agent_ids` with
    /// a real `Err` naming the dotted path, not the empty-repo `Ok(HashSet::new())` a raw
    /// `serde_yaml::from_str` bypassing `deny_unknown_fields`'s canonical parser used to
    /// produce. A PRESENT-but-malformed workflow is never the same signal as an ABSENT one
    /// (the empty-repo case the prior test above still covers): only the latter may resolve
    /// to an empty set.
    #[test]
    fn get_referenced_agent_ids_errors_loudly_on_an_unknown_key_instead_of_returning_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rigger = root.join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger).unwrap();
        std::fs::write(
            rigger.join("workflow.yml"),
            "defaults:\n  max_parallel_unitz: 2\n",
        )
        .unwrap();

        let err = get_referenced_agent_ids(root)
            .expect_err("an unknown key must fail to load, never resolve to an empty set");
        assert!(
            err.to_string()
                .contains("defaults.max_parallel_unitz: unknown key"),
            "must name the dotted path of the unrecognized key through the canonical parser: {err}"
        );
    }

    /// Spec 08 item 4: a FAILED scaffold write is an error naming the artifact, never a
    /// swallowed `false` that drops the artifact from the summary while setup exits 0. An
    /// already-present file is a silent `Ok(false)` (kept), a fresh path is `Ok(true)`
    /// (wrote), and a genuine write failure is `Err` naming the path.
    #[test]
    fn write_if_absent_wrote_kept_and_errors_naming_the_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Fresh path -> wrote.
        let fresh = root.join("fresh.txt");
        assert!(
            write_if_absent(&fresh, "hi").unwrap(),
            "a fresh path is newly written"
        );
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "hi");

        // Already present -> kept, silent, and left byte-for-byte untouched.
        assert!(
            !write_if_absent(&fresh, "OVERWRITE").unwrap(),
            "an existing file is kept, not rewritten"
        );
        assert_eq!(
            std::fs::read_to_string(&fresh).unwrap(),
            "hi",
            "keeping never touches the existing bytes"
        );

        // A genuine write failure (the parent directory does not exist) is an ERROR that
        // names the artifact - not a swallowed false.
        let unwritable = root.join("no-such-dir").join("agent.md");
        let err = write_if_absent(&unwritable, "x")
            .expect_err("a failed write must be an error, not a swallowed false");
        assert!(
            err.to_string().contains("agent.md"),
            "the error must name the artifact it could not write; got: {err}"
        );
    }

    // ---- `rigger setup --agents <dir>`: importing a starting fleet from a local dir ----

    /// `rigger setup` takes only the `--agents <dir>` flag; a bare setup parses to no
    /// import, `--agents <dir>` captures the source directory, a missing value errors,
    /// and an unknown flag errors (never a silent skip).
    #[test]
    fn parse_setup_args_reads_the_agents_directory_flag() {
        assert!(parse_setup_args(&[]).unwrap().agents_dir.is_none());

        let opts = parse_setup_args(&["--agents".into(), "/some/collection".into()]).unwrap();
        assert_eq!(
            opts.agents_dir.as_deref(),
            Some(Path::new("/some/collection"))
        );

        assert!(
            parse_setup_args(&["--agents".into()]).is_err(),
            "--agents with no directory must be a clear error"
        );
        assert!(
            parse_setup_args(&["--bogus".into()]).is_err(),
            "an unknown setup flag must be a clear error"
        );
    }

    /// `import_agents` copies each `.md` from a local collection directory into
    /// `.rigger/agents/`, normalizing the collection's identity field (`name:`) to
    /// Rigger's `id:` so a foreign agent loads under Rigger's schema. The imported file
    /// parses via the same `config::parse_agent` the loader uses.
    #[test]
    fn import_agents_copies_and_normalizes_the_identity_field() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A valid project to validate against (workflow + the default fleet).
        init_project(root).unwrap();

        // A foreign collection whose agents use `name:` as their identity field (the
        // Claude Code / agency-agents shape), plus an extra unknown frontmatter key.
        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("researcher.md"),
            "---\nname: researcher\ndescription: digs up prior art\nmodel: sonnet\n---\n\
             You research prior art and cite sources.\n",
        )
        .unwrap();
        // A non-.md file must be ignored.
        std::fs::write(src.join("README.txt"), "not an agent").unwrap();

        let summary = import_agents(root, &src).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                imported: 1,
                skipped: 0
            }
        );

        let imported = std::fs::read_to_string(root.join(".rigger/agents/researcher.md")).unwrap();
        assert!(
            imported.contains("id: researcher"),
            "the identity field must be normalized to `id:`; got:\n{imported}"
        );
        assert!(
            !imported.contains("name: researcher"),
            "the original `name:` identity key must be renamed, not left in place"
        );
        // The extra frontmatter and the prompt body survive the normalization untouched.
        assert!(imported.contains("description: digs up prior art"));
        assert!(imported.contains("You research prior art and cite sources."));

        // It parses under Rigger's schema with the normalized id.
        let a = config::parse_agent(imported.as_bytes()).unwrap();
        assert_eq!(a.id, "researcher");
        assert_eq!(a.model, "sonnet");
    }

    /// Import never overwrites an existing agent file: a collection file whose name
    /// collides with one already in `.rigger/agents/` is kept as-is and counted as
    /// skipped, so a re-run (or importing over the scaffolded fleet) is safe.
    #[test]
    fn import_agents_refuses_to_overwrite_an_existing_agent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();

        // `planner.md` already exists (scaffolded by init_project). Capture it.
        let existing_path = root.join(".rigger/agents/planner.md");
        let original = std::fs::read_to_string(&existing_path).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("planner.md"),
            "---\nname: planner\n---\nA DIFFERENT planner that must not clobber the local one.\n",
        )
        .unwrap();
        std::fs::write(
            src.join("newcomer.md"),
            "---\nid: newcomer\n---\nBrand new agent.\n",
        )
        .unwrap();

        let summary = import_agents(root, &src).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                imported: 1,
                skipped: 1
            },
            "the colliding planner.md is skipped; only newcomer.md is imported"
        );
        assert_eq!(
            std::fs::read_to_string(&existing_path).unwrap(),
            original,
            "the pre-existing agent file must be left byte-for-byte untouched"
        );
        assert!(root.join(".rigger/agents/newcomer.md").exists());
    }

    /// Importing a collection of `files` into a freshly scaffolded project fails (`why`),
    /// and none of the files is written into `.rigger/agents/` - the import aborts atomically.
    fn assert_import_refused(files: &[(&str, &str)], why: &str) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();
        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        for (name, content) in files {
            std::fs::write(src.join(name), content).unwrap();
        }
        assert!(import_agents(root, &src).is_err(), "{why}");
        for (name, _) in files {
            assert!(
                !root.join(RIGGER_DIR).join("agents").join(name).exists(),
                "{name} must NOT be written - the import aborts before writing"
            );
        }
    }

    rigger::test_cases! {
        /// Import runs the same validation `rigger validate` applies: a malformed agent
        /// file (no frontmatter) fails the import loudly instead of writing a file that
        /// would later break `config::load`.
        import_agents_validates_and_rejects_a_malformed_agent: assert_import_refused(
            &[("broken.md", "no frontmatter here, just prose\n")],
            "an agent file with no YAML frontmatter must fail the import validation",
        );
        /// Import is atomic on an id collision with an agent already on disk. A collection
        /// file whose normalized id equals a scaffolded agent's - under a DIFFERENT filename,
        /// so the filename-only overwrite guard does not catch it - is rejected BEFORE any
        /// write, leaving `.rigger/agents/` untouched. Without this, the file is written and
        /// the trailing whole-fleet load then fails on the duplicate id, bricking every later
        /// `config::load`.
        import_agents_rejects_an_id_colliding_with_an_existing_agent: assert_import_refused(
            // A different filename, but its id collides with the scaffolded `planner`.
            &[(
                "my-planner.md",
                "---\nid: planner\n---\nA colliding planner under a new filename.\n",
            )],
            "an imported id that collides with an existing agent must fail the import",
        );
        /// Import is atomic on a duplicate id WITHIN one import: two collection files that
        /// normalize to the same id are rejected before either is written, so no half-import
        /// is left behind.
        import_agents_rejects_a_duplicate_id_within_one_import: assert_import_refused(
            // `name:` normalizes to the same `id: twin`.
            &[
                ("a-dup.md", "---\nid: twin\n---\nFirst.\n"),
                ("b-dup.md", "---\nname: twin\n---\nSecond.\n"),
            ],
            "two imported files sharing an id must fail the import",
        );
        /// Import rejects an agent whose identity field is present but blank - the empty-id
        /// arm - by the SAME rule `config::load` applies, and writes nothing. A `name:` with
        /// an empty value normalizes to a blank `id:`.
        import_agents_rejects_an_agent_with_a_blank_id: assert_import_refused(
            &[(
                "blank.md",
                "---\nname: \"\"\ndescription: has a blank identity\n---\nBody.\n",
            )],
            "a blank id must fail the import (the same rule config::load enforces)",
        );
    }

    /// Import runs the SAME whole-project validation `rigger validate` applies: a project
    /// whose workflow references a missing agent fails the import even when the imported
    /// file itself is well-formed. This drives the trailing `config::load` referential
    /// check.
    #[test]
    fn import_agents_runs_full_validation_and_rejects_a_broken_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();
        // Break a workflow agent reference so the whole-project load fails referentially.
        let wf_path = root.join(".rigger/workflow.yml");
        let wf = std::fs::read_to_string(&wf_path).unwrap();
        std::fs::write(&wf_path, wf.replace("agent: rust-engineer", "agent: ghost")).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("newcomer.md"),
            "---\nid: newcomer\n---\nA well-formed new agent.\n",
        )
        .unwrap();

        assert!(
            import_agents(root, &src).is_err(),
            "import must run the same validation `rigger validate` applies and reject a \
             project whose workflow references a missing agent"
        );
    }

    /// `rigger setup` runs npm install against the provisioned shim so `node_modules`
    /// is ready. When npm is available we run it FOR REAL against a temp dir and
    /// confirm `node_modules` appears; when npm is unavailable we assert the clear
    /// error path instead (never a silent skip).
    #[test]
    fn setup_runs_npm_install_or_reports_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();

        if tool_available("npm", "--version") {
            // npm is on PATH: provisioning must run it for real and leave node_modules.
            provision_shim(dir.path()).expect("provision_shim must succeed when npm is available");
            assert!(
                shim.join("node_modules").is_dir(),
                "npm install must populate node_modules in the provisioned shim dir"
            );
        } else {
            // npm is NOT on PATH: the error must be clear and actionable, not a silent
            // skip. Point RIGGER_NPM at a binary that does not exist to exercise the
            // missing-npm path deterministically.
            std::env::set_var("RIGGER_NPM", "definitely-not-a-real-npm-binary-xyz");
            let err = run_npm_install(&shim).expect_err("a missing npm must be a clear error");
            std::env::remove_var("RIGGER_NPM");
            let msg = err.to_string();
            assert!(
                msg.contains("npm") && msg.to_lowercase().contains("path"),
                "the missing-npm error must mention npm and PATH; got: {msg}"
            );
        }
    }

    /// The scaffolded workflow (`rigger init`/`setup` on a NEW project) declares
    /// `build: { wrapper: auto }` (spec 65 Design: "`rigger setup` writes the `build:`
    /// section with `wrapper: auto` for new projects") - a fresh project benefits from a
    /// machine's already-installed compilation-cache wrapper with no further config.
    #[test]
    fn scaffold_workflow_declares_build_wrapper_auto() {
        let wf: config::Workflow =
            serde_yaml::from_str(SCAFFOLD_WORKFLOW).expect("the scaffolded workflow must parse");
        assert_eq!(
            wf.build.wrapper, "auto",
            "a freshly scaffolded workflow.yml must default build.wrapper to auto"
        );
    }

    /// Spec 102 criterion 2 (THE SCAFFOLD WRITES THE KEY): `rigger init`/`setup` scaffold
    /// `defaults.max_parallel_units: 2`, the wave-width bound `run_wave` (spec 102) enforces,
    /// WITH a sizing comment explaining why 2 - not a bare unexplained number. This test owns
    /// the key's scaffolded value and the presence of its comment; the byte-for-byte
    /// file-write path is covered separately below.
    #[test]
    fn scaffold_workflow_declares_max_parallel_units_two_with_a_sizing_comment() {
        let wf: config::Workflow =
            serde_yaml::from_str(SCAFFOLD_WORKFLOW).expect("the scaffolded workflow must parse");
        assert_eq!(
            wf.defaults.max_parallel_units, 2,
            "a freshly scaffolded workflow.yml must default max_parallel_units to 2"
        );
        assert!(
            SCAFFOLD_WORKFLOW.contains("build cache")
                && SCAFFOLD_WORKFLOW.contains("max_parallel_units: 2"),
            "the scaffold must carry a sizing comment (explaining the per-unit build-cache \
             cost) next to max_parallel_units, not a bare number"
        );
    }

    /// Spec 102 criterion 2: `rigger init` on a FRESH project actually WRITES
    /// `defaults.max_parallel_units: 2` with its sizing comment to disk - the scaffold
    /// constant proven above must be what `init_project` emits, not just what it declares.
    #[test]
    fn init_project_writes_max_parallel_units_with_its_sizing_comment() {
        let dir = tempfile::tempdir().unwrap();
        init_project(dir.path()).expect("a fresh project must scaffold cleanly");
        let workflow_path = dir.path().join(RIGGER_DIR).join("workflow.yml");
        let written = std::fs::read_to_string(&workflow_path).unwrap();
        assert!(
            written.contains("max_parallel_units: 2"),
            "rigger init must write defaults.max_parallel_units: 2; got:\n{written}"
        );
        assert!(
            written.contains("build cache"),
            "rigger init must write the sizing comment alongside max_parallel_units; got:\n{written}"
        );
    }

    /// `rigger setup`/`init` NEVER clobbers an existing `workflow.yml` (spec 65 Design:
    /// "never clobbers an existing one") - including a committed `build:` section that
    /// differs from the fresh-project default. A project that has already opted OUT
    /// (`wrapper: off`) must stay opted out across a rerun, never silently flipped back to
    /// `auto`.
    #[test]
    fn init_project_never_clobbers_an_existing_build_section() {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let workflow_path = rigger_dir.join("workflow.yml");
        std::fs::write(&workflow_path, "build:\n  wrapper: off\n").unwrap();

        init_project(dir.path()).expect("a rerun over an existing project must succeed");

        let after = std::fs::read_to_string(&workflow_path).unwrap();
        assert_eq!(
            after, "build:\n  wrapper: off\n",
            "an existing workflow.yml's build: section must be left byte-for-byte untouched"
        );
    }

    // =======================================================================================
    // Spec 92, criterion 4 (IN EVERY SESSION'S HAND): `rigger setup` registers the operator's
    // own MCP lookup surface and the graph-first PreToolUse hook, and `rigger grep-guard`
    // bounces a bare source grep.
    // =======================================================================================

    /// `install_operator_mcp` is drift-aware and re-runnable, the same three-state contract
    /// every other `rigger setup` install artifact has: absent -> Installed (fresh write),
    /// drifted (an older build's entry, or a hand edit) -> Refreshed, already matching ->
    /// AlreadyCurrent (a silent no-op, not even an mtime bump).
    #[test]
    fn install_operator_mcp_installs_refreshes_and_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mcp_path = root.join(".mcp.json");

        assert_eq!(
            install_operator_mcp(root).unwrap(),
            InstallOutcome::Installed,
            "the first install reports a fresh install"
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&mcp_path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["rigger"]["command"], "rigger");
        assert_eq!(v["mcpServers"]["rigger"]["args"][0], "mcp");

        assert_eq!(
            install_operator_mcp(root).unwrap(),
            InstallOutcome::AlreadyCurrent,
            "a rerun on an up-to-date .mcp.json changes nothing"
        );

        // Simulate drift: an older build (or a hand edit) wrote a different command.
        std::fs::write(
            &mcp_path,
            r#"{"mcpServers":{"rigger":{"command":"/old/stale/path","args":[]}}}"#,
        )
        .unwrap();
        assert_eq!(
            install_operator_mcp(root).unwrap(),
            InstallOutcome::Refreshed,
            "a drifted entry self-heals"
        );
        let v2: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&mcp_path).unwrap()).unwrap();
        assert_eq!(v2["mcpServers"]["rigger"]["command"], "rigger");
    }

    /// `install_lookup_hook` has the same three-state contract, and (unlike the SessionStart
    /// merge, whose event only ever holds rigger's own entry) must preserve an UNRELATED
    /// PreToolUse hook a machine already carries under a different matcher/command.
    #[test]
    fn install_lookup_hook_installs_refreshes_and_is_a_noop_and_preserves_foreign_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let settings_path = root.join(".claude").join("settings.json");
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(
            &settings_path,
            r#"{"hooks":{"PreToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"prettier --write"}]}]}}"#,
        )
        .unwrap();

        assert_eq!(
            install_lookup_hook(root).unwrap(),
            InstallOutcome::Refreshed,
            "the settings file already existed, so adding our hook to it is a refresh"
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        let blocks = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(
            blocks
                .iter()
                .any(|b| b["hooks"][0]["command"] == "prettier --write"),
            "the pre-existing foreign hook must survive"
        );
        assert!(
            blocks
                .iter()
                .any(|b| b["hooks"][0]["command"] == hooks::GREP_GUARD_COMMAND
                    && b["matcher"] == hooks::GREP_GUARD_MATCHER),
            "our lookup hook must be installed"
        );

        assert_eq!(
            install_lookup_hook(root).unwrap(),
            InstallOutcome::AlreadyCurrent,
            "a rerun changes nothing"
        );
    }

    /// The repository root: this binary crate's manifest directory.
    fn repo() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Every committed persona under `.rigger/agents/`, `(file name, text)`, sorted by name.
    fn committed_personas() -> Vec<(String, String)> {
        let dir = repo().join(".rigger").join("agents");
        let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .map(|p| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read_to_string(&p).unwrap())
            })
            .collect();
        out.sort();
        out
    }

    /// Every agent text rigger ships: the committed personas and the scaffold seeds, each
    /// labelled with where it lives.
    fn every_shipped_persona() -> Vec<(String, String)> {
        let mut all: Vec<(String, String)> = committed_personas()
            .into_iter()
            .map(|(f, t)| (format!(".rigger/agents/{f}"), t))
            .collect();
        all.extend(
            SCAFFOLD_AGENTS
                .iter()
                .map(|(f, t)| (format!("SCAFFOLD_AGENTS {f}"), (*t).to_string())),
        );
        all
    }

    /// Byran's seam rulings: every persona, reviewers included, runs on opus with no ladder,
    /// and may fan its mechanical work out (the `Agent` tool, `recurse: true`), in the
    /// committed fleet and in every scaffold seed alike.
    #[test]
    fn every_persona_and_scaffold_seed_runs_on_opus_and_may_fan_out() {
        assert_eq!(
            committed_personas().len(),
            8,
            "the committed fleet is eight personas"
        );
        for (at, text) in every_shipped_persona() {
            let def = config::parse_agent(text.as_bytes())
                .unwrap_or_else(|e| panic!("{at} must parse: {e}"));
            assert_eq!(def.model, "opus", "{at} runs on opus");
            assert!(def.model_ladder.is_empty(), "{at} carries no model ladder");
            assert!(
                def.tools.iter().any(|t| t == "Agent"),
                "{at} grants the Agent tool; got {:?}",
                def.tools
            );
            assert!(def.recurse, "{at} is `recurse: true`");
            assert!(
                !text.contains("cannot fan out"),
                "{at} still says it cannot fan out"
            );
        }
    }

    /// DRY: a harness-wide rule lives in the built-in instruction layer, which reaches every
    /// spawn; no persona file or scaffold seed carries a copy of any of its lines.
    #[test]
    fn personas_and_seeds_carry_no_copy_of_a_built_in_instruction() {
        for (at, text) in every_shipped_persona() {
            for (name, body) in instructions::BUILTIN {
                for line in body.lines().map(str::trim).filter(|l| l.len() > 40) {
                    assert!(
                        !text.contains(line),
                        "{at} copies a line of the built-in {name:?} (it reaches every spawn \
                         already): {line:?}"
                    );
                }
            }
        }
    }

    /// A persona is prompt text that reaches every spawn of its role verbatim, so no committed
    /// persona or scaffold seed carries a stray tool-call markup token left by the editor that
    /// wrote it.
    #[test]
    fn no_persona_carries_stray_tool_markup() {
        for (at, text) in every_shipped_persona() {
            for token in ["</content>", "</invoke>", "<parameter", "<invoke"] {
                assert!(
                    !text.contains(token),
                    "{at} carries the stray tool markup {token:?}"
                );
            }
        }
    }

    /// The plan protocol in the binary is the one source of the rule that splits a criterion too
    /// large for one unit into ordered units: no shipped planner persona restates the rule or
    /// forbids the split.
    #[test]
    fn the_planner_persona_never_contradicts_the_plan_protocol_split_rule() {
        let planners: Vec<(String, String)> = every_shipped_persona()
            .into_iter()
            .filter(|(at, _)| at.ends_with("planner.md"))
            .collect();
        assert_eq!(
            planners.len(),
            2,
            "the committed planner and its scaffold seed"
        );
        for (at, text) in planners {
            let text = text.to_lowercase();
            for claim in [
                "split",
                "exactly one unit",
                "one unit per criterion",
                "one per acceptance criterion",
            ] {
                assert!(
                    !text.contains(claim),
                    "{at} restates the split rule the plan protocol owns: {claim:?}"
                );
            }
        }
    }

    /// The roles that write or judge tests carry the mutation-proof test block exactly once;
    /// no other role does.
    #[test]
    fn the_test_roles_carry_the_mutation_test_block_exactly_once() {
        const HEADING: &str = "## Every test must survive mutation testing";
        for (file, text) in committed_personas() {
            let want = usize::from(matches!(
                file.as_str(),
                "rust-engineer.md" | "sdet-author.md" | "sdet.md"
            ));
            assert_eq!(
                text.matches(HEADING).count(),
                want,
                "{file}: the test block appears {want} time(s)"
            );
        }
    }

    /// A mutation survivor is always a failure: the engineer closes it with a failing test or
    /// a rewrite, never by justifying it, and its accounting has no justified status.
    #[test]
    fn rust_engineer_closes_every_survivor_and_never_justifies_one() {
        let text = std::fs::read_to_string(repo().join(".rigger/agents/rust-engineer.md")).unwrap();
        for banned in ["JUSTIFIED", "missed-justified", "equivalence reason"] {
            assert!(
                !text.contains(banned),
                "rust-engineer.md still offers {banned:?}"
            );
        }
        for wanted in [
            "(surviving) mutant is always a failure",
            "never by an\n  `exclude_re` or `mutants::skip`",
            "caught | missed-caught (naming the catching test) | unviable | timeout",
        ] {
            assert!(
                text.contains(wanted),
                "rust-engineer.md must say {wanted:?}"
            );
        }
    }

    /// The CLI handlers live in `src/cli/`; the periphery author's CLI-surface probe diffs
    /// them alongside the registry in `src/main.rs`.
    #[test]
    fn sdet_author_probes_the_cli_modules_for_new_cli_surface() {
        let text = std::fs::read_to_string(repo().join(".rigger/agents/sdet-author.md")).unwrap();
        assert!(
            text.contains("git diff BASE -- src/main.rs src/cli"),
            "the CLI probe must cover src/cli"
        );
        assert!(
            !text.contains("git diff BASE -- src/main.rs |"),
            "the CLI probe must not stop at src/main.rs"
        );
    }

    /// The fan-out helpers the working discipline names live at `.claude/agents/`: committed
    /// in this repository and scaffolded byte-identical into every project `init` sets up.
    #[test]
    fn init_scaffolds_the_fan_out_helpers_byte_identical_to_the_committed_files() {
        let dir = tempfile::tempdir().unwrap();
        init_project(dir.path()).expect("init must scaffold");
        for (file, model) in [
            ("lookup.md", "model: haiku"),
            ("verify.md", "model: sonnet"),
        ] {
            let committed = repo().join(".claude").join("agents").join(file);
            let committed = std::fs::read_to_string(&committed)
                .unwrap_or_else(|e| panic!("{} must be committed: {e}", committed.display()));
            let scaffolded = dir.path().join(".claude").join("agents").join(file);
            let scaffolded = std::fs::read_to_string(&scaffolded)
                .unwrap_or_else(|e| panic!("init must write {}: {e}", scaffolded.display()));
            assert_eq!(
                scaffolded, committed,
                "{file}: the scaffold mirrors the committed file"
            );
            assert!(committed.contains(model), "{file} runs on {model}");
        }
        let rerun = init_project(dir.path()).expect("a rerun must succeed");
        assert!(!rerun.changed(), "a rerun rewrites no helper");
    }

    /// KISS is the fewest moving parts, never a line count: no agent, instruction or handbook
    /// text rigger ships states a size limit.
    #[test]
    fn no_size_limit_sentence_ships_in_any_agent_instruction_or_handbook_text() {
        let limit = regex::Regex::new(
            r"too_many_lines|cognitive_complexity|over \d[\d,]* lines|\d[\d,]* lines (fails|caps|limit)",
        )
        .unwrap();
        let mut texts = every_shipped_persona();
        for dir in ["crates/rigger-domain/src/instructions", ".claude/agents"] {
            for entry in std::fs::read_dir(repo().join(dir)).into_iter().flatten() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|x| x == "md") {
                    texts.push((
                        path.display().to_string(),
                        std::fs::read_to_string(&path).unwrap(),
                    ));
                }
            }
        }
        let handbook = repo().join("docs/handbook/authoring-agents.md");
        texts.push((
            handbook.display().to_string(),
            std::fs::read_to_string(&handbook).unwrap(),
        ));
        for (name, body) in instructions::BUILTIN {
            texts.push((format!("BUILTIN {name}"), (*body).to_string()));
        }
        let hits: Vec<String> = texts
            .iter()
            .flat_map(|(at, text)| {
                text.lines()
                    .enumerate()
                    .filter(|(_, l)| limit.is_match(l))
                    .map(move |(i, l)| format!("{at}:{}: {l}", i + 1))
            })
            .collect();
        assert!(
            hits.is_empty(),
            "size-limit sentences ship:\n{}",
            hits.join("\n")
        );
    }
}
