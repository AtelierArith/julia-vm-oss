//! Julia type inference for method dispatch.
//!
//! Handles inference of JuliaType for expressions, used by the method dispatch system
//! to determine which method to call. Also provides ValueType-to-JuliaType conversion.

use crate::compile::promotion::{extract_complex_param, promote_type};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, Stmt, UnaryOp};
use crate::types::JuliaType;
use crate::vm::value::julia_array_type_for_ndims;
use crate::vm::{ArrayElementType, ValueType};

use crate::compile::{
    binary_op_to_function_name, is_base_function, is_builtin_type_name, is_euler_name, is_pi_name,
    is_random_function, CoreCompiler,
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

fn is_integer_julia_type(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Int8
            | JuliaType::Int16
            | JuliaType::Int32
            | JuliaType::Int64
            | JuliaType::Int128
            | JuliaType::UInt8
            | JuliaType::UInt16
            | JuliaType::UInt32
            | JuliaType::UInt64
            | JuliaType::UInt128
            | JuliaType::Integer
    )
}

/// Promote two element types for Complex arithmetic.
/// Uses the centralized promotion module following Julia's promote_rule/promote_type pattern.
fn promote_complex_element(elem1: &str, elem2: &str) -> String {
    promote_type(elem1, elem2)
}

fn reshape_rank_from_args(args: &[Expr]) -> Option<usize> {
    if args.len() < 2 {
        return None;
    }

    if args.len() == 2 {
        if let Expr::TupleLiteral { elements, .. } = &args[1] {
            return Some(elements.len());
        }
        return None;
    }

    Some(args.len() - 1)
}

fn top_level_comma_index(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}

/// Whether a `JuliaType` names an array-family receiver that a slice
/// (`A[:, 1]`, `A[:, :]`) should rank-recover into a `Vector`/`Matrix`
/// (Issue #7333). Tuples, strings, ranges, etc. are excluded so a non-array
/// slice keeps the conservative `Array` fallback.
fn is_array_family_receiver(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => true,
        JuliaType::Struct(name) => matches!(
            name.split('{').next().unwrap_or(name.as_str()),
            "Array" | "Vector" | "Matrix"
        ),
        _ => false,
    }
}

fn array_element_from_julia_type(ty: JuliaType) -> JuliaType {
    match ty {
        JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) => *elem,
        JuliaType::Struct(name) => name
            .strip_prefix("Array{")
            .and_then(|body| body.strip_suffix('}'))
            .and_then(|body| top_level_comma_index(body).map(|idx| body[..idx].trim()))
            .map(JuliaType::from_name_or_struct)
            .unwrap_or(JuliaType::Any),
        _ => JuliaType::Any,
    }
}

/// Static rank (ndims) of an array-family `JuliaType`, or `None` when the rank is
/// not statically known. Used to sharpen `IteratorSize(::AbstractArray{T,N})` to
/// the concrete `HasShape{N}` so `_collect` dispatches to `::HasShape` (Issue #5850).
fn ndims_from_array_julia_type(jt: &JuliaType) -> Option<i64> {
    match jt {
        JuliaType::VectorOf(_) => Some(1),
        JuliaType::MatrixOf(_) => Some(2),
        JuliaType::Struct(name) => match name.split('{').next().unwrap_or(name.as_str()) {
            "Vector" => Some(1),
            "Matrix" => Some(2),
            "Array" => name
                .rsplit(',')
                .next()
                .and_then(|s| s.trim().trim_end_matches('}').parse::<i64>().ok()),
            _ => None,
        },
        _ => None,
    }
}

impl CoreCompiler<'_> {
    /// Whether a parametric constructor name like `SMatrix{2,2}` is written with
    /// fewer type parameters than its resolved struct declares (e.g. the
    /// 3-parameter `SMatrix{M,N,T}`), leaving trailing parameters unbound
    /// (Issue #8090). Such a written type is incomplete for static dispatch: the
    /// concrete runtime value carries every parameter, so a method specialized on
    /// the full parameter set must be selected at runtime rather than rejected
    /// statically against the truncated type.
    fn parametric_constructor_has_unbound_trailing_params(
        &self,
        resolved_base_name: &str,
        function: &str,
    ) -> bool {
        let Some(declared) = self
            .shared_ctx
            .parametric_structs
            .get(resolved_base_name)
            .map(|def| def.def.type_params.len())
        else {
            return false;
        };
        let Some((_, type_args)) = crate::compile::parse_parametric_call(function) else {
            return false;
        };
        type_args.len() < declared
    }

    pub(in crate::compile) fn infer_julia_type(&self, expr: &Expr) -> JuliaType {
        match expr {
            Expr::Literal(lit, _) => {
                if let Some(inferred) = super::shared::infer_scalar_literal(lit) {
                    return inferred.julia_type();
                }
                match lit {
                    Literal::Array(_, _) => JuliaType::Array,
                    Literal::ArrayI64(_, _) => JuliaType::Array,
                    Literal::ArrayBool(_, _) => JuliaType::Array,
                    Literal::Struct(struct_name, _) => JuliaType::Struct(struct_name.clone()),
                    Literal::DataType(_) => JuliaType::DataType,
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
                    // Scalar literals are handled by infer_scalar_literal above.
                    Literal::Int(_)
                    | Literal::Int128(_)
                    | Literal::BigInt(_)
                    | Literal::BigFloat(_)
                    | Literal::Float(_)
                    | Literal::Float32(_)
                    | Literal::Float16(_)
                    | Literal::Str(_)
                    | Literal::Char(_)
                    | Literal::Bool(_)
                    | Literal::Nothing
                    | Literal::Missing
                    | Literal::Module(_) => {
                        unreachable!("scalar literal inference should handle {lit:?}")
                    }
                }
            }
            Expr::Var(name, _) => {
                // Builtin type constants must stay `Type{T}` for dispatch even
                // when local/global type maps also record them as DataType values.
                if is_builtin_type_name(name) {
                    return if let Some(resolved) = JuliaType::from_name(name) {
                        JuliaType::TypeOf(Box::new(resolved))
                    } else {
                        JuliaType::DataType
                    };
                }

                if !self.locals.contains_key(name) {
                    if let Some(type_name) = self.resolve_visible_type_object_name(name) {
                        return JuliaType::TypeOf(Box::new(JuliaType::from_name_or_struct(
                            &type_name,
                        )));
                    }
                }

                if self.declared_globals.contains(name) {
                    return JuliaType::Any;
                }

                if !self.locals.contains_key(name) {
                    if is_pi_name(name) {
                        return JuliaType::Struct("Irrational{:π}".to_string());
                    }
                    if is_euler_name(name) {
                        return JuliaType::Struct("Irrational{:ℯ}".to_string());
                    }
                }

                // First check julia_type_locals for parametric types (e.g., Tuple{Int64, Int64})
                // This preserves precise type information that ValueType cannot represent
                if let Some(jt) = self.julia_type_locals.get(name) {
                    return jt.clone();
                }

                // Bare abstract-numeric params (`x::Real`, `x::Number`, `x::Integer`, ...)
                // must report `Any` here so compile-time dispatch routes type-generic
                // calls (`zero`, `one`, `oneunit`, ...) through runtime dispatch on the
                // concrete value, matching the untyped (`f(x)=zero(x)`) and
                // `where {T<:Real}` forms. The annotation widens `x` to `ValueType::F64`
                // (Real/Number) or `ValueType::I64` (Integer) in `self.locals`, which
                // would otherwise make `infer_julia_type` report `Float64`/`Int64` and
                // statically bind `zero(x)` to `zero(x::Float64)`/`zero(x::Int64)` — so
                // `f(3)` ran the Float64 body and erred ("expected I64, got Float64") or
                // silently widened the result. The variable already loads via `LoadAny`
                // for these params, so reporting `Any` keeps the dispatch decision
                // consistent with the runtime representation (Issue #5076).
                if self.abstract_numeric_params.contains(name) {
                    return JuliaType::Any;
                }

                // Fall back to locals/global_types for ValueType-based lookup
                let var_type = self
                    .locals
                    .get(name)
                    .or_else(|| self.shared_ctx.global_types.get(name));
                match var_type {
                    Some(ValueType::I64) => JuliaType::Int64,
                    Some(ValueType::F64) => JuliaType::Float64,
                    Some(ValueType::ComplexF32) => {
                        JuliaType::Struct("Complex{Float32}".to_string())
                    }
                    Some(ValueType::ComplexF64) => {
                        JuliaType::Struct("Complex{Float64}".to_string())
                    }
                    Some(ValueType::Array) => JuliaType::Array,
                    Some(ValueType::ArrayOf(ref elem_type, ref array_ndims)) => {
                        // Convert ArrayElementType to JuliaType for proper Vector{T} dispatch
                        use crate::vm::ArrayElementType;
                        let julia_elem = match elem_type {
                            ArrayElementType::I8 => JuliaType::Int8,
                            ArrayElementType::I16 => JuliaType::Int16,
                            ArrayElementType::I32 => JuliaType::Int32,
                            ArrayElementType::I64 => JuliaType::Int64,
                            ArrayElementType::I128 => JuliaType::Int128,
                            ArrayElementType::U8 => JuliaType::UInt8,
                            ArrayElementType::U16 => JuliaType::UInt16,
                            ArrayElementType::U32 => JuliaType::UInt32,
                            ArrayElementType::U64 => JuliaType::UInt64,
                            ArrayElementType::U128 => JuliaType::UInt128,
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
                            // SubString{String}: share dispatch with String — only the
                            // display tag differs (Issue #3574).
                            ArrayElementType::SubString => JuliaType::String,
                            ArrayElementType::Char => JuliaType::Char,
                            ArrayElementType::Symbol => JuliaType::Symbol,
                            ArrayElementType::Nothing => JuliaType::Nothing,
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
                            ArrayElementType::UnionOf(ref members) => {
                                // Issue #3549: keep Union{...} eltype name for dispatch.
                                // Issue #6720: render the structured members back
                                // to the display body (order preserved).
                                if members.is_empty() {
                                    JuliaType::Bottom
                                } else {
                                    JuliaType::Struct(format!(
                                        "Union{{{}}}",
                                        ArrayElementType::union_body_string(members)
                                    ))
                                }
                            }
                            ArrayElementType::Abstract(ref name) => {
                                JuliaType::from_name_or_struct(name)
                            }
                        };
                        // Issue #6817: respect the array rank so a 2-D `Matrix`
                        // variable dispatches to `::Matrix`, not `::Vector`.
                        // When the element is unknown (`Any`) but the rank is
                        // known (e.g. a comprehension), report the element-free
                        // bare alias (`Matrix`/`Vector`) so the rank matches
                        // `::Matrix`/`::Vector` while element-specific methods
                        // (`::Matrix{Int64}`) fall to runtime dispatch on the
                        // concrete value rather than producing a spurious
                        // `Matrix{Any}` ambiguity. Unknown rank keeps `Vector{T}`.
                        let elem_unknown = julia_elem == JuliaType::Any;
                        match array_ndims {
                            Some(2) if elem_unknown => JuliaType::Struct("Matrix".to_string()),
                            Some(2) => JuliaType::MatrixOf(Box::new(julia_elem)),
                            Some(n) if *n >= 3 => {
                                JuliaType::Struct(format!("Array{{{}, {}}}", julia_elem.name(), n))
                            }
                            Some(1) if elem_unknown => JuliaType::Struct("Vector".to_string()),
                            _ => JuliaType::VectorOf(Box::new(julia_elem)),
                        }
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
                    // Memory participates in Base's GenericMemory family. Preserve
                    // the parametric type for method dispatch instead of erasing it
                    // to Any, so `count(f, ::Memory)` does not fall into String
                    // overloads.
                    Some(ValueType::Memory) => JuliaType::Struct("Memory".to_string()),
                    Some(ValueType::MemoryOf(elem_type)) => {
                        JuliaType::Struct(format!("Memory{{{}}}", elem_type.julia_type_name()))
                    }
                    None => {
                        // ============================================================
                        // TYPE INFERENCE PRIORITY ORDER (Issue #1692, #1701)
                        // ============================================================
                        //
                        // IMPORTANT: The order of checks in this branch is critical!
                        // Changing the order can break type dispatch for various scenarios.
                        // Builtin type names are checked before ValueType lookup above
                        // so global DataType entries do not erase Type{T} dispatch.
                        //
                        // Priority order (highest to lowest):
                        //   1. Special constants (pi, ℯ)
                        //   2. Type parameters from where clause (T, S, etc.)
                        //   3. User-defined struct types (as Type{T})
                        //   3.5. User-defined abstract types (as Type{T})
                        //   4. Function names (names in method_tables)
                        //   4.5. Builtin function names (is_base_function)
                        //   5. Global const types from shared_ctx.global_types (Issue #3088)
                        //   6. Fallback to Any
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

                        // Priority 1.1: Float64 math constants (NaN, Inf).
                        // Mirrors the ValueType inference in `infer/mod.rs` (which
                        // already returns ValueType::F64 for NaN/Inf), so that
                        // array literals like `[NaN, NaN]` infer as
                        // `VectorOf(Float64)` instead of `VectorOf(Any)`.
                        // Without this, dispatch on `Vector{Float64}` becomes
                        // ambiguous in the presence of other `Vector{T}` methods
                        // (Issue #3580 follow-up).
                        if name == "NaN" || name == "Inf" || name == "NaN64" || name == "Inf64" {
                            return JuliaType::Float64;
                        }
                        if name == "NaN32" || name == "Inf32" {
                            return JuliaType::Float32;
                        }
                        if name == "NaN16" || name == "Inf16" {
                            return JuliaType::Float16;
                        }

                        // Priority 2: Type parameters from where clause
                        if let Some(tp) = self
                            .current_type_param_index
                            .get(name.as_str())
                            .and_then(|&idx| self.current_type_params.get(idx))
                        {
                            // Value type parameters (`Val{N}`, `NTuple{N,T}`,
                            // array ranks, etc.) materialize as ordinary values
                            // in the method body. Keep JuliaType inference in
                            // sync with `compile_expr(Expr::Var)`'s LoadAny path
                            // so macro-expanded comparisons such as
                            // `@assert N ≥ 2` dispatch as `Int64 >= Int64`
                            // instead of `DataType >= Int64` (Issue #8328).
                            if self.val_type_params.contains(name) {
                                return JuliaType::Int64;
                            }
                            if self.val_bool_params.contains(name) {
                                return JuliaType::Bool;
                            }
                            if self.val_symbol_params.contains(name) {
                                return JuliaType::Symbol;
                            }
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

                        // Priority 4.1: User-defined PARAMETRIC struct types
                        // (Issue #7247). A parametric struct (`struct Foo{T} ... end`)
                        // lives in `parametric_structs`, not `struct_table`. Its bare
                        // name is still a first-class type object (`Type{Foo}`), but
                        // when the struct also declares custom OUTER constructors
                        // (`Foo(a::Real) = ...`) those register `Foo` in
                        // `method_tables`, so without this arm the bare reference fell
                        // through to Priority 5 and was mis-typed as the constructor
                        // function (`typeof(Foo)`). That made `ff(Foo, x)` fail to
                        // match a `ff(::Type{Foo}, x)` method (resolving instead to
                        // `ff(::typeof(Foo), x)`), exactly like the non-parametric
                        // struct arm above which must precede `method_tables` for the
                        // same reason. Mirrors the bare-name resolution order in
                        // `compile/expr/mod.rs` (parametric struct before function).
                        if self.shared_ctx.parametric_structs.contains_key(name) {
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

                        // Priority 5: Function names in method_tables.
                        //
                        // Julia dispatch sees a named function value as its singleton
                        // type (`typeof(identity)`, `typeof(+)`, ...), which is a subtype
                        // of Function. Preserve that shape so callable-specific methods
                        // like `map(::typeof(identity), xs)` can beat generic
                        // `map(f::Function, xs)` while still matching Function fallbacks.
                        if self.method_tables.contains_key(name) {
                            return JuliaType::Struct(format!("typeof({})", name));
                        }

                        // Priority 5.5: Builtin function names (Issue #2070)
                        // Builtins like uppercase, lowercase, etc. are not in method_tables
                        // but should still carry their callable singleton type for
                        // dispatch. The CoreType subtype relation keeps them compatible
                        // with `::Function` methods.
                        if is_base_function(name) {
                            return JuliaType::Struct(format!("typeof({})", name));
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
                        } else if lt.is_builtin_numeric() && rt.is_builtin_numeric() {
                            // Issue #5205: preserve narrow-integer result types
                            // (Int8 + Int8 -> Int8, UInt16 + UInt16 -> UInt16, ...) via the
                            // centralized promotion registry, matching upstream Julia and the
                            // ValueType inference path (promote_numeric_value_types). Without
                            // this, narrow-int arithmetic collapsed to Int64, so chained
                            // expressions like `a + b + c` mis-dispatched the outer `+` as
                            // (Int64, Int8) through the +(::Number, ::Number) promotion
                            // fallback. That widened the value to Int64 and then routed it
                            // through a now range-checked convert back to Int8 (Issue #5192),
                            // throwing InexactError instead of wrapping (modular) like
                            // upstream's native narrow-int `+`.
                            JuliaType::from_name_or_struct(&promote_type(
                                &lt.to_string(),
                                &rt.to_string(),
                            ))
                        } else {
                            JuliaType::Int64
                        }
                    }
                }
            }
            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                // Infer element type and rank for proper Array alias dispatch.
                // Matrix literals such as `[1.0 0.0; 0.0 1.0]` lower to the
                // same Expr variant as vectors, but carry a 2-D shape.
                if elements.is_empty() {
                    JuliaType::Array
                } else {
                    let first_elem_type = self.infer_julia_type(&elements[0]);
                    // Check if all elements have the same type
                    let all_same = elements
                        .iter()
                        .skip(1)
                        .all(|e| self.infer_julia_type(e) == first_elem_type);
                    let elem_type = if all_same {
                        first_elem_type
                    } else {
                        JuliaType::Any
                    };
                    julia_array_type_for_ndims(elem_type, shape.len())
                }
            }
            Expr::TypedEmptyArray { element_type, .. } => {
                JuliaType::VectorOf(Box::new(JuliaType::from_name_or_struct(element_type)))
            }
            Expr::Range { start, step, .. } => {
                // Char ranges (`'a':'e'`) are always StepRange in
                // upstream Julia, even when the step is implicit (1),
                // because `:` over non-numeric types defaults to the
                // explicit-step form. Reporting UnitRange for the
                // step-less case caused show dispatch to route to
                // `show(::UnitRange)` and drop the step from the
                // output (Issue #4830, follow-up to #4795).
                if matches!(self.infer_julia_type(start), JuliaType::Char) {
                    JuliaType::StepRange
                } else if step.is_none() {
                    // UnitRange when step is None (or 1), StepRange otherwise
                    JuliaType::UnitRange
                } else {
                    JuliaType::StepRange
                }
            }
            // Issue #6817: a comprehension's rank equals its iterator-clause count
            // (`Vector` for 1 clause, `Matrix` for 2, `Array{T,N}` for N). The
            // element is left as `Any` (the rank is what governs `::Matrix` vs
            // `::Vector` dispatch; element-specific methods fall to runtime
            // dispatch on the concrete value).
            Expr::Comprehension { .. } => JuliaType::Struct("Vector".to_string()),
            // The whitespace flatten form is always a 1-D `Vector` regardless of
            // clause count; only the comma cartesian form is N-D (Issue #8014).
            Expr::MultiComprehension {
                iterations,
                flatten,
                ..
            } => {
                if *flatten {
                    JuliaType::Struct("Vector".to_string())
                } else {
                    match iterations.len() {
                        1 => JuliaType::Struct("Vector".to_string()),
                        2 => JuliaType::Struct("Matrix".to_string()),
                        n => JuliaType::Struct(format!("Array{{Any, {}}}", n)),
                    }
                }
            }
            Expr::Generator { .. } => {
                JuliaType::Any // Generator maps to Any for type dispatch
            }
            Expr::TupleLiteral { elements, .. } => {
                // Infer parametric tuple type for proper dispatch
                // e.g., (42, 10) -> Tuple{Int64, Int64}
                let elem_types: Vec<JuliaType> =
                    elements.iter().map(|e| self.infer_julia_type(e)).collect();
                JuliaType::TupleOf(elem_types)
            }
            Expr::Pair { key, value, .. } => JuliaType::Struct(format!(
                "Pair{{{},{}}}",
                self.infer_julia_type(key).name(),
                self.infer_julia_type(value).name()
            )),
            Expr::NamedTupleLiteral { fields, .. } => {
                // Infer the concrete named-tuple type `@NamedTuple{a::T1, b::T2}`
                // when every field's element type is statically known, so the
                // type-level `NamedTuple{(:a, :b)}` / `NamedTuple{(:a, :b),
                // Tuple{...}}` dispatch (Issue #5063) can match on field names.
                // This mirrors the runtime `typeof` string. If any field type is
                // not statically resolvable, fall back to the bare `NamedTuple`.
                let mut field_strs = Vec::with_capacity(fields.len());
                let mut all_known = true;
                for (name, value) in fields {
                    let field_ty = self.infer_julia_type(value);
                    match &field_ty {
                        JuliaType::Any => {
                            // Unknown / Any field: bail out to the bare form so we
                            // never claim a more precise type than we can prove.
                            all_known = false;
                            break;
                        }
                        // Upstream prints an `Any`-typed field as the bare name;
                        // here a concrete type is always emitted with `::`.
                        _ => field_strs.push(format!("{}::{}", name, field_ty.name())),
                    }
                }
                if all_known {
                    JuliaType::Struct(format!("@NamedTuple{{{}}}", field_strs.join(", ")))
                } else {
                    JuliaType::NamedTuple
                }
            }
            Expr::Index { array, indices, .. } => {
                // Each colon / range / integer-vector index contributes one
                // dimension to the result rank; scalar indices contribute none
                // (Issue #7333). `slice_dims > 0` is the old "is a slice" flag.
                let slice_dims = indices
                    .iter()
                    .filter(|idx| {
                        matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. })
                            || match self.infer_julia_type(idx) {
                                JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) => {
                                    is_integer_julia_type(&elem)
                                }
                                JuliaType::Array => true,
                                _ => false,
                            }
                    })
                    .count();
                let is_slice = slice_dims > 0;
                let array_ty = self.infer_julia_type(array);
                if array_ty == JuliaType::String {
                    return if is_slice {
                        JuliaType::String
                    } else {
                        JuliaType::Char
                    };
                }
                if is_slice {
                    // Recover the result rank from the slice dimensions so a
                    // sliced array carrier dispatches by rank like a literal
                    // array: `m[:, 1]`/`m[1, :]` -> Vector, `m[:, :]` -> Matrix
                    // (Issue #7333). Before this a slice pinned the
                    // unparameterized `Array` (rank unknown), so `m[:, 1]` failed
                    // to match `::Vector` methods even though `typeof`/`isa`
                    // reported `Vector`. The element type comes from the receiver;
                    // when it is unknown we report the bare `Vector`/`Matrix`
                    // alias (mirroring the `Expr::Var` rank logic above) so
                    // element-specific methods still fall to runtime dispatch on
                    // the concrete value rather than a spurious `Vector{Any}`
                    // match. Non-array receivers (tuples, ranges) keep the
                    // conservative `Array` fallback.
                    if is_array_family_receiver(&array_ty) {
                        let elem = array_element_from_julia_type(array_ty);
                        let elem_unknown = elem == JuliaType::Any;
                        return match slice_dims {
                            1 if elem_unknown => JuliaType::Struct("Vector".to_string()),
                            1 => JuliaType::VectorOf(Box::new(elem)),
                            2 if elem_unknown => JuliaType::Struct("Matrix".to_string()),
                            2 => JuliaType::MatrixOf(Box::new(elem)),
                            n => JuliaType::Struct(format!("Array{{{}, {}}}", elem.name(), n)),
                        };
                    }
                    return JuliaType::Array;
                }

                // Tuple/NamedTuple element-type sharpening (Issue #5183).
                // `t[k]` with a constant `k` over a statically known
                // `TupleOf`/concrete `@NamedTuple{...}` returns the precise
                // element type instead of `Any`, so multi-value returns stay
                // type-stable at the use site.
                if indices.len() == 1 {
                    if let Some(k) = super::shared::const_tuple_index(&indices[0]) {
                        let container = self.infer_julia_type(array);
                        if let Some(elem) = super::shared::tuple_element_julia_type(&container, k) {
                            return elem;
                        }
                    }
                }

                // Check array type from locals to get proper element type for struct arrays
                // This enables correct method dispatch for expressions like `imag(arr[1])`
                // where arr is an array of Complex structs
                if let Expr::Var(name, _) = array.as_ref() {
                    if let Some(ValueType::ArrayOf(ArrayElementType::StructOf(type_id), _)) =
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
            Expr::QuoteLiteral { constructor, .. } => match constructor.as_ref() {
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    ..
                } => JuliaType::Symbol,
                Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    ..
                } => JuliaType::Expr,
                Expr::Builtin {
                    name: BuiltinOp::QuoteNodeNew,
                    ..
                } => JuliaType::QuoteNode,
                Expr::Builtin {
                    name: BuiltinOp::LineNumberNodeNew,
                    ..
                } => JuliaType::LineNumberNode,
                _ => self.infer_julia_type(constructor),
            },
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                if matches!(function.as_str(), "Broadcasted" | "Base.Broadcasted") {
                    return JuliaType::Struct("Broadcasted".to_string());
                }
                if function == "materialize" && super::is_broadcasted_comparison_call(args.first())
                {
                    return JuliaType::VectorOf(Box::new(JuliaType::Bool));
                }

                if super::shared::is_truncated_result_call(function, args, kwargs) {
                    return JuliaType::Struct("Distributions.Truncated".to_string());
                }

                if let Some(folded) = super::shared::folded_nary_operator_call(function, args) {
                    return self.infer_julia_type(&folded);
                }

                if let Some(memory_ty) = super::shared::memory_constructor_julia_type(function) {
                    return memory_ty;
                }
                if matches!(function.as_str(), "merge" | "Base.merge") {
                    if let Some(ty) = self.static_named_tuple_merge_julia_type(args) {
                        return ty;
                    }
                }
                if matches!(function.as_str(), "copy" | "Base.copy")
                    && args.len() == 1
                    && matches!(self.infer_julia_type(&args[0]), JuliaType::Dict)
                {
                    return JuliaType::Dict;
                }
                // `copy(s::Set{T})` returns a fresh `Set{T}` (Issue #6721). The
                // parametric `copy(::Set{T})` method's interprocedural return
                // infers to Any, which would make a following `push!(c, x)` take
                // the Array fast path; pin the Set struct type from the argument.
                if matches!(function.as_str(), "copy" | "Base.copy") && args.len() == 1 {
                    let arg_type = self.infer_julia_type(&args[0]);
                    if matches!(&arg_type, JuliaType::Struct(name) if name.split('{').next() == Some("Set"))
                    {
                        return arg_type;
                    }
                }
                if matches!(function.as_str(), "values" | "Base.values") && args.len() == 1 {
                    let arg_type = self.infer_julia_type(&args[0]);
                    if matches!(arg_type, JuliaType::NamedTuple)
                        || matches!(&arg_type, JuliaType::Struct(name) if {
                            let base_name = name.split('{').next().unwrap_or(name.as_str());
                            matches!(base_name, "@NamedTuple" | "NamedTuple")
                        })
                    {
                        return JuliaType::Tuple;
                    }
                }
                if matches!(function.as_str(), "filter" | "Base.filter") && args.len() == 2 {
                    let arg_type = self.infer_julia_type(&args[1]);
                    if matches!(&arg_type, JuliaType::Struct(name) if name.split('{').next() == Some("Dict"))
                    {
                        return arg_type;
                    }
                    if matches!(arg_type, JuliaType::Dict) {
                        return JuliaType::Dict;
                    }
                }
                if matches!(function.as_str(), "pairs" | "Base.pairs") && args.len() == 1 {
                    let arg_type = self.infer_julia_type(&args[0]);
                    if matches!(&arg_type, JuliaType::Struct(name) if name.split('{').next() == Some("Dict"))
                    {
                        return arg_type;
                    }
                    if matches!(arg_type, JuliaType::Dict) {
                        return JuliaType::Dict;
                    }
                    if matches!(
                        arg_type,
                        JuliaType::Array
                            | JuliaType::VectorOf(_)
                            | JuliaType::MatrixOf(_)
                            | JuliaType::Tuple
                            | JuliaType::TupleOf(_)
                            | JuliaType::NamedTuple
                    ) || matches!(&arg_type, JuliaType::Struct(name) if {
                        let base_name = name.split('{').next().unwrap_or(name.as_str());
                        matches!(
                            base_name,
                            "Array" | "Vector" | "Matrix" | "Memory" | "Tuple" | "NamedTuple"
                        )
                    }) {
                        return JuliaType::Struct("Pairs".to_string());
                    }
                    return JuliaType::Any;
                }
                // `filter(pred, coll)` only drops entries, so the container type
                // is preserved. Propagate the receiver's dict/set type so a
                // `filtered = filter(p, d)` binding keeps the same parametric
                // `JuliaType` as the dict it came from. Without this the result
                // widened to `Any`, so collection-mutation routing demoted
                // `empty!(filtered)` to a legacy `DictEmpty` boundary instead of
                // native struct-backed dispatch (Issue #6672).
                if matches!(function.as_str(), "filter" | "Base.filter") && args.len() == 2 {
                    let receiver = self.infer_julia_type(&args[1]);
                    if matches!(receiver, JuliaType::Dict)
                        || matches!(&receiver, JuliaType::Struct(name) if {
                            let short = name.rsplit('.').next().unwrap_or(name);
                            let base = short.split('{').next().unwrap_or(short);
                            matches!(base, "Dict" | "Set")
                        })
                    {
                        return receiver;
                    }
                }
                if function == "Generator" || function == "Base.Generator" {
                    return JuliaType::Generator;
                }
                // `IteratorSize(x)` over a statically-ranked array returns the
                // concrete `HasShape{N}` (upstream
                // `IteratorSize(::AbstractArray{T,N}) = HasShape{N}()`), so a
                // downstream `_collect(cont, itr, ::HasEltype, isz)` statically
                // dispatches to the more-specific `::HasShape` method rather than the
                // `::IteratorSize` catch-all (Issue #5850). The qualified
                // `Base.IteratorSize` form is handled in the ModuleCall arm.
                if matches!(function.as_str(), "IteratorSize" | "Base.IteratorSize")
                    && args.len() == 1
                {
                    if let Some(n) = ndims_from_array_julia_type(&self.infer_julia_type(&args[0])) {
                        return JuliaType::Struct(format!("HasShape{{{n}}}"));
                    }
                }
                if function.starts_with("NamedTuple{") && function.ends_with('}') {
                    if let Some(ty) = self.named_tuple_constructor_julia_type(function, args) {
                        return ty;
                    }
                }
                if function == "reshape" || function == "Base.reshape" {
                    if let Some(ndims) = reshape_rank_from_args(args) {
                        let receiver_type = args
                            .first()
                            .map(|array| self.infer_julia_type(array))
                            .unwrap_or(JuliaType::Any);
                        if matches!(
                            receiver_type,
                            JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
                        ) || matches!(&receiver_type, JuliaType::Struct(name) if {
                            let base = name.split('{').next().unwrap_or(name);
                            matches!(base, "Array" | "Vector" | "Matrix")
                        }) {
                            let elem_type = array_element_from_julia_type(receiver_type);
                            return julia_array_type_for_ndims(elem_type, ndims);
                        }
                    }
                }

                // Lowercase `complex` resolves through the shared registry's
                // contextual tfunc, with the legacy Complex{Float64} result
                // pinned as the fallback (Issue #5922).
                if function == "complex" {
                    let ids = super::expr_tfuncs::SharedCtxStructIds(&*self.shared_ctx);
                    return super::expr_tfuncs::infer_julia_complex_call(&ids);
                }
                if function == "getindex" && args.len() == 2 {
                    let collection_type = self.infer_julia_type(&args[0]);
                    if collection_type == JuliaType::String {
                        let is_slice =
                            matches!(&args[1], Expr::Range { .. } | Expr::SliceAll { .. });
                        return if is_slice {
                            JuliaType::String
                        } else {
                            JuliaType::Char
                        };
                    }
                }
                if super::expr_tfuncs::is_array_constructor_call(function) {
                    if let Some(inferred) = super::expr_tfuncs::infer_julia_array_constructor_call(
                        function,
                        args,
                        |arg| self.infer_julia_type(arg),
                    ) {
                        return inferred;
                    }
                }
                if let Some(inferred) =
                    super::expr_tfuncs::infer_julia_type_call(function, args, |arg| {
                        self.infer_julia_type(arg)
                    })
                {
                    return inferred;
                }
                // collect/rand/randn and the fixed-result helpers (isequal,
                // IOBuffer, hash/fld/cld, date accessors, big, trues/falses)
                // are routed through expr_tfuncs above — Issue #5922.
                //
                // Public Dict construction routes through the pure-Julia
                // `Dict{K,V}` struct methods (Issue #6619).
                if let Some(inferred) = super::expr_tfuncs::infer_julia_dict_constructor_call(
                    function,
                    args,
                    &mut |arg| self.infer_julia_type(arg),
                ) {
                    return inferred;
                }
                // Public Set construction routes through the pure-Julia Set{T}
                // struct (over Dict{T,Nothing}); `Set([...])` infers as
                // `Set{eltype}` so user `Set{T}` methods dispatch (Issue #6721).
                if let Some(inferred) = super::expr_tfuncs::infer_julia_set_constructor_call(
                    function,
                    args,
                    &mut |arg| self.infer_julia_type(arg),
                ) {
                    return inferred;
                }
                if matches!(function.as_str(), "view" | "Base.view") {
                    let julia_args = args
                        .iter()
                        .map(|arg| self.infer_julia_type(arg))
                        .collect::<Vec<_>>();
                    if let Some(inferred) =
                        super::expr_tfuncs::infer_julia_view_call(function, &julia_args)
                    {
                        return inferred;
                    }
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
                } else if function == "enumerate" {
                    // enumerate(iter) returns Enumerate{typeof(iter)}
                    // Use Enumerate{Any} since we don't track concrete inner type
                    JuliaType::Struct("Enumerate{Any}".to_string())
                } else if function == "zip" {
                    // zip returns Zip/Zip3/... depending on arity (Issues #1990/#4281)
                    match args.len() {
                        3 => JuliaType::Struct("Zip3{Any, Any, Any}".to_string()),
                        4 => JuliaType::Struct("Zip4{Any, Any, Any, Any}".to_string()),
                        5 => JuliaType::Struct("Zip5{Any, Any, Any, Any, Any}".to_string()),
                        6 => JuliaType::Struct("Zip6{Any, Any, Any, Any, Any, Any}".to_string()),
                        7 => {
                            JuliaType::Struct("Zip7{Any, Any, Any, Any, Any, Any, Any}".to_string())
                        }
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
                } else if function == "rest" && args.len() == 2 {
                    // rest(iter, state) returns Rest{typeof(iter), typeof(state)}
                    // Use Any parameters since the local inference lattice does not
                    // retain concrete iterator state types here.
                    JuliaType::Struct("Rest{Any, Any}".to_string())
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
                    if let Some(resolved_base_name) = self.resolve_parametric_struct_name(base_name)
                    {
                        // Issue #8090: when a parametric constructor is written with
                        // FEWER type parameters than the struct declares (e.g.
                        // `SMatrix{2,2}` for the 3-parameter `SMatrix{M,N,T}`), the
                        // trailing parameters are left unbound and inferred from the
                        // arguments at construction time. The runtime value is fully
                        // concrete (`SMatrix{2,2,Float64}`), so a method specialized on
                        // the full parameter set (`f(::SMatrix{N,N,T})`) can only be
                        // selected at runtime. Reporting the truncated static type here
                        // made a directly-nested constructor argument fail static
                        // dispatch with no runtime fallback, whereas binding the result
                        // to a local first worked (its slot widens to `Any`, routing to
                        // runtime dispatch). Widen to `Any` so the nested call routes to
                        // runtime multiple dispatch on the concrete value, matching the
                        // bound-local path.
                        if self.parametric_constructor_has_unbound_trailing_params(
                            &resolved_base_name,
                            function,
                        ) {
                            return JuliaType::Any;
                        }
                        // This is a parametric struct instantiation - use the full name as type
                        let type_args = &function[function.find('{').unwrap()..];
                        JuliaType::Struct(format!("{}{}", resolved_base_name, type_args))
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
                        for (idx, (field_name, field_ty)) in info.fields.iter().enumerate() {
                            if field_name == field {
                                if let Some(field_julia_type) = self
                                    .shared_ctx
                                    .field_julia_types_by_type_id(info.type_id)
                                    .and_then(|field_types| field_types.get(idx))
                                    .filter(|field_type| **field_type != JuliaType::Any)
                                {
                                    return field_julia_type.clone();
                                }
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
                    UnaryOp::Not if self.julia_unary_not_operand_is_callable(operand) => {
                        JuliaType::Function
                    }
                    UnaryOp::Not => JuliaType::Bool,
                    _ => self.infer_julia_type(operand), // Neg, Pos preserve operand type
                }
            }
            Expr::Builtin { name, args, .. } => {
                // Infer JuliaType for builtin operations
                match name {
                    BuiltinOp::TypeOf => {
                        // Lowering represents a static parametric type expression such
                        // as `Complex{Float64}` as TypeOf("Complex{Float64}"), and the
                        // VM turns that string into the DataType value. For method
                        // dispatch the value's singleton type is Type{Complex{Float64}},
                        // matching Julia's treatment of type objects.
                        if let [Expr::Literal(Literal::Str(type_name), _)] = args.as_slice() {
                            if type_name.contains('{') && type_name.ends_with('}') {
                                return JuliaType::TypeOf(Box::new(
                                    JuliaType::from_name_or_struct(type_name),
                                ));
                            }
                        }
                        JuliaType::DataType
                    }
                    BuiltinOp::Supertype => JuliaType::DataType,
                    BuiltinOp::Typename | BuiltinOp::FunctionName => JuliaType::Symbol,
                    BuiltinOp::Isa
                    | BuiltinOp::HasKey
                    | BuiltinOp::Isbitstype
                    // Isbits, Hasfield, Ismutable removed - pure Julia (Issue #6738)
                    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                    // removed - now Pure Julia (base/reflection.jl)
                    => JuliaType::Bool,
                    BuiltinOp::Length
                    | BuiltinOp::TimeNs
                    | BuiltinOp::DictGet
                    | BuiltinOp::Sizeof => JuliaType::Int64,
                    // `rand(n)`/`randn(n, m)` recover a rank-aware
                    // `Vector`/`Matrix` so a native-array carrier dispatches like
                    // a literal `Vector` (Issue #7307); bare `rand()`/`randn()`
                    // and collection/RNG forms stay scalar `Float64`.
                    BuiltinOp::Rand | BuiltinOp::Randn => {
                        super::expr_tfuncs::infer_rand_array_julia_type_for(
                            matches!(name, BuiltinOp::Randn),
                            args,
                            |arg| self.infer_julia_type(arg),
                        )
                        .unwrap_or(JuliaType::Float64)
                    }
                    BuiltinOp::Sqrt => JuliaType::Float64,
                    BuiltinOp::Zeros
                    | BuiltinOp::Ones => JuliaType::Array,
                    BuiltinOp::Reshape => reshape_rank_from_args(args)
                        .map(|ndims| {
                            let elem_type = args
                                .first()
                                .map(|array| {
                                    array_element_from_julia_type(self.infer_julia_type(array))
                                })
                                .unwrap_or(JuliaType::Any);
                            julia_array_type_for_ndims(elem_type, ndims)
                        })
                        .unwrap_or(JuliaType::Array),
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
                    // `first(t)`/`last(t)` over a statically known tuple type
                    // returns the precise first/last element type (Issue #5183),
                    // keeping multi-value-return helpers type-stable for dispatch.
                    BuiltinOp::TupleFirst | BuiltinOp::TupleLast => args
                        .first()
                        .and_then(|arg| {
                            self.tuple_first_last_julia_type(
                                arg,
                                matches!(name, BuiltinOp::TupleLast),
                            )
                        })
                        .unwrap_or(JuliaType::Any),
                    BuiltinOp::DictValues if args.len() == 1 => {
                        let arg_type = self.infer_julia_type(&args[0]);
                        if matches!(arg_type, JuliaType::NamedTuple)
                            || matches!(&arg_type, JuliaType::Struct(name) if {
                                let base_name = name.split('{').next().unwrap_or(name.as_str());
                                matches!(base_name, "@NamedTuple" | "NamedTuple")
                            })
                        {
                            JuliaType::Tuple
                        } else {
                            JuliaType::Any
                        }
                    }
                    _ => JuliaType::Any,
                }
            }
            Expr::DynamicTypeConstruct {
                base,
                type_args,
                splat_mask,
                ..
            } if splat_mask.iter().all(|is_splat| !is_splat) => {
                let params = type_args
                    .iter()
                    .map(|arg| match self.infer_julia_type(arg) {
                        JuliaType::TypeOf(inner) => Some(inner.name().to_string()),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(params) = params {
                    let type_name = if params.is_empty() {
                        base.clone()
                    } else {
                        format!("{}{{{}}}", base, params.join(","))
                    };
                    JuliaType::TypeOf(Box::new(JuliaType::from_name_or_struct(&type_name)))
                } else {
                    JuliaType::DataType
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
                    return JuliaType::Generator;
                }
                if resolved_module == "Base" && function == "IteratorSize" && args.len() == 1 {
                    if let Some(n) = ndims_from_array_julia_type(&self.infer_julia_type(&args[0])) {
                        return JuliaType::Struct(format!("HasShape{{{n}}}"));
                    }
                }

                // A module-qualified constructor call (`M.Norm(0.0)`) builds the
                // struct, so its return type is the constructed struct type — not
                // `Any`. The plain (unqualified) `Expr::Call` arm already recognizes
                // this via `resolve_struct_name` / `resolve_parametric_struct_name`,
                // but the `ModuleCall` arm previously fell straight through to the
                // method-table lookup (which finds the constructor but records a
                // `-> Any` return type) and then to the `Any` fallback. That made a
                // nested qualified call (`M.onearg(M.Norm(0.0))`) infer the inner
                // argument as `Any`, so the qualified outer dispatch could not match
                // a `onearg(::Dist)` / `onearg(::Norm)` method and failed at compile
                // time with `NoMethodFound{arg_types: [Any]}` (Issue #7235 sub-case 3,
                // the qualified-access part). Binding the constructor result to a
                // local first already worked; this closes the inline-argument gap.
                if let Some(resolved_name) = self.resolve_struct_name(function) {
                    return JuliaType::Struct(resolved_name);
                }
                if let Some(resolved_name) = self.resolve_parametric_struct_name(function) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    if let Ok(type_args) = self.shared_ctx.infer_type_args(function, &arg_types) {
                        if !type_args.is_empty() {
                            let type_arg_names: Vec<String> =
                                type_args.iter().map(|t| t.name().to_string()).collect();
                            return JuliaType::Struct(format!(
                                "{}{{{}}}",
                                resolved_name,
                                type_arg_names.join(", ")
                            ));
                        }
                    }
                    return JuliaType::Struct(resolved_name);
                }

                // Look up the method table for this function and infer return type
                if let Some(table) = self.method_tables.get(function.as_str()) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    if let Ok(method) = table.dispatch(&arg_types) {
                        return self.value_type_to_julia_type(&method.return_type);
                    }
                }
                // Fallback: check module_functions mapping
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
            // Function values carry Julia's callable singleton type. The shared
            // type lattice treats `typeof(f)` structs as subtypes of Function, so
            // generic HOF methods still match while callable-specific methods can win.
            Expr::FunctionRef { name, .. } => JuliaType::Struct(format!("typeof({})", name)),
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
                        if func.name == *var_name && var_name.starts_with("__partial_apply_") {
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
            ValueType::ComplexF32 => JuliaType::Struct("Complex{Float32}".to_string()),
            ValueType::ComplexF64 => JuliaType::Struct("Complex{Float64}".to_string()),
            ValueType::Bool => JuliaType::Bool,
            // ArrayOf maps to Array for dispatch (element type tracked separately).
            // Issue #6817: a known multi-dimensional rank projects to the rank
            // alias (`Matrix` / `Array{T,N}`) so a 2-D comprehension dispatches to
            // `::Matrix`; unknown/1-D ranks keep the historical bare `Array`.
            ValueType::ArrayOf(elem, Some(2)) => JuliaType::MatrixOf(Box::new(
                crate::vm::value::array_element_type_to_julia_type(elem),
            )),
            ValueType::ArrayOf(elem, Some(n)) if *n >= 3 => JuliaType::Struct(format!(
                "Array{{{}, {}}}",
                crate::vm::value::array_element_type_to_julia_type(elem).name(),
                n
            )),
            ValueType::ArrayOf(_, _) => JuliaType::Array,
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
            ValueType::Memory => JuliaType::Struct("Memory".to_string()),
            ValueType::MemoryOf(elem_type) => {
                JuliaType::Struct(format!("Memory{{{}}}", elem_type.julia_type_name()))
            }
        }
    }

    fn julia_unary_not_operand_is_callable(&self, operand: &Expr) -> bool {
        match operand {
            Expr::FunctionRef { .. } => true,
            Expr::Var(name, _)
                if self
                    .locals
                    .get(name)
                    .or_else(|| self.shared_ctx.global_types.get(name))
                    == Some(&ValueType::Function) =>
            {
                true
            }
            Expr::Var(name, _) if !self.locals.contains_key(name) => {
                self.method_tables.contains_key(name)
                    || is_base_function(name)
                    || self.function_aliases.contains_key(name)
                    || (self.usings.contains("Random") && is_random_function(name))
            }
            _ => false,
        }
    }

    fn static_named_tuple_merge_julia_type(&self, args: &[Expr]) -> Option<JuliaType> {
        if args.is_empty() {
            return None;
        }
        let mut arg_fields = Vec::with_capacity(args.len());
        for arg in args {
            arg_fields.push(static_named_tuple_fields_from_julia_type(
                &self.infer_julia_type(arg),
            )?);
        }
        let merged = merge_static_named_tuple_fields(&arg_fields);
        let fields = merged
            .into_iter()
            .map(|field| match field.type_name {
                Some(ty) => format!("{}::{}", field.name, ty),
                None => field.name,
            })
            .collect::<Vec<_>>();
        Some(JuliaType::Struct(format!(
            "@NamedTuple{{{}}}",
            fields.join(", ")
        )))
    }

    fn named_tuple_constructor_julia_type(
        &self,
        function: &str,
        args: &[Expr],
    ) -> Option<JuliaType> {
        if args.len() != 1 {
            return None;
        }
        let inner = function
            .strip_prefix("NamedTuple{")?
            .strip_suffix('}')?
            .trim();
        let params = split_named_tuple_constructor_params(inner);
        let names = parse_named_tuple_constructor_names(params.first()?.trim())?;
        if names.is_empty() {
            return Some(JuliaType::Struct("@NamedTuple{}".to_string()));
        }

        let field_types = if let Some(types_param) = params.get(1) {
            parse_named_tuple_constructor_tuple_types(types_param.trim())?
        } else if let Expr::TupleLiteral { elements, .. } = &args[0] {
            if elements.len() != names.len() {
                return None;
            }
            elements
                .iter()
                .map(|element| self.infer_julia_type(element).name().into_owned())
                .collect()
        } else {
            return None;
        };
        if field_types.len() != names.len() {
            return None;
        }
        let fields = names
            .into_iter()
            .zip(field_types)
            .map(|(name, ty)| {
                if ty == "Any" {
                    name
                } else {
                    format!("{name}::{ty}")
                }
            })
            .collect::<Vec<_>>();
        Some(JuliaType::Struct(format!(
            "@NamedTuple{{{}}}",
            fields.join(", ")
        )))
    }
}

#[derive(Clone)]
struct StaticNamedTupleField {
    name: String,
    type_name: Option<String>,
}

fn static_named_tuple_fields_from_julia_type(ty: &JuliaType) -> Option<Vec<StaticNamedTupleField>> {
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
            let (name, type_name) = field
                .split_once("::")
                .map_or((field.trim(), None), |(name, ty)| {
                    (name.trim(), Some(ty.trim().to_string()))
                });
            if name.is_empty() {
                None
            } else {
                Some(StaticNamedTupleField {
                    name: name.to_string(),
                    type_name,
                })
            }
        })
        .collect()
}

fn merge_static_named_tuple_fields(
    arg_fields: &[Vec<StaticNamedTupleField>],
) -> Vec<StaticNamedTupleField> {
    let mut merged = Vec::<StaticNamedTupleField>::new();
    for fields in arg_fields {
        for field in fields {
            if let Some(existing) = merged
                .iter_mut()
                .find(|candidate| candidate.name == field.name)
            {
                *existing = field.clone();
            } else {
                merged.push(field.clone());
            }
        }
    }
    merged
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

fn split_named_tuple_constructor_params(inner: &str) -> Vec<&str> {
    let mut params = Vec::new();
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 => {
                let param = inner[start..idx].trim();
                if !param.is_empty() {
                    params.push(param);
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        params.push(last);
    }
    params
}

fn parse_named_tuple_constructor_names(param: &str) -> Option<Vec<String>> {
    let inner = param.strip_prefix('(')?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(|raw| raw.strip_prefix(':').map(str::to_string))
        .collect()
}

fn parse_named_tuple_constructor_tuple_types(param: &str) -> Option<Vec<String>> {
    let inner = param.strip_prefix("Tuple{")?.strip_suffix('}')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(
        split_static_named_tuple_fields(inner)
            .into_iter()
            .map(|ty| {
                JuliaType::from_name_or_struct(ty.trim())
                    .name()
                    .into_owned()
            })
            .collect(),
    )
}
