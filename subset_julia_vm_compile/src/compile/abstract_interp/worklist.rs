//! SSA/CFG worklist scaffold for abstract interpretation (Issue #3506, Step 1).
//!
//! This module is the *skeleton* of a Julia-style worklist driver. Julia's
//! `abstract_interpret_basic_block` / `typeinf_local` walk a control-flow
//! graph of basic blocks, propagating per-block input states until a
//! fixpoint is reached. This file introduces the analogous data structures
//! in Rust so that subsequent PRs can incrementally migrate the existing
//! tree-walking interpreter onto a CFG without an all-at-once rewrite.
//!
//! ## Scope of this PR (skeleton only)
//!
//! - [`BlockId`] / [`BasicBlock`] / [`Cfg`]: a minimal CFG representation
//!   parameterized by a per-block instruction payload (statements,
//!   SSA values, etc.). The shape mirrors `BlockInfo` in upstream Julia.
//! - [`Worklist`]: a FIFO of pending blocks plus an `in_queue` and a `seen`
//!   set. Mirrors Julia's `W` worklist in `typeinf_local`.
//! - [`BlockStateLattice`]: the merge contract a per-block state must
//!   satisfy. The fixpoint walker composes a transfer function with this
//!   join.
//! - [`run_to_fixpoint`]: the worklist-based fixpoint driver itself —
//!   calls a transfer function on each block, joins outputs into successor
//!   inputs, and re-enqueues anything that changed.
//! - [`WorklistRun`]: result bundle (per-block inputs/outputs and the
//!   `seen` set), so callers and tests can observe what was visited.
//!
//! ## What this PR deliberately does NOT do
//!
//! - It does NOT lower the existing tree-shaped [`crate::ir::core::Block`]
//!   to a [`Cfg`]. That lowering is the next step; this PR only delivers
//!   the destination shape and the fixpoint driver.
//! - It does NOT wire any production inference path through the worklist.
//!   The live engine still goes through
//!   [`crate::compile::abstract_interp::engine::InferenceEngine`].
//! - It does NOT yet implement Phi resolution. `BlockStateLattice::join`
//!   subsumes the Phi-merge for the moment; an explicit phi step belongs
//!   with the SSA conversion in a follow-up.
//!
//! ## Why a skeleton-first PR
//!
//! Issue #3506 is the largest restructure on the inference roadmap.
//! Sibling work items (#3503 lattice, #3504 alias, #3505 cycles, #3508
//! unify) touch the same files. Landing a small, self-contained scaffold
//! up-front lets the worklist data shapes stabilize before later PRs port
//! the live path or change the lattice underneath.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use std::collections::{HashSet, VecDeque};

use crate::compile::abstract_interp::env::TypeEnv;
use crate::compile::lattice::widening::MAX_INFERENCE_ITERATIONS;
use crate::ir::core::{Block, Expr, Stmt};

/// Stable identifier for a basic block within a [`Cfg`].
///
/// Values are dense, zero-based indices into [`Cfg::blocks`]. They are
/// assigned by [`CfgBuilder`] and remain valid for the lifetime of the
/// graph. Mirrors the integer block-number identity used in upstream
/// Julia's `BBNumber`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BlockId(pub usize);

impl BlockId {
    /// Returns the underlying index. Provided for convenience when
    /// indexing parallel side-tables keyed by block id.
    #[inline]
    #[must_use]
    pub fn index(self) -> usize {
        self.0
    }
}

/// Single basic block in a [`Cfg`].
///
/// `instructions` is generic so that the same CFG shape can carry today's
/// statement-index payloads while later PRs migrate to SSA value
/// references. The successor / predecessor edges are stored explicitly so
/// the worklist can propagate without re-deriving control flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock<I> {
    /// This block's identifier within its enclosing [`Cfg`].
    pub id: BlockId,
    /// Opaque instruction payload. For the skeleton callers can use a
    /// `Vec<usize>` of statement indices; later PRs will substitute SSA
    /// values once the IR has been linearized.
    pub instructions: Vec<I>,
    /// Successors in execution order. The last entry is reached on
    /// fall-through; preceding entries correspond to explicit branches.
    pub succ: Vec<BlockId>,
    /// Predecessors as discovered during CFG construction.
    pub pred: Vec<BlockId>,
}

impl<I> BasicBlock<I> {
    /// Convenience constructor used by [`CfgBuilder`].
    fn new(id: BlockId) -> Self {
        Self {
            id,
            instructions: Vec::new(),
            succ: Vec::new(),
            pred: Vec::new(),
        }
    }
}

/// Control-flow graph: an ordered list of basic blocks plus an entry
/// block. Loops are represented as ordinary back-edges in `succ`/`pred`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cfg<I> {
    blocks: Vec<BasicBlock<I>>,
    entry: BlockId,
}

impl<I> Cfg<I> {
    /// Returns the entry block id (the block where execution begins).
    #[inline]
    #[must_use]
    pub fn entry(&self) -> BlockId {
        self.entry
    }

    /// Returns the total number of blocks in the CFG.
    #[inline]
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Iterates over blocks in id order.
    pub fn blocks(&self) -> impl Iterator<Item = &BasicBlock<I>> {
        self.blocks.iter()
    }

    /// Returns the block with the given id, or `None` if `id` is out of
    /// range. Out-of-range ids cannot be produced by [`CfgBuilder`] but
    /// callers may construct ids manually.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&BasicBlock<I>> {
        self.blocks.get(id.0)
    }
}

/// Builder that assigns dense [`BlockId`]s and back-fills predecessor
/// edges so callers do not have to maintain them manually.
#[derive(Debug, Default)]
pub struct CfgBuilder<I> {
    blocks: Vec<BasicBlock<I>>,
}

impl<I> CfgBuilder<I> {
    /// Creates a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    /// Allocates a new block with no instructions and no edges and
    /// returns its id. Callers can subsequently set instructions via
    /// [`Self::set_instructions`] and add edges via [`Self::add_edge`].
    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock::new(id));
        id
    }

    /// Replaces the instruction payload for an existing block. Panics
    /// only if `id` was not produced by this builder, which is a
    /// programmer error.
    pub fn set_instructions(&mut self, id: BlockId, instructions: Vec<I>) {
        self.blocks[id.0].instructions = instructions;
    }

    /// Appends one instruction payload to an existing block.
    pub fn push_instruction(&mut self, id: BlockId, instruction: I) {
        self.blocks[id.0].instructions.push(instruction);
    }

    /// Adds a directed edge `from -> to`, updating both the successor
    /// list of `from` and the predecessor list of `to`. Duplicate edges
    /// are tolerated: at most one entry is recorded in each direction so
    /// the worklist need not deduplicate during propagation.
    pub fn add_edge(&mut self, from: BlockId, to: BlockId) {
        if !self.blocks[from.0].succ.contains(&to) {
            self.blocks[from.0].succ.push(to);
        }
        if !self.blocks[to.0].pred.contains(&from) {
            self.blocks[to.0].pred.push(from);
        }
    }

    /// Finalizes the builder into an immutable [`Cfg`]. Returns `None`
    /// if no blocks were allocated, since a CFG without an entry block
    /// has no meaningful semantics.
    #[must_use]
    pub fn build(self, entry: BlockId) -> Option<Cfg<I>> {
        if self.blocks.is_empty() || entry.0 >= self.blocks.len() {
            return None;
        }
        Some(Cfg {
            blocks: self.blocks,
            entry,
        })
    }
}

/// CFG plus a statement side table produced by [`lower_block_to_cfg`].
///
/// The CFG payload is `usize`: each block instruction indexes
/// `statements`. This keeps nested branch/body statements unambiguous
/// while preserving the statement-index payload shape expected by the
/// worklist migration.
#[derive(Debug, Clone)]
pub struct LoweredCfg<'a> {
    /// Lowered control-flow graph.
    pub cfg: Cfg<usize>,
    /// Statement side table indexed by block instruction payloads.
    pub statements: Vec<&'a Stmt>,
    /// Predicates attached to branch edges.
    pub edge_predicates: Vec<EdgePredicate<'a>>,
}

impl<'a> LoweredCfg<'a> {
    /// Returns the predicate attached to `from -> to`, if the edge is
    /// conditionally reached.
    #[must_use]
    pub fn edge_predicate(&self, from: BlockId, to: BlockId) -> Option<&EdgePredicate<'a>> {
        self.edge_predicates
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
    }
}

/// Direction of a conditionally-reached CFG edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchOutcome {
    /// Edge reached when the condition is true.
    Then,
    /// Edge reached when the condition is false.
    Else,
}

/// Predicate metadata for a branch edge.
#[derive(Debug, Clone, Copy)]
pub struct EdgePredicate<'a> {
    /// Source block.
    pub from: BlockId,
    /// Destination block.
    pub to: BlockId,
    /// Condition controlling the edge.
    pub condition: &'a Expr,
    /// Whether this is the true or false successor.
    pub outcome: BranchOutcome,
}

/// Lowers a small structured subset of [`Block`] into a CFG.
///
/// Supported control flow:
/// - straight-line statements stay in the current block;
/// - `if` creates then/else successor blocks and a join block;
/// - `while` creates a header/body/exit shape with a body-to-header
///   back-edge.
///
/// Other statements remain straight-line payloads. This helper is meant
/// as a migration bridge for the live engine, not as a complete Julia IR
/// CFG lowerer.
///
/// Returns `None` only if `CfgBuilder::build` rejects the entry block; that
/// cannot currently happen here (the entry id was allocated on the builder a
/// couple of lines above and nothing removes blocks in between), but
/// reporting it through the `Option` this function already threads through
/// keeps every caller's existing "fast path did not apply, fall back"
/// handling as the single source of truth instead of a raw unwrap on a
/// same-module invariant (Issue #10905, Phase 1b of #10869).
#[must_use]
pub fn lower_block_to_cfg(block: &Block) -> Option<LoweredCfg<'_>> {
    let mut lowerer = CfgLowerer::new();
    let entry = lowerer.builder.new_block();
    lowerer.lower_statements(block, Some(entry));
    let cfg = lowerer.builder.build(entry)?;
    Some(LoweredCfg {
        cfg,
        statements: lowerer.statements,
        edge_predicates: lowerer.edge_predicates,
    })
}

struct CfgLowerer<'a> {
    builder: CfgBuilder<usize>,
    statements: Vec<&'a Stmt>,
    edge_predicates: Vec<EdgePredicate<'a>>,
}

impl<'a> CfgLowerer<'a> {
    fn new() -> Self {
        Self {
            builder: CfgBuilder::new(),
            statements: Vec::new(),
            edge_predicates: Vec::new(),
        }
    }

    fn lower_statements(
        &mut self,
        block: &'a Block,
        mut current: Option<BlockId>,
    ) -> Option<BlockId> {
        for stmt in &block.stmts {
            let Some(block_id) = current else {
                break;
            };
            current = self.lower_statement(stmt, block_id);
        }
        current
    }

    fn lower_statement(&mut self, stmt: &'a Stmt, current: BlockId) -> Option<BlockId> {
        match stmt {
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if(stmt, condition, then_branch, else_branch.as_ref(), current),
            Stmt::While {
                condition, body, ..
            } => self.lower_while(stmt, condition, body, current),
            Stmt::Return { .. } => {
                self.push_statement(current, stmt);
                None
            }
            _ => {
                self.push_statement(current, stmt);
                Some(current)
            }
        }
    }

    fn lower_if(
        &mut self,
        stmt: &'a Stmt,
        condition: &'a Expr,
        then_branch: &'a Block,
        else_branch: Option<&'a Block>,
        current: BlockId,
    ) -> Option<BlockId> {
        self.push_statement(current, stmt);

        let then_entry = self.builder.new_block();
        let join = self.builder.new_block();
        self.add_predicated_edge(current, then_entry, condition, BranchOutcome::Then);
        if let Some(then_exit) = self.lower_statements(then_branch, Some(then_entry)) {
            self.builder.add_edge(then_exit, join);
        }

        if let Some(else_branch) = else_branch {
            let else_entry = self.builder.new_block();
            self.add_predicated_edge(current, else_entry, condition, BranchOutcome::Else);
            if let Some(else_exit) = self.lower_statements(else_branch, Some(else_entry)) {
                self.builder.add_edge(else_exit, join);
            }
        } else {
            self.add_predicated_edge(current, join, condition, BranchOutcome::Else);
        }

        if self.builder.blocks[join.0].pred.is_empty() {
            None
        } else {
            Some(join)
        }
    }

    fn lower_while(
        &mut self,
        stmt: &'a Stmt,
        condition: &'a Expr,
        body: &'a Block,
        current: BlockId,
    ) -> Option<BlockId> {
        let header = if self
            .builder
            .blocks
            .get(current.0)
            .is_some_and(|block| block.instructions.is_empty())
        {
            current
        } else {
            let header = self.builder.new_block();
            self.builder.add_edge(current, header);
            header
        };

        self.push_statement(header, stmt);

        let body_entry = self.builder.new_block();
        let exit = self.builder.new_block();
        self.add_predicated_edge(header, body_entry, condition, BranchOutcome::Then);
        self.add_predicated_edge(header, exit, condition, BranchOutcome::Else);

        if let Some(body_exit) = self.lower_statements(body, Some(body_entry)) {
            self.builder.add_edge(body_exit, header);
        }

        Some(exit)
    }

    fn add_predicated_edge(
        &mut self,
        from: BlockId,
        to: BlockId,
        condition: &'a Expr,
        outcome: BranchOutcome,
    ) {
        self.builder.add_edge(from, to);
        self.edge_predicates.push(EdgePredicate {
            from,
            to,
            condition,
            outcome,
        });
    }

    fn push_statement(&mut self, block: BlockId, stmt: &'a Stmt) {
        let stmt_id = self.statements.len();
        self.statements.push(stmt);
        self.builder.push_instruction(block, stmt_id);
    }
}

/// Per-block state that participates in the worklist fixpoint.
///
/// `join_in_place` must implement a monotone least-upper-bound: the
/// fixpoint terminates only when every join is monotone (`self` only ever
/// grows up the lattice) and the lattice has finite height (or callers
/// have arranged widening, as the live [`TypeEnv`] does).
pub trait BlockStateLattice: Clone + PartialEq {
    /// Joins `other` into `self`. Returns `true` if `self` changed.
    ///
    /// Implementations must satisfy:
    ///
    /// 1. **Monotonicity**: `self` after join ⊒ `self` before join.
    /// 2. **Idempotence**: joining `self` with itself returns `false`.
    /// 3. **Commutativity** (semantically): the *value* of `self` after
    ///    join must not depend on the order successors are processed.
    fn join_in_place(&mut self, other: &Self) -> bool;
}

/// Reference impl of [`BlockStateLattice`] for [`TypeEnv`]. Delegates to
/// [`TypeEnv::merge_changed`] so the worklist can drive the existing
/// type environment without copying its semantics.
impl BlockStateLattice for TypeEnv {
    fn join_in_place(&mut self, other: &Self) -> bool {
        self.merge_changed(other)
    }
}

/// FIFO worklist of pending blocks.
///
/// Maintains three sets:
///
/// - `queue`: blocks that still need their transfer function executed.
/// - `in_queue`: presence-test mirror so [`Self::enqueue`] is O(1) and
///   does not introduce duplicates (Julia's worklist has the same
///   property: a block is either pending or settled, never both).
/// - `seen`: every block ever popped at least once. Used by tests and by
///   diagnostics to detect unreachable code.
#[derive(Debug, Default, Clone)]
pub struct Worklist {
    queue: VecDeque<BlockId>,
    in_queue: HashSet<BlockId>,
    seen: HashSet<BlockId>,
}

impl Worklist {
    /// Creates an empty worklist.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            in_queue: HashSet::new(),
            seen: HashSet::new(),
        }
    }

    /// Pushes a block onto the worklist. No-op if the block is already
    /// pending — the existing entry will run with the latest joined
    /// input state, so re-queuing would only waste work.
    pub fn enqueue(&mut self, id: BlockId) {
        if self.in_queue.insert(id) {
            self.queue.push_back(id);
        }
    }

    /// Pops the next pending block in FIFO order, recording it in
    /// `seen`. Returns `None` when the worklist is exhausted, which is
    /// the fixpoint termination signal.
    pub fn dequeue(&mut self) -> Option<BlockId> {
        let id = self.queue.pop_front()?;
        self.in_queue.remove(&id);
        self.seen.insert(id);
        Some(id)
    }

    /// Returns true when no further blocks are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// True if `id` has been popped from the worklist at least once.
    #[must_use]
    pub fn was_seen(&self, id: BlockId) -> bool {
        self.seen.contains(&id)
    }

    /// Borrow the seen set (handy for tests and diagnostics).
    #[must_use]
    pub fn seen(&self) -> &HashSet<BlockId> {
        &self.seen
    }
}

/// Result of [`run_to_fixpoint`].
///
/// `block_inputs[i]` is the input state computed for block `i`, or
/// `None` if the block proved unreachable from the entry. `seen` is the
/// set of blocks the worklist actually visited and is therefore the
/// reachable set under the supplied transfer function.
#[derive(Debug, Clone)]
pub struct WorklistRun<S> {
    /// Per-block input state, indexed by [`BlockId::index`].
    pub block_inputs: Vec<Option<S>>,
    /// Per-block output state, indexed by [`BlockId::index`].
    pub block_outputs: Vec<Option<S>>,
    /// The reachable block ids (every block that was ever popped).
    pub seen: HashSet<BlockId>,
    /// Whether the fixpoint was reached within the iteration budget.
    /// `false` indicates the worklist was forcibly terminated and the
    /// returned states are a (sound) over-approximation.
    pub converged: bool,
    /// Number of transfer-function invocations performed. Useful for
    /// tests asserting termination and for performance regressions.
    pub steps: usize,
}

/// Drives a CFG to a fixpoint using a Julia-style block worklist.
///
/// The driver:
///
/// 1. Seeds block_inputs[entry] with `entry_state` and enqueues `entry`.
/// 2. Pops blocks FIFO. For each popped block it clones the recorded
///    input, calls `transfer(block_id, input)` to compute the per-block
///    output, and joins that output into every successor's input.
/// 3. Re-enqueues a successor whose input changed.
/// 4. Terminates when the worklist is empty (fixpoint) or after
///    `MAX_INFERENCE_ITERATIONS * block_count` transfer invocations
///    (defensive safety cap; should never trigger when the supplied
///    lattice has finite height).
///
/// `transfer` receives an `&S` and returns the per-successor output
/// state. For blocks that diverge or have only side-effects on the
/// abstract state, the same value can be returned. Callers who need
/// per-edge state (e.g., to model a branch-predicated `Conditional`
/// type) can wrap multiple successor states in `S` itself.
pub fn run_to_fixpoint<I, S, F>(cfg: &Cfg<I>, entry_state: S, mut transfer: F) -> WorklistRun<S>
where
    S: BlockStateLattice,
    F: FnMut(BlockId, &S) -> S,
{
    run_to_fixpoint_with_edges(
        cfg,
        entry_state,
        |id, input| transfer(id, input),
        |_, _, output| output.clone(),
    )
}

/// Drives a CFG to a fixpoint with per-edge transfer.
///
/// This variant is used by the production observation pass while #5602 moves
/// branch narrowing onto CFG successor edges. `transfer` computes the block
/// output state; `edge_transfer` can refine that output separately for each
/// successor before it is joined into the successor input.
pub fn run_to_fixpoint_with_edges<I, S, F, E>(
    cfg: &Cfg<I>,
    entry_state: S,
    mut transfer: F,
    mut edge_transfer: E,
) -> WorklistRun<S>
where
    S: BlockStateLattice,
    F: FnMut(BlockId, &S) -> S,
    E: FnMut(BlockId, BlockId, &S) -> S,
{
    let n = cfg.block_count();
    let mut block_inputs: Vec<Option<S>> = vec![None; n];
    let mut block_outputs: Vec<Option<S>> = vec![None; n];
    let mut worklist = Worklist::new();

    if n == 0 {
        return WorklistRun {
            block_inputs,
            block_outputs,
            seen: HashSet::new(),
            converged: true,
            steps: 0,
        };
    }

    block_inputs[cfg.entry.0] = Some(entry_state);
    worklist.enqueue(cfg.entry);

    // Defensive iteration budget. With a sound widening operator the
    // fixpoint is reached in at most O(blocks * lattice_height) steps;
    // we use the lattice-height cap from `MAX_INFERENCE_ITERATIONS` as
    // an upper bound on per-block visits.
    let max_steps = n.saturating_mul(MAX_INFERENCE_ITERATIONS).max(1);
    let mut steps = 0usize;
    let mut converged = true;

    while let Some(bb) = worklist.dequeue() {
        if steps >= max_steps {
            // Re-enqueue so the caller sees the worklist as non-empty
            // and `converged == false`. Mirrors Julia's behaviour of
            // bailing out with a sound over-approximation.
            crate::compile::infer_metrics::record_worklist_step_limit_hit();
            converged = false;
            worklist.enqueue(bb);
            break;
        }
        steps += 1;

        let input = match &block_inputs[bb.0] {
            Some(s) => s.clone(),
            // No input state means the block was enqueued but its input
            // was wiped — this should not happen with the routines in
            // this module, but be defensive.
            None => continue,
        };

        let output = transfer(bb, &input);
        block_outputs[bb.0] = Some(output.clone());

        let block = match cfg.block(bb) {
            Some(b) => b,
            None => continue,
        };

        for &succ in &block.succ {
            let succ_output = edge_transfer(bb, succ, &output);
            let changed = match &mut block_inputs[succ.0] {
                Some(existing) => existing.join_in_place(&succ_output),
                slot @ None => {
                    *slot = Some(succ_output);
                    true
                }
            };
            if changed {
                worklist.enqueue(succ);
            }
        }
    }

    WorklistRun {
        block_inputs,
        block_outputs,
        seen: worklist.seen,
        converged,
        steps,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    //! Unit tests for the worklist scaffold.
    //!
    //! These exercise the data structures and the fixpoint driver in
    //! isolation. They use a deliberately simple state lattice
    //! (`MaxState`, the "max so far" of an integer) so the assertions
    //! focus on visit ordering, termination, and seen-set coverage —
    //! the contracts that this PR is responsible for. The real lattice
    //! ([`TypeEnv`]) gets exercised in a single end-to-end test below.

    use super::*;
    use crate::compile::lattice::types::{ConcreteType, LatticeType};
    use crate::ir::core::{Block, Expr, Literal, Stmt};
    use crate::span::Span;

    /// Minimal lattice used for fixpoint tests: an integer with `max`
    /// as its join. Trivially monotone with finite height (bounded by
    /// the integer values that any test produces).
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MaxState(i32);

    impl BlockStateLattice for MaxState {
        fn join_in_place(&mut self, other: &Self) -> bool {
            if other.0 > self.0 {
                self.0 = other.0;
                true
            } else {
                false
            }
        }
    }

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    /// Builds a simple straight-line CFG: A -> B -> C.
    fn straight_line_cfg() -> (Cfg<()>, [BlockId; 3]) {
        let mut b = CfgBuilder::<()>::new();
        let a = b.new_block();
        let bb = b.new_block();
        let c = b.new_block();
        b.add_edge(a, bb);
        b.add_edge(bb, c);
        (b.build(a).unwrap(), [a, bb, c])
    }

    /// Builds an if/else CFG:
    ///     A -> {B, C} -> D
    /// where B and C are the two branches and D is the join point.
    fn if_else_cfg() -> (Cfg<()>, [BlockId; 4]) {
        let mut b = CfgBuilder::<()>::new();
        let a = b.new_block();
        let then_bb = b.new_block();
        let else_bb = b.new_block();
        let join_bb = b.new_block();
        b.add_edge(a, then_bb);
        b.add_edge(a, else_bb);
        b.add_edge(then_bb, join_bb);
        b.add_edge(else_bb, join_bb);
        (b.build(a).unwrap(), [a, then_bb, else_bb, join_bb])
    }

    /// Builds a while-loop CFG:
    ///     pre -> header -> body -> header
    ///                   \-> exit
    fn loop_cfg() -> (Cfg<()>, [BlockId; 4]) {
        let mut b = CfgBuilder::<()>::new();
        let pre = b.new_block();
        let header = b.new_block();
        let body = b.new_block();
        let exit = b.new_block();
        b.add_edge(pre, header);
        b.add_edge(header, body);
        b.add_edge(header, exit);
        b.add_edge(body, header); // back-edge
        (b.build(pre).unwrap(), [pre, header, body, exit])
    }

    #[test]
    fn worklist_enqueue_deduplicates() {
        let mut w = Worklist::new();
        w.enqueue(BlockId(0));
        w.enqueue(BlockId(0));
        w.enqueue(BlockId(1));
        w.enqueue(BlockId(0));
        // First pop -> 0, second pop -> 1, then empty (the dup was
        // suppressed by `in_queue`).
        assert_eq!(w.dequeue(), Some(BlockId(0)));
        assert_eq!(w.dequeue(), Some(BlockId(1)));
        assert_eq!(w.dequeue(), None);
        assert!(w.was_seen(BlockId(0)));
        assert!(w.was_seen(BlockId(1)));
    }

    #[test]
    fn straight_line_visits_every_block_once() {
        let (cfg, [a, bb, c]) = straight_line_cfg();
        // Transfer is identity-on-state. Counter is captured by closure
        // to assert each block is processed exactly once for this
        // monotone-stable input.
        let mut visits = vec![0usize; cfg.block_count()];
        let result = run_to_fixpoint(&cfg, MaxState(7), |id, st| {
            visits[id.0] += 1;
            st.clone()
        });
        assert!(result.converged);
        assert_eq!(visits, vec![1, 1, 1]);
        assert_eq!(result.seen.len(), 3);
        for id in [a, bb, c] {
            assert!(result.seen.contains(&id));
            assert_eq!(result.block_inputs[id.0], Some(MaxState(7)));
        }
    }

    #[test]
    fn if_else_seen_set_covers_all_reachable_blocks() {
        let (cfg, [a, then_bb, else_bb, join_bb]) = if_else_cfg();
        let result = run_to_fixpoint(&cfg, MaxState(0), |_, s| s.clone());
        assert!(result.converged);
        for id in [a, then_bb, else_bb, join_bb] {
            assert!(result.seen.contains(&id), "missing block {id:?}");
            assert!(result.block_inputs[id.0].is_some());
        }
    }

    #[test]
    fn if_else_join_takes_lub_of_branch_outputs() {
        // A produces 1, then-branch publishes 5, else-branch publishes
        // 3. The join block must see max(5, 3) == 5 because the
        // lattice's `join_in_place` is `max`.
        let (cfg, [a, then_bb, else_bb, join_bb]) = if_else_cfg();
        let result = run_to_fixpoint(&cfg, MaxState(1), |id, _| {
            if id == a {
                MaxState(1)
            } else if id == then_bb {
                MaxState(5)
            } else if id == else_bb {
                MaxState(3)
            } else {
                MaxState(0)
            }
        });
        assert!(result.converged);
        assert_eq!(result.block_inputs[join_bb.0], Some(MaxState(5)));
    }

    #[test]
    fn loop_terminates_at_fixpoint() {
        // Body increments the loop variable up to 3, then stops growing;
        // the worklist must terminate once `header`'s input stabilises.
        let (cfg, [pre, header, body, exit]) = loop_cfg();
        let mut body_visits = 0usize;
        let result = run_to_fixpoint(&cfg, MaxState(0), |id, s| {
            if id == body {
                body_visits += 1;
                // Saturate at 3 so the lattice has finite height.
                MaxState((s.0 + 1).min(3))
            } else {
                s.clone()
            }
        });
        assert!(
            result.converged,
            "loop must reach a fixpoint within the budget"
        );
        // Reachable set: pre, header, body, exit.
        assert_eq!(result.seen.len(), 4);
        for id in [pre, header, body, exit] {
            assert!(result.seen.contains(&id));
        }
        // The header (and exit) must observe the saturated value.
        assert_eq!(result.block_inputs[header.0], Some(MaxState(3)));
        assert_eq!(result.block_inputs[exit.0], Some(MaxState(3)));
        // Body should have been re-visited (more than once) because of
        // the back-edge, but a small bounded number of times.
        assert!(body_visits >= 2);
        assert!(body_visits <= MAX_INFERENCE_ITERATIONS);
    }

    #[test]
    fn unreachable_blocks_are_not_visited() {
        // Block D is allocated but never linked from the entry. The
        // worklist must skip it, leaving its input as `None` and the
        // seen set ignoring it entirely.
        let mut b = CfgBuilder::<()>::new();
        let a = b.new_block();
        let bb = b.new_block();
        let unreached = b.new_block();
        b.add_edge(a, bb);
        let cfg = b.build(a).unwrap();

        let result = run_to_fixpoint(&cfg, MaxState(1), |_, s| s.clone());
        assert!(result.converged);
        assert!(result.seen.contains(&a));
        assert!(result.seen.contains(&bb));
        assert!(!result.seen.contains(&unreached));
        assert!(result.block_inputs[unreached.0].is_none());
    }

    #[test]
    fn empty_cfg_returns_immediately() {
        // CfgBuilder::build rejects an empty graph, so we exercise the
        // defensive `n == 0` branch by constructing a Cfg by hand. The
        // public surface intentionally cannot reach this state.
        let cfg: Cfg<()> = Cfg {
            blocks: Vec::new(),
            entry: BlockId(0),
        };
        let result = run_to_fixpoint(&cfg, MaxState(0), |_, _| MaxState(0));
        assert!(result.converged);
        assert_eq!(result.steps, 0);
        assert!(result.seen.is_empty());
    }

    #[test]
    fn type_env_works_as_block_state() {
        // End-to-end smoke test that the live [`TypeEnv`] satisfies
        // the [`BlockStateLattice`] contract: an if/else where each
        // branch binds `x` to a different concrete type joins to a
        // Union at the merge point.
        let (cfg, [_, then_bb, else_bb, join_bb]) = if_else_cfg();

        let mut entry = TypeEnv::new();
        entry.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let result = run_to_fixpoint(&cfg, entry, |id, env| {
            let mut out = env.clone();
            if id == then_bb {
                out.set(
                    "x",
                    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64,
                    ))),
                );
            } else if id == else_bb {
                out.set(
                    "x",
                    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Float64,
                    ))),
                );
            }
            out
        });
        assert!(result.converged);

        let join_env = result.block_inputs[join_bb.0]
            .as_ref()
            .expect("join block must be reached");
        // The merge of Int64 and Float64 must have widened to a Union
        // (or Top), but in any case it must NOT remain a single
        // ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)) — that would mean the worklist failed
        // to propagate the else-branch's contribution.
        let x_ty = join_env.get("x").expect("x must be bound at the join");
        assert_ne!(
            x_ty,
            &LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_ne!(
            x_ty,
            &LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn return_terminates_cfg_lowering_5602() {
        let block = Block {
            stmts: vec![
                Stmt::Assign {
                    var: "x".to_string(),
                    value: Expr::Literal(Literal::Int(1), dummy_span()),
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                    span: dummy_span(),
                },
                Stmt::Assign {
                    var: "unreachable".to_string(),
                    value: Expr::Literal(Literal::Float(2.5), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };

        let lowered = lower_block_to_cfg(&block).unwrap();
        assert_eq!(lowered.statements.len(), 2);
        assert!(!lowered
            .statements
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Assign { var, .. } if var == "unreachable")));

        let entry = lowered.cfg.block(lowered.cfg.entry()).unwrap();
        assert_eq!(entry.instructions.len(), 2);
        assert!(entry.succ.is_empty());
    }

    #[test]
    fn returning_if_branch_does_not_feed_join_5602() {
        let block = Block {
            stmts: vec![
                Stmt::If {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    then_branch: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![Stmt::Assign {
                            var: "x".to_string(),
                            value: Expr::Literal(Literal::Int(2), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                Stmt::Assign {
                    var: "tail".to_string(),
                    value: Expr::Literal(Literal::Int(3), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };

        let lowered = lower_block_to_cfg(&block).unwrap();
        let return_block = lowered
            .cfg
            .blocks()
            .find(|block| {
                block
                    .instructions
                    .iter()
                    .any(|stmt_id| matches!(lowered.statements[*stmt_id], Stmt::Return { .. }))
            })
            .expect("then return should lower into its own block");
        assert!(
            return_block.succ.is_empty(),
            "returning branch must terminate instead of feeding the join"
        );

        let tail_block = lowered
            .cfg
            .blocks()
            .find(|block| {
                block.instructions.iter().any(|stmt_id| {
                    matches!(
                        lowered.statements[*stmt_id],
                        Stmt::Assign { ref var, .. } if var == "tail"
                    )
                })
            })
            .expect("else fallthrough should keep the tail assignment reachable");
        assert!(!tail_block.pred.contains(&return_block.id));
    }

    #[test]
    fn all_returning_if_terminates_tail_lowering_5602() {
        let block = Block {
            stmts: vec![
                Stmt::If {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    then_branch: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Literal(Literal::Int(2), dummy_span())),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                Stmt::Assign {
                    var: "unreachable_tail".to_string(),
                    value: Expr::Literal(Literal::Int(3), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };

        let lowered = lower_block_to_cfg(&block).unwrap();
        assert!(!lowered
            .statements
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Assign { var, .. } if var == "unreachable_tail")));
        let return_blocks = lowered
            .cfg
            .blocks()
            .filter(|block| {
                block
                    .instructions
                    .iter()
                    .any(|stmt_id| matches!(lowered.statements[*stmt_id], Stmt::Return { .. }))
            })
            .count();
        assert_eq!(return_blocks, 2);
    }

    #[test]
    fn lowers_if_and_while_to_cfg_and_preserves_block_outputs() {
        let block = Block {
            stmts: vec![
                Stmt::Assign {
                    var: "x".to_string(),
                    value: Expr::Literal(Literal::Int(1), dummy_span()),
                    span: dummy_span(),
                },
                Stmt::If {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    then_branch: Block {
                        stmts: vec![Stmt::Assign {
                            var: "y".to_string(),
                            value: Expr::Literal(Literal::Int(2), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![Stmt::Assign {
                            var: "y".to_string(),
                            value: Expr::Literal(Literal::Float(2.5), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                Stmt::While {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    body: Block {
                        stmts: vec![Stmt::Assign {
                            var: "z".to_string(),
                            value: Expr::Literal(Literal::Bool(true), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };

        let lowered = lower_block_to_cfg(&block).unwrap();
        assert_eq!(lowered.statements.len(), 6);

        let cfg = &lowered.cfg;
        let mut entry = TypeEnv::new();
        entry.set(
            "flag",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        let result = run_to_fixpoint(cfg, entry, |id, env| {
            let mut out = env.clone();
            for stmt_id in &cfg.block(id).unwrap().instructions {
                match lowered.statements[*stmt_id] {
                    Stmt::Assign {
                        ref var, ref value, ..
                    } => {
                        let ty = match value {
                            Expr::Literal(Literal::Int(_), _) => LatticeType::Concrete(
                                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                            ),
                            Expr::Literal(Literal::Float(_), _) => LatticeType::Concrete(
                                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                            ),
                            Expr::Literal(Literal::Bool(_), _) => LatticeType::Concrete(
                                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
                            ),
                            _ => LatticeType::Top,
                        };
                        out.set(var, ty);
                    }
                    Stmt::If { .. } | Stmt::While { .. } => {}
                    _ => {}
                }
            }
            out
        });

        assert!(result.converged);

        let join_block = cfg
            .blocks()
            .find(|block| {
                block
                    .pred
                    .iter()
                    .filter(|pred| {
                        let pred_block = cfg.block(**pred).unwrap();
                        pred_block
                            .instructions
                            .iter()
                            .any(|stmt_id| matches!(lowered.statements[*stmt_id], Stmt::Assign { ref var, .. } if var == "y"))
                    })
                    .count()
                    == 2
            })
            .expect("if join block should have both branch predecessors");
        let join_input = result.block_inputs[join_block.id.0]
            .as_ref()
            .expect("if join block should be reachable");
        let joined_y = join_input.get("y").expect("branch assignment should join");
        assert_ne!(
            joined_y,
            &LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_ne!(
            joined_y,
            &LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        let while_header = cfg
            .blocks()
            .find(|block| {
                block
                    .instructions
                    .iter()
                    .any(|stmt_id| matches!(lowered.statements[*stmt_id], Stmt::While { .. }))
            })
            .expect("while header should carry the while statement id");
        let while_body = while_header
            .succ
            .iter()
            .copied()
            .find(|succ| cfg.block(*succ).unwrap().succ.contains(&while_header.id))
            .expect("while body should back-edge to header");
        let body_output = result.block_outputs[while_body.0]
            .as_ref()
            .expect("while body output should be recorded");
        assert_eq!(
            body_output.get("z"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
    }
}
