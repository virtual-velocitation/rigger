# 125 - rigger works in any language, with any toolchain, on any OS

**Goal:** a consumer in any language, with any toolchain, on Linux, macOS or Windows gets from the
rigger binary only tools, the declared workflow's enforcement, contract and framing: its language
comes from a template root it names, its toolchain's variables from its own workflow, its code
facts from an extractor it may declare, and every OS facility from a native per-OS adapter. Read
at 26942c67, four couplings stand against that:

- The binary embeds a language. `build.rs` generates `TEMPLATE_SETS` from `scaffold/` at compile
  time (`build/template_sets.rs`, included at `src/cli/setup.rs:1884`), so the Rust gate template
  set, its gate scripts and its implementer persona ship inside every binary, and its `files`
  list (`scaffold/rust/files`) copies two scripts written for this repository's own crates into a
  consumer. The scaffold seeds no test author, so a scaffolded loop never runs the test-authoring
  step; its implementer seed (`setup.rs:2018-2029`) tells the implementer to "run the named
  gates, commit", work the gate stage repeats on the commit; its workflow declares no
  `max_wall_clock`, so `rigger validate` warns on a fresh scaffold; and `rigger init` ignores six
  runtime paths (`setup.rs:229-236`), leaving the event, graph and progress databases, their
  locks and the scratch directory to be swept into a commit.
- The binary exports a toolchain's variables unasked. Both gate sites set `CARGO_TARGET_DIR`
  through `spawn::BUILD_CACHE_ENV` (`crates/rigger-gates-shell/src/gate.rs:619`, :652,
  `crates/rigger-domain/src/spawn.rs:833`), every spawn gets it from `spawn_env`
  (`crates/rigger-conductor/src/conductor.rs:9527-9541`, `build_cache_var` :885),
  `build.wrapper` resolves only the two named wrappers (`KNOWN_WRAPPERS`, `gate.rs:235`), the
  scaffold writes `wrapper: auto` (`setup.rs:1816-1821`), and a new scratch root gets a toolchain
  config file (`crates/rigger-worktree-git/src/worktree.rs:1848-1871`). Another toolchain gets no
  per-unit cache and no job budget (`docs/handbook/authoring-loops.md:19`).
- The code store's extractor, `FactsExtractor` (`crates/rigger-grounder/src/facts.rs:97`), reads
  a file as code only through the bundled grammar registry (`registry::for_path`), six languages,
  so a project in another language gets no code half in its graph.
- rigger does not compile on Windows, and three OS facilities have one OS's adapter only.
  `crates/rigger-process/src/reap.rs:90` imports the system-call dependency's process module with
  no platform gate, and `setup.rs:592` imports a Unix permissions trait ungated, so
  `rigger-process` and every crate above it fail there. The process facts the reaper, the holder
  check and the dash read come only from `/proc` (`reap.rs:121-303`,
  `crates/rigger-process/src/holders.rs:21-83`, `crates/rigger-dash/src/dash.rs:542-602`); every
  gate runs under `sh` and, when guarded, under the external lock command (`gate.rs:607-613`);
  four state and cache homes read `HOME`, which Windows does not set; the gate request channel
  answers `Unsupported` off Unix (`crates/rigger-process/src/gate_socket.rs:357-388`); the Linux
  keep-awake adapter takes logind's inhibitor alone, which a graphical desktop's own idle suspend
  does not consult (`crates/rigger-process/src/keep_awake/linux.rs:29`); and CI runs on
  `ubuntu-latest` only (`.github/workflows/rust.yml:51`).

## Design

**BASE, decided here.** Spec 125 lands on `rigger-run` and is written against the tree at
26942c67, spec 121 landed: 121's Design and criteria are read as landed and none is restated;
where this spec replaces a 121 decision, SUPERSEDES names it. The symbol governs and the line is
a pointer. The per-OS adapter form is the keep-awake interface's
(`crates/rigger-process/src/keep_awake.rs:11-25` with `keep_awake/{linux,macos,windows}.rs`): one
`std::cfg_select!` binds each OS's module and a `_` arm covers every other OS. Every interface
this spec adds takes that form, and its macOS and Windows adapters are written from the OS
vendors' documented APIs.

**SUPERSEDES, decided here.**

| 121 text | Replaced by |
| --- | --- |
| criterion 6's last sentence ("The scaffold seeds no sdet-author persona and declares no test author (SCAFFOLD DEFAULTS), so this repository's persona is the rule's one home; a consumer declaring a test author writes its own rule.") | criterion 2: the scaffold seeds a test author carrying the rule and declares it |
| SCAFFOLD DEFAULTS' "no `test_author` (... which the scaffold never seeds)" and criterion 10's "`SCAFFOLD_WORKFLOW` declares none (SCAFFOLD DEFAULTS)" | criterion 2 |
| criterion 15's base seed sentence "confirm GREEN, run the named gates, commit." in the `implementer.md` seed | criterion 3; the rest of criterion 15 stands |
| criterion 11's `spawn::BUILD_CACHE_ENV` and its readers: `RunCtx::spawn_env`, both `ExecRunner::run` sites, `spawn_request`'s environment lookup and `reviewer_carries_build_cache` | criterion 4 |

**INVENTORY, decided here.** Every item the four couplings reach, where it lives, whether it is
language-, toolchain- or OS-specific, and where it goes.

PERSONAS AND SCAFFOLD:

| Item | Where | Kind | Goes to |
| --- | --- | --- | --- |
| the Rust gate template set: `set.yml`, the `.rigger/gates/` scripts its `files` list names, its `implementer:` persona | compiled in by `build.rs` and `build/template_sets.rs`, `include!` at `setup.rs:1884` | language-specific | out of the binary; stays in this repository's `scaffold/`, read at run time from the root `--template <dir>` names (criterion 1) |
| `mutation.sh` and `container-env.sh` in the Rust set's `files` | `scaffold/rust/files` | this repository's own crates (`container-env.sh:7`) | out of the set; they stay this repository's gates (criterion 1) |
| `SCAFFOLD_WORKFLOW` | `setup.rs:1778-1871` | neutral but for its wrapper block (`:1816-1821`) | THE SCAFFOLDED WORKFLOW (criteria 1, 2, 4, 5, 6) |
| the `planner`, `architecture-reviewer`, `sdet`, `adversary` and `adjudicator` seeds | `SCAFFOLD_AGENTS`, `setup.rs:1994` | neutral (no language marker) | stay: starter personas a consumer owns once written |
| the `implementer.md` seed | `SCAFFOLD_AGENTS`, `setup.rs:2018-2029` | neutral | stays; one sentence changes (criterion 3) |
| a test author seed | none | - | a neutral `sdet-author.md` seed (criterion 2) |
| the runtime ignore lines | `init_project`, `setup.rs:229-236` | neutral | one list naming rigger's runtime files by class (criterion 13) |
| the `lookup` and `verify` helpers | `HELPER_AGENTS`, `crates/rigger-driver/src/hooks.rs` | neutral | stay |
| the built-in layer and the reviewer texts | binary, 121's registry | neutral (`setup.rs:4511`) | stay; the working discipline's table check changes (criterion 3) |
| this repository's `.rigger/agents/*.md` | repository | `rust-engineer.md` language-specific, the rest neutral | stay: this repository is the consumer that owns them, and none ships in the binary |

BUILD ENVIRONMENT:

| Item | Where | Kind | Goes to |
| --- | --- | --- | --- |
| `CARGO_TARGET_DIR` on every gate | `gate.rs:619`, :652 | toolchain | removed; `build.env` (criterion 4) |
| `CARGO_TARGET_DIR` on every spawn | `spawn_env`, `conductor.rs:9527-9541`; `build_cache_var` :885 | toolchain | removed; `build.env` (criterion 4) |
| `spawn::BUILD_CACHE_ENV` | `spawn.rs:833` | toolchain | removed (criterion 4) |
| the request's cache read from the variable | `spawn_request`, `crates/rigger-domain/src/agent.rs:265-270` | toolchain | reads the spawn's site cache (criterion 4) |
| the reviewer's cache predicate | `reviewer_carries_build_cache`, `conductor.rs:894` | toolchain | the site cache being non-empty (criterion 4) |
| `build.wrapper`, `build.cache_dir`, `KNOWN_WRAPPERS` and their resolution | `crates/rigger-domain/src/config.rs:1135-1167`, `gate.rs:207-440` | toolchain | retired (criterion 4) |
| `build.jobs` exported as the job count | `BuildEnv::resolve`, `gate.rs:144` | toolchain | the key stays; it reaches a build only as `{jobs}` (criterion 4) |
| the scratch-root config file | `worktree.rs:1848-1871`, called at :212 | toolchain | removed (criterion 4) |
| the scaffold's `wrapper: auto` | `setup.rs:1816-1821` | toolchain | removed (criterion 4); the set's `env` (criterion 6), with the walk row "set env / DROPPED" |
| `rigger validate`'s wrapper and cache lines | `build_environment_report`, `src/cli/validate.rs:373-394` | toolchain | `build env:` lines (criterion 4) |
| operator text naming `CARGO_TARGET_DIR` | `FOOTPRINT_RECLAIM_HINT_UNOWNED_AGENT_SCRATCH`, `src/cli/mod.rs:4474-4478` | toolchain | "your build cache variable" (criterion 4) |
| CI's wrapper install step | `.github/workflows/rust.yml:105-110` | serves the wrapper criterion 4 retires | removed (criterion 4) |
| the pre-commit hook's binary search through `CARGO_TARGET_DIR` | `src/cli/mod.rs:5293-5470` | installed in every project; it only reads candidate locations of rigger's own binary, built by rigger's own toolchain, and exports nothing to a consumer's build | OUT OF SCOPE |

CODE FACTS:

| Item | Where | Kind | Goes to |
| --- | --- | --- | --- |
| the six bundled grammars | `crates/rigger-grounder/src/grounder/symbols/registry.rs` | language-specific | stay, the overridable default (criterion 7) |
| the code store's extractor and its fingerprint | `FactsExtractor`, `facts.rs:97-143`; `store_fingerprint` :46 | six languages | `ProjectExtractor` beside it, its fingerprint folding the declaration (criterion 7) |
| the store's openers | `open_store` :53, `read_head` :61, `DirectoryHead` :77-93, `CodeStore::in_force` (`src/code_store.rs:54`) | - | read through `ProjectExtractor` (criterion 7) |
| the facts read-off | `code_facts` :145, `def_indices_by_path` :196 | - | take definitions, references and the partial flag, so a declared answer feeds them (criterion 8) |
| the show surface's body extent | `locate_definition_extent`, `src/cli/graph.rs:587-609` | - | a declared extension bounds through its extractor (criterion 8) |

OS FACILITIES (production code; test modules OUT OF SCOPE):

| Item | Where | Gated | Interface |
| --- | --- | --- | --- |
| the process-module import and the signal call | `reap.rs:90`, `send_signal` :418-429 | no | per-OS arms in `reap.rs` (criterion 9) |
| the process scans and readers: pids, start time, parent, cwd, command | `reap.rs:121-303`, `signal_if_unchanged` :373-380 | no (runtime `/proc`) | the process interface (criterion 9) |
| the holder check: open files, the unexeced-fork flag | `holders.rs:21-83` | no (runtime `/proc`) | the process interface's `holders` (criterion 9) |
| the dash's port holder and process state | `dash.rs:542-602` | no (runtime `/proc`) | the process interface (criterion 9) |
| the permissions import | `setup.rs:592` | no | under its own cfg block (criterion 10) |
| `sh` and the external lock command | `gate.rs:607-613` | no (runtime) | the shell and the lock (criteria 8, 10) |
| executable lookup | `find_executable`, `gate.rs:260-269`; `is_executable_file` :270-281 | mode check gated | into `subprocess`, Windows extensions (criterion 10) |
| by-name spawn of a tool | `subprocess::command`, `crates/rigger-process/src/subprocess.rs:16-18` | no | resolved through `find_executable` on Windows (criterion 10) |
| the state and cache homes | `crates/rigger-store-sqlite/src/registry.rs:189`, `worktree.rs:2787`, :2886, `src/cli/hygiene.rs:382` (`gate.rs:208` goes with criterion 4) | no (runtime `HOME`) | `rigger_process::home_dir` (criterion 11) |
| the gate request channel | `gate_socket.rs`, `cfg(unix)` with `Unsupported` stubs at :357-388 | yes | a Windows named-pipe adapter (criterion 14) |
| keep-awake | `keep_awake.rs`; Linux takes logind's inhibitor alone | yes | the model; the Linux adapter adds the desktop portal's inhibitor (criterion 15) |
| file trust, directory sync, the store's mapped snapshot, the format lock | `crates/rigger-process/src/file_trust.rs`, `sync_dir`, `published.rs`, `format.rs` | yes, or portable | stay: each has an arm or a portable call on every OS |
| the connection-file mode check | `src/cli/mod.rs:576-612` | yes | stays: permission bits have no Windows meaning |
| the POSIX pre-commit hook body | `src/cli/mod.rs:5293-5470` | n/a | stays: git runs hooks through its own shell on every OS |

**THE TEMPLATE ROOT, decided here (criterion 1).** `rigger init` takes `--template <dir>`, and
`rigger setup` takes `--template <dir>` beside `--agents <dir>`; `cmd_init` (`setup.rs:531`) takes
the argument list `src/main.rs:156` hands it. A template root is a directory holding `scaffold/`,
one subdirectory per gate template set in today's layout: `set.yml` and `files`, a list file of
`<root>`-relative paths, each read from `<root>/<rel>` (one with no file there refused as "{rel}
names no file under <root>") and written at `<rel>` in the project.
`load_template_sets(root: &Path) -> Result<Vec<TemplateSet>, String>` reads it at run time with
the checks `build/template_sets.rs` makes at build time and the same refusal texts, the
missing-file refusal alone gaining " under <root>" and every other text only `scaffold/`
written as `<root>/scaffold/` (for example "<root>/scaffold/ holds no gate template set
directory" and "gate template set {key}: missing <root>/scaffold/{key}/{required}"), then
`parse_template_set`'s checks (`setup.rs:1909`), all before `init_project` (`setup.rs:137`)
writes anything; among them, a file the root supplies that is not UTF-8 text is refused as "{rel}
is not UTF-8 text", `{rel}` its `<root>`-relative path. `TemplateSet` (`setup.rs:1877-1881`)
holds owned strings, and `init_project` takes the loaded sets in place of `TEMPLATE_SETS`.
`build.rs`'s template generation and its `scaffold/` rerun line go (the version derivation and
its watch stay), as do `build/template_sets.rs`, the `include!` and the `all` module
`tests/template_sets_build.rs` (`tests/all.rs:258`), whose cases move to `setup.rs`'s tests over
`load_template_sets`. With no `--template`, `init_project` is handed no set: a workflow it writes
declares no gates, and its summary prints, in place of today's no-set line (`setup.rs:489-493`),
"no template root was given (--template <dir>), so .rigger/workflow.yml declares no gates -
declare your own under gates:"; a root none of whose sets matches keeps today's no-set line.
`rigger setup` without `--template` likewise writes no set file, so an existing project's workflow
and gate files stay as they are. One parser, `parse_setup_args` (`setup.rs:1542`), serves both
commands and names its command in each refusal ("init: --template needs a directory argument",
"init: unknown argument {other:?}"); `--agents` stays `setup`'s alone, a repeated flag takes its
last value, as `--agents` does today, and a relative `--template` resolves against the working
directory the command runs in.

This repository's `scaffold/` stays where it is: the Rust set is `scaffold/rust/` plus
`.rigger/gates/red-before-green.sh`, the one script its `files` lists. `mutation.sh` and
`container-env.sh` leave the list and stay this repository's gates, and `scaffold/rust/set.yml`'s
comment block naming `mutation.sh` goes with them. Its tests name the repository root
(`env!("CARGO_MANIFEST_DIR")`) as the template root: every `TEMPLATE_SETS` use in `setup.rs`'s
tests goes through one helper over `load_template_sets`, and every test that runs `rigger init` or
`rigger setup` over a project holding `Cargo.toml` and asserts on the Rust set's gates, files or
implementer passes `--template` with that root (`tests/init_template_sets_periphery.rs`, whose
`mutation.sh` and `container-env.sh` pins at :32-33 and :206-210 go, and
`tests/review_steps_periphery.rs:1506-1515` among them); every test asserting today's no-set line
(`assert_no_set_scaffold`'s three callers, `tests/init_template_sets_periphery.rs:164`, :281 and
:298, among them) passes `--template` with the repository root, and only criterion 1's new
no-root test runs with no root; criterion 1 owns those edits.
`set_implementers()` (`setup.rs:4840`) reads the sets through that one helper, so
`every_shipped_persona` (:4824) and `every_shipped_agent_text` (:4434) keep the Rust set's texts
under the hygiene tests; criterion 1's marker test selects its texts by label, as the built-in
layer's test does (`setup.rs:4511`).

**THE SCAFFOLDED WORKFLOW, decided here (criteria 1, 2, 4, 5, 6).** `SCAFFOLD_WORKFLOW` is one
surface; each change has one owner:

- the head comment's set sentence (`setup.rs:1782-1783`) becomes "Its gates come from the gate
  template set matching this project's language in the template root `rigger init --template
  <dir>` names; with none, it declares no gates." (criterion 1);
- the wrapper block (`setup.rs:1816-1821`: the comment naming the two wrappers, then
  `build:\n  wrapper: auto`) goes (criterion 4), and in its place, between the `defaults:` block and
  the gate library's comment, criterion 6 writes
  `# The variables rigger exports to every gate and spawn, each a template over\n# {build_cache}
  (the site's build cache) and {jobs} (build.jobs); none by default.\nbuild:\n  env: @BUILD_ENV@`;
- `test_author: sdet-author` goes under `defaults:` after `max_unit_added_lines`, with the comment
  `# The agent that writes each unit's periphery tests after its own tests pass.` (criterion 2);
- `max_wall_clock: 3600` goes under `defaults:` after `max_retries`, with the comment `# The
  per-spawn wall-clock bound in seconds: a spawn silent past it with no result is stopped and\n#
  recorded as an infrastructure fault, charging no attempt. 0 = unbounded.` (criterion 5).

**THE TEST AUTHOR SEED, decided here (criterion 2).** `SCAFFOLD_AGENTS` gains `sdet-author.md`,
after `sdet.md`: frontmatter `id: sdet-author`, `model: opus`, `tools: [Read, Edit, Write, Grep,
Glob, Bash, Agent]`, `isolation: worktree`, `recurse: true`; body "You author the periphery tests
of ONE unit inside its worktree, after the implementer's tests pass and before the gates run:
contract, API and integration tests that drive the unit from outside. You never author or change
a unit test. Find the unit's boundary surface from its diff, write one test per behaviour at that
surface.", then `## A documentation-only round` and "Extend the existing owner test's needle for
the changed sentence, or add no pin; never a new test.". `get_referenced_agent_ids`
(`setup.rs:432`) adds a non-empty `defaults.test_author`, so `write_seed_agents`' referenced-ids
filter (`setup.rs:278`) writes the seed for every project, whatever its template root, through
`write_if_absent` (`setup.rs:1730`), which keeps a project's own `sdet-author.md`. The seed-set
test names `sdet-author`; each agent count the `rigger init` validate pins carry
(`tests/review_steps_periphery.rs:1511-1512`, `tests/init_template_sets_periphery.rs:177` and
:222) grows from 6 to 7; and `tests/init_template_sets_periphery.rs`' `NO_SET_AGENTS_LINE`
(:325-326), `RUST_AGENTS_LINE` (:330-331), `tests/common/fixtures/config.rs`' `ROLE_SEED_FILES`
(:19) and `agents_beside` (:29) gain `sdet-author.md`; `scaffold_parses_into_a_valid_config`'s count
and comment (`setup.rs:2643-2646`) become 7, naming the sdet-author;
`scaffold_agents_and_workflow_reference_the_same_canonical_set` (:3520-3583) adds a non-empty
`defaults.test_author` to its `referenced_by`, as `get_referenced_agent_ids` does, and its
`canonical` set and doc gain `sdet-author`; and
`tests/init_setup_unknown_key_agent_fleet_periphery.rs`' `DEFAULT_FLEET` (:29-36) gains
`sdet-author.md`, its doc at :83 saying seven. The seed is a consumer surface rigger
writes once, never a registry entry.

**THE IMPLEMENTER'S SENTENCE, decided here (criterion 3).** In the `implementer.md` seed, "confirm
GREEN, run the named gates, commit." becomes "confirm GREEN, run the checks your change needs,
commit. The workflow's gates run the full battery on the commit.", the sentence the Rust set's
`implementer:` already carries less its tool list. The seed is otherwise unchanged, and
`tests/common/fixtures/config.rs`' `LANGUAGE_NEUTRAL_IMPLEMENTER` (:11-16) changes with it. This
repository's `.rigger/agents/rust-engineer.md` reads "Commit when the checks your change needs
pass; the gates run on your commit." in place of "Commit when the gates pass." (:40-41). The
built-in working discipline's table bullet check
(`crates/rigger-domain/src/instructions/working-discipline.md:60-61`) becomes "Check: each shape
you add was looked up where the project's test helpers live, and each behaviour your tests check
over several values reads as one table.", so no hand-over check asks an author to run a gate;
`instructions.rs`' hand-over test pins the new phrase.

**THE SCAFFOLD WALL CLOCK, decided here (criterion 5).** The scaffold's `max_wall_clock: 3600`
(THE SCAFFOLDED WORKFLOW) makes `unbounded_wall_clock_advisory`
(`crates/rigger-config-files/src/config.rs:108`) answer none over a freshly scaffolded tree, with
a template root and without, so `rigger validate` on a new project prints no wall-clock warning.
An existing workflow keeps what it declares.

**THE DECLARED BUILD ENVIRONMENT, decided here (criterion 4).** `BuildConfig`
(`crates/rigger-domain/src/config.rs:1135`) gains `env: BTreeMap<String, String>`, defaulted
empty: the variables rigger exports to every gate and every spawn, nothing else. A name matches
`[A-Za-z_][A-Za-z0-9_]*` and is not one rigger sets itself on a gate or a spawn: `STORE_FENCE_ENV`
and `GATE_SCRATCH_ENV` (`gate.rs:539`, :543), `RIGGER_RUN_BASE` (`conductor.rs:1013`, :10095),
the spawn marker `SPAWN_ENV` (`crates/rigger-driver/src/driver/mod.rs:88`), `GATE_SOCKET_ENV`
(`crates/rigger-domain/src/gate_channel.rs:15`), the store connection name `CONN_ENV`
(`KURRENTDB_CONN`, `src/cli/mod.rs:529-530`) and the two agent credential names
(`crates/rigger-driver/src/claude_auth.rs:17`, :21). `SPAWN_ENV` moves to `rigger_domain::spawn`,
re-exported from `driver` for its callers, beside a new `spawn::RUN_BASE_ENV` (`RIGGER_RUN_BASE`)
both conductor sites read, `spawn::CONN_ENV`, re-exported for `src/cli`, and the two credential
names `claude_auth` then reads, so
`BuildEnv::declared` reads every reserved name with no new crate edge. The second pair of the
spawn host's `harness_env` (`driver/mod.rs:127`), the print-mode wait ceiling, stays overridable
by a declaration, as by any spawn environment today. Beyond the reserved names, a gate's own
`with_var` pairs, `harness_env`'s pairs and `handed_env`'s store pair (`src/cli/mod.rs:371-376`), rigger neither sets nor clears a variable `build.env`
does not declare: one the conductor inherits reaches every gate and spawn unchanged.

A value is a template: literal text, `{build_cache}` (the site's build cache), `{jobs}`
(`build.jobs` in decimal when above 0), and `{{` and `}}` for a literal brace; any other `{`, and
a `}` that closes nothing, is refused naming the variable. At a site a variable is exported only
when every placeholder in it resolves non-empty there; a template with no placeholder is exported
as written at every site. `BuildEnv::declared(&BuildConfig) -> Result<BuildEnv, String>` is the
one check and holds the declaration; `Config::validate`
(`crates/rigger-config-files/src/config_store.rs:288`, its build check :299) and `build_env_of`
(`conductor.rs:859-866`) call it in place of `resolve_build_layer` and `BuildEnv::resolve`.
`BuildEnv::at(&self, cache: &str) -> SiteEnv { vars, cache }` resolves the declaration at one
site, with the `with_var` pairs (`gate.rs:177`) appended, its `cache` the given cache when an
exported variable uses `{build_cache}` and empty otherwise; `SiteEnv::apply` sets the vars on a
command. The site's cache is the one `spawn_env`'s `cache` argument picks today:

- a spawn: `worktree::unit_sibling(dir, cache)` (the unit's cache for a spawn building the unit's
  tree, its review cache at a review tier), empty when `dir` has no unit sibling;
- a gate: `ExecRunner::run`'s `target_dir` when set, else the shared gate cache
  (`build_cache_dir`); both `BUILD_CACHE_ENV` sites go and the fence logic stays, and a gate with
  no `target_dir` holds the shared lock whenever its guard is set, as today, whatever its
  `SiteEnv.cache`, since a variable it inherits may point its build into the shared gate cache.

`spawn_env` returns the `SiteEnv`, the gate socket pair appended as today, and `build_cache_var`
goes; `SpawnOpts` (`agent.rs:107`) gains `build_cache: String`, set from it at each `spawn_env`
caller (`conductor.rs:5494`, :6831, :7518, :9096, :11767), and `spawn_request` (`agent.rs:243`)
fills the request's `cargo_target_dir` from it in place of the variable lookup.
`reviewer_carries_build_cache` takes the run's `BuildEnv` and answers whether the site cache at
the review sibling is non-empty, at each caller (`conductor.rs:8933`, :9311, :12076,
`crates/rigger-conductor/src/canary_store.rs:559`), so a reviewer's prompt names a build cache
exactly when its environment carries one. `spawn::BUILD_CACHE_ENV` goes; the pre-commit hook's
template (`src/cli/mod.rs:5469`) writes `CARGO_TARGET_DIR` itself. `rigger validate` prints one
`build env: NAME=template` line per variable in name order, or `build env: none declared`, in
place of `build_environment_report`'s wrapper and cache lines; its budget line stays.

**THE RETIRED WRAPPER, decided here (criterion 4).** `build.wrapper` and `build.cache_dir` stay
fields so a workflow carrying them parses, and `Config::validate` refuses either when non-empty
through `reject_retired_key` (`config_store.rs:43`), naming its line, with the why "rigger exports
only what `build.env` declares; declare there each variable your builds need (your build cache
over `{build_cache}`, your job count over `{jobs}`)". Removed: `BuildEnv::resolve`,
`default_cache_dir`, `resolved_cache_dir`, `KNOWN_WRAPPERS`, `WrapperUnavailable`,
`resolve_wrapper_name_from`, `CacheDirUnusable`, `BuildLayerUnavailable`, `resolve_build_layer`
and `resolve_build_layer_from` with their helpers (`gate.rs:207-440`); `SCRATCH_CARGO_CONFIG`,
`ensure_scratch_root_cargo_config` and its call (`worktree.rs:1848-1871`, :212); the scaffold's
wrapper block; CI's wrapper install step (`rust.yml:105-110`); and `rigger-gates-shell`'s
`rigger-store-sqlite` dependency, with its comment, its feature forwarding and the `registry`
import (`crates/rigger-gates-shell/Cargo.toml:17-18`, :24-25, `src/lib.rs:15-16`), which
`default_cache_dir` alone read. `find_executable` and `is_executable_file` stay, moved into `subprocess` by criterion 10.
The scaffold pin `scaffold_workflow_declares_build_wrapper_auto` (`setup.rs:4034`) becomes: every
rendering's `build.wrapper` and `build.cache_dir` are empty, and `tests/cli.rs`' `rigger validate`
build.wrapper section (:12663) is rewritten to the `build env:` lines. The operator text
`FOOTPRINT_RECLAIM_HINT_UNOWNED_AGENT_SCRATCH` (`src/cli/mod.rs:4474-4478`) reads "your build cache
variable" where it names `CARGO_TARGET_DIR`, `.cargo/config.toml`'s comment (:16) reads "An
environment RUSTC_WRAPPER overrides this entry.", and the handbook's upgrade notes
(`authoring-loops.md:115`) gain "The `build.wrapper` and `build.cache_dir` keys are retired, and
`rigger validate` refuses a workflow.yml that still carries either - delete the line and declare
under `build.env` the variables your builds need."

**THE SET'S ENVIRONMENT, decided here (criterion 6).** `SetSpec` (`setup.rs:1893`) gains
`env: BTreeMap<String, String>`, defaulted empty, which `parse_template_set` checks through
`BuildEnv::declared`, refusing "gate template set {key}: env: {error}". `scaffold_workflow`
(`setup.rs:1951`) fills `@BUILD_ENV@` with the matched set's `env` as a YAML flow map in name
order, each key a plain YAML scalar (a name YAML reads as null or a boolean, `null`, `true` or `false` in
lower, Capitalized or UPPER case, double-quoted) and each value a double-quoted YAML scalar escaping
`"`, `\` and every character below U+0020
or in U+007F-U+009F as `\uXXXX` (a form YAML reads back as the same value), and `{}` with no set.
`scaffold/rust/set.yml` declares `env: {CARGO_BUILD_JOBS: "{jobs}", CARGO_TARGET_DIR:
"{build_cache}"}`, so a scaffolded Rust project keeps its per-unit caches and job count.

**THE LANGUAGE COVERAGE PARAGRAPH, decided here (criteria 4, 8).** `authoring-loops.md:19` is one
paragraph with two owners. Criterion 4 replaces its first three sentences with "rigger exports no
toolchain's variable. The workflow's `build.env` declares every variable a spawn and a gate
receive, its values built from the build cache (`{build_cache}`) and the job budget (`{jobs}`); a
workflow that declares none exports nothing."; criterion 8 replaces its last sentence with "The
code store extracts Rust, C#, Go, JavaScript, TypeScript and Python with its bundled grammars,
and the workflow's `extractors:` names a command per language for any other, or to replace a
bundled one; an extractor answers each request line, flushed, before reading the next, with
1-based lines, and exits when its input ends; a changed extractor is announced by changing its
declaration, which re-extracts the tree at the store's next advance. A request is one JSON line
`{"path", "text"}`; an answer is one JSON line `{"path", "defs", "refs"}` or `{"path",
"error"}`, its `path` the request's, a def carrying `kind` (`Function`, `Method`, `Type`,
`Trait`, `Impl`, `Module`, `Constant`, `Constructor`, `Macro`, `Declaration`, `Static`, `Variant`,
`Field`, `Variable`, `Property` or `Other`), `name`, `line`, `end_line` and `path`, its id path in
the file (`T::new`, say), and a ref `name` and `line`, every `name` and `path` non-empty; a def
may carry `parent`, its container's path, and `is_test`, marking test code; a ref may carry
`enclosing`, the path of the def it sits in, which feeds its callers, `is_test`, and `ref_kind`
(`const`, `type` or `use`, absent for a call); an answer may carry `partial`, marking an
extraction that did not cover the whole file."

**THE EXTRACTORS KEY, decided here (criterion 7).** `Workflow`
(`crates/rigger-domain/src/config.rs:1194`) gains `extractors: BTreeMap<String, ExtractorDecl>`,
defaulted empty, `ExtractorDecl { extensions: Vec<String>, run: String }` refusing unknown fields.
One check, `config::check_extractors`, refuses, naming `extractors.<name>`: a name outside
`[a-z0-9_-]+`, an empty `run`, an empty `extensions`, an extension that is empty or holds `.` or
`/`, and an extension two extractors both list; `Config::validate` calls it. The key and the check
compile in every lane; the code store exists only under `symbols`
(`crates/rigger-grounder/src/lib.rs:9-10`), so a lane without it parses and validates the key and
runs nothing.

`ProjectExtractor` (`facts.rs`, beside `FactsExtractor`, which stays the bundled-only unit
extractor its callers and tests name) implements `Extract` (`crates/rigger-domain/src/code_view.rs:278`).
`ProjectExtractor::of(root)` holds the map `declared_extractors(root)` answers and the sessions
criterion 8 adds. `declared_extractors(root) -> BTreeMap<String, ExtractorDecl>` is the code
store's one read of the map, through `load_workflow` and `check_extractors`: an absent workflow
answers the empty map, and a read or check failure answers the empty map and prints `rigger:
extractors: {error}` to stderr, so the bundled grammars proceed alone. Its fingerprint is
`fingerprint_of(GRAMMAR_TAGS_VERSION, FACTS_GENERATION)` with no declaration, `FactsExtractor`'s
own, so a store built before this spec stays in force; with any it is
`fingerprint_of(GRAMMAR_TAGS_VERSION + "+" + digest, FACTS_GENERATION)`, the digest
`store::content_hash` over the declared map's JSON in name order. `open_store`, `read_head`,
`DirectoryHead` and `store_fingerprint(root)` (`facts.rs:46-93`) compose `ProjectExtractor::of`
their root in place of `FactsExtractor`, and `CodeStore::in_force` (`src/code_store.rs:54`) passes
its root. A changed declaration changes the fingerprint, so the store's next advance extracts the
tree under it and the facts memo, keyed by fingerprint, holds nothing stale; a removed one
returns the fingerprint to the bundled one, as a grammar change does today. A file whose extension
no declaration lists extracts exactly as `FactsExtractor` extracts it.

**THE EXTRACTOR PROTOCOL, decided here (criterion 8).** The workflow definition file keeps
`Half::Workflow` and is classified before any declaration is consulted, as `FactsExtractor` does
today (`crates/rigger-grounder/src/facts.rs:107-140`); a declaration's extensions never reach it.
For a file whose extension a declaration
lists, `ProjectExtractor::extract` gives `Half::Code` with `lang` the declared name, and its
definitions, mentions and partial flag from the declared extractor's answer, the design facts
over its text as for any code file; a declaration named like a bundled `lang_str` (`rust`,
`csharp`, `js`, `ts`, `go`, `python`) replaces that grammar for the extensions it lists. Its
scope is `is_dev_only`'s, as today. `code_facts` and `def_indices_by_path` (`facts.rs:145`, :196)
take a file's definitions, references and partial flag in place of `&FileSymbols`, so the bundled
and the declared answer feed one read-off; a declared answer adds no `Lang` variant.

`ProjectExtractor` holds one session per extractor behind one lock, started at the first file of
its extensions: `run` through `subprocess::shell_command(run, root)`, stdin and stdout piped,
stderr inherited; `rigger-grounder` gains a workspace edge to `rigger-process`, declared
`default-features = false` with its `store` and `core` features forwarded to `rigger-process`'s,
as THE HOME declares `rigger-store-sqlite`'s. `subprocess::shell_command(run, dir) -> Command` is `sh -c <run>` on unix and
`cmd` with `cmd_argument(run)` as one raw argument on Windows, in `dir`; `cmd_argument(run) ->
String` (`/D /S /C "` + `run` + `"`) is host-independent and tested on every host. Each request is
one JSON line `{"path": <root-relative path, `/`-separated on every OS>, "text": <file text>}`;
each answer is one JSON line, either `{"path", "defs", "refs", "partial"?}`, its `defs` and
`refs` in the bundled extraction's `Def` and `SymRef` derived JSON forms
(`crates/rigger-grounder/src/grounder/symbols/model.rs`), or `{"path", "error"}`. Every failure
(an `error` answer, an answer that does not parse, an answer whose `path` is not the request's,
a def or ref whose `line` is 0 or past the request text's last line (one more than the number of
`\n` the text holds), a def whose `end_line` is below its `line` or past that last line, a cycle
among the answer's def paths through `parent` (a def whose `parent` is its own `path` included),
a def or ref with an empty `name`, a def with an empty `path`, a ref whose `ref_kind` is present
and is not `const`, `type` or `use`, a def whose `is_out_of_line_module` is true or that carries
a `path_override` or an `enclosing_inline_module_path`, end of output, a failed write) gives the
file its identity facts and no lists, as a failed bundled extraction does today; rigger prints
`rigger: extractor {name}: {path}: {why}` to stderr, and the session is closed, the next file of
its extensions starting a new one. The failed file returns when its content or the declaration
changes. A session closes by closing its stdin and its stdout, then waiting for the child, so a
child still writing fails its next write and ends; dropping `ProjectExtractor` closes every
session. rigger sets no deadline on an answer: the extractor is the consumer's own command,
framed as the handbook sentence criterion 8 owns states. The lock serves a session one file at a
time across the threads of one advance; two processes run their own sessions.

`locate_definition_extent` (`src/cli/graph.rs:587`, and its light-lane twin :617) gains the
project root: a source whose extension a declaration lists is extracted through a
`ProjectExtractor` session and bounded by `extract::extent_in(defs, Recorded) -> Option<u32>`, the
match split out of `locate_extent` (`crates/rigger-grounder/src/grounder/symbols/extract.rs`),
which `locate_extent` calls after its own extraction.

**THE PROCESS INTERFACE, decided here (criterion 9).** `crates/rigger-process/src/os_process.rs`,
with `os_process/{linux,macos,windows}.rs` bound by one `std::cfg_select!` and a `_` arm answering
nothing, under the feature gate `reap.rs` carries (`lib.rs:21-22`), re-exported beside `reap`. Its
functions, each answering `None` or empty for a process it cannot read:

- `pids() -> Vec<u32>`, `parent(pid) -> Option<u32>`, `start_time(pid) -> Option<u64>` (an opaque
  value equal only for the same process), `cwd(pid) -> Option<PathBuf>`, `command(pid) -> String`,
  `state(pid) -> Option<char>`, `open_files(pid) -> Vec<PathBuf>`,
  `is_unexeced_fork_of(pid, parent) -> bool`, `listening_pid(port) -> Option<u32>`,
  `holders(dir: &Path) -> Vec<u32>`.

Only the raw reads move to `os_process/linux.rs`, unchanged: the `/proc` listing, the cwd link
with `strip_deleted_suffix` (`reap.rs:719`), `read_command` (:168), `read_ppid` (:242),
`pid_starttime`'s stat read with `stat_field_after_comm` (:214, :225), `holders.rs`'
`/proc/<pid>/fd` read in `open_files` (:62) and `proc_of` (:83), and `dash.rs`' `tcp_listen_inode_for_port`
(:542) and `process_state`'s read (:602). The compositions `processes_rooted_under` (:121),
`scan_with_starttime` (:201), `still_started` (:236), `ancestor_pids` (:257) and `descendants_of`
(:302) stay in `reap.rs`, each once, on every OS, composed over `os_process::{pids, parent,
start_time, cwd, command}`, so their callers (`reap.rs`, `src/cli/validate.rs:757`) compile on
every OS; `stat_field_after_comm` moves into `os_process/linux.rs`, and `reap` re-exports it under
`target_os = "linux"` for its Linux-only test callers (`tests/cli.rs:25905`, `tests/common/mod.rs:487`,
`crates/rigger-process/tests/keep_awake_threads.rs:33`); `reap::pid_starttime` stays as `reap`'s
name over `os_process::start_time`. The Linux `os_process::cwd` answers the link with the
` (deleted)` suffix already stripped, and `is_inside` (`reap.rs:710`) becomes `cwd.starts_with(base)`
on every OS; the deleted-suffix case is tested on the Linux `cwd` read. `holders.rs` keeps `pub use
os_process::open_files`, now the interface's `Vec`, which its two fixture callers
(`tests/common/fixtures/host.rs:197`, `tests/cli.rs:5134`) iterate through `into_iter`, and keeps
`pub fn own_open_files`, composed once over `os_process::{is_unexeced_fork_of, open_files}` on
every OS; `holders.rs`' tests of `holds` (:52) and `is_unexeced_fork_of` (:41) move into
`os_process.rs`' test module beside the scan. Linux and macOS answer `holders` by today's `processes_holding` scan
(`holders.rs:21`: a cwd or an open file inside, this process and an unexeced fork of it skipped),
which lives once in `os_process.rs`, generic over the per-OS readers, and both Unix arms call it.
macOS reads the process-information calls of its system library, declared as externs: the pid
list, the BSD info (parent, start time, run state: `R` running or idle, `S` sleeping, `T`
stopped, `Z` zombie), the vnode path info (cwd), the executable path, the descriptor list with
each vnode's path (open files) and each socket's TCP listen port (`listening_pid`);
`is_unexeced_fork_of(pid, parent)` answers true for a process whose BSD info parent is `parent`
and whose BSD info flags lack the exec flag (`PROC_FLAG_EXEC`, set once it has called exec).
Windows reads the process snapshot (pids, and a parent trusted only when it is alive and started
no later than the child), the process times (start time), the full image name (command), the
owner-pid TCP listener table (`listening_pid`) and the Restart Manager (`holders`: one session
registers every regular file under the directory, in calls of at most 256 files, and answers the
pids its list reports, this process excluded; a failed start, registration or list call answers
no holder); it documents no call that reads another process's working directory, state or open
files, so `cwd` and `state` answer `None` and `open_files` empty, and `is_unexeced_fork_of`
answers false (no fork). The Windows bindings dependency gains the tool-help, IP-helper, socket
and Restart Manager features those calls need. `holders::processes_holding(dir)` canonicalizes
`dir` and answers `os_process::holders`. `send_signal` (`reap.rs:418-429`) stays the one signal
call, in `reap.rs`, over a reap-local `Signal { Term, Hard }`: its unix arm is today's call, its
Windows arm opens the process for termination, terminates it and closes the handle, and its `_`
arm does nothing; the `pid <= 1` guard stays, and the `reap.rs:90` import moves into the unix arm.
`signal_if_unchanged` (`reap.rs:373`) reads `os_process::cwd`, and a `None` cwd fails its
recheck, so on Windows only a pid-tree descendant is ever signalled. `reap.rs`, `holders.rs` and
`dash.rs` (`process_state` reads `state`, `pid_holding_port` :572 reads `listening_pid`) read the
interface. Criterion 9 owns any re-pin `tests/no_os_kill_audit.rs` (`tests/all.rs:167`) needs for
the moved call, and adds to that test and to `.rigger/gates/content.sh`'s `no-os-kill` check
(:62-68) two shapes: the Windows process-termination call outside `reap.rs`, and a
`Command::new` shell-out to the Windows process-termination utility anywhere. This interface and
criterion 12's job close the check-in findings `sdet-u105c8macwin-windows-crate-unbuildable` and
`sdet-u105c8macwin-lesson-reaper-not-cross-platform`.

**THE SHELL AND THE LOCK, decided here (criterion 10).**

- `ExecRunner::run` (`gate.rs:548`) builds every gate through `subprocess::shell_command`
  (criterion 8) in place of `command_in("sh", dir)` and the `flock` wrapper (`gate.rs:607-613`).
- `gate::hold_shared_lock(guard) -> Result<File, ProbeFault>` opens the guard through
  `open_build_cache_guard` (`gate.rs:714`) and takes the existing file-lock dependency's shared
  lock, folding `probe_build_cache_guard` (:739) with its message unchanged.
  `subprocess::inherit_on_exec(&mut Command, &File)` clears the descriptor's close-on-exec flag in
  the child before exec on unix, through the system-call dependency's descriptor-flag call (its
  features may grow), and does nothing elsewhere. A guarded gate takes the lock after
  `budget.acquire()`, immediately before `cmd.output()` (a `ProbeFault` returns before the
  command runs, releasing the slot), holds it across `cmd.output()` and then closes its handle,
  never unlocking, so on unix the lock lives while any process the gate started holds the
  inherited descriptor, as the external lock command's does today; on Windows it lives as long as
  the gate process, so a child the gate leaves running is outside the exclusion, and the holder
  check a reclaim runs before any removal (`reclaim_dead_footprint`, `src/cli/mod.rs:4892`) leaves
  in place an entry that child holds a file in. The external lock command leaves production.
- `find_executable` and `is_executable_file` move from `gate.rs` into `subprocess`, where
  `resolve_requirements` keeps calling them; on Windows `find_executable` tries a name with no
  extension only with each `PATHEXT` extension, in `PATHEXT` order, never bare, and a name
  carrying an extension as given, and reads a backslash as a path separator. On Windows
  `subprocess::command`, where every by-name spawn starts (the agent CLI, git), spawns a program
  name holding no path separator at the path `find_executable` resolves over `PATH`, and the name
  as given when that resolves nothing; elsewhere it stays `Command::new`. The Windows rule is the
  host-independent `pathext_candidates(name, pathext) -> Vec<String>` (the name with each
  extension, in order, for a name with none, else the name alone) the cfg arms call, tested on
  every host.
- `setup.rs:592`'s import moves into the cfg block that uses it (:610).
- `authoring-loops.md:111`'s paragraph opens with "rigger runs a gate's and an extractor's `run`
  with `sh -c` on Linux and macOS and with `cmd /D /S /C` on Windows, so a gate written for `sh`
  needs an `sh` on `PATH` there."

**THE HOME, decided here (criterion 11).** `rigger_process::home_dir() -> Option<OsString>`, in
`crates/rigger-process/src/home.rs`, is the one read of the home directory: `HOME` on unix,
exactly as today (an empty value is unset, as the pure cores already read it, and there is no
password-database fallback), and `std::env::home_dir()` elsewhere. The four ambient `HOME` reads
(`registry.rs:189`, `worktree.rs:2787`, :2886, `hygiene.rs:382`) call it, the XDG variables
keeping their precedence, and `rigger-store-sqlite` gains the same-ring workspace edge to
`rigger-process` that `tests/boundary_audit.rs`'s rings permit, declared `default-features =
false` with its `store` and `core` forwarding to `rigger-process`'s, as `rigger-gates-shell`'s edge
is; `home.rs` carries no feature gate; the hygiene refusal (`hygiene.rs:385-387`) reads "neither
XDG_CACHE_HOME nor a home directory is known" in place of "neither XDG_CACHE_HOME nor HOME is
set". The fifth read, `gate.rs:208`, goes with criterion 4's `default_cache_dir`.

**THE RUNTIME IGNORES, decided here (criterion 13).** `init_project`'s ignore list
(`setup.rs:229-236`) becomes one constant, `RUNTIME_IGNORES`, naming rigger's runtime files by
class: `.claude/`, `.rigger/*.db`, `.rigger/*.db-wal`, `.rigger/*.db-shm`, `.rigger/*.lock`,
`.rigger/dash.url`, `.rigger/dash.marker`, `.rigger/dash.attempt`, `.rigger/store.conn`,
`.rigger/store`, `.rigger/*.bak-*`, `.rigger/tmp/` and `.rigger/playbooks/`, each written through
`write_gitignore_entries` as today; the comment above it (:213-228) states the class each line
covers. A class pattern covers a database, lock or scratch file rigger adds later with no edit
here. `.rigger/workflow.yml`, `agents/`, `gates/`, `instructions/`, `project.id` and
`docs-overlay.yml` stay tracked.

**THE GATE PIPE, decided here (criterion 14).** `gate_socket.rs`' adapters are bound by one
`std::cfg_select!`: the unix arm is today's code unchanged, a `windows` arm serves and asks over a
named pipe, and the `_` arm keeps today's `Unsupported` stubs. The Windows pipe is
`\\.\pipe\rigger-gate-<16 hex of SHA-256 of the canonical repo root>`, named by
`pipe_name(root_bytes) -> String`, host-independent and tested on every host, with `socket_path`
answering it on Windows; the digest rule is shared with the unix arm, and `ring` moves from the
`cfg(unix)` target to every non-wasm target. The pipe is created with an owner-only security
descriptor (`D:P(A;;GA;;;OW)`) and `PIPE_REJECT_REMOTE_CLIENTS`. The first instance is created
with `FILE_FLAG_FIRST_PIPE_INSTANCE`, and its `ERROR_ACCESS_DENIED` is answered as `AddrInUse`, as
the socket refuses a live driver's path. Instances are unlimited: `serve` creates the next instance
before it hands each connected one to its own scoped thread running the shared `answer_one`
logic, one JSON request line and one reply line, as the unix arm spawns one thread per
connection, and `close` ends the serving loop within one poll as on unix. `ask` waits through
`WaitNamedPipe` on `ERROR_PIPE_BUSY` and retries, and answers `None` only on
`ERROR_FILE_NOT_FOUND`. The Windows bindings dependency gains the pipe, file and security features
those calls need. The
`run.rs:1021-1040` bind and `gate_verb.rs` ask stay as they are.

**THE KEEP-AWAKE PORTAL, decided here (criterion 15).** After logind grants its inhibitor, the
Linux adapter (`keep_awake/linux.rs`) also asks the desktop portal when a graphical session is
present: `graphical_session(wayland_display, display) -> bool`, true when either variable is
non-empty, reads `WAYLAND_DISPLAY` and `DISPLAY`. It then connects to the session bus within
the remaining `DEADLINE` and calls `org.freedesktop.portal.Inhibit.Inhibit("", 12, {"reason":
reason})` on `org.freedesktop.portal.Desktop` at `/org/freedesktop/portal/desktop` (flags 4
suspend and 8 idle). The portal's inhibitor lives while that session-bus connection stays open,
so the connection and the runtime that drives it live on one thread the guard holds. Dropping the
guard closes the session-bus connection and joins the portal thread before it returns. Criterion 15
owns `crates/rigger-process/tests/keep_awake_threads.rs`: its test compares the thread set before
the acquire with the set after the guard drops, and its module doc and `acquire_on`'s doc
(`keep_awake/linux.rs:34-41`) say the portal thread is the one thread a granted acquire leaves
standing while its guard lives. A portal that is absent, refuses or does not answer by the deadline leaves
logind's inhibitor alone in force and prints nothing; with no graphical session no session-bus
connection is made.

**THE BOUNDARY SCANS, decided here (criteria 9, 10, 11).** Each scan is a module of the `all` test
executable (`tests/all.rs`) and joins the source audits' existing readers: it reads
`tests/common/repo.rs`' `production_sources` (:397), skips the lines
`tests/common/source_audit.rs`' `cfg_test_ranges` (:144) answers and every line whose trimmed
text starts with `//`, and fails naming each hit's path and line. Criterion 9's
(`tests/os_process_boundary.rs`) finds `"/proc` outside
`crates/rigger-process/src/os_process/linux.rs`; criterion 10's (`tests/os_shell_boundary.rs`)
finds the external lock command's name as a string literal and `"sh"` as the first argument of
`command`, `command_in` or `Command::new` outside `crates/rigger-process/src/subprocess.rs`;
criterion 11's (`tests/os_home_boundary.rs`) finds `"HOME"` outside
`crates/rigger-process/src/home.rs`.

**THE CROSS-OS CHECK, decided here (criterion 12).** `.github/workflows/rust.yml` gains a job
`cross-os-check` over a matrix `os: [windows-latest, macos-latest]`, `runs-on: ${{ matrix.os }}`:
checkout with `fetch-depth: 0`, the toolchain step with the `wasm32-unknown-unknown` target (the
dash's build script builds its console for it), and a check step under `shell: bash` running
`export RUSTC_WRAPPER=` and then `cargo check --workspace` (an empty value, set in the step's own
shell so every nested cargo inherits it, overrides `.cargo/config.toml`'s Unix wrapper script).
The version derivation degrades without the version tool, so the job installs nothing else.
`tests/ci_lanes.rs`' `every_job_that_builds_dash_installs_the_console_core_wasm_target` (:616)
adds `cross-os-check` to its job list and names the three jobs in its doc (:608-615); it is the
job's one wasm-target pin. On macOS and Windows the job's one test step runs the process crate's
unit tests (`cargo test -p rigger-process --lib`) before the check; no other test runs there.
Nothing in the run compiles a macOS or Windows arm, since nothing is installed: the
PR's `cross-os-check` is their first compile. The spec is done when its PR merges, and the PR
merges only with `cross-os-check` green, the check that observes the Goal's macOS and Windows
compile. A red `cross-os-check` is fixed on the PR branch by an operator fix that runs this
repository's gate battery, every declared gate and both lanes, read against the owning criterion
before merge: criterion 9 owns a failure in `os_process`, `reap.rs`, `holders.rs` or `dash.rs`,
criterion 11 one in `home.rs` or a home read, criterion 14 one in `gate_socket.rs`, criterion 12
one in the job's own environment, and criterion 10 one in any other file, any other Unix-only
item the job finds included.

**CONSTRAINTS WALK, decided here.**

| Mechanism | Corner | Decision |
| --- | --- | --- |
| template root | empty | no `--template`: a workflow with no gates and the no-root line; a root with no `scaffold/` is refused before any write |
| template root | existing data | an existing `.rigger/workflow.yml` is left as it is, as today; a project's own persona file is kept by `write_if_absent` |
| template root | repeated | `init` twice with the same root writes nothing new the second time, as today; a later `init` with another root writes only what is absent |
| template root | DROPPED | a set or a set file removed from the root after a project was scaffolded from it, `mutation.sh` and `container-env.sh` included: rigger deletes nothing it scaffolded |
| template root | crash-resume | every check runs before the first write; a scaffold cut short is completed by the next `init` through `write_if_absent`, as today |
| test author seed | existing data | a workflow scaffolded before this spec keeps no `test_author` until its operator adds it (121's upgrade note stands) |
| test author seed | DROPPED | `test_author` removed later: the seed file stays, the consumer's, and 121's spawn spawns none |
| implementer sentence | existing data | a project's already-written `implementer.md` keeps its text; rigger never rewrites a written persona |
| scaffold wall clock | existing data | a workflow scaffolded before this spec keeps what it declares, and `rigger validate` keeps warning on it until its operator sets the key |
| build env | empty | nothing declared: no gate and no spawn gets a variable rigger did not set for itself or inherit; a build lands where the inherited environment points, or with none set in its toolchain's default place, inside the worktree for a unit, removed with the worktree |
| build env | existing data | a workflow carrying `build.wrapper` or `build.cache_dir` is refused naming the line; an earlier binary's scratch-root config file stays, an operator file from then on (OUT OF SCOPE) |
| build env | concurrent | each spawn and gate resolves its own `SiteEnv` from one immutable declaration; nothing rigger exports is shared |
| build env | reverted | as every workflow key, an edited declaration takes effect when the workflow is next loaded |
| build env | DROPPED | a variable removed: the next spawn and gate do not get it; a cache directory it pointed at keeps the reaper its path already has, and this spec changes no reaper |
| set env | empty | no set, or a set with no `env`: `env: {}` |
| set env | DROPPED | the scaffold no longer looks for a compilation-cache wrapper: a project scaffolded from the Rust set gets none, and a consumer with one installed declares its variables under `build.env` |
| extractors | empty | no declaration: the bundled grammars alone, the fingerprint unchanged, no store rebuilt |
| extractors | DROPPED | a declaration removed: the fingerprint returns to the bundled one, and the store's next advance re-extracts under it (a kept snapshot of that fingerprint advances from its own tip) |
| extractors | crash-resume | a session that dies mid-advance fails that file only; the next file restarts it, and the failed file returns on its next change or a changed declaration |
| extractors | cold start | a workflow.yml that fails to load or check, at any key, prints a `rigger: extractors:` line and extracts with the bundled grammars, so the store advanced while it stays broken holds no declared language, which returns at the first advance after the fix: accepted, since a run refuses the same file at load |
| extractors | reverted | declarations A, then B, then A: the fingerprint is A's again, and a kept snapshot of A's fingerprint advances from its own tip |
| extractors | existing data | a store built before this spec stays in force with no declaration and is re-extracted once under one |
| extractors | concurrent | one lock serves a session one file at a time across an advance's threads; two processes run their own sessions |
| extractors | worktree overlay | a reader in a worktree extracts its differing files with the root's declaration; a declaration edited in that worktree takes effect once it lands |
| process interface | empty | a process that exits between the pid list and a read answers `None` or empty, as today's readers do |
| process interface | Windows | no cwd: the cwd-rooted scan finds nothing and only a pid-tree descendant is signalled; the holder check answers the pids the Restart Manager lists as holding a file under the directory, so an entry they hold is left in place, and a process tied to it by its working directory alone is not seen: the removal leaves that entry partly removed and uncounted, and a later reclaim finishes it once that process has ended; a failed Restart Manager call answers no holder, with the same outcome |
| shell | Windows | a gate's or an extractor's `run` is read by `cmd`; a gate written for `sh`, as the Rust set's are, needs an `sh` on `PATH` |
| lock | concurrent | on unix a guarded gate's shared lock and the reaper's exclusive one keep today's exclusion |
| lock | Windows | the lock ends with the gate process; a child the gate leaves running is outside the exclusion, and the holder check leaves in place an entry it holds a file in |
| home | empty | on unix an unset or empty `HOME` and no XDG variable give today's refusals and degrades, unchanged; elsewhere no home directory gives the same |
| runtime ignores | existing data | a `.gitignore` already carrying a line is left as it is; a project whose runtime files were committed before keeps them tracked until its operator removes them, as today |
| runtime ignores | repeated | a second `init` appends nothing (the exact-line check) |
| gate pipe | concurrent | two drivers on one repo: the second bind is refused with `AddrInUse`, as on unix |
| gate pipe | concurrent | two asks to one driver: each answered on its own instance, concurrently, as on unix |
| gate pipe | Windows | another user's process cannot open the pipe (owner-only descriptor) and a remote client is rejected |
| keep-awake portal | empty | no graphical session, no session bus or no portal: logind's inhibitor alone, as today |
| keep-awake portal | DROPPED | the guard dropped: the session-bus connection closes, which ends the portal's inhibitor, and logind's descriptor closes as today |

## Notes (non-criteria)

OUT OF SCOPE: the wire field name `cargo_target_dir` and the `cargo-target*` directory names
(wire contract and rigger's own directories); the pre-commit hook's binary search
(`src/cli/mod.rs:5293-5470`: it only reads where rigger's own binary was built and exports nothing
to a consumer's build); per-OS conventional cache and state directories (the XDG layout under the
home directory serves every OS); test modules naming `std::os::unix` (they compile only on unix,
and `cargo check` builds no test target); the scratch-root config file an earlier binary wrote
(its own text makes it the operator's to tune or delete).

RENDERED AND HELP TEXT the units change (each by the unit owning the behaviour):

- criterion 1: the `init` and `setup` usage entries (`src/main.rs:351-356`) gain `--template
  <dir>`; `cmd_init`'s doc; `SCAFFOLD_WORKFLOW`'s head comment; `authoring-loops.md:17`
  ("`rigger init` scaffolds the set whose marker file sits at the project root, from the template
  root `--template <dir>` names (this repository's `scaffold/` holds the `rust` set, on
  `Cargo.toml`); a set's `files` lists paths relative to the root, each copied to the same path
  in the project") and :179 ("`rigger init --template <dir>` takes them from the gate template set
  in that root matching your language"); `README.md`'s quick start (:52-58) runs `rigger setup
  --template <dir>` with a checkout of this repository as `<dir>` and says a project named no root
  gets a workflow with no gates, and :72 reads "runs the declared gates".
- criterion 2: `authoring-loops.md:121` reads "in this repository's `.rigger/agents/sdet-author.md`
  and in the `sdet-author.md` persona `rigger init` scaffolds, under `## A documentation-only
  round`".
- criterion 3: the example implementer bodies at `docs/handbook/authoring-agents.md:16` and
  `docs/architecture.md:211` read "confirm GREEN, run the checks your change needs, commit. The
  workflow's gates run the full battery on the commit."
- criterion 4: `authoring-loops.md:19`'s first part and :115's added sentence;
  `src/cli/mod.rs:4474-4478`; `.cargo/config.toml:16`; `.cargo/nice-rustc.sh:10-11`'s last
  sentence, which reads "An environment `RUSTC_WRAPPER` overrides this config entry.";
  `tests/spawn_target_dir_periphery.rs`' module doc, its pair now the test's own; the docs of
  `BuildConfig`, `BuildEnv`, `spawn_env`, `spawn_request` and `Config::validate`'s build check,
  the comment at `crates/rigger-driver/src/driver/cli.rs:85-88` and the doc at
  `crates/rigger-process/src/budget.rs:41-43`, each naming the declared variables where it names
  a wrapper; `harness_env`'s doc (`crates/rigger-driver/src/driver/mod.rs:90-126`), which says a
  declaration overrides its second pair alone; the `Runner` trait doc (`gate.rs:13-16`), the
  `SpawnOpts.env` doc (`crates/rigger-domain/src/agent.rs:158-170`, its `BUILD_CACHE_ENV` link
  retargeted to `SpawnOpts::build_cache`), the `SpawnRequest.cargo_target_dir` doc
  (`crates/rigger-domain/src/spawn.rs:620-627`), the `ExecRunner::run` comments at `gate.rs:614-653`,
  the `reviewer_carries_build_cache` doc (`conductor.rs:890-893`), the `RunCtx::build_env` doc
  (`conductor.rs:9442-9467`) and the `BuildEnv::with_var` doc (`gate.rs:167-176`), each naming the
  site cache and `build.env` or `BuildEnv::declared` where it names the wrapper, `resolve` or
  `BUILD_CACHE_ENV`; and the `config_store.rs:2-4` module doc and the
  `crates/rigger-config-files/Cargo.toml:22` comment, each reading "checks the declared build
  environment" where it names the build-wrapper probe.
- criterion 6: `SCAFFOLD_WORKFLOW`'s doc (`setup.rs:1769-1777`) names `@BUILD_ENV@` as its fifth
  placeholder, filled from the matched set's `env` or as `{}`.
- criterion 8: `authoring-loops.md:19`'s last sentence.
- criterion 13: `scaffold_summary_lines`' gitignore line (`setup.rs:524`) reads "added .gitignore
  entries so machine-local installs and rigger's runtime files stay untracked: {list}".
- criterion 9: the docs of `holders.rs`, `reap.rs` and `dash.rs` that name `/proc` move to the
  Linux adapter; the reset-store skill's holder clause (`crates/rigger-domain/src/docs.rs:937-939`,
  rendered by the tree's own `rigger docs` into `skills/rigger-reset-store/SKILL.md:20`) reads "a
  process with an open file inside, or on Linux and macOS one whose working directory is inside".
- criterion 10: `authoring-loops.md:111`'s opening sentence (THE SHELL AND THE LOCK); the
  `reclaim_shared_build_cache` doc (`src/cli/mod.rs:4282-4289`), the test comment at
  `src/cli/mod.rs:7795` and the `probe_build_cache_guard` doc (`gate.rs:734-738`), each describing
  the in-process shared lock `hold_shared_lock` takes and the children inherit through
  `inherit_on_exec`; the comment opening `ExecRunner::run` (`gate.rs:561-601`), rewritten to state
  the lock's lifetime (on unix while any process the gate started holds the inherited descriptor,
  on Windows as long as the gate process, with the reclaim's holder check); the `src/cli/mod.rs:4323`
  reference, retargeted to `hold_shared_lock`; and the `crates/rigger-gates-shell/Cargo.toml:30`
  comment, naming the OS lock call per arm.
- criterion 12: `every_job_that_builds_dash_installs_the_console_core_wasm_target`'s doc
  (`tests/ci_lanes.rs:608-615`) names `build-test`, `install-nolock` and `cross-os-check`.
- criterion 14: `gate_socket.rs`' module doc names the named pipe as the Windows adapter.
- criterion 15: `keep_awake_threads.rs`' module doc and `acquire_on`'s doc
  (`keep_awake/linux.rs:34-41`) name the portal thread a granted acquire leaves standing while its
  guard lives.

OPERATOR HAND-OFF: after merge, with no run live, refresh the binary first (an older binary
refuses `build.env`), then add `env: {CARGO_BUILD_JOBS: "{jobs}", CARGO_TARGET_DIR:
"{build_cache}"}` under this repository's `build:` in `.rigger/workflow.yml`, then run `rigger
setup`. Merge the PR only with its `cross-os-check` green; a red one is fixed on the PR branch by
an operator fix that runs the gate battery, read against the owning criterion before merge (THE
CROSS-OS CHECK). No unit edits `.rigger/workflow.yml`.

UNIT ORDER (the needs chain): criterion 4 splits into its declaration and sites (4a) and its
retirements (4b). 4a owns `build.env`, `BuildEnv::declared` and `at`, the reserved names' moves,
both sites, `SpawnOpts.build_cache`, `reviewer_carries_build_cache`, `BUILD_CACHE_ENV`'s removal,
the switch of `Config::validate`, `build_env_of` and `rigger validate` to the declaration, and every
test pinning what those replace (`tests/build_env_authority_periphery.rs`,
`tests/spawn_target_dir_periphery.rs`, `tests/reset_build_cache_periphery.rs`' fixture,
`validate.rs`' report tests, `tests/cli.rs`' build.wrapper section); 4b owns the retire refusal,
every removal THE RETIRED WRAPPER lists, the `setup.rs:4034` pin, the tests pinning a retired
key's accepted values (`config_store.rs:2490-2504` becomes the refusal's table: `auto`, `off` and
a named wrapper refused, empty and whitespace-only passing; `setup.rs:4113-4134`'s fixture and doc
name a `build.env` section) and the operator, comment and handbook text; criterion 8 into its sessions and facts (8a) and the show surface (8b);
criterion 9 into the interface with its Linux adapter and reroutes (9a) and the macOS and Windows
adapters (9b); criteria 2 and 5 are one unit, both editing `SCAFFOLD_WORKFLOW`'s defaults and the
scaffold tests. 1, 3, 4a, 7, 9a, 12 and 15 start at once; 2+5 needs 1; 13 needs 1; 4b needs 4a; 6 needs 1 and 4b;
8a needs 7; 8b needs 8a; 9b needs 9a; 10 needs 4a and 8a; 11 needs 4b; 14 needs 9b; 16 needs all.

LANDING-ORDER SIMULATION:

- 1 before 2+5: the head comment and the `defaults:` keys change in different lines; 2+5's
  repository-root cases go through criterion 1's helper over `load_template_sets`, its no-root
  cases pass no set -> holds.
- 1 before 13: 13's test passes `init_project` criterion 1's helper over `load_template_sets`, or
  no set -> holds.
- 1 before 6: the runtime sets carry no `env` yet and `@BUILD_ENV@` does not exist; the scaffold
  still carries the wrapper block until 4b -> holds.
- 4b before 6: `BuildEnv::declared` exists for `parse_template_set`, and 4b has removed the
  scaffold's wrapper block and rewritten its pin; 6 writes the `env` block where the block was ->
  holds.
- 4a before 4b: the declaration, both sites and `rigger validate` read `build.env`, and 4a
  rewrites every test that pinned the resolver, the exported variable or the wrapper lines; the
  wrapper fields still parse, read by nothing but `BuildEnv::resolve`, which 4b removes -> holds.
- 4b and 10 in either order: 4b deletes `path_has_executable`, the wrapper probe's call of
  `find_executable`; 10 moves `find_executable` and `is_executable_file`, which
  `resolve_requirements` keeps calling, and points `path_has_executable` at the moved function
  while it remains -> holds.
- 4a before 10: 4a rewrites `ExecRunner::run`'s environment; 10 rewrites its command and lock on
  that text -> holds.
- 4b before 11: 11's scan finds no `"HOME"` only once 4b has removed `default_cache_dir`
  (`gate.rs:208`) -> holds.
- 7 before 8a: `extractors:` is validated, `ProjectExtractor` folds it into the fingerprint and
  opens the store, while every file still extracts through the bundled grammars -> holds.
- 8a before 10: `subprocess::shell_command` exists for `ExecRunner::run` -> holds.
- 9a before 9b: the interface and its Linux arm exist; the `_` arm answers nothing for macOS and
  Windows until 9b -> holds.
- 9b before 14: both edit the Windows bindings dependency's feature list in
  `crates/rigger-process/Cargo.toml` -> holds.
- 9a, 11, 13 and 15 in any order: disjoint files except `rigger-process`'s manifest and its
  `lib.rs` module lines, and each scan looks only for its own strings -> holds.
- 12 before 9, 10, 11 and 14: the job reads YAML only; it is red on the PR until they land, and
  no unit's gate runs it -> holds.

TEST PLACEMENT: criterion 1 in `setup.rs`'s test module and
`tests/init_template_sets_periphery.rs`; criteria 2 and 5 in `setup.rs`'s test module; criterion
3 in `setup.rs`'s test module and `instructions.rs`' hand-over test; criterion 4 in `gate.rs`'s,
`conductor.rs`'s, `config_store.rs`'s and `validate.rs`'s test modules, `tests/cli.rs` and
`tests/build_env_authority_periphery.rs`, rewritten, with `tests/reset_build_cache_periphery.rs`'
fixture workflow declaring the variable its gate writes through (:498-499); criterion 6 in
`setup.rs`'s test module; criterion 7 in the domain `config.rs` test module and `facts.rs`' test
module; criterion 8 in `facts.rs`' test module with a stub extractor script, `subprocess.rs`' and
`src/cli/graph.rs`'s; criterion 9 in `tests/os_process_boundary.rs` and the interface's own
module; criterion 10 in `gate.rs`'s and `subprocess.rs`' test modules and
`tests/os_shell_boundary.rs`; criterion 11 in `home.rs`' test module, `tests/os_home_boundary.rs`
and `tests/cli.rs`' scratch-orphans tests; criterion 12 in `tests/ci_lanes.rs`; criterion 13 in
`setup.rs`'s test module; criterion 14 in `gate_socket.rs`' test module; criterion 15 in
`keep_awake/linux.rs`' test module, over a fake portal on a private bus as its logind tests use, and
`crates/rigger-process/tests/keep_awake_threads.rs`, rewritten.

## Global constraints

- Hyphens, never em or en dashes, in every added line.
- No new event type and no new external crate dependency (the Windows bindings dependency's and
  the system-call dependency's features may grow, and `ring` widens its target); no size limit.
- Every string the binary puts before an agent stays contract or framing; a language's method
  lives in a persona the consumer owns.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE TEMPLATE ROOT IS THE CONSUMER'S: `rigger init --template <repository root>` in a project holding `Cargo.toml` writes the Rust set's gates, `.rigger/gates/red-before-green.sh` and its implementer, and no written file holds `crates/<name>` for any directory `<name>` under this repository's `crates/`; `rigger init` with no root writes a workflow with no gates and prints the no-root line; a root with no `scaffold/` is refused before any write; and `language_marker_hits` answers nothing over `scaffold_workflow(None)`, every `SCAFFOLD_AGENTS` seed and `HELPER_AGENTS`. This criterion OWNS `load_template_sets`, the `--template` arguments, the build-time generation's removal, the moved `tests/template_sets_build.rs` cases, the Rust set's `files` list and `set.yml` comment, and `set_implementers()`'s read; the seeds' texts are criteria 2's and 3's, NOT this one's.
- [ ] a test proves THE SCAFFOLD SEEDS A TEST AUTHOR: `init_project` into an empty project, with no template root and with the repository root, writes `.rigger/agents/sdet-author.md` with the seed's frontmatter and its `## A documentation-only round` rule, the written workflow declares `test_author: sdet-author`, and the written tree loads and validates; a project's own `sdet-author.md` is kept. This criterion OWNS the seed, `get_referenced_agent_ids`' test author, the seed-set, agents-line, agent-count, canonical-set, `DEFAULT_FLEET` and "7 agents" pins and the handbook's :121; the implementer's sentence is criterion 3's, NOT this one's.
- [ ] a test proves THE IMPLEMENTER LEAVES THE GATES TO THE GATES: whitespace-normalized, the `implementer.md` seed holds "confirm GREEN, run the checks your change needs, commit. The workflow's gates run the full battery on the commit." and not "run the named gates"; the built-in working discipline holds "Check: each shape you add was looked up where the project's test helpers live" and not "the project's duplication gate is green". This criterion OWNS that sentence and its pins, `LANGUAGE_NEUTRAL_IMPLEMENTER`, `rust-engineer.md:40-41` and the working-discipline table check only; the test author is criterion 2's, NOT this one's.
- [ ] a test proves RIGGER EXPORTS ONLY WHAT THE WORKFLOW DECLARES: with no `build.env`, a gate run through the built binary with `CARGO_TARGET_DIR` removed from its environment sees none, and `spawn_env` answers only rigger's own pairs; with `build.env: {B: "{build_cache}/x", J: "{jobs}", L: "lit"}` and `build.jobs: 3`, a gate sees `B` over its cache, `J=3` and `L=lit`, a unit spawn's `B` is over its unit sibling, a review tier's over its review sibling, and a spawn with no unit sibling gets `J` and `L` and no `B`; with `build.jobs: 0` no `J`; a reviewer's prompt carries the build cache sentence exactly when its spawn carries `B`; a bad name, each name rigger sets itself, an unknown placeholder and a stray brace are each refused naming the variable; a workflow carrying `build.wrapper` or `build.cache_dir` is refused with the retire message; `rigger validate` prints the `build env:` lines. This criterion OWNS `build.env`, `BuildEnv::declared` and `at`, the moved `SPAWN_ENV`, `RUN_BASE_ENV` and credential names, both sites, `SpawnOpts.build_cache`, `reviewer_carries_build_cache`'s source, `BUILD_CACHE_ENV`'s removal, the retirements and removals THE RETIRED WRAPPER lists, the `setup.rs:4034` pin, `tests/cli.rs`' build.wrapper section and the rewritten `tests/build_env_authority_periphery.rs`; the scaffold's `env` is criterion 6's, NOT this one's.
- [ ] a test proves A NEW SCAFFOLD BOUNDS ITS SPAWNS: the workflow `init_project` writes, with no template root and with the repository root, declares `max_wall_clock: 3600`, and `unbounded_wall_clock_advisory` over the loaded tree answers none. This criterion OWNS the scaffold's `max_wall_clock` line and its comment; the test author is criterion 2's, NOT this one's.
- [ ] a test proves THE SCAFFOLD DECLARES THE SET'S ENVIRONMENT: `rigger init --template <repository root>` in a Rust project writes `env: {CARGO_BUILD_JOBS: "{jobs}", CARGO_TARGET_DIR: "{build_cache}"}` under `build:` and the written workflow declares exactly those two variables; with no root it writes `env: {}`; a set whose `env` fails the check is refused naming the set. This criterion OWNS `SetSpec.env`, `@BUILD_ENV@` and the Rust set's `env`; the declaration's grammar is criterion 4's, NOT this one's.
- [ ] a test proves THE WORKFLOW DECLARES EXTRACTORS: `Config::validate` accepts a well-formed `extractors:` and refuses each malformed declaration THE EXTRACTORS KEY lists, naming `extractors.<name>`; with none, or with a declaration that fails the check, `declared_extractors` answers the empty map and `ProjectExtractor::of`'s fingerprint string equals `FactsExtractor`'s; two different declarations give two fingerprint strings different from it and from each other, and one declaration read twice gives one; a store opened through `open_store` under one declaration is not in force under another. This criterion OWNS the key, `check_extractors`, `declared_extractors`, `ProjectExtractor`'s fingerprint and the openers' composition; running an extractor is criterion 8's, NOT this one's.
- [ ] a test proves A DECLARED EXTRACTOR GROUNDS ITS LANGUAGE: with a workflow declaring a stub extractor script for an extension no grammar covers, the code store `open_store` advances holds the stub's definitions for each such file as code facts under the declared name, the stub started once for the advance; a file the stub answers with `{"error"}`, with another file's `path`, or with two defs whose `parent` names the other has its identity facts and no definitions, and the next file still extracts; a declaration named `rust` over `rs` replaces the bundled grammar for `.rs` files; `cmd_argument` wraps a run as THE EXTRACTOR PROTOCOL states; and `locate_definition_extent` bounds a declared def's body by the stub's `end_line`. This criterion OWNS the sessions, the protocol, `code_facts`' new inputs, `shell_command`, `cmd_argument`, `extent_in` and the handbook's last :19 sentence; the key and the fingerprint are criterion 7's, NOT this one's.
- [ ] a test proves PROCESS FACTS COME THROUGH ONE INTERFACE: criterion 9's boundary scan (THE BOUNDARY SCANS) finds nothing; on a unix host, `os_process::pids()` holds the test's pid, `parent` of it is its parent's, `start_time` answers one value twice, `cwd` is its working directory, `command` is non-empty, `state` is `Some('R')` or `Some('S')`, `open_files` holds a file it holds open, and `listening_pid` of a port it listens on is its pid; the reaper's, holder check's and dash's existing tests pass through the interface. This criterion OWNS the interface, its three adapters, `send_signal`'s per-OS arms, the reroutes and any `tests/no_os_kill_audit.rs` re-pin; the shell and the lock are criterion 10's and the home criterion 11's, NOT this one's.
- [ ] a test proves THE SHELL AND THE LOCK ARE PORTABLE: on unix, a guarded gate whose command backgrounds a child with its stdout and stderr sent away from the gate's pipes keeps the shared build-cache lock until that child exits (an exclusive try-lock on the guard, through a second open of it after the gate's run returns, fails while the child lives and succeeds after it exits), a guard that cannot be locked fails with `ProbeFault`'s message, `pathext_candidates("git", ".COM;.EXE")` is `git.COM`, `git.EXE` and of `git.exe` is `git.exe`, and criterion 10's boundary scan (THE BOUNDARY SCANS) finds nothing. This criterion OWNS `ExecRunner::run`'s command, `hold_shared_lock`, `inherit_on_exec`, `find_executable`'s move and Windows lookup, `subprocess::command`'s Windows resolution, the `setup.rs:592` move and the handbook's shell sentence; `shell_command` is criterion 8's, the process interface criterion 9's and the home criterion 11's, NOT this one's.
- [ ] a test proves THE HOME IS READ IN ONE PLACE: on unix `home_dir()` answers the test process's non-empty `HOME`; the built binary's `rigger reset --scratch-orphans`, run with `XDG_CACHE_HOME` removed and `HOME` removed or empty, fails printing "neither XDG_CACHE_HOME nor a home directory is known"; and criterion 11's boundary scan (THE BOUNDARY SCANS) finds nothing. This criterion OWNS `home_dir`, `home.rs`, the four reads, the store edge and the hygiene refusal; the shell and the lock are criterion 10's, NOT this one's.
- [ ] a test proves THE TREE CHECKS ON EVERY OS: `tests/ci_lanes.rs` reads a `cross-os-check` job whose matrix `os` is exactly `windows-latest` and `macos-latest`, whose `runs-on` is the matrix value, whose checkout fetches full history, and whose check step runs under `bash`, exporting `RUSTC_WRAPPER` empty before `cargo check --workspace`; `every_job_that_builds_dash_installs_the_console_core_wasm_target` reads it among its jobs. This criterion OWNS the job; making the tree compile there is criteria 9's, 10's, 11's and 14's, NOT this one's.
- [ ] a test proves A NEW PROJECT IGNORES RIGGER'S RUNTIME FILES: after `init_project` in a fresh git repository, `git check-ignore` reports ignored each of `.rigger/events.db`, `.rigger/events.db-wal`, `.rigger/events.db-shm`, `.rigger/graph.db`, `.rigger/graph.db.lock`, `.rigger/progress.db`, `.rigger/step.lock`, `.rigger/store/x`, `.rigger/tmp/x`, `.rigger/playbooks/x`, `.rigger/dash.url`, `.rigger/dash.marker`, `.rigger/dash.attempt`, `.rigger/store.conn` and `.rigger/events.db.bak-x`, and none of `.rigger/workflow.yml`, `.rigger/agents/planner.md` and `.rigger/project.id`. This criterion OWNS `RUNTIME_IGNORES` and its comment; the seeds are criterion 2's, NOT this one's.
- [ ] a test proves THE GATE CHANNEL HAS A WINDOWS ADAPTER: on every host `pipe_name` over one root's bytes is `\\.\pipe\rigger-gate-` followed by the same 16 hex digits the unix socket name carries for those bytes, and over two roots gives two names; the unix arm's existing tests pass unchanged. This criterion OWNS the `cfg_select!` binding, the Windows arm, `pipe_name` and `ring`'s target; the cross-OS compile is criterion 12's, NOT this one's.
- [ ] a test proves THE KEEP-AWAKE HOLDS A GRAPHICAL SESSION AWAKE: `graphical_session` is true when `WAYLAND_DISPLAY` or `DISPLAY` is non-empty and false when both are empty or unset; with a graphical session, an acquire against a fake logind and a fake portal on private buses makes one portal `Inhibit` call with flags 12 and the reason, its connection open while the guard stands and closed after the guard drops; with no graphical session it makes no portal connection; and a portal that refuses leaves the acquire granted. This criterion OWNS the portal call, `graphical_session`, the guard's connection thread and the rewritten `keep_awake_threads.rs`; logind's inhibitor stays as it is, NOT this one's.
- [ ] `.rigger/gates/lanes.sh` runs green for both feature lanes on the finished tree, verified by `sh .rigger/gates/lanes.sh no-default` and `sh .rigger/gates/lanes.sh core` each exiting 0. This criterion OWNS only the lanes; every behaviour is criteria 1 to 15's, NOT this one's.
