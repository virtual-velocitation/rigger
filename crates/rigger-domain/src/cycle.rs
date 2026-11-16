//! The one cycle finder over a directed graph: the workflow's stage graph and the def paths an
//! extractor's answer names as parents both ask it.

use std::collections::HashMap;
use std::hash::Hash;

/// The node a depth-first walk meets again while it is still inside that node, when the graph has a
/// cycle: the walk starts from each of `nodes` in their order, skipping a node an earlier start
/// already walked, and steps from a node to each of its `successors` in their order; a node with
/// no successors ends its step. The walk keeps its own stack, so a deep graph read from outside
/// the process never exhausts the thread's. None when the graph has no cycle.
pub fn first_cycle<N, S>(
    nodes: impl IntoIterator<Item = N>,
    successors: impl Fn(N) -> S,
) -> Option<N>
where
    N: Copy + Eq + Hash,
    S: IntoIterator<Item = N>,
{
    // A node is `false` while the walk is inside it and `true` once every step from it is done.
    let mut walked: HashMap<N, bool> = HashMap::new();
    for start in nodes {
        if walked.contains_key(&start) {
            continue;
        }
        walked.insert(start, false);
        let mut walk = vec![(start, successors(start).into_iter())];
        while let Some((at, next)) = walk.last_mut() {
            let Some(successor) = next.next() else {
                walked.insert(*at, true);
                walk.pop();
                continue;
            };
            match walked.get(&successor) {
                Some(false) => return Some(successor),
                Some(true) => {}
                None => {
                    walked.insert(successor, false);
                    walk.push((successor, successors(successor).into_iter()));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE WALK ANSWERS THE NODE IT MEETS AGAIN: over each graph's edges, walked from its starts in
    /// order, the finder answers the first node met while the walk is still inside it, and none for
    /// a graph whose every path ends, a node reached twice by two paths and an edge to a node with
    /// no edges among them.
    #[test]
    fn the_walk_answers_the_first_node_it_meets_while_still_inside_it() {
        type Row = (
            &'static str,
            &'static [&'static str],
            &'static [(&'static str, &'static str)],
            Option<&'static str>,
        );
        let rows: [Row; 9] = [
            ("no nodes", &[], &[], None),
            ("a chain", &["a", "b", "c"], &[("a", "b"), ("b", "c")], None),
            (
                "a diamond reaches one node twice",
                &["a", "b", "c", "d"],
                &[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")],
                None,
            ),
            ("an edge to a node with none", &["a"], &[("a", "x")], None),
            ("a node its own successor", &["a"], &[("a", "a")], Some("a")),
            (
                "a two-node cycle",
                &["a", "b"],
                &[("a", "b"), ("b", "a")],
                Some("a"),
            ),
            (
                "the start order decides",
                &["b", "a"],
                &[("a", "b"), ("b", "a")],
                Some("b"),
            ),
            (
                "a cycle past an ended start",
                &["a", "b", "c"],
                &[("b", "c"), ("c", "b")],
                Some("b"),
            ),
            (
                "the successor order decides",
                &["a", "b", "c"],
                &[("a", "c"), ("a", "b"), ("b", "a"), ("c", "c")],
                Some("c"),
            ),
        ];
        for (why, nodes, edges, want) in rows {
            let successors = |node: &'static str| {
                edges
                    .iter()
                    .filter(move |(from, _)| *from == node)
                    .map(|&(_, to)| to)
            };
            assert_eq!(
                first_cycle(nodes.iter().copied(), successors),
                want,
                "{why}"
            );
        }
    }
}
