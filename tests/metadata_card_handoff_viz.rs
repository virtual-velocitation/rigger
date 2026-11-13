//! Periphery (integration) test for spec 63's METADATA CARD client seam (criterion 2): the served
//! page's OWN `renderCard` draws the "one hover-card anatomy everywhere" - a title row, a
//! provenance row (file:line, degree, community), and per-taxonomy chip rows (FILE / CONCEPTS /
//! MEMORY for a code-entity subject, TOP ENTITIES for a file subject, TOP EVIDENCE for a concept
//! subject) - and a card CHIP actually hands off to its own taxonomy's lens: clicking a FILE or
//! CONCEPTS (or TOP-ENTITIES / TOP-EVIDENCE) chip forces that lens and re-projects the chip's id AS
//! THE SUBJECT (`kgLens`/`kgSubject` land on the target), while a MEMORY chip carries no lens at
//! all - it reuses criterion 5's plain re-seed (`kgSeed` lands on the card's own subject, `kgLens`
//! is left UNTOUCHED) since this design names no memory lens. A TOP-EVIDENCE member is not always
//! a code entity (the intent layer can fold a file, or any other intent-layer kind, into the same
//! concept's REALIZES membership - spec 54/29b), so its chip's lens is resolved from the member's
//! OWN kind, never assumed code: this file proves a FILE evidence member hands off to the FILES
//! lens and a kind with no lens of its own (a design-doc) carries no handoff at all.
//!
//! The implementer's inside-out `dash.rs` tests pin the pure `card`/`Card` surface; this layer
//! proves the CLIENT actually renders it AND that a chip click drives the right seam - the
//! behavioral half a structural grep on the served bytes cannot make, and the boundary the spec's
//! "Test seams" note assigns to a node-gated runtime harness (spec 42/55/59 precedent). Driven by
//! the served page's OWN script under node's built-in `vm` (hermetic, no npm) through a DOM +
//! fetch shim, mirroring `subject_view_memory_rail_client.rs`. `dash` compiles on BOTH the default
//! and the `--no-default-features` lane (the seam is not feature-gated), so this guards the client
//! seam in both lanes.

use crate::common;

use common::served::node_harness_passes;

use crate::vm_harness;
use vm_harness::vm_harness;

/// The DOM shim (node `vm`, no npm): the element surfaces the client seam touches (innerHTML /
/// dataset / .hidden / addEventListener). Mirrors `subject_view_memory_rail_client.rs`'s shim
/// verbatim - the same handful of surfaces every dash client-seam harness in this repo needs.
const DOM_SHIM: &str = r#"
const __els = {};
function __Stub(){ this._attrs = {}; }
__Stub.prototype.setAttribute = function(k,v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return this._attrs[k]; };
__Stub.prototype.addEventListener = function(){};
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={};
  this.hidden=false; this.className=""; this.checked=false;
  this.clientWidth = 800; this.clientHeight = 300;
  this.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; }; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.querySelector = function(){ return new __Stub(); };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
"#;

/// The in-memory `/api/graph` fixture the `fetch` shims answer from: `card=<id>` serves one of
/// three fixture cards (a code entity, its file, and the concept it realizes); a `seed=&lens=`
/// re-projection or a plain `seed=` neighborhood serves a minimal-but-valid body naming the
/// requested subject (so `kgSubject`/`kgSeed` land on it), and a `cluster=` drill echoes its
/// cluster key as its seed, as the route does; anything else (the initial whole-graph overview,
/// `explain=`, `/api/instances`, `/api/state`) serves a graceful empty.
const CARD_FIXTURE: &str = r#"
const CODE_CARD = { id: "combat.rs::fire", kind: "code-entity", label: "fire",
  file: "combat.rs", line: "42", degree: 5, community: "combat lifecycle",
  concepts: [ { id: "concept/combat", label: "combat resolution" } ],
  decisions: 2, findings: 1, top_entities: [], top_evidence: [] };
const FILE_CARD = { id: "combat.rs", kind: "file", label: "combat.rs",
  degree: 2, concepts: [], decisions: 0, findings: 0,
  top_entities: [ { id: "combat.rs::fire", kind: "code-entity", label: "fire" },
                  { id: "combat.rs::reload", kind: "code-entity", label: "reload" } ],
  top_evidence: [] };
const CONCEPT_CARD = { id: "concept/combat", kind: "concept", label: "combat resolution",
  degree: 1, concepts: [], decisions: 0, findings: 0,
  top_entities: [], top_evidence: [ { id: "combat.rs::fire", kind: "code-entity", label: "fire" } ] };
function __cardFor(id){
  if (id === "combat.rs::fire") return CODE_CARD;
  if (id === "combat.rs") return FILE_CARD;
  if (id === "concept/combat") return CONCEPT_CARD;
  return null;
}
function __qval(url, key){
  const m = String(url).match(new RegExp("[?&]" + key + "=([^&]*)"));
  return m ? decodeURIComponent(m[1]) : null;
}
function __view(url){
  const s = String(url);
  const cardId = __qval(s, "card");
  if (cardId !== null) return { card: __cardFor(cardId) };
  const cluster = __qval(s, "cluster");
  if (cluster !== null) return { seed: cluster, depth: 1, nodes: [], edges: [] };
  const seed = __qval(s, "seed");
  if (seed !== null && __qval(s, "lens") !== null) return { subject: seed, clusters: [], edges: [], total: 0 };
  if (seed !== null) return { seed: seed, depth: 2, nodes: [], edges: [] };
  return { clusters: [], edges: [], total: 0 };
}
function __answer(url){
  return { headers: { get: function(){ return null; } }, json: function(){ return Promise.resolve(__view(url)); } };
}
"#;

/// A `fetch` shim answering every `/api/graph` request from [`CARD_FIXTURE`] at once. Every other
/// kind of URL rejects (matching the live page's own network shape when nothing else answers).
const RESOLVING_FETCH: &str = r#"
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    return Promise.resolve(__answer(url));
  }
  return Promise.reject(new Error("no network for " + url));
};
"#;

/// A `fetch` shim like [`RESOLVING_FETCH`] except that each `card=` request waits until the driver
/// releases it, so a driver can see what a view draws while its card is still being fetched and
/// what a card that lands late does. `__releaseCardsNewestFirst()` answers every held request, the
/// latest one first.
const HELD_CARD_FETCH: &str = r#"
const __heldCards = [];
function __releaseCardsNewestFirst(){ while (__heldCards.length) __heldCards.pop()(); }
const fetch = function(url){
  if (String(url).indexOf("/api/graph") === -1) return Promise.reject(new Error("no network for " + url));
  if (__qval(url, "card") === null) return Promise.resolve(__answer(url));
  return new Promise(function(resolve){ __heldCards.push(function(){ resolve(__answer(url)); }); });
};
"#;

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "dash-metadata-card-harness.js";

/// Driver: (1) `renderCard` draws each of the three card taxonomies with their documented rows and
/// chips, and hides on `null`; (2) `#kgcard`'s own delegated click listener hands a FILE/CONCEPTS/
/// TOP-ENTITIES/TOP-EVIDENCE chip off to its OWN taxonomy's lens, carrying the chip's id as the new
/// subject; (3) a MEMORY chip carries NO lens at all - it reuses the plain re-seed instead, leaving
/// the current lens untouched.
const CARD_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  function fire(listeners, target){
    (listeners||[]).forEach(function(fn){ fn({ target: target, preventDefault: function(){} }); });
  }
  function chipTarget(attrs){
    return { dataset: attrs, closest: function(sel){ return sel === "[data-seed]" ? this : null; } };
  }

  await flush();

  // --- renderCard: a CODE-ENTITY subject's card -----------------------------------------------
  renderCard({ id: "combat.rs::fire", kind: "code-entity", label: "fire",
    file: "combat.rs", line: "42", degree: 5, community: "combat lifecycle",
    concepts: [ { id: "concept/combat", label: "combat resolution" } ],
    decisions: 2, findings: 1, top_entities: [], top_evidence: [] });
  if (el("kgcard").hidden !== false) throw new Error("a non-null card must reveal #kgcard");
  const codeHtml = el("kgcard")._html;
  if (codeHtml.indexOf("fire") === -1) throw new Error("REGRESSION: the card title must name the subject: " + codeHtml);
  if (codeHtml.indexOf("combat.rs:42") === -1) throw new Error("REGRESSION: the card must carry file:line: " + codeHtml);
  if (codeHtml.indexOf("degree 5") === -1) throw new Error("REGRESSION: the card must carry its degree: " + codeHtml);
  if (codeHtml.indexOf("combat lifecycle") === -1) throw new Error("REGRESSION: the card must carry its community: " + codeHtml);
  if (codeHtml.indexOf('data-seed="combat.rs"') === -1 || codeHtml.indexOf('data-handoff="files"') === -1)
    throw new Error("REGRESSION: the FILE chip must hand off to the files lens: " + codeHtml);
  if (codeHtml.indexOf('data-seed="concept/combat"') === -1 || codeHtml.indexOf('data-handoff="concepts"') === -1)
    throw new Error("REGRESSION: the CONCEPTS chip must hand off to the concepts lens: " + codeHtml);
  if (codeHtml.indexOf("2 decisions") === -1 || codeHtml.indexOf("1 finding") === -1)
    throw new Error("REGRESSION: the MEMORY row must carry decision/finding COUNTS: " + codeHtml);
  const memIdx = codeHtml.indexOf("2 decisions");
  if (codeHtml.slice(Math.max(0, memIdx - 100), memIdx).indexOf("data-handoff") !== -1)
    throw new Error("REGRESSION: a MEMORY chip must carry NO data-handoff - no memory lens exists: " + codeHtml);

  // --- renderCard: a FILE subject's card (TOP ENTITIES) ---------------------------------------
  renderCard({ id: "combat.rs", kind: "file", label: "combat.rs", degree: 2,
    concepts: [], decisions: 0, findings: 0,
    top_entities: [ { id: "combat.rs::fire", kind: "code-entity", label: "fire" },
                    { id: "combat.rs::reload", kind: "code-entity", label: "reload" } ],
    top_evidence: [] });
  const fileHtml = el("kgcard")._html;
  if (fileHtml.indexOf("TOP ENTITIES") === -1) throw new Error("REGRESSION: a file card must carry TOP ENTITIES: " + fileHtml);
  if (fileHtml.indexOf('data-seed="combat.rs::fire"') === -1 || fileHtml.indexOf('data-handoff="code"') === -1)
    throw new Error("REGRESSION: a CODE-ENTITY top-entity chip must hand off to the code lens: " + fileHtml);
  if (fileHtml.indexOf("CONCEPTS") !== -1 || fileHtml.indexOf("MEMORY") !== -1)
    throw new Error("REGRESSION: a file's card must carry no FILE/CONCEPTS/MEMORY rows (a code-entity's own rows): " + fileHtml);

  // --- renderCard: a CONCEPT subject's card (TOP EVIDENCE) ------------------------------------
  renderCard({ id: "concept/combat", kind: "concept", label: "combat resolution", degree: 1,
    concepts: [], decisions: 0, findings: 0, top_entities: [],
    top_evidence: [ { id: "combat.rs::fire", kind: "code-entity", label: "fire" } ] });
  const conceptHtml = el("kgcard")._html;
  if (conceptHtml.indexOf("TOP EVIDENCE") === -1) throw new Error("REGRESSION: a concept card must carry TOP EVIDENCE: " + conceptHtml);
  if (conceptHtml.indexOf('data-seed="combat.rs::fire"') === -1 || conceptHtml.indexOf('data-handoff="code"') === -1)
    throw new Error("REGRESSION: a CODE-ENTITY evidence chip must hand off to the code lens: " + conceptHtml);

  // --- renderCard: a CONCEPT subject's card whose evidence spans DIFFERENT kinds (spec 63 c2's
  // own defect fix, "Card handoff ownership is total"): the intent layer can fold a file (or any
  // other intent-layer kind) alongside a code entity into the SAME concept's REALIZES membership
  // (mirroring concepts.rs's own primary derivation fixture, which realizes a concept from a
  // KIND_FILE node via SPECIFIES) - each member's chip must hand off to THAT member's OWN
  // taxonomy's lens, never a lens hardcoded for the whole row.
  renderCard({ id: "concept/combat", kind: "concept", label: "combat resolution", degree: 3,
    concepts: [], decisions: 0, findings: 0, top_entities: [],
    top_evidence: [
      { id: "combat.rs::fire", kind: "code-entity", label: "fire" },
      { id: "combat.rs", kind: "file", label: "combat.rs" },
      { id: "doc/combat-design", kind: "design-doc", label: "combat design" }
    ] });
  const mixedHtml = el("kgcard")._html;
  if (mixedHtml.indexOf('data-seed="combat.rs::fire"') === -1 || mixedHtml.indexOf('data-handoff="code"') === -1)
    throw new Error("REGRESSION: a CODE-ENTITY evidence chip must hand off to the code lens: " + mixedHtml);
  if (mixedHtml.indexOf('data-seed="combat.rs"') === -1 || mixedHtml.indexOf('data-handoff="files"') === -1)
    throw new Error("REGRESSION: a FILE evidence chip must hand off to the FILES lens, not code: " + mixedHtml);
  const docChipIdx = mixedHtml.indexOf('data-seed="doc/combat-design"');
  if (docChipIdx === -1) throw new Error("REGRESSION: a design-doc evidence chip must still render: " + mixedHtml);
  const docChipTagStart = mixedHtml.lastIndexOf("<a ", docChipIdx);
  const docChipTagEnd = mixedHtml.indexOf(">", docChipIdx);
  if (mixedHtml.slice(docChipTagStart, docChipTagEnd).indexOf("data-handoff") !== -1)
    throw new Error("REGRESSION: a design-doc evidence chip has no lens of its own - it must carry NO data-handoff: " + mixedHtml);

  // --- renderCard(null) hides it ---------------------------------------------------------------
  renderCard(null);
  if (el("kgcard").hidden !== true) throw new Error("REGRESSION: renderCard(null) must hide #kgcard");

  // --- THE CHIP-TO-LENS HANDOFF DISPATCH: #kgcard's own delegated click listener --------------
  const cardListeners = (el("kgcard")._listeners && el("kgcard")._listeners.click) || [];
  if (!cardListeners.length) throw new Error("no delegated click listener on #kgcard (chip handoff unwired)");

  kgLens = "code"; // start somewhere OTHER than "files", so the FILE chip's effect is observable.
  fire(cardListeners, chipTarget({ seed: "combat.rs", handoff: "files" }));
  await flush();
  if (kgLens !== "files") throw new Error("REGRESSION: a FILE chip must force the files lens: got " + kgLens);
  if (kgSubject !== "combat.rs") throw new Error("REGRESSION: the FILE chip's handoff must carry the chosen subject: got " + kgSubject);

  fire(cardListeners, chipTarget({ seed: "concept/combat", handoff: "concepts" }));
  await flush();
  if (kgLens !== "concepts") throw new Error("REGRESSION: a CONCEPTS chip must force the concepts lens: got " + kgLens);
  if (kgSubject !== "concept/combat") throw new Error("REGRESSION: the CONCEPTS chip's handoff must carry the chosen subject: got " + kgSubject);

  fire(cardListeners, chipTarget({ seed: "combat.rs::fire", handoff: "code" }));
  await flush();
  if (kgLens !== "code") throw new Error("REGRESSION: a TOP-ENTITIES/TOP-EVIDENCE chip must force the code lens: got " + kgLens);
  if (kgSubject !== "combat.rs::fire") throw new Error("REGRESSION: its handoff must carry the chosen subject: got " + kgSubject);

  // --- THE MEMORY CHIP: data-seed alone, no data-handoff - reuses the PLAIN re-seed -----------
  kgLens = "concepts";
  fire(cardListeners, chipTarget({ seed: "combat.rs::fire" }));
  await flush();
  if (kgLens !== "concepts")
    throw new Error("REGRESSION: the MEMORY chip must NOT switch lens (no memory lens exists): got " + kgLens);
  if (kgSeed !== "combat.rs::fire")
    throw new Error("REGRESSION: the MEMORY chip must re-seed the plain neighborhood (c5's mechanism): got " + kgSeed);

  console.log("OK metadata-card-renders-every-taxonomy-and-chips-hand-off-correctly");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

rigger::test_cases! {
    /// RUNTIME guard for spec 63 c2's own Done-when clause: a code subject's card carries file:line,
    /// concept chips, and memory counts, AND a card chip resolves to a lens handoff target of the
    /// chip's own taxonomy carrying the chosen subject - proven for EVERY card taxonomy (code, file,
    /// concept), per the spec's own "criterion 2 owns... every card taxonomy" scope.
    metadata_card_renders_every_taxonomy_and_chips_hand_off_to_their_own_lens:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FIXTURE, RESOLVING_FETCH], CARD_DRIVER, HARNESS_FILE), "OK metadata-card-renders-every-taxonomy-and-chips-hand-off-correctly");
    /// RUNTIME guard proving the card's WIRING (never just its own rendering, covered above): each of
    /// the four call sites the diff added actually drives `loadCard`/`renderCard`, exactly as its own
    /// documentation promises - the seam `tests/dash_kg_graph_route.rs` and
    /// `tests/subject_lens_overlay_served_page.rs` only had to TOLERATE, never had to PROVE.
    metadata_card_wiring_fires_at_every_render_and_drill_call_site:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FIXTURE, RESOLVING_FETCH], WIRING_DRIVER, HARNESS_FILE), "OK metadata-card-wiring-fires-at-every-call-site");
    /// RUNTIME guard that each view is drawn in the room its panes leave: the card's dock shows
    /// only when a card arrives, and the view is laid out once more when it does; a view drawn
    /// for another subject never keeps the last subject's card; and a card that lands after its
    /// view was left never opens over the next view - the latest card request is the only one
    /// drawn.
    every_view_is_drawn_in_the_room_its_panes_leave_and_a_late_card_is_dropped:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FIXTURE, HELD_CARD_FETCH, DRAW_RECORDER], PANES_DRIVER, HARNESS_FILE), "OK every-view-drawn-in-the-room-its-panes-leave");
    /// RUNTIME guard that a shown card survives a re-render of the same subject: the overlay
    /// toggle and a tier toggle re-draw the seed from its body without hiding, blanking or
    /// re-fetching its card, and a re-read of the same subject under another tests filter keeps
    /// the card shown until the re-read card replaces it.
    a_shown_card_survives_a_re_render_of_the_same_subject:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FIXTURE, HELD_CARD_FETCH, DRAW_RECORDER], CARD_KEPT_DRIVER, HARNESS_FILE), "OK a-shown-card-survives-a-re-render");
    /// RUNTIME guard that a subject with no card never narrows the drawing: neither while its
    /// card is fetched nor after the answer names no card, and not when the subject before it
    /// had a card.
    a_subject_with_no_card_never_narrows_the_drawing:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FIXTURE, HELD_CARD_FETCH, DRAW_RECORDER], NO_CARD_DRIVER, HARNESS_FILE), "OK a-subject-with-no-card-never-narrows-the-drawing");
    /// RUNTIME guard that each view sets or clears both panes for its own subject before it
    /// draws: a call walk or re-projection of the seed keeps the seed's rail and card; a call
    /// walk, drill or re-projection of another subject is drawn with neither and cards its own
    /// subject; and a view that cannot be read shows its message with both panes hidden.
    each_view_sets_both_panes_for_its_own_subject_before_it_draws:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FIXTURE, RESOLVING_FETCH, DRAW_RECORDER], OWN_PANES_DRIVER, HARNESS_FILE), "OK each-view-sets-both-panes-for-its-own-subject");
}

/// A recorder of the drawings: from the call on, every write of `#kgpanel` records whether the
/// card and the rail showed while it was drawn.
const DRAW_RECORDER: &str = r#"
function __recordDraws(){
  const draws = [];
  Object.defineProperty(el("kgpanel"), "innerHTML", { get(){ return this._html; },
    set(v){ draws.push({ card: !el("kgcard").hidden, rail: !el("kgrail").hidden }); this._html = String(v); } });
  return draws;
}
function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
"#;

/// Driver for the PANES against each view's drawing, while every card fetch is held.
/// - A files-lens drill is drawn in the whole panel while its card is fetched; when the card
///   lands it shows and the drill is drawn once more, beside it.
/// - A concepts-lens drill of another subject is drawn without the last subject's card; leaving
///   it before its card lands, the overview is drawn with both panes hidden, and the drill's
///   card, released afterwards, neither shows nor draws anything.
/// - The seed and then a re-projection of another subject are each drawn without a card;
///   released newest first, the re-projection's card is the one shown and the seed's late card
///   is dropped.
const PANES_DRIVER: &str = r#"
;(async function(){
  await flush();
  const draws = __recordDraws();
  const drawn = () => JSON.stringify(draws[draws.length - 1]);
  renderMemoryRail(null);
  renderCard(null);

  kgLens = "files";
  await drillCluster("combat.rs");
  await flush();
  if (drawn() !== JSON.stringify({ card: false, rail: false }))
    throw new Error("REGRESSION: the drill was drawn beside a card that had not arrived: " + drawn());
  if (el("kgcard").hidden !== true)
    throw new Error("REGRESSION: the dock showed before its card arrived: " + JSON.stringify(el("kgcard")._html));
  let before = draws.length;
  __releaseCardsNewestFirst();
  await flush();
  if (el("kgcard").hidden !== false || el("kgcard")._html.indexOf("TOP ENTITIES") === -1)
    throw new Error("REGRESSION: the drill's card did not show when it arrived: " + el("kgcard")._html);
  if (draws.length !== before + 1 || drawn() !== JSON.stringify({ card: true, rail: false }))
    throw new Error("REGRESSION: the drill was not drawn once more beside its arrived card: " + JSON.stringify(draws.slice(before)));

  kgLens = "concepts";
  await drillCluster("concept/combat");
  await flush();
  if (drawn() !== JSON.stringify({ card: false, rail: false }))
    throw new Error("REGRESSION: the drill of another subject was drawn beside the last subject's card: " + drawn());
  renderMemoryRail({ decisions: [], findings: [], concepts: [] });
  await loadKgOverview();
  await flush();
  if (drawn() !== JSON.stringify({ card: false, rail: false }))
    throw new Error("REGRESSION: the overview was drawn before it hid the panes: " + drawn());
  before = draws.length;
  __releaseCardsNewestFirst();
  await flush();
  if (el("kgcard").hidden !== true)
    throw new Error("REGRESSION: a card that landed after its drill was left opened over the overview");
  if (draws.length !== before)
    throw new Error("REGRESSION: a dropped card drew the overview again: " + JSON.stringify(draws.slice(before)));

  renderGraph({ seed: "combat.rs::fire", nodes: [], edges: [], memory: { decisions: [], findings: [], concepts: [] } });
  if (drawn() !== JSON.stringify({ card: false, rail: true }))
    throw new Error("REGRESSION: the seed was drawn before its rail, or beside a card that had not arrived: " + drawn());
  renderReprojection({ subject: "combat.rs", clusters: [], edges: [], total: 0 });
  if (draws[draws.length - 1].card !== false)
    throw new Error("REGRESSION: the re-projection was drawn beside a card that had not arrived: " + drawn());
  __releaseCardsNewestFirst();
  await flush();
  if (el("kgcard")._html.indexOf("TOP ENTITIES") === -1)
    throw new Error("REGRESSION: the seed's late card replaced the re-projection's: " + el("kgcard")._html);
  if (draws[draws.length - 1].card !== true)
    throw new Error("REGRESSION: the re-projection was not drawn once more beside its arrived card: " + drawn());

  console.log("OK every-view-drawn-in-the-room-its-panes-leave");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// Driver for a shown card across re-renders of its subject: the seed's card is shown, then the
/// overlay toggle's re-render and a tier toggle's re-render each draw the seed once more, beside
/// the card, without the dock ever hiding or holding anything but that card and without a card
/// request; then the tests filter re-reads the seed, and the card stays shown while the re-read
/// card is fetched.
const CARD_KEPT_DRIVER: &str = r#"
;(async function(){
  await flush();
  await seedGraph("combat.rs::fire");
  __releaseCardsNewestFirst();
  await flush();
  const dock = el("kgcard");
  if (dock.hidden !== false || dock._html.indexOf("combat.rs:42") === -1)
    throw new Error("setup: the seed's card must show: " + dock._html);
  const shown = dock._html;
  const lapses = [];
  let hidden = dock.hidden;
  Object.defineProperty(dock, "hidden", { configurable: true, get(){ return hidden; },
    set(v){ if (v) lapses.push("hidden"); hidden = v; } });
  Object.defineProperty(dock, "innerHTML", { configurable: true, get(){ return this._html; },
    set(v){ if (String(v) !== shown) lapses.push(JSON.stringify(String(v))); this._html = String(v); } });
  const draws = __recordDraws();

  rerenderCurrentView();
  renderGraph(kgLast);
  await flush();
  if (lapses.length)
    throw new Error("REGRESSION: a re-render of the same subject hid or blanked its card: " + lapses.join(", "));
  if (__heldCards.length)
    throw new Error("REGRESSION: a re-render of the same subject fetched its card again (" + __heldCards.length + " requests)");
  if (JSON.stringify(draws.map(d => d.card)) !== JSON.stringify([true, true]))
    throw new Error("REGRESSION: each re-render must draw the seed once, beside its card: " + JSON.stringify(draws));

  setTests("include");
  await flush();
  if (lapses.length)
    throw new Error("REGRESSION: re-reading the same subject hid or blanked its card before the re-read card landed: " + lapses.join(", "));
  if (__heldCards.length !== 1)
    throw new Error("REGRESSION: re-reading the subject under another tests filter must fetch its card once: " + __heldCards.length);
  __releaseCardsNewestFirst();
  await flush();
  if (dock.hidden !== false || dock._html.indexOf("combat.rs:42") === -1)
    throw new Error("REGRESSION: the re-read card did not replace the shown one: " + dock._html);

  console.log("OK a-shown-card-survives-a-re-render");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// Driver for subjects whose answer names no card: a seed with no card is drawn once, with the
/// dock hidden, before and after its answer; a files-lens drill of a cluster with no card the
/// same; and a subject with no card after one whose card showed is drawn without that card.
const NO_CARD_DRIVER: &str = r#"
;(async function(){
  await flush();
  const draws = __recordDraws();
  const narrowed = () => draws.some(d => d.card);

  renderGraph({ seed: "nowhere.rs::ghost", nodes: [], edges: [], memory: null });
  await flush();
  __releaseCardsNewestFirst();
  await flush();
  if (narrowed() || draws.length !== 1)
    throw new Error("REGRESSION: a seed with no card narrowed or redrew the drawing: " + JSON.stringify(draws));

  kgLens = "files";
  await drillCluster("nowhere");
  await flush();
  __releaseCardsNewestFirst();
  await flush();
  if (narrowed() || draws.length !== 2)
    throw new Error("REGRESSION: a drill with no card narrowed or redrew the drawing: " + JSON.stringify(draws));

  renderGraph({ seed: "combat.rs::fire", nodes: [], edges: [], memory: null });
  __releaseCardsNewestFirst();
  await flush();
  if (el("kgcard").hidden !== false) throw new Error("setup: the seed's card must show");
  const from = draws.length;
  renderGraph({ seed: "nowhere.rs::ghost", nodes: [], edges: [], memory: null });
  await flush();
  __releaseCardsNewestFirst();
  await flush();
  if (draws.slice(from).some(d => d.card) || el("kgcard").hidden !== true)
    throw new Error("REGRESSION: a subject with no card was drawn beside the last subject's card: " + JSON.stringify(draws.slice(from)));

  console.log("OK a-subject-with-no-card-never-narrows-the-drawing");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// Driver for each view's own panes, recorded at each drawing of the panel: the seed of `fire`
/// (its rail and card shown), then a call walk of `fire` and one of `reload`; the seed of
/// `reload` and a call walk of `fire`; a files-lens and a code-lens drill; a re-projection of
/// another subject and one of the seed; then every view whose read fails, each after the seed of
/// `fire` again.
const OWN_PANES_DRIVER: &str = r#"
;(async function(){
  await flush();
  const draws = __recordDraws();
  const at = () => JSON.stringify(draws[draws.length - 1]);
  const both = JSON.stringify({ card: true, rail: true });
  const neither = JSON.stringify({ card: false, rail: false });
  const card = () => (el("kgcard").hidden ? "" : el("kgcard")._html);
  const MEMORY = { decisions: [], findings: [], concepts: [] };
  const seed = async id => {
    renderGraph({ seed: id, nodes: [], edges: [], memory: MEMORY });
    await flush();
    if (el("kgrail").hidden) throw new Error("setup: the seed of " + id + " must show its rail");
  };

  await seed("combat.rs::fire");
  await seedCalls("combat.rs::fire", "down");
  if (at() !== both || card().indexOf("combat.rs:42") === -1)
    throw new Error("REGRESSION: the call walk of the seed was drawn without the seed's rail or card: " + at());
  await seedCalls("combat.rs::reload", "up");
  if (at() !== neither)
    throw new Error("REGRESSION: the call walk of another subject was drawn beside the seed's panes: " + at());
  await flush();
  if (!el("kgcard").hidden || !el("kgrail").hidden)
    throw new Error("REGRESSION: the call walk of a subject with no card kept the seed's panes open");

  await seed("combat.rs::reload");
  await seedCalls("combat.rs::fire", "down");
  if (at() !== neither)
    throw new Error("REGRESSION: the call walk of another subject was drawn beside the last seed's rail: " + at());
  await flush();
  if (card().indexOf("combat.rs:42") === -1 || !el("kgrail").hidden)
    throw new Error("REGRESSION: the call walk must card its own seed and show no other subject's rail: " + card());

  await seed("combat.rs::fire");
  kgLens = "files";
  await drillCluster("combat.rs");
  if (at() !== neither)
    throw new Error("REGRESSION: a files-lens drill was drawn beside the seed's rail or card: " + at());
  await seed("combat.rs::fire");
  kgLens = "code";
  await drillCluster("community/1/0");
  if (at() !== neither)
    throw new Error("REGRESSION: a code-lens drill was drawn beside the seed's rail or card: " + at());
  await flush();

  await seed("combat.rs::fire");
  renderReprojection({ subject: "combat.rs", clusters: [], edges: [], total: 0 });
  if (at() !== neither)
    throw new Error("REGRESSION: a re-projection of another subject was drawn beside the seed's rail or card: " + at());
  await flush();
  await seed("combat.rs::fire");
  renderReprojection({ subject: "combat.rs::fire", clusters: [], edges: [], total: 0 });
  if (at() !== both)
    throw new Error("REGRESSION: a re-projection of the seed was drawn without the seed's rail or card: " + at());
  await flush();

  const answered = __view;
  let failing = false;
  __view = function(url){ if (failing) throw new Error("unreadable"); return answered(url); };
  const unreadable = [
    ["the neighborhood", () => seedGraph("combat.rs")],
    ["the path", () => { kgSeed = "combat.rs::fire"; return pathTo("combat.rs"); }],
    ["the files-lens drill", () => { kgLens = "files"; return drillCluster("combat.rs"); }],
    ["the call walk", () => seedCalls("combat.rs::fire", "down")],
    ["the re-projection", () => reprojectSubject("combat.rs::fire")],
    ["the overview", () => loadKgOverview()]
  ];
  for (const [name, show] of unreadable) {
    failing = false;
    await seed("combat.rs::fire");
    failing = true;
    await show();
    if (at() !== neither)
      throw new Error("REGRESSION: " + name + ", unreadable, showed its message beside the last subject's panes: " + at());
    await flush();
    if (!el("kgcard").hidden || !el("kgrail").hidden)
      throw new Error("REGRESSION: a pane opened over the message of " + name);
  }

  console.log("OK each-view-sets-both-panes-for-its-own-subject");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// Driver for the CARD's OWN WIRING (spec 63 c2): `renderCard`/`loadCard` are proven above in
/// isolation (called directly with fixture data), which is structurally blind to whether the
/// FOUR real call sites the diff added - `renderGraph`, `renderReprojection`, `drillCluster`, and
/// `loadKgOverview` - actually invoke them. `tests/dash_kg_graph_route.rs` and
/// `tests/subject_lens_overlay_served_page.rs` were retargeted to TOLERATE the additive `card=`
/// fetch those call sites now make, but neither asserts the fetch happens - a silently-dropped
/// `loadCard`/`renderCard(null)` call at any of the four sites would regress unnoticed. This
/// drives each call site directly against the shared `RESOLVING_FETCH` fixture and asserts the
/// card that lands in `#kgcard` (or its absence) matches that site's own documented contract:
/// `renderGraph`/`renderReprojection` card their seed/subject; a files/concepts-lens
/// `drillCluster` cards its cluster (a code-lens drill names no card, so it hides any stale one);
/// `loadKgOverview` cards nothing at the whole-graph altitude.
const WIRING_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  // The page's own on-load overview settles first, so it is never a later view than the call
  // site driven below (a later view's hidden card drops an earlier view's card).
  await flush();

  // --- renderGraph(g) must card g.seed -----------------------------------------------------
  renderGraph({ seed: "combat.rs::fire", nodes: [], edges: [] });
  await flush();
  if (el("kgcard").hidden !== false) throw new Error("REGRESSION: renderGraph did not card its seed at all");
  let html = el("kgcard")._html;
  if (html.indexOf("fire") === -1 || html.indexOf("combat.rs:42") === -1)
    throw new Error("REGRESSION: renderGraph must fetch+render the SEED's own card: " + html);

  // --- renderReprojection(rp) must card its subject ----------------------------------------
  renderReprojection({ subject: "combat.rs", clusters: [], edges: [], total: 0 });
  await flush();
  html = el("kgcard")._html;
  if (html.indexOf("TOP ENTITIES") === -1)
    throw new Error("REGRESSION: renderReprojection must fetch+render the SUBJECT's own card: " + html);

  // --- drillCluster(key): a FILES-lens drill cards its cluster (top entities) -------------
  kgLens = "files";
  await drillCluster("combat.rs");
  await flush();
  html = el("kgcard")._html;
  if (el("kgcard").hidden !== false || html.indexOf("TOP ENTITIES") === -1)
    throw new Error("REGRESSION: a files-lens drill must card its cluster: " + html);

  // --- drillCluster(key): a CONCEPTS-lens drill cards its cluster (top evidence) ----------
  kgLens = "concepts";
  await drillCluster("concept/combat");
  await flush();
  html = el("kgcard")._html;
  if (el("kgcard").hidden !== false || html.indexOf("TOP EVIDENCE") === -1)
    throw new Error("REGRESSION: a concepts-lens drill must card its cluster: " + html);

  // --- drillCluster(key): a CODE-lens drill's cluster is a community - no card exists -----
  kgLens = "code";
  await drillCluster("combat.rs::fire");
  await flush();
  if (el("kgcard").hidden !== true)
    throw new Error("REGRESSION: a code-lens drill names no card and must hide any stale one");

  // --- loadKgOverview() cards nothing at the whole-graph altitude -------------------------
  renderCard({ id: "combat.rs", kind: "file", label: "combat.rs", degree: 1, concepts: [],
    decisions: 0, findings: 0, top_entities: [], top_evidence: [] });
  if (el("kgcard").hidden !== false) throw new Error("setup: the card must be visible before loadKgOverview");
  await loadKgOverview();
  if (el("kgcard").hidden !== true)
    throw new Error("REGRESSION: loadKgOverview must hide the card - no subject at the whole-graph altitude");

  console.log("OK metadata-card-wiring-fires-at-every-call-site");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;
