//! THE READERS FIXTURE: the one Rust source text the graph's readers (spec 117) are proven over
//! through the built binary: the lookup's referrers by kind, under each tests filter and through a
//! worktree's overlay; `--show`'s doc and referrers; the MCP lookup's reply against the CLI's; and
//! a read served from the settled code half against the same read resolved near its ids. It names
//! no crate, so every lane compiles it.

/// A source file whose documented constant `LIMIT` is read by two product functions (`spill`
/// before `fits`, so source order is not name order), by a `#[test]` function of the test module
/// that imports it and by the `#[cfg(test)]` twin of the product `helper`, and whose type `Bin` is
/// named in `spill`'s signature.
/// Its function `ready` is called on a body line of `start`, then on two of `begin`'s (source order
/// again not name order), and by a `#[test]` function.
pub const READERS: &str = "\
/// The most widgets one bin holds.
/// It bounds every bin.
pub const LIMIT: usize = 3;

pub struct Bin {
    pub count: usize,
}

pub fn spill(bin: &Bin) -> usize {
    let over = bin.count;
    over - LIMIT
}

pub fn fits(n: usize) -> bool {
    let room = n;
    room <= LIMIT
}

#[cfg(test)]
fn helper() -> usize {
    LIMIT
}
#[cfg(not(test))]
fn helper() -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::LIMIT;

    #[test]
    fn the_bound_holds() {
        let n = LIMIT;
    }
}

pub fn ready() -> bool {
    true
}

pub fn start() -> bool {
    ready()
}

pub fn begin() -> bool {
    let first = ready();
    first || ready()
}

#[test]
fn it_starts() {
    let up = ready();
}
";
