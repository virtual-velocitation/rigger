//! Docs-overlay fixtures: what this repository's `.rigger/docs-overlay.yml` installs into the
//! registry skills it carries a fact for.

/// The project list this repository's overlay sets under step 6 of the installed
/// `planning-a-spec`, between step 6's paragraph and step 7: its two Global constraints, one
/// `- ` line each, in the overlay's order.
pub const THIS_REPOSITORYS_PLANNING_CONSTRAINTS: &str =
    "This project's:\n- Hyphens not em dashes (U+2014 fails the diff gate).\n- Both feature \
     lanes green.\n\n";
