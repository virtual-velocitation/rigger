//! The one stable, dependency-free hash every derived id goes through.

/// FNV-1a/64 over `bytes`: the workspace's ONE stable, dependency-free hash (never for security).
/// Its output is identical across processes, machines, builds and releases, unlike `std`'s
/// `DefaultHasher`, so every id derived through it reproduces: a playbook's slug (the pool is a
/// reproducible projection - the same lesson text always rebuilds to the same file name), a
/// criterion's stable id, a gate's input digest, a project id derived from its remote, a registry
/// entry's file stem, a source file's content key and a coupling community's id.
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

#[cfg(test)]
mod tests {
    use super::fnv1a_64;

    /// The published FNV-1a/64 test vectors: a seed, prime or algorithm change moves every id
    /// derived from the hash, so each one is pinned.
    #[test]
    fn fnv1a_64_answers_the_published_test_vectors() {
        let rows: [(&[u8], u64); 3] = [
            (b"", 0xcbf2_9ce4_8422_2325),
            (b"a", 0xaf63_dc4c_8601_ec8c),
            (b"foobar", 0x8594_4171_f739_67e8),
        ];
        for (bytes, hash) in rows {
            assert_eq!(
                fnv1a_64(bytes),
                hash,
                "{:?}",
                String::from_utf8_lossy(bytes)
            );
        }
    }
}
