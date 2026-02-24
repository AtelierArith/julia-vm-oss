//! N-ary and binary operator call compilation.
//!
//! Handles reduction of n-argument operator calls to chained binary operations:
//! - `+(a, b, c)` → `+(+(a, b), c)` (left-fold)
//! - Both user-defined method dispatch and builtin operator paths

use crate::intrinsics::Intrinsic;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

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
                "+" => Intrinsic::AddFloat, // VM will use AddInt if both are I64
                "*" => Intrinsic::MulFloat, // VM will use MulInt if both are I64
                "-" => Intrinsic::SubFloat, // VM will use SubInt if both are I64
                "/" => Intrinsic::DivFloat,
                _ => return err(format!("unsupported nary operator: {}", op)),
            };

            // Build candidates from method table
            let candidates: Vec<(usize, String, String)> = table
                .methods
                .iter()
                .filter(|m| m.params.len() == 2)
                .map(|m| {
                    let left_ty = m.params[0].1.to_string();
                    let right_ty = m.params[1].1.to_string();
                    (m.global_index, left_ty, right_ty)
                })
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
