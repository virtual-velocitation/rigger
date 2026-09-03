//! Periphery (contract) test for spec 77 criterion 2's ASSIGNMENT side of the "assignment
//! and reclaim must never diverge" property `driver::replay::mutation_scratch_path`'s own
//! doc comment names as the reason this is ONE authority: the seeded implementer persona
//! (`.rigger/agents/rust-engineer.md`) hand-documents a literal, hex-escaped TMPDIR leaf
//! template (`<unit>_2fimplementer_23<attempt>`) and a worked example, entirely as prose an
//! LLM implementer reads and manually substitutes into a shell command before running
//! `cargo mutants` - there is no code path connecting that hand-authored text to the real
//! `liveness::marker_filename` injective encoding `mutation_scratch_path` (the RECLAIM side,
//! `main.rs::reclaim_spawn_scratch`) actually computes for the SAME spawn.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/driver/replay.rs`'s own unit tests pin `mutation_scratch_path`'s output for
//! specific literal inputs. `src/main.rs`'s own unit test
//! (`implementer_persona_pins_the_seeded_mutation_scratch_root_registration_contract`) pins
//! that the persona file's TEXT contains a specific literal substring. Both are
//! self-consistent in isolation, and both would keep passing if a future change to
//! `marker_filename`'s escaping (widening `_XX` to `_XXX`, escaping `/` some other way, or
//! any other reshaping of the encoding) updated the CODE side of this contract but the
//! hand-authored prose template in the persona file were left stale, or vice versa: the two
//! independently-maintained literal strings would simply drift apart, silently. An LLM
//! implementer following the stale prose would point `cargo mutants`' `TMPDIR` at a
//! directory `mutation_scratch_path` (and so `rigger result`'s registered-scratch-root
//! reclaim) no longer resolves to - leaking the tree forever and never reclaiming it,
//! exactly the failure this whole authority exists to prevent.
//!
//! This file closes that gap over the crate's PUBLIC surface, using the REAL production
//! encoding function and the REAL checked-in persona file - never a third hand-typed copy
//! of the escaping rule: it extracts the persona's own committed template and proves that
//! substituting real unit/attempt values into it, byte for byte, always equals what
//! `mutation_scratch_path` actually computes for the matching real spawn id.

use std::path::Path;

use rigger::driver::replay::mutation_scratch_path;

fn read_persona() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".rigger/agents/rust-engineer.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the seeded rust-engineer persona must be readable: {e}"))
}

/// Pulls the literal TMPDIR leaf template the persona documents immediately after
/// `rigger-mutants/`, up to the closing double quote of the shell string - e.g.
/// `<unit>_2fimplementer_23<attempt>`. Reads it out of the committed file rather than
/// retyping it, so this test tracks whatever the persona actually says, not a frozen copy.
fn persona_tmpdir_leaf_template(persona: &str) -> &str {
    const ROOT_MARKER: &str = "rigger-mutants/";
    let after_root = persona
        .find(ROOT_MARKER)
        .map(|i| &persona[i + ROOT_MARKER.len()..])
        .expect("the persona must name the rigger-mutants registered scratch root");
    let end = after_root
        .find('"')
        .expect("the TMPDIR value is a double-quoted shell string with a closing quote");
    &after_root[..end]
}

#[test]
fn persona_tmpdir_template_is_the_two_placeholder_shape_this_test_relies_on() {
    // A narrow sanity check ahead of the parameterized round-trip below: if the persona's
    // own template shape ever changes (a new placeholder, a dropped one, reordered
    // tokens), THIS test should name that plainly rather than let the round-trip test
    // fail on a confusing per-case mismatch.
    let persona = read_persona();
    let template = persona_tmpdir_leaf_template(&persona);
    assert_eq!(
        template, "<unit>_2fimplementer_23<attempt>",
        "the persona's TMPDIR leaf template changed shape; update this test's \
         understanding of it deliberately rather than let the round-trip test below fail \
         on a confusing per-case mismatch. Got: {template:?}"
    );
}

#[test]
fn persona_tmpdir_template_matches_the_real_mutation_scratch_encoding_for_every_case() {
    let persona = read_persona();
    let template = persona_tmpdir_leaf_template(&persona);
    let cache_home = Path::new("/home/u/.cache");

    // Representative unit/attempt pairs, including the persona's own worked example
    // (`u77c2/implementer#4`) and a dashed unit id (`-` passes `marker_filename` through
    // unescaped, unlike every other non-alphanumeric byte) to prove the round-trip holds
    // generally, not merely for the one numeral the persona happens to spell out.
    let cases: &[(&str, &str)] = &[("u77c2", "4"), ("u1", "0"), ("unit-with-dashes", "12")];

    for (unit, attempt) in cases {
        let expected_leaf = template
            .replace("<unit>", unit)
            .replace("<attempt>", attempt);
        let spawn_id = format!("{unit}/implementer#{attempt}");

        let computed = mutation_scratch_path(cache_home, &spawn_id).unwrap_or_else(|| {
            panic!("[{spawn_id}] a well-formed spawn id must never encode to None")
        });
        let computed_leaf = computed
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("[{spawn_id}] mutation_scratch_path always nests one leaf directly under cache_home"));

        assert_eq!(
            computed_leaf, expected_leaf,
            "[{spawn_id}] the persona's own hand-authored TMPDIR template, with <unit> and \
             <attempt> substituted, must produce the EXACT SAME leaf mutation_scratch_path \
             (the reclaim authority `rigger result` actually calls) computes for the \
             matching spawn id - a future change to the encoding rule that updates the code \
             but leaves this prose stale must fail HERE, not silently leak an agent's \
             cargo-mutants tree that `rigger result` can no longer find to reclaim; \
             persona template: {template:?}"
        );
    }

    // And the persona's own literal worked example (`.../rigger-mutants/u77c2_2fimplementer_234`
    // for spawn `u77c2/implementer#4`) must still be present verbatim in the committed
    // text, tying the prose's illustrative instance to the same real computed value above -
    // not merely to the abstract template.
    let worked_example_leaf = mutation_scratch_path(cache_home, "u77c2/implementer#4")
        .unwrap()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
        .to_string();
    assert!(
        persona.contains(&worked_example_leaf),
        "the persona's own worked example must literally contain `{worked_example_leaf}` - \
         the real leaf mutation_scratch_path computes for spawn `u77c2/implementer#4`, the \
         exact spawn id the persona's prose uses as its illustration; persona text:\n{persona}"
    );
}
