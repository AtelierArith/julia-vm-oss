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
mod tail_recursion;

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
pub use tail_recursion::{optimize_aot_program_with_tail_recursion, AotTailRecursionOptimizer};

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

/// Optimization level for AoT compilation, mirroring `rustc`/`clang` `-O` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptLevel {
    /// `-O0`: no AoT IR optimizations (fastest compile, easiest to debug).
    O0,
    /// `-O1`: cheap, always-beneficial passes (constant folding + DCE).
    O1,
    /// `-O2`: the full recommended pipeline (default).
    #[default]
    O2,
    /// `-O3`: the full pipeline plus an extra cleanup round.
    O3,
}

impl OptLevel {
    /// Parse an `-O`/`--opt-level` value such as `0`, `1`, `2`, `3` (or `O2`).
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().trim_start_matches(['O', 'o']) {
            "0" => Ok(OptLevel::O0),
            "1" => Ok(OptLevel::O1),
            "2" | "" => Ok(OptLevel::O2),
            "3" => Ok(OptLevel::O3),
            other => Err(format!(
                "invalid optimization level '{}' (expected 0, 1, 2, or 3)",
                other
            )),
        }
    }
}

/// Run AoT optimizations at the requested [`OptLevel`], returning the total
/// number of changes applied. `O2` is equivalent to [`optimize_aot_program_full`].
pub fn optimize_aot_program_at_level(program: &mut AotProgram, level: OptLevel) -> usize {
    optimize_aot_program_at_level_with_options(program, level, false)
}

/// Run AoT optimizations while optionally preserving every function definition.
///
/// C ABI exports are resolved after optimization and may name generated method
/// symbols, so callers can request function preservation to avoid deleting
/// export candidates in the whole-function prune.
pub fn optimize_aot_program_at_level_with_options(
    program: &mut AotProgram,
    level: OptLevel,
    preserve_functions: bool,
) -> usize {
    match level {
        OptLevel::O0 => 0,
        OptLevel::O1 => {
            let mut total = 0;
            total += optimize_aot_program_with_constant_folding(program);
            total += optimize_aot_program_with_dce(program);
            total
        }
        OptLevel::O2 => optimize_aot_program_full_with_options(program, preserve_functions),
        OptLevel::O3 => {
            let mut total = optimize_aot_program_full_with_options(program, preserve_functions);
            // Extra cleanup round: inlining at O2 can expose more constant
            // folding / CSE / DCE opportunities.
            total += optimize_aot_program_with_constant_folding(program);
            total += optimize_aot_program_with_strength_reduction(program);
            total += optimize_aot_program_with_cse(program);
            total += optimize_aot_program_with_dce(program);
            if !preserve_functions {
                let funcs_before = program.functions.len();
                program.prune_unreachable_functions();
                total += funcs_before - program.functions.len();
            }
            total
        }
    }
}

/// Run all AoT optimizations on a program in the recommended order
pub fn optimize_aot_program_full(program: &mut AotProgram) -> usize {
    optimize_aot_program_full_with_options(program, false)
}

/// Run all AoT optimizations, optionally skipping whole-function pruning.
pub fn optimize_aot_program_full_with_options(
    program: &mut AotProgram,
    preserve_functions: bool,
) -> usize {
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

    // 6. Direct self-tail recursion to loop conversion (before inlining)
    total += optimize_aot_program_with_tail_recursion(program);

    // 7. Inlining (after other optimizations simplify functions)
    total += optimize_aot_program_with_inlining(program, 10);

    // 8. Another round of constant folding, strength reduction, CSE, and DCE after inlining
    total += optimize_aot_program_with_constant_folding(program);
    total += optimize_aot_program_with_strength_reduction(program);
    total += optimize_aot_program_with_cse(program);
    total += optimize_aot_program_with_dce(program);

    // 9. Whole-function dead-function elimination. After inlining and broadcast
    //    specialization, helpers whose only callers were the `broadcast`/`collect`
    //    calls that specialization replaced are now unreachable; codegen would
    //    otherwise still emit them, often with type-erased `-> Value` signatures
    //    (Issue #6629). Conservatively keeps all functions when the reachable
    //    closure dispatches dynamically.
    if !preserve_functions {
        let funcs_before = program.functions.len();
        program.prune_unreachable_functions();
        total += funcs_before - program.functions.len();
    }

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
    use crate::aot::ir::{
        AotBinOp, AotExpr, AotFunction, AotInlinePolicy, AotStmt, AotUnaryOp, CompoundAssignOp,
        IrModule,
    };
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
            inline_policy: AotInlinePolicy::Auto,
            return_needs_value: false,
        };
        assert!(candidate.should_inline(10));

        // Recursive function should not be inlined
        let recursive_candidate = InlineCandidate {
            name: "factorial".to_string(),
            size: 3,
            is_recursive: true,
            is_pure: true,
            score: i32::MIN,
            inline_policy: AotInlinePolicy::Auto,
            return_needs_value: false,
        };
        assert!(!recursive_candidate.should_inline(10));

        // Large function should not be inlined
        let large_candidate = InlineCandidate {
            name: "complex".to_string(),
            size: 50,
            is_recursive: false,
            is_pure: true,
            score: 10,
            inline_policy: AotInlinePolicy::Auto,
            return_needs_value: false,
        };
        assert!(!large_candidate.should_inline(10));

        let noinline_candidate = InlineCandidate {
            name: "marked_noinline".to_string(),
            size: 1,
            is_recursive: false,
            is_pure: true,
            score: 25,
            inline_policy: AotInlinePolicy::Never,
            return_needs_value: false,
        };
        assert!(!noinline_candidate.should_inline(10));

        let inline_candidate = InlineCandidate {
            name: "marked_inline".to_string(),
            size: 50,
            is_recursive: false,
            is_pure: true,
            score: 10,
            inline_policy: AotInlinePolicy::Always,
            return_needs_value: false,
        };
        assert!(inline_candidate.should_inline(10));

        let runtime_return_candidate = InlineCandidate {
            name: "runtime_return".to_string(),
            size: 1,
            is_recursive: false,
            is_pure: true,
            score: 25,
            inline_policy: AotInlinePolicy::Always,
            return_needs_value: true,
        };
        assert!(!runtime_return_candidate.should_inline(10));
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
            inline_policy: AotInlinePolicy::Auto,
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
                    inline_policy: AotInlinePolicy::Auto,
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
                inline_policy: AotInlinePolicy::Auto,
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

    #[test]
    fn test_inliner_respects_noinline_policy() {
        let mut program = AotProgram::new();

        let mut func = AotFunction::new(
            "marked_noinline".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        func.inline_policy = AotInlinePolicy::Never;
        func.body = vec![AotStmt::Return(Some(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }))];
        program.add_function(func);

        program.main = vec![AotStmt::Let {
            name: "y".to_string(),
            ty: StaticType::I64,
            value: AotExpr::CallStatic {
                function: "marked_noinline".to_string(),
                args: vec![AotExpr::LitI64(5)],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Auto,
            },
            is_mutable: false,
        }];

        let inlined = optimize_aot_program_with_inlining(&mut program, 10);
        assert_eq!(inlined, 0);
        assert!(matches!(
            &program.main[0],
            AotStmt::Let {
                value: AotExpr::CallStatic { function, .. },
                ..
            } if function == "marked_noinline"
        ));
    }

    #[test]
    fn test_inliner_respects_inline_policy_above_size_limit() {
        let mut program = AotProgram::new();

        let mut func = AotFunction::new(
            "marked_inline".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        func.inline_policy = AotInlinePolicy::Always;
        func.body = vec![
            AotStmt::Let {
                name: "a".to_string(),
                ty: StaticType::I64,
                value: AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::I64,
                },
                is_mutable: false,
            },
            AotStmt::Let {
                name: "b".to_string(),
                ty: StaticType::I64,
                value: AotExpr::Var {
                    name: "a".to_string(),
                    ty: StaticType::I64,
                },
                is_mutable: false,
            },
            AotStmt::Let {
                name: "c".to_string(),
                ty: StaticType::I64,
                value: AotExpr::Var {
                    name: "b".to_string(),
                    ty: StaticType::I64,
                },
                is_mutable: false,
            },
            AotStmt::Return(Some(AotExpr::Var {
                name: "c".to_string(),
                ty: StaticType::I64,
            })),
        ];
        program.add_function(func);

        program.main = vec![AotStmt::Let {
            name: "y".to_string(),
            ty: StaticType::I64,
            value: AotExpr::CallStatic {
                function: "marked_inline".to_string(),
                args: vec![AotExpr::LitI64(5)],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Auto,
            },
            is_mutable: false,
        }];

        let inlined = optimize_aot_program_with_inlining(&mut program, 1);
        assert_eq!(inlined, 1);
        assert!(
            program.main.len() > 1,
            "inline metadata should override the automatic size limit"
        );
    }

    #[test]
    fn test_inliner_callsite_inline_overrides_noinline_definition() {
        let mut program = AotProgram::new();

        let mut func = AotFunction::new(
            "definition_noinline".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        func.inline_policy = AotInlinePolicy::Never;
        func.body = vec![AotStmt::Return(Some(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }))];
        program.add_function(func);

        program.main = vec![AotStmt::Let {
            name: "y".to_string(),
            ty: StaticType::I64,
            value: AotExpr::CallStatic {
                function: "definition_noinline".to_string(),
                args: vec![AotExpr::LitI64(5)],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Always,
            },
            is_mutable: false,
        }];

        let inlined = optimize_aot_program_with_inlining(&mut program, 10);
        assert_eq!(inlined, 1);
    }

    #[test]
    fn test_inliner_callsite_noinline_overrides_inline_definition() {
        let mut program = AotProgram::new();

        let mut func = AotFunction::new(
            "definition_inline".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        func.inline_policy = AotInlinePolicy::Always;
        func.body = vec![AotStmt::Return(Some(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }))];
        program.add_function(func);

        program.main = vec![AotStmt::Let {
            name: "y".to_string(),
            ty: StaticType::I64,
            value: AotExpr::CallStatic {
                function: "definition_inline".to_string(),
                args: vec![AotExpr::LitI64(5)],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Never,
            },
            is_mutable: false,
        }];

        let inlined = optimize_aot_program_with_inlining(&mut program, 10);
        assert_eq!(inlined, 0);
        assert!(matches!(
            &program.main[0],
            AotStmt::Let {
                value: AotExpr::CallStatic { function, .. },
                ..
            } if function == "definition_inline"
        ));
    }

    #[test]
    fn tail_recursion_rewrites_direct_self_return_issue_6987() {
        let mut program = AotProgram::new();
        let n_var = || AotExpr::Var {
            name: "n".to_string(),
            ty: StaticType::I64,
        };
        let acc_var = || AotExpr::Var {
            name: "acc".to_string(),
            ty: StaticType::I64,
        };

        let mut fact = AotFunction::new(
            "fact".to_string(),
            vec![
                ("n".to_string(), StaticType::I64),
                ("acc".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        fact.body = vec![AotStmt::If {
            condition: AotExpr::BinOpStatic {
                op: AotBinOp::Le,
                left: Box::new(n_var()),
                right: Box::new(AotExpr::LitI64(1)),
                result_ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Return(Some(acc_var()))],
            else_branch: Some(vec![AotStmt::Return(Some(AotExpr::CallStatic {
                function: "fact".to_string(),
                args: vec![
                    AotExpr::BinOpStatic {
                        op: AotBinOp::Sub,
                        left: Box::new(n_var()),
                        right: Box::new(AotExpr::LitI64(1)),
                        result_ty: StaticType::I64,
                    },
                    AotExpr::BinOpStatic {
                        op: AotBinOp::Mul,
                        left: Box::new(acc_var()),
                        right: Box::new(n_var()),
                        result_ty: StaticType::I64,
                    },
                ],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Auto,
            }))]),
        }];
        program.add_function(fact);

        let transforms = optimize_aot_program_with_tail_recursion(&mut program);

        assert_eq!(transforms, 1);
        let body = &program.functions[0].body;
        assert!(matches!(
            body.as_slice(),
            [AotStmt::While {
                condition: AotExpr::LitBool(true),
                body,
            }] if matches!(
                body.as_slice(),
                [AotStmt::If {
                    else_branch: Some(else_branch),
                    ..
                }] if matches!(
                    else_branch.as_slice(),
                    [
                        AotStmt::Let { name: tmp_n, .. },
                        AotStmt::Let { name: tmp_acc, .. },
                        AotStmt::Assign {
                            target: AotExpr::Var { name: target_n, .. },
                            value: AotExpr::Var { name: value_n, .. },
                        },
                        AotStmt::Assign {
                            target: AotExpr::Var { name: target_acc, .. },
                            value: AotExpr::Var { name: value_acc, .. },
                        },
                        AotStmt::Continue,
                    ] if tmp_n == value_n
                        && tmp_acc == value_acc
                        && target_n == "n"
                        && target_acc == "acc"
                )
            )
        ));
    }

    #[test]
    fn tail_recursion_leaves_mutual_tail_calls_as_static_calls_issue_7060() {
        let mut program = AotProgram::new();
        let n_var = || AotExpr::Var {
            name: "n".to_string(),
            ty: StaticType::I64,
        };
        let decrement = || AotExpr::BinOpStatic {
            op: AotBinOp::Sub,
            left: Box::new(n_var()),
            right: Box::new(AotExpr::LitI64(1)),
            result_ty: StaticType::I64,
        };

        let mut even = AotFunction::new(
            "is_even".to_string(),
            vec![("n".to_string(), StaticType::I64)],
            StaticType::Bool,
        );
        even.body = vec![AotStmt::Return(Some(AotExpr::CallStatic {
            function: "is_odd".to_string(),
            args: vec![decrement()],
            return_ty: StaticType::Bool,
            inline_policy: AotInlinePolicy::Auto,
        }))];
        program.add_function(even);

        let mut odd = AotFunction::new(
            "is_odd".to_string(),
            vec![("n".to_string(), StaticType::I64)],
            StaticType::Bool,
        );
        odd.body = vec![AotStmt::Return(Some(AotExpr::CallStatic {
            function: "is_even".to_string(),
            args: vec![decrement()],
            return_ty: StaticType::Bool,
            inline_policy: AotInlinePolicy::Auto,
        }))];
        program.add_function(odd);

        let transforms = optimize_aot_program_with_tail_recursion(&mut program);

        assert_eq!(
            transforms, 0,
            "mutual recursion is supported as ordinary static calls; only direct self-tail recursion is TCO-lowered"
        );
        assert!(matches!(
            &program.functions[0].body[..],
            [AotStmt::Return(Some(AotExpr::CallStatic { function, .. }))] if function == "is_odd"
        ));
        assert!(matches!(
            &program.functions[1].body[..],
            [AotStmt::Return(Some(AotExpr::CallStatic { function, .. }))] if function == "is_even"
        ));
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

    #[test]
    fn loop_unrolling_empty_range_removes_body_issue_6946() {
        let mut program = AotProgram::new();

        program.main = vec![AotStmt::ForRange {
            var: "i".to_string(),
            start: AotExpr::LitI64(5),
            stop: AotExpr::LitI64(1),
            step: None,
            body: vec![AotStmt::Expr(AotExpr::CallDynamic {
                function: "side_effect".to_string(),
                args: vec![AotExpr::Var {
                    name: "i".to_string(),
                    ty: StaticType::I64,
                }],
            })],
        }];

        let optimized = optimize_aot_program_with_loops(&mut program);

        assert_eq!(optimized, 1);
        assert!(
            program.main.is_empty(),
            "empty positive range should remove the loop body"
        );
    }

    #[test]
    fn loop_unrolling_zero_step_keeps_original_loop_issue_6946() {
        let mut program = AotProgram::new();

        program.main = vec![AotStmt::ForRange {
            var: "i".to_string(),
            start: AotExpr::LitI64(1),
            stop: AotExpr::LitI64(4),
            step: Some(AotExpr::LitI64(0)),
            body: vec![AotStmt::Expr(AotExpr::Var {
                name: "i".to_string(),
                ty: StaticType::I64,
            })],
        }];

        let optimized = optimize_aot_program_with_loops(&mut program);

        assert_eq!(optimized, 0);
        assert!(matches!(
            program.main.as_slice(),
            [AotStmt::ForRange { .. }]
        ));
    }

    #[test]
    fn licm_does_not_hoist_alias_dependent_mutated_value_issue_6946() {
        let mut program = AotProgram::new();

        program.main = vec![
            AotStmt::Let {
                name: "acc".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(0),
                is_mutable: true,
            },
            AotStmt::ForRange {
                var: "i".to_string(),
                start: AotExpr::LitI64(1),
                stop: AotExpr::LitI64(16),
                step: None,
                body: vec![
                    AotStmt::Let {
                        name: "tmp".to_string(),
                        ty: StaticType::I64,
                        value: AotExpr::BinOpStatic {
                            op: AotBinOp::Add,
                            left: Box::new(AotExpr::Var {
                                name: "acc".to_string(),
                                ty: StaticType::I64,
                            }),
                            right: Box::new(AotExpr::LitI64(1)),
                            result_ty: StaticType::I64,
                        },
                        is_mutable: false,
                    },
                    AotStmt::CompoundAssign {
                        target: AotExpr::Var {
                            name: "acc".to_string(),
                            ty: StaticType::I64,
                        },
                        op: crate::aot::ir::CompoundAssignOp::AddAssign,
                        value: AotExpr::Var {
                            name: "tmp".to_string(),
                            ty: StaticType::I64,
                        },
                    },
                ],
            },
        ];
        let config = LoopOptConfig {
            enable_licm: true,
            enable_unrolling: false,
            ..LoopOptConfig::default()
        };

        let optimized = optimize_aot_program_with_loops_config(&mut program, config);

        assert_eq!(optimized, 0);
        assert!(matches!(
            program.main.as_slice(),
            [
                AotStmt::Let { name, .. },
                AotStmt::ForRange {
                    body,
                    ..
                }
            ] if name == "acc"
                && matches!(
                    body.as_slice(),
                    [
                        AotStmt::Let { name: tmp, .. },
                        AotStmt::CompoundAssign { .. }
                    ] if tmp == "tmp"
                )
        ));
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

    #[test]
    fn string_mul_constant_folding_issue_6970() {
        let mut program = AotProgram::new();

        program.main = vec![
            AotStmt::Let {
                name: "s".to_string(),
                ty: StaticType::Str,
                value: AotExpr::BinOpStatic {
                    op: AotBinOp::Mul,
                    left: Box::new(AotExpr::LitStr("a".to_string())),
                    right: Box::new(AotExpr::LitStr("b".to_string())),
                    result_ty: StaticType::Str,
                },
                is_mutable: false,
            },
            AotStmt::Let {
                name: "t".to_string(),
                ty: StaticType::Str,
                value: AotExpr::BinOpStatic {
                    op: AotBinOp::Mul,
                    left: Box::new(AotExpr::LitChar('c')),
                    right: Box::new(AotExpr::LitStr("d".to_string())),
                    result_ty: StaticType::Str,
                },
                is_mutable: false,
            },
        ];

        let folds = optimize_aot_program_with_constant_folding(&mut program);
        assert_eq!(folds, 2, "Expected 2 string constant folds");

        assert!(matches!(
            &program.main[0],
            AotStmt::Let {
                value: AotExpr::LitStr(value),
                ..
            } if value == "ab"
        ));
        assert!(matches!(
            &program.main[1],
            AotStmt::Let {
                value: AotExpr::LitStr(value),
                ..
            } if value == "cd"
        ));
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

    #[test]
    fn strength_reduction_preserves_bool_numeric_ops_issue_6980() {
        let mut program = AotProgram::new();

        program.main = vec![
            AotStmt::Expr(AotExpr::BinOpStatic {
                op: AotBinOp::IntDiv,
                left: Box::new(AotExpr::LitBool(true)),
                right: Box::new(AotExpr::LitI64(2)),
                result_ty: StaticType::I64,
            }),
            AotStmt::Expr(AotExpr::BinOpStatic {
                op: AotBinOp::Pow,
                left: Box::new(AotExpr::LitBool(false)),
                right: Box::new(AotExpr::LitI64(0)),
                result_ty: StaticType::Bool,
            }),
            AotStmt::Expr(AotExpr::BinOpStatic {
                op: AotBinOp::Mul,
                left: Box::new(AotExpr::LitBool(true)),
                right: Box::new(AotExpr::LitI64(2)),
                result_ty: StaticType::I64,
            }),
        ];

        let reductions = optimize_aot_program_with_strength_reduction(&mut program);
        assert_eq!(reductions, 0);
        assert!(matches!(
            &program.main[0],
            AotStmt::Expr(AotExpr::BinOpStatic {
                op: AotBinOp::IntDiv,
                ..
            })
        ));
        assert!(matches!(
            &program.main[1],
            AotStmt::Expr(AotExpr::BinOpStatic {
                op: AotBinOp::Pow,
                ..
            })
        ));
        assert!(matches!(
            &program.main[2],
            AotStmt::Expr(AotExpr::BinOpStatic {
                op: AotBinOp::Mul,
                ..
            })
        ));
    }

    // ========== AoT CSE Tests ==========

    #[test]
    fn test_cse_creation() {
        let cse = AotCSE::new();
        assert_eq!(cse.elimination_count(), 0);
    }

    #[test]
    fn cse_reuses_first_binding_without_temp_scaffold_issue_6943() {
        let mut program = AotProgram::new();
        let common_expr = || AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "a".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "b".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: common_expr(),
                is_mutable: false,
            },
            AotStmt::Let {
                name: "y".to_string(),
                ty: StaticType::I64,
                value: common_expr(),
                is_mutable: false,
            },
        ];

        let eliminations = optimize_aot_program_with_cse(&mut program);

        assert_eq!(eliminations, 1);
        assert_eq!(program.main.len(), 2);
        assert!(matches!(
            &program.main[1],
            AotStmt::Let {
                value: AotExpr::Var { name, .. },
                ..
            } if name == "x"
        ));
    }

    #[test]
    fn cse_reuses_dominating_expr_in_if_branch_issue_6985() {
        let mut program = AotProgram::new();
        let common_expr = || AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "a".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "b".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "pre".to_string(),
                ty: StaticType::I64,
                value: common_expr(),
                is_mutable: false,
            },
            AotStmt::If {
                condition: AotExpr::LitBool(true),
                then_branch: vec![AotStmt::Let {
                    name: "inside".to_string(),
                    ty: StaticType::I64,
                    value: common_expr(),
                    is_mutable: false,
                }],
                else_branch: None,
            },
        ];

        let eliminations = optimize_aot_program_with_cse(&mut program);

        assert_eq!(eliminations, 1);
        assert!(matches!(
            &program.main[1],
            AotStmt::If {
                then_branch,
                ..
            } if matches!(
                then_branch.as_slice(),
                [AotStmt::Let {
                    value: AotExpr::Var { name, .. },
                    ..
                }] if name == "pre"
            )
        ));
    }

    #[test]
    fn cse_reuses_pre_loop_expr_when_operands_not_modified_issue_6985() {
        let mut program = AotProgram::new();
        let common_expr = || AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "a".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "b".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "pre".to_string(),
                ty: StaticType::I64,
                value: common_expr(),
                is_mutable: false,
            },
            AotStmt::While {
                condition: AotExpr::LitBool(true),
                body: vec![AotStmt::Let {
                    name: "inside".to_string(),
                    ty: StaticType::I64,
                    value: common_expr(),
                    is_mutable: false,
                }],
            },
        ];

        let eliminations = optimize_aot_program_with_cse(&mut program);

        assert_eq!(eliminations, 1);
        assert!(matches!(
            &program.main[1],
            AotStmt::While { body, .. } if matches!(
                body.as_slice(),
                [AotStmt::Let {
                    value: AotExpr::Var { name, .. },
                    ..
                }] if name == "pre"
            )
        ));
    }

    #[test]
    fn cse_does_not_reuse_pre_loop_expr_after_operand_mutation_issue_6985() {
        let mut program = AotProgram::new();
        let common_expr = || AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "a".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "b".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "pre".to_string(),
                ty: StaticType::I64,
                value: common_expr(),
                is_mutable: false,
            },
            AotStmt::While {
                condition: AotExpr::LitBool(true),
                body: vec![
                    AotStmt::Assign {
                        target: AotExpr::Var {
                            name: "a".to_string(),
                            ty: StaticType::I64,
                        },
                        value: AotExpr::LitI64(3),
                    },
                    AotStmt::Let {
                        name: "inside".to_string(),
                        ty: StaticType::I64,
                        value: common_expr(),
                        is_mutable: false,
                    },
                ],
            },
        ];

        let eliminations = optimize_aot_program_with_cse(&mut program);

        assert_eq!(eliminations, 0);
        assert!(matches!(
            &program.main[1],
            AotStmt::While { body, .. } if matches!(
                body.as_slice(),
                [
                    AotStmt::Assign { .. },
                    AotStmt::Let {
                        value: AotExpr::BinOpStatic { .. },
                        ..
                    }
                ]
            )
        ));
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

    #[test]
    fn dce_simplifies_foldable_if_condition_issue_6984() {
        let mut program = AotProgram::new();

        program.main = vec![AotStmt::If {
            condition: AotExpr::BinOpStatic {
                op: AotBinOp::Eq,
                left: Box::new(AotExpr::BinOpStatic {
                    op: AotBinOp::Add,
                    left: Box::new(AotExpr::LitI64(1)),
                    right: Box::new(AotExpr::LitI64(1)),
                    result_ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(2)),
                result_ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(10),
                is_mutable: false,
            }],
            else_branch: Some(vec![AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(20),
                is_mutable: false,
            }]),
        }];

        let eliminations = optimize_aot_program_with_dce(&mut program);

        assert_eq!(eliminations, 1);
        assert!(matches!(
            program.main.as_slice(),
            [AotStmt::Let {
                value: AotExpr::LitI64(10),
                ..
            }]
        ));
    }

    #[test]
    fn dce_removes_foldable_false_while_condition_issue_6984() {
        let mut program = AotProgram::new();

        program.main = vec![AotStmt::While {
            condition: AotExpr::UnaryOp {
                op: AotUnaryOp::Not,
                operand: Box::new(AotExpr::LitBool(true)),
                result_ty: StaticType::Bool,
            },
            body: vec![AotStmt::Expr(AotExpr::CallDynamic {
                function: "side_effect".to_string(),
                args: vec![],
            })],
        }];

        let eliminations = optimize_aot_program_with_dce(&mut program);

        assert_eq!(eliminations, 1);
        assert!(program.main.is_empty());
    }

    #[test]
    fn dce_removes_overwritten_assign_issue_6986() {
        let mut program = AotProgram::new();

        let x_var = || AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(0),
                is_mutable: true,
            },
            AotStmt::Assign {
                target: x_var(),
                value: AotExpr::LitI64(1),
            },
            AotStmt::Assign {
                target: x_var(),
                value: AotExpr::LitI64(2),
            },
            AotStmt::Return(Some(x_var())),
        ];

        let eliminations = optimize_aot_program_with_dce(&mut program);

        assert_eq!(eliminations, 1);
        assert_eq!(program.main.len(), 3);
        assert!(
            !program.main.iter().any(|stmt| matches!(
                stmt,
                AotStmt::Assign {
                    value: AotExpr::LitI64(1),
                    ..
                }
            )),
            "The overwritten x = 1 store should be removed: {:?}",
            program.main
        );
    }

    #[test]
    fn dce_keeps_store_read_before_overwrite_issue_6986() {
        let mut program = AotProgram::new();

        let x_var = || AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(0),
                is_mutable: true,
            },
            AotStmt::Assign {
                target: x_var(),
                value: AotExpr::LitI64(1),
            },
            AotStmt::Expr(x_var()),
            AotStmt::Assign {
                target: x_var(),
                value: AotExpr::LitI64(2),
            },
            AotStmt::Return(Some(x_var())),
        ];

        let eliminations = optimize_aot_program_with_dce(&mut program);

        assert_eq!(eliminations, 0);
        assert_eq!(program.main.len(), 5);
    }

    #[test]
    fn dce_keeps_loop_body_mutation_read_after_loop_issue_7416() {
        let mut program = AotProgram::new();

        let total_var = || AotExpr::Var {
            name: "total".to_string(),
            ty: StaticType::I64,
        };
        let x_var = || AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "a".to_string(),
                ty: StaticType::Array {
                    element: Box::new(StaticType::I64),
                    ndims: Some(1),
                },
                value: AotExpr::ArrayLit {
                    elements: vec![AotExpr::LitI64(1), AotExpr::LitI64(2), AotExpr::LitI64(3)],
                    elem_ty: StaticType::I64,
                    shape: vec![3],
                },
                is_mutable: true,
            },
            AotStmt::Let {
                name: "total".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(0),
                is_mutable: true,
            },
            AotStmt::ForEach {
                var: "x".to_string(),
                iter: AotExpr::Var {
                    name: "a".to_string(),
                    ty: StaticType::Array {
                        element: Box::new(StaticType::I64),
                        ndims: Some(1),
                    },
                },
                body: vec![AotStmt::CompoundAssign {
                    target: total_var(),
                    op: CompoundAssignOp::AddAssign,
                    value: x_var(),
                }],
            },
            AotStmt::Expr(total_var()),
        ];

        let eliminations = optimize_aot_program_with_dce(&mut program);

        assert_eq!(eliminations, 0);
        let AotStmt::ForEach { body, .. } = &program.main[2] else {
            panic!("expected preserved for-each loop: {:?}", program.main);
        };
        assert!(
            matches!(body.as_slice(), [AotStmt::CompoundAssign { .. }]),
            "loop body mutation read after the loop must be preserved: {:?}",
            program.main
        );
    }

    #[test]
    fn dce_keeps_dead_store_with_effectful_rhs_issue_6986() {
        let mut program = AotProgram::new();

        let x_var = || AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        };

        program.main = vec![
            AotStmt::Let {
                name: "x".to_string(),
                ty: StaticType::I64,
                value: AotExpr::LitI64(0),
                is_mutable: true,
            },
            AotStmt::Assign {
                target: x_var(),
                value: AotExpr::CallDynamic {
                    function: "side_effect".to_string(),
                    args: vec![],
                },
            },
            AotStmt::Assign {
                target: x_var(),
                value: AotExpr::LitI64(2),
            },
            AotStmt::Return(Some(x_var())),
        ];

        let eliminations = optimize_aot_program_with_dce(&mut program);

        assert_eq!(eliminations, 0);
        assert!(matches!(
            &program.main[1],
            AotStmt::Assign {
                value: AotExpr::CallDynamic { .. },
                ..
            }
        ));
    }
}
