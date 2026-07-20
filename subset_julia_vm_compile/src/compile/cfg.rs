//! Bytecode control-flow graph analysis for optimization passes.
//!
//! Issue #5185 needs LICM/CSE consumers for effect information. This module is
//! the first non-behavioral foundation: split flat VM bytecode into basic
//! blocks and recover natural loops from backward edges.

use crate::bytecode::Instr;
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: usize,
    pub start: usize,
    pub end: usize,
    pub successors: Vec<usize>,
    pub predecessors: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaturalLoop {
    pub header: usize,
    pub latch: usize,
    pub blocks: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
    instr_to_block: Vec<usize>,
}

impl ControlFlowGraph {
    pub fn build(code: &[Instr]) -> Self {
        if code.is_empty() {
            return Self {
                blocks: Vec::new(),
                instr_to_block: Vec::new(),
            };
        }

        let leaders = collect_leaders(code);
        let leader_vec = leaders.into_iter().collect::<Vec<_>>();
        let mut instr_to_block = vec![0; code.len()];
        let mut blocks = Vec::with_capacity(leader_vec.len());

        for (idx, start) in leader_vec.iter().enumerate() {
            let end = leader_vec.get(idx + 1).copied().unwrap_or(code.len());
            for block_slot in instr_to_block.iter_mut().take(end).skip(*start) {
                *block_slot = idx;
            }
            blocks.push(BasicBlock {
                id: idx,
                start: *start,
                end,
                successors: Vec::new(),
                predecessors: Vec::new(),
            });
        }

        for block in &mut blocks {
            block.successors = block_successors(code, block, &instr_to_block);
        }

        for block_idx in 0..blocks.len() {
            let successors = blocks[block_idx].successors.clone();
            for succ in successors {
                if let Some(block) = blocks.get_mut(succ) {
                    block.predecessors.push(block_idx);
                }
            }
        }

        Self {
            blocks,
            instr_to_block,
        }
    }

    pub fn block_for_instr(&self, instr_idx: usize) -> Option<usize> {
        self.instr_to_block.get(instr_idx).copied()
    }

    pub fn natural_loops(&self) -> Vec<NaturalLoop> {
        let mut loops = Vec::new();
        for block in &self.blocks {
            for &succ in &block.successors {
                let Some(header) = self.blocks.get(succ) else {
                    continue;
                };
                if header.start > block.start {
                    continue;
                }

                let mut members = HashSet::from([succ, block.id]);
                let mut stack = vec![block.id];
                while let Some(current) = stack.pop() {
                    let Some(current_block) = self.blocks.get(current) else {
                        continue;
                    };
                    for &pred in &current_block.predecessors {
                        if members.insert(pred) {
                            stack.push(pred);
                        }
                    }
                }

                let mut blocks = members.into_iter().collect::<Vec<_>>();
                blocks.sort_unstable();
                loops.push(NaturalLoop {
                    header: succ,
                    latch: block.id,
                    blocks,
                });
            }
        }
        loops
    }
}

fn collect_leaders(code: &[Instr]) -> BTreeSet<usize> {
    let mut leaders = BTreeSet::from([0]);
    for (idx, instr) in code.iter().enumerate() {
        for target in jump_targets(instr) {
            if target < code.len() {
                leaders.insert(target);
            }
        }
        // `PushHandler` does not itself divert control flow (the following
        // instruction runs normally, entering the protected try body), but it
        // is a multi-successor point: an exception can transfer control to
        // `catch_ip`/`finally_ip` from here. Force a block split right after
        // it so `block_successors` (which only inspects a block's *last*
        // instruction) sees `PushHandler` as block-terminating and can attach
        // the catch/finally edges (Issue #10820).
        if (is_branch_or_return(instr) || matches!(instr, Instr::PushHandler(_, _)))
            && idx + 1 < code.len()
        {
            leaders.insert(idx + 1);
        }
    }
    leaders
}

fn block_successors(code: &[Instr], block: &BasicBlock, instr_to_block: &[usize]) -> Vec<usize> {
    if block.end == 0 || block.start >= block.end {
        return Vec::new();
    }

    let instr_idx = block.end - 1;
    let mut successors = Vec::new();
    match &code[instr_idx] {
        Instr::Jump(target) => push_target_block(&mut successors, *target, instr_to_block),
        Instr::JumpIfZero(target)
        | Instr::JumpIfNeI64(target)
        | Instr::JumpIfEqI64(target)
        | Instr::JumpIfLtI64(target)
        | Instr::JumpIfGtI64(target)
        | Instr::JumpIfGtI64Slots(_, _, target)
        | Instr::AddConstI64SlotAndJumpIfLe(_, _, _, target)
        | Instr::JumpIfLeI64(target)
        | Instr::JumpIfGeI64(target)
        | Instr::JumpIfEqF64(target)
        | Instr::JumpIfNeF64(target)
        | Instr::JumpIfNotLtF64(target)
        | Instr::JumpIfNotGtF64(target)
        | Instr::JumpIfNotLeF64(target)
        | Instr::JumpIfNotGeF64(target)
        | Instr::JumpIfCmpI64SlotConst(_, _, _, target) => {
            push_target_block(&mut successors, *target, instr_to_block);
            push_target_block(&mut successors, instr_idx + 1, instr_to_block);
        }
        // A handler installation is not a jump: execution falls through into
        // the protected region, but an exception raised anywhere before the
        // matching `PopHandler` can also transfer control to `catch_ip` (and
        // `finally_ip` on unwind). Model both as real CFG edges so dominance
        // analyses (e.g. the slot-backing verifier, Issue #10820) see that
        // only stores strictly before `PushHandler` are guaranteed visible at
        // the catch/finally entry — not stores made inside the try body.
        Instr::PushHandler(catch_ip, finally_ip) => {
            for target in catch_ip.iter().chain(finally_ip.iter()) {
                push_target_block(&mut successors, *target, instr_to_block);
            }
            push_target_block(&mut successors, instr_idx + 1, instr_to_block);
        }
        instr if is_return(instr) => {}
        _ => push_target_block(&mut successors, instr_idx + 1, instr_to_block),
    }
    successors
}

fn push_target_block(successors: &mut Vec<usize>, target: usize, instr_to_block: &[usize]) {
    if let Some(block) = instr_to_block.get(target).copied() {
        if !successors.contains(&block) {
            successors.push(block);
        }
    }
}

fn jump_targets(instr: &Instr) -> Vec<usize> {
    match instr {
        Instr::Jump(target)
        | Instr::JumpIfZero(target)
        | Instr::JumpIfNeI64(target)
        | Instr::JumpIfEqI64(target)
        | Instr::JumpIfLtI64(target)
        | Instr::JumpIfGtI64(target)
        | Instr::JumpIfGtI64Slots(_, _, target)
        | Instr::AddConstI64SlotAndJumpIfLe(_, _, _, target)
        | Instr::JumpIfLeI64(target)
        | Instr::JumpIfGeI64(target)
        | Instr::JumpIfEqF64(target)
        | Instr::JumpIfNeF64(target)
        | Instr::JumpIfNotLtF64(target)
        | Instr::JumpIfNotGtF64(target)
        | Instr::JumpIfNotLeF64(target)
        | Instr::JumpIfNotGeF64(target)
        | Instr::JumpIfCmpI64SlotConst(_, _, _, target) => vec![*target],
        Instr::PushHandler(catch_ip, finally_ip) => {
            catch_ip.iter().chain(finally_ip.iter()).copied().collect()
        }
        _ => Vec::new(),
    }
}

fn is_branch_or_return(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::Jump(_)
            | Instr::JumpIfZero(_)
            | Instr::JumpIfNeI64(_)
            | Instr::JumpIfEqI64(_)
            | Instr::JumpIfLtI64(_)
            | Instr::JumpIfGtI64(_)
            | Instr::JumpIfGtI64Slots(_, _, _)
            | Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _)
            | Instr::JumpIfLeI64(_)
            | Instr::JumpIfGeI64(_)
            | Instr::JumpIfEqF64(_)
            | Instr::JumpIfNeF64(_)
            | Instr::JumpIfNotLtF64(_)
            | Instr::JumpIfNotGtF64(_)
            | Instr::JumpIfNotLeF64(_)
            | Instr::JumpIfNotGeF64(_)
            | Instr::JumpIfCmpI64SlotConst(_, _, _, _)
    ) || is_return(instr)
}

fn is_return(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::ReturnI64
            | Instr::ReturnF64
            | Instr::ReturnF32
            | Instr::ReturnF16
            | Instr::ReturnArray
            | Instr::ReturnNothing
            | Instr::ReturnAny
            | Instr::ReturnRange
            | Instr::ReturnStruct
            | Instr::ReturnRng
            | Instr::ReturnTuple
            | Instr::ReturnNamedTuple
            | Instr::ReturnDict
            | Instr::ReturnSet
            | Instr::ReturnRef
            | Instr::ReturnMemory
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn cfg_splits_conditional_loop_issue_5185() {
        let code = vec![
            Instr::LoadBool("keep_going".to_string()), // 0
            Instr::JumpIfZero(5),                      // 1
            Instr::LoadI64("x".to_string()),           // 2
            Instr::StoreI64("y".to_string()),          // 3
            Instr::Jump(0),                            // 4
            Instr::ReturnNothing,                      // 5
        ];

        let cfg = ControlFlowGraph::build(&code);

        assert_eq!(cfg.blocks.len(), 3);
        assert_eq!((cfg.blocks[0].start, cfg.blocks[0].end), (0, 2));
        assert_eq!((cfg.blocks[1].start, cfg.blocks[1].end), (2, 5));
        assert_eq!((cfg.blocks[2].start, cfg.blocks[2].end), (5, 6));
        assert_eq!(cfg.blocks[0].successors, vec![2, 1]);
        assert_eq!(cfg.blocks[1].successors, vec![0]);
        assert_eq!(cfg.block_for_instr(3), Some(1));

        let loops = cfg.natural_loops();
        assert_eq!(
            loops,
            vec![NaturalLoop {
                header: 0,
                latch: 1,
                blocks: vec![0, 1],
            }]
        );
    }

    #[test]
    fn cfg_recovers_nested_backward_edges_issue_5185() {
        let code = vec![
            Instr::JumpIfZero(9), // 0 outer header
            Instr::JumpIfZero(5), // 1 inner header
            Instr::PushI64(1),    // 2 inner body
            Instr::Pop,           // 3
            Instr::Jump(1),       // 4 inner latch
            Instr::PushI64(2),    // 5 outer body after inner
            Instr::Pop,           // 6
            Instr::Jump(0),       // 7 outer latch
            Instr::Nop,           // 8 fallthrough leader after outer latch
            Instr::ReturnNothing, // 9
        ];

        let cfg = ControlFlowGraph::build(&code);
        let loops = cfg.natural_loops();

        assert!(loops.contains(&NaturalLoop {
            header: 1,
            latch: 2,
            blocks: vec![1, 2],
        }));
        assert!(loops.contains(&NaturalLoop {
            header: 0,
            latch: 3,
            blocks: vec![0, 1, 2, 3],
        }));
    }

    /// `PushHandler` must be modeled as a real CFG edge to `catch_ip`/
    /// `finally_ip`, in addition to the normal try-body fallthrough — not
    /// just as a leader-forcing instruction (Issue #10820). Without the
    /// `block_successors` arm, a `catch`/`finally` block would have no
    /// predecessor at all, making every dominance analysis over the CFG
    /// (e.g. the slot-backing verifier) treat it as unconditionally
    /// unreachable from the try region, rather than "reachable, but not
    /// dominated by stores made only inside the protected region".
    #[test]
    fn cfg_models_push_handler_catch_and_fallthrough_edges_issue_10820() {
        let code = vec![
            Instr::PushHandler(Some(4), None), // 0
            Instr::PushI64(1),                 // 1 try body
            Instr::Pop,                        // 2
            Instr::Jump(5),                    // 3 skip catch on normal completion
            Instr::PushI64(2),                 // 4 catch body
            Instr::ReturnI64,                  // 5
        ];

        let cfg = ControlFlowGraph::build(&code);

        // Block 0 = [0, 1) (PushHandler alone, split forced right after it).
        assert_eq!((cfg.blocks[0].start, cfg.blocks[0].end), (0, 1));
        // Successors are [catch block, fallthrough block] — both the
        // exceptional and the normal path out of the handler installation.
        let catch_block = cfg.block_for_instr(4).expect("catch block");
        let body_block = cfg.block_for_instr(1).expect("try-body block");
        assert_eq!(cfg.blocks[0].successors, vec![catch_block, body_block]);
        // The catch block's only predecessor is the `PushHandler` block —
        // NOT the try body, since an exception can fire before any of the
        // try body's own stores run.
        assert_eq!(cfg.blocks[catch_block].predecessors, vec![0]);
    }
}
