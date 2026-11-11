//! Periphery test for community detection over a real tree: the code store advanced over the
//! two-subsystem sources groups densely-coupled definitions across directory lines. Each
//! cross-file call names a function defined in another file, so a community spanning the
//! directory boundary proves the coupling resolves those calls onto their definitions.

use rigger::code_store::CodeStore;
use rigger_domain::code_view::CodeView;
use rigger_store::code::{Mode, Snapshot};

use crate::common;
use common::fixtures::{two_subsystem_sources, write_files};

/// The code store's view of a directory holding the two-subsystem sources.
pub fn two_subsystem_view() -> Snapshot {
    let dir = tempfile::tempdir().unwrap();
    let sources = two_subsystem_sources();
    let files: Vec<(&str, &str)> = sources
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    write_files(dir.path(), &files);
    CodeStore::in_directory(dir.path())
        .open(Mode::Write)
        .expect("the code store advances over the tree")
        .view
}

#[test]
fn the_pass_groups_densely_coupled_entities_across_directory_lines() {
    let view = two_subsystem_view();
    let groupings = view.groupings();
    let community_of = |member: &str| {
        groupings
            .communities
            .iter()
            .find(|c| c.members.iter().any(|m| m == member))
            .map(|c| c.id.clone())
            .unwrap_or_else(|| panic!("{member} is a community member: {groupings:?}"))
    };

    let a = community_of("src/combat/hit.rs::strike");
    assert_eq!(
        community_of("src/net/link.rs::send"),
        a,
        "strike (combat) and send (net) group into one community across directory lines"
    );
    let b = community_of("src/render/draw.rs::paint");
    assert_eq!(
        community_of("src/ui/hud.rs::layout"),
        b,
        "paint (render) and layout (ui) group into one community across directory lines"
    );
    assert_ne!(a, b, "the thin bridge does not merge the two subsystems");
    assert!(
        groupings
            .communities
            .iter()
            .flat_map(|c| &c.members)
            .all(|m| !m.contains("::%")),
        "a bare placeholder is never a member on its own: {groupings:?}"
    );
}
