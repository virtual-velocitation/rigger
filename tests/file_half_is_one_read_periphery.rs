//! Periphery (API / integration) tests for spec 119 criterion 13, THE FILE HALF IS ONE READ:
//! `query::node_file(kind, id)` gives the file an id names by one kind table, and every reader
//! that folds a node by its file reads it. The crates' own tests pin each reader from inside its
//! crate; these run OUTSIDE the crates and guard the boundaries the rule crosses:
//!
//!  - the public kind table: `rigger::contextgraph::query::node_file` over all sixteen graph
//!    kinds, each conditional arm from both sides (an id it names a file for, and one it names
//!    none for), the unconditional `file` arm by the ids its last segment would otherwise refuse,
//!    and the arm naming no file by every other kind;
//!  - the readers agreeing across crates over one in-memory graph: the files lens whole and the
//!    card (`contextgraph::query`), the files lens re-grained (`dash`), and the console map's
//!    district (`console`).
//!
//! Every test runs on both lanes.

mod common;

use common::fixtures::{card_file, kinded_def_node, one_community_graph, plain, ONE_COMMUNITY};
use common::lens::files_lens_rows;
use rigger::console::map;
use rigger::contextgraph::query::node_file;
use rigger::contextgraph::{
    KIND_AGENT, KIND_ARCH_DECISION, KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT,
    KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, KIND_FINDING, KIND_GATE, KIND_HANDBOOK_RULE,
    KIND_LESSON, KIND_RATIONALE, KIND_STAGE, KIND_UNIT,
};

/// GIVEN an id of each of the sixteen graph kinds, WHEN `node_file` reads it, THEN a code
/// entity names its `::` file half whatever its shape, and none when it holds no `::` or an empty
/// file half; a file names its whole path whatever its last segment; a design doc, architecture
/// decision, handbook rule, rationale or artifact names the part before its first `#` only when
/// that part's last segment carries an extension; and every other kind names none, a dotted or
/// `::`-bearing id included.
#[test]
fn node_file_names_the_file_of_every_graph_kind_by_its_kind_table() {
    let cases: [(&str, &str, Option<&str>); 29] = [
        (KIND_CODE_ENTITY, "Makefile::all", Some("Makefile")),
        (KIND_CODE_ENTITY, "src/a.rs::T::new", Some("src/a.rs")),
        (KIND_CODE_ENTITY, "src/a.rs", None),
        (KIND_CODE_ENTITY, "::x", None),
        (KIND_FILE, "src/utils/Makefile", Some("src/utils/Makefile")),
        (KIND_FILE, ".gitignore", Some(".gitignore")),
        (KIND_FILE, "src/nested/", Some("src/nested/")),
        (KIND_DESIGN_DOC, "docs/a.md#slug", Some("docs/a.md")),
        (KIND_DESIGN_DOC, "docs/README", None),
        (
            KIND_ARCH_DECISION,
            "docs/arch.md#ad-1",
            Some("docs/arch.md"),
        ),
        (KIND_ARCH_DECISION, "ad-1", None),
        (
            KIND_HANDBOOK_RULE,
            "docs/handbook.md#rule",
            Some("docs/handbook.md"),
        ),
        (KIND_HANDBOOK_RULE, "docs/handbook#rule", None),
        (KIND_RATIONALE, "src/a.rs#L3", Some("src/a.rs")),
        (KIND_RATIONALE, "src/Makefile#L3", None),
        (KIND_ARTIFACT, "target/out.json", Some("target/out.json")),
        (KIND_ARTIFACT, "build-log", None),
        (KIND_FINDING, "a.rs::x", None),
        (KIND_LESSON, "notes.txt", None),
        (KIND_DECISION, "v1.2-choice", None),
        (KIND_DECISION, "Makefile::all", None),
        (KIND_UNIT, "u1.2", None),
        (KIND_AGENT, "u1/impl.rs#0", None),
        (KIND_GATE, "gate:lint.sh", None),
        (KIND_STAGE, "build.sh", None),
        (KIND_COMMUNITY, ONE_COMMUNITY, None),
        (KIND_CONCEPT, "concept/a.rs", None),
        // An unknown kind names no file either, whatever its id.
        ("no-such-kind", "src/a.rs::x", None),
        ("", "src/a.rs", None),
    ];
    assert_eq!(
        cases.map(|(kind, id, _)| node_file(kind, id)),
        cases.map(|(_, _, file)| file),
        "each id names the file its kind's arm of the table gives"
    );
}

/// GIVEN one community holding the extensionless file's definitions `Makefile::all` and
/// `Makefile::build`, the placeholder `src/b.rs::%all` of the first's leaf, the definition
/// `src/aaa.rs::g` and a dotted decision `v1.2-choice`, WHEN the files lens folds the whole graph
/// and re-grains the community, the card shows `Makefile::all` and the console map builds its
/// districts, THEN the files lens and the re-grain fold the two definitions and the placeholder
/// under `Makefile` and leave the decision under no file, the card gives `Makefile::all` the file
/// `Makefile`, and the map counts its definitions in `Makefile`'s district, which their two
/// members win over the one `aaa` member, so the one district holds all three named members.
#[test]
fn every_file_reader_folds_an_extensionless_files_entity_under_its_file() {
    let g = one_community_graph(
        ONE_COMMUNITY,
        vec![
            kinded_def_node("Makefile::all", "function"),
            kinded_def_node("Makefile::build", "function"),
            plain("src/b.rs::%all", KIND_CODE_ENTITY),
            kinded_def_node("src/aaa.rs::g", "function"),
            plain("v1.2-choice", KIND_DECISION),
        ],
    );
    assert_eq!(
        files_lens_rows(&g, ONE_COMMUNITY, 5),
        [
            "whole Makefile 3",
            "whole src/aaa.rs 1",
            "concept Makefile 3",
            "concept src/aaa.rs 1",
            "concept v1.2-choice <- []",
        ],
        "an extensionless file's entities fold under it; a dotted decision under no file"
    );
    assert_eq!(
        card_file(&g, "Makefile::all"),
        Some("Makefile".to_string()),
        "the card gives an extensionless file's entity its file"
    );
    assert_eq!(
        map::build(&g)
            .districts
            .iter()
            .map(|d| (d.purpose.as_str(), d.population))
            .collect::<Vec<_>>(),
        [("Makefile", 3)],
        "the two `Makefile` members outvote the one `aaa` member for the district of all three"
    );
}
