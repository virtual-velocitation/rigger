//! The checks every crate's `prompt_surface` tests share (spec 121, criterion 16): the registry
//! test over a module's source and its groups, which one macro generates in each crate, and the
//! concatenation test's group pick and whitespace-normalized comparison. Each reads only the
//! source text and the groups its caller passes, never a file at run time.

use rigger_domain::prompt_surface::Group;
use std::cell::RefCell;

/// The class words a registry const's doc comment opens with, each followed by a colon.
const CLASS_WORDS: [&str; 2] = ["CONTRACT", "FRAMING"];

/// Every `pub const` the module `source` declares, in source order, beside the class word that
/// opens the first line of its doc comment. Fails on a const whose doc comment is missing or
/// whose first line opens with neither `CONTRACT:` nor `FRAMING:`.
pub fn const_classes(source: &str) -> Vec<(String, String)> {
    let mut first_doc: Option<&str> = None;
    let mut pairs = Vec::new();
    for line in source.lines().map(str::trim_start) {
        if let Some(doc) = line.strip_prefix("///") {
            first_doc.get_or_insert(doc.trim_start());
        } else if let Some(rest) = line.strip_prefix("pub const ") {
            let name = rest.split(':').next().unwrap_or(rest).trim();
            let doc = first_doc.take().unwrap_or_default();
            let class = CLASS_WORDS
                .iter()
                .find(|word| doc.strip_prefix(**word).is_some_and(|r| r.starts_with(':')))
                .unwrap_or_else(|| {
                    panic!("pub const {name}'s doc opens with CONTRACT: or FRAMING:, not {doc:?}")
                });
            pairs.push((name.to_string(), class.to_string()));
        } else if !line.starts_with("#[") {
            first_doc = None;
        }
    }
    pairs
}

/// A crate's registry test: each `pub const` in its module `source` opens its doc comment with
/// its class word, the (const, class) pairs equal `pinned` (the crate's rows of THE CLASS TABLE,
/// order aside, pinned or derived by the table's naming rule), and
/// [`assert_groups_name_the_consts`] holds.
pub fn assert_registry<N: AsRef<str>>(source: &str, groups: &[Group], pinned: &[(N, &str)]) {
    let mut classes = const_classes(source);
    classes.sort();
    let mut expected: Vec<(String, String)> = pinned
        .iter()
        .map(|(name, class)| (name.as_ref().to_string(), class.to_string()))
        .collect();
    expected.sort();
    assert_eq!(
        classes, expected,
        "the module's (const, class) pairs are THE CLASS TABLE's rows"
    );
    assert_groups_name_the_consts(source, groups);
}

/// A crate's registry test, `#[test] fn $name`, invoked in a test module whose file sits beside
/// the crate's `prompt_surface.rs` with that module's `prompt_groups` in scope: [`assert_registry`]
/// over that module's source (`include_str!` resolves against the file that invokes the macro),
/// its `prompt_groups()` and `$pinned`, the crate's rows of THE CLASS TABLE.
macro_rules! registry_test {
    ($name:ident, $pinned:expr) => {
        /// Every prompt text of this crate is one const whose doc comment opens with THE CLASS
        /// TABLE's class, each const is a group's text, and no two groups share a surface and a
        /// name.
        #[test]
        fn $name() {
            $crate::prompt_surface_fixtures::assert_registry(
                include_str!("prompt_surface.rs"),
                prompt_groups(),
                $pinned,
            );
        }
    };
}
pub(crate) use registry_test;

/// `groups` name exactly the `pub const`s of the module `source`, so every const is some group's
/// text and every group's text is a const, and no two groups share a surface and a name.
pub fn assert_groups_name_the_consts(source: &str, groups: &[Group]) {
    let mut consts: Vec<String> = const_classes(source)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    consts.sort();
    let mut named: Vec<String> = groups.iter().map(|g| g.name.to_string()).collect();
    named.sort();
    named.dedup();
    assert_eq!(named, consts, "the groups name exactly the module's consts");
    let mut keys: Vec<(&str, &str)> = groups.iter().map(|g| (g.surface, g.name)).collect();
    keys.sort();
    let repeated: Vec<(&str, &str)> = keys
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
        .collect();
    assert_eq!(
        repeated,
        Vec::<(&str, &str)>::new(),
        "no two groups share a surface and a name"
    );
}

/// One concatenation test's picks over a crate's `groups`: each [`GroupPicks::rendered`] call is
/// one branch's expected render, and the picks it records are what the test covers.
pub struct GroupPicks<'g> {
    groups: &'g [Group],
    picked: RefCell<Vec<(&'static str, &'static str)>>,
}

impl<'g> GroupPicks<'g> {
    /// A test's picks over `groups`, none picked yet.
    pub fn new(groups: &'g [Group]) -> Self {
        GroupPicks {
            groups,
            picked: RefCell::new(Vec::new()),
        }
    }

    /// The texts of the groups `names` picks from `surface`, concatenated in registry order: one
    /// branch's expected render, each of those groups recorded as picked under `surface`. Fails
    /// when `names` picks a group `surface` does not hold, so a misspelt surface or name reads red
    /// rather than empty.
    pub fn rendered(&self, surface: &str, names: &[&str]) -> String {
        let picked: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| g.surface == surface && names.contains(&g.name))
            .collect();
        let mut found: Vec<&str> = picked.iter().map(|g| g.name).collect();
        found.sort();
        let mut wanted = names.to_vec();
        wanted.sort();
        assert_eq!(
            found, wanted,
            "{surface:?} holds each group the branch renders"
        );
        self.picked
            .borrow_mut()
            .extend(picked.iter().map(|g| (g.surface, g.name)));
        picked.iter().map(|g| g.text).collect()
    }
}

/// The concatenation test's comparison: `render` equals `expected`, whitespace-normalized.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn assert_renders(case: &str, render: &str, expected: &str) {
    use rigger_domain::wave::normalize_ws;
    assert_eq!(
        normalize_ws(render),
        normalize_ws(expected),
        "{case}: the render is its groups concatenated in registry order"
    );
}
