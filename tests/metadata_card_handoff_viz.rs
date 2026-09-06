//! Periphery (integration) test for spec 63's METADATA CARD client seam (criterion 2): the served
//! page's OWN `renderCard` draws the "one hover-card anatomy everywhere" - a title row, a
//! provenance row (file:line, degree, community), and per-taxonomy chip rows (FILE / CONCEPTS /
//! MEMORY for a code-entity subject, TOP ENTITIES for a file subject, TOP EVIDENCE for a concept
//! subject) - and a card CHIP actually hands off to its own taxonomy's lens: clicking a FILE or
//! CONCEPTS (or TOP-ENTITIES / TOP-EVIDENCE) chip forces that lens and re-projects the chip's id AS
//! THE SUBJECT (`kgLens`/`kgSubject` land on the target), while a MEMORY chip carries no lens at
//! all - it reuses criterion 5's plain re-seed (`kgSeed` lands on the card's own subject, `kgLens`
//! is left UNTOUCHED) since this design names no memory lens.
//!
//! The implementer's inside-out `dash.rs` tests pin the pure `card`/`Card` surface; this layer
//! proves the CLIENT actually renders it AND that a chip click drives the right seam - the
//! behavioral half a structural grep on the served bytes cannot make, and the boundary the spec's
//! "Test seams" note assigns to a node-gated runtime harness (spec 42/55/59 precedent). Driven by
//! the served page's OWN script under node's built-in `vm` (hermetic, no npm) through a DOM +
//! fetch shim, mirroring `subject_view_memory_rail_client.rs`. `dash` compiles on BOTH the default
//! and the `--no-default-features` lane (the seam is not feature-gated), so this guards the client
//! seam in both lanes.

use std::process::Command;

use rigger::dash;

/// Extract the single inline `<script>` body from the served page (the slice the runtime harness drives).
fn page_script(page: &str) -> &str {
    let open = page
        .find("<script>")
        .expect("the served page carries a <script>")
        + "<script>".len();
    let close = page
        .find("</script>")
        .expect("the served page closes its <script>");
    &page[open..close]
}

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub `ubuntu-latest`,
/// which ships Node.js on PATH, so this runtime guard runs in CI).
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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

/// A `fetch` shim resolving every `/api/graph` request from a tiny in-memory fixture: `card=<id>`
/// serves one of three fixture cards (a code entity, its file, and the concept it realizes); a
/// `seed=&lens=` re-projection or a plain `seed=` neighborhood serves a minimal-but-valid body
/// naming the requested subject (so `kgSubject`/`kgSeed` land on it); anything else (the initial
/// whole-graph overview, `explain=`, `/api/instances`, `/api/state`) serves a graceful empty. Every
/// other kind of URL rejects (matching the live page's own network shape when nothing else answers).
const RESOLVING_FETCH: &str = r#"
const CODE_CARD = { id: "combat.rs::fire", kind: "code-entity", label: "fire",
  file: "combat.rs", line: "42", degree: 5, community: "combat lifecycle",
  concepts: [ { id: "concept/combat", label: "combat resolution" } ],
  decisions: 2, findings: 1, top_entities: [], top_evidence: [] };
const FILE_CARD = { id: "combat.rs", kind: "file", label: "combat.rs",
  degree: 2, concepts: [], decisions: 0, findings: 0,
  top_entities: [ { id: "combat.rs::fire", label: "fire" }, { id: "combat.rs::reload", label: "reload" } ],
  top_evidence: [] };
const CONCEPT_CARD = { id: "concept/combat", kind: "concept", label: "combat resolution",
  degree: 1, concepts: [], decisions: 0, findings: 0,
  top_entities: [], top_evidence: [ { id: "combat.rs::fire", label: "fire" } ] };
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
  const seed = __qval(s, "seed");
  if (seed !== null && __qval(s, "lens") !== null) return { subject: seed, clusters: [], edges: [], total: 0 };
  if (seed !== null) return { seed: seed, depth: 2, nodes: [], edges: [] };
  return { clusters: [], edges: [], total: 0 };
}
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    return Promise.resolve({ json: function(){ return Promise.resolve(__view(url)); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
"#;

/// Assemble a complete node `vm` program: the shared DOM shim, the fetch fixtures, the served page
/// script (read from `argv[2]`), then the driver - which shares the page's scope, so it calls the
/// page's own functions and reads its module state (`kgLens`, `kgSubject`, `kgSeed`) directly.
fn build_harness(driver: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__CARD_SHIM__`;
const DRIVER = String.raw`__CARD_DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-metadata-card-harness.js" });
"##;
    let shim = format!("{DOM_SHIM}\n{RESOLVING_FETCH}");
    TEMPLATE
        .replace("__CARD_SHIM__", &shim)
        .replace("__CARD_DRIVER__", driver)
}

/// Spawn `node` on a self-contained vm harness, asserting it exits 0 and prints `ok_token`.
fn run_node_harness(harness_src: &str, ok_token: &str) {
    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the runtime harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, harness_src).expect("write the runtime harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served client seam");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the runtime harness must drive the metadata-card client seam, but node failed:\n\
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains(ok_token),
        "the runtime harness must confirm '{ok_token}':\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

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
    top_entities: [ { id: "combat.rs::fire", label: "fire" }, { id: "combat.rs::reload", label: "reload" } ],
    top_evidence: [] });
  const fileHtml = el("kgcard")._html;
  if (fileHtml.indexOf("TOP ENTITIES") === -1) throw new Error("REGRESSION: a file card must carry TOP ENTITIES: " + fileHtml);
  if (fileHtml.indexOf('data-seed="combat.rs::fire"') === -1 || fileHtml.indexOf('data-handoff="code"') === -1)
    throw new Error("REGRESSION: a top-entity chip must hand off to the code lens: " + fileHtml);
  if (fileHtml.indexOf("CONCEPTS") !== -1 || fileHtml.indexOf("MEMORY") !== -1)
    throw new Error("REGRESSION: a file's card must carry no FILE/CONCEPTS/MEMORY rows (a code-entity's own rows): " + fileHtml);

  // --- renderCard: a CONCEPT subject's card (TOP EVIDENCE) ------------------------------------
  renderCard({ id: "concept/combat", kind: "concept", label: "combat resolution", degree: 1,
    concepts: [], decisions: 0, findings: 0, top_entities: [],
    top_evidence: [ { id: "combat.rs::fire", label: "fire" } ] });
  const conceptHtml = el("kgcard")._html;
  if (conceptHtml.indexOf("TOP EVIDENCE") === -1) throw new Error("REGRESSION: a concept card must carry TOP EVIDENCE: " + conceptHtml);
  if (conceptHtml.indexOf('data-seed="combat.rs::fire"') === -1 || conceptHtml.indexOf('data-handoff="code"') === -1)
    throw new Error("REGRESSION: an evidence chip must hand off to the code lens: " + conceptHtml);

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

/// RUNTIME guard for spec 63 c2's own Done-when clause: a code subject's card carries file:line,
/// concept chips, and memory counts, AND a card chip resolves to a lens handoff target of the
/// chip's own taxonomy carrying the chosen subject - proven for EVERY card taxonomy (code, file,
/// concept), per the spec's own "criterion 2 owns... every card taxonomy" scope.
#[test]
fn metadata_card_renders_every_taxonomy_and_chips_hand_off_to_their_own_lens() {
    if !node_available() {
        eprintln!(
            "SKIP metadata_card_renders_every_taxonomy_and_chips_hand_off_to_their_own_lens: no \
             `node` runtime on PATH. This runtime guard needs node (present on dev machines and \
             on ubuntu-latest CI); install node to run it."
        );
        return;
    }
    run_node_harness(
        &build_harness(CARD_DRIVER),
        "OK metadata-card-renders-every-taxonomy-and-chips-hand-off-correctly",
    );
}
