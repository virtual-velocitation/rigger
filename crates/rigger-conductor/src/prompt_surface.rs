//! The conductor's and the canary's prompt texts (spec 121, criterion 16), each one `pub const`
//! whose doc comment opens with its class: the decision and finding protocols, the planner's
//! refine protocol, the adjudicator's verdict contract, the retry, review-round and gate-evidence
//! blocks, the grounding sections and their elision notes, the DAG critique and re-plan prompts,
//! and the canary's review and adjudicator prompts. A renderer in `conductor.rs` or
//! `canary_store.rs` formats from these through `fill` wherever a const carries a `{slot}`. The
//! registry types and `fill` are `rigger_domain::prompt_surface`'s. The module holds the consts
//! and their registry, [`prompt_groups`], both pure, so it compiles in every lane.

/// CONTRACT: how an agent records a decision, the `rigger_emit` call and the `DecisionMade` JSON
/// shape the conductor reads; every worker prompt carries it.
pub const EMIT_PROTOCOL: &str = "Record each decision you make by calling the rigger_emit tool \
     with type \"DecisionMade\" and data:\n{\"id\":\"<short-id>\",\"summary\":\"<one \
     line>\",\"governs\":[\"<file>\"],\"supersedes\":\"<prior-id-or-empty>\"}\n";

/// FRAMING: what a recorded decision does, after [`EMIT_PROTOCOL`].
pub const EMIT_LIVE: &str =
    "This writes it to the shared event log live, so other agents see it immediately.";

/// FRAMING: the planner's role and its refine-not-re-decompose rule, `{unit_size_cap}` holding
/// [`UNIT_SIZE_CAP`] and a space when the workflow declares a cap, else nothing.
pub const PLAN_ROLE: &str = "You are the planner. The conductor has ALREADY created one baseline \
     implement unit per acceptance criterion below - the spec is decomposed by construction. \
     Your job is to REFINE that baseline, not to re-decompose it. {unit_size_cap}";

/// CONTRACT: how a split part and a refinement name the criterion they serve, the id the
/// conductor pairs a unit with its baseline by.
pub const PLAN_SPLIT_ID_ECHO: &str = "Each part of a split echoes the split criterion's id and \
     ends its `criterion` text with an OWNS sentence naming its part (the conductor keeps that \
     sentence after the criterion's exact text as that unit's contract).\nEach criterion below \
     is shown with a STABLE id in [brackets]. When your unit serves a criterion, your unit \
     SUPERSEDES (replaces) that criterion's baseline - it does NOT run alongside it - so identify \
     the criterion you serve by ECHOING its id: copy the id shown in brackets next to that \
     criterion into the `criterion_id` field (the brackets are display delimiters - the \
     conductor accepts the id with or without them). ";

/// FRAMING: what the conductor does with the echoed id and what a wrong one costs.
pub const PLAN_PAIRING: &str = "The conductor pairs your unit with its baseline by that id, so \
     even if you reword or truncate the criterion text in `criterion`, the correct id still \
     supersedes the one baseline (no duplicate). A wrong or missing `criterion_id` is what makes \
     your unit run as an EXTRA unit on top of the baseline (duplicated work). ";

/// CONTRACT: the criterion text a proposal still carries in its `criterion` field.
pub const PLAN_COPY_TEXT: &str =
    "Still copy the criterion text into `criterion` (verbatim is safest). ";

/// FRAMING: what several units echoing one id do.
pub const PLAN_REAL_SPLIT: &str =
    "Several units echoing the SAME id (a real split) all run and replace the one baseline.\n";

/// CONTRACT: how the planner proposes a unit, the `rigger_emit` call and the `UnitProposed` JSON
/// shape the conductor harvests, `{implementer}` holding the implementer agent id.
pub const PLAN_UNIT_PROPOSED: &str = "Propose each refinement by calling the rigger_emit tool \
     with type \"UnitProposed\" and data:\n{\"id\":\"<short-id>\",\"agent\":\"{implementer}\",\
     \"criterion\":\"<the spec criterion it serves>\",\"criterion_id\":\"<the id shown in \
     [brackets] next to that criterion>\",\"needs\":[\"<unit ids it depends on>\"]}\n";

/// FRAMING: what an unmatched id does, and that every proposed unit runs the fan-out template's
/// gates.
pub const PLAN_UNMATCHED: &str = "A unit whose `criterion_id` is none of the ids below still \
     runs, but as a genuinely-new sub-unit that the conductor flags as unmatched. Every unit you \
     propose - a refinement, a split, or a new sub-unit - automatically runs the fan-out \
     template's own gates; you never need to name them.\n\n";

/// CONTRACT: the acceptance criteria a proposal echoes, `{criteria}` holding one `- [{id}]
/// {text}` line per criterion, or [`PLAN_NO_CRITERIA`] when the spec enumerates none.
pub const PLAN_CRITERIA_LEAD: &str = "The acceptance criteria to refine against (echo the [id] \
     shown next to each criterion into that unit's `criterion_id`, and copy the text into \
     `criterion`):\n{criteria}";

/// FRAMING: what fills [`PLAN_CRITERIA_LEAD`]'s `{criteria}` when the spec enumerates no
/// criterion.
pub const PLAN_NO_CRITERIA: &str = "(no enumerated acceptance criteria)";

/// FRAMING: the unit size cap of `{cap}` lines, printed by the planner's refine protocol and the
/// DAG critique prompt only when the workflow declares `defaults.max_unit_added_lines`.
pub const UNIT_SIZE_CAP: &str =
    "A unit is too large when it is expected to add more than {cap} lines.";

/// CONTRACT: the `upheld` list an adjudicator's verdict line carries and its shape.
pub const UPHELD_PROTOCOL: &str = " Your verdict line lists as \"upheld\":[\"<finding id>\"] \
     every finding you agree is real, a note as well as a blocking one:";

/// FRAMING: what the upheld notes become, closing [`UPHELD_PROTOCOL`]'s sentence.
pub const UPHELD_NOTES: &str = " the upheld notes are the run's notes, fixed once by the \
     workflow's check-in stage where it has one.";

/// FRAMING: the run notes block's header, printed to the check-in stage only when the run's
/// review upheld a note.
pub const RUN_NOTES_HEADER: &str = "\n\nRUN NOTES: the review of this run upheld these findings:\n";

/// FRAMING: what the check-in stage does with the run notes, closing their block.
pub const RUN_NOTES_CLOSING: &str =
    "Fix every note that is still true; leave a note that is false or already fixed.\n";

/// FRAMING: a re-run's task after the emit guard refused its record: the role's `{prompt}`,
/// then the refusal, `{refusal}` holding the emit tool's recorded refusal.
pub const REFUSED_RECORD_NOTE: &str =
    "{prompt}\n\nYour previous run's record was refused: {refusal}; emit it again correctly.";

/// CONTRACT: the `required` list an adjudicator's reject carries on its verdict line and its
/// shape.
pub const REQUIRED_PROTOCOL: &str = " On a reject, your verdict line also lists every item the \
     next attempt must fix, as \"required\":[{\"finding\":\"<what must change>\",\"path\":\"<the \
     repo-relative file it is in>\",\"correctness\":<true for a correctness defect, else \
     false>,\"pattern\":\"<the defect's shape when it recurs across sites, else empty>\"}].";

/// FRAMING: what the harness enforces of every review tier, a lens, the adversary and the
/// adjudicator: the reviewed worktree stays as the reviewer found it.
pub const REVIEWER_DISCIPLINE: &str = " Never write to this unit's own worktree - it is the tree \
     being judged, not yours to edit. Leave the unit's worktree exactly as you found it.";

/// FRAMING: the build cache a reviewer's reproduction builds into, printed only when the
/// reviewer's spawn carries the unit's review cache.
pub const REVIEWER_BUILD_CACHE: &str = " A reproduction builds into the build cache your spawn \
     carries: the unit's review cache, never the cache the unit's gates use.";

/// CONTRACT: how a lens or the adversary records a finding, the `rigger_emit` call, the
/// `ReviewFinding` JSON shape and the `by` attribution, `{actor}` holding the reviewer's role.
pub const REVIEW_FINDING_PROTOCOL: &str = "Record each review finding you raise by calling the \
     rigger_emit tool with type \"ReviewFinding\" and data:\n{\"id\":\"<short-id>\",\"by\":\
     \"{actor}\",\"summary\":\"<one line>\",\"about\":[\"<file>\"]}\nThe `by` field ATTRIBUTES \
     the finding to you - keep it EXACTLY as \"{actor}\" so the review-quality metrics can \
     measure your findings' survival even when you run out-of-process (where the conductor \
     stamps no actor for you). ";

/// FRAMING: what a recorded finding does, after [`REVIEW_FINDING_PROTOCOL`].
pub const REVIEW_FINDING_LIVE: &str = "This writes the finding to the shared context graph live, \
     so the adversary, the adjudicator, and your fellow reviewers see it immediately (via \
     grounding and rigger_peers).";

/// FRAMING: the code-neighborhood grounding section's header, printed only when the section has
/// a definition.
pub const CODE_NEIGHBORHOOD_HEADER: &str =
    "Code neighborhood of these files (definitions the run extracted into the unified graph):";

/// FRAMING: the design-intent grounding section's header, printed only when the section has a
/// node.
pub const DESIGN_INTENT_HEADER: &str = "Design intent that governs these files (the handbook rule \
     that governs them, the RA/architecture section that specifies them, the load-bearing \
     decision that constrains them, and the local rationale - reached by the one graph \
     traversal):";

/// FRAMING: the decisions grounding section's header, printed only when the section has a
/// decision.
pub const DECISIONS_HEADER: &str = "Decisions that govern these files:";

/// FRAMING: the lessons grounding section's header, printed only when the section has a lesson.
pub const LESSONS_HEADER: &str = "Lessons already learned about these files:";

/// FRAMING: the findings grounding section's header, printed only when the section has a
/// finding.
pub const FINDINGS_HEADER: &str = "Findings other reviewers have already raised about these files:";

/// FRAMING: the note a decisions, lessons or findings section closes with when it elides an
/// entry: `{elided}` entries of the `{noun}` kind, read back as `{recover}` ([`RECORD_RECOVERY`]).
pub const ELISION_NOTE_RECORDS: &str =
    "- (+{elided} older {noun}(s) elided to keep this prompt under budget - {recover})\n";

/// CONTRACT: how a reader reaches a decision, lesson or finding a capped surface left out: by
/// its id through the graph. The prompt sections' elision note, `rigger peers` and
/// `rigger graph --around`'s governance tail each name it.
pub const RECORD_RECOVERY: &str = "read one by id with `rigger graph --show <id>` (the \
     `rigger_graph` tool's `show`; its `around` on a file lists the ids)";

/// FRAMING: the note the code-neighborhood section closes with when it elides a definition,
/// `{elided}` of them, recovered with `{recover}` (a `GRAPH_AROUND_*` command).
pub const ELISION_NOTE_DEFINITIONS: &str = "- (+{elided} more definition(s) elided to keep this \
     prompt under budget - recover the full set with {recover})\n";

/// FRAMING: the note the design-intent section closes with when it elides a node, `{elided}` of
/// them, recovered with `{recover}` (a `GRAPH_AROUND_*` command).
pub const ELISION_NOTE_DESIGN_INTENT: &str = "- (+{elided} more design-intent node(s) elided to \
     keep this prompt under budget - recover the full set with {recover})\n";

/// CONTRACT: the recovery command an elision note names when the grounding seed is empty.
pub const GRAPH_AROUND_ANY: &str = "`rigger graph --around <file>`";

/// CONTRACT: the recovery command an elision note names for the one seed file `{seed}`.
pub const GRAPH_AROUND_ONE: &str = "`rigger graph --around {seed}`";

/// CONTRACT: the recovery command an elision note names for several seed files, `{seeds}`
/// holding them space-separated.
pub const GRAPH_AROUND_EACH: &str = "`rigger graph --around <file>` on each of: {seeds}";

/// FRAMING: the unit's task block, its `{name}` and its criterion `{criterion}` verbatim, printed
/// only for a non-producer stage that owns a criterion.
pub const TASK_BLOCK: &str = "UNIT: {name}\nACCEPTANCE CRITERION (verbatim from the spec - this \
     is your whole contract; the reviewer judges the unit against exactly this text):\n\
     {criterion}\n\n";

/// FRAMING: a re-entry after a halted spawn, naming the `{commit}` its uncommitted work was
/// captured as; printed only when a halted spawn's tree was captured.
pub const PRIOR_HALTED: &str = "A prior incarnation of this spawn was halted before it could \
     report; its uncommitted work is captured as commit {commit} on your branch. Finish and \
     report; do not start over.\n";

/// FRAMING: the retry header, printed only when a gate, review, REQUIRED or contradiction cause
/// is set.
pub const PRIOR_RETRY_HEADER: &str =
    "Your previous attempt failed the checks below. Fix exactly these - do not start over:\n";

/// FRAMING: one failed gate's `{evidence}`, once per gate the previous attempt failed.
pub const PRIOR_GATE: &str = "Your previous attempt failed these gates: {evidence}\n";

/// FRAMING: the review rejection's `{reason}`, printed only when review rejected.
pub const PRIOR_REVIEW: &str = "Your previous attempt was rejected by review: {reason}\n";

/// FRAMING: the lead-in to the REQUIRED items, printed only when the REQUIRED list is non-empty.
pub const PRIOR_REQUIRED_LEAD: &str = "REQUIRED - review holds the next round to each of these:\n";

/// CONTRACT: the audit record a REQUIRED pattern item asks for, its `{shape}` and the
/// `DecisionMade` id `{audit}`; printed only for an item with a pattern shape.
pub const PRIOR_PATTERN: &str = "  PATTERN: {shape}. Record DecisionMade `{audit}` for it.\n";

/// FRAMING: the reverted-change sentence and its `{contradiction}`, printed only on a
/// compensation re-entry.
pub const PRIOR_CONTRADICTION: &str = "A later unit's work proved your integrated change wrong; \
     your integrating commit was REVERTED off the run branch. Fix exactly this contradiction: \
     {contradiction}\n";

/// FRAMING: a later review round's lead-in: the round-start `{base}` and the `{delta}` changed
/// since, [`DELTA_NONE`] when nothing changed.
pub const ROUND_LEAD_IN: &str = "\n\nLATER REVIEW ROUND: an earlier round judged this unit at \
     {base} and sent it back. Review what changed since - `git diff {base}..HEAD`, which changed \
     {delta} - and whether each item that round required is now fixed.\n";

/// FRAMING: what fills a changed-paths slot when the delta is empty.
pub const DELTA_NONE: &str = "nothing";

/// FRAMING: the line a later review round prints when the earlier round required no item.
pub const ROUND_REQUIRED_NONE: &str = "That round required no item.\n";

/// FRAMING: the header over the items the earlier round required, printed only when it required
/// one.
pub const ROUND_REQUIRED_HEADER: &str = "The items that round REQUIRED:\n";

/// CONTRACT: the implementer's audit record `{audit}` for a REQUIRED pattern item's `{shape}`;
/// printed only for an item with a pattern shape.
pub const ROUND_PATTERN: &str =
    "  PATTERN: {shape}. The implementer's audit record is DecisionMade `{audit}`.\n";

/// CONTRACT: the rule that splits a finding outside the round's delta, and the `required` list a
/// reject names.
pub const ROUND_OUTSIDE_DELTA: &str = "A finding outside that delta blocks only as a correctness \
     defect (`\"correctness\":true`); any other finding outside it is recorded for the operator \
     and never blocks. A reject this round names every item it requires in `required`.\n";

/// FRAMING: the gate evidence block's header for the `{commit}` under review, printed only when a
/// gate ran on it.
pub const GATE_EVIDENCE_HEADER: &str = "\n\nGATE EVIDENCE for the commit under review (commit \
     {commit}, which `git rev-parse HEAD` prints in the unit's worktree): every gate below \
     already ran on exactly this commit.\n";

/// FRAMING: one gate's row, its `{gate}`, its `{verdict}` and its input `{digest}`.
pub const GATE_EVIDENCE_ROW: &str = "- {gate}: {verdict} (input digest {digest})\n";

/// FRAMING: the conflict-resolution task for unit `{unit}`, the conflicting `{paths}` one line
/// each, and the resolve-and-commit sentences.
pub const CONFLICT_RESOLUTION: &str = "Your unit \"{unit}\"'s change was approved and gated \
     green, but integrating it hit a real merge conflict against work another unit landed on the \
     run branch meanwhile - this is not a defect in your implementation, and no remediation \
     attempt is charged for it. The run branch has already been merged into your worktree (`git \
     merge --no-commit`) and conflict markers are left in place in exactly these \
     path(s):\n{paths}\n\nResolve the conflicts in these paths only, preserving both sides' \
     intent, `git add` each resolved path, and `git commit` to complete the merge on your CURRENT \
     branch. Do not revert, discard, or re-implement any of your PRIOR commits - they stay \
     exactly as they are; you are only completing this one merge.";

/// FRAMING: a resumed sdet-author's task, `{causes}` holding what sent the unit back; printed
/// only when a cause is set.
pub const LATER_ATTEMPT: &str = "LATER ATTEMPT: the implementer has revised this unit since \
     {causes}. Bring the periphery tests in line with that change.\n";

/// FRAMING: the review cause, the round-start `{base}` and the paths `{changed}` since; printed
/// only when a review round sent the unit back.
pub const LATER_ATTEMPT_REVIEW_CAUSE: &str =
    "review sent this unit back at {base} - `git diff {base}..HEAD` since changed {changed}";

/// FRAMING: one failed gate's `{evidence}`, once per gate the previous attempt failed.
pub const LATER_ATTEMPT_GATE_CAUSE: &str = "the previous attempt failed these gates: {evidence}";

/// FRAMING: the round's fact, printed only on a round whose delta changed documentation only.
pub const DOC_ONLY_ROUND: &str = "This round changed documentation only.";

/// FRAMING: the retrieval facts the implement slice prints in place of the capped sections.
pub const PEERS_POINTER: &str = "\nPrior decisions, lessons, and findings about these files are \
     not inlined here to keep this prompt precise - retrieve the ones your sub-problem needs, \
     scoped to your blast-radius files, with the lookup verbs below (the full history is \
     retained in the store and any record reads by id, not truncated into this prompt).\n";

/// FRAMING: the lookup pointer's lead-in, printed on every grounding slice.
pub const LOOKUP_LEAD_IN: &str = "\nThe knowledge graph answers lookups through four verbs: ";

/// CONTRACT: the knowledge graph's lookup verbs, each with its job, and the two worked examples.
pub const LOOKUP_VERBS: &str = "`rigger graph --lookup <term>...` (each term at once from the \
     graph, a live scan of your working tree and the project's ranked prose), `rigger graph \
     --around <file|entity>` (structure: callers, callees, neighbors), `rigger graph --show \
     <entity>` (text: definition site and body), `rigger peers <file>...` (memory: decisions, \
     findings, lessons). Who calls <name>: `rigger graph --around <name>`. Where is <name> \
     defined: `rigger graph --show <name>`.\n";

/// FRAMING: the prior plan critique's rejection, `{prior_reason}`, printed only on a re-review.
pub const DAG_PRIOR_REJECT: &str = "A prior plan-critique REJECTED this \
     decomposition:\n{prior_reason}\n\nThe planner has since revised the DAG; re-review it \
     below.\n\n";

/// FRAMING: the plan-critique gate's task.
pub const DAG_CRITIQUE_OPENER: &str = "You are the plan-critique gate. Review the PROPOSED unit \
     DAG below - the decomposition the planner produced - BEFORE any implementer runs, and judge \
     it against the decomposition rules in your persona: the CROSS-UNIT rules that per-unit \
     review cannot see.\n\n";

/// FRAMING: the header over the proposed units.
pub const DAG_UNITS_HEADER: &str = "Proposed units:\n";

/// FRAMING: one proposed unit's row: its `{name}`, its escaped `{criterion}`, its `{needs}` and
/// its blast `{radius}`.
pub const DAG_UNIT_ROW: &str =
    "- `{name}`: criterion=\"{criterion}\"; needs=[{needs}]; blast radius=[{radius}]\n";

/// FRAMING: what fills a unit row's `{criterion}` when the unit owns none.
pub const DAG_NO_CRITERION: &str = "(no criterion)";

/// FRAMING: what fills a unit row's `{needs}` when the unit needs none.
pub const DAG_NO_NEEDS: &str = "none";

/// FRAMING: what fills a unit row's `{radius}` when no file was grounded.
pub const DAG_NO_RADIUS: &str = "(none grounded)";

/// FRAMING: the overlap line printed only when no two units share a file.
pub const DAG_OVERLAP_DISJOINT: &str = "\nBlast-radius overlap (informational): every proposed \
     unit has a DISJOINT blast radius.\n";

/// FRAMING: the overlap header printed only when two units share a file.
pub const DAG_OVERLAP_HEADER: &str = "\nBlast-radius overlap (informational - the partitioner \
     serializes these into sequential batches):\n";

/// FRAMING: one conflicting pair, units `{a}` and `{other}` and the files `{shared}` they both
/// touch.
pub const DAG_OVERLAP_ROW: &str = "- units `{a}` and `{other}` both touch: {shared}\n";

/// CONTRACT: the verdict line and the cause contract a plan critique's reject follows,
/// `{spec_ambiguity}` holding the spec-ambiguity cause and `{size_cap_cause}`
/// [`DAG_SIZE_CAP_CAUSE`] when a unit size cap is declared, else nothing.
pub const DAG_CRITIQUE_CONTRACT: &str = "\nRender your final verdict as a JSON line: \
     {\"verdict\":\"approve\"} to release the fan-out, or {\"verdict\":\"reject\"} to send the \
     decomposition back to the planner. A reject's cause follows this gate's contract, which \
     governs it over any generic cause wording in your persona: \"cause\":\"{spec_ambiguity}\" \
     only when the upheld defect is in a criterion's own text and no decomposition can remove \
     it (two criteria that contradict under every landing order, a criterion no plan can \
     satisfy, a demanded mitigation no criterion owns); \"cause\":\"decomposition-conflict\" for \
     every defect a re-plan can fix (twin units, a missing exclusion between units, \
     {size_cap_cause}a unit owning no criterion, a split the planner chose). When unsure, \
     \"cause\":\"decomposition-conflict\".\n";

/// CONTRACT: the cause example [`DAG_CRITIQUE_CONTRACT`] names among the defects a re-plan can
/// fix, only when the workflow declares a unit size cap.
pub const DAG_SIZE_CAP_CAUSE: &str = "a unit over the size cap, ";

/// FRAMING: the plan critique's rejection `{feedback}` a re-plan opens with, and the revise
/// sentence.
pub const REPLAN_LEAD: &str = "The plan-critique gate REJECTED your previous \
     decomposition:\n{feedback}\n\nRevise the unit DAG to resolve it, then re-emit your \
     UnitProposed refinements. ";

/// CONTRACT: how a re-plan's ids refine or replace the units an earlier pass proposed.
pub const REPLAN_ID_CONTRACT: &str = "To REFINE an existing unit, re-emit it under its EXACT \
     existing unit-id - the refinement UPDATES that unit in place: its criterion text, when \
     given, becomes the unit's text and its needs edges are replaced. The unit keeps the \
     criterion it serves, so a re-emit naming a DIFFERENT criterion keeps the unit's text; use a \
     NEW unit-id ONLY for a genuinely new or split unit. A NEW id serving a criterion replaces \
     every unit an earlier pass proposed for it that this pass does not re-emit, so re-emitting \
     a refinement under a NEW id drops the unit it meant to refine instead of refining it.\n\n";

/// FRAMING: the canary item's framing, `{file}` naming the code under review and `{review}`
/// holding it.
pub const CANARY_REVIEW_HEADER: &str = "You are reviewing a micro-unit. The code under review is \
     the file `{file}`. Review it for defects and raise a finding about `{file}` for each one you \
     find.\n\n----- BEGIN {file} -----\n{review}\n----- END {file} -----";

/// FRAMING: what fills [`CANARY_REVIEW_HEADER`]'s `{file}` when the item has no anchor.
pub const CANARY_NO_ANCHOR: &str = "the code below";

/// FRAMING: the lead-in to the findings the canary adjudicator weighs.
pub const CANARY_FINDINGS_LEAD: &str = "\n\nThe review tiers raised these findings, in order:\n";

/// FRAMING: the line the canary adjudicator reads when no tier raised a finding.
pub const CANARY_NO_FINDINGS: &str = "(no findings were raised)\n";

/// CONTRACT: the verdict line the canary adjudicator renders, which `verdict_approves` reads.
pub const CANARY_VERDICT: &str = "\nRender your gating verdict as a single JSON line: \
     {\"verdict\":\"approve\"} to integrate, or {\"verdict\":\"reject\"} to send it back.";

/// The conductor's and the canary's groups by surface, each named by its renderer and listing
/// every group its render prints, a sub-renderer's and a slot-filling group's included, in
/// render order.
pub fn prompt_groups() -> &'static [rigger_domain::prompt_surface::Group] {
    rigger_domain::prompt_groups![
        "conductor::emit_protocol" => [EMIT_PROTOCOL, EMIT_LIVE],
        "conductor::render_plan_protocol" => [
            PLAN_ROLE,
            UNIT_SIZE_CAP,
            PLAN_SPLIT_ID_ECHO,
            PLAN_PAIRING,
            PLAN_COPY_TEXT,
            PLAN_REAL_SPLIT,
            PLAN_UNIT_PROPOSED,
            PLAN_UNMATCHED,
            PLAN_CRITERIA_LEAD,
            PLAN_NO_CRITERIA,
        ],
        "conductor::adjudicator_contract" => [
            REVIEWER_DISCIPLINE,
            REVIEWER_BUILD_CACHE,
            UPHELD_PROTOCOL,
            UPHELD_NOTES,
            REQUIRED_PROTOCOL,
        ],
        "conductor::review_protocol" => [
            REVIEW_FINDING_PROTOCOL,
            REVIEW_FINDING_LIVE,
            REVIEWER_DISCIPLINE,
            REVIEWER_BUILD_CACHE,
        ],
        "conductor::notes_block" => [RUN_NOTES_HEADER, RUN_NOTES_CLOSING],
        "conductor::RefusedRecord::task" => [REFUSED_RECORD_NOTE],
        "conductor::task_block" => [TASK_BLOCK],
        "conductor::PriorFailure::block" => [
            PRIOR_HALTED,
            PRIOR_RETRY_HEADER,
            PRIOR_GATE,
            PRIOR_REVIEW,
            PRIOR_REQUIRED_LEAD,
            PRIOR_PATTERN,
            PRIOR_CONTRADICTION,
        ],
        "conductor::ReviewRound::block" => [
            ROUND_LEAD_IN,
            DELTA_NONE,
            ROUND_REQUIRED_NONE,
            ROUND_REQUIRED_HEADER,
            ROUND_PATTERN,
            ROUND_OUTSIDE_DELTA,
        ],
        "conductor::gate_evidence_block" => [GATE_EVIDENCE_HEADER, GATE_EVIDENCE_ROW],
        "conductor::conflict_resolution_prompt" => [CONFLICT_RESOLUTION],
        "conductor::later_attempt_block" => [
            LATER_ATTEMPT,
            LATER_ATTEMPT_REVIEW_CAUSE,
            DELTA_NONE,
            LATER_ATTEMPT_GATE_CAUSE,
        ],
        "conductor::doc_only_round" => [DOC_ONLY_ROUND],
        "conductor::write_peers_pointer" => [PEERS_POINTER, LOOKUP_LEAD_IN, LOOKUP_VERBS],
        "conductor::write_lookup_pointer" => [LOOKUP_LEAD_IN, LOOKUP_VERBS],
        "conductor::build_dag_critique_prompt" => [
            DAG_PRIOR_REJECT,
            DAG_CRITIQUE_OPENER,
            UNIT_SIZE_CAP,
            DAG_UNITS_HEADER,
            DAG_UNIT_ROW,
            DAG_NO_CRITERION,
            DAG_NO_NEEDS,
            DAG_NO_RADIUS,
            DAG_OVERLAP_DISJOINT,
            DAG_OVERLAP_HEADER,
            DAG_OVERLAP_ROW,
            DAG_CRITIQUE_CONTRACT,
            DAG_SIZE_CAP_CAUSE,
        ],
        "conductor::re_plan_lead" => [REPLAN_LEAD, REPLAN_ID_CONTRACT],
        "conductor::write_code_neighborhood" => [
            CODE_NEIGHBORHOOD_HEADER,
            ELISION_NOTE_DEFINITIONS,
            GRAPH_AROUND_ONE,
            GRAPH_AROUND_EACH,
        ],
        "conductor::write_design_intent" => [
            DESIGN_INTENT_HEADER,
            ELISION_NOTE_DESIGN_INTENT,
            GRAPH_AROUND_ONE,
            GRAPH_AROUND_EACH,
        ],
        "conductor::write_capped_decisions" => [
            DECISIONS_HEADER,
            ELISION_NOTE_RECORDS,
            RECORD_RECOVERY,
        ],
        "conductor::write_capped_lessons" => [LESSONS_HEADER, ELISION_NOTE_RECORDS, RECORD_RECOVERY],
        "conductor::write_capped_findings" => [
            FINDINGS_HEADER,
            ELISION_NOTE_RECORDS,
            RECORD_RECOVERY,
        ],
        "conductor::graph_around_recovery" => [
            GRAPH_AROUND_ANY,
            GRAPH_AROUND_ONE,
            GRAPH_AROUND_EACH,
        ],
        "canary_store::review_header" => [CANARY_REVIEW_HEADER, CANARY_NO_ANCHOR],
        "canary_store::adjudicator_prompt" => [
            CANARY_REVIEW_HEADER,
            CANARY_NO_ANCHOR,
            CANARY_FINDINGS_LEAD,
            CANARY_NO_FINDINGS,
            CANARY_VERDICT,
        ],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::registry_test;

    /// THE CLASS TABLE's conductor rows: each const and the class its doc comment opens with.
    const CLASS_TABLE: &[(&str, &str)] = &[
        ("CANARY_FINDINGS_LEAD", "FRAMING"),
        ("CANARY_NO_ANCHOR", "FRAMING"),
        ("CANARY_NO_FINDINGS", "FRAMING"),
        ("CANARY_REVIEW_HEADER", "FRAMING"),
        ("CANARY_VERDICT", "CONTRACT"),
        ("CODE_NEIGHBORHOOD_HEADER", "FRAMING"),
        ("CONFLICT_RESOLUTION", "FRAMING"),
        ("DAG_CRITIQUE_CONTRACT", "CONTRACT"),
        ("DAG_CRITIQUE_OPENER", "FRAMING"),
        ("DAG_NO_CRITERION", "FRAMING"),
        ("DAG_NO_NEEDS", "FRAMING"),
        ("DAG_NO_RADIUS", "FRAMING"),
        ("DAG_OVERLAP_DISJOINT", "FRAMING"),
        ("DAG_OVERLAP_HEADER", "FRAMING"),
        ("DAG_OVERLAP_ROW", "FRAMING"),
        ("DAG_PRIOR_REJECT", "FRAMING"),
        ("DAG_SIZE_CAP_CAUSE", "CONTRACT"),
        ("DAG_UNITS_HEADER", "FRAMING"),
        ("DAG_UNIT_ROW", "FRAMING"),
        ("DECISIONS_HEADER", "FRAMING"),
        ("DELTA_NONE", "FRAMING"),
        ("DESIGN_INTENT_HEADER", "FRAMING"),
        ("DOC_ONLY_ROUND", "FRAMING"),
        ("ELISION_NOTE_DEFINITIONS", "FRAMING"),
        ("ELISION_NOTE_DESIGN_INTENT", "FRAMING"),
        ("ELISION_NOTE_RECORDS", "FRAMING"),
        ("EMIT_LIVE", "FRAMING"),
        ("EMIT_PROTOCOL", "CONTRACT"),
        ("FINDINGS_HEADER", "FRAMING"),
        ("GATE_EVIDENCE_HEADER", "FRAMING"),
        ("GATE_EVIDENCE_ROW", "FRAMING"),
        ("GRAPH_AROUND_ANY", "CONTRACT"),
        ("GRAPH_AROUND_EACH", "CONTRACT"),
        ("GRAPH_AROUND_ONE", "CONTRACT"),
        ("LATER_ATTEMPT", "FRAMING"),
        ("LATER_ATTEMPT_GATE_CAUSE", "FRAMING"),
        ("LATER_ATTEMPT_REVIEW_CAUSE", "FRAMING"),
        ("LESSONS_HEADER", "FRAMING"),
        ("LOOKUP_LEAD_IN", "FRAMING"),
        ("LOOKUP_VERBS", "CONTRACT"),
        ("PATTERN_LABEL", "CONTRACT"),
        ("PEERS_POINTER", "FRAMING"),
        ("PLAN_COPY_TEXT", "CONTRACT"),
        ("PLAN_CRITERIA_LEAD", "CONTRACT"),
        ("PLAN_NO_CRITERIA", "FRAMING"),
        ("PLAN_PAIRING", "FRAMING"),
        ("PLAN_REAL_SPLIT", "FRAMING"),
        ("PLAN_ROLE", "FRAMING"),
        ("PLAN_SPLIT_ID_ECHO", "CONTRACT"),
        ("PLAN_UNIT_PROPOSED", "CONTRACT"),
        ("PLAN_UNMATCHED", "FRAMING"),
        ("PRIOR_CONTRADICTION", "FRAMING"),
        ("PRIOR_GATE", "FRAMING"),
        ("PRIOR_HALTED", "FRAMING"),
        ("PRIOR_PATTERN", "CONTRACT"),
        ("PRIOR_REQUIRED_LEAD", "FRAMING"),
        ("PRIOR_RETRY_HEADER", "FRAMING"),
        ("PRIOR_REVIEW", "FRAMING"),
        ("RECORD_RECOVERY", "CONTRACT"),
        ("REFUSED_RECORD_NOTE", "FRAMING"),
        ("REPLAN_ID_CONTRACT", "CONTRACT"),
        ("REPLAN_LEAD", "FRAMING"),
        ("REQUIRED_PROTOCOL", "CONTRACT"),
        ("REVIEWER_BUILD_CACHE", "FRAMING"),
        ("REVIEWER_DISCIPLINE", "FRAMING"),
        ("REVIEW_FINDING_LIVE", "FRAMING"),
        ("REVIEW_FINDING_PROTOCOL", "CONTRACT"),
        ("ROUND_LEAD_IN", "FRAMING"),
        ("ROUND_OUTSIDE_DELTA", "CONTRACT"),
        ("ROUND_PATTERN", "CONTRACT"),
        ("ROUND_REQUIRED_HEADER", "FRAMING"),
        ("ROUND_REQUIRED_NONE", "FRAMING"),
        ("RUN_NOTES_CLOSING", "FRAMING"),
        ("RUN_NOTES_HEADER", "FRAMING"),
        ("TASK_BLOCK", "FRAMING"),
        ("UNIT_SIZE_CAP", "FRAMING"),
        ("UPHELD_NOTES", "FRAMING"),
        ("UPHELD_PROTOCOL", "CONTRACT"),
    ];

    registry_test!(
        every_conductor_prompt_text_is_a_group_of_its_class_table_class,
        CLASS_TABLE
    );
}
