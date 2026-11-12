//! The binary crate's prompt texts (spec 121, criterion 16), each one `pub const` whose doc
//! comment opens with its class: `rigger grep-guard`'s deny reason, `rigger guard-write`'s deny
//! reason and the stderr texts of its exit-2 refusals, and `rigger prime`'s fixed texts. The
//! renderers in `src/cli` format from these through `fill` wherever a const carries a `{slot}`.
//! [`all_groups`] is the one enumeration of every crate's registry groups. The registry types
//! and `fill` are `rigger_domain::prompt_surface`'s. The consts and this crate's own groups are
//! pure and compile in every lane; the outer crates' groups carry the lib's store predicate.

#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod tests {
    use super::*;

    /// `all_groups()` holds every crate's `prompt_groups()`, each group once, in the order
    /// domain, conductor, driver, dash, then this crate's.
    #[test]
    fn all_groups_holds_every_crate_s_groups_in_crate_order() {
        let crate_lists = [
            rigger_domain::prompt_surface::prompt_groups(),
            rigger_conductor::prompt_surface::prompt_groups(),
            rigger_driver::prompt_surface::prompt_groups(),
            rigger_dash::prompt_surface::prompt_groups(),
            prompt_groups(),
        ];
        let expected: Vec<(&str, &str, &str)> = crate_lists
            .iter()
            .flat_map(|groups| groups.iter())
            .map(|g| (g.surface, g.name, g.text))
            .collect();
        let held: Vec<(&str, &str, &str)> = all_groups()
            .iter()
            .map(|g| (g.surface, g.name, g.text))
            .collect();
        assert_eq!(
            held, expected,
            "all_groups() is the five crates' groups in crate order"
        );
    }
}
