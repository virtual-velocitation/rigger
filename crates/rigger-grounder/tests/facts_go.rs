//! GO STAGES ITS ROW (spec 117, OTHER LANGUAGES): the facts the bundled Go grammar gives a file
//! hold a type mention for a type identifier in type position, a field mention for a selector
//! that is not called while a called one stays a call, and a function's doc span from the `//`
//! block above it.

#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

extern crate rigger_grounder as rigger;

#[path = "../../../tests/common/symbol_index.rs"]
mod symbol_index;

use rigger_domain::code_facts::Mention;
use rigger_domain::code_view::{Entry, Extract};
use rigger_domain::contextgraph::{REF_KIND_FIELD, REF_KIND_TYPE};
use rigger_grounder::facts::FactsExtractor;
use symbol_index::mention_in;

/// A case: what it shows, the Go source, the mentions its facts hold, whole and in order, and
/// each definition's leaf with its doc span, in order.
type Case = (
    &'static str,
    &'static str,
    Vec<Mention>,
    Vec<(&'static str, Option<(u32, u32)>)>,
);

/// GO STAGES ITS ROW: a parameter's type is a `type` mention and no call; `x.f` not called is a
/// `field` mention, while a called `x.g` is a call and no field, the selector `x.h` in its
/// arguments a field; and a `//` block above a function is that function's doc span, over the
/// production extractor.
#[test]
fn go_stages_its_row() {
    // A mention inside the file's one function, the definition at index 0, named by its leaf.
    let in_fn = |name, kind, line| mention_in(Some(0), name, name, kind, line, false);
    let cases: [Case; 4] = [
        (
            "a parameter's type",
            "package p\n\nfunc Draw(s Shape) {}\n",
            vec![in_fn("Shape", Some(REF_KIND_TYPE), 3)],
            vec![("Draw", None)],
        ),
        (
            "a selector not called and a called one",
            "package p\n\nfunc Move(x T) {\n\tx.f = x.g()\n}\n",
            vec![
                in_fn("T", Some(REF_KIND_TYPE), 3),
                in_fn("f", Some(REF_KIND_FIELD), 4),
                in_fn("g", None, 4),
            ],
            vec![("Move", None)],
        ),
        (
            "a selector in a called selector's arguments",
            "package p\n\nfunc Pass(x T) {\n\tx.g(x.h)\n}\n",
            vec![
                in_fn("T", Some(REF_KIND_TYPE), 3),
                in_fn("g", None, 4),
                in_fn("h", Some(REF_KIND_FIELD), 4),
            ],
            vec![("Pass", None)],
        ),
        (
            "a `//` block above a function",
            "package p\n\n// Zero is nothing.\n// It reads nothing.\nfunc Zero() {}\n",
            vec![],
            vec![("Zero", Some((3, 4)))],
        ),
    ];
    for (case, source, mentions, docs) in cases {
        let entry = Entry {
            path: "p.go".to_string(),
            blob: String::new(),
        };
        let facts = FactsExtractor.extract(&entry, source.as_bytes());
        assert_eq!(facts.mentions, mentions, "{case}: the mentions");
        assert_eq!(
            facts
                .defs
                .iter()
                .map(|d| (d.leaf.as_str(), d.doc))
                .collect::<Vec<_>>(),
            docs,
            "{case}: each definition's doc span"
        );
    }
}
