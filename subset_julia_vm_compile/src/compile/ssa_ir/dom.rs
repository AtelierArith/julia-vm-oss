//! Reachability and dominator computation over [`SsaFunction`] CFGs.
//!
//! Factored out of the verifier (Issue #8550) so the SSA optimization passes
//! (Issue #8551) reuse the same dominator tree instead of copying it: the
//! verifier checks defs-dominate-uses with it, dead code elimination uses the
//! reachable set, and pure-call CSE scopes its value-numbering table by a
//! depth-first walk of the dominator tree.

use std::collections::{BTreeMap, BTreeSet};

use super::model::{BlockId, SsaFunction};

/// Blocks reachable from the entry along successor edges.
pub(super) fn compute_reachable(func: &SsaFunction) -> BTreeSet<BlockId> {
    let mut reachable = BTreeSet::new();
    let mut stack = vec![func.entry];
    while let Some(block_id) = stack.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        if let Some(block) = func.block(block_id) {
            stack.extend(block.succs.iter().copied());
        }
    }
    reachable
}

/// Immediate dominators of the reachable blocks (entry maps to itself),
/// via the iterative Cooper–Harvey–Kennedy algorithm on reverse post-order.
pub(super) fn compute_idoms(
    func: &SsaFunction,
    reachable: &BTreeSet<BlockId>,
) -> BTreeMap<BlockId, BlockId> {
    let rpo = reverse_post_order(func);
    let rpo_number: BTreeMap<BlockId, usize> = rpo
        .iter()
        .enumerate()
        .map(|(number, block)| (*block, number))
        .collect();

    let mut idoms = BTreeMap::from([(func.entry, func.entry)]);
    let mut changed = true;
    while changed {
        changed = false;
        for &block_id in rpo.iter().skip(1) {
            let Some(block) = func.block(block_id) else {
                continue;
            };
            let mut new_idom: Option<BlockId> = None;
            for &pred in &block.preds {
                if !reachable.contains(&pred) || !idoms.contains_key(&pred) {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => pred,
                    Some(current) => intersect(pred, current, &idoms, &rpo_number),
                });
            }
            let Some(new_idom) = new_idom else { continue };
            if idoms.get(&block_id) != Some(&new_idom) {
                idoms.insert(block_id, new_idom);
                changed = true;
            }
        }
    }
    idoms
}

fn reverse_post_order(func: &SsaFunction) -> Vec<BlockId> {
    let mut post_order = Vec::new();
    let mut visited = BTreeSet::new();
    // Iterative DFS: (block, next successor index to visit).
    let mut stack = vec![(func.entry, 0usize)];
    visited.insert(func.entry);
    while let Some((block_id, succ_index)) = stack.pop() {
        let Some(block) = func.block(block_id) else {
            continue;
        };
        if let Some(&succ) = block.succs.get(succ_index) {
            stack.push((block_id, succ_index + 1));
            if visited.insert(succ) {
                stack.push((succ, 0));
            }
        } else {
            post_order.push(block_id);
        }
    }
    post_order.reverse();
    post_order
}

fn intersect(
    a: BlockId,
    b: BlockId,
    idoms: &BTreeMap<BlockId, BlockId>,
    rpo_number: &BTreeMap<BlockId, usize>,
) -> BlockId {
    let number = |block: BlockId| rpo_number.get(&block).copied().unwrap_or(usize::MAX);
    let mut a = a;
    let mut b = b;
    while a != b {
        while number(a) > number(b) {
            let Some(&next) = idoms.get(&a) else {
                return b;
            };
            a = next;
        }
        while number(b) > number(a) {
            let Some(&next) = idoms.get(&b) else {
                return a;
            };
            b = next;
        }
    }
    a
}

/// Whether `a` dominates `b` (reflexive).
pub(super) fn dominates(
    a: BlockId,
    b: BlockId,
    entry: BlockId,
    idoms: &BTreeMap<BlockId, BlockId>,
) -> bool {
    let mut current = b;
    loop {
        if current == a {
            return true;
        }
        if current == entry {
            return false;
        }
        match idoms.get(&current) {
            Some(&idom) if idom != current => current = idom,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominates_is_reflexive_at_entry() {
        let idoms = BTreeMap::from([(BlockId(0), BlockId(0))]);
        assert!(dominates(BlockId(0), BlockId(0), BlockId(0), &idoms));
    }

    #[test]
    fn dominates_walks_idom_chain() {
        // 0 -> 1 -> 2 linear chain.
        let idoms = BTreeMap::from([
            (BlockId(0), BlockId(0)),
            (BlockId(1), BlockId(0)),
            (BlockId(2), BlockId(1)),
        ]);
        assert!(dominates(BlockId(0), BlockId(2), BlockId(0), &idoms));
        assert!(dominates(BlockId(1), BlockId(2), BlockId(0), &idoms));
        assert!(!dominates(BlockId(2), BlockId(1), BlockId(0), &idoms));
    }
}
