//! IR optimization passes for AoT compilation
//!
//! This module provides optimization passes that transform
//! the IR to produce more efficient code.

mod constant_folding;
mod cse;
mod dce;
mod inlining;
mod loop_opt;
mod pass;
mod strength_reduction;

// Re-exports
pub use constant_folding::{optimize_aot_program_with_constant_folding, AotConstantFolder};
pub use cse::{optimize_aot_program_with_cse, AotCSE};
pub use dce::{optimize_aot_program_with_dce, AotDeadCodeEliminator};
pub use inlining::{optimize_aot_program_with_inlining, AotInliner, InlineCandidate};
pub use loop_opt::{
    optimize_aot_program_with_loops, optimize_aot_program_with_loops_config, AotLoopOptimizer,
    LoopOptConfig,
};
pub use pass::{
    CommonSubexpressionElimination, ConstantFolding, DeadCodeElimination, Inlining,
    LoopInvariantCodeMotion, StrengthReduction,
};
pub use strength_reduction::{optimize_aot_program_with_strength_reduction, AotStrengthReducer};

use super::ir::{AotProgram, IrFunction, IrModule};
use super::AotResult;

/// Optimization pass trait
pub trait OptimizationPass: std::fmt::Debug {
    /// Name of this optimization pass
    fn name(&self) -> &str;

    /// Run the optimization on a function
    fn optimize_function(&self, func: &mut IrFunction) -> AotResult<bool>;

    /// Run the optimization on a module
    fn optimize_module(&self, module: &mut IrModule) -> AotResult<usize> {
        let mut changes = 0;
        for func in &mut module.functions {
            if self.optimize_function(func)? {
                changes += 1;
            }
        }
        Ok(changes)
    }
}

/// Run all AoT optimizations on a program in the recommended order
pub fn optimize_aot_program_full(program: &mut AotProgram) -> usize {
    let mut total = 0;

    // 1. Constant folding first (simplifies expressions)
    total += optimize_aot_program_with_constant_folding(program);

    // 2. Dead code elimination (removes unreachable code)
    total += optimize_aot_program_with_dce(program);

    // 3. Strength reduction (x * 2 -> x << 1, x^2 -> x * x, etc.)
    total += optimize_aot_program_with_strength_reduction(program);

    // 4. Common Subexpression Elimination (after constant folding and strength reduction)
    total += optimize_aot_program_with_cse(program);

    // 5. Loop optimization (after constant folding enables more unrolling)
    total += optimize_aot_program_with_loops(program);

    // 6. Inlining (after other optimizations simplify functions)
    total += optimize_aot_program_with_inlining(program, 10);

    // 7. Another round of constant folding, strength reduction, CSE, and DCE after inlining
    total += optimize_aot_program_with_constant_folding(program);
    total += optimize_aot_program_with_strength_reduction(program);
    total += optimize_aot_program_with_cse(program);
    total += optimize_aot_program_with_dce(program);

    total
}

/// Optimization pipeline
#[derive(Debug)]
pub struct OptimizationPipeline {
    passes: Vec<Box<dyn OptimizationPass>>,
    max_iterations: usize,
}

impl Default for OptimizationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl OptimizationPipeline {
    /// Create a new optimization pipeline
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            max_iterations: 10,
        }
    }

    /// Create a pipeline with default passes
    pub fn default_pipeline() -> Self {
        let mut pipeline = Self::new();
        pipeline.add_pass(Box::new(ConstantFolding::new()));
        pipeline.add_pass(Box::new(DeadCodeElimination::new()));
        pipeline.add_pass(Box::new(CommonSubexpressionElimination::new()));
        pipeline.add_pass(Box::new(StrengthReduction::new()));
        pipeline
    }

    /// Add a pass to the pipeline
    pub fn add_pass(&mut self, pass: Box<dyn OptimizationPass>) {
        self.passes.push(pass);
    }

    /// Set maximum iterations
    pub fn set_max_iterations(&mut self, max: usize) {
        self.max_iterations = max;
    }

    /// Run the pipeline on a module
    pub fn run(&self, module: &mut IrModule) -> AotResult<usize> {
        let mut total_changes = 0;

        for _iteration in 0..self.max_iterations {
            let mut changes_this_iteration = 0;

            for pass in &self.passes {
                changes_this_iteration += pass.optimize_module(module)?;
            }

            total_changes += changes_this_iteration;

            // Fixed point reached
            if changes_this_iteration == 0 {
                break;
            }
        }

        Ok(total_changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::ir::{AotBinOp, AotExpr, AotFunction, AotStmt, IrModule};
    use crate::aot::types::StaticType;

    #[test]
    fn test_constant_folding() {
        let pass = ConstantFolding::new();
        assert_eq!(pass.name(), "constant_folding");
    }

    #[test]
    fn test_dce() {
        let pass = DeadCodeElimination::new();
        assert_eq!(pass.name(), "dead_code_elimination");
    }

    #[test]
    fn test_inlining() {
        let pass = Inlining::with_max_size(30);
        assert_eq!(pass.max_inline_size, 30);
    }

    #[test]
    fn test_pipeline() {
        let pipeline = OptimizationPipeline::default_pipeline();
        let mut module = IrModule::new("test".to_string());
        let result = pipeline.run(&mut module);
        assert!(result.is_ok());
    }

    // ========== AoT Inliner Tests ==========

    #[test]
    fn test_aot_inliner_creation() {
        let inliner = AotInliner::new(10);
        assert_eq!(inliner.max_inline_size(), 10);
    }

    #[test]
    fn test_inline_candidate_should_inline() {
        // Small, non-recursive function should be inlined
        let candidate = InlineCandidate {
            name: "square".to_string(),
            size: 1,
            is_recursive: false,
            is_pure: true,
            score: 25,
        };
        assert!(candidate.should_inline(10));

        // Recursive function should not be inlined
        let recursive_candidate = InlineCandidate {
            name: "factorial".to_string(),
            size: 3,
            is_recursive: true,
            is_pure: true,
            score: i32::MIN,
        };
        assert!(!recursive_candidate.should_inline(10));

        // Large function should not be inlined
        let large_candidate = InlineCandidate {
            name: "complex".to_string(),
            size: 50,
            is_recursive: false,
            is_pure: true,
            score: 10,
        };
        assert!(!large_candidate.should_inline(10));
    }

    #[test]
    fn test_count_statements() {
        // Empty body
        let empty_stmts: Vec<AotStmt> = vec![];
        assert_eq!(AotInliner::count_statements(&empty_stmts), 0);

        // Single statement
        let single_stmt = vec![AotStmt::Return(Some(AotExpr::LitI64(42)))];
        assert_eq!(AotInliner::count_statements(&single_stmt), 1);

        // Statement with nested if
        let nested_stmts = vec![AotStmt::If {
            condition: AotExpr::LitBool(true),
            then_branch: vec![AotStmt::Return(Some(AotExpr::LitI64(1)))],
            else_branch: Some(vec![AotStmt::Return(Some(AotExpr::LitI64(0)))]),
        }];
        assert_eq!(AotInliner::count_statements(&nested_stmts), 3); // 1 if + 1 then + 1 else
    }

    #[test]
    fn test_expr_is_pure() {
        // Literals are pure
        assert!(AotInliner::expr_is_pure(&AotExpr::LitI64(42)));
        assert!(AotInliner::expr_is_pure(&AotExpr::LitF64(1.25)));
        assert!(AotInliner::expr_is_pure(&AotExpr::LitBool(true)));

        // Variables are pure
        assert!(AotInliner::expr_is_pure(&AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }));

        // Binary operations on pure operands are pure
        assert!(AotInliner::expr_is_pure(&AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::LitI64(1)),
            right: Box::new(AotExpr::LitI64(2)),
            result_ty: StaticType::I64,
        }));

        // Function calls are impure by default
        assert!(!AotInliner::expr_is_pure(&AotExpr::CallStatic {
            function: "foo".to_string(),
            args: vec![],
            return_ty: StaticType::I64,
        }));
    }

    #[test]
    fn test_analyze_simple_program() {
        let mut program = AotProgram::new();

        // Add a simple function: square(x) = x * x
        let mut square_func = AotFunction::new(
            "square".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        square_func.body = vec![AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Mul,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        }))];
        program.add_function(square_func);

        // Add a recursive function: factorial
        let mut factorial_func = AotFunction::new(
            "factorial".to_string(),
            vec![("n".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        factorial_func.body = vec![AotStmt::If {
            condition: AotExpr::BinOpStatic {
                op: AotBinOp::Le,
                left: Box::new(AotExpr::Var {
                    name: "n".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(1)),
                result_ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Return(Some(AotExpr::LitI64(1)))],
            else_branch: Some(vec![AotStmt::Return(Some(AotExpr::BinOpStatic {
                op: AotBinOp::Mul,
                left: Box::new(AotExpr::Var {
                    name: "n".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::CallStatic {
                    function: "factorial".to_string(),
                    args: vec![AotExpr::BinOpStatic {
                        op: AotBinOp::Sub,
                        left: Box::new(AotExpr::Var {
                            name: "n".to_string(),
                            ty: StaticType::I64,
                        }),
                        right: Box::new(AotExpr::LitI64(1)),
                        result_ty: StaticType::I64,
                    }],
                    return_ty: StaticType::I64,
                }),
                result_ty: StaticType::I64,
            }))]),
        }];
        program.add_function(factorial_func);

        let mut inliner = AotInliner::new(10);
        inliner.analyze_program(&program);

        // Check square function analysis
        let square_candidate = inliner.get_candidates().get("square").unwrap();
        assert_eq!(square_candidate.size, 1);
        assert!(!square_candidate.is_recursive);
        assert!(square_candidate.score > 0);

        // Check factorial function analysis
        let factorial_candidate = inliner.get_candidates().get("factorial").unwrap();
        assert!(factorial_candidate.is_recursive);
        assert!(factorial_candidate.score < 0);
    }

    #[test]
    fn test_inline_simple_function() {
        let mut program = AotProgram::new();

        // Add a simple function: square(x) = x * x
        let mut square_func = AotFunction::new(
            "square".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        square_func.body = vec![AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Mul,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        }))];
        program.add_function(square_func);

        // Main block: y = square(5)
        program.main = vec![AotStmt::Let {
            name: "y".to_string(),
            ty: StaticType::I64,
            value: AotExpr::CallStatic {
                function: "square".to_string(),
                args: vec![AotExpr::LitI64(5)],
                return_ty: StaticType::I64,
            },
            is_mutable: false,
        }];

        let inlined = optimize_aot_program_with_inlining(&mut program, 10);
        assert_eq!(inlined, 1, "Expected 1 function call to be inlined");

        // After inlining, main should have:
        // 1. let _inline0_0_x = 5
        // 2. let y = _inline0_0_x * _inline0_0_x
        assert!(
            program.main.len() >= 2,
            "Main should have at least 2 statements after inlining"
        );
    }

    // ========== AoT Loop Optimizer Tests ==========

    #[test]
    fn test_loop_optimizer_creation() {
        let optimizer = AotLoopOptimizer::new();
        assert_eq!(optimizer.licm_count(), 0);
        assert_eq!(optimizer.unroll_count(), 0);
    }

    #[test]
    fn test_loop_optimizer_config() {
        let config = LoopOptConfig {
            enable_licm: false,
            enable_unrolling: true,
            max_unroll_iterations: 4,
            max_unroll_body_size: 5,
        };
        let optimizer = AotLoopOptimizer::with_config(config.clone());
        assert_eq!(optimizer.config.max_unroll_iterations, 4);
    }

    #[test]
    fn test_loop_unrolling_simple() {
        let mut program = AotProgram::new();

        // Create a simple loop: for i in 1:4 result += i end
        program.main = vec![
            AotStmt::Let {
                name: "result".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(0),
                is_mutable: true,
            },
            AotStmt::ForRange {
                var: "i".to_string(),
                start: AotExpr::LitI64(1),
                stop: AotExpr::LitI64(4),
                step: None,
                body: vec![AotStmt::CompoundAssign {
                    target: AotExpr::Var {
                        name: "result".to_string(),
                        ty: StaticType::I64,
                    },
                    op: crate::aot::ir::CompoundAssignOp::AddAssign,
                    value: AotExpr::Var {
                        name: "i".to_string(),
                        ty: StaticType::I64,
                    },
                }],
            },
        ];

        let optimized = optimize_aot_program_with_loops(&mut program);

        // The loop should have been unrolled (4 iterations, small body)
        assert!(optimized > 0, "Expected loop to be unrolled");
        // After unrolling, we should have the let statement + 4 compound assigns
        assert!(
            program.main.len() >= 5,
            "Expected at least 5 statements after unrolling, got {}",
            program.main.len()
        );
    }

    // ========== AoT Constant Folder Tests ==========

    #[test]
    fn test_constant_folder_creation() {
        let folder = AotConstantFolder::new();
        assert_eq!(folder.fold_count(), 0);
    }

    #[test]
    fn test_constant_folding_simple_addition() {
        let mut program = AotProgram::new();

        // x = 2 + 3 (should become x = 5)
        program.main = vec![AotStmt::Let {
            name: "x".to_string(),
            ty: StaticType::I64,
            value: AotExpr::BinOpStatic {
                op: AotBinOp::Add,
                left: Box::new(AotExpr::LitI64(2)),
                right: Box::new(AotExpr::LitI64(3)),
                result_ty: StaticType::I64,
            },
            is_mutable: false,
        }];

        let folds = optimize_aot_program_with_constant_folding(&mut program);
        assert_eq!(folds, 1, "Expected 1 constant fold");

        // Check that the expression was folded
        if let AotStmt::Let { value, .. } = &program.main[0] {
            assert!(
                matches!(value, AotExpr::LitI64(5)),
                "Expected LitI64(5), got {:?}",
                value
            );
        }
    }

    // ========== AoT Strength Reducer Tests ==========

    #[test]
    fn test_strength_reducer_creation() {
        let reducer = AotStrengthReducer::new();
        assert_eq!(reducer.reduction_count(), 0);
    }

    #[test]
    fn test_strength_reduction_multiply_by_power_of_two() {
        let mut program = AotProgram::new();

        // x = y * 8 (should become x = y << 3)
        program.main = vec![AotStmt::Let {
            name: "x".to_string(),
            ty: StaticType::I64,
            value: AotExpr::BinOpStatic {
                op: AotBinOp::Mul,
                left: Box::new(AotExpr::Var {
                    name: "y".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(8)),
                result_ty: StaticType::I64,
            },
            is_mutable: false,
        }];

        let reductions = optimize_aot_program_with_strength_reduction(&mut program);
        assert_eq!(reductions, 1, "Expected 1 strength reduction");

        // Check that multiplication was replaced with shift
        assert!(
            matches!(&program.main[0], AotStmt::Let { .. }),
            "Expected AotStmt::Let, got {:?}",
            &program.main[0]
        );
        if let AotStmt::Let { value, .. } = &program.main[0] {
            assert!(
                matches!(value, AotExpr::BinOpStatic { .. }),
                "Expected BinOpStatic, got {:?}",
                value
            );
            if let AotExpr::BinOpStatic { op, .. } = value {
                assert_eq!(*op, AotBinOp::Shl, "Expected Shl operation");
            }
        }
    }

    // ========== AoT CSE Tests ==========

    #[test]
    fn test_cse_creation() {
        let cse = AotCSE::new();
        assert_eq!(cse.elimination_count(), 0);
    }

    // ========== AoT DCE Tests ==========

    #[test]
    fn test_dce_creation() {
        let dce = AotDeadCodeEliminator::new();
        assert_eq!(dce.elimination_count(), 0);
    }

    #[test]
    fn test_dce_removes_code_after_return() {
        let mut program = AotProgram::new();

        // return 5; x = 10; (x = 10 should be removed)
        program.main = vec![
            AotStmt::Return(Some(AotExpr::LitI64(5))),
            AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(10),
                is_mutable: false,
            },
        ];

        let eliminations = optimize_aot_program_with_dce(&mut program);
        assert_eq!(eliminations, 1, "Expected 1 elimination");
        assert_eq!(
            program.main.len(),
            1,
            "Expected 1 statement after DCE, got {}",
            program.main.len()
        );
    }
}
