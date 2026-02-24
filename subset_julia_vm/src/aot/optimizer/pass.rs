//! Basic optimization pass definitions
//!
//! This module contains the basic pass struct definitions that implement
//! the OptimizationPass trait for IrFunction optimization.
//!
//! These passes operate on the lower-level SSA IR (IrFunction, BasicBlock, Instruction).

use super::OptimizationPass;
use crate::aot::ir::{
    BasicBlock, BinOpKind, ConstValue, Instruction, IrFunction, Terminator, UnaryOpKind, VarRef,
};
use crate::aot::AotResult;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Constant Folding
// ============================================================================

/// Constant folding optimization
///
/// Evaluates operations on constant values at compile time, replacing
/// expressions like `2 + 3` with `5`.
#[derive(Debug, Default)]
pub struct ConstantFolding {
    /// Track constant values for variables
    constants: HashMap<String, ConstValue>,
}

impl ConstantFolding {
    /// Create a new constant folding pass
    pub fn new() -> Self {
        Self {
            constants: HashMap::new(),
        }
    }

    /// Try to fold a binary operation on two constant values
    fn fold_binop(
        &self,
        op: BinOpKind,
        left: &ConstValue,
        right: &ConstValue,
    ) -> Option<ConstValue> {
        match (left, right) {
            // Integer operations
            (ConstValue::Int64(a), ConstValue::Int64(b)) => self.fold_i64_binop(op, *a, *b),
            (ConstValue::Int32(a), ConstValue::Int32(b)) => self.fold_i32_binop(op, *a, *b),

            // Float operations
            (ConstValue::Float64(a), ConstValue::Float64(b)) => self.fold_f64_binop(op, *a, *b),
            (ConstValue::Float32(a), ConstValue::Float32(b)) => self.fold_f32_binop(op, *a, *b),

            // Boolean operations
            (ConstValue::Bool(a), ConstValue::Bool(b)) => self.fold_bool_binop(op, *a, *b),

            // Mixed int/float (promote to float)
            (ConstValue::Int64(a), ConstValue::Float64(b)) => {
                self.fold_f64_binop(op, *a as f64, *b)
            }
            (ConstValue::Float64(a), ConstValue::Int64(b)) => {
                self.fold_f64_binop(op, *a, *b as f64)
            }

            _ => None,
        }
    }

    fn fold_i64_binop(&self, op: BinOpKind, a: i64, b: i64) -> Option<ConstValue> {
        match op {
            BinOpKind::Add => Some(ConstValue::Int64(a.wrapping_add(b))),
            BinOpKind::Sub => Some(ConstValue::Int64(a.wrapping_sub(b))),
            BinOpKind::Mul => Some(ConstValue::Int64(a.wrapping_mul(b))),
            BinOpKind::Div => {
                if b != 0 {
                    Some(ConstValue::Float64(a as f64 / b as f64))
                } else {
                    None
                }
            }
            BinOpKind::Rem => {
                if b != 0 {
                    Some(ConstValue::Int64(a % b))
                } else {
                    None
                }
            }
            BinOpKind::Pow => {
                if b >= 0 && b <= 63 {
                    Some(ConstValue::Int64(a.wrapping_pow(b as u32)))
                } else {
                    Some(ConstValue::Float64((a as f64).powf(b as f64)))
                }
            }
            BinOpKind::Lt => Some(ConstValue::Bool(a < b)),
            BinOpKind::Le => Some(ConstValue::Bool(a <= b)),
            BinOpKind::Gt => Some(ConstValue::Bool(a > b)),
            BinOpKind::Ge => Some(ConstValue::Bool(a >= b)),
            BinOpKind::Eq => Some(ConstValue::Bool(a == b)),
            BinOpKind::Ne => Some(ConstValue::Bool(a != b)),
            BinOpKind::BitAnd => Some(ConstValue::Int64(a & b)),
            BinOpKind::BitOr => Some(ConstValue::Int64(a | b)),
            BinOpKind::BitXor => Some(ConstValue::Int64(a ^ b)),
            BinOpKind::Shl => Some(ConstValue::Int64(a << (b as u32 & 63))),
            BinOpKind::Shr => Some(ConstValue::Int64(a >> (b as u32 & 63))),
            BinOpKind::And | BinOpKind::Or => None,
        }
    }

    fn fold_i32_binop(&self, op: BinOpKind, a: i32, b: i32) -> Option<ConstValue> {
        match op {
            BinOpKind::Add => Some(ConstValue::Int32(a.wrapping_add(b))),
            BinOpKind::Sub => Some(ConstValue::Int32(a.wrapping_sub(b))),
            BinOpKind::Mul => Some(ConstValue::Int32(a.wrapping_mul(b))),
            BinOpKind::Div => {
                if b != 0 {
                    Some(ConstValue::Float64(a as f64 / b as f64))
                } else {
                    None
                }
            }
            BinOpKind::Rem => {
                if b != 0 {
                    Some(ConstValue::Int32(a % b))
                } else {
                    None
                }
            }
            BinOpKind::Lt => Some(ConstValue::Bool(a < b)),
            BinOpKind::Le => Some(ConstValue::Bool(a <= b)),
            BinOpKind::Gt => Some(ConstValue::Bool(a > b)),
            BinOpKind::Ge => Some(ConstValue::Bool(a >= b)),
            BinOpKind::Eq => Some(ConstValue::Bool(a == b)),
            BinOpKind::Ne => Some(ConstValue::Bool(a != b)),
            _ => None,
        }
    }

    fn fold_f64_binop(&self, op: BinOpKind, a: f64, b: f64) -> Option<ConstValue> {
        match op {
            BinOpKind::Add => Some(ConstValue::Float64(a + b)),
            BinOpKind::Sub => Some(ConstValue::Float64(a - b)),
            BinOpKind::Mul => Some(ConstValue::Float64(a * b)),
            BinOpKind::Div => Some(ConstValue::Float64(a / b)),
            BinOpKind::Pow => Some(ConstValue::Float64(a.powf(b))),
            BinOpKind::Lt => Some(ConstValue::Bool(a < b)),
            BinOpKind::Le => Some(ConstValue::Bool(a <= b)),
            BinOpKind::Gt => Some(ConstValue::Bool(a > b)),
            BinOpKind::Ge => Some(ConstValue::Bool(a >= b)),
            BinOpKind::Eq => Some(ConstValue::Bool(a == b)),
            BinOpKind::Ne => Some(ConstValue::Bool(a != b)),
            _ => None,
        }
    }

    fn fold_f32_binop(&self, op: BinOpKind, a: f32, b: f32) -> Option<ConstValue> {
        match op {
            BinOpKind::Add => Some(ConstValue::Float32(a + b)),
            BinOpKind::Sub => Some(ConstValue::Float32(a - b)),
            BinOpKind::Mul => Some(ConstValue::Float32(a * b)),
            BinOpKind::Div => Some(ConstValue::Float32(a / b)),
            BinOpKind::Pow => Some(ConstValue::Float32(a.powf(b))),
            BinOpKind::Lt => Some(ConstValue::Bool(a < b)),
            BinOpKind::Le => Some(ConstValue::Bool(a <= b)),
            BinOpKind::Gt => Some(ConstValue::Bool(a > b)),
            BinOpKind::Ge => Some(ConstValue::Bool(a >= b)),
            BinOpKind::Eq => Some(ConstValue::Bool(a == b)),
            BinOpKind::Ne => Some(ConstValue::Bool(a != b)),
            _ => None,
        }
    }

    fn fold_bool_binop(&self, op: BinOpKind, a: bool, b: bool) -> Option<ConstValue> {
        match op {
            BinOpKind::And => Some(ConstValue::Bool(a && b)),
            BinOpKind::Or => Some(ConstValue::Bool(a || b)),
            BinOpKind::Eq => Some(ConstValue::Bool(a == b)),
            BinOpKind::Ne => Some(ConstValue::Bool(a != b)),
            _ => None,
        }
    }

    /// Try to fold a unary operation on a constant value
    fn fold_unaryop(&self, op: UnaryOpKind, operand: &ConstValue) -> Option<ConstValue> {
        match (op, operand) {
            (UnaryOpKind::Neg, ConstValue::Int64(v)) => Some(ConstValue::Int64(-*v)),
            (UnaryOpKind::Neg, ConstValue::Int32(v)) => Some(ConstValue::Int32(-*v)),
            (UnaryOpKind::Neg, ConstValue::Float64(v)) => Some(ConstValue::Float64(-*v)),
            (UnaryOpKind::Neg, ConstValue::Float32(v)) => Some(ConstValue::Float32(-*v)),
            (UnaryOpKind::Not, ConstValue::Bool(v)) => Some(ConstValue::Bool(!*v)),
            (UnaryOpKind::BitNot, ConstValue::Int64(v)) => Some(ConstValue::Int64(!*v)),
            (UnaryOpKind::BitNot, ConstValue::Int32(v)) => Some(ConstValue::Int32(!*v)),
            _ => None,
        }
    }

    /// Get the constant value for a variable reference if known
    fn get_constant(&self, var: &VarRef) -> Option<&ConstValue> {
        let key = format!("{}.{}", var.name, var.version);
        self.constants.get(&key)
    }

    /// Set the constant value for a variable
    fn set_constant(&mut self, var: &VarRef, value: ConstValue) {
        let key = format!("{}.{}", var.name, var.version);
        self.constants.insert(key, value);
    }

    /// Optimize a single basic block
    fn optimize_block(&mut self, block: &mut BasicBlock) -> bool {
        let mut changed = false;
        let mut new_instructions = Vec::with_capacity(block.instructions.len());

        for inst in block.instructions.drain(..) {
            match inst {
                Instruction::LoadConst {
                    ref dest,
                    ref value,
                } => {
                    // Track constant value
                    self.set_constant(dest, value.clone());
                    new_instructions.push(inst);
                }
                Instruction::BinOp {
                    ref dest,
                    op,
                    ref left,
                    ref right,
                } => {
                    // Try to fold if both operands are constants
                    let left_const = self.get_constant(left).cloned();
                    let right_const = self.get_constant(right).cloned();

                    if let (Some(lc), Some(rc)) = (left_const, right_const) {
                        if let Some(result) = self.fold_binop(op, &lc, &rc) {
                            // Replace with LoadConst
                            self.set_constant(dest, result.clone());
                            new_instructions.push(Instruction::LoadConst {
                                dest: dest.clone(),
                                value: result,
                            });
                            changed = true;
                            continue;
                        }
                    }
                    new_instructions.push(inst);
                }
                Instruction::UnaryOp {
                    ref dest,
                    op,
                    ref operand,
                } => {
                    // Try to fold if operand is constant
                    if let Some(operand_const) = self.get_constant(operand).cloned() {
                        if let Some(result) = self.fold_unaryop(op, &operand_const) {
                            // Replace with LoadConst
                            self.set_constant(dest, result.clone());
                            new_instructions.push(Instruction::LoadConst {
                                dest: dest.clone(),
                                value: result,
                            });
                            changed = true;
                            continue;
                        }
                    }
                    new_instructions.push(inst);
                }
                Instruction::Copy { ref dest, ref src } => {
                    // Propagate constant through copy
                    if let Some(src_const) = self.get_constant(src).cloned() {
                        self.set_constant(dest, src_const);
                    }
                    new_instructions.push(inst);
                }
                _ => {
                    new_instructions.push(inst);
                }
            }
        }

        block.instructions = new_instructions;
        changed
    }
}

impl OptimizationPass for ConstantFolding {
    fn name(&self) -> &str {
        "constant_folding"
    }

    fn optimize_function(&self, func: &mut IrFunction) -> AotResult<bool> {
        let mut folder = ConstantFolding::new();
        let mut changed = false;

        for block in &mut func.blocks {
            if folder.optimize_block(block) {
                changed = true;
            }
        }

        Ok(changed)
    }
}

// ============================================================================
// Dead Code Elimination
// ============================================================================

/// Dead code elimination
///
/// Removes unreachable code and unused variable definitions.
/// Works by:
/// 1. Removing instructions after terminators (return, unconditional jump)
/// 2. Removing unused variable definitions (definitions whose values are never used)
/// 3. Removing unreachable basic blocks
#[derive(Debug, Default)]
pub struct DeadCodeElimination;

impl DeadCodeElimination {
    /// Create a new DCE pass
    pub fn new() -> Self {
        Self
    }

    /// Collect all variable uses in a function
    fn collect_uses(func: &IrFunction) -> HashSet<String> {
        let mut uses = HashSet::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    Instruction::Copy { src, .. } => {
                        uses.insert(format!("{}.{}", src.name, src.version));
                    }
                    Instruction::BinOp { left, right, .. } => {
                        uses.insert(format!("{}.{}", left.name, left.version));
                        uses.insert(format!("{}.{}", right.name, right.version));
                    }
                    Instruction::UnaryOp { operand, .. } => {
                        uses.insert(format!("{}.{}", operand.name, operand.version));
                    }
                    Instruction::Call { args, .. } => {
                        for arg in args {
                            uses.insert(format!("{}.{}", arg.name, arg.version));
                        }
                    }
                    Instruction::GetIndex { array, index, .. } => {
                        uses.insert(format!("{}.{}", array.name, array.version));
                        uses.insert(format!("{}.{}", index.name, index.version));
                    }
                    Instruction::SetIndex {
                        array,
                        index,
                        value,
                    } => {
                        uses.insert(format!("{}.{}", array.name, array.version));
                        uses.insert(format!("{}.{}", index.name, index.version));
                        uses.insert(format!("{}.{}", value.name, value.version));
                    }
                    Instruction::GetField { object, .. } => {
                        uses.insert(format!("{}.{}", object.name, object.version));
                    }
                    Instruction::SetField { object, value, .. } => {
                        uses.insert(format!("{}.{}", object.name, object.version));
                        uses.insert(format!("{}.{}", value.name, value.version));
                    }
                    Instruction::TypeAssert { src, .. } => {
                        uses.insert(format!("{}.{}", src.name, src.version));
                    }
                    Instruction::Phi { incoming, .. } => {
                        for (_, var) in incoming {
                            uses.insert(format!("{}.{}", var.name, var.version));
                        }
                    }
                    Instruction::LoadConst { .. } => {}
                }
            }

            // Check uses in terminator
            if let Some(term) = &block.terminator {
                match term {
                    Terminator::Return(Some(var)) => {
                        uses.insert(format!("{}.{}", var.name, var.version));
                    }
                    Terminator::Branch { cond, .. } => {
                        uses.insert(format!("{}.{}", cond.name, cond.version));
                    }
                    Terminator::Switch { value, .. } => {
                        uses.insert(format!("{}.{}", value.name, value.version));
                    }
                    _ => {}
                }
            }
        }

        uses
    }

    /// Remove unused definitions from a block
    fn remove_unused_defs(block: &mut BasicBlock, uses: &HashSet<String>) -> bool {
        let original_len = block.instructions.len();

        block.instructions.retain(|inst| {
            // Get the destination variable if any
            let dest = match inst {
                Instruction::LoadConst { dest, .. } => Some(dest),
                Instruction::Copy { dest, .. } => Some(dest),
                Instruction::BinOp { dest, .. } => Some(dest),
                Instruction::UnaryOp { dest, .. } => Some(dest),
                Instruction::GetIndex { dest, .. } => Some(dest),
                Instruction::GetField { dest, .. } => Some(dest),
                Instruction::TypeAssert { dest, .. } => Some(dest),
                Instruction::Phi { dest, .. } => Some(dest),
                // These may have side effects or modify state
                Instruction::Call { dest: _, .. } => {
                    // Keep calls even if result is unused (may have side effects)
                    return true;
                }
                Instruction::SetIndex { .. } | Instruction::SetField { .. } => {
                    // Always keep mutations
                    return true;
                }
            };

            if let Some(d) = dest {
                let key = format!("{}.{}", d.name, d.version);
                // Keep if the definition is used
                uses.contains(&key)
            } else {
                true
            }
        });

        block.instructions.len() != original_len
    }

    /// Find reachable blocks starting from entry
    fn find_reachable_blocks(func: &IrFunction) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut worklist = vec![func.entry.clone()];

        while let Some(label) = worklist.pop() {
            if reachable.contains(&label) {
                continue;
            }
            reachable.insert(label.clone());

            // Find the block and add successors
            if let Some(block) = func.blocks.iter().find(|b| b.label == label) {
                if let Some(term) = &block.terminator {
                    match term {
                        Terminator::Jump(target) => {
                            worklist.push(target.clone());
                        }
                        Terminator::Branch {
                            then_block,
                            else_block,
                            ..
                        } => {
                            worklist.push(then_block.clone());
                            worklist.push(else_block.clone());
                        }
                        Terminator::Switch { cases, default, .. } => {
                            for (_, target) in cases {
                                worklist.push(target.clone());
                            }
                            worklist.push(default.clone());
                        }
                        Terminator::Return(_) => {}
                    }
                }
            }
        }

        reachable
    }
}

impl OptimizationPass for DeadCodeElimination {
    fn name(&self) -> &str {
        "dead_code_elimination"
    }

    fn optimize_function(&self, func: &mut IrFunction) -> AotResult<bool> {
        let mut changed = false;

        // 1. Remove unreachable blocks
        let reachable = Self::find_reachable_blocks(func);
        let original_block_count = func.blocks.len();
        func.blocks.retain(|b| reachable.contains(&b.label));
        if func.blocks.len() != original_block_count {
            changed = true;
        }

        // 2. Collect all uses
        let uses = Self::collect_uses(func);

        // 3. Remove unused definitions
        for block in &mut func.blocks {
            if Self::remove_unused_defs(block, &uses) {
                changed = true;
            }
        }

        Ok(changed)
    }
}

// ============================================================================
// Common Subexpression Elimination
// ============================================================================

/// Common subexpression elimination
#[derive(Debug, Default)]
pub struct CommonSubexpressionElimination;

impl CommonSubexpressionElimination {
    /// Create a new CSE pass
    pub fn new() -> Self {
        Self
    }
}

impl OptimizationPass for CommonSubexpressionElimination {
    fn name(&self) -> &str {
        "common_subexpression_elimination"
    }

    fn optimize_function(&self, _func: &mut IrFunction) -> AotResult<bool> {
        // CSE is implemented for AoT IR level, not IrFunction
        // Use AotCSE for AotProgram optimization
        Ok(false)
    }
}

// ============================================================================
// Strength Reduction
// ============================================================================

/// Strength reduction optimization
#[derive(Debug, Default)]
pub struct StrengthReduction;

impl StrengthReduction {
    /// Create a new strength reduction pass
    pub fn new() -> Self {
        Self
    }
}

impl OptimizationPass for StrengthReduction {
    fn name(&self) -> &str {
        "strength_reduction"
    }

    fn optimize_function(&self, _func: &mut IrFunction) -> AotResult<bool> {
        // Low-level IR optimization is not yet implemented
        // Use optimize_aot_program_with_strength_reduction for AoT IR
        Ok(false)
    }
}

// ============================================================================
// Loop Invariant Code Motion
// ============================================================================

/// Loop invariant code motion
///
/// Moves computations that produce the same result on every loop iteration
/// out of the loop to reduce redundant work.
///
/// Works by:
/// 1. Detecting natural loops in the CFG (using back edges)
/// 2. Identifying loop-invariant instructions (operands defined outside loop or
///    by other invariant instructions)
/// 3. Moving invariant instructions to a preheader block
#[derive(Debug, Default)]
pub struct LoopInvariantCodeMotion;

impl LoopInvariantCodeMotion {
    /// Create a new LICM pass
    pub fn new() -> Self {
        Self
    }

    /// Find predecessor blocks for each block
    fn find_predecessors(func: &IrFunction) -> HashMap<String, Vec<String>> {
        let mut preds: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize empty predecessor lists
        for block in &func.blocks {
            preds.insert(block.label.clone(), Vec::new());
        }

        // Build predecessor map from terminators
        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                match term {
                    Terminator::Jump(target) => {
                        if let Some(v) = preds.get_mut(target) {
                            v.push(block.label.clone());
                        }
                    }
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => {
                        if let Some(v) = preds.get_mut(then_block) {
                            v.push(block.label.clone());
                        }
                        if let Some(v) = preds.get_mut(else_block) {
                            v.push(block.label.clone());
                        }
                    }
                    Terminator::Switch { cases, default, .. } => {
                        for (_, target) in cases {
                            if let Some(v) = preds.get_mut(target) {
                                v.push(block.label.clone());
                            }
                        }
                        if let Some(v) = preds.get_mut(default) {
                            v.push(block.label.clone());
                        }
                    }
                    Terminator::Return(_) => {}
                }
            }
        }

        preds
    }

    /// Find back edges (edges where target dominates source - simplified detection)
    /// Returns (source_block, target_block) pairs
    fn find_back_edges(func: &IrFunction) -> Vec<(String, String)> {
        let mut back_edges = Vec::new();

        // A simple heuristic: if a block jumps to a block that comes earlier
        // in the function (by index), it might be a back edge
        let block_order: HashMap<String, usize> = func
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.label.clone(), i))
            .collect();

        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                let targets: Vec<&String> = match term {
                    Terminator::Jump(t) => vec![t],
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => vec![then_block, else_block],
                    Terminator::Switch { cases, default, .. } => {
                        let mut t: Vec<_> = cases.iter().map(|(_, target)| target).collect();
                        t.push(default);
                        t
                    }
                    Terminator::Return(_) => vec![],
                };

                for target in targets {
                    // Check if target comes before source (potential back edge)
                    if let (Some(&target_idx), Some(&source_idx)) =
                        (block_order.get(target), block_order.get(&block.label))
                    {
                        if target_idx <= source_idx {
                            back_edges.push((block.label.clone(), target.clone()));
                        }
                    }
                }
            }
        }

        back_edges
    }

    /// Find all blocks that are part of the loop defined by a back edge
    fn find_loop_blocks(
        header: &str,
        back_edge_source: &str,
        preds: &HashMap<String, Vec<String>>,
    ) -> HashSet<String> {
        let mut loop_blocks = HashSet::new();
        loop_blocks.insert(header.to_string());

        if header == back_edge_source {
            return loop_blocks;
        }

        let mut worklist = vec![back_edge_source.to_string()];
        while let Some(block) = worklist.pop() {
            if loop_blocks.contains(&block) {
                continue;
            }
            loop_blocks.insert(block.clone());

            // Add predecessors to worklist
            if let Some(block_preds) = preds.get(&block) {
                for pred in block_preds {
                    if !loop_blocks.contains(pred) {
                        worklist.push(pred.clone());
                    }
                }
            }
        }

        loop_blocks
    }

    /// Collect all variables defined within a set of blocks
    fn collect_loop_defs(func: &IrFunction, loop_blocks: &HashSet<String>) -> HashSet<String> {
        let mut defs = HashSet::new();

        for block in &func.blocks {
            if !loop_blocks.contains(&block.label) {
                continue;
            }

            for inst in &block.instructions {
                if let Some(dest) = Self::get_instruction_dest(inst) {
                    defs.insert(format!("{}.{}", dest.name, dest.version));
                }
            }
        }

        defs
    }

    /// Get the destination variable of an instruction
    fn get_instruction_dest(inst: &Instruction) -> Option<&VarRef> {
        match inst {
            Instruction::LoadConst { dest, .. } => Some(dest),
            Instruction::Copy { dest, .. } => Some(dest),
            Instruction::BinOp { dest, .. } => Some(dest),
            Instruction::UnaryOp { dest, .. } => Some(dest),
            Instruction::Call { dest, .. } => dest.as_ref(),
            Instruction::GetIndex { dest, .. } => Some(dest),
            Instruction::GetField { dest, .. } => Some(dest),
            Instruction::TypeAssert { dest, .. } => Some(dest),
            Instruction::Phi { dest, .. } => Some(dest),
            Instruction::SetIndex { .. } | Instruction::SetField { .. } => None,
        }
    }

    /// Check if an instruction is loop-invariant
    fn is_loop_invariant(
        inst: &Instruction,
        loop_defs: &HashSet<String>,
        invariant_defs: &HashSet<String>,
    ) -> bool {
        // Helper to check if a variable reference is invariant
        let is_operand_invariant = |var: &VarRef| -> bool {
            let key = format!("{}.{}", var.name, var.version);
            // Operand is invariant if:
            // 1. It's defined outside the loop (!loop_defs.contains)
            // 2. Or it's defined by another invariant instruction
            !loop_defs.contains(&key) || invariant_defs.contains(&key)
        };

        match inst {
            // Constants are always invariant
            Instruction::LoadConst { .. } => true,

            // Copy is invariant if source is invariant
            Instruction::Copy { src, .. } => is_operand_invariant(src),

            // Binary ops are invariant if both operands are invariant
            Instruction::BinOp { left, right, .. } => {
                is_operand_invariant(left) && is_operand_invariant(right)
            }

            // Unary ops are invariant if operand is invariant
            Instruction::UnaryOp { operand, .. } => is_operand_invariant(operand),

            // Calls are generally not invariant (may have side effects)
            Instruction::Call { .. } => false,

            // Array/field access may not be invariant (array contents may change)
            Instruction::GetIndex { array, index, .. } => {
                is_operand_invariant(array) && is_operand_invariant(index)
            }

            Instruction::GetField { object, .. } => is_operand_invariant(object),

            // These instructions have side effects
            Instruction::SetIndex { .. } | Instruction::SetField { .. } => false,

            // Type assertions are invariant if source is invariant
            Instruction::TypeAssert { src, .. } => is_operand_invariant(src),

            // Phi nodes depend on control flow, not invariant
            Instruction::Phi { .. } => false,
        }
    }

    /// Check if it's safe to move an instruction (no side effects)
    fn is_safe_to_hoist(inst: &Instruction) -> bool {
        match inst {
            Instruction::LoadConst { .. }
            | Instruction::Copy { .. }
            | Instruction::BinOp { .. }
            | Instruction::UnaryOp { .. }
            | Instruction::GetField { .. }
            | Instruction::TypeAssert { .. } => true,

            // GetIndex could be safe if we know the array is not modified
            Instruction::GetIndex { .. } => true,

            // These have side effects or depend on control flow
            Instruction::Call { .. }
            | Instruction::SetIndex { .. }
            | Instruction::SetField { .. }
            | Instruction::Phi { .. } => false,
        }
    }

    /// Retarget a terminator's edges from `old_target` to `new_target`
    fn retarget_terminator(
        terminator: &mut Option<Terminator>,
        old_target: &str,
        new_target: &str,
    ) {
        if let Some(term) = terminator {
            match term {
                Terminator::Jump(target) => {
                    if target == old_target {
                        *target = new_target.to_string();
                    }
                }
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    if then_block == old_target {
                        *then_block = new_target.to_string();
                    }
                    if else_block == old_target {
                        *else_block = new_target.to_string();
                    }
                }
                Terminator::Switch {
                    cases, default, ..
                } => {
                    for (_, target) in cases.iter_mut() {
                        if target == old_target {
                            *target = new_target.to_string();
                        }
                    }
                    if default == old_target {
                        *default = new_target.to_string();
                    }
                }
                Terminator::Return(_) => {}
            }
        }
    }
}

impl OptimizationPass for LoopInvariantCodeMotion {
    fn name(&self) -> &str {
        "loop_invariant_code_motion"
    }

    fn optimize_function(&self, func: &mut IrFunction) -> AotResult<bool> {
        let mut changed = false;

        // Build predecessor map
        let preds = Self::find_predecessors(func);

        // Find back edges (loops)
        let back_edges = Self::find_back_edges(func);

        // Process each loop
        for (source, header) in back_edges {
            // Find all blocks in this loop
            let loop_blocks = Self::find_loop_blocks(&header, &source, &preds);

            if loop_blocks.is_empty() {
                continue;
            }

            // Collect definitions within the loop
            let loop_defs = Self::collect_loop_defs(func, &loop_blocks);

            // Find invariant instructions (iterate until fixed point)
            let mut invariant_defs: HashSet<String> = HashSet::new();
            let mut invariant_insts: Vec<(String, usize)> = Vec::new();

            loop {
                let prev_size = invariant_defs.len();

                for block in &func.blocks {
                    if !loop_blocks.contains(&block.label) {
                        continue;
                    }

                    for (inst_idx, inst) in block.instructions.iter().enumerate() {
                        // Skip if already marked as invariant
                        if let Some(dest) = Self::get_instruction_dest(inst) {
                            let key = format!("{}.{}", dest.name, dest.version);
                            if invariant_defs.contains(&key) {
                                continue;
                            }
                        }

                        // Check if instruction is invariant and safe to hoist
                        if Self::is_loop_invariant(inst, &loop_defs, &invariant_defs)
                            && Self::is_safe_to_hoist(inst)
                        {
                            if let Some(dest) = Self::get_instruction_dest(inst) {
                                let key = format!("{}.{}", dest.name, dest.version);
                                invariant_defs.insert(key);
                                invariant_insts.push((block.label.clone(), inst_idx));
                            }
                        }
                    }
                }

                // Fixed point reached
                if invariant_defs.len() == prev_size {
                    break;
                }
            }

            // Hoist invariant instructions into a preheader block
            if !invariant_insts.is_empty() {
                // 1. Collect invariant instructions from loop blocks
                let mut hoisted_instructions = Vec::new();
                let mut removals: HashMap<String, Vec<usize>> = HashMap::new();
                for (block_label, inst_idx) in &invariant_insts {
                    removals
                        .entry(block_label.clone())
                        .or_default()
                        .push(*inst_idx);
                }

                // Collect instructions before removing them
                for block in &func.blocks {
                    if let Some(indices) = removals.get(&block.label) {
                        for &idx in indices {
                            hoisted_instructions.push(block.instructions[idx].clone());
                        }
                    }
                }

                // 2. Remove invariant instructions from loop blocks (descending order)
                for block in &mut func.blocks {
                    if let Some(indices) = removals.get_mut(&block.label) {
                        indices.sort_unstable_by(|a, b| b.cmp(a));
                        for &idx in indices.iter() {
                            block.instructions.remove(idx);
                        }
                    }
                }

                // 3. Create preheader block
                let preheader_label = format!("preheader_{}", header);
                let mut preheader = BasicBlock::new(preheader_label.clone());
                preheader.instructions = hoisted_instructions;
                preheader.terminator = Some(Terminator::Jump(header.clone()));

                // 4. Redirect non-loop predecessors of the header to the preheader
                let header_preds = preds.get(&header).cloned().unwrap_or_default();
                for pred_label in &header_preds {
                    if loop_blocks.contains(pred_label) {
                        continue; // Don't redirect back edges
                    }
                    for block in &mut func.blocks {
                        if block.label == *pred_label {
                            Self::retarget_terminator(
                                &mut block.terminator,
                                &header,
                                &preheader_label,
                            );
                        }
                    }
                }

                // 5. Update phi nodes in the header: incoming edges from non-loop
                //    predecessors should now reference the preheader
                let non_loop_preds: HashSet<String> = header_preds
                    .iter()
                    .filter(|p| !loop_blocks.contains(*p))
                    .cloned()
                    .collect();
                if !non_loop_preds.is_empty() {
                    for block in &mut func.blocks {
                        if block.label == header {
                            for inst in &mut block.instructions {
                                if let Instruction::Phi { incoming, .. } = inst {
                                    for (label, _) in incoming.iter_mut() {
                                        if non_loop_preds.contains(label) {
                                            *label = preheader_label.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 6. Insert preheader block before the header for readable CFG ordering
                let header_pos = func
                    .blocks
                    .iter()
                    .position(|b| b.label == header)
                    .unwrap_or(0);
                func.blocks.insert(header_pos, preheader);

                changed = true;
            }
        }

        Ok(changed)
    }
}

// ============================================================================
// Inlining
// ============================================================================

/// Inlining optimization
#[derive(Debug)]
pub struct Inlining {
    /// Maximum function size to inline (in statements)
    pub max_inline_size: usize,
    /// Maximum call depth to inline
    pub max_inline_depth: usize,
}

impl Default for Inlining {
    fn default() -> Self {
        Self {
            max_inline_size: 10,
            max_inline_depth: 3,
        }
    }
}

impl Inlining {
    /// Create a new inlining pass
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom max size
    pub fn with_max_size(max_inline_size: usize) -> Self {
        Self {
            max_inline_size,
            ..Self::default()
        }
    }
}

impl OptimizationPass for Inlining {
    fn name(&self) -> &str {
        "inlining"
    }

    fn optimize_function(&self, _func: &mut IrFunction) -> AotResult<bool> {
        // Note: Low-level IR inlining is not yet implemented
        // Use optimize_aot_program for high-level AoT IR inlining
        Ok(false)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::types::JuliaType;

    fn make_var(name: &str, version: usize) -> VarRef {
        VarRef {
            name: name.to_string(),
            version,
            ty: JuliaType::Int64,
        }
    }

    #[test]
    fn test_constant_folding_i64_add() {
        let folder = ConstantFolding::new();

        // Test folding 2 + 3 = 5
        let result =
            folder.fold_binop(BinOpKind::Add, &ConstValue::Int64(2), &ConstValue::Int64(3));
        assert_eq!(result, Some(ConstValue::Int64(5)));
    }

    #[test]
    fn test_constant_folding_i64_mul() {
        let folder = ConstantFolding::new();

        // Test folding 4 * 5 = 20
        let result =
            folder.fold_binop(BinOpKind::Mul, &ConstValue::Int64(4), &ConstValue::Int64(5));
        assert_eq!(result, Some(ConstValue::Int64(20)));
    }

    #[test]
    fn test_constant_folding_comparison() {
        let folder = ConstantFolding::new();

        // Test folding 3 < 5 = true
        let result = folder.fold_binop(BinOpKind::Lt, &ConstValue::Int64(3), &ConstValue::Int64(5));
        assert_eq!(result, Some(ConstValue::Bool(true)));

        // Test folding 5 < 3 = false
        let result = folder.fold_binop(BinOpKind::Lt, &ConstValue::Int64(5), &ConstValue::Int64(3));
        assert_eq!(result, Some(ConstValue::Bool(false)));
    }

    #[test]
    fn test_constant_folding_unary_neg() {
        let folder = ConstantFolding::new();

        // Test folding -5
        let result = folder.fold_unaryop(UnaryOpKind::Neg, &ConstValue::Int64(5));
        assert_eq!(result, Some(ConstValue::Int64(-5)));
    }

    #[test]
    fn test_constant_folding_block() {
        let mut folder = ConstantFolding::new();

        // Create a block with: x = 2 + 3
        let mut block = BasicBlock::new("test".to_string());
        let const_2 = make_var("const_2", 0);
        let const_3 = make_var("const_3", 0);
        let result = make_var("result", 0);

        block.instructions.push(Instruction::LoadConst {
            dest: const_2.clone(),
            value: ConstValue::Int64(2),
        });
        block.instructions.push(Instruction::LoadConst {
            dest: const_3.clone(),
            value: ConstValue::Int64(3),
        });
        block.instructions.push(Instruction::BinOp {
            dest: result.clone(),
            op: BinOpKind::Add,
            left: const_2,
            right: const_3,
        });

        let changed = folder.optimize_block(&mut block);
        assert!(changed);

        // The last instruction should now be LoadConst with value 5
        assert_eq!(block.instructions.len(), 3);
        assert!(
            matches!(&block.instructions[2], Instruction::LoadConst { .. }),
            "Expected LoadConst instruction, got {:?}",
            &block.instructions[2]
        );
        if let Instruction::LoadConst { value, .. } = &block.instructions[2] {
            assert_eq!(*value, ConstValue::Int64(5));
        }
    }

    #[test]
    fn test_dce_removes_unused_defs() {
        // Create a function with unused definitions
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        let unused_var = make_var("unused", 0);
        let used_var = make_var("used", 0);

        func.blocks[0].instructions.push(Instruction::LoadConst {
            dest: unused_var.clone(),
            value: ConstValue::Int64(42),
        });
        func.blocks[0].instructions.push(Instruction::LoadConst {
            dest: used_var.clone(),
            value: ConstValue::Int64(10),
        });
        func.blocks[0].terminator = Some(Terminator::Return(Some(used_var)));

        let pass = DeadCodeElimination::new();
        let changed = pass.optimize_function(&mut func).unwrap();

        assert!(changed);
        // unused_var definition should be removed
        assert_eq!(func.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn test_dce_keeps_used_defs() {
        // Create a function where all definitions are used
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        let var_a = make_var("a", 0);
        let var_b = make_var("b", 0);
        let var_c = make_var("c", 0);

        func.blocks[0].instructions.push(Instruction::LoadConst {
            dest: var_a.clone(),
            value: ConstValue::Int64(1),
        });
        func.blocks[0].instructions.push(Instruction::LoadConst {
            dest: var_b.clone(),
            value: ConstValue::Int64(2),
        });
        func.blocks[0].instructions.push(Instruction::BinOp {
            dest: var_c.clone(),
            op: BinOpKind::Add,
            left: var_a,
            right: var_b,
        });
        func.blocks[0].terminator = Some(Terminator::Return(Some(var_c)));

        let pass = DeadCodeElimination::new();
        let changed = pass.optimize_function(&mut func).unwrap();

        assert!(!changed);
        assert_eq!(func.blocks[0].instructions.len(), 3);
    }

    #[test]
    fn test_licm_finds_back_edges() {
        // Create a simple loop structure
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        // entry -> loop_header -> loop_body -> loop_header (back edge)
        func.blocks[0].label = "entry".to_string();
        func.blocks[0].terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut header = BasicBlock::new("loop_header".to_string());
        let cond = make_var("cond", 0);
        header.instructions.push(Instruction::LoadConst {
            dest: cond.clone(),
            value: ConstValue::Bool(true),
        });
        header.terminator = Some(Terminator::Branch {
            cond,
            then_block: "loop_body".to_string(),
            else_block: "exit".to_string(),
        });

        let mut body = BasicBlock::new("loop_body".to_string());
        body.terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut exit = BasicBlock::new("exit".to_string());
        exit.terminator = Some(Terminator::Return(None));

        func.blocks.push(header);
        func.blocks.push(body);
        func.blocks.push(exit);

        let back_edges = LoopInvariantCodeMotion::find_back_edges(&func);
        assert!(!back_edges.is_empty());
        // Should find loop_body -> loop_header back edge
        assert!(back_edges
            .iter()
            .any(|(src, tgt)| src == "loop_body" && tgt == "loop_header"));
    }

    #[test]
    fn test_licm_hoists_invariant_to_preheader() {
        // CFG: entry -> loop_header -> loop_body -> loop_header (back edge)
        //                           \-> exit
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        func.blocks[0].label = "entry".to_string();
        func.blocks[0].terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut header = BasicBlock::new("loop_header".to_string());
        let cond = make_var("cond", 0);
        header.instructions.push(Instruction::LoadConst {
            dest: cond.clone(),
            value: ConstValue::Bool(true),
        });
        header.terminator = Some(Terminator::Branch {
            cond,
            then_block: "loop_body".to_string(),
            else_block: "exit".to_string(),
        });

        let mut body = BasicBlock::new("loop_body".to_string());
        let invariant_var = make_var("inv", 0);
        body.instructions.push(Instruction::LoadConst {
            dest: invariant_var,
            value: ConstValue::Int64(42),
        });
        body.terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut exit = BasicBlock::new("exit".to_string());
        exit.terminator = Some(Terminator::Return(None));

        func.blocks.push(header);
        func.blocks.push(body);
        func.blocks.push(exit);

        let pass = LoopInvariantCodeMotion::new();
        let changed = pass.optimize_function(&mut func).unwrap();

        assert!(changed, "LICM should report a change");

        // A preheader block should have been created
        let preheader = func
            .blocks
            .iter()
            .find(|b| b.label.starts_with("preheader_"));
        assert!(
            preheader.is_some(),
            "Expected a preheader block, blocks: {:?}",
            func.blocks.iter().map(|b| &b.label).collect::<Vec<_>>()
        );

        let preheader = preheader.unwrap();
        assert_eq!(
            preheader.instructions.len(),
            1,
            "Preheader should contain 1 hoisted instruction"
        );
        assert!(
            matches!(
                &preheader.instructions[0],
                Instruction::LoadConst {
                    value: ConstValue::Int64(42),
                    ..
                }
            ),
            "Expected hoisted LoadConst(42), got {:?}",
            &preheader.instructions[0]
        );

        // Preheader should jump to the loop header
        assert!(
            matches!(&preheader.terminator, Some(Terminator::Jump(t)) if t == "loop_header"),
            "Preheader should jump to loop_header"
        );

        // The loop body should no longer contain the invariant instruction
        let body_block = func
            .blocks
            .iter()
            .find(|b| b.label == "loop_body")
            .unwrap();
        assert!(
            body_block.instructions.is_empty(),
            "Loop body should be empty after hoisting, got {:?}",
            body_block.instructions
        );
    }

    #[test]
    fn test_licm_entry_redirected_to_preheader() {
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        func.blocks[0].label = "entry".to_string();
        func.blocks[0].terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut header = BasicBlock::new("loop_header".to_string());
        let cond = make_var("cond", 0);
        header.instructions.push(Instruction::LoadConst {
            dest: cond.clone(),
            value: ConstValue::Bool(true),
        });
        header.terminator = Some(Terminator::Branch {
            cond,
            then_block: "loop_body".to_string(),
            else_block: "exit".to_string(),
        });

        let mut body = BasicBlock::new("loop_body".to_string());
        let inv = make_var("inv", 0);
        body.instructions.push(Instruction::LoadConst {
            dest: inv,
            value: ConstValue::Int64(99),
        });
        body.terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut exit = BasicBlock::new("exit".to_string());
        exit.terminator = Some(Terminator::Return(None));

        func.blocks.push(header);
        func.blocks.push(body);
        func.blocks.push(exit);

        let pass = LoopInvariantCodeMotion::new();
        let changed = pass.optimize_function(&mut func).unwrap();
        assert!(changed);

        // Entry should now jump to the preheader
        let entry = func.blocks.iter().find(|b| b.label == "entry").unwrap();
        assert!(
            matches!(&entry.terminator, Some(Terminator::Jump(t)) if t.starts_with("preheader_")),
            "Entry should jump to preheader, got {:?}",
            entry.terminator
        );
    }

    #[test]
    fn test_licm_back_edge_not_redirected() {
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        func.blocks[0].label = "entry".to_string();
        func.blocks[0].terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut header = BasicBlock::new("loop_header".to_string());
        let cond = make_var("cond", 0);
        header.instructions.push(Instruction::LoadConst {
            dest: cond.clone(),
            value: ConstValue::Bool(true),
        });
        header.terminator = Some(Terminator::Branch {
            cond,
            then_block: "loop_body".to_string(),
            else_block: "exit".to_string(),
        });

        let mut body = BasicBlock::new("loop_body".to_string());
        let inv = make_var("inv", 0);
        body.instructions.push(Instruction::LoadConst {
            dest: inv,
            value: ConstValue::Int64(7),
        });
        body.terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut exit = BasicBlock::new("exit".to_string());
        exit.terminator = Some(Terminator::Return(None));

        func.blocks.push(header);
        func.blocks.push(body);
        func.blocks.push(exit);

        let pass = LoopInvariantCodeMotion::new();
        pass.optimize_function(&mut func).unwrap();

        // The loop body's back edge should still point to loop_header
        let body_block = func
            .blocks
            .iter()
            .find(|b| b.label == "loop_body")
            .unwrap();
        assert!(
            matches!(&body_block.terminator, Some(Terminator::Jump(t)) if t == "loop_header"),
            "Back edge should still target loop_header, got {:?}",
            body_block.terminator
        );
    }

    #[test]
    fn test_licm_no_change_without_invariants() {
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        func.blocks[0].label = "entry".to_string();
        func.blocks[0].terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut header = BasicBlock::new("loop_header".to_string());
        let cond = make_var("cond", 0);
        header.instructions.push(Instruction::LoadConst {
            dest: cond.clone(),
            value: ConstValue::Bool(true),
        });
        header.terminator = Some(Terminator::Branch {
            cond: cond.clone(),
            then_block: "loop_body".to_string(),
            else_block: "exit".to_string(),
        });

        let mut body = BasicBlock::new("loop_body".to_string());
        let phi_var = make_var("phi", 0);
        body.instructions.push(Instruction::Phi {
            dest: phi_var,
            incoming: vec![
                ("loop_header".to_string(), cond.clone()),
                ("loop_body".to_string(), cond),
            ],
        });
        body.terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut exit = BasicBlock::new("exit".to_string());
        exit.terminator = Some(Terminator::Return(None));

        func.blocks.push(header);
        func.blocks.push(body);
        func.blocks.push(exit);

        let pass = LoopInvariantCodeMotion::new();
        let changed = pass.optimize_function(&mut func).unwrap();

        assert!(!changed, "No invariant instructions to hoist");
        assert!(
            !func
                .blocks
                .iter()
                .any(|b| b.label.starts_with("preheader_")),
            "No preheader should be created"
        );
    }

    #[test]
    fn test_licm_hoists_binop_with_external_operands() {
        let mut func = IrFunction::new("test".to_string(), vec![], JuliaType::Nothing);

        let var_a = make_var("a", 0);
        let var_b = make_var("b", 0);
        func.blocks[0].label = "entry".to_string();
        func.blocks[0].instructions.push(Instruction::LoadConst {
            dest: var_a.clone(),
            value: ConstValue::Int64(10),
        });
        func.blocks[0].instructions.push(Instruction::LoadConst {
            dest: var_b.clone(),
            value: ConstValue::Int64(20),
        });
        func.blocks[0].terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut header = BasicBlock::new("loop_header".to_string());
        let cond = make_var("cond", 0);
        header.instructions.push(Instruction::LoadConst {
            dest: cond.clone(),
            value: ConstValue::Bool(true),
        });
        header.terminator = Some(Terminator::Branch {
            cond,
            then_block: "loop_body".to_string(),
            else_block: "exit".to_string(),
        });

        let mut body = BasicBlock::new("loop_body".to_string());
        let result = make_var("result", 0);
        body.instructions.push(Instruction::BinOp {
            dest: result,
            op: BinOpKind::Add,
            left: var_a,
            right: var_b,
        });
        body.terminator = Some(Terminator::Jump("loop_header".to_string()));

        let mut exit = BasicBlock::new("exit".to_string());
        exit.terminator = Some(Terminator::Return(None));

        func.blocks.push(header);
        func.blocks.push(body);
        func.blocks.push(exit);

        let pass = LoopInvariantCodeMotion::new();
        let changed = pass.optimize_function(&mut func).unwrap();
        assert!(changed);

        let preheader = func
            .blocks
            .iter()
            .find(|b| b.label.starts_with("preheader_"))
            .unwrap();
        assert_eq!(preheader.instructions.len(), 1);
        assert!(
            matches!(&preheader.instructions[0], Instruction::BinOp { op: BinOpKind::Add, .. }),
            "Expected hoisted BinOp(Add), got {:?}",
            &preheader.instructions[0]
        );
    }

    #[test]
    fn test_licm_retarget_terminator_jump() {
        let mut term = Some(Terminator::Jump("old".to_string()));
        LoopInvariantCodeMotion::retarget_terminator(&mut term, "old", "new");
        assert!(
            matches!(&term, Some(Terminator::Jump(t)) if t == "new"),
            "Jump should be retargeted"
        );
    }

    #[test]
    fn test_licm_retarget_terminator_branch() {
        let cond = make_var("c", 0);
        let mut term = Some(Terminator::Branch {
            cond,
            then_block: "old".to_string(),
            else_block: "other".to_string(),
        });
        LoopInvariantCodeMotion::retarget_terminator(&mut term, "old", "new");
        assert!(
            matches!(&term, Some(Terminator::Branch { then_block, else_block, .. })
                if then_block == "new" && else_block == "other"),
            "Branch then_block should be retargeted"
        );
    }

    #[test]
    fn test_licm_retarget_terminator_no_match() {
        let mut term = Some(Terminator::Jump("keep".to_string()));
        LoopInvariantCodeMotion::retarget_terminator(&mut term, "old", "new");
        assert!(
            matches!(&term, Some(Terminator::Jump(t)) if t == "keep"),
            "Jump should not change if target doesn't match"
        );
    }
}
