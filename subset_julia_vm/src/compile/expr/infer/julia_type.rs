//! Julia type inference for method dispatch.
//!
//! Handles inference of JuliaType for expressions, used by the method dispatch system
//! to determine which method to call. Also provides ValueType-to-JuliaType conversion.

use crate::compile::promotion::{extract_complex_param, promote_type};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, Stmt, UnaryOp};
use crate::types::JuliaType;
use crate::vm::{ArrayElementType, ValueType};

use crate::compile::{
    binary_op_to_function_name, is_base_function, is_builtin_type_name, is_pi_name, CoreCompiler,
};

/// Extract the element type from a Complex{T} JuliaType.
/// Returns Some("Float64"), Some("Int64"), Some("Bool"), etc.
fn extract_complex_element(ty: &JuliaType) -> Option<String> {
    match ty {
        JuliaType::Struct(name) => extract_complex_param(name),
        _ => None,
    }
}

/// Convert a JuliaType to its element type string for Complex promotion.
fn julia_type_to_complex_elem(ty: &JuliaType) -> String {
    match ty {
        JuliaType::Float64 => "Float64".to_string(),
        JuliaType::Float32 => "Float32".to_string(),
        JuliaType::Int64 => "Int64".to_string(),
        JuliaType::Int32 => "Int32".to_string(),
        JuliaType::Int16 => "Int16".to_string(),
        JuliaType::Int8 => "Int8".to_string(),
        JuliaType::UInt64 => "UInt64".to_string(),
        JuliaType::UInt32 => "UInt32".to_string(),
        JuliaType::UInt16 => "UInt16".to_string(),
        JuliaType::UInt8 => "UInt8".to_string(),
        JuliaType::Bool => "Bool".to_string(),
        _ => "Float64".to_string(), // Default to Float64 for unknown types
    }
}

/// Promote two element types for Complex arithmetic.
/// Uses the centralized promotion module following Julia's promote_rule/promote_type pattern.
fn promote_complex_element(elem1: &str, elem2: &str) -> String {
    promote_type(elem1, elem2)
}

impl CoreCompiler<'_> {
    pub(in crate::compile) fn infer_julia_type(&self, expr: &Expr) -> JuliaType {
        match expr {
            Expr::Literal(lit, _) => match lit {
                Literal::Int(_) => JuliaType::Int64,
                Literal::Int128(_) => JuliaType::Int128,
                Literal::BigInt(_) => JuliaType::BigInt,
                Literal::BigFloat(_) => JuliaType::BigFloat,
                Literal::Float(_) => JuliaType::Float64,
                Literal::Float32(_) => JuliaType::Float32,
                Literal::Float16(_) => JuliaType::Float16,
                Literal::Str(_) => JuliaType::String,
                Literal::Char(_) => JuliaType::Char,
                Literal::Bool(_) => JuliaType::Bool,
                Literal::Nothing => JuliaType::Nothing,
                Literal::Missing => JuliaType::Missing,
                Literal::Module(_) => JuliaType::Module,
                Literal::Array(_, _) => JuliaType::Array,
                Literal::ArrayI64(_, _) => JuliaType::Array,
                Literal::ArrayBool(_, _) => JuliaType::Array,
                Literal::Struct(struct_name, _) => JuliaType::Struct(struct_name.clone()),
                Literal::Undef => JuliaType::Any, // Required kwarg marker
                // Metaprogramming literals
                Literal::Symbol(_) => JuliaType::Symbol,
                Literal::Expr { .. } => JuliaType::Expr,
                Literal::QuoteNode(_) => JuliaType::QuoteNode,
                Literal::LineNumberNode { .. } => JuliaType::LineNumberNode,
                // Regex literal
                Literal::Regex { .. } => JuliaType::Struct("Regex".to_string()),
                // Enum literal: type is the specific enum type
                Literal::Enum { type_name, .. } => JuliaType::Enum(type_name.clone()),
            },
            Expr::Var(name, _) => {
                // First check julia_type_locals for parametric types (e.g., Tuple{Int64, Int64})
                // This preserves precise type information that ValueType cannot represent
                if let Some(jt) = self.julia_type_locals.get(name) {
                    return jt.clone();
                }

                // Fall back to locals/global_types for ValueType-based lookup
                let var_type = self
                    .locals
                    .get(name)
                    .or_else(|| self.shared_ctx.global_types.get(name));
                match var_type {
                    Some(ValueType::I64) => JuliaType::Int64,
                    Some(ValueType::F64) => JuliaType::Float64,
                    Some(ValueType::Array) => JuliaType::Array,
                    Some(ValueType::ArrayOf(ref elem_type)) => {
                        // Convert ArrayElementType to JuliaType for proper Vector{T} dispatch
                        use crate::vm::ArrayElementType;
                        let julia_elem = match elem_type {
                            ArrayElementType::I8 => JuliaType::Int8,
                            ArrayElementType::I16 => JuliaType::Int16,
                            ArrayElementType::I32 => JuliaType::Int32,
                            ArrayElementType::I64 => JuliaType::Int64,
                            ArrayElementType::U8 => JuliaType::UInt8,
                            ArrayElementType::U16 => JuliaType::UInt16,
                            ArrayElementType::U32 => JuliaType::UInt32,
                            ArrayElementType::U64 => JuliaType::UInt64,
                            ArrayElementType::F32 => JuliaType::Float32,
                            ArrayElementType::F64 => JuliaType::Float64,
                            ArrayElementType::ComplexF32 => {
                                JuliaType::Struct("Complex{Float32}".to_string())
                            }
                            ArrayElementType::ComplexF64 => {
                                JuliaType::Struct("Complex{Float64}".to_string())
                            }
                            ArrayElementType::Bool => JuliaType::Bool,
                            ArrayElementType::String => JuliaType::String,
                            ArrayElementType::Char => JuliaType::Char,
                            ArrayElementType::StructOf(type_id) => self
                                .shared_ctx
                                .get_struct_name(*type_id)
                                .map(JuliaType::Struct)
                                .unwrap_or(JuliaType::Any),
                            ArrayElementType::StructInlineOf(type_id, _) => self
                                .shared_ctx
                                .get_struct_name(*type_id)
                                .map(JuliaType::Struct)
                                .unwrap_or(JuliaType::Any),
                            ArrayElementType::Struct => JuliaType::Any,
                            ArrayElementType::Any => JuliaType::Any,
                            ArrayElementType::TupleOf(ref field_types) => {
                                // Convert field types to Julia tuple type
                                let type_names: Vec<String> = field_types
                                    .iter()
                                    .map(|ft| match ft {
                                        ArrayElementType::I64 => "Int64".to_string(),
                                        ArrayElementType::F64 => "Float64".to_string(),
                                        ArrayElementType::Bool => "Bool".to_string(),
                                        ArrayElementType::String => "String".to_string(),
                                        _ => "Any".to_string(),
                                    })
                                    .collect();
                                JuliaType::Struct(format!("Tuple{{{}}}", type_names.join(", ")))
                            }
                        };
                        JuliaType::VectorOf(Box::new(julia_elem))
                    }
                    Some(ValueType::Str) => JuliaType::String,
                    Some(ValueType::Struct(type_id)) => {
                        // Look up struct name from type_id (handles all structs including Complex)
                        self.shared_ctx
                            .get_struct_name(*type_id)
                            .map(JuliaType::Struct)
                            .unwrap_or(JuliaType::Any)
                    }
                    Some(ValueType::Rng) => JuliaType::Any,
                    Some(ValueType::Range) => JuliaType::UnitRange, // Default to UnitRange
                    Some(ValueType::Tuple) => JuliaType::Tuple,
                    Some(ValueType::NamedTuple) => JuliaType::NamedTuple,
                    Some(ValueType::Dict) => JuliaType::Dict,
                    Some(ValueType::Set) => JuliaType::Set,
                    Some(ValueType::Nothing) => JuliaType::Nothing,
                    Some(ValueType::Missing) => JuliaType::Missing,
                    Some(ValueType::Generator) => JuliaType::Any,
                    Some(ValueType::Char) => JuliaType::Char,
                    Some(ValueType::DataType) => JuliaType::DataType,
                    Some(ValueType::Module) => JuliaType::Module,
                    Some(ValueType::Any) => JuliaType::Any,
                    Some(ValueType::BigInt) => JuliaType::BigInt,
                    Some(ValueType::BigFloat) => JuliaType::BigFloat,
                    Some(ValueType::IO) => JuliaType::IO,
                    // New numeric types
                    Some(ValueType::I8) => JuliaType::Int8,
                    Some(ValueType::I16) => JuliaType::Int16,
                    Some(ValueType::I32) => JuliaType::Int32,
                    Some(ValueType::I128) => JuliaType::Int128,
                    Some(ValueType::U8) => JuliaType::UInt8,
                    Some(ValueType::U16) => JuliaType::UInt16,
                    Some(ValueType::U32) => JuliaType::UInt32,
                    Some(ValueType::U64) => JuliaType::UInt64,
                    Some(ValueType::U128) => JuliaType::UInt128,
                    Some(ValueType::F16) => JuliaType::Float16,
                    Some(ValueType::F32) => JuliaType::Float32,
                    Some(ValueType::Bool) => JuliaType::Bool,
                    // Macro system types
                    Some(ValueType::Symbol) => JuliaType::Symbol,
                    Some(ValueType::Expr) => JuliaType::Expr,
                    Some(ValueType::QuoteNode) => JuliaType::QuoteNode,
                    Some(ValueType::LineNumberNode) => JuliaType::LineNumberNode,
                    Some(ValueType::GlobalRef) => JuliaType::GlobalRef,
                    Some(ValueType::Pairs) => JuliaType::Pairs,
                    Some(ValueType::Function) => JuliaType::Function,
                    // Regex types
                    Some(ValueType::Regex) => JuliaType::Struct("Regex".to_string()),
                    Some(ValueType::RegexMatch) => JuliaType::Struct("RegexMatch".to_string()),
                    // Enum type
                    Some(ValueType::Enum) => JuliaType::Any,
                    // Union type
                    Some(ValueType::Union(_)) => JuliaType::Any,
                    // Memory type
                    Some(ValueType::Memory) | Some(ValueType::MemoryOf(_)) => JuliaType::Any,
                    None => {
                        // ============================================================
                        // TYPE INFERENCE PRIORITY ORDER (Issue #1692, #1701)
                        // ============================================================
                        //
                        // IMPORTANT: The order of checks in this branch is critical!
                        // Changing the order can break type dispatch for various scenarios.
                        //
                        // Priority order (highest to lowest):
                        //   1. Special constants (pi, ℯ)
                        //   2. Type parameters from where clause (T, S, etc.)
                        //   3. Builtin type names (Int64, Float64, Tuple, Array, etc.)
                        //   4. User-defined struct types (as Type{T})
                        //   4.5. User-defined abstract types (as Type{T})
                        //   5. Function names (names in method_tables)
                        //   5.5. Builtin function names (is_base_function)
                        //   6. Global const types from shared_ctx.global_types (Issue #3088)
                        //   7. Fallback to Any
                        //
                        // KEY INVARIANT: Builtin type names MUST be checked BEFORE
                        // method_tables because types like Tuple/Array can have methods
                        // defined (e.g., Tuple(ci::CartesianIndex)), but should still
                        // be typed as TypeOf(T) for proper Type{T} dispatch.
                        //
                        // Without this ordering, `nameof(Tuple)` would dispatch to
                        // `nameof(f::Function)` instead of `nameof(t::Type)`.
                        //
                        // See Issue #1692 for the original bug and test fixture:
                        //   tests/fixtures/type_inference/builtin_type_dispatch.jl
                        // ============================================================

                        // Priority 1: Special constants (pi, Euler's number, etc.)
                        if is_pi_name(name) {
                            return JuliaType::Float64;
                        }

                        // Priority 2: Type parameters from where clause
                        if let Some(tp) = self
                            .current_type_param_index
                            .get(name.as_str())
                            .and_then(|&idx| self.current_type_params.get(idx))
                        {
                            // If TypeVar has an upper bound, use it for dispatch
                            // e.g., T<:Integer → JuliaType::Integer (enables static dispatch)
                            if let Some(bound) = tp.get_upper_bound() {
                                if let Some(bound_type) = JuliaType::from_name(bound) {
                                    return bound_type;
                                }
                            }
                            // Unconstrained TypeVar or unknown bound → DataType
                            // (preserves existing behavior for T(x) constructor calls)
                            return JuliaType::DataType;
                        }

                        // Priority 3: Builtin type names (MUST come before method_tables!)
                        if is_builtin_type_name(name) {
                            // Known type names like Int64, Float64, Tuple, Array should be TypeOf(T)
                            // so that Type{T} dispatch works correctly
                            return if let Some(resolved) = JuliaType::from_name(name) {
                                JuliaType::TypeOf(Box::new(resolved))
                            } else {
                                // Fallback for types from_name doesn't handle
                                JuliaType::DataType
                            };
                        }

                        // Priority 4: User-defined struct types (Issue #2695)
                        // Must come before method_tables because struct convenience
                        // constructors register the struct name in method_tables, but
                        // bare references to a struct name should resolve as Type, not
                        // Function. E.g., fieldnames(Broadcasted) needs Broadcasted to
                        // be typed as Type{Broadcasted}, not Function.
                        if self.shared_ctx.struct_table.contains_key(name) {
                            return JuliaType::TypeOf(Box::new(JuliaType::Struct(
                                name.to_string(),
                            )));
                        }

                        // Priority 4.5: User-defined abstract types
                        if self.abstract_type_names.contains(name) {
                            return JuliaType::TypeOf(Box::new(JuliaType::AbstractUser(
                                name.to_string(),
                                None,
                            )));
                        }

                        // Priority 5: Function names in method_tables
                        if self.method_tables.contains_key(name) {
                            // Names in method_tables are user-defined functions
                            // This enables proper dispatch for HOFs like map(f::Function, A)
                            // Issue #1658: Without this, function names return Any instead of Function
                            return JuliaType::Function;
                        }

                        // Priority 5.5: Builtin function names (Issue #2070)
                        // Builtins like uppercase, lowercase, etc. are not in method_tables
                        // but should still be typed as Function for HOF dispatch
                        if is_base_function(name) {
                            return JuliaType::Function;
                        }

                        // Priority 6: Global const types (Issue #3088)
                        // Global consts like `im = Complex{Bool}(false, true)` are tracked in
                        // shared_ctx.global_types. Convert ValueType -> JuliaType for dispatch.
                        if let Some(vt) = self.shared_ctx.global_types.get(name) {
                            let jt = self.value_type_to_julia_type(vt);
                            if jt != JuliaType::Any {
                                return jt;
                            }
                        }

                        // Priority 7: Fallback to Any for unknown names
                        JuliaType::Any
                    }
                }
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let lt = self.infer_julia_type(left);
                let rt = self.infer_julia_type(right);

                // Check if either operand is a struct type
                let left_is_struct = matches!(lt, JuliaType::Struct(_));
                let right_is_struct = matches!(rt, JuliaType::Struct(_));

                // Check if there's a user-defined operator for these types
                let op_name = binary_op_to_function_name(op);
                if let Some(table) = self.method_tables.get(op_name) {
                    let arg_types = vec![lt.clone(), rt.clone()];
                    if let Ok(method) = table.dispatch(&arg_types) {
                        let return_type = self.value_type_to_julia_type(&method.return_type);
                        // If method dispatch succeeded with a concrete type, use it
                        // But if return type is Any AND Complex types are involved,
                        // fall through to use Complex promotion rules (Issue #1329)
                        if return_type != JuliaType::Any {
                            return return_type;
                        }
                        // Return type is Any - check if we can do better with Complex promotion
                    }
                }

                // Handle struct types (fixes Issue #1055)
                if left_is_struct || right_is_struct {
                    // Comparison operators still return Bool regardless
                    if matches!(
                        op,
                        BinaryOp::Lt
                            | BinaryOp::Gt
                            | BinaryOp::Le
                            | BinaryOp::Ge
                            | BinaryOp::Eq
                            | BinaryOp::Ne
                    ) {
                        return JuliaType::Bool;
                    }

                    // Handle Complex arithmetic (Issue #1329)
                    // Complex types follow Julia's promotion rules
                    let left_complex_elem = extract_complex_element(&lt);
                    let right_complex_elem = extract_complex_element(&rt);

                    if left_complex_elem.is_some() || right_complex_elem.is_some() {
                        // Apply Complex promotion rules
                        let result_elem = match (&left_complex_elem, &right_complex_elem) {
                            // Complex op Complex -> Complex{promote(T1, T2)}
                            (Some(e1), Some(e2)) => promote_complex_element(e1, e2),
                            // Complex op Real -> Complex{promote(T, Real)}
                            (Some(e), None) => {
                                promote_complex_element(e, &julia_type_to_complex_elem(&rt))
                            }
                            // Real op Complex -> Complex{promote(Real, T)}
                            (None, Some(e)) => {
                                promote_complex_element(&julia_type_to_complex_elem(&lt), e)
                            }
                            // Should not happen
                            (None, None) => "Float64".to_string(),
                        };
                        return JuliaType::Struct(format!("Complex{{{}}}", result_elem));
                    }

                    // Other struct types: return Any for runtime dispatch
                    return JuliaType::Any;
                }

                // Builtin operator type rules (for primitive types only)
                // Complex operations use Pure Julia dispatch (base/complex.jl)

                // If either operand is Any, result type depends on the operation
                let has_any = lt == JuliaType::Any || rt == JuliaType::Any;

                match op {
                    // Division always returns Float64
                    BinaryOp::Div => JuliaType::Float64,
                    // Power: String^Int -> String (repeat), Int^Int -> Int, Any -> Any, otherwise -> Float64
                    BinaryOp::Pow => {
                        if lt == JuliaType::String {
                            // String ^ Int returns String (via repeat function)
                            JuliaType::String
                        } else if lt == JuliaType::Int64 && rt == JuliaType::Int64 {
                            JuliaType::Int64
                        } else if has_any {
                            JuliaType::Any
                        } else {
                            JuliaType::Float64
                        }
                    }
                    // Comparisons always return Bool
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => JuliaType::Bool,
                    // For arithmetic operations, infer result type based on operands
                    _ => {
                        // Issue #2127: String/Char concatenation via * returns String
                        if matches!(op, BinaryOp::Mul)
                            && (lt == JuliaType::String
                                || rt == JuliaType::String
                                || (matches!(lt, JuliaType::String | JuliaType::Char)
                                    && matches!(rt, JuliaType::String | JuliaType::Char)))
                        {
                            return JuliaType::String;
                        }
                        if has_any {
                            // Any operand means result is Any (runtime determines actual type)
                            JuliaType::Any
                        } else if lt == JuliaType::Float64 || rt == JuliaType::Float64 {
                            JuliaType::Float64
                        } else {
                            JuliaType::Int64
                        }
                    }
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                // Infer element type for proper Vector{T} dispatch
                if elements.is_empty() {
                    JuliaType::Array
                } else {
                    let first_elem_type = self.infer_julia_type(&elements[0]);
                    // Check if all elements have the same type
                    let all_same = elements
                        .iter()
                        .skip(1)
                        .all(|e| self.infer_julia_type(e) == first_elem_type);
                    if all_same {
                        JuliaType::VectorOf(Box::new(first_elem_type))
                    } else {
                        JuliaType::VectorOf(Box::new(JuliaType::Any))
                    }
                }
            }
            Expr::Range { step, .. } => {
                // UnitRange when step is None (or 1), StepRange otherwise
                if step.is_none() {
                    JuliaType::UnitRange
                } else {
                    JuliaType::StepRange
                }
            }
            Expr::Comprehension { .. } | Expr::MultiComprehension { .. } => JuliaType::Array,
            Expr::Generator { .. } => {
                JuliaType::Any // Generator maps to Any for type dispatch
            }
            Expr::TupleLiteral { elements, .. } => {
                // Infer parametric tuple type for proper dispatch
                // e.g., (42, 10) -> Tuple{Int64, Int64}
                let elem_types: Vec<JuliaType> =
                    elements.iter().map(|e| self.infer_julia_type(e)).collect();
                if elem_types.is_empty() {
                    JuliaType::Tuple // Empty tuple
                } else {
                    JuliaType::TupleOf(elem_types)
                }
            }
            Expr::NamedTupleLiteral { .. } => JuliaType::NamedTuple,
            Expr::Index { array, indices, .. } => {
                let is_slice = indices
                    .iter()
                    .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }));
                if is_slice {
                    return JuliaType::Array;
                }

                // Check array type from locals to get proper element type for struct arrays
                // This enables correct method dispatch for expressions like `imag(arr[1])`
                // where arr is an array of Complex structs
                if let Expr::Var(name, _) = array.as_ref() {
                    if let Some(ValueType::ArrayOf(ArrayElementType::StructOf(type_id))) =
                        self.locals.get(name)
                    {
                        // Only return specific type for struct arrays to enable correct dispatch
                        if let Some(struct_name) = self.shared_ctx.get_struct_name(*type_id) {
                            return JuliaType::Struct(struct_name);
                        }
                    }
                }

                // Default to Any for non-struct arrays and unknown types
                JuliaType::Any
            }
            Expr::SliceAll { .. } => JuliaType::Array,
            Expr::Call { function, args, .. } => {
                // Handle lowercase "complex" function -> Complex struct
                if function == "complex" {
                    return JuliaType::Struct("Complex{Float64}".to_string());
                }
                // Check if any argument is a struct or Any type
                // If so, we should defer to method dispatch for these functions
                let has_struct_or_any_arg = args.iter().any(|arg| {
                    let ty = self.infer_julia_type(arg);
                    matches!(ty, JuliaType::Struct(_) | JuliaType::Any)
                });
                // Handle math functions that return F64 (JuliaType-level inference).
                // IMPORTANT: This list is a type inference hint, independent of whether the
                // function is a Rust builtin or Pure Julia. Do NOT remove entries during
                // builtin migration. See Issue #2634 and docs/vm/BUILTIN_REMOVAL.md Layer 5.
                // Only apply when arguments are primitive types (not struct/Any)
                if !has_struct_or_any_arg {
                    match function.as_str() {
                        "floor" | "ceil" | "round" | "trunc" |
                        "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" |
                        "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" |
                        "exp" | "log" | "log2" | "log10" | "log1p" | "expm1" |
                        // Note: abs, abs2, sign are NOT here - they return the same type as input
                        // (e.g., abs(x::Int64) → Int64, abs(x::Float64) → Float64)
                        // Note: sum is now Pure Julia (preserves element type)
                        "prod" | "max" | "min" | "mean" | "std" | "var" => {
                            return JuliaType::Float64;
                        }
                        "isequal" | "isless" | "iseven" | "isodd" | "isnan" | "isinf" |
                        "isfinite" | "isnothing" | "ismissing" | "haskey" | "isa" |
                        "startswith" | "endswith" | "occursin" | "contains" => {
                            return JuliaType::Bool;
                        }
                        "string" | "repr" | "uppercase" | "lowercase" | "strip" |
                        "lstrip" | "rstrip" | "chomp" | "chop" | "join" |
                        "take!" | "takestring!" | "sprint" |
                        "lowercasefirst" | "uppercasefirst" | "escape_string" |
                        "chopprefix" | "chopsuffix" | "replace" |
                        "lpad" | "rpad" | "repeat" | "bitstring" | "ascii" | "unescape_string" => {
                        // bytes2hex removed - now Pure Julia (base/strings/util.jl)
                            return JuliaType::String;
                        }
                        // reverse: returns same type as input (String for string, Array for array)
                        "reverse" => {
                            if let Some(arg) = args.first() {
                                let arg_type = self.infer_julia_type(arg);
                                match arg_type {
                                    JuliaType::String => return JuliaType::String,
                                    JuliaType::Array => return JuliaType::Array,
                                    _ => return arg_type,
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Handle builtin functions that always return their type regardless of args
                match function.as_str() {
                    // Integer-returning functions
                    "length" | "size" | "ndims" | "count" | "hash" |
                    "fld" | "cld" | "div" | "mod" | "rem" |
                    // Date/time accessor functions that return integers
                    "year" | "month" | "day" | "hour" | "minute" | "second" |
                    "dayofweek" | "dayofyear" | "week" | "days" |
                    "Int64" | "Int" => {
                        return JuliaType::Int64;
                    }
                    "Int32" => {
                        return JuliaType::Int32;
                    }
                    "Int16" => {
                        return JuliaType::Int16;
                    }
                    "Int8" => {
                        return JuliaType::Int8;
                    }
                    "Int128" => {
                        return JuliaType::Int128;
                    }
                    // Unsigned integer constructors
                    "UInt8" => {
                        return JuliaType::UInt8;
                    }
                    "UInt16" => {
                        return JuliaType::UInt16;
                    }
                    "UInt32" => {
                        return JuliaType::UInt32;
                    }
                    "UInt64" => {
                        return JuliaType::UInt64;
                    }
                    "UInt128" => {
                        return JuliaType::UInt128;
                    }
                    // Float32 conversion - Issue #1759
                    "Float32" => {
                        return JuliaType::Float32;
                    }
                    // Float64 conversion
                    "Float64" => {
                        return JuliaType::Float64;
                    }
                    // BigInt constructor
                    "BigInt" => {
                        return JuliaType::BigInt;
                    }
                    // BigFloat constructor
                    "BigFloat" => {
                        return JuliaType::BigFloat;
                    }
                    // big() function - converts to BigInt or BigFloat depending on argument
                    // (Issue #1910)
                    "big" => {
                        if let Some(arg) = args.first() {
                            let arg_type = self.infer_julia_type(arg);
                            return match arg_type {
                                JuliaType::Float32 | JuliaType::Float64 => JuliaType::BigFloat,
                                _ => JuliaType::BigInt,
                            };
                        }
                        return JuliaType::BigInt; // Default to BigInt
                    }
                    "zeros" | "ones" | "fill" => {
                        return JuliaType::Array;
                    }
                    "trues" | "falses" => {
                        // trues/falses return Vector{Bool}
                        return JuliaType::VectorOf(Box::new(JuliaType::Bool));
                    }
                    "collect" => {
                        // Infer element type from argument for proper Vector{T} dispatch
                        if !args.is_empty() {
                            let arg_type = self.infer_julia_type(&args[0]);
                            match arg_type {
                                // Ranges produce Int64 elements
                                JuliaType::UnitRange | JuliaType::StepRange => {
                                    return JuliaType::VectorOf(Box::new(JuliaType::Int64));
                                }
                                // Preserve VectorOf type (e.g., collect on generator with known type)
                                JuliaType::VectorOf(elem) => {
                                    return JuliaType::VectorOf(elem);
                                }
                                _ => {}
                            }
                        }
                        return JuliaType::Array;
                    }
                    "rand" | "randn" => {
                        if args.is_empty() {
                            return JuliaType::Float64;
                        } else {
                            return JuliaType::Array;
                        }
                    }
                    // abs, abs2, sign preserve argument type for primitive types
                    "abs" | "abs2" | "sign" => {
                        if !args.is_empty() {
                            let arg_type = self.infer_julia_type(&args[0]);
                            // For builtin numeric types, preserve the type
                            if arg_type.is_builtin_numeric() {
                                return arg_type;
                            }
                            // For struct types (e.g., Complex), fall through to method dispatch
                        }
                    }
                    _ => {}
                }
                // Dict() with empty/pair args returns JuliaType::Dict (builtin), not Struct("Dict")
                // Even though mutable struct Dict{K,V} exists, the compiler intercepts
                // builtin patterns and emits NewDict. Type inference must match. (Issue #2748)
                if function == "Dict" || function.starts_with("Dict{") {
                    let is_builtin_pattern = args.is_empty()
                        || args.iter().all(|a| matches!(a, Expr::Pair { .. }))
                        || args.len() == 1
                            && matches!(
                                &args[0],
                                Expr::Comprehension { .. } | Expr::Generator { .. }
                            );
                    if is_builtin_pattern {
                        return JuliaType::Dict;
                    }
                    // Non-builtin pattern: fall through to struct constructor
                }
                // Check if this is a struct constructor call
                // Use resolve_struct_name to handle module-qualified names (e.g., Month -> Dates.Month)
                if let Some(resolved_name) = self.resolve_struct_name(function) {
                    JuliaType::Struct(resolved_name)
                } else if let Some(resolved_name) = self.resolve_parametric_struct_name(function) {
                    // Parametric struct - infer type parameters from arguments
                    // e.g., Point(1, 2) -> MyGeometry.Point{Int64} (if Point is from MyGeometry)
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    if let Ok(type_args) = self.shared_ctx.infer_type_args(function, &arg_types) {
                        if !type_args.is_empty() {
                            let type_arg_names: Vec<String> =
                                type_args.iter().map(|t| t.name().to_string()).collect();
                            // Use resolved (potentially qualified) name for method dispatch
                            return JuliaType::Struct(format!(
                                "{}{{{}}}",
                                resolved_name,
                                type_arg_names.join(", ")
                            ));
                        }
                    }
                    JuliaType::Struct(resolved_name)
                } else if function == "typeof"
                    || function == "promote_type"
                    || function == "promote_rule"
                    || function == "eltype"
                    || function == "keytype"
                    || function == "valtype"
                {
                    // Type-returning functions always return DataType
                    JuliaType::DataType
                } else if function == "enumerate" {
                    // enumerate(iter) returns Enumerate{typeof(iter)}
                    // Use Enumerate{Any} since we don't track concrete inner type
                    JuliaType::Struct("Enumerate{Any}".to_string())
                } else if function == "zip" {
                    // zip returns Zip/Zip3/Zip4 depending on arity (Issue #1990)
                    match args.len() {
                        3 => JuliaType::Struct("Zip3{Any, Any, Any}".to_string()),
                        4 => JuliaType::Struct("Zip4{Any, Any, Any, Any}".to_string()),
                        _ => JuliaType::Struct("Zip{Any, Any}".to_string()),
                    }
                } else if function == "take" {
                    // take(iter, n) returns Take{typeof(iter)}
                    // Use Take{Any} since we don't track concrete inner type
                    JuliaType::Struct("Take{Any}".to_string())
                } else if function == "drop" {
                    // drop(iter, n) returns Drop{typeof(iter)}
                    // Use Drop{Any} since we don't track concrete inner type
                    JuliaType::Struct("Drop{Any}".to_string())
                } else if function == "iterate" {
                    // iterate(collection) and iterate(collection, state) return (element, state) or nothing
                    // For type inference purposes, treat as Tuple to enable proper tuple indexing (y[2])
                    // This is safe because code should check `y === nothing` before accessing y[2]
                    JuliaType::Tuple
                } else if function.contains('{') {
                    // Handle parametric struct constructors like Val{1}(), Val{2}(), Point{Int64}(), etc.
                    // The function name includes the type parameters (e.g., "Val{2}")
                    // Return the full parametric type name for proper method dispatch
                    let base_name = &function[..function.find('{').unwrap()];
                    if self.shared_ctx.parametric_structs.contains_key(base_name) {
                        // This is a parametric struct instantiation - use the full name as type
                        JuliaType::Struct(function.clone())
                    } else {
                        JuliaType::Any
                    }
                } else if let Some(table) = self.method_tables.get(function) {
                    // Check method table for return type
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|arg| self.infer_julia_type(arg)).collect();
                    if let Ok(method) = table.dispatch(&arg_types) {
                        // Prefer parametric return type (e.g., TupleOf) over lossy ValueType (Issue #2317)
                        if let Some(ref jt) = method.return_julia_type {
                            return jt.clone();
                        }
                        return self.value_type_to_julia_type(&method.return_type);
                    }
                    JuliaType::Any
                } else {
                    JuliaType::Any
                }
            }
            Expr::FieldAccess { object, field, .. } => {
                // Infer the type of the object and look up the field type
                let obj_type = self.infer_julia_type(object);
                if let JuliaType::Struct(struct_name) = obj_type {
                    // Look up the struct definition and find the field type
                    // First try exact name, then try base name for parametric types
                    let struct_info =
                        self.shared_ctx.struct_table.get(&struct_name).or_else(|| {
                            // Try base name for parametric types like "Complex{Float64}"
                            if let Some(brace_idx) = struct_name.find('{') {
                                let base_name = &struct_name[..brace_idx];
                                self.shared_ctx.struct_table.get(base_name)
                            } else {
                                None
                            }
                        });
                    if let Some(info) = struct_info {
                        for (field_name, field_ty) in &info.fields {
                            if field_name == field {
                                return match field_ty {
                                    ValueType::I64 => JuliaType::Int64,
                                    ValueType::F64 => JuliaType::Float64,
                                    ValueType::Str => JuliaType::String,
                                    ValueType::Array => JuliaType::Array,
                                    ValueType::Struct(tid) => {
                                        // Look up struct name (handles all structs including Complex)
                                        self.shared_ctx
                                            .get_struct_name(*tid)
                                            .map(JuliaType::Struct)
                                            .unwrap_or(JuliaType::Any)
                                    }
                                    _ => JuliaType::Any,
                                };
                            }
                        }
                    }
                }
                JuliaType::Any
            }
            Expr::UnaryOp { op, operand, .. } => {
                match op {
                    UnaryOp::Not => JuliaType::Bool,     // ! always returns Bool
                    _ => self.infer_julia_type(operand), // Neg, Pos preserve operand type
                }
            }
            Expr::Builtin { name, args, .. } => {
                // Infer JuliaType for builtin operations
                match name {
                    BuiltinOp::TypeOf | BuiltinOp::Supertype => JuliaType::DataType,
                    BuiltinOp::Isa
                    | BuiltinOp::HasKey
                    | BuiltinOp::Isbits
                    | BuiltinOp::Isbitstype
                    | BuiltinOp::Hasfield
                    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                    // removed - now Pure Julia (base/reflection.jl)
                    | BuiltinOp::Ismutable => JuliaType::Bool,
                    BuiltinOp::Length
                    | BuiltinOp::TimeNs
                    | BuiltinOp::DictGet
                    | BuiltinOp::Sizeof => JuliaType::Int64,
                    BuiltinOp::Rand | BuiltinOp::Randn => JuliaType::Float64,
                    BuiltinOp::Sqrt => JuliaType::Float64,
                    BuiltinOp::Zeros
                    | BuiltinOp::Ones => JuliaType::Array,
                    // Note: Adjoint and Transpose are now Pure Julia
                    BuiltinOp::Lu => JuliaType::Tuple,
                    BuiltinOp::Det => JuliaType::Float64,
                    // Note: Inv removed — dead code (Issue #2643)
                    // IfElse/ternary: preserve parametric type if both branches return
                    // the same type (Issue #2319). This enables `t = if c; (1,2) else (3,4) end`
                    // to track the TupleOf type for dispatch.
                    BuiltinOp::IfElse => {
                        if args.len() >= 3 {
                            let then_ty = self.infer_julia_type(&args[1]);
                            let else_ty = self.infer_julia_type(&args[2]);
                            // If both branches return the same parametric type, preserve it
                            if then_ty == else_ty {
                                then_ty
                            } else {
                                // Different types: compute common supertype
                                // For now, if either is parametric (TupleOf, VectorOf, etc.),
                                // fall back to the base type; otherwise Any
                                match (&then_ty, &else_ty) {
                                    (JuliaType::TupleOf(_), JuliaType::TupleOf(_)) => {
                                        JuliaType::Tuple
                                    }
                                    (JuliaType::VectorOf(_), JuliaType::VectorOf(_)) => {
                                        JuliaType::Array
                                    }
                                    (JuliaType::MatrixOf(_), JuliaType::MatrixOf(_)) => {
                                        JuliaType::Array
                                    }
                                    _ => JuliaType::Any,
                                }
                            }
                        } else {
                            JuliaType::Any
                        }
                    }
                    _ => JuliaType::Any,
                }
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                ..
            } => {
                // Module-qualified function call: Module.func(args)
                // Look up the method table for this function and infer return type
                if let Some(table) = self.method_tables.get(function.as_str()) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    if let Ok(method) = table.dispatch(&arg_types) {
                        return self.value_type_to_julia_type(&method.return_type);
                    }
                }
                // Fallback: check module_functions mapping
                let resolved_module = self
                    .module_aliases
                    .get(module.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or(module.as_str());
                if self
                    .module_functions
                    .get(resolved_module)
                    .map(|fs| fs.contains(function.as_str()))
                    .unwrap_or(false)
                {
                    // Known module function but couldn't determine return type
                    JuliaType::Any
                } else {
                    JuliaType::Any
                }
            }
            // Function types - Issue #1658: FunctionRef must return Function type
            // to enable proper dispatch for HOFs like map(f::Function, A::Array)
            Expr::FunctionRef { .. } => JuliaType::Function,
            // Ternary expression (cond ? then_expr : else_expr) - Issue #2319
            // Preserve parametric type if both branches return the same type
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let then_ty = self.infer_julia_type(then_expr);
                let else_ty = self.infer_julia_type(else_expr);
                // If both branches return the same parametric type, preserve it
                if then_ty == else_ty {
                    then_ty
                } else {
                    // Different types: compute common supertype
                    // For parametric types, fall back to base type
                    match (&then_ty, &else_ty) {
                        (JuliaType::TupleOf(_), JuliaType::TupleOf(_)) => JuliaType::Tuple,
                        (JuliaType::VectorOf(_), JuliaType::VectorOf(_)) => JuliaType::Array,
                        (JuliaType::MatrixOf(_), JuliaType::MatrixOf(_)) => JuliaType::Array,
                        _ => JuliaType::Any,
                    }
                }
            }
            // LetBlock (begin...end, let...end) - infer from last statement
            Expr::LetBlock { body, bindings, .. } => {
                // Detect partial-apply closure pattern (Issue #3119):
                // [FunctionDef("__partial_apply_N"), Expr(Var("__partial_apply_N"))]
                // This LetBlock is produced by lower_operator_partial_apply_as_nested when
                // `==(val)` appears inside a function body (no LambdaContext available).
                // The nested FunctionDef may not yet be in method_tables at this point
                // (compiled after its parent), so we detect the pattern structurally.
                if bindings.is_empty() && body.stmts.len() == 2 {
                    if let (
                        crate::ir::core::Stmt::FunctionDef { func, .. },
                        Stmt::Expr {
                            expr: Expr::Var(var_name, _),
                            ..
                        },
                    ) = (&body.stmts[0], &body.stmts[1])
                    {
                        if func.name == *var_name
                            && var_name.starts_with("__partial_apply_")
                        {
                            return JuliaType::Function;
                        }
                    }
                }
                if let Some(Stmt::Expr { expr, .. }) = body.stmts.last() {
                    // If the last statement is an expression, infer its type
                    return self.infer_julia_type(expr);
                }
                JuliaType::Nothing
            }
            _ => JuliaType::Any,
        }
    }

    /// Convert a ValueType to JuliaType for method dispatch.
    pub(in crate::compile) fn value_type_to_julia_type(&self, vt: &ValueType) -> JuliaType {
        match vt {
            ValueType::I64 => JuliaType::Int64,
            ValueType::F64 => JuliaType::Float64,
            ValueType::Str => JuliaType::String,
            ValueType::Array => JuliaType::Array,
            ValueType::Nothing => JuliaType::Nothing,
            ValueType::Missing => JuliaType::Missing,
            ValueType::Struct(type_id) => {
                // Look up struct name (handles all structs including Complex)
                self.shared_ctx
                    .get_struct_name(*type_id)
                    .map(JuliaType::Struct)
                    .unwrap_or(JuliaType::Any)
            }
            ValueType::Tuple => JuliaType::Tuple,
            ValueType::NamedTuple => JuliaType::NamedTuple,
            ValueType::Dict => JuliaType::Dict,
            ValueType::Set => JuliaType::Set,
            ValueType::Range => JuliaType::UnitRange,
            ValueType::Generator => JuliaType::Any,
            ValueType::Char => JuliaType::Char,
            ValueType::DataType => JuliaType::DataType,
            ValueType::Module => JuliaType::Module,
            ValueType::Rng | ValueType::Any => JuliaType::Any,
            ValueType::BigInt => JuliaType::BigInt,
            ValueType::BigFloat => JuliaType::BigFloat,
            ValueType::IO => JuliaType::IO,
            // New numeric types
            ValueType::I8 => JuliaType::Int8,
            ValueType::I16 => JuliaType::Int16,
            ValueType::I32 => JuliaType::Int32,
            ValueType::I128 => JuliaType::Int128,
            ValueType::U8 => JuliaType::UInt8,
            ValueType::U16 => JuliaType::UInt16,
            ValueType::U32 => JuliaType::UInt32,
            ValueType::U64 => JuliaType::UInt64,
            ValueType::U128 => JuliaType::UInt128,
            ValueType::F16 => JuliaType::Float16,
            ValueType::F32 => JuliaType::Float32,
            ValueType::Bool => JuliaType::Bool,
            // ArrayOf maps to Array for dispatch (element type tracked separately)
            ValueType::ArrayOf(_) => JuliaType::Array,
            // Macro system types
            ValueType::Symbol => JuliaType::Symbol,
            ValueType::Expr => JuliaType::Expr,
            ValueType::QuoteNode => JuliaType::QuoteNode,
            ValueType::LineNumberNode => JuliaType::LineNumberNode,
            ValueType::GlobalRef => JuliaType::GlobalRef,
            // Pairs type (for kwargs...)
            ValueType::Pairs => JuliaType::Pairs,
            // Function type
            ValueType::Function => JuliaType::Function,
            // Regex types
            ValueType::Regex => JuliaType::Struct("Regex".to_string()),
            ValueType::RegexMatch => JuliaType::Struct("RegexMatch".to_string()),
            // Enum type
            ValueType::Enum => JuliaType::Any,
            // Union type
            ValueType::Union(_) => JuliaType::Any,
            // Memory type
            ValueType::Memory | ValueType::MemoryOf(_) => JuliaType::Any,
        }
    }
}
