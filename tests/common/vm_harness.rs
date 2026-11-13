//! The one node `vm` program the dash client-seam suites run the served page script in, and the
//! one stand-in browser that program gives the page. Mounted once by `tests/all.rs`; a suite
//! reaches it with `use crate::vm_harness;`.
//!
//! Each suite's stand-in has exactly the capabilities its page paths have always met: a missing
//! `querySelector` or `clientHeight` sends the page down a different path (often a throw a promise
//! chain swallows), so [`DomShim`] renders only what a suite asks for, never a superset.

/// A complete node `vm` program: the `shim` parts, one per line (the suite's DOM and `fetch`
/// stand-ins), then the served page script (read from `argv[2]`), then `driver` - which shares
/// the page's scope, so it calls the page's own functions and reads its module state directly.
/// `filename` names the program in node's stack traces.
pub fn vm_harness(shim: &[&str], driver: &str, filename: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__HARNESS_SHIM__`;
const DRIVER = String.raw`__HARNESS_DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "__HARNESS_FILENAME__" });
"##;
    TEMPLATE
        .replace("__HARNESS_FILENAME__", filename)
        .replace("__HARNESS_SHIM__", &shim.join("\n"))
        .replace("__HARNESS_DRIVER__", driver)
}

/// The stand-in browser a suite's page script runs against: `document` with its memoised
/// elements, `window`, the timers, and optionally `location` and `fetch`. Rendered with
/// [`DomShim::render`] as the first shim part of a [`vm_harness`] program.
#[derive(Clone, Copy)]
pub struct DomShim {
    /// What an element's `querySelector` answers.
    pub query: Query,
    /// Whether `addEventListener` records each listener on the element (which then carries
    /// `_listeners` and `dataset`), or ignores it.
    pub listeners: bool,
    /// The state fields an element starts with.
    pub fields: Fields,
    /// The panel's `clientHeight`, beside a `clientWidth` of 800 and an 800x300 bounding box;
    /// `None` gives elements no size at all.
    pub panel_height: Option<u32>,
    /// Whether the panel measures the drawing's slot: a `clientTop`, a `.kgsvg` slot at the top
    /// of the room under the panel's 10px top padding and a one-line 24px toolbar, 776px wide
    /// inside its 12px padding on each side (the drawing's surface, whose width the page draws
    /// at), and the panel's 10px bottom padding through `getComputedStyle`. So a 300px panel
    /// leaves the drawing 776 by 256px, and a 320px one 776 by 276px.
    pub slot: bool,
    /// Whether `setInterval` exists (as an inert timer, like `setTimeout`).
    pub interval: bool,
    /// Whether `location` exists, with an empty `href`.
    pub location: bool,
    /// The `fetch` this shim provides, if any.
    pub fetch: Fetch,
    /// Whether the shim declares `__fetched`, the list a suite's own `fetch` records each URL in.
    pub fetch_log: bool,
}

/// What an element's `querySelector` answers.
#[derive(Clone, Copy)]
pub enum Query {
    /// Elements have no `querySelector`.
    Absent,
    /// A fresh `__Stub` node, whose attributes behave as [`Attrs`] says.
    Stub(Attrs),
    /// A fresh unnamed element; elements then carry the inert attribute methods themselves.
    Element,
}

/// How a `__Stub` node keeps its attributes.
#[derive(Clone, Copy)]
pub enum Attrs {
    /// `setAttribute` records the value as a string; `getAttribute` reads it back, `undefined`
    /// when unset.
    Recorded,
    /// `setAttribute` keeps nothing; `getAttribute` answers `null`.
    Inert,
}

/// The state fields an element starts with.
#[derive(Clone, Copy)]
pub enum Fields {
    /// None.
    Bare,
    /// `hidden` false.
    Shown,
    /// `hidden` false, an empty `className` and `checked` false.
    Form,
    /// `hidden` true and a `classList` that keeps the classes added to it.
    Classed,
}

/// The `fetch` a [`DomShim`] provides.
#[derive(Clone, Copy)]
pub enum Fetch {
    /// None: the suite supplies its own as a later shim part, or the page never fetches.
    Suite,
    /// Every fetch rejects with an `Error` whose message is this JS expression, which may name
    /// the fetched `url`.
    Refused(&'static str),
    /// Every fetch resolves to an empty graph reply with no headers.
    EmptyGraph,
}

impl DomShim {
    /// The explorer panel most drawing suites run in: an 800x300 panel that measures its
    /// drawing's slot, answering `querySelector` with a stub that records its attributes.
    pub const PANEL: DomShim = DomShim {
        query: Query::Stub(Attrs::Recorded),
        listeners: true,
        fields: Fields::Bare,
        panel_height: Some(300),
        slot: true,
        interval: false,
        location: false,
        fetch: Fetch::Suite,
        fetch_log: false,
    };

    /// An inert page for suites that call the page's layout functions directly: an 800x300
    /// panel that does not measure a slot, inert stub nodes and timers, and an empty graph for
    /// every fetch.
    pub const INERT: DomShim = DomShim {
        query: Query::Stub(Attrs::Inert),
        listeners: true,
        fields: Fields::Bare,
        panel_height: Some(300),
        slot: false,
        interval: true,
        location: true,
        fetch: Fetch::EmptyGraph,
        fetch_log: false,
    };

    /// Elements that only hold markup and text: no size, no `querySelector`, listeners ignored.
    pub const MARKUP: DomShim = DomShim {
        query: Query::Absent,
        listeners: false,
        fields: Fields::Bare,
        panel_height: None,
        slot: false,
        interval: false,
        location: false,
        fetch: Fetch::Suite,
        fetch_log: false,
    };

    /// The shim's JS: one line per stand-in, free of backticks and `${` so it embeds verbatim in
    /// the program's `String.raw` template.
    pub fn render(&self) -> String {
        assert!(
            !self.slot || (self.panel_height.is_some() && !matches!(self.query, Query::Absent)),
            "a panel that measures its drawing's slot needs a size and a querySelector"
        );
        let mut js: Vec<String> = vec!["const __els = {};".into()];
        if self.fetch_log {
            js.push("const __fetched = [];".into());
        }
        if let Query::Stub(attrs) = self.query {
            js.push(
                match attrs {
                    Attrs::Recorded => {
                        r#"function __Stub(){ this._attrs = {}; }
__Stub.prototype.setAttribute = function(k,v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return this._attrs[k]; };"#
                    }
                    Attrs::Inert => {
                        r#"function __Stub(){}
__Stub.prototype.setAttribute = function(){};
__Stub.prototype.getAttribute = function(){ return null; };"#
                    }
                }
                .into(),
            );
            js.push(
                r#"__Stub.prototype.addEventListener = function(){};
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; };"#
                    .into(),
            );
        }
        js.push(self.element());
        js.push(
            r#"Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };"#
                .into(),
        );
        if self.panel_height.is_some() {
            js.push(r#"__El.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; };"#.into());
        }
        if self.slot {
            js.push(
                r#"const __slot = { getBoundingClientRect: function(){ return { left: 12, top: 34, width: 776, height: 0 }; } };
const getComputedStyle = function(){ return { paddingBottom: "10px" }; };"#
                    .into(),
            );
        }
        let other = match self.query {
            Query::Absent => None,
            Query::Stub(_) => Some("new __Stub()"),
            Query::Element => Some(r#"new __El("")"#),
        };
        if let Some(other) = other {
            let answer = if self.slot {
                r#"__El.prototype.querySelector = function(sel){ return sel === ".kgsvg" ? __slot : __OTHER__; };"#
            } else {
                r#"__El.prototype.querySelector = function(){ return __OTHER__; };"#
            };
            js.push(answer.replace("__OTHER__", other));
        }
        if let Query::Element = self.query {
            js.push(
                r#"__El.prototype.setAttribute = function(){};
__El.prototype.getAttribute = function(){ return null; };"#
                    .into(),
            );
        }
        js.push(
            if self.listeners {
                r#"__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };"#
            } else {
                r#"__El.prototype.addEventListener = function(){};"#
            }
            .into(),
        );
        js.push(
            r#"const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };"#
                .into(),
        );
        if self.interval {
            js.push("const setInterval = function(){ return 0; };".into());
        }
        if self.location {
            js.push(r#"const location = { href: "" };"#.into());
        }
        match self.fetch {
            Fetch::Suite => {}
            Fetch::Refused(why) => js.push(
                r#"const fetch = function(url){ return Promise.reject(new Error(__WHY__)); };"#
                    .replace("__WHY__", why),
            ),
            Fetch::EmptyGraph => js.push(
                r#"const fetch = function(){ return Promise.resolve({ headers: { get: function(){ return null; } }, json: function(){ return Promise.resolve({ clusters: [], edges: [], total: 0, nodes: [] }); } }); };"#
                    .into(),
            ),
        }
        js.join("\n")
    }

    /// The element constructor, carrying the fields this shim's elements start with.
    fn element(&self) -> String {
        let mut el =
            String::from(r#"function __El(id){ this.id=id; this._html=""; this._text="";"#);
        if self.listeners {
            el.push_str(" this._listeners={}; this.dataset={};");
        }
        el.push_str(match self.fields {
            Fields::Bare => "",
            Fields::Shown => " this.hidden=false;",
            Fields::Form => r#" this.hidden=false; this.className=""; this.checked=false;"#,
            Fields::Classed => {
                r#" this.hidden=true; this._classes={}; const self=this;
  this.classList = { toggle: function(c,on){ if (on) self._classes[c]=true; else delete self._classes[c]; },
    add: function(c){ self._classes[c]=true; }, remove: function(c){ delete self._classes[c]; },
    contains: function(c){ return !!self._classes[c]; } };"#
            }
        });
        if let Some(height) = self.panel_height {
            el.push_str(
                &" this.clientWidth = 800; this.clientHeight = __HEIGHT__;"
                    .replace("__HEIGHT__", &height.to_string()),
            );
        }
        if self.slot {
            el.push_str(" this.clientTop = 0;");
        }
        el.push_str(" }");
        el
    }
}
