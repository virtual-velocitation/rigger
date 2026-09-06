//! Spec 67, criterion 2 (ROWS LEAD WITH THE PERSONA) - the driver's per-worker display label.
//!
//! `src/main.rs`'s own doc-only assertions (if any) would only ever pin the SOURCE TEXT of the
//! label builder; they cannot prove the function actually RUNS correctly (a broken template
//! literal, a wrong regex capture, an off-by-one splitting the role from the attempt). The whole
//! `workflows/rigger.js` file cannot execute outside the workflow harness (top-level await, the
//! injected `agent`/`parallel`/`log` globals - see `tests/cli.rs::rigger_js_source`'s own doc
//! comment), but the label builder this criterion adds depends on NO injected global at all - it
//! is a pure function of its `req` argument. So, exactly like `tests/step_attention_periphery.rs`
//! does for `relayAttention`, this file extracts the real declarations VERBATIM from the shipped
//! source - never hand-copied, so a future edit is exercised here without a separate update - and
//! runs them for real under `node`.

use std::path::Path;
use std::process::Command;

/// Extract a top-level declaration - `function <name>(...) { ... }` or `const <NAME> = { ... }` -
/// VERBATIM from `start_marker` through its brace-matched close, inclusive. The same brace-
/// counting `tests/step_attention_periphery.rs::js_declaration` uses (this file's own copy, per
/// the established per-file duplication convention that file's header documents).
fn js_declaration<'a>(src: &'a str, start_marker: &str) -> &'a str {
    let start = src
        .find(start_marker)
        .unwrap_or_else(|| panic!("workflow must contain `{start_marker}`"));
    let open = start
        + src[start..]
            .find('{')
            .expect("declaration must open a brace");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("`{start_marker}` is not brace-balanced");
}

/// Read `workflows/rigger.js` at test time from the crate manifest dir - mirrors `tests/
/// cli.rs`'s and `tests/step_attention_periphery.rs`'s identical `rigger_js_source` helper (the
/// established per-file duplication convention).
fn rigger_js_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join("rigger.js");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Run the REAL `workerLabel(req)` - extracted verbatim from the shipped `workflows/rigger.js`
/// along with the `PERSONA_VERB` table and the `personaOf`/`firstSentence` helpers it calls -
/// under a real `node` subprocess. `workerLabel` depends on no injected global (no `log`, no
/// `agent`), so nothing needs stubbing. Returns the rendered label, or `None` when `node` is not
/// on PATH - the same graceful-absence contract `tests/step_attention_periphery.rs::
/// run_relay_attention` and `src/main.rs`'s own `node --check` test already establish for this
/// crate (missing node is an environment fact, never a test failure).
fn run_worker_label(id: &str, title: &str) -> Option<String> {
    run_worker_label_for_unit(id, title, None)
}

/// Like [`run_worker_label`], but also stamps `req.unit` - the field `workerLabel` reads to
/// derive the `Plan`/`Plan-Critique` structural persona for the two run-wide meta-stage spawns
/// (their role half is always an ordinary role - `implementer`/`replan`, `adversary`/
/// `adjudicator` - so it can never itself carry that persona; see `workerLabel`'s own doc
/// comment in `workflows/rigger.js`). `None` omits the field entirely, matching an ordinary
/// build unit's wave item.
fn run_worker_label_for_unit(id: &str, title: &str, unit: Option<&str>) -> Option<String> {
    let src = rigger_js_source();
    let verb_table = js_declaration(&src, "const PERSONA_VERB = {");
    let persona_of = js_declaration(&src, "function personaOf(role) {");
    let first_sentence = js_declaration(&src, "function firstSentence(s) {");
    let role_attempt = js_declaration(&src, "function roleAttempt(id) {");
    let worker_label = js_declaration(&src, "function workerLabel(req) {");

    let mut req = serde_json::json!({ "id": id, "title": title });
    if let Some(unit) = unit {
        req["unit"] = serde_json::Value::String(unit.to_string());
    }
    let req = req.to_string();
    let script = format!(
        "{verb_table}\n{persona_of}\n{first_sentence}\n{role_attempt}\n{worker_label}\n\
         process.stdout.write(workerLabel(JSON.parse(process.argv[2])) + '\\n')\n"
    );

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, script.as_bytes()).unwrap();

    match Command::new(&node).arg(f.path()).arg(&req).output() {
        Ok(out) => {
            assert!(
                out.status.success(),
                "the real workerLabel must run without throwing on {req}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            Some(text.trim_end_matches('\n').to_string())
        }
        Err(e) => {
            assert!(
                e.kind() == std::io::ErrorKind::NotFound,
                "node failed for a reason other than being absent: {e}"
            );
            None
        }
    }
}

/// Two different tiers of the SAME unit render DISTINCT persona tokens AND distinct action
/// phrases - the exact defect a shared `${req.unit}:${req.stage}`-shaped label (or a single
/// hardcoded verb) would hide, since both tiers share the same unit id. Also covers the base
/// format for a mapped, non-review role: `<Persona> - <action phrase> #<attempt>: <subject>`.
#[test]
fn two_tiers_of_the_same_unit_render_distinct_persona_and_verb() {
    let implementer = run_worker_label(
        "u1/implementer#0",
        "implement the thing. This criterion OWNS nothing in particular.",
    );
    let Some(implementer) = implementer else {
        return; // node unavailable; graceful absence, mirrored from src/main.rs's own gate.
    };
    assert_eq!(
        implementer, "Implementer - implement #0: implement the thing.",
        "a mapped role must render <Persona> - <verb> #<attempt>: <first sentence>"
    );

    let lens = run_worker_label(
        "u1/lens:sdet#2",
        "evaluate whether the tests are discriminating. Some other sentence.",
    )
    .expect("node already proven available above");
    assert_eq!(
        lens,
        "Lens:SDET - evaluate testing effectiveness #2: evaluate whether the tests are discriminating."
    );

    // SAME unit (u1), a different tier: distinct persona token AND distinct action phrase from
    // the implementer tier above.
    assert_ne!(
        implementer.split(" - ").next(),
        lens.split(" - ").next(),
        "the two tiers of unit u1 must render distinct persona tokens"
    );
    assert_ne!(
        implementer.split(" - ").nth(1),
        lens.split(" - ").nth(1),
        "the two tiers of unit u1 must render distinct action phrases"
    );
}

/// The full documented role map, one persona+verb pair per role, including the two roles whose
/// action phrase renders WITHOUT a roster clause (spec 67 criterion 4 owns adding that
/// parenthetical once the conductor stamps `req.reviews`; this unit never reads that field).
#[test]
fn every_documented_role_maps_to_its_own_persona_and_action_phrase() {
    // Every id/unit pair here is a REAL production shape - never a fabricated one a real
    // conductor construction path could not mint (src/conductor.rs:4139, 6080, 6265, 6280;
    // src/spawn.rs:1713's own test fixture). In particular `plan`/`plan-critique` are unit ids,
    // never role tokens - no real spawn's role half is ever literally "plan" or "plan-critique" -
    // so their persona comes from the structural `req.unit` override, not from a role mapping.
    let cases = [
        ("u2/implementer#0", None, "Implementer - implement"),
        (
            "u2/sdet-author#0",
            None,
            "SDET-Author - author the discriminating tests for",
        ),
        (
            "u2/lens:sdet#0",
            None,
            "Lens:SDET - evaluate testing effectiveness",
        ),
        (
            "u2/lens:architecture-reviewer#0",
            None,
            "Lens:Architecture-Reviewer - evaluate architectural integrity",
        ),
        (
            "u2/adversary#1",
            None,
            "Adversary - challenge the findings, assumptions, and rigor",
        ),
        ("u2/adjudicator#1", None, "Adjudicator - weigh and rule"),
        (
            // The planning unit's own producer spawn (conductor.rs:4139): a plain ROLE_IMPLEMENTER
            // under unit "plan" - the persona still reads "Plan", never "Implementer".
            "plan/implementer#0",
            Some("plan"),
            "Plan - implement",
        ),
        (
            // A re-plan respawn after a plan-critique reject (conductor.rs:6080): role "replan",
            // still under unit "plan", with its OWN verb rather than the generic fallback.
            "plan/replan#1",
            Some("plan"),
            "Plan - revise the unit DAG from the critique feedback",
        ),
        (
            // The plan-critique gate's tier-2 spawn (conductor.rs:6265): an ordinary "adversary"
            // role under unit "plan-critique" - the persona still reads "Plan-Critique".
            "plan-critique/adversary#0",
            Some("plan-critique"),
            "Plan-Critique - challenge the findings, assumptions, and rigor",
        ),
        (
            // The plan-critique gate's tier-3 spawn (conductor.rs:6280): an ordinary "adjudicator"
            // role under unit "plan-critique".
            "plan-critique/adjudicator#0",
            Some("plan-critique"),
            "Plan-Critique - weigh and rule",
        ),
    ];
    for (id, unit, want_prefix) in cases {
        let Some(label) = run_worker_label_for_unit(id, "some criterion sentence.", unit) else {
            return;
        };
        assert!(
            label.starts_with(want_prefix),
            "{id} (unit {unit:?}) must render a label starting with {want_prefix:?}; got {label:?}"
        );
        assert!(
            !label.contains("roster") && !label.contains("<roster>"),
            "this unit must never render a roster placeholder or literal (c4's job); got {label:?}"
        );
    }
}

/// A Gap-18 reviewer RESPAWN id (spec 07, `src/spawn.rs::spawn_retry_id`'s own
/// `{unit}/{role}#{attempt}~retry{n}` shape) still renders the FULL persona-led label, not the
/// pre-criterion `${req.id}: ${subject}` fallback - the exact regression a `$`-anchored
/// digit-only regex produced (it demands the id END in digits, so it never matches a
/// `~retry{n}`-suffixed id). Gap-18 respawns are a normal, frequently-exercised path for
/// lens/adversary/adjudicator spawns whose result comes back empty or whitespace-only, so this is
/// the opposite of a corner case.
#[test]
fn a_gap_18_respawn_id_still_renders_the_full_persona_led_label() {
    let Some(label) = run_worker_label(
        "u1/adjudicator#1~retry2",
        "weigh and rule on the merged tree.",
    ) else {
        return;
    };
    assert_eq!(
        label, "Adjudicator - weigh and rule #1: weigh and rule on the merged tree.",
        "a ~retry{{n}}-suffixed id must render the persona-led label, using the ATTEMPT ordinal \
         (matching src/spawn.rs::attempt_of), never fall back to the pre-criterion slug format"
    );
}

/// The retry suffix rides the ROLE half too (a Gap-18 respawn of a plan-critique reviewer) -
/// proving the structural `Plan-Critique` persona override composes with the retry-aware parser,
/// not just the base id shape covered above.
#[test]
fn a_gap_18_respawn_of_a_plan_critique_reviewer_keeps_its_structural_persona() {
    let Some(label) = run_worker_label_for_unit(
        "plan-critique/adversary#0~retry2",
        "challenge the revised decomposition.",
        Some("plan-critique"),
    ) else {
        return;
    };
    assert_eq!(
        label,
        "Plan-Critique - challenge the findings, assumptions, and rigor #0: challenge the \
         revised decomposition."
    );
}

/// An UNMAPPED custom lens keeps its OWN persona token, readable, with the generic `review` verb
/// - never a slug, never dropped to a bare id.
#[test]
fn an_unmapped_custom_lens_keeps_its_persona_token_with_the_generic_review_verb() {
    let Some(label) = run_worker_label("u3/lens:custom-checker#0", "check something unusual.")
    else {
        return;
    };
    assert_eq!(
        label,
        "Lens:Custom-Checker - review #0: check something unusual."
    );
}

/// An untitled item (no `req.title`, or one that is empty after whitespace-normalization) falls
/// back to `req.id`, exactly like the pre-existing fallback this unit must not regress.
#[test]
fn an_untitled_item_falls_back_to_the_spawn_id() {
    for title in ["", "   \n\t  "] {
        let Some(label) = run_worker_label("u4/implementer#0", title) else {
            return;
        };
        assert_eq!(label, "u4/implementer#0");
    }
}

/// The subject is the title's FIRST SENTENCE, whitespace-normalized, passed WHOLE - no
/// driver-side slice or ellipsis, even when the title carries several further sentences (a
/// realistic multi-sentence criterion, exactly like this unit's own recorded title).
#[test]
fn the_subject_is_the_titles_first_sentence_passed_whole_with_no_truncation() {
    let title = "a test proves ROWS LEAD WITH THE PERSONA: a titled wave item's label renders \
                 the documented shape (persona title-cased from the role half; subject is the \
                 title's first sentence, whitespace-normalized, passed whole with no \
                 driver-side truncation or ellipsis), two different tiers of the SAME unit \
                 render distinct persona tokens AND distinct action phrases, an unmapped \
                 custom lens keeps its persona token with the generic \"review:\" verb, and an \
                 untitled item falls back to the spawn id. This criterion OWNS the label.";
    let Some(label) = run_worker_label("u5/implementer#0", title) else {
        return;
    };
    assert_eq!(
        label,
        "Implementer - implement #0: a test proves ROWS LEAD WITH THE PERSONA: a titled wave \
         item's label renders the documented shape (persona title-cased from the role half; \
         subject is the title's first sentence, whitespace-normalized, passed whole with no \
         driver-side truncation or ellipsis), two different tiers of the SAME unit render \
         distinct persona tokens AND distinct action phrases, an unmapped custom lens keeps \
         its persona token with the generic \"review:\" verb, and an untitled item falls back \
         to the spawn id.",
        "the subject must be the FULL first sentence, whitespace-normalized, never truncated"
    );
    assert!(
        !label.contains("This criterion OWNS"),
        "a second sentence must never leak into the rendered subject; got {label:?}"
    );
}

/// Internal whitespace (newlines, tabs, runs of spaces) in the title is collapsed to single
/// spaces before the sentence is cut - the same normalization the pre-existing `work` line
/// already applied, now feeding the persona-led builder instead of the raw `${req.id} · ${work}`
/// concatenation.
#[test]
fn internal_whitespace_is_normalized_before_the_sentence_is_cut() {
    let Some(label) = run_worker_label(
        "u6/adjudicator#3",
        "  weigh   the\n\tfindings and   rule.   second sentence.  ",
    ) else {
        return;
    };
    assert_eq!(
        label,
        "Adjudicator - weigh and rule #3: weigh the findings and rule."
    );
}
