//! Higher-order function return type inference.
//!
//! Handles call-site specialization for HOFs like map, filter, and reduce
//! to infer their return types based on the function argument and collection type.

use crate::ir::core::Expr;
use crate::vm::ValueType;

use crate::compile::CoreCompiler;

impl CoreCompiler<'_> {
    /// Infer the return type of a `map(f, arr)` call at the call site.
    ///
    /// This performs call-site specialization for HOF type inference:
    /// - Extracts the function name from the first argument
    /// - Extracts the element type from the array argument
    /// - Looks up the function in the method table
    /// - Infers the return type by analyzing the function with the element type
    ///
    /// Returns `Some(ValueType)` if inference succeeds, `None` otherwise.
    pub(in crate::compile) fn infer_map_call_return_type(
        &mut self,
        func_arg: &Expr,
        arr_arg: &Expr,
    ) -> Option<ValueType> {
        // Extract function name from the first argument
        let func_name = match func_arg {
            Expr::FunctionRef { name, .. } => name.clone(),
            Expr::Var(name, _) => name.clone(),
            _ => {
                return None; // Can't handle lambdas or complex expressions yet
            }
        };

        // Extract array element type from the second argument
        let arr_type = self.infer_expr_type(arr_arg);
        let element_type = match &arr_type {
            ValueType::ArrayOf(elem) => {
                use crate::vm::ArrayElementType;
                match elem {
                    ArrayElementType::I64 => ValueType::I64,
                    ArrayElementType::F64 => ValueType::F64,
                    ArrayElementType::I32 => ValueType::I32,
                    ArrayElementType::F32 => ValueType::F32,
                    ArrayElementType::Bool => ValueType::Bool,
                    ArrayElementType::String => ValueType::Str,
                    ArrayElementType::Char => ValueType::Char,
                    _ => ValueType::Any,
                }
            }
            ValueType::Array => ValueType::Any,
            _ => return None, // Not an array
        };

        // Look up the function in the method table
        if let Some(table) = self.method_tables.get(func_name.as_str()) {
            // Create argument type for dispatch (single element)
            let arg_julia_type = self.value_type_to_julia_type(&element_type);

            // Try to dispatch and get return type
            if let Ok(method) = table.dispatch(&[arg_julia_type]) {
                // If method return type is Any, try to re-infer with concrete argument types
                let return_type = if matches!(&method.return_type, ValueType::Any) {
                    // Try to get the function IR for re-inference
                    if let Some(func_ir) = self
                        .shared_ctx
                        .function_ir_by_global_index
                        .get(&method.global_index)
                    {
                        crate::compile::inference::infer_function_return_type_v2_with_arg_types(
                            func_ir,
                            &self.shared_ctx.struct_table,
                            std::slice::from_ref(&element_type),
                        )
                    } else {
                        method.return_type.clone()
                    }
                } else {
                    method.return_type.clone()
                };

                // Convert return type to ArrayOf for map result
                let result_element_type = match &return_type {
                    ValueType::I64 => crate::vm::ArrayElementType::I64,
                    ValueType::F64 => crate::vm::ArrayElementType::F64,
                    ValueType::I32 => crate::vm::ArrayElementType::I32,
                    ValueType::F32 => crate::vm::ArrayElementType::F32,
                    ValueType::Bool => crate::vm::ArrayElementType::Bool,
                    ValueType::Str => crate::vm::ArrayElementType::String,
                    ValueType::Char => crate::vm::ArrayElementType::Char,
                    _ => crate::vm::ArrayElementType::Any,
                };
                return Some(ValueType::ArrayOf(result_element_type));
            }
        }

        None
    }

    /// Infer the return type of a `filter(pred, arr)` call at the call site.
    ///
    /// Filter returns an array with the same element type as the input.
    pub(in crate::compile) fn infer_filter_call_return_type(
        &mut self,
        arr_arg: &Expr,
    ) -> Option<ValueType> {
        // Extract array element type
        let arr_type = self.infer_expr_type(arr_arg);
        match &arr_type {
            ValueType::ArrayOf(elem) => Some(ValueType::ArrayOf(elem.clone())),
            ValueType::Array => Some(ValueType::Array),
            _ => None, // Not an array
        }
    }

    /// Infer the return type of a `reduce(op, itr)` or `foldl/foldr` call at the call site.
    ///
    /// For reduce operations:
    /// - If the operator is a known function like `+`, `*`, etc., the return type
    ///   depends on the element type
    /// - For `+` and `*` on integers, the result is Int64
    /// - For `+` and `*` on floats, the result is Float64
    ///
    /// This enables proper type inference for `reduce(+, [1,2,3])` -> Int64
    pub(in crate::compile) fn infer_reduce_call_return_type(
        &mut self,
        op_arg: &Expr,
        itr_arg: &Expr,
    ) -> Option<ValueType> {
        // Extract the iterator element type
        let itr_type = self.infer_expr_type(itr_arg);
        let element_type = match &itr_type {
            ValueType::ArrayOf(elem) => {
                use crate::vm::ArrayElementType;
                match elem {
                    ArrayElementType::I64 => ValueType::I64,
                    ArrayElementType::F64 => ValueType::F64,
                    ArrayElementType::I32 => ValueType::I32,
                    ArrayElementType::F32 => ValueType::F32,
                    ArrayElementType::Bool => ValueType::Bool,
                    ArrayElementType::String => ValueType::Str,
                    ArrayElementType::Char => ValueType::Char,
                    _ => ValueType::Any,
                }
            }
            ValueType::Array => ValueType::Any,
            _ => return None, // Not an iterable we can analyze
        };

        // Check if the operator is a known function
        let op_name = match op_arg {
            Expr::FunctionRef { name, .. } => name.clone(),
            Expr::Var(name, _) => name.clone(),
            _ => return None,
        };

        // For binary operators like +, *, -, etc., the result type is typically
        // the same as or promoted from the element type
        match op_name.as_str() {
            "+" | "*" | "-" | "/" | "^" | "min" | "max" => {
                // Numeric operations preserve or promote the type
                match &element_type {
                    ValueType::I64 | ValueType::I32 => {
                        // Integer operations return integers (for +, *, -)
                        // Division / on integers may return Float64, but for inference
                        // we assume integer result for reduce context
                        if op_name == "/" {
                            Some(ValueType::F64)
                        } else {
                            Some(ValueType::I64)
                        }
                    }
                    ValueType::F64 | ValueType::F32 => Some(ValueType::F64),
                    _ => Some(element_type),
                }
            }
            "&" | "|" | "xor" => {
                // Bitwise operations on integers return integers
                Some(element_type)
            }
            _ => {
                // For user-defined operators, try to look up and infer
                if let Some(table) = self.method_tables.get(op_name.as_str()) {
                    // reduce(op, itr) calls op(acc, elem) where acc and elem are both element_type
                    let arg_julia_type = self.value_type_to_julia_type(&element_type);
                    if let Ok(method) = table.dispatch(&[arg_julia_type.clone(), arg_julia_type]) {
                        // If method return type is Any, try to re-infer
                        if matches!(&method.return_type, ValueType::Any) {
                            if let Some(func_ir) = self
                                .shared_ctx
                                .function_ir_by_global_index
                                .get(&method.global_index)
                            {
                                let inferred = crate::compile::inference::infer_function_return_type_v2_with_arg_types(
                                    func_ir,
                                    &self.shared_ctx.struct_table,
                                    &[element_type.clone(), element_type.clone()],
                                );
                                return Some(inferred);
                            }
                        }
                        return Some(method.return_type.clone());
                    }
                }
                // Default to element type for unknown operators
                Some(element_type)
            }
        }
    }
}
