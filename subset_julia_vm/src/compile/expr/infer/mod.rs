//! Type inference for expression compilation.
//!
//! Handles inference of:
//! - Expression types (ValueType)
//! - Julia types for method dispatch (JuliaType)
//! - Array element types

mod array;
mod expr_tfuncs;
mod hof;
mod julia_type;
mod shared;

pub(crate) use array::{infer_array_element_type, infer_nested_array_literal_element_type};

use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::types::JuliaType;
use crate::vm::{ArrayElementType, ValueType};

use crate::compile::inference::promote_numeric_value_types;
use crate::compile::type_helpers::short_circuit_result_type;
use crate::compile::{
    binary_op_to_function_name, is_base_function, is_euler_name, is_math_constant, is_pi_name,
    is_random_function, CoreCompiler,
};

fn is_broadcasted_comparison_call(expr: Option<&Expr>) -> bool {
    let Some(Expr::Call { function, args, .. }) = expr else {
        return false;
    };
    if function != "Broadcasted" {
        return false;
    }
    let Some(Expr::FunctionRef { name, .. }) = args.first() else {
        return false;
    };
    matches!(name.as_str(), "<" | ">" | "<=" | ">=" | "==" | "!=")
}

fn array_literal_element_ranks(elements: &[Expr]) -> Option<Vec<usize>> {
    elements
        .iter()
        .map(|element| match element {
            Expr::ArrayLiteral { shape, .. } => Some(shape.len()),
            _ => None,
        })
        .collect()
}

impl CoreCompiler<'_> {
    pub(in crate::compile) fn infer_view_call_return_type(
        &mut self,
        function: &str,
        args: &[Expr],
        julia_args: &[JuliaType],
    ) -> Option<ValueType> {
        let value_args = args
            .iter()
            .map(|arg| self.infer_expr_type(arg))
            .collect::<Vec<_>>();
        let mut inst = expr_tfuncs::SharedCtxInstantiation(&mut *self.shared_ctx);
        expr_tfuncs::infer_value_view_call(function, &value_args, julia_args, &mut inst)
    }

    /// Resolve the call-site return type of a parametric struct constructor when
    /// the program defines a *user-written outer constructor* (a method named
    /// after the struct, with its own body in `function_ir_by_global_index`).
    ///
    /// Such a constructor can transform its arguments before instantiating the
    /// struct (e.g. `Foo(x::Real) = (v = float(x); Foo{typeof(v)}(v))`), so the
    /// concrete type parameters do not follow from the raw argument types. The
    /// naive type-arg inference (`infer_value_parametric_struct_ctor`) models the
    /// *default inner* constructor, which binds the type parameters directly from
    /// the argument types; using it here mis-typed the field load (Issue #7284:
    /// "expected I64, got Float64", because `float()` widened the field at
    /// runtime). Re-inferring through the user constructor body yields the
    /// correct concrete struct (or `Any` when the body is too dynamic to resolve,
    /// which still keeps field access dynamic and correct).
    ///
    /// Returns `None` when there is no matching user-defined method (e.g. the
    /// struct is constructed only via its default constructor), so the caller
    /// keeps the legacy precise type-arg inference path.
    fn infer_user_outer_constructor_return_type(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> Option<ValueType> {
        let table = self.method_tables.get(function)?;
        let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
        let method = table.dispatch(&arg_types).ok()?;
        // Only a user-written constructor (one whose body IR is recorded) can
        // transform its arguments; the synthesized default constructors have no
        // IR here, so they keep the precise type-arg inference path.
        let func_ir = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&method.global_index)?
            .clone();
        // A precise declared/inferred return type on the method already accounts
        // for the constructor body (the abstract-interpretation engine ran over
        // it), so prefer it directly.
        if !matches!(method.return_type, ValueType::Any) {
            return Some(method.return_type.clone());
        }
        // Otherwise re-infer through the body with the concrete argument value
        // types so a `Foo{typeof(float(x))}(...)` tail resolves to the promoted
        // instantiation. Mirrors the non-parametric user-function call path.
        let arg_value_types: Vec<ValueType> =
            args.iter().map(|arg| self.infer_expr_type(arg)).collect();
        if arg_value_types
            .iter()
            .any(|ty| matches!(ty, ValueType::Any))
        {
            // Dynamic arguments: a user constructor transforms them in ways the
            // body re-inference cannot resolve precisely. Widen to `Any` rather
            // than risk a wrong concrete field type.
            return Some(ValueType::Any);
        }
        let inferred =
            self.infer_shared_function_return_type_with_arg_types(&func_ir, &arg_value_types);
        // Even an `Any` result is intentional here: it means the user
        // constructor's field types are not statically known, so field access
        // must stay dynamic (correct), unlike the naive type-arg model.
        Some(inferred)
    }

    pub(in crate::compile) fn infer_zeros_ones_array_type(&mut self, args: &[Expr]) -> ValueType {
        let value_args = args
            .iter()
            .map(|arg| self.infer_expr_type(arg))
            .collect::<Vec<_>>();
        let julia_args = args
            .iter()
            .map(|arg| self.infer_julia_type(arg))
            .collect::<Vec<_>>();
        if let Some(inferred) = expr_tfuncs::infer_value_array_constructor_call(
            "zeros",
            args,
            &value_args,
            &julia_args,
            |name| self.shared_ctx.get_struct_type_id(name),
        ) {
            return inferred;
        }

        let element_type = args
            .first()
            .and_then(|arg| match self.infer_julia_type(arg) {
                JuliaType::TypeOf(inner) => {
                    shared::array_element_type_for_julia_type(&inner, |name| {
                        self.shared_ctx.get_struct_type_id(name)
                    })
                }
                _ => None,
            })
            .unwrap_or(ArrayElementType::F64);

        ValueType::ArrayOf(element_type, None)
    }

    /// Sharpened element `ValueType` for `array[idx]` when `array` has a
    /// statically known tuple/`NamedTuple` type and `idx` is a constant integer
    /// literal in range (Issue #5183). Returns `None` to defer to the existing
    /// dynamic (`Any`) path.
    pub(in crate::compile) fn tuple_const_index_value_type(
        &self,
        array: &Expr,
        idx: &Expr,
    ) -> Option<ValueType> {
        let k = shared::const_tuple_index(idx)?;
        let container = self.infer_julia_type(array);
        let elem = shared::tuple_element_julia_type(&container, k)?;
        Some(self.julia_type_to_value_type_resolved(&elem))
    }

    /// Sharpened element `ValueType` for `first(t)`/`last(t)` over a statically
    /// known tuple type (Issue #5183), or `None` to defer to the dynamic path.
    fn tuple_first_last_value_type(&self, arg: &Expr, last: bool) -> Option<ValueType> {
        let elem = self.tuple_first_last_julia_type(arg, last)?;
        Some(self.julia_type_to_value_type_resolved(&elem))
    }

    /// Sharpened element `JuliaType` for `first(t)`/`last(t)` over a statically
    /// known tuple type (Issue #5183), or `None` to defer to the dynamic path.
    fn tuple_first_last_julia_type(&self, arg: &Expr, last: bool) -> Option<JuliaType> {
        let container = self.infer_julia_type(arg);
        let JuliaType::TupleOf(elem_types) = &container else {
            return None;
        };
        // A `Vararg{T}` tail leaves the arity unfixed, so `last` is unsound.
        if let Some(tail) = elem_types.last() {
            if crate::types::unbounded_vararg_element(tail).is_some() {
                return None;
            }
        }
        if last {
            elem_types.last().cloned()
        } else {
            elem_types.first().cloned()
        }
    }

    /// Convert a `JuliaType` to a `ValueType`, resolving user-struct names via the
    /// shared struct table so concrete tuple/named-tuple fields keep their struct
    /// identity (Issue #5183).
    pub(in crate::compile) fn julia_type_to_value_type_resolved(
        &self,
        jt: &JuliaType,
    ) -> ValueType {
        crate::compile::type_helpers::julia_type_to_value_type_with_table(
            jt,
            &self.shared_ctx.struct_table,
        )
    }

    pub(in crate::compile) fn infer_expr_type(&mut self, expr: &Expr) -> ValueType {
        let _complex_id = self.get_struct_type_id("Complex").unwrap_or(0);
        match expr {
            Expr::Literal(lit, _) => {
                if let Some(inferred) = shared::infer_scalar_literal(lit) {
                    return inferred.value_type();
                }
                match lit {
                    Literal::Array(_, _) => ValueType::ArrayOf(ArrayElementType::F64, None),
                    Literal::ArrayI64(_, _) => ValueType::ArrayOf(ArrayElementType::I64, None),
                    Literal::ArrayBool(_, _) => ValueType::ArrayOf(ArrayElementType::Bool, None),
                    Literal::Struct(type_name, _) => {
                        // Look up struct type_id from struct_table
                        if let Some(struct_info) = self.shared_ctx.struct_table.get(type_name) {
                            ValueType::Struct(struct_info.type_id)
                        } else {
                            ValueType::Any
                        }
                    }
                    Literal::DataType(_) => ValueType::DataType,
                    Literal::Undef => ValueType::Any, // Required kwarg marker
                    // Metaprogramming literals
                    Literal::Symbol(_) => ValueType::Symbol,
                    Literal::Expr { .. } => ValueType::Any,
                    Literal::QuoteNode(_) => ValueType::Any,
                    Literal::LineNumberNode { .. } => ValueType::Any,
                    // Regex literal
                    Literal::Regex { .. } => ValueType::Regex,
                    // Enum literal
                    Literal::Enum { .. } => ValueType::Enum,
                    // Scalar literals are handled by infer_scalar_literal above.
                    Literal::Int(_)
                    | Literal::Int128(_)
                    | Literal::BigInt(_)
                    | Literal::BigFloat(_)
                    | Literal::Bool(_)
                    | Literal::Float(_)
                    | Literal::Float32(_)
                    | Literal::Float16(_)
                    | Literal::Str(_)
                    | Literal::Char(_)
                    | Literal::Nothing
                    | Literal::Missing
                    | Literal::Module(_) => {
                        unreachable!("scalar literal inference should handle {lit:?}")
                    }
                }
            }
            Expr::Var(name, _) => {
                if name == "nothing" && !self.locals.contains_key(name) {
                    return ValueType::Nothing;
                }

                if self.declared_globals.contains(name) {
                    return ValueType::Any;
                }

                // Check if it's a known constant before falling back to locals
                if !self.locals.contains_key(name) {
                    // Check for built-in irrational singletons before the
                    // float-literal constants so compile-time equality dispatch
                    // stays aligned with compile_expr's NewStruct emission
                    // (Issue #8481).
                    if is_pi_name(name) || is_euler_name(name) {
                        let struct_name = if is_pi_name(name) {
                            "Irrational{:π}"
                        } else {
                            "Irrational{:ℯ}"
                        };
                        if let Some(struct_info) = self.shared_ctx.struct_table.get(struct_name) {
                            return ValueType::Struct(struct_info.type_id);
                        }
                        return ValueType::F64;
                    }
                    // Check for NaN and Inf (Float64)
                    if name == "NaN" || name == "Inf" || name == "NaN64" || name == "Inf64" {
                        return ValueType::F64;
                    }
                    // Check for NaN32 and Inf32 (Float32)
                    if name == "NaN32" || name == "Inf32" {
                        return ValueType::F32;
                    }
                    // Check for ENDIAN_BOM (Int32 value for byte order detection)
                    if name == "ENDIAN_BOM" {
                        return ValueType::I64;
                    }
                    // Standard IO streams resolve to IO so callers like
                    // `print(io, ...)` / `println(io, ...)` can route output
                    // to the correct sink at compile time (Issue #3573).
                    if name == "stdout" || name == "stderr" || name == "stdin" || name == "devnull"
                    {
                        return ValueType::IO;
                    }
                    // Check for MathConstants when imported via `using Base.MathConstants`
                    if self.usings.contains("Base.MathConstants") && is_math_constant(name) {
                        return ValueType::F64;
                    }
                    if self.resolve_visible_type_object_name(name).is_some() {
                        return ValueType::DataType;
                    }
                }
                // Issue #3622: typed parameters with narrow integer widths
                // (Int8/16/32/128, UInt8/16/32/64/128) collapse to ValueType::I64
                // in `compiler.locals` because julia_type_to_value_type maps them
                // all to I64. Consult julia_type_locals — populated for narrow
                // integer params in compile/mod.rs — so that `a + b` for two
                // typed UInt8 params infers UInt8 instead of I64 and the existing
                // small-int back-conversion / I128 / U128 early-routes fire.
                // Bool is excluded because Julia's `Bool + Bool` returns Int64.
                if let Some(precise) = self
                    .julia_type_locals
                    .get(name)
                    .and_then(narrow_int_value_type)
                {
                    return precise;
                }
                // Bare abstract-numeric params (`x::Real`, `x::Number`, `x::Integer`, ...)
                // must report `Any` here, mirroring the `infer_julia_type` /
                // `compile_var` (LoadAny) handling (Issue #5076 / #5169). The
                // annotation widens `x` to `ValueType::F64` (Real/Number) or
                // `ValueType::I64` (Integer) in `self.locals`, but the variable is
                // always loaded via `LoadAny`, so the concrete runtime type is the
                // argument's actual type. When such a param is *forwarded* to
                // another user function (`f(x::Real) = g(x)`), the call-site return
                // inference re-runs the callee's body with the caller's argument
                // ValueType (`compile_call`'s speculative shared-engine
                // call-site inference). Reporting `F64`
                // there made `g(y)=zero(y)` re-infer as `zero(::Float64)` → `F64`,
                // so `f` coerced the runtime `Int64` result of `g(x)` to `Float64`
                // on return. Reporting `Any` keeps the forwarded arg type
                // consistent with the runtime representation, so the speculative
                // re-inference is skipped and the type-generic callee dispatches on
                // the concrete value at runtime, matching upstream Julia 1.12
                // (Issue #5167 part 2).
                if self.abstract_numeric_params.contains(name) {
                    return ValueType::Any;
                }
                // Check locals first, then global_types
                // Default to Any (not I64) to ensure dynamic dispatch for unknown types
                self.locals
                    .get(name)
                    .cloned()
                    .or_else(|| self.shared_ctx.global_types.get(name).cloned())
                    .unwrap_or(ValueType::Any)
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let lt = self.infer_expr_type(left);
                let rt = self.infer_expr_type(right);

                // Check for user-defined operators if either operand is a Struct
                if matches!(lt, ValueType::Struct(_)) || matches!(rt, ValueType::Struct(_)) {
                    // Infer Julia types for method dispatch
                    let left_julia_ty = self.infer_julia_type(left);
                    let right_julia_ty = self.infer_julia_type(right);
                    let op_name = binary_op_to_function_name(op);
                    if let Some(table) = self.method_tables.get(op_name) {
                        let arg_types = vec![left_julia_ty, right_julia_ty];
                        if let Ok(method) = table.dispatch(&arg_types) {
                            if method.return_type != ValueType::Any {
                                return method.return_type.clone();
                            }
                            // Dispatch succeeded but the matched Base method
                            // declares an `Any` return type (the `*`/`+` numeric
                            // operators do). Fall through to the Complex-promotion
                            // recovery below instead of losing the element type.
                        }
                    }
                    // The JuliaType inference path applies Julia's Complex
                    // promotion rules (`Real op Complex{T} -> Complex{promote(...)}`)
                    // as a fallback when operator dispatch yields `Any`. Mirror
                    // that here so a type-annotation-free literal element such as
                    // `1.0 + 2.0im` infers `Complex{Float64}` instead of `Any`,
                    // which kept `[1.0 + 2.0im, ...]` from storing as
                    // `Vector{ComplexF64}` (Issue #6851).
                    let promoted = self.infer_julia_type(expr);
                    if matches!(&promoted, JuliaType::Struct(name) if name.starts_with("Complex")) {
                        let promoted_vt = self.julia_type_to_value_type_with_ctx(&promoted);
                        if !matches!(promoted_vt, ValueType::Any) {
                            return promoted_vt;
                        }
                    }
                    // Method dispatch failed but struct operand involved.
                    // Return Any to enable runtime dispatch (fixes Issue #1055).
                    // Comparison operators still return Bool regardless.
                    return match op {
                        BinaryOp::Lt
                        | BinaryOp::Gt
                        | BinaryOp::Le
                        | BinaryOp::Ge
                        | BinaryOp::Eq
                        | BinaryOp::Ne => ValueType::Bool,
                        // `&&`/`||` yield the right operand's value (or a Bool),
                        // not always Bool (Issue #6278).
                        BinaryOp::And | BinaryOp::Or => short_circuit_result_type(rt),
                        _ => ValueType::Any,
                    };
                }

                // Fallback for primitive types (no struct operands)
                // Check if either operand is Any (e.g., untyped function parameter)
                let has_any = lt == ValueType::Any || rt == ValueType::Any;

                // `==`/`!=` over runtime-unknown operands cannot be assumed
                // Bool when the user program defines an equality method with a
                // non-Bool return type (legal in Julia): `bb[1] == bb[2]` with
                // a user `==(::Box, ::Box) = "box-any"` returns a String at
                // runtime, and the unconditional Bool here made the inline
                // `(a == b) == "box-any"` comparison constant-fold to `false`
                // at the String-vs-non-String equality shortcut (Issue #6539).
                // Base methods are excluded (`function_ir_by_global_index`
                // holds only non-Base/non-stdlib functions), so plain Base
                // equality keeps its Bool inference.
                if has_any && matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                    let op_name = binary_op_to_function_name(op);
                    let user_non_bool_method =
                        self.method_tables.get(op_name).is_some_and(|table| {
                            table.methods.iter().any(|m| {
                                m.param_count() == 2
                                    && m.return_type != ValueType::Bool
                                    && self
                                        .shared_ctx
                                        .function_ir_by_global_index
                                        .contains_key(&m.global_index)
                            })
                        });
                    if user_non_bool_method {
                        return ValueType::Any;
                    }
                }

                match op {
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    // `&&`/`||` yield the right operand's value (or a Bool), not
                    // always Bool, so an inline `(a && b) == lit` comparison sees
                    // the correct left type (Issue #6278).
                    BinaryOp::And | BinaryOp::Or => short_circuit_result_type(rt),
                    BinaryOp::Div => ValueType::F64,
                    BinaryOp::Pow => {
                        // Power operator: String^Int -> Str (repeat), Int^Int -> Int, otherwise -> Float64 (Julia semantics)
                        if lt == ValueType::Str {
                            // String ^ Int returns String (via repeat function)
                            ValueType::Str
                        } else if lt == ValueType::I64 && rt == ValueType::I64 {
                            ValueType::I64
                        } else if has_any {
                            ValueType::Any
                        } else {
                            ValueType::F64
                        }
                    }
                    _ => {
                        // Julia * only concatenates String/Char with String/Char (Issue #3465)
                        if matches!(op, BinaryOp::Mul)
                            && matches!(lt, ValueType::Str | ValueType::Char)
                            && matches!(rt, ValueType::Str | ValueType::Char)
                        {
                            return ValueType::Str;
                        }
                        // Array arithmetic: Array +/- Array returns Array
                        let left_is_array =
                            matches!(lt, ValueType::Array | ValueType::ArrayOf(_, _));
                        let right_is_array =
                            matches!(rt, ValueType::Array | ValueType::ArrayOf(_, _));
                        if left_is_array || right_is_array {
                            if matches!(op, BinaryOp::Mul) && left_is_array && right_is_array {
                                return ValueType::Array;
                            }
                            // For array operations, try to preserve element type
                            match (&lt, &rt) {
                                (ValueType::ArrayOf(elem, _), _)
                                | (_, ValueType::ArrayOf(elem, _)) => {
                                    ValueType::ArrayOf(elem.clone(), None)
                                }
                                _ => ValueType::Array,
                            }
                        } else if has_any {
                            // If either operand is Any, the result type is determined at runtime
                            // We return Any to signal that dynamic dispatch should be used
                            ValueType::Any
                        } else if lt == ValueType::BigFloat || rt == ValueType::BigFloat {
                            // Issue #3498: BigFloat dominates other numerics (BigFloat + Float64,
                            // BigFloat + BigInt, BigFloat + Int64 all yield BigFloat in Julia).
                            ValueType::BigFloat
                        } else if lt == ValueType::BigInt || rt == ValueType::BigInt {
                            // Issue #3498: BigInt + Float* widens to BigFloat; BigInt + Int/Bool
                            // stays BigInt.
                            let other = if lt == ValueType::BigInt { &rt } else { &lt };
                            match other {
                                ValueType::F16 | ValueType::F32 | ValueType::F64 => {
                                    ValueType::BigFloat
                                }
                                _ => ValueType::BigInt,
                            }
                        } else if let Some(promoted) = promote_numeric_value_types(&lt, &rt) {
                            // Issue #3498: Reuse the centralized promotion logic (the same
                            // promote_type pipeline used by tfunc/lattice inference) for all
                            // primitive numeric pairs. This correctly handles:
                            //   * UInt64+UInt64 → UInt64, Int128+Int128 → Int128
                            //   * UInt8+UInt8 → UInt8 (no widening to Int64)
                            //   * Int8+Int16 → Int16, UInt32+UInt64 → UInt64
                            //   * Mixed signedness (UInt64+Int64 → UInt64, Int8+UInt8 → UInt8)
                            //   * Float16+Float16 → Float16, Float32+Bool → Float32
                            //   * Int+Float → Float (existing behavior)
                            promoted
                        } else {
                            // Non-numeric operands or unknown combo: keep historical Int64
                            // fallback so untyped/dynamic cases still pick a concrete code path.
                            ValueType::I64
                        }
                    }
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                // Infer element types to determine array element type
                let elem_types: Vec<ValueType> =
                    elements.iter().map(|e| self.infer_expr_type(e)).collect();
                let nested_ranks = array_literal_element_ranks(elements);
                let array_elem_type = nested_ranks
                    .as_deref()
                    .and_then(|ranks| infer_nested_array_literal_element_type(&elem_types, ranks))
                    .or_else(|| self.tuple_literal_array_element_type(elements))
                    .unwrap_or_else(|| {
                        infer_array_element_type(
                            &elem_types,
                            |type_id| self.shared_ctx.get_struct_name(type_id),
                            |name| {
                                self.shared_ctx
                                    .struct_table
                                    .get(name)
                                    .map(|info| info.type_id)
                            },
                        )
                        .0
                    });
                ValueType::ArrayOf(array_elem_type, None)
            }
            Expr::Range { .. } => ValueType::Range,
            // Issue #6817: carry the comprehension's rank (= number of iterator
            // clauses) so a 2-D `Matrix` result dispatches to `::Matrix` rather
            // than collapsing to a rank-free `Array` that resolves to `::Vector`.
            Expr::Comprehension { .. } => ValueType::ArrayOf(ArrayElementType::Any, Some(1)),
            // Whitespace flatten form is always 1-D; the comma cartesian form's
            // rank is its binding count (Issue #8014).
            Expr::MultiComprehension {
                iterations,
                flatten,
                ..
            } => {
                let rank = if *flatten { 1 } else { iterations.len() };
                ValueType::ArrayOf(ArrayElementType::Any, Some(rank))
            }
            Expr::Generator { .. } => ValueType::Generator,
            Expr::Index { array, indices, .. } => {
                // Inline typed-array literal `T[a, b, ...]` (lowered to
                // `getindex(T, a, b, ...)`). `compile_builtin_array` materializes
                // this into a `Vector{T}` and returns `ValueType::ArrayOf(elem)`;
                // inference must agree or `resolve_sprint_function_ref` picks the
                // wrong `show` overload and renders `Vector{T}()` (Issue #5241).
                if !indices.is_empty() {
                    if let Some(elem) = self.typed_array_literal_element_type(array) {
                        return ValueType::ArrayOf(elem, None);
                    }
                }

                // Each colon / range / integer-vector index contributes one
                // dimension to the result rank; scalar indices contribute none
                // (Issue #7333). `slice_dims > 0` is the old "is a slice" flag.
                let slice_dims = indices
                    .iter()
                    .filter(|idx| {
                        matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. })
                            || matches!(
                                self.infer_expr_type(idx),
                                ValueType::Array
                                    | ValueType::ArrayOf(ArrayElementType::I8, _)
                                    | ValueType::ArrayOf(ArrayElementType::I16, _)
                                    | ValueType::ArrayOf(ArrayElementType::I32, _)
                                    | ValueType::ArrayOf(ArrayElementType::I64, _)
                                    | ValueType::ArrayOf(ArrayElementType::I128, _)
                                    | ValueType::ArrayOf(ArrayElementType::U8, _)
                                    | ValueType::ArrayOf(ArrayElementType::U16, _)
                                    | ValueType::ArrayOf(ArrayElementType::U32, _)
                                    | ValueType::ArrayOf(ArrayElementType::U64, _)
                                    | ValueType::ArrayOf(ArrayElementType::U128, _)
                                    | ValueType::Range
                            )
                    })
                    .count();
                let is_slice = slice_dims > 0;

                // Check if indexing a String
                let array_type = self.infer_expr_type(array);
                if array_type == ValueType::Str {
                    if is_slice {
                        return ValueType::Str; // String slice returns String
                    } else {
                        return ValueType::Char; // String indexing returns Char
                    }
                }

                // Tuple/NamedTuple element-type sharpening (Issue #5183).
                // `t[k]` with a constant `k` over a statically known
                // `TupleOf`/concrete `@NamedTuple{...}` yields the precise element
                // ValueType so `(a, b) = f()`-style multi-value returns stay
                // type-stable at the use site, instead of collapsing to `Any`.
                if !is_slice && indices.len() == 1 {
                    if let Some(elem) = self.tuple_const_index_value_type(array, &indices[0]) {
                        return elem;
                    }
                }

                // Check array type from locals
                if let Expr::Var(name, _) = array.as_ref() {
                    match self.locals.get(name) {
                        // `ValueType::Dict` is opaque: it carries no value-type
                        // parameter, so an inline `dict[key]` element type is
                        // unknown at compile time. Returning `I64` here caused
                        // String-valued dicts to miscompile `dict[k] == "s"` to
                        // `false` via the String-vs-non-String fold in `==`
                        // compilation (Issue #5269). Defer to runtime dispatch.
                        Some(ValueType::Dict) => return ValueType::Any,
                        Some(ValueType::Str) => {
                            if is_slice {
                                return ValueType::Str;
                            }
                            return ValueType::Char;
                        }
                        Some(ValueType::ArrayOf(ref elem_type, _)) => {
                            if is_slice {
                                // Carry the rank so the value channel agrees with
                                // the rank-aware JuliaType dispatch channel
                                // (Issue #7333): `m[:, 1]` -> rank 1, `m[:, :]`
                                // -> rank 2.
                                return ValueType::ArrayOf(elem_type.clone(), Some(slice_dims));
                            }
                            // Return element type for single element access
                            return match elem_type {
                                ArrayElementType::I64 => ValueType::I64,
                                ArrayElementType::F64 => ValueType::F64,
                                ArrayElementType::Bool => ValueType::I64, // Bool stored as I64
                                ArrayElementType::StructOf(type_id) => ValueType::Struct(*type_id),
                                ArrayElementType::TupleOf(_) => ValueType::Tuple, // Tuple array access returns Tuple
                                _ => ValueType::Any,
                            };
                        }
                        Some(ValueType::Array) => {
                            if is_slice {
                                return ValueType::Array;
                            }
                            return ValueType::Any; // Array element type determined at runtime
                        }
                        _ => {}
                    }
                }

                // Default for slicing or unknown array
                if is_slice {
                    // Slicing a range with a range index yields a range, not a
                    // Vector (`(1:10)[1:3] === 1:3`), so inline consumers like
                    // `collect`/`sum`/broadcast compile the range path (Issue #5751).
                    if array_type == ValueType::Range {
                        return ValueType::Range;
                    }
                    ValueType::Array
                } else {
                    ValueType::Any // Tuple/array element type determined at runtime
                }
            }
            Expr::SliceAll { .. } => ValueType::Array,
            Expr::Builtin { name, args, .. } => {
                // Infer return type for builtin operations
                match name {
                    // Complex operations (Conj, Real, Imag, Abs, Abs2) are now Pure Julia
                    BuiltinOp::Zero => {
                        // zero returns same type as input
                        if !args.is_empty() {
                            self.infer_expr_type(&args[0])
                        } else {
                            ValueType::F64
                        }
                    }
                    BuiltinOp::Sqrt => {
                        // sqrt(::Complex) is handled by Pure Julia (base/complex.jl)
                        // For primitives, sqrt returns F64
                        if !args.is_empty() {
                            let arg_ty = self.infer_expr_type(&args[0]);
                            if matches!(arg_ty, ValueType::Struct(_)) {
                                // Struct sqrt returns the same struct type
                                arg_ty
                            } else {
                                ValueType::F64
                            }
                        } else {
                            ValueType::F64
                        }
                    }
                    BuiltinOp::Zeros
                    | BuiltinOp::Ones => ValueType::Array,
                    // Note: Trues, Falses, Fill are now Pure Julia — Issue #2640
                    // Note: Adjoint and Transpose are now Pure Julia
                    BuiltinOp::Lu => ValueType::Tuple,
                    BuiltinOp::Det => ValueType::F64,
                    // Note: Inv, TupleLength removed — dead code (Issue #2643)
                    BuiltinOp::Length | BuiltinOp::TimeNs => ValueType::I64,
                    BuiltinOp::Size => {
                        // `size(arr, k)` returns the k-th dim length as I64;
                        // `size(arr)` returns a Tuple of dim lengths. Default falls
                        // back to F64 in this match arm, which silently mis-routes
                        // dispatch (e.g. `similar(arr, size(arr, 1), size(arr, 2))`
                        // — Issue #3777). Distinguish by argc.
                        if args.len() >= 2 {
                            ValueType::I64
                        } else {
                            ValueType::Tuple
                        }
                    }
                    // get(d, key, default) returns value type or default — conservatively Any (Issue #3471)
                    BuiltinOp::DictGet => ValueType::Any,
                    BuiltinOp::Rand | BuiltinOp::Randn => {
                        if args.is_empty() {
                            ValueType::F64
                        } else {
                            // `rand(n)`/`randn(n, m)` carry a ranked `ArrayOf` so
                            // the value channel agrees with the JuliaType dispatch
                            // channel (Issue #7307); collection/RNG forms keep the
                            // legacy unparameterized `Array`.
                            expr_tfuncs::infer_rand_array_value_type_for(
                                matches!(name, BuiltinOp::Randn),
                                args,
                                |arg| self.infer_expr_type(arg),
                            )
                            .unwrap_or(ValueType::Array)
                        }
                    }
                    BuiltinOp::StableRNG
                    | BuiltinOp::XoshiroRNG
                    | BuiltinOp::MersenneTwisterRNG => ValueType::Rng,
                    // `first(t)`/`last(t)` over a statically known tuple type
                    // return the precise first/last element ValueType
                    // (Issue #5183); otherwise the element type is unknown.
                    BuiltinOp::TupleFirst | BuiltinOp::TupleLast => args
                        .first()
                        .and_then(|arg| {
                            self.tuple_first_last_value_type(
                                arg,
                                matches!(name, BuiltinOp::TupleLast),
                            )
                        })
                        .unwrap_or(ValueType::Any),
                    BuiltinOp::DictKeys | BuiltinOp::DictValues | BuiltinOp::DictPairs => {
                        ValueType::Tuple
                    }
                    BuiltinOp::DictDelete
                    | BuiltinOp::DictMerge
                    | BuiltinOp::DictMergeBang
                    | BuiltinOp::DictEmpty => ValueType::Dict,
                    BuiltinOp::DictGetBang => ValueType::Any, // Returns the value
                    BuiltinOp::Ref => ValueType::Any,         // Ref can wrap any type
                    BuiltinOp::TypeOf | BuiltinOp::Supertype => ValueType::DataType,
                    BuiltinOp::Typename | BuiltinOp::FunctionName => ValueType::Symbol,
                    BuiltinOp::Isa
                    | BuiltinOp::Isbitstype
                    | BuiltinOp::HasKey
                    // Isbits, Hasfield, Ismutable removed - pure Julia (Issue #6738)
                    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                    // removed - now Pure Julia (base/reflection.jl)
                    => ValueType::Bool,
                    BuiltinOp::Sizeof => ValueType::I64,
                    BuiltinOp::Iterate => ValueType::Any, // Returns Tuple or Nothing
                    BuiltinOp::Collect => ValueType::Array,
                    BuiltinOp::Generator => ValueType::Generator, // lazy iterator
                    BuiltinOp::SymbolNew => ValueType::Symbol, // Symbol("name")
                    BuiltinOp::ExprNew => ValueType::Expr, // Expr(head, args...)
                    BuiltinOp::LineNumberNodeNew => ValueType::LineNumberNode, // LineNumberNode(line, file)
                    BuiltinOp::QuoteNodeNew => ValueType::QuoteNode,           // QuoteNode(value)
                    BuiltinOp::GlobalRefNew => ValueType::GlobalRef, // GlobalRef(mod, name)
                    BuiltinOp::Gensym => ValueType::Symbol,          // gensym() or gensym("base")
                    BuiltinOp::Esc => ValueType::Expr,               // esc(expr)
                    BuiltinOp::Eval => ValueType::Any, // eval(expr) - result type is dynamic
                    BuiltinOp::MacroExpand | BuiltinOp::MacroExpandBang => ValueType::Any, // macroexpand returns any type
                    BuiltinOp::IncludeString | BuiltinOp::EvalFile => ValueType::Any, // dynamic code evaluation
                    // Note: BuiltinOp::Zero is already handled above
                    BuiltinOp::IfElse => {
                        if args.len() >= 3 {
                            let then_ty = self.infer_expr_type(&args[1]);
                            let else_ty = self.infer_expr_type(&args[2]);
                            if then_ty == ValueType::F64 || else_ty == ValueType::F64 {
                                ValueType::F64
                            } else {
                                then_ty
                            }
                        } else {
                            ValueType::I64
                        }
                    }
                    _ => ValueType::F64,
                }
            }
            Expr::UnaryOp { op, operand, .. } => {
                match op {
                    UnaryOp::Not if self.infer_unary_not_operand_is_callable(operand) => {
                        ValueType::Function
                    }
                    UnaryOp::Not => ValueType::Bool,
                    _ => self.infer_expr_type(operand), // Neg, Pos preserve operand type
                }
            }
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                if function == "materialize" && is_broadcasted_comparison_call(args.first()) {
                    return ValueType::ArrayOf(ArrayElementType::Bool, None);
                }

                if shared::is_truncated_result_call(function, args, kwargs) {
                    return self
                        .shared_ctx
                        .get_struct_type_id("Distributions.Truncated")
                        .or_else(|| self.shared_ctx.get_struct_type_id("Truncated"))
                        .map(ValueType::Struct)
                        .unwrap_or(ValueType::Any);
                }

                if let Some(folded) = shared::folded_nary_operator_call(function, args) {
                    return self.infer_expr_type(&folded);
                }

                // Check if this is a broadcast call (function name starts with '.')
                // Broadcast operations return Array
                if function.starts_with('.') {
                    return ValueType::Array;
                }

                if let Some(memory_ty) = shared::memory_constructor_value_type(function, |name| {
                    self.shared_ctx.get_struct_type_id(name)
                }) {
                    return memory_ty;
                }
                if matches!(function.as_str(), "merge" | "Base.merge")
                    && !args.is_empty()
                    && args.iter().all(|arg| {
                        static_named_tuple_field_names_from_julia_type(&self.infer_julia_type(arg))
                            .is_some()
                    })
                {
                    return ValueType::NamedTuple;
                }
                if matches!(function.as_str(), "copy" | "Base.copy")
                    && args.len() == 1
                    && matches!(self.infer_expr_type(&args[0]), ValueType::Dict)
                {
                    return ValueType::Dict;
                }
                // `copy(s::Set{T})` returns a fresh `Set{T}` struct (Issue #6721);
                // pin the argument's struct type so a following typed `Set`
                // operation resolves through Set method dispatch.
                if matches!(function.as_str(), "copy" | "Base.copy") && args.len() == 1 {
                    let arg_value_type = self.infer_expr_type(&args[0]);
                    if matches!(&arg_value_type, ValueType::Struct(type_id)
                        if self.shared_ctx.get_struct_name(*type_id)
                            .is_some_and(|name| name == "Set" || name.starts_with("Set{")))
                    {
                        return arg_value_type;
                    }
                    if matches!(arg_value_type, ValueType::Set) {
                        return ValueType::Set;
                    }
                }
                if matches!(function.as_str(), "values" | "Base.values")
                    && args.len() == 1
                    && matches!(self.infer_expr_type(&args[0]), ValueType::NamedTuple)
                {
                    return ValueType::Tuple;
                }
                if matches!(function.as_str(), "pairs" | "Base.pairs") && args.len() == 1 {
                    return match self.infer_expr_type(&args[0]) {
                        ValueType::Dict => ValueType::Dict,
                        ValueType::Array
                        | ValueType::ArrayOf(_, _)
                        | ValueType::Memory
                        | ValueType::MemoryOf(_)
                        | ValueType::Tuple
                        | ValueType::NamedTuple => self
                            .shared_ctx
                            .get_struct_type_id("Pairs")
                            .map(ValueType::Struct)
                            .unwrap_or(ValueType::Any),
                        _ => ValueType::Any,
                    };
                }

                if function == "getindex" && args.len() == 2 {
                    let collection_type = self.infer_expr_type(&args[0]);
                    if collection_type == ValueType::Str {
                        let is_slice =
                            matches!(&args[1], Expr::Range { .. } | Expr::SliceAll { .. });
                        return if is_slice {
                            ValueType::Str
                        } else {
                            ValueType::Char
                        };
                    }
                }
                if let Some(inferred) = expr_tfuncs::infer_value_type_call(function, args, |arg| {
                    self.infer_expr_type(arg)
                }) {
                    return inferred;
                }
                if matches!(function.as_str(), "view" | "Base.view") {
                    let julia_args = args
                        .iter()
                        .map(|arg| self.infer_julia_type(arg))
                        .collect::<Vec<_>>();
                    if let Some(inferred) =
                        self.infer_view_call_return_type(function, args, &julia_args)
                    {
                        return inferred;
                    }
                }
                if expr_tfuncs::is_type_object_call(function) {
                    let julia_args = args
                        .iter()
                        .map(|arg| self.infer_julia_type(arg))
                        .collect::<Vec<_>>();
                    if let Some(inferred) =
                        expr_tfuncs::infer_value_type_object_call(function, &julia_args)
                    {
                        return inferred;
                    }
                }
                if expr_tfuncs::is_array_constructor_call(function) {
                    let value_args = args
                        .iter()
                        .map(|arg| self.infer_expr_type(arg))
                        .collect::<Vec<_>>();
                    let julia_args = args
                        .iter()
                        .map(|arg| self.infer_julia_type(arg))
                        .collect::<Vec<_>>();
                    if let Some(inferred) = expr_tfuncs::infer_value_array_constructor_call(
                        function,
                        args,
                        &value_args,
                        &julia_args,
                        |name| self.shared_ctx.get_struct_type_id(name),
                    ) {
                        return inferred;
                    }
                }

                // Check if this is a type constructor or known builtin function
                // (gcd/lcm, IOBuffer, big, and the DataType-returning helpers
                // are routed through expr_tfuncs above — Issue #5922).
                match function.as_str() {
                    // Lowercase `complex` constructor resolves through the shared
                    // registry's contextual tfunc (Issue #5922); the struct-table
                    // lookup order (Complex{Float64} → Complex → any deterministic
                    // instantiation) lives in `tfunc_complex_contextual`.
                    "complex" => {
                        let ids = expr_tfuncs::SharedCtxStructIds(&*self.shared_ctx);
                        expr_tfuncs::infer_value_complex_call(&ids).unwrap_or(ValueType::Any)
                    }
                    // `float(::Type)` returns a type object, e.g.
                    // `float(eltype(xs)) -> Float64`. Keeping the local binding
                    // as DataType lets runtime parametric constructors such as
                    // `Segment{Tx,...}` use it as a type parameter. (Issue #8324)
                    "float" | "Base.float"
                        if args.len() == 1
                            && matches!(self.infer_expr_type(&args[0]), ValueType::DataType) =>
                    {
                        ValueType::DataType
                    }
                    // Public Dict()/Dict{K,V}() construction routes through the
                    // pure-Julia Dict struct methods (Issue #6619). The
                    // adapter resolves the `Dict{K,V}` instantiation when the
                    // constructor result is known, otherwise it widens to Any.
                    f if f == "Dict" || f.starts_with("Dict{") => {
                        let arg_types: Vec<JuliaType> =
                            args.iter().map(|a| self.infer_julia_type(a)).collect();
                        let mut inst = expr_tfuncs::SharedCtxInstantiation(&mut *self.shared_ctx);
                        expr_tfuncs::infer_value_dict_constructor_call(
                            function, &arg_types, &mut inst,
                        )
                        .unwrap_or(ValueType::Any)
                    }
                    // Public Set()/Set{T}() construction routes through the
                    // pure-Julia Set{T} struct (over Dict{T,Nothing}) so a `Set`
                    // value infers as the struct instantiation, not the legacy
                    // native carrier, and user `Set{T}` methods dispatch (Issue
                    // #6721). Mirrors the Dict adapter above.
                    f if f == "Set" || f.starts_with("Set{") => {
                        let arg_types: Vec<JuliaType> =
                            args.iter().map(|a| self.infer_julia_type(a)).collect();
                        let mut inst = expr_tfuncs::SharedCtxInstantiation(&mut *self.shared_ctx);
                        expr_tfuncs::infer_value_set_constructor_call(
                            function, &arg_types, &mut inst,
                        )
                        .unwrap_or(ValueType::Any)
                    }
                    _ => {
                        // Default struct constructor: exact struct-table entries
                        // resolve through the registry's shared constructor rule
                        // (Issue #5922). The parametric/Rational/`{`-instantiated
                        // arms resolve through the adapter's StructInstantiation
                        // seam (instantiation needs `&mut SharedCompileContext`,
                        // so the rules live in the adapter, not the registry).
                        let default_ctor = {
                            let ids = expr_tfuncs::SharedCtxStructIds(&*self.shared_ctx);
                            expr_tfuncs::infer_value_struct_constructor_call(function, &ids)
                        };
                        if let Some(inferred) = default_ctor {
                            inferred
                        } else if self.shared_ctx.parametric_structs.contains_key(function) {
                            // A user-defined *outer* constructor (`Foo(x::Real) =
                            // Foo{typeof(float(x))}(...)`) can transform its
                            // arguments before binding the struct's type
                            // parameters, so the type parameters do not follow
                            // from the raw argument types. Inferring `Foo{Int64}`
                            // from an `Int64` argument then typed the field load
                            // as `Int64` while the constructor body promoted it to
                            // `Float64` at runtime ("expected I64, got Float64",
                            // Issue #7284). Re-infer through the user constructor's
                            // body first; only fall back to the naive type-arg
                            // inference (the default inner-constructor model) when
                            // there is no matching user method.
                            if let Some(inferred) =
                                self.infer_user_outer_constructor_return_type(function, args)
                            {
                                inferred
                            } else {
                                // Parametric struct constructor - infer type args from arguments
                                let arg_types: Vec<JuliaType> =
                                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                                let mut inst =
                                    expr_tfuncs::SharedCtxInstantiation(&mut *self.shared_ctx);
                                expr_tfuncs::infer_value_parametric_struct_ctor(
                                    function, &mut inst, &arg_types,
                                )
                            }
                        } else if crate::vm::value::is_rational_type_name(function) {
                            let inst = expr_tfuncs::SharedCtxInstantiation(&mut *self.shared_ctx);
                            expr_tfuncs::infer_value_rational_ctor(function, &inst)
                        } else if function.contains('{') {
                            // Parametric struct instantiations like Val{1}(),
                            // Point{Int64}(): resolve the (possibly qualified)
                            // base name, then the adapter instantiates on demand.
                            let base_name = &function[..function.find('{').unwrap()];
                            if let Some(resolved_base_name) =
                                self.resolve_parametric_struct_name(base_name)
                            {
                                let mut inst =
                                    expr_tfuncs::SharedCtxInstantiation(&mut *self.shared_ctx);
                                expr_tfuncs::infer_value_instantiated_ctor(
                                    function,
                                    &resolved_base_name,
                                    &mut inst,
                                )
                            } else {
                                ValueType::Any
                            }
                        } else {
                            // Special handling for HOF (Higher-Order Functions) like map/filter.
                            // These need call-site specialization to infer the correct return type.
                            //
                            // These rules analyze the *expression* of the function
                            // argument (inline lambda bodies, FunctionRef
                            // method-table lookups) to compute the mapped element
                            // type — a plain TransferFn, which sees only argument
                            // lattice types, cannot. `map` has been migrated onto
                            // the registry path (Issue #6604): the registry rule
                            // `tfuncs::hof_ops::map_call_result` receives the
                            // function-argument expression via the
                            // `HofLambdaAnalyzer` seam (`TFuncContext::arg_exprs`)
                            // and calls back into `CoreCompiler` to infer the
                            // lambda's return type. `infer_map_call_return_type`
                            // is now the thin adapter that drives it. The
                            // remaining HOFs (broadcast/filter/reduce/mapreduce)
                            // still infer directly at this layer pending migration.
                            if matches!(function.as_str(), "map" | "Base.map") && args.len() == 2 {
                                // map(f, arr) - infer return type based on f's return type
                                if let Some(return_type) =
                                    self.infer_map_call_return_type(&args[0], &args[1])
                                {
                                    return return_type;
                                }
                            } else if matches!(function.as_str(), "map" | "Base.map")
                                && args.len() == 3
                            {
                                // binary map(f, left, right) maps f over visible element types.
                                if let Some(return_type) = self
                                    .infer_binary_map_call_return_type(&args[0], &args[1], &args[2])
                                {
                                    return return_type;
                                }
                            } else if matches!(function.as_str(), "map" | "Base.map")
                                && args.len() >= 4
                            {
                                // n-ary map(f, left, right, rest...) maps f over visible element types.
                                if let Some(return_type) =
                                    self.infer_nary_map_call_return_type(&args[0], &args[1..])
                                {
                                    return return_type;
                                }
                            } else if matches!(function.as_str(), "broadcast" | "Base.broadcast")
                                && args.len() == 2
                            {
                                // unary broadcast(f, arr) - infer return type like map(f, arr)
                                if let Some(return_type) =
                                    self.infer_map_call_return_type(&args[0], &args[1])
                                {
                                    return return_type;
                                }
                            } else if matches!(function.as_str(), "broadcast" | "Base.broadcast")
                                && args.len() == 3
                            {
                                // binary broadcast(f, left, right) maps f over visible element types.
                                if let Some(return_type) = self
                                    .infer_binary_map_call_return_type(&args[0], &args[1], &args[2])
                                {
                                    return return_type;
                                }
                            } else if matches!(function.as_str(), "broadcast" | "Base.broadcast")
                                && args.len() >= 4
                            {
                                // n-ary broadcast(f, left, right, rest...) maps f over visible element types.
                                if let Some(return_type) =
                                    self.infer_nary_map_call_return_type(&args[0], &args[1..])
                                {
                                    return return_type;
                                }
                            } else if matches!(function.as_str(), "filter" | "Base.filter")
                                && args.len() == 2
                            {
                                // filter(pred, arr) - return type is same element type as input
                                if let Some(return_type) =
                                    self.infer_filter_call_return_type(&args[1])
                                {
                                    return return_type;
                                }
                            } else if matches!(
                                function.as_str(),
                                "mapreduce"
                                    | "mapfoldl"
                                    | "mapfoldr"
                                    | "Base.mapreduce"
                                    | "Base.mapfoldl"
                                    | "Base.mapfoldr"
                            ) && args.len() >= 3
                            {
                                // mapreduce(f, op, itr) / mapfoldl / mapfoldr - infer from
                                // mapped element type and reducer when both are visible.
                                if let Some(return_type) = self
                                    .infer_mapreduce_call_return_type(&args[0], &args[1], &args[2])
                                {
                                    return return_type;
                                }
                            } else if matches!(
                                function.as_str(),
                                "reduce"
                                    | "foldl"
                                    | "foldr"
                                    | "Base.reduce"
                                    | "Base.foldl"
                                    | "Base.foldr"
                            ) && args.len() >= 2
                            {
                                // reduce(op, itr) / foldl / foldr - infer from the operator and
                                // iterator element type, including inline lambda operators.
                                if let Some(return_type) =
                                    self.infer_reduce_call_return_type(&args[0], &args[1])
                                {
                                    return return_type;
                                }
                            }

                            // Pure Julia functions - try to infer return type from method table
                            if let Some(table) = self.method_tables.get(function.as_str()) {
                                // Infer argument types for dispatch
                                let arg_types: Vec<JuliaType> =
                                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                                // Try to find matching method and get its return type
                                if let Ok(method) = table.dispatch(&arg_types) {
                                    if !matches!(method.return_type, ValueType::Any) {
                                        return method.return_type.clone();
                                    }

                                    let arg_value_types: Vec<ValueType> =
                                        args.iter().map(|arg| self.infer_expr_type(arg)).collect();
                                    if !arg_value_types
                                        .iter()
                                        .any(|ty| matches!(ty, ValueType::Any))
                                    {
                                        if let Some(func_ir) = self
                                            .shared_ctx
                                            .function_ir_by_global_index
                                            .get(&method.global_index)
                                        {
                                            let inferred = self
                                                .infer_shared_function_return_type_with_arg_types(
                                                    func_ir,
                                                    &arg_value_types,
                                                );
                                            if self.should_accept_body_reinferred_call_return_type(
                                                &inferred,
                                            ) {
                                                return inferred;
                                            }
                                        }
                                    }

                                    return method.return_type.clone();
                                }
                            }
                            // Fallback to Any if no method matches
                            ValueType::Any
                        }
                    }
                }
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                ..
            } => {
                // Module-qualified function call: Module.func(args)
                let resolved_module = self
                    .module_aliases
                    .get(module.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or(module.as_str());
                if resolved_module == "Base" && function == "Generator" {
                    return ValueType::Generator;
                }
                let qualified_struct_name = format!("{resolved_module}.{function}");
                if let Some(type_id) = self
                    .shared_ctx
                    .get_struct_type_id(&qualified_struct_name)
                    .or_else(|| self.shared_ctx.get_struct_type_id(function))
                {
                    return ValueType::Struct(type_id);
                }
                // Look up the method table for this function and infer return type
                if let Some(table) = self.method_tables.get(function.as_str()) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    if let Ok(method) = table.dispatch(&arg_types) {
                        return method.return_type.clone();
                    }
                }
                // Fallback to Any if no method matches
                ValueType::Any
            }
            Expr::TupleLiteral { .. } => ValueType::Tuple,
            Expr::NamedTupleLiteral { .. } => ValueType::Tuple,
            Expr::Pair { .. } => ValueType::Tuple,
            // QuoteLiteral produces either Expr or Symbol depending on the constructor
            Expr::QuoteLiteral { constructor, .. } => {
                // Recursively infer the type from the constructor
                self.infer_expr_type(constructor)
            }
            // FieldAccess - check for Expr fields
            Expr::FieldAccess { object, field, .. } => {
                let obj_ty = self.infer_expr_type(object);
                if obj_ty == ValueType::Expr {
                    match field.as_str() {
                        "head" => ValueType::Symbol,
                        "args" => ValueType::Array,
                        _ => ValueType::Any,
                    }
                } else if let ValueType::Struct(type_id) = obj_ty {
                    // Look up field type from struct definition
                    for (_, struct_info) in self.shared_ctx.struct_table.iter() {
                        if struct_info.type_id == type_id {
                            for (field_name, field_ty) in &struct_info.fields {
                                if field_name == field {
                                    return field_ty.clone();
                                }
                            }
                            break;
                        }
                    }
                    ValueType::Any
                } else {
                    ValueType::Any
                }
            }
            Expr::TypedEmptyArray { element_type, .. } => {
                // Typed empty array like Bool[], Int64[], Float64[]
                match element_type.as_str() {
                    "Int" if crate::types::native_int_type_name() == "Int32" => {
                        ValueType::ArrayOf(ArrayElementType::I32, None)
                    }
                    "Int" | "Int64" => ValueType::ArrayOf(ArrayElementType::I64, None),
                    "Int32" => ValueType::ArrayOf(ArrayElementType::I64, None), // Store as I64
                    "UInt" if crate::types::native_uint_type_name() == "UInt32" => {
                        ValueType::ArrayOf(ArrayElementType::U32, None)
                    }
                    "UInt" | "UInt64" => ValueType::ArrayOf(ArrayElementType::U64, None),
                    "UInt32" => ValueType::ArrayOf(ArrayElementType::U32, None),
                    "UInt16" => ValueType::ArrayOf(ArrayElementType::U16, None),
                    "UInt8" => ValueType::ArrayOf(ArrayElementType::U8, None),
                    "Float64" | "Float32" => ValueType::ArrayOf(ArrayElementType::F64, None),
                    "Bool" => ValueType::ArrayOf(ArrayElementType::Bool, None),
                    "String" => ValueType::ArrayOf(ArrayElementType::String, None),
                    "Char" => ValueType::ArrayOf(ArrayElementType::Char, None),
                    // Issue #5711: keep Symbol / Regex / RegexMatch element types.
                    "Symbol" => ValueType::ArrayOf(ArrayElementType::Symbol, None),
                    "Regex" => {
                        ValueType::ArrayOf(ArrayElementType::Abstract("Regex".to_string()), None)
                    }
                    "RegexMatch" => ValueType::ArrayOf(
                        ArrayElementType::Abstract("RegexMatch".to_string()),
                        None,
                    ),
                    "Union{}" | "Bottom" => {
                        ValueType::ArrayOf(ArrayElementType::UnionOf(Vec::new()), None)
                    }
                    "Any" => ValueType::ArrayOf(ArrayElementType::Any, None),
                    type_name => {
                        // Check if it's a struct type
                        let base_name = type_name.split('{').next().unwrap_or(type_name);
                        if let Some(type_id) = self.shared_ctx.get_struct_type_id(base_name) {
                            ValueType::ArrayOf(ArrayElementType::StructOf(type_id), None)
                        } else {
                            // Preserve concrete parametric eltypes (Issue #6768).
                            ValueType::ArrayOf(
                                super::concrete_parametric_element_type_from_name(type_name)
                                    .unwrap_or(ArrayElementType::Any),
                                None,
                            )
                        }
                    }
                }
            }
            // Value-position `if`/ternary (and `begin`/`if` lowered to these by
            // lowering/expr/misc.rs) join their branch value types instead of
            // widening to Any (Issue #5180). This keeps slot typing for
            // `x = cond ? a + b : c + d` (stays I64) and operand/index uses of
            // such expressions, while mixed I64/F64 and incompatible branches
            // widen exactly like the pre-inference reference in
            // `compile::inference` (`infer_value_type_with_structs`).
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let then_ty = self.infer_expr_type(then_expr);
                let else_ty = self.infer_expr_type(else_expr);
                join_value_position_branch_types(then_ty, else_ty)
            }
            // `let ... ; body end` (and `begin`/value-position `if` lowered to a
            // LetBlock) evaluates to the value of the last statement in its body.
            // Infer that tail-value type so a block used as an index/operand keeps
            // slot typing (Issue #5180). A block whose tail is a `return`, or that
            // has no fall-through tail expression, stays Any.
            Expr::LetBlock { body, .. } => match body.stmts.last() {
                Some(crate::ir::core::Stmt::Expr { expr, .. }) => self.infer_expr_type(expr),
                _ => ValueType::Any,
            },
            // Default fallback - use Any instead of F64 to avoid type mismatches
            _ => ValueType::Any,
        }
    }

    fn infer_unary_not_operand_is_callable(&mut self, operand: &Expr) -> bool {
        if self.infer_expr_type(operand) == ValueType::Function {
            return true;
        }

        match operand {
            Expr::FunctionRef { .. } => true,
            Expr::Var(name, _) if !self.locals.contains_key(name) => {
                self.method_tables.contains_key(name)
                    || is_base_function(name)
                    || self.function_aliases.contains_key(name)
                    || (self.usings.contains("Random") && is_random_function(name))
            }
            _ => false,
        }
    }
}

/// Join the two branch value types of a value-position `if`/ternary, using the
/// same widening policy as the pre-inference reference
/// (`compile::inference::infer_value_type_with_structs`'s `Expr::Ternary` arm,
/// Issue #3533): same type → keep; either branch a Tuple → Tuple; either branch
/// a Struct (and they differ) → Any (do not silently drop the other branch);
/// two numeric types → promote (`I64 + F64 -> F64`, `I64 + I64 -> I64`);
/// otherwise widen to Any (Issue #5180).
fn join_value_position_branch_types(then_ty: ValueType, else_ty: ValueType) -> ValueType {
    if then_ty == else_ty {
        then_ty
    } else if then_ty == ValueType::Tuple || else_ty == ValueType::Tuple {
        ValueType::Tuple
    } else if matches!(then_ty, ValueType::Struct(_)) || matches!(else_ty, ValueType::Struct(_)) {
        ValueType::Any
    } else if let Some(promoted) = promote_numeric_value_types(&then_ty, &else_ty) {
        promoted
    } else {
        ValueType::Any
    }
}

/// Map a narrow-integer JuliaType (recorded in `julia_type_locals` for typed
/// parameters by compile/mod.rs) to its precise ValueType. Bool is intentionally
/// excluded because Julia's `Bool + Bool` widens to Int64, so leaving Bool
/// params as ValueType::I64 in `locals` matches Julia semantics.
fn narrow_int_value_type(jt: &JuliaType) -> Option<ValueType> {
    match jt {
        JuliaType::Int8 => Some(ValueType::I8),
        JuliaType::Int16 => Some(ValueType::I16),
        JuliaType::Int32 => Some(ValueType::I32),
        JuliaType::Int128 => Some(ValueType::I128),
        JuliaType::UInt8 => Some(ValueType::U8),
        JuliaType::UInt16 => Some(ValueType::U16),
        JuliaType::UInt32 => Some(ValueType::U32),
        JuliaType::UInt64 => Some(ValueType::U64),
        JuliaType::UInt128 => Some(ValueType::U128),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::join_value_position_branch_types;
    use crate::vm::ValueType;

    // Issue #5180: value-position if/ternary/begin must type-join their branch
    // value types instead of widening to Any.

    #[test]
    fn same_typed_branches_keep_concrete_type() {
        // `cond ? a + b : c + d` where both branches are I64 stays I64.
        assert_eq!(
            join_value_position_branch_types(ValueType::I64, ValueType::I64),
            ValueType::I64
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::F64, ValueType::F64),
            ValueType::F64
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::Bool, ValueType::Bool),
            ValueType::Bool
        );
    }

    #[test]
    fn mixed_int_float_branches_promote_to_float() {
        // Julia widening: I64 + F64 -> F64 (NOT Any), matching the existing
        // promote_numeric_value_types policy.
        assert_eq!(
            join_value_position_branch_types(ValueType::I64, ValueType::F64),
            ValueType::F64
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::F64, ValueType::I64),
            ValueType::F64
        );
    }

    #[test]
    fn incompatible_branches_widen_to_any() {
        // Int vs String: no numeric promotion → Any (dynamic slot).
        assert_eq!(
            join_value_position_branch_types(ValueType::I64, ValueType::Str),
            ValueType::Any
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::Str, ValueType::I64),
            ValueType::Any
        );
    }

    #[test]
    fn struct_branch_does_not_dominate() {
        // Issue #3533: when branches differ and one is a Struct, widen to Any
        // rather than silently picking the struct branch.
        assert_eq!(
            join_value_position_branch_types(ValueType::Struct(7), ValueType::I64),
            ValueType::Any
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::I64, ValueType::Struct(7)),
            ValueType::Any
        );
        // Same struct on both branches keeps the struct type.
        assert_eq!(
            join_value_position_branch_types(ValueType::Struct(7), ValueType::Struct(7)),
            ValueType::Struct(7)
        );
    }

    #[test]
    fn either_tuple_branch_yields_tuple() {
        assert_eq!(
            join_value_position_branch_types(ValueType::Tuple, ValueType::I64),
            ValueType::Tuple
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::I64, ValueType::Tuple),
            ValueType::Tuple
        );
    }

    #[test]
    fn any_branch_stays_any() {
        assert_eq!(
            join_value_position_branch_types(ValueType::Any, ValueType::I64),
            ValueType::Any
        );
        assert_eq!(
            join_value_position_branch_types(ValueType::I64, ValueType::Any),
            ValueType::Any
        );
    }
}

fn static_named_tuple_field_names_from_julia_type(ty: &JuliaType) -> Option<Vec<String>> {
    let JuliaType::Struct(name) = ty else {
        return None;
    };
    let body = name.strip_prefix("@NamedTuple{")?.strip_suffix('}')?.trim();
    if body.is_empty() {
        return Some(Vec::new());
    }
    split_static_named_tuple_fields(body)
        .into_iter()
        .map(|field| {
            let name = field
                .split_once("::")
                .map_or(field.trim(), |(name, _)| name.trim());
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

fn split_static_named_tuple_fields(body: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in body.char_indices() {
        match ch {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                let field = body[start..idx].trim();
                if !field.is_empty() {
                    fields.push(field);
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = body[start..].trim();
    if !last.is_empty() {
        fields.push(last);
    }
    fields
}
