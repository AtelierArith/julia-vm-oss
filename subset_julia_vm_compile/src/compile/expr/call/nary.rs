//! N-ary and binary operator call compilation.
//!
//! Handles reduction of n-argument operator calls to chained binary operations:
//! - `+(a, b, c)` → `+(+(a, b), c)` (left-fold)
//! - Both user-defined method dispatch and builtin operator paths

use crate::bytecode::{Instr, ValueType};
use crate::intrinsics::Intrinsic;
use crate::ir::core::Expr;

use crate::compile::{err, function_name_to_binary_op, CResult, CompileError, CoreCompiler};

impl CoreCompiler<'_> {
    /// Reduce n-arg operator call to chained binary calls.
    /// Julia's generic: +(a, b, c, xs...) = afoldl(+, a+b, c, xs...)
    /// So +(a, b, c) becomes +(+(a, b), c)
    pub(in crate::compile) fn compile_nary_operator_reduction(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> CResult<ValueType> {
        if args.len() < 2 {
            return err(format!("operator {} requires at least 2 arguments", op));
        }

        // Issue #5205: when the operator maps to a known BinaryOp, build the
        // left-folded nested Expr::BinaryOp tree and compile it through
        // compile_binary_op. That path tracks operand types statically, so
        // narrow-integer chains such as `a + b + c` (Int8) keep their precise
        // result type at each step (Int8 + Int8 -> Int8) and use wrapping
        // (modular) arithmetic, matching upstream Julia. The previous lowering
        // emitted bare CallDynamicBinaryBoth reductions with no compile-time
        // type info, so an Int8 intermediate widened to Int64 and then routed
        // back through the range-checked convert (Issue #5192), throwing
        // InexactError instead of wrapping. Parenthesized `(a + b) + c` already
        // reached compile_binary_op, which is why only the flattened chain
        // regressed.
        if let Some(binary_op) = function_name_to_binary_op(op) {
            let span = args[0].span();
            let mut folded = Expr::BinaryOp {
                op: binary_op,
                left: Box::new(args[0].clone()),
                right: Box::new(args[1].clone()),
                span,
            };
            for arg in args.iter().skip(2) {
                let next_span = arg.span();
                folded = Expr::BinaryOp {
                    op: binary_op,
                    left: Box::new(folded),
                    right: Box::new(arg.clone()),
                    span: next_span,
                };
            }
            return self.compile_expr(&folded);
        }

        // Fallback for operators without a BinaryOp mapping: left-fold via the
        // dynamic binary-op call path.
        // Left-fold: +(a, b, c, d) -> +(+(+(a, b), c), d)
        // First, compile args[0] and args[1] as a binary call
        self.compile_expr(&args[0])?;
        self.compile_expr(&args[1])?;
        self.compile_binary_op_call(op)?;

        // Then fold in each remaining argument
        for arg in args.iter().skip(2) {
            self.compile_expr(arg)?;
            self.compile_binary_op_call(op)?;
        }

        // Return type depends on the operator and argument types
        // For simplicity, use Any since we don't know the exact runtime type
        Ok(ValueType::Any)
    }

    /// Reduce n-arg builtin operator call to chained binary calls.
    /// Used when there's no user-defined method table for + or *.
    /// +(a, b, c, d) -> ((a + b) + c) + d
    pub(in crate::compile) fn compile_nary_builtin_reduction(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> CResult<ValueType> {
        if args.len() < 2 {
            return err(format!("operator {} requires at least 2 arguments", op));
        }

        let binary_op = function_name_to_binary_op(op).ok_or_else(|| {
            CompileError::Msg(format!("unsupported operator for n-arg reduction: {}", op))
        })?;

        // Compile first two args and apply binary op
        let mut result_ty = self.compile_builtin_binary_op(&binary_op, &args[0], &args[1])?;

        // Fold in remaining args: result = result op arg
        for arg in args.iter().skip(2) {
            // The result of the previous op is on the stack
            // Compile next arg
            let arg_ty = self.compile_expr(arg)?;

            // Determine the appropriate instruction based on BOTH types
            // If either accumulated result or new arg is F64, use F64 ops (type promotion)
            let use_float = result_ty == ValueType::F64 || arg_ty == ValueType::F64;

            match op {
                "+" => {
                    if use_float {
                        self.emit(Instr::AddF64);
                        result_ty = ValueType::F64;
                    } else {
                        self.emit(Instr::AddI64);
                        result_ty = ValueType::I64;
                    }
                }
                "*" => {
                    if use_float {
                        self.emit(Instr::MulF64);
                        result_ty = ValueType::F64;
                    } else {
                        self.emit(Instr::MulI64);
                        result_ty = ValueType::I64;
                    }
                }
                _ => return err(format!("unsupported nary operator: {}", op)),
            }
        }

        Ok(result_ty)
    }

    /// Compile a binary operator call using the dispatch mechanism.
    /// This handles both builtin operators and user-defined operator methods.
    pub(in crate::compile) fn compile_binary_op_call(&mut self, op: &str) -> CResult<()> {
        // Check if there's a user-defined method for this operator
        if let Some(table) = self.method_tables.get(op) {
            // User-defined methods exist - use runtime dispatch
            // We don't know the argument types at this point (they're on stack),
            // so we need dynamic dispatch

            // IMPORTANT: Always use CallDynamicBinaryBoth with intrinsic fallback for n-ary reduction.
            // Even when user-defined methods exist (e.g., for Complex or Rational),
            // we still need to support primitive operations like Int64 + Int64.
            // This fixes Issue #1053 where t[1] + t[2] + t[3] failed with MethodError.
            let intrinsic = match op {
                "+" => Intrinsic::DynamicAdd, // VM will use AddInt if both are I64
                "*" => Intrinsic::DynamicMul, // VM will use MulInt if both are I64
                "-" => Intrinsic::DynamicSub, // VM will use SubInt if both are I64
                "/" => Intrinsic::DynamicDiv,
                _ => return err(format!("unsupported nary operator: {}", op)),
            };

            // Build candidates from method table (Issue #6496: index-only
            // payload; the runtime derives the type names from FunctionInfo)
            let candidates: Vec<usize> = table
                .methods
                .iter()
                .filter(|m| m.param_count() == 2)
                .map(|m| m.global_index)
                .collect();

            self.emit(Instr::CallDynamicBinaryBoth(intrinsic, candidates));
        } else {
            // No user-defined methods - use builtin
            let instr = match op {
                "+" => Instr::AddI64, // VM will handle type coercion
                "*" => Instr::MulI64,
                _ => return err(format!("unsupported nary operator: {}", op)),
            };
            self.emit(instr);
        }
        Ok(())
    }
}
