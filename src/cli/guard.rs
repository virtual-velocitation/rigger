use super::*;

/// A `rigger grep-guard` decision: pass the tool call through untouched, pass it through
/// with its `tool_input` rewritten first, or block it with the reason shown to the agent
/// (spec 92, criterion 4: the graph-first lookup hook's stated message).
#[derive(Debug, PartialEq, Eq)]
enum GuardDecision {
    Allow,
    /// Allow, but only after the real shell runs the REWRITTEN `tool_input` carried here in
    /// place of the one the agent sent - spec 92's HOOK SCOPE amendment: the `--literal`
    /// escape-hatch marker has no meaning to a real `grep` invocation (it is not one of
    /// grep's own flags), so the hook strips it before the command reaches a real shell,
    /// rather than passing it through to fail there instead.
    AllowWithUpdatedInput(serde_json::Value),
    Deny(String),
}

/// The pure decision core of `rigger grep-guard` (spec 92, criterion 4, HOOK SCOPE
/// amendment d-spec92-hook-no-target-axis): given the PreToolUse tool name and its raw
/// `tool_input`, decide whether this call should bounce toward `rigger_ground`/
/// `rigger_graph`, pass through untouched, or pass through with its `tool_input` rewritten
/// first. The hook has NO target axis - a shell command's real search target is
/// undecidable from its text alone (`..`, `*`, `~`, `$(pwd)`, a redirection, a symlink all
/// defeat a path-based guess) - so every `Grep` tool call and every `grep`-invoking `Bash`
/// command is denied inside a rigger project, with no path/target ever inspected; the
/// FORMER guarded-tree apparatus (a `GREP_GUARDED_TREES` allowlist of trees, and the path-
/// containment machinery built on it) is retired outright, not kept beside this rule. Pure
/// (no I/O), so the decision is unit-testable against synthesized hook payloads without
/// spawning a real hook process or touching a real filesystem. `--literal` on a `Bash`
/// `grep` command is the sole, deliberate escape hatch (Design's CONSTRAINTS WALK: "a
/// literal-text lookup - `--literal` passes the hook"; the HOOK SCOPE amendment: the hook
/// STRIPS the marker from the command it allows, via `updatedInput`, because grep itself
/// has no such flag); the built-in `Grep` tool carries no such flag slot, so a genuinely
/// literal search through it is `grep --literal` via `Bash` instead - "grep is for literal
/// text - add `--literal` to proceed" names exactly that path.
fn grep_guard_decision(tool_name: &str, tool_input: &serde_json::Value) -> GuardDecision {
    match tool_name {
        "Grep" => GuardDecision::Deny(GREP_GUARD_MESSAGE.to_string()),
        "Bash" => {
            let command = tool_input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if !command_invokes_grep(command) {
                return GuardDecision::Allow;
            }
            if shell_command_words(command).any(|w| w == "--literal") {
                let mut updated_input = tool_input.clone();
                if let Some(obj) = updated_input.as_object_mut() {
                    obj.insert(
                        "command".to_string(),
                        serde_json::Value::String(strip_literal_marker(command)),
                    );
                }
                return GuardDecision::AllowWithUpdatedInput(updated_input);
            }
            GuardDecision::Deny(GREP_GUARD_MESSAGE.to_string())
        }
        _ => GuardDecision::Allow,
    }
}

/// Splits a Bash command line into real shell WORDS, PAIRED with each word's `[start, end)`
/// BYTE range in the raw `command` text - the one scan [`shell_command_words`] (word values
/// only) and [`strip_literal_marker`] (excising a specific occurrence of the `--literal`
/// marker from the real command text) both build on, so the two can never drift on what a
/// "word" is (one mutation authority, not a value-only walk here and a second span-tracking
/// walk maintained in parallel elsewhere). Word VALUES are already resolved to what a real
/// shell would pass as argv: ONE coherent walk over the raw command string, not two disjoint
/// passes (reject-fix round 5: sdet-u92c4r4-backslash-newline-continuation-still-bypasses-
/// the-guard). A prior version split the raw text on delimiters FIRST and only then resolved
/// quoting/escaping per token, so a delimiter a real shell would not treat as a separator - a
/// backslash-newline line continuation - permanently split one command name into two dead
/// fragments before the escape logic ever ran. This walk instead tracks quote state and finds
/// word boundaries in the SAME scan: outside any quote, whitespace and the metacharacters
/// that can fuse two commands (or a command and a redirected path) together with no
/// whitespace between them - a pipe `|`, a semicolon `;`, `&` (backgrounding/`&&`), `(` `)`
/// (subshells and `$( )` command substitution), `<` `>` (input/output redirection - reject-
/// fix round 5: sdet-u92c4r5-redirect-metachar-fuses-guarded-path-first-segment, spec 92's
/// HOOK SCOPE amendment names `; | & ( ) < >` verbatim), a backtick (the older command-
/// substitution form), and `$` itself - end the current word (reject-fix adj-u92c4r2-verdict-
/// reject-shell-metachar-bypass: plain `str::split_whitespace` hid `grep` fused to a
/// neighbor by one of these with no separating whitespace); single quotes bracket a LITERAL
/// run (no escape has meaning inside them); double quotes bracket a run where only `\\`,
/// `\"`, `` \` ``, `\$` and a line continuation are escapes (any other backslash stays
/// literal); and outside any quote a backslash escapes the very next character LITERALLY -
/// including a delimiter, which is why an escaped delimiter can no longer split a word -
/// except a backslash immediately followed by a newline, which vanishes with NO separator
/// and no output, exactly the join point a real shell removes before word splitting ever
/// sees it. Quoting can open and close more than once within one word - `g''rep` is `g` + an
/// EMPTY single-quoted run + `rep` = `grep`, the same word `"grep"`, `'grep'`, and `gr\ep`
/// each also resolve to; its SPAN still covers the whole raw run (`g''rep`, six raw bytes),
/// even though the resolved word value is the four-byte `grep`, so excising it by span
/// removes exactly what was written, quoting included. Quote state resets at each new word
/// (coarse by the same design this guard already accepts: a quoted metacharacter still ends
/// the word, the rare false positive `--literal` exists to pass through) - the point is that
/// a real invocation, however it wraps a line or spells its command name, can now never hide
/// from the scan.
fn shell_command_word_spans(command: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let mut spans: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    let mut current = String::new();
    let mut start = 0usize;
    let mut quote: Option<char> = None;
    let mut chars = command.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if current.is_empty() && quote.is_none() {
            // Track where the word IN PROGRESS began in the raw text - reset on every char
            // seen while there is no word yet (a delimiter between words, or the true first
            // char of the next one); the last assignment before `current` stops being empty
            // is exactly that word's start.
            start = idx;
        }
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    current.push(c);
                }
            }
            Some(_) => {
                // Inside double quotes: backslash escapes only \\, \", \$, a backtick, or (like
                // outside any quote) a line continuation - a backslash immediately followed by a
                // newline vanishes with no separator.
                if c == '"' {
                    quote = None;
                } else if c == '\\' && matches!(chars.peek(), Some((_, '\\' | '"' | '$' | '`'))) {
                    let (_, next) = chars.next().expect("peeked Some above");
                    current.push(next);
                } else if c == '\\' && matches!(chars.peek(), Some((_, '\n'))) {
                    chars.next();
                } else {
                    current.push(c);
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '\\' if matches!(chars.peek(), Some((_, '\n'))) => {
                    // Line continuation (reject-fix round 5): removed with zero separator,
                    // so it can never again split one word into two dead fragments.
                    chars.next();
                }
                '\\' => {
                    if let Some((_, next)) = chars.next() {
                        current.push(next);
                    }
                }
                _ if c.is_whitespace() || "|;&()`$<>".contains(c) => {
                    if !current.is_empty() {
                        spans.push((std::mem::take(&mut current), start..idx));
                    }
                }
                _ => current.push(c),
            },
        }
    }
    if !current.is_empty() {
        spans.push((current, start..command.len()));
    }
    spans
}

/// Word VALUES only - see [`shell_command_word_spans`], the one scan both this and
/// [`strip_literal_marker`] build on.
fn shell_command_words(command: &str) -> std::vec::IntoIter<String> {
    shell_command_word_spans(command)
        .into_iter()
        .map(|(word, _)| word)
        .collect::<Vec<_>>()
        .into_iter()
}

/// Removes every raw occurrence of the `--literal` escape-hatch marker from `command` (spec
/// 92's HOOK SCOPE amendment: "the hook removes that marker from the command it allows...
/// because grep itself has no such flag" - GNU grep really does reject it: `unrecognized
/// option '--literal'`, exit 2), using the EXACT span [`shell_command_word_spans`] resolved
/// each occurrence to, not a naive substring replace - so a quoted or backslash-spliced
/// spelling of the marker (`'--literal'`, `--liter''al`) is excised by its real extent in
/// the raw text, never a marker that merely happens to appear inside a later argument like
/// the search pattern. Removes exactly one adjacent whitespace byte together with each
/// marker (the one immediately before it, when there is one, else the one immediately after)
/// so the surrounding words are neither glued together nor left double-spaced -
/// `grep --literal pattern` becomes `grep pattern`, never `grep  pattern`. Everything else in
/// the command - pipes, redirections, quoting, the pattern itself - is untouched. Multiple
/// occurrences are removed in reverse text order so an earlier removal never invalidates a
/// later span's byte offsets.
fn strip_literal_marker(command: &str) -> String {
    let mut result = command.to_string();
    for (word, range) in shell_command_word_spans(command).into_iter().rev() {
        if word != "--literal" {
            continue;
        }
        let mut start = range.start;
        let mut end = range.end;
        if start > 0
            && result
                .as_bytes()
                .get(start - 1)
                .is_some_and(u8::is_ascii_whitespace)
        {
            start -= 1;
        } else if result
            .as_bytes()
            .get(end)
            .is_some_and(u8::is_ascii_whitespace)
        {
            end += 1;
        }
        result.replace_range(start..end, "");
    }
    result
}

/// The PATH BASENAME of one [`shell_command_words`] word - the substring after its last `/`,
/// or the whole word when it has none - the same notion a shell uses to resolve a command
/// name regardless of how it was invoked.
fn word_basename(word: &str) -> &str {
    match word.rsplit_once('/') {
        Some((_, base)) => base,
        None => word,
    }
}

/// True when a Bash command line contains `grep` as a whole [`shell_command_words`] word's
/// PATH BASENAME (reject-fix round 5, adv-u92c4-r4-path-qualified-grep-bypasses-command-check:
/// `/usr/bin/grep`, `./grep`, and a relative `bin/grep` all name the same binary a bare
/// `grep` does, so a directory prefix must not defeat the match), not a substring of a longer
/// word like `zgrep` or `--grep-something`, and not hidden by fusion to an adjacent command
/// via `|`/`;`/`&`/`$( )`/a backtick with no surrounding whitespace (adj-u92c4r2-verdict-
/// reject-shell-metachar-bypass) - the literal command name the Design text names ("a Grep or
/// a `grep`"), never `rg`/`egrep`/`fgrep` or any other tool this criterion's stated scope
/// does not cover.
fn command_invokes_grep(command: &str) -> bool {
    shell_command_words(command).any(|w| word_basename(&w) == "grep")
}

/// `rigger grep-guard`: the command the installed PreToolUse hook runs (see
/// [`install_lookup_hook`]). Reads ONE Claude Code PreToolUse payload as JSON on stdin
/// (`{"tool_name", "tool_input"}`), writes a `hookSpecificOutput.permissionDecision`
/// verdict to stdout (an `updatedInput` alongside an "allow" verdict when the decision
/// rewrote the command - see [`GuardDecision::AllowWithUpdatedInput`]), and always exits 0 -
/// a hook's own exit code is a SEPARATE failure channel from its JSON decision, so this
/// command reports "deny" through the JSON body alone, never a nonzero exit (a transport
/// hiccup stays tellable apart from a deliberate block). Inert outside a rigger project (no
/// [`RIGGER_DIR`] in the current tree) - malformed or unreadable stdin degrades to an allow
/// rather than erroring, so the hook never blocks a tool call it failed to understand. This
/// command does no I/O beyond stdin/stdout and the [`RIGGER_DIR`] check: since the hook has
/// no target axis (d-spec92-hook-no-target-axis), the pure [`grep_guard_decision`] needs no
/// resolved project root to decide against.
pub(crate) fn cmd_grep_guard(_args: &[String]) -> Res {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
    let decision = if !Path::new(RIGGER_DIR).is_dir() {
        GuardDecision::Allow
    } else {
        let payload: serde_json::Value =
            serde_json::from_str(input.trim()).unwrap_or_else(|_| serde_json::json!({}));
        let tool_name = payload
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let tool_input = payload
            .get("tool_input")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        grep_guard_decision(tool_name, &tool_input)
    };
    let out = match decision {
        GuardDecision::Allow => serde_json::json!({}),
        GuardDecision::AllowWithUpdatedInput(updated_input) => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": updated_input,
            }
        }),
        GuardDecision::Deny(reason) => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        }),
    };
    println!("{out}");
    Ok(())
}

/// Parse `rigger guard-write`'s own args: one or more `--root <dir>`, in the order given
/// (the deny reason names the FIRST one, so order is meaningful, not just a set). At least
/// one is required - a guard installed with no root would silently allow every write, the
/// opposite of THE WRITE GUARD's contract, so a misconfigured host fails loudly at argument
/// parse time rather than silently passing every future tool call.
fn parse_guard_write_roots(args: &[String]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut roots = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                let dir = args.get(i + 1).ok_or(
                    "guard-write: --root expects a directory: rigger guard-write --root <dir>...",
                )?;
                if dir.is_empty() {
                    return Err("guard-write: --root expects a non-empty directory".into());
                }
                roots.push(dir.clone());
                i += 2;
            }
            other => {
                return Err(format!(
                    "guard-write: unknown argument {other:?}: rigger guard-write --root <dir>..."
                )
                .into())
            }
        }
    }
    if roots.is_empty() {
        return Err("guard-write: at least one --root is required".into());
    }
    Ok(roots)
}

/// THE WRITE GUARD's target axis (spec 104 criterion 4): what an incoming `PreToolUse`
/// call means for the guard's decision. THE GUARD DENIES BY DEFAULT
/// (op-104-write-guard-deny-by-default-complete-audit): `NotCovered` (Allow) is returned
/// ONLY when the top-level payload is a JSON object whose `tool_name` is an actual string
/// naming a tool outside the three this guard matches - nothing this guard protects, so it
/// is allowed rather than guessed at. Every OTHER shape - an unparseable or non-object
/// top-level payload, an absent or non-string `tool_name`, and (for a covered tool) an
/// absent/non-object `tool_input` or an absent/non-string target field - is
/// `TargetUnreadable`, denied exactly like a resolved target outside every root, never
/// folded into the same Allow as `NotCovered`: this guard's entire purpose is containment,
/// so a payload it cannot positively classify as a covered write inside a root is not
/// evidence of safety, it is evidence the guard cannot see what the call would do.
/// `Target` carries the raw string to resolve and check against the configured roots.
#[derive(Debug, PartialEq, Eq)]
enum WriteTarget {
    NotCovered,
    TargetUnreadable,
    Target(String),
}

/// THE WRITE GUARD's target axis (spec 104 criterion 4): the path an `Edit`/`Write`/
/// `NotebookEdit` call would touch, straight from that tool's own documented `tool_input`
/// shape - `file_path` for `Edit`/`Write`, `notebook_path` for `NotebookEdit`. Assumes
/// `tool_name` was already confirmed to come from a JSON object's own string field -
/// [`guard_write_read_target`] owns that classification; this function only decides the
/// covered-vs-not-covered axis and the target-field axis underneath it.
/// `WriteTarget::NotCovered` for every other tool name (the installed hook's own matcher
/// already scopes calls to these three). `WriteTarget::TargetUnreadable` for a covered
/// tool whose `tool_input` lacks the expected key, or carries it as a non-string (or whose
/// `tool_input` is itself not a JSON object at all - indexing a non-object by a string key
/// already yields `None`, so that shape falls into the same arm) - this guard's entire
/// purpose is containment, so a payload it cannot read is denied, never folded into the
/// same outcome as a tool it does not cover.
fn guard_write_target(tool_name: &str, tool_input: &serde_json::Value) -> WriteTarget {
    let key = match tool_name {
        "Edit" | "Write" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return WriteTarget::NotCovered,
    };
    match tool_input.get(key).and_then(serde_json::Value::as_str) {
        Some(raw) => WriteTarget::Target(raw.to_string()),
        None => WriteTarget::TargetUnreadable,
    }
}

/// THE GUARD DENIES BY DEFAULT (op-104-write-guard-deny-by-default-complete-audit): decide
/// [`WriteTarget`] and the payload's own `cwd` from ONE whole parsed PreToolUse JSON
/// `Value`, owning every shape question [`guard_write_target`] itself never sees. A
/// top-level value that is not a JSON object (an array, `null`, a bare string, number, or
/// bool - reachable when `serde_json::from_str` itself still succeeds, so the caller's own
/// parse-`Err` arm never fires) is `TargetUnreadable`. `tool_name` absent or not a string
/// is ALSO `TargetUnreadable`, never defaulted to `""` (a default that used to read as a
/// real, deliberate "other tool" and fall into `NotCovered` = Allow - the exact fail-open
/// this function exists to close). Only once the payload is confirmed to be an object
/// carrying `tool_name` as a real string does `guard_write_target` ever run, so
/// `WriteTarget::NotCovered` is reachable through exactly one door: a string `tool_name`
/// this guard does not cover. `tool_input`'s own absence still defaults to an empty object
/// (safe: fed through `guard_write_target`, an empty object cannot supply a covered tool's
/// target field either, so it still resolves to `TargetUnreadable`, never a bypass).
fn guard_write_read_target(payload: &serde_json::Value) -> (WriteTarget, String) {
    let obj = match payload.as_object() {
        Some(obj) => obj,
        None => return (WriteTarget::TargetUnreadable, String::new()),
    };
    let tool_name = match obj.get("tool_name").and_then(serde_json::Value::as_str) {
        Some(name) => name,
        None => return (WriteTarget::TargetUnreadable, String::new()),
    };
    let tool_input = obj
        .get("tool_input")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let cwd = obj
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    (guard_write_target(tool_name, &tool_input), cwd)
}

/// Resolve one path component's symlink chain against `result` (the caller's
/// already-resolved prefix), pushing the resolved target back through the SAME walk
/// (mutual recursion with [`resolve_lexical_realpath`]) so a symlink that itself points
/// through another symlink, or ends in more `..`, keeps resolving left to right exactly as
/// a real `realpath` would. `budget` bounds total hops against a symlink cycle; once spent,
/// any further component is kept lexically (unresolved) rather than looping forever.
fn resolve_lexical_realpath(path: &Path, budget: &mut u32) -> PathBuf {
    let mut result = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Prefix(_) => {}
            std::path::Component::RootDir => result.push(std::path::MAIN_SEPARATOR.to_string()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::Normal(part) => {
                result.push(part);
                if *budget == 0 {
                    continue;
                }
                if let Ok(link) = std::fs::read_link(&result) {
                    *budget -= 1;
                    let joined = if link.is_absolute() {
                        link
                    } else {
                        result
                            .parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR.to_string()))
                            .join(&link)
                    };
                    result = resolve_lexical_realpath(&joined, budget);
                }
            }
        }
    }
    result
}

/// One symlink-hop budget for [`resolve_lexical_realpath`] - generous enough for any real
/// tree, small enough that a symlink cycle on disk cannot hang the guard.
const SYMLINK_RESOLVE_BUDGET: u32 = 40;

/// Resolve `raw` (an Edit/Write/NotebookEdit target, absolute or relative) against `cwd`
/// (the PreToolUse payload's OWN reported `cwd` - never this process's, so the decision is
/// correct no matter where the host happens to run the hook command from) into an
/// absolute, symlink-resolved path - WITHOUT requiring the target to exist, unlike
/// `std::fs::canonicalize` (a `Write` routinely creates a brand new file). Every existing
/// ancestor's symlinks are resolved left to right exactly as a real `realpath` would, so a
/// `..` that walks back out through an escaping symlink lands where the symlink actually
/// points, not where the raw text alone suggests - THE WRITE GUARD's stated "relative
/// paths against the hook's cwd, `..`, symlinks" in one pass. A relative `raw` with an
/// empty `cwd` (a malformed or absent payload field) resolves to a bare relative path,
/// which cannot lie under any absolute root - the safe default is deny, never allow.
fn resolve_write_target(cwd: &str, raw: &str) -> PathBuf {
    let raw_path = Path::new(raw);
    let base = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        Path::new(cwd).join(raw_path)
    };
    let mut budget = SYMLINK_RESOLVE_BUDGET;
    resolve_lexical_realpath(&base, &mut budget)
}

/// Whether `target` (already resolved by [`resolve_write_target`]) lies under `root`.
/// `root` is resolved through the SAME symlink-aware walk (never a second, divergent
/// containment rule), so a root that is itself reached through a symlink still compares
/// correctly. Equal to `root` counts as under it.
fn write_target_under_root(target: &Path, root: &str) -> bool {
    let mut budget = SYMLINK_RESOLVE_BUDGET;
    let resolved_root = resolve_lexical_realpath(Path::new(root), &mut budget);
    target.starts_with(&resolved_root)
}

/// The shared "outside every root" verdict, naming the FIRST root (`roots` is never
/// empty - [`parse_guard_write_roots`] enforces that before [`guard_write_decision`] is
/// ever called). Used both for a target actually resolved outside every root and for
/// `WriteTarget::TargetUnreadable` - the fail-safe default is deny, never allow, and both
/// cases read identically to whatever consumes the decision.
fn guard_write_deny_outside_first_root(roots: &[String]) -> GuardDecision {
    let first_root = roots.first().map(String::as_str).unwrap_or("");
    GuardDecision::Deny(format!(
        "write target is outside the allowed root: {first_root}"
    ))
}

/// THE WRITE GUARD's pure decision core (spec 104 criterion 4): a target outside every
/// root is denied naming the FIRST root; a covered tool whose target this guard could not
/// read (`WriteTarget::TargetUnreadable`) is denied the SAME way, never allowed - a
/// malformed payload for a tool this guard covers is not evidence of safety, it is
/// evidence the guard cannot see what the call would do; `WriteTarget::NotCovered` (a tool
/// this guard does not cover) is always allowed. Reuses [`GuardDecision`] - the SAME
/// verdict type `rigger grep-guard` reports through, never a second parallel one - though
/// this guard never rewrites a tool call, so its `AllowWithUpdatedInput` arm never arises
/// here.
fn guard_write_decision(roots: &[String], cwd: &str, target: WriteTarget) -> GuardDecision {
    let raw = match target {
        WriteTarget::NotCovered => return GuardDecision::Allow,
        WriteTarget::TargetUnreadable => return guard_write_deny_outside_first_root(roots),
        WriteTarget::Target(raw) => raw,
    };
    let resolved = resolve_write_target(cwd, &raw);
    if roots
        .iter()
        .any(|root| write_target_under_root(&resolved, root))
    {
        GuardDecision::Allow
    } else {
        guard_write_deny_outside_first_root(roots)
    }
}

/// Claude Code's own documented `PreToolUse` hook contract (the same contract
/// [`cmd_grep_guard`]'s doc comment states): exit code 2 is the ONE BLOCKING exit - the
/// tool call is stopped and stderr is fed back as the reason. Every other nonzero exit is
/// NON-blocking: Claude Code merely surfaces it and lets the tool call through anyway.
/// `rigger guard-write` IS the security containment boundary (unlike the advisory
/// `rigger grep-guard`, which always exits 0 by design), so a transport or configuration
/// failure here - unreadable stdin, no `--root` at all, a dangling `--root` flag - exits
/// THIS code, never the generic exit-1 path every other `rigger` command error takes: exit
/// 1 would silently allow every subsequent write for the rest of the spawn's life
/// (op-104-write-guard-deny-by-default-complete-audit).
const GUARD_WRITE_BLOCKING_EXIT_CODE: i32 = 2;

/// `rigger guard-write --root <dir> [--root <dir> ...]`: THE WRITE GUARD (spec 104
/// criterion 4) - the `PreToolUse` command hook for `Edit`/`Write`/`NotebookEdit` the host
/// injects into a launched agent's settings. Reads ONE PreToolUse payload
/// as JSON on stdin (`{"tool_name","tool_input","cwd"}`) and allows a target under one of
/// `roots`, denying every other - absolute, relative, `..`, symlink-escaping - with the
/// reason naming the first root. THE GUARD DENIES BY DEFAULT
/// (op-104-write-guard-deny-by-default-complete-audit): every payload shape this guard
/// cannot positively classify as a covered tool writing to a resolvable target - an
/// unparseable top-level payload, one that parses but is not a JSON object, an absent or
/// non-string `tool_name`, or (for a covered tool) an absent/non-object `tool_input` or an
/// absent/non-string target field, all owned by [`guard_write_read_target`] - denies
/// naming the first root exactly like a resolved out-of-root target, never a silent fold
/// into an empty (Allow) object: this guard cannot tell whether a payload shape it could
/// not read was a covered tool writing outside every root, so it denies rather than
/// guesses. It reads no store: every fact the decision needs travels on argv or stdin,
/// exactly like [`cmd_grep_guard`]'s own pure/no-store discipline. A well-formed
/// invocation always exits 0 - the verdict rides in the JSON body - but a transport or
/// configuration failure (unreadable stdin, no usable `--root`) exits
/// [`GUARD_WRITE_BLOCKING_EXIT_CODE`] directly with the reason on stderr, never the
/// generic exit-1 every other `rigger` command error takes and never a silent 0: unlike
/// `cmd_grep_guard` (advisory; a missed block is merely a missed reminder), this guard IS
/// the containment boundary, so a transport failure that read as non-blocking would
/// silently allow every write for the rest of the spawn's life.
pub(crate) fn cmd_guard_write(args: &[String]) -> Res {
    let roots = parse_guard_write_roots(args).unwrap_or_else(|e| {
        eprintln!("rigger guard-write: {e}");
        std::process::exit(GUARD_WRITE_BLOCKING_EXIT_CODE);
    });

    let mut input = String::new();
    if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input) {
        eprintln!("rigger guard-write: failed to read the PreToolUse payload from stdin: {e}");
        std::process::exit(GUARD_WRITE_BLOCKING_EXIT_CODE);
    }
    let (target, cwd) = match serde_json::from_str::<serde_json::Value>(input.trim()) {
        Ok(payload) => guard_write_read_target(&payload),
        Err(_) => (WriteTarget::TargetUnreadable, String::new()),
    };
    let decision = guard_write_decision(&roots, &cwd, target);

    let out = match decision {
        GuardDecision::Allow => serde_json::json!({}),
        GuardDecision::Deny(reason) => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        }),
        GuardDecision::AllowWithUpdatedInput(_) => {
            unreachable!("guard-write never rewrites a tool call's input")
        }
    };
    println!("{out}");
    Ok(())
}

/// `rigger hook <subcommand>` - the shared namespace for hook commands the per-spawn
/// `--settings` JSON installs (today, only [`cmd_hook_stop_failure`]; `guard-write` predates
/// this namespace and keeps its own top-level command name for compatibility).
pub(crate) fn cmd_hook(args: &[String]) -> Res {
    match args.first().map(String::as_str) {
        Some("stop-failure") => cmd_hook_stop_failure(&args[1..]),
        Some(other) => Err(format!(
            "hook: unknown subcommand {other:?}: rigger hook stop-failure --spawn <id> --class <category>"
        )
        .into()),
        None => Err(
            "hook: expected a subcommand: rigger hook stop-failure --spawn <id> --class <category>"
                .into(),
        ),
    }
}

/// `rigger hook stop-failure --spawn <id> --class <category>`: THE HOOKS' `StopFailure`
/// family - command AND record halves both (spec 104 criterion 5, Design: "criterion 5's,
/// command, record and injection both"). The installed hook runs
/// this the moment a turn ends on `category`, so FAILURE CLASS's first-priority source
/// ([`rigger::progress::latest_stop_failure_class`]) survives even when the stream's own
/// last line is lost. `--class` must name one of [`conductor::AgentFailure`]'s known
/// categories - every command this crate itself installs always does, so an unrecognized
/// value here means a stale or hand-edited settings file, and this refuses loudly rather
/// than silently recording a category [`conductor::AgentFailure::from_category`] would
/// degrade to `unknown` at classification time anyway. Routed through [`require_store_dir`]
/// like every other courier ([`cmd_progress`]'s own doc), so a worker running it from a
/// nested worktree records into the project's real store, never a misfiled one.
fn cmd_hook_stop_failure(args: &[String]) -> Res {
    let mut spawn_id: Option<String> = None;
    let mut class: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--spawn" => {
                let v = args
                    .get(i + 1)
                    .ok_or("hook stop-failure: --spawn expects a value")?;
                spawn_id = Some(v.clone());
                i += 2;
            }
            "--class" => {
                let v = args
                    .get(i + 1)
                    .ok_or("hook stop-failure: --class expects a value")?;
                class = Some(v.clone());
                i += 2;
            }
            other => {
                return Err(format!(
                    "hook stop-failure: unknown argument {other:?}: rigger hook stop-failure \
                     --spawn <id> --class <category>"
                )
                .into());
            }
        }
    }
    let spawn_id = spawn_id.ok_or("hook stop-failure: --spawn is required")?;
    let class = class.ok_or("hook stop-failure: --class is required")?;
    if spawn_id.trim().is_empty() {
        return Err("hook stop-failure: --spawn must be non-empty".into());
    }
    if !conductor::AgentFailure::CATEGORIES
        .iter()
        .any(|c| c.as_str() == class)
    {
        let known: Vec<&str> = conductor::AgentFailure::CATEGORIES
            .iter()
            .map(|c| c.as_str())
            .collect();
        return Err(format!(
            "hook stop-failure: unrecognized --class {class:?}; known categories: {}",
            known.join(", ")
        )
        .into());
    }

    let (loc, selection) = require_store_dir()?;
    refresh_registry_entry(&loc, &selection);
    let run_backend = resolve_store(&selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    let events = run_store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let run_id = runscope::current_run_id(&events).unwrap_or_default();
    let prog_backend = Store::open(&loc.file("progress.db"))?;
    let prog_store = Namespaced::new(&prog_backend, &loc.identity());
    let pos = rigger::progress_store::record_stop_failure(
        &prog_store,
        &run_id,
        &progress::StopFailure {
            spawn: spawn_id.clone(),
            class: class.clone(),
        },
    )?;
    println!("stop-failure recorded for {spawn_id} (class {class}, position {pos})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts `decision` is [`GuardDecision::AllowWithUpdatedInput`] carrying a `command`
    /// with the `--literal` marker genuinely gone (not merely a bare `Allow`, which round 5
    /// wrongly accepted: the marker then reached a real shell unchanged, and real GNU grep
    /// rejects it outright - `unrecognized option '--literal'`, exit 2), then returns that
    /// stripped command string so a caller can pin further properties of it (redirections,
    /// the pattern, an exact expected value).
    fn assert_allows_with_literal_stripped(
        decision: GuardDecision,
        original_command: &str,
    ) -> String {
        match decision {
            GuardDecision::AllowWithUpdatedInput(updated) => {
                let stripped = updated["command"]
                    .as_str()
                    .unwrap_or_else(|| {
                        panic!("updatedInput must carry a string `command`; got {updated}")
                    })
                    .to_string();
                assert!(
                    !shell_command_words(&stripped).any(|w| w == "--literal"),
                    "the stripped command must carry no --literal marker for {original_command:?}; \
                     got {stripped:?}"
                );
                stripped
            }
            other => panic!(
                "expected AllowWithUpdatedInput stripping --literal from {original_command:?}, \
                 got {other:?}"
            ),
        }
    }

    /// The pure decision core, d-spec92-hook-no-target-axis (spec 92's Design amended after
    /// round 5 to retire the guarded-tree apparatus): a `Bash` `grep` invocation is denied
    /// with the stated message NO MATTER WHAT it targets - a guarded tree from the old rule
    /// (`src/`), a tree that rule never covered (`docs/`), a single unrelated file
    /// (`README.md`), the whole-project convention (`.`), an ancestor (`..`), or an absolute
    /// path nowhere near this project - because the hook inspects only whether the command
    /// INVOKES grep, never what it points at. The SAME command with `--literal` passes for
    /// every one of those targets too, with the marker genuinely REMOVED from the command
    /// carried onward (spec 92's HOOK SCOPE amendment - reject-fix
    /// arch-u92c4r5-literal-escape-hatch-never-strips-marker: a bare `Allow` left the marker
    /// in place for a real shell to choke on, since GNU grep has no such flag).
    #[test]
    fn grep_guard_decision_denies_every_bash_grep_target_and_passes_literal() {
        for target in [
            "src/",
            "docs/",
            "README.md",
            ".",
            "..",
            "/unrelated/absolute/path",
        ] {
            let command = format!("grep -rn foo {target}");
            let decision = grep_guard_decision("Bash", &serde_json::json!({"command": command}));
            match decision {
                GuardDecision::Deny(msg) => assert_eq!(msg, GREP_GUARD_MESSAGE),
                other => panic!("grep targeting {target:?} must be denied, got {other:?}"),
            }

            let literal_command = format!("grep --literal -rn foo {target}");
            let decision =
                grep_guard_decision("Bash", &serde_json::json!({"command": literal_command}));
            let stripped = assert_allows_with_literal_stripped(decision, &literal_command);
            assert_eq!(
                stripped,
                format!("grep -rn foo {target}"),
                "the marker and exactly one adjacent space must be removed, nothing else \
                 rewritten, for target {target:?}"
            );
        }
    }

    /// A `Bash` command that is not a `grep` invocation at all - even one that merely
    /// mentions "grep" inside another word, like `zgrep` - is never bounced: the guard
    /// matches the literal command name only, per its own stated scope.
    #[test]
    fn grep_guard_decision_allows_non_grep_bash_commands() {
        assert_eq!(
            grep_guard_decision("Bash", &serde_json::json!({"command": "ls src/"})),
            GuardDecision::Allow
        );
        assert_eq!(
            grep_guard_decision(
                "Bash",
                &serde_json::json!({"command": "zgrep foo src/a.gz"}),
            ),
            GuardDecision::Allow,
            "zgrep is a different tool than the literal `grep` this hook names"
        );
    }

    /// A `tool` call whose input sets `key` to each of `values` is denied with the guard's own
    /// message (`why` says what the case proves).
    fn assert_grep_guard_denies(tool: &str, key: &str, values: &[&str], why: &str) {
        for value in values {
            assert_eq!(
                grep_guard_decision(tool, &serde_json::json!({ key: value })),
                GuardDecision::Deny(GREP_GUARD_MESSAGE.to_string()),
                "{why}: {value:?}"
            );
        }
    }

    rigger::test_cases! {
        /// The built-in `Grep` tool call is denied for EVERY `path` (d-spec92-hook-no-target-axis:
        /// no target axis survives - a guarded tree from the old rule, a tree it never covered,
        /// an omitted path defaulting to the cwd, an absolute path nowhere near this project),
        /// and carries no `--literal` escape hatch of its own - the built-in tool has no flag
        /// slot for it, so a genuinely literal search runs through `Bash` `grep --literal`
        /// instead.
        grep_guard_decision_denies_every_grep_tool_path: assert_grep_guard_denies(
            "Grep",
            "path",
            &[
                "src/",
                "docs/",
                "",
                "/unrelated/absolute/path",
                "..",
            ],
            "the hook has no target axis, so a Grep of any path must be denied",
        );
        /// Reject-fix (adj-u92c4r2-verdict-reject-shell-metachar-bypass /
        /// adv-u92c4r2-command-invokes-grep-tokenizes-on-whitespace-only): a `grep` invocation
        /// fused to an adjacent command with NO surrounding whitespace - via a pipe `|`, a
        /// semicolon `;`, an `&`, a `$( )` command substitution, or a backtick - must be denied
        /// exactly like the spaced form already is. Before that fix `command_invokes_grep` split
        /// on whitespace only, so a fused metacharacter hid the literal word `grep` from the scan
        /// entirely; this proof survives d-spec92-hook-no-target-axis unchanged, since detecting
        /// the invocation (not its target) is still exactly what the tokenizer must get right.
        grep_guard_decision_denies_a_shell_metacharacter_fused_grep: assert_grep_guard_denies(
            "Bash",
            "command",
            &[
                "cat src/main.rs|grep pattern",
                "true;grep pattern src/main.rs",
                "if $(grep -q pattern src/main.rs); then echo yes; fi",
                "grep pattern src/main.rs&",
                "echo hi&&grep pattern src/main.rs",
                "echo `grep pattern src/main.rs`",
            ],
            "a grep fused to an adjacent command via a shell metacharacter must still be denied",
        );
        /// Reject-fix (sdet-u92c4r5-redirect-metachar-fuses-guarded-path-first-segment): `<` and
        /// `>` must end a shell word exactly like `;`/`|`/`&`/`(`/`)` already do - spec 92's HOOK
        /// SCOPE amendment names `; | & ( ) < >` verbatim. This still matters after
        /// d-spec92-hook-no-target-axis: a redirection fused directly to the command name with no
        /// whitespace (`grep<file.txt`, a real shell equivalent of `grep <file.txt`) would
        /// otherwise merge into one word neither equal to nor ending in the bare basename `grep`,
        /// hiding the invocation from `command_invokes_grep` entirely - independent of what the
        /// command targets.
        grep_guard_decision_denies_a_redirect_metacharacter_fused_grep: assert_grep_guard_denies(
            "Bash",
            "command",
            &[
                "grep<file.txt pattern",
                "grep>out.txt pattern file.txt",
                "true;grep pattern <file.txt",
            ],
            "a grep fused to < or > with no surrounding whitespace must still be denied",
        );
        /// Reject-fix (adv-u92c4r3-quoted-or-escaped-grep-still-bypasses-the-guard): a `grep` word
        /// wrapped in double quotes, wrapped in single quotes, split by a backslash escape, or split
        /// by an EMPTY quoted run in the middle of the word - four ordinary shell idioms a real shell
        /// resolves to the plain word `grep`, none of them adversarial - must all still be denied.
        grep_guard_decision_denies_a_quoted_or_escaped_grep: assert_grep_guard_denies(
            "Bash",
            "command",
            &[
                r#""grep" pattern src/main.rs"#,
                "'grep' pattern src/main.rs",
                r"gr\ep pattern src/main.rs",
                "g''rep pattern src/main.rs",
            ],
            "a quoted or escaped grep must still be denied",
        );
        /// Reject-fix (adv-u92c4-r4-path-qualified-grep-bypasses-command-check): a path-qualified
        /// spelling of the same binary - `/usr/bin/grep`, `./grep`, a relative `bin/grep` - is the
        /// same command a bare `grep` names, so it must be denied exactly like the bare form already
        /// is.
        grep_guard_decision_denies_a_path_qualified_grep: assert_grep_guard_denies(
            "Bash",
            "command",
            &[
                "/usr/bin/grep pattern src/main.rs",
                "./grep pattern src/main.rs",
                "bin/grep pattern src/main.rs",
            ],
            "a path-qualified grep must still be denied",
        );
    }

    /// A tool other than `Grep`/`Bash` is never touched by this hook.
    #[test]
    fn grep_guard_decision_ignores_other_tools() {
        assert_eq!(
            grep_guard_decision("Read", &serde_json::json!({"file_path": "src/main.rs"})),
            GuardDecision::Allow
        );
    }

    /// `command` - a grep carrying the `--literal` escape hatch - passes the guard with the
    /// marker stripped.
    fn assert_literal_grep_passes(command: &str) {
        let decision = grep_guard_decision("Bash", &serde_json::json!({ "command": command }));
        assert_allows_with_literal_stripped(decision, command);
    }

    rigger::test_cases! {
        /// The SAME fused shapes with `--literal` added must still pass through (with the marker
        /// stripped), proving the escape hatch survives the tokenizer rather than becoming
        /// unreachable once fusion is detected.
        grep_guard_decision_literal_survives_a_shell_metacharacter_fused_grep: assert_literal_grep_passes(
            "true;grep --literal pattern src/main.rs",
        );
    }

    /// The same redirect-fused shape with `--literal` added must still pass through, with the
    /// redirection itself surviving the marker's removal untouched.
    #[test]
    fn grep_guard_decision_literal_survives_a_redirect_metacharacter_fused_grep() {
        let decision = grep_guard_decision(
            "Bash",
            &serde_json::json!({"command": "grep --literal pattern <src/main.rs"}),
        );
        let stripped =
            assert_allows_with_literal_stripped(decision, "grep --literal pattern <src/main.rs");
        assert_eq!(
            stripped, "grep pattern <src/main.rs",
            "only the marker and its one adjacent space must be removed"
        );
    }

    /// The same quoted/escaped shapes with a quoted `--literal` must still pass through, the
    /// entire quoted marker excised by its real span - proving the escape hatch itself
    /// survives quote/escape normalization AND marker stripping together.
    #[test]
    fn grep_guard_decision_literal_survives_a_quoted_literal_on_a_quoted_grep() {
        let decision = grep_guard_decision(
            "Bash",
            &serde_json::json!({"command": r#""grep" "--literal" pattern src/main.rs"#}),
        );
        let stripped = assert_allows_with_literal_stripped(
            decision,
            r#""grep" "--literal" pattern src/main.rs"#,
        );
        assert_eq!(
            stripped, r#""grep" pattern src/main.rs"#,
            "the whole quoted marker token must be excised, not merely its interior"
        );
    }

    /// Reject-fix (sdet-u92c4r4-backslash-newline-continuation-still-bypasses-the-guard): a
    /// backslash immediately followed by a newline is an ordinary shell line continuation - it
    /// vanishes with NO separator, joining what looks like two words into one, exactly as a real
    /// shell does before word splitting ever runs.
    #[test]
    fn grep_guard_decision_denies_a_grep_split_by_a_line_continuation() {
        assert_eq!(
            grep_guard_decision(
                "Bash",
                &serde_json::json!({"command": "gr\\\nep pattern src/main.rs"}),
            ),
            GuardDecision::Deny(GREP_GUARD_MESSAGE.to_string()),
            "a backslash-newline line continuation splitting `grep` must still be denied"
        );
    }

    rigger::test_cases! {
        /// The same line-continuation shape with `--literal` added must still pass through.
        grep_guard_decision_literal_survives_a_line_continuation_split_grep: assert_literal_grep_passes(
            "gr\\\nep --literal pattern src/main.rs",
        );
    }

    /// The same path-qualified shapes with `--literal` added must still pass through.
    #[test]
    fn grep_guard_decision_literal_survives_a_path_qualified_grep() {
        for command in [
            "/usr/bin/grep --literal pattern src/main.rs",
            "./grep --literal pattern src/main.rs",
            "bin/grep --literal pattern src/main.rs",
        ] {
            let decision = grep_guard_decision("Bash", &serde_json::json!({"command": command}));
            assert_allows_with_literal_stripped(decision, command);
        }
    }

    /// A path-qualified spelling of a DIFFERENT command (not `grep`) must still be allowed - the
    /// basename comparison must not become a substring match.
    #[test]
    fn grep_guard_decision_allows_a_path_qualified_non_grep_command() {
        assert_eq!(
            grep_guard_decision(
                "Bash",
                &serde_json::json!({"command": "/usr/bin/zgrep pattern src/main.rs"}),
            ),
            GuardDecision::Allow,
            "a path-qualified different command must not be treated as grep"
        );
    }

    // ---- rigger guard-write (spec 104 criterion 4, THE WRITE GUARD) ----

    #[test]
    fn guard_write_target_reads_edit_and_write_file_path() {
        for tool in ["Edit", "Write"] {
            assert_eq!(
                guard_write_target(tool, &serde_json::json!({"file_path": "src/main.rs"})),
                WriteTarget::Target("src/main.rs".to_string()),
                "{tool} must read tool_input.file_path"
            );
        }
    }

    #[test]
    fn guard_write_target_reads_notebook_edit_notebook_path() {
        assert_eq!(
            guard_write_target(
                "NotebookEdit",
                &serde_json::json!({"notebook_path": "nb.ipynb"})
            ),
            WriteTarget::Target("nb.ipynb".to_string())
        );
    }

    #[test]
    fn guard_write_target_ignores_every_other_tool() {
        for tool in ["Read", "Bash", "Grep", ""] {
            assert_eq!(
                guard_write_target(tool, &serde_json::json!({"file_path": "src/main.rs"})),
                WriteTarget::NotCovered,
                "{tool:?} carries no target this guard covers"
            );
        }
    }

    /// The defect this type exists to close: a COVERED tool name must never fold a
    /// missing or non-string field into the SAME outcome as a tool this guard does not
    /// cover - each is a distinct, distinguishable `WriteTarget` variant.
    #[test]
    fn guard_write_target_reports_target_unreadable_for_a_covered_tool_missing_the_field() {
        for tool in ["Edit", "Write", "NotebookEdit"] {
            assert_eq!(
                guard_write_target(tool, &serde_json::json!({})),
                WriteTarget::TargetUnreadable,
                "{tool} with no path field at all must be TargetUnreadable, never NotCovered \
                 or a silently-allowed target"
            );
        }
    }

    #[test]
    fn guard_write_target_reports_target_unreadable_for_a_non_string_field() {
        assert_eq!(
            guard_write_target("Write", &serde_json::json!({"file_path": 42})),
            WriteTarget::TargetUnreadable,
            "a non-string file_path must be TargetUnreadable, never read as if absent-and-allowed"
        );
        assert_eq!(
            guard_write_target(
                "NotebookEdit",
                &serde_json::json!({"notebook_path": ["a", "b"]})
            ),
            WriteTarget::TargetUnreadable,
            "a non-string notebook_path must be TargetUnreadable"
        );
    }

    // ---- guard_write_read_target (THE GUARD DENIES BY DEFAULT,
    // op-104-write-guard-deny-by-default-complete-audit): the payload-shape axis
    // guard_write_target itself never sees - everything OTHER than a JSON object whose
    // tool_name is a real string must deny (TargetUnreadable), never fall through to
    // guard_write_target's own NotCovered = Allow arm. ----

    #[test]
    fn guard_write_read_target_denies_a_non_object_top_level_value() {
        for payload in [
            serde_json::json!([1, 2, 3]),
            serde_json::json!(null),
            serde_json::json!("hello"),
            serde_json::json!(42),
            serde_json::json!(true),
        ] {
            assert_eq!(
                guard_write_read_target(&payload),
                (WriteTarget::TargetUnreadable, String::new()),
                "a top-level JSON value that parses but is not an object must be \
                 TargetUnreadable, never silently defaulted into NotCovered/Allow: {payload}"
            );
        }
    }

    #[test]
    fn guard_write_read_target_denies_an_absent_or_non_string_tool_name() {
        // The returned cwd is irrelevant once the target itself is TargetUnreadable -
        // guard_write_decision never resolves a path for that arm - so this asserts only
        // the WriteTarget half against a `..` matcher, never the exact cwd string.
        assert!(
            matches!(
                guard_write_read_target(&serde_json::json!({
                    "cwd": "/root",
                    "tool_input": {"file_path": "/outside/x.txt"},
                })),
                (WriteTarget::TargetUnreadable, _)
            ),
            "an absent tool_name must be TargetUnreadable, never defaulted to \"\" and read \
            as a real (uncovered) tool"
        );
        assert!(
            matches!(
                guard_write_read_target(&serde_json::json!({
                    "tool_name": 42,
                    "cwd": "/root",
                    "tool_input": {"file_path": "/outside/x.txt"},
                })),
                (WriteTarget::TargetUnreadable, _)
            ),
            "a non-string tool_name must be TargetUnreadable"
        );
    }

    #[test]
    fn guard_write_read_target_reaches_not_covered_only_through_a_real_string_tool_name() {
        assert_eq!(
            guard_write_read_target(&serde_json::json!({
                "tool_name": "Bash",
                "cwd": "/root",
                "tool_input": {"command": "ls"},
            })),
            (WriteTarget::NotCovered, "/root".to_string()),
            "a genuine string tool_name outside Edit/Write/NotebookEdit is the ONE door to \
             NotCovered"
        );
    }

    #[test]
    fn guard_write_read_target_denies_a_covered_tool_with_missing_or_non_object_tool_input() {
        assert_eq!(
            guard_write_read_target(&serde_json::json!({"tool_name": "Write", "cwd": "/root"})),
            (WriteTarget::TargetUnreadable, "/root".to_string()),
            "tool_input absent entirely for a covered tool must be TargetUnreadable"
        );
        assert_eq!(
            guard_write_read_target(&serde_json::json!({
                "tool_name": "Write",
                "cwd": "/root",
                "tool_input": "not-an-object",
            })),
            (WriteTarget::TargetUnreadable, "/root".to_string()),
            "tool_input present but not a JSON object must be TargetUnreadable"
        );
    }

    #[test]
    fn guard_write_read_target_reads_a_well_formed_covered_payload() {
        assert_eq!(
            guard_write_read_target(&serde_json::json!({
                "tool_name": "Write",
                "cwd": "/root",
                "tool_input": {"file_path": "notes.txt"},
            })),
            (
                WriteTarget::Target("notes.txt".to_string()),
                "/root".to_string()
            )
        );
    }

    #[test]
    fn guard_write_read_target_defaults_an_absent_cwd_to_empty() {
        assert_eq!(
            guard_write_read_target(&serde_json::json!({
                "tool_name": "Write",
                "tool_input": {"file_path": "notes.txt"},
            })),
            (WriteTarget::Target("notes.txt".to_string()), String::new())
        );
    }

    #[test]
    fn resolve_write_target_passes_an_absolute_path_through() {
        let dir = tempfile::tempdir().unwrap();
        // Canonicalize the fixture's own path up front: `resolve_write_target` resolves
        // symlinks in whatever ancestors already exist, so if the OS temp dir itself is
        // reached through one, the un-canonicalized `dir.path()` would not textually match
        // the function's own (correct) output - an environment quirk, not a logic bug.
        let real = std::fs::canonicalize(dir.path()).unwrap();
        let want = real.join("f.txt");
        assert_eq!(
            resolve_write_target("/somewhere/else", want.to_str().unwrap()),
            want
        );
    }

    /// `raw`, resolved against a real tempdir cwd, lands on `expected` under that cwd.
    fn assert_resolves_under_cwd(raw: &str, expected: &str) {
        let dir = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap();
        let cwd = real.to_str().unwrap();
        assert_eq!(resolve_write_target(cwd, raw), real.join(expected));
    }

    rigger::test_cases! {
        resolve_write_target_joins_a_relative_path_onto_cwd: assert_resolves_under_cwd("sub/f.txt", "sub/f.txt");
        resolve_write_target_normalizes_dot_and_dot_dot: assert_resolves_under_cwd("./a/../b.txt", "b.txt");
    }

    #[test]
    fn resolve_write_target_walks_dot_dot_past_the_process_root_without_panicking() {
        // A relative target with more `..` than the resolved path has components must not
        // panic (`PathBuf::pop()` is a documented no-op once there is no parent) - it
        // bottoms out at the filesystem root and keeps resolving from there. Synthetic,
        // guaranteed-nonexistent names so no real symlink on the test machine can interfere
        // with the assertion.
        let resolved =
            resolve_write_target("/", "../../../nonexistent-guard-write-probe-dir/leaf.txt");
        assert_eq!(
            resolved,
            Path::new("/nonexistent-guard-write-probe-dir/leaf.txt")
        );
    }

    #[test]
    fn resolve_write_target_follows_a_symlinked_ancestor_directory() {
        // A root-relative `..` that walks back out through an ancestor symlink must land
        // where the symlink actually points, not where the raw text suggests.
        let base = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspace = base.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let link = workspace.join("escape");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();

        let resolved = resolve_write_target(workspace.to_str().unwrap(), "escape/secret.txt");
        let want = std::fs::canonicalize(outside.path())
            .unwrap()
            .join("secret.txt");
        assert_eq!(
            resolved, want,
            "must resolve through the symlink, landing outside workspace"
        );
    }

    #[test]
    fn write_target_under_root_rejects_a_sibling_whose_name_merely_shares_a_prefix() {
        // A component-aware comparison, not a raw string prefix: "/root-extra" must NOT
        // count as under "/root".
        let dir = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap();
        let mut sibling_name = real.file_name().unwrap().to_os_string();
        sibling_name.push("-extra");
        let sibling = real.with_file_name(sibling_name);
        assert!(
            !write_target_under_root(&sibling, real.to_str().unwrap()),
            "a sibling whose name merely starts with the root's own last component must \
             not be treated as inside it"
        );
    }

    #[test]
    fn write_target_under_root_treats_equal_as_under() {
        let dir = tempfile::tempdir().unwrap();
        // Canonicalize the fixture path itself (not just what the function resolves) so
        // this assertion is robust on a machine where the OS temp dir is itself reached
        // through a symlink - both sides of the comparison must start from the same real
        // path, or the test would fail on environment quirks unrelated to the guard's logic.
        let real = std::fs::canonicalize(dir.path()).unwrap();
        assert!(write_target_under_root(&real, real.to_str().unwrap()));
    }

    #[test]
    fn write_target_under_root_accepts_a_descendant_and_rejects_a_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap();
        let root = real.to_str().unwrap();
        assert!(write_target_under_root(&real.join("a/b.txt"), root));

        let sibling = tempfile::tempdir().unwrap();
        let sibling_real = std::fs::canonicalize(sibling.path()).unwrap();
        assert!(!write_target_under_root(&sibling_real.join("b.txt"), root));
    }

    #[test]
    fn guard_write_decision_allows_a_target_under_any_configured_root() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let roots = vec![
            first.path().to_str().unwrap().to_string(),
            second.path().to_str().unwrap().to_string(),
        ];
        let target = second.path().join("scratch/out.txt");
        assert_eq!(
            guard_write_decision(
                &roots,
                "/irrelevant",
                WriteTarget::Target(target.to_str().unwrap().to_string())
            ),
            GuardDecision::Allow,
            "a target under the SECOND root must still be allowed"
        );
    }

    #[test]
    fn guard_write_decision_denies_outside_every_root_naming_the_first() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let roots = vec![
            first.path().to_str().unwrap().to_string(),
            second.path().to_str().unwrap().to_string(),
        ];
        let target = outside.path().join("f.txt");
        let decision = guard_write_decision(
            &roots,
            "/irrelevant",
            WriteTarget::Target(target.to_str().unwrap().to_string()),
        );
        assert_eq!(
            decision,
            GuardDecision::Deny(format!(
                "write target is outside the allowed root: {}",
                first.path().to_str().unwrap()
            ))
        );
    }

    #[test]
    fn guard_write_decision_denies_a_dot_dot_escape() {
        let root = tempfile::tempdir().unwrap();
        let inside = root.path().join("inside");
        std::fs::create_dir_all(&inside).unwrap();
        let roots = vec![root.path().to_str().unwrap().to_string()];
        let decision = guard_write_decision(
            &roots,
            inside.to_str().unwrap(),
            WriteTarget::Target("../../outside.txt".to_string()),
        );
        assert!(matches!(decision, GuardDecision::Deny(_)), "{decision:?}");
    }

    #[test]
    fn guard_write_decision_allows_a_tool_this_guard_does_not_cover() {
        let roots = vec!["/some/root".to_string()];
        assert_eq!(
            guard_write_decision(&roots, "/cwd", WriteTarget::NotCovered),
            GuardDecision::Allow
        );
    }

    /// The fix this criterion's prior round was rejected for missing: a covered tool whose
    /// target this guard could not read must be DENIED, naming the first root exactly as a
    /// resolved out-of-root target is - never folded into the same Allow as `NotCovered`.
    #[test]
    fn guard_write_decision_denies_target_unreadable_naming_the_first_root() {
        let roots = vec!["/first/root".to_string(), "/second/root".to_string()];
        assert_eq!(
            guard_write_decision(&roots, "/cwd", WriteTarget::TargetUnreadable),
            GuardDecision::Deny(
                "write target is outside the allowed root: /first/root".to_string()
            ),
            "an unreadable target for a covered tool must deny naming the FIRST root, the \
             fail-safe default, never allow"
        );
    }

    #[test]
    fn parse_guard_write_roots_collects_every_root_in_order() {
        let args: Vec<String> = ["--root", "/a", "--root", "/b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            parse_guard_write_roots(&args).unwrap(),
            vec!["/a".to_string(), "/b".to_string()]
        );
    }

    #[test]
    fn parse_guard_write_roots_requires_at_least_one() {
        assert!(parse_guard_write_roots(&[]).is_err());
    }

    #[test]
    fn parse_guard_write_roots_rejects_a_dangling_flag() {
        let args = vec!["--root".to_string()];
        assert!(parse_guard_write_roots(&args).is_err());
    }

    #[test]
    fn parse_guard_write_roots_rejects_an_unknown_argument() {
        let args = vec!["--spawn".to_string(), "x".to_string()];
        assert!(parse_guard_write_roots(&args).is_err());
    }
}
