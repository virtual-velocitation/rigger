//! The pure half of the playbook distiller: the stable hash and the playbook value type.

/// FNV-1a/64 over `bytes`: the crate's ONE stable, dependency-free hash (never for security).
/// Its output is identical across processes, machines, builds and releases, unlike `std`'s
/// `DefaultHasher`, so every id derived through it reproduces: a playbook's slug (the pool is a
/// reproducible projection - the same lesson text always rebuilds to the same file name), a
/// criterion's stable id, a gate's input digest, and a project id derived from its remote.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// One distilled playbook: a deduplicated lesson, the blast-radius files that trigger it,
/// and how many `LessonLearned` events folded into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playbook {
    /// The stable slug (`playbook-<16 hex>` of the summary) that names the pool file and
    /// the frontmatter `id`. Deriving it from the summary makes the SAME lesson dedup to
    /// the SAME playbook across runs.
    pub id: String,
    /// The distilled lesson body (the deduplicated `LessonLearned` summary).
    pub summary: String,
    /// The TRIGGER PREDICATE: the sorted union of every folded lesson's `about` files, the
    /// blast radius the injector overlaps against an agent's grounded seed to rank relevance.
    pub triggers: Vec<String>,
    /// How many `LessonLearned` events collapsed into this one playbook (>= 1).
    pub lessons: usize,
}
