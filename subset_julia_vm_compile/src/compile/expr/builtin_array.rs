//! Array function compilation.
//!
//! Handles compilation of Julia array functions:
//! - length(collection): Get length of collection
//! - getindex(collection, indices...): Index into collection
//! - setindex!(collection, value, indices...): Indexed assignment

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::bytecode::{ArrayElementType, Instr, ValueType};
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::types::JuliaType;
use subset_julia_vm_bytecode::{
    bare_type_name_array_element_type, builtin_type_array_element_type,
};

use super::super::{err, internal_compile_error, is_builtin_type_name, CResult, CoreCompiler};

/// Element type for a typed array literal whose element type is a parametric
/// heap container / struct (`Pair{Int,Int}[...]`, `Dict{Int,Int}[...]`,
/// `Set{Int}[...]`, `MyStruct[...]`), a small `Union`, or an abstract type.
/// Mirrors the runtime `array_element_type_from_julia_type` mapping for the
/// boxed-storage cases so the literal stores its elements verbatim (Issues
/// #5233, #5143).
///
/// Recognizing the `Union{...}` and abstract (`Real`, `Number`, ...) cases here
/// is what routes `Union{Int64,Float64}[1, 2.5]` through the verbatim-store
/// branch in `compile_builtin_array`'s `getindex` arm. Without it the literal
/// fell through to the generic `getindex(::DataType, idx)` path which coerced
/// each `Float64` member to `Int64` before constructing the array, breaking
/// per-element multiple dispatch (Issue #5143).
fn heap_julia_type_array_element_type(jt: &JuliaType) -> Option<ArrayElementType> {
    match jt {
        // Small `Union{...}`: box into `Any` storage tagged with the rendered
        // union body so `eltype` reports `Union{...}` and the members keep
        // their own concrete runtime types.
        // Issue #6720: store the structured union members directly.
        JuliaType::Union(types) => Some(ArrayElementType::UnionOf(types.clone())),
        JuliaType::Bottom => Some(ArrayElementType::UnionOf(Vec::new())),
        // Abstract element types preserve `Vector{Real}` etc. via boxed storage.
        JuliaType::Number
        | JuliaType::Real
        | JuliaType::Integer
        | JuliaType::Signed
        | JuliaType::Unsigned
        | JuliaType::AbstractFloat => Some(ArrayElementType::Abstract(jt.name().to_string())),
        JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => {
            Some(ArrayElementType::Abstract(jt.name().to_string()))
        }
        JuliaType::Struct(name) => {
            let base = name.split('{').next().unwrap_or(name);
            match base {
                "Complex" => match name.as_str() {
                    "Complex{Float64}" | "ComplexF64" => Some(ArrayElementType::ComplexF64),
                    "Complex{Float32}" | "ComplexF32" => Some(ArrayElementType::ComplexF32),
                    _ => Some(ArrayElementType::Any),
                },
                "Pair" => Some(ArrayElementType::Abstract(name.clone())),
                // A `Union{...}` rendered as a struct name (alternate spelling
                // of `JuliaType::Union`) keeps its union eltype tag.
                "Union" if name.starts_with("Union{") && name.ends_with('}') => {
                    Some(ArrayElementType::union_from_body(&name[6..name.len() - 1]))
                }
                "Array" | "Vector" | "Matrix" if name.contains('{') => {
                    Some(ArrayElementType::Abstract(name.clone()))
                }
                // Dict/Set/other heap containers and user structs box into Any
                // (matching `array_element_type_from_julia_type`'s default).
                _ => Some(ArrayElementType::Any),
            }
        }
        _ => None,
    }
}

fn static_datatype_expr_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args,
            ..
        } => {
            let [Expr::Literal(Literal::Str(type_name), _)] = args.as_slice() else {
                return None;
            };
            Some(JuliaType::from_name_or_struct(type_name).name().to_string())
        }
        Expr::Literal(Literal::DataType(type_name), _) => {
            Some(JuliaType::from_name_or_struct(type_name).name().to_string())
        }
        Expr::Var(type_name, _) => {
            Some(JuliaType::from_name_or_struct(type_name).name().to_string())
        }
        _ => None,
    }
}

fn is_julia_array_like_type(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
    ) || matches!(ty, JuliaType::Struct(name)
        if name == "Array"
            || name.starts_with("Array{")
            || name.starts_with("Vector{")
            || name.starts_with("Matrix{"))
}

fn is_range_like_julia_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange => true,
        JuliaType::Struct(name) => {
            let base = name
                .split('{')
                .next()
                .unwrap_or(name.as_str())
                .rsplit('.')
                .next()
                .unwrap_or(name.as_str());
            matches!(
                base,
                "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange" | "OneTo" | "LogRange"
            )
        }
        JuliaType::UnionAll { body, .. } => is_range_like_julia_type(body),
        _ => false,
    }
}

impl CoreCompiler<'_> {
    /// Compile-time element type for an **inline typed-array literal** `T[a, b, ...]`.
    ///
    /// The non-empty literal is lowered to `Expr::Index { array: Var("T"), .. }`
    /// (i.e. `getindex(T, a, b, ...)`); `compile_builtin_array`'s `getindex` arm
    /// materializes it into a `Vector{T}` and returns `ValueType::ArrayOf(elem)`.
    /// Type inference (`infer_expr_type`) must agree, otherwise the `Index` arm
    /// reports `ValueType::Any` and downstream dispatch — notably
    /// `resolve_sprint_function_ref` for `sprint(show, T[...])` — fails to pick
    /// the `show(io::IO, ::Array)` overload and mis-renders the array as the
    /// `Vector{T}()` empty-constructor form (Issue #5241).
    ///
    /// Returns `Some(element_type)` only for the typed-literal shape: `array` is a
    /// bare type identifier (builtin numeric/Char/String/Symbol/Any, the heap
    /// containers `Pair`/`Dict`/`Set`, a parametric container, or a user struct)
    /// that is **not** a local variable. Local array variables are handled by the
    /// surrounding inference logic and must not reach here.
    /// Resolve a `JuliaType` element type for a typed-array literal, preserving a
    /// *user struct* element type as a `StructOf(type_id)` tag (Issue #7304). The
    /// free [`heap_julia_type_array_element_type`] boxes user structs into `Any`
    /// (it has no struct registry); this `&self` wrapper looks the struct name up
    /// in the compile context so `T[...]` reports `Vector{T}` for a user `struct`
    /// `T`. The `StructOf` tag is resolved back to the struct name by reflection.
    fn heap_julia_type_array_element_type_resolved(
        &self,
        jt: &JuliaType,
    ) -> Option<ArrayElementType> {
        let base = heap_julia_type_array_element_type(jt)?;
        if matches!(base, ArrayElementType::Any) {
            if let JuliaType::Struct(name) = jt {
                if let Some(info) = self.resolve_struct_info_scoped(name) {
                    if name.contains('{') {
                        let exact_name = self
                            .shared_ctx
                            .get_struct_name(info.type_id)
                            .unwrap_or_else(|| name.clone());
                        return Some(ArrayElementType::Abstract(exact_name));
                    }
                    return Some(ArrayElementType::StructOf(info.type_id));
                }
            }
        }
        Some(base)
    }

    /// `Vector{T}` / `Matrix{T}` annotation -> element type, when representable.
    /// Also used by typed-parameter registration (Issue #9133) so
    /// `a::Vector{Float64}` keeps its element type as `ArrayOf(F64, ...)`
    /// instead of widening to `Array`.
    ///
    /// Routes the struct case through [`Self::heap_julia_type_array_element_type_resolved`]
    /// (not the free [`heap_julia_type_array_element_type`]) so a user-struct
    /// element resolves to `StructOf(type_id)` instead of `Any` (Issue #9188).
    /// Without this, `Vector{UserStruct}` fields/annotations lost their element
    /// type at the `getindex`/`IndexLoad` boundary: `c.items[i]` for
    /// `items::Vector{Pt}` compiled with an `Any` element hint, so the
    /// assigned local's slot stayed `Any` and subsequent `.field` access fell
    /// back to dynamic `GetFieldByName` dispatch instead of typed `GetField`.
    pub(in crate::compile) fn array_julia_type_element_type(
        &self,
        jt: &JuliaType,
    ) -> Option<ArrayElementType> {
        let element = match jt {
            JuliaType::VectorOf(element) | JuliaType::MatrixOf(element) => element.as_ref(),
            _ => return None,
        };
        builtin_type_array_element_type(&element.name())
            .or_else(|| self.heap_julia_type_array_element_type_resolved(element))
    }

    pub(in crate::compile::expr) fn typed_array_literal_element_type(
        &self,
        array: &Expr,
    ) -> Option<ArrayElementType> {
        // A local variable bound to an array is an ordinary indexing expression,
        // never a `T[...]` literal — skip it so the caller's local-aware branch
        // takes over.
        if let Expr::Var(name, _) = array {
            if self.locals.contains_key(name.as_str()) {
                return None;
            }
        }

        if let Expr::Var(type_name, _) = array {
            if let Some(elem) = bare_type_name_array_element_type(type_name) {
                return Some(elem);
            }
        }

        if let Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args,
            ..
        } = array
        {
            if let [Expr::Literal(Literal::Str(type_name), _)] = args.as_slice() {
                let jt = JuliaType::from_name_or_struct(type_name);
                if let Some(elem) = self.heap_julia_type_array_element_type_resolved(&jt) {
                    return Some(elem);
                }
            }
        }

        if let Expr::Literal(Literal::DataType(type_name), _) = array {
            let jt = JuliaType::from_name_or_struct(type_name);
            if let Some(elem) = self.heap_julia_type_array_element_type_resolved(&jt) {
                return Some(elem);
            }
        }

        if let Expr::DynamicTypeConstruct {
            base,
            type_args,
            splat_mask,
            ..
        } = array
        {
            if splat_mask.iter().all(|is_splat| !is_splat) {
                match base.as_str() {
                    "Pair" => {
                        let params = type_args
                            .iter()
                            .map(static_datatype_expr_name)
                            .collect::<Option<Vec<_>>>()?;
                        return Some(ArrayElementType::Abstract(format!(
                            "Pair{{{}}}",
                            params.join(",")
                        )));
                    }
                    "Dict" | "Set" => return Some(ArrayElementType::Any),
                    "Union" => {
                        let body = type_args
                            .iter()
                            .map(static_datatype_expr_name)
                            .collect::<Option<Vec<_>>>()?
                            .join(", ");
                        return Some(ArrayElementType::union_from_body(&body));
                    }
                    _ => {}
                }
                let params = type_args
                    .iter()
                    .map(static_datatype_expr_name)
                    .collect::<Option<Vec<_>>>()?;
                let jt =
                    JuliaType::from_name_or_struct(&format!("{}{{{}}}", base, params.join(",")));
                if let Some(elem) = self.heap_julia_type_array_element_type_resolved(&jt) {
                    return Some(elem);
                }
            }
        }

        // Parametric / non-`Var` type targets (`Pair{Int,Int}`, `Dict{Int,Int}`,
        // `Set{Int}`, `MyStruct`) resolve through `infer_julia_type` to a
        // `Type{T}` object; map that to the matching container element type.
        if let JuliaType::TypeOf(inner) = self.infer_julia_type(array) {
            return self.heap_julia_type_array_element_type_resolved(inner.as_ref());
        }

        None
    }

    fn compile_similar_dim_arg(&mut self, dim: &Expr) -> CResult<()> {
        let value_ty = self.infer_expr_type(dim);
        let is_tuple_like = matches!(value_ty, ValueType::Tuple)
            || matches!(
                self.infer_julia_type(dim),
                JuliaType::Tuple | JuliaType::TupleOf(_)
            );

        if is_tuple_like || matches!(value_ty, ValueType::Any) {
            // Any may resolve to either an integer dim or tuple dims at runtime.
            // The Similar VM handler validates both forms precisely (Issue #4643).
            self.compile_expr(dim).map(|_| ())
        } else {
            self.compile_expr_as(dim, ValueType::I64)
        }
    }

    /// The rank `similar(a, dims...)` produces, when the dims shape is
    /// statically countable — otherwise `None` (Issue #9642).
    ///
    /// Upstream `similar(A::AbstractArray, dims::DimOrInd...)` and
    /// `similar(A::AbstractArray, dims::Tuple)` (`julia/base/abstractarray.jl`)
    /// both route through `to_shape(dims)`, whose rank is the *number of
    /// explicit dims* — never the source array `A`'s own rank. A single
    /// non-tuple dim arg (`similar(a, n)`) always yields a `Vector` even when
    /// `a` is a matrix; `N` integer dim args (`similar(a, d1, ..., dN)`)
    /// always yield rank `N`. A single dims-tuple arg's rank is only known
    /// statically when the tuple's own element count is known
    /// (`JuliaType::TupleOf`); an opaque `Tuple` of unknown arity, or a bare
    /// `Any` single dim arg (Issue #4643: may resolve to either an integer
    /// dim or a dims tuple at runtime), leaves the result rank unknown so the
    /// caller falls back to the conservative, rank-erased `ValueType::Array`
    /// instead of guessing.
    fn similar_dims_rank(&mut self, dim_args: &[Expr]) -> Option<usize> {
        if dim_args.len() != 1 {
            return Some(dim_args.len());
        }
        let dim = &dim_args[0];
        let julia_ty = self.infer_julia_type(dim);
        if let JuliaType::TupleOf(elements) = &julia_ty {
            return Some(elements.len());
        }
        let value_ty = self.infer_expr_type(dim);
        let is_ambiguous =
            matches!(value_ty, ValueType::Tuple | ValueType::Any) || julia_ty == JuliaType::Tuple;
        if is_ambiguous {
            None
        } else {
            Some(1)
        }
    }

    /// Compile array function calls.
    /// Returns Some(type) if handled, None if not an array function.
    /// Note: zeros/ones are Pure Julia dispatch in base/array.jl (Issue #4036).
    pub(in super::super) fn compile_builtin_array(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "_mark_bitvector" => {
                if args.len() != 1 {
                    return err("_mark_bitvector requires exactly 1 argument: _mark_bitvector(v)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MarkBitVector, 1));
                Ok(Some(ValueType::ArrayOf(ArrayElementType::Bool, None)))
            }
            "_mark_bitarray" => {
                if args.len() != 1 {
                    return err("_mark_bitarray requires exactly 1 argument: _mark_bitarray(v)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MarkBitArray, 1));
                Ok(Some(ValueType::ArrayOf(ArrayElementType::Bool, None)))
            }
            "_linspace_range_f64" => {
                // _linspace_range_f64(start, stop, len[, tag]) —
                // TwicePrecision-backed float range for
                // `range(start, stop; length)` (Issue #9419); upstream
                // range_start_stop_length(::T, ::T, ::Integer) where
                // T<:IEEEFloat (julia/base/twiceprecision.jl _linspace).
                // The optional tag selects the element type (0 = Float64,
                // 1 = Float32, 2 = Float16; Issue #9509).
                if args.len() != 3 && args.len() != 4 {
                    return err(
                        "_linspace_range_f64 requires 3 or 4 arguments: _linspace_range_f64(start, stop, len[, tag])",
                    );
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr_as(&args[2], ValueType::I64)?;
                if args.len() == 4 {
                    self.compile_expr_as(&args[3], ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::LinspaceF64, args.len()));
                Ok(Some(ValueType::Range))
            }
            "_steprangelen_range_f64" => {
                // _steprangelen_range_f64(start, step, len, tag) —
                // TwicePrecision-backed float range for
                // `range(start; step, length)` (Issue #9509); upstream
                // range_start_step_length(::T, ::T, ::Integer) where
                // T<:IEEEFloat (julia/base/twiceprecision.jl:448). tag: 0 =
                // Float64, 1 = Float32, 2 = Float16.
                if args.len() != 4 {
                    return err(
                        "_steprangelen_range_f64 requires exactly 4 arguments: _steprangelen_range_f64(start, step, len, tag)",
                    );
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr_as(&args[2], ValueType::I64)?;
                self.compile_expr_as(&args[3], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::SteprangelenF64, 4));
                Ok(Some(ValueType::Range))
            }
            "_try_complex_scale_tp_range_f64" => {
                // _try_complex_scale_tp_range_f64(re, im, r) — upstream range
                // broadcast fusion `x::Complex .* r::StepRangeLen` (Issue
                // #9659): materialize the complex-scaled TwicePrecision range
                // with upstream-bit-identical element values, or `nothing`
                // when `r` is not a TwicePrecision-backed Float64 range (the
                // caller then falls back to the generic broadcast path).
                if args.len() != 3 {
                    return err(
                        "_try_complex_scale_tp_range_f64 requires exactly 3 arguments: (re, im, r)",
                    );
                }
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.compile_expr_as(&args[1], ValueType::F64)?;
                self.compile_expr(&args[2])?;
                self.emit(Instr::CallBuiltin(BuiltinId::ComplexScaleTpRange, 3));
                Ok(Some(ValueType::Any))
            }
            "_try_broadcast_typed_kernel" => {
                // _try_broadcast_typed_kernel(f, args...) — bulk typed-kernel
                // broadcast (Issues #9693/#8797): one dispatch + one Rust loop
                // over the array storage, or `nothing` for the generic path.
                if args.len() < 2 || args.len() > 5 {
                    return err(
                        "_try_broadcast_typed_kernel requires 2..=5 arguments: (f, args...)",
                    );
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(
                    BuiltinId::BroadcastTypedKernel,
                    args.len(),
                ));
                Ok(Some(ValueType::Any))
            }
            "_try_broadcast_binary_arith" => {
                // _try_broadcast_binary_arith(f, a, b) — upstream-exact
                // elementwise +/-/* broadcast fast path (Issue #8797), or
                // `nothing` for the generic path.
                if args.len() != 3 {
                    return err("_try_broadcast_binary_arith requires exactly 3 arguments");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::BroadcastBinaryArith, 3));
                Ok(Some(ValueType::Any))
            }
            "similar" => {
                // similar(a)                       - uninitialized array, same eltype, same shape
                // similar(a, n)                    - same eltype, length n (Issue #2129)
                // similar(a, n, m, ...)            - same eltype, given multi-dim shape (Issue #3751)
                // similar(a, T)                    - eltype T, same shape (Issue #3751)
                // similar(a, T, n, m, ...)         - eltype T, given shape (Issue #3751)
                //
                // The runtime BuiltinId::Similar handler distinguishes the typed form
                // (second arg is a DataType) from the untyped dim form (second arg is an
                // integer dim). All forms route through a single CallBuiltin with the
                // same argc. Type/DataType args must be pushed without I64 coercion.
                if args.is_empty() {
                    return err(
                        "similar requires at least 1 argument: similar(array[, T][, dims...])",
                    );
                }
                self.compile_expr(&args[0])?;
                // Detect typed form: similar(arr, T, ...) where T is a Type/DataType.
                // `infer_expr_type` returns DataType for type-producing calls (e.g.
                // `eltype(x)`), but bare `Int`/`Float64` references are `Expr::Var`
                // which falls through to `Any`. Recognise those by name explicitly.
                let typed_form = if args.len() >= 2 {
                    let t = self.infer_expr_type(&args[1]);
                    if matches!(t, ValueType::DataType) {
                        true
                    } else if let Expr::Var(n, _) = &args[1] {
                        is_builtin_type_name(n)
                    } else {
                        false
                    }
                } else {
                    false
                };
                if typed_form {
                    // Push T as a DataType value, then any remaining args as integer dims.
                    self.compile_expr(&args[1])?;
                    for dim in &args[2..] {
                        self.compile_similar_dim_arg(dim)?;
                    }
                } else {
                    for dim in &args[1..] {
                        self.compile_similar_dim_arg(dim)?;
                    }
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Similar, args.len()));
                // Element type tracking: when the user passed an explicit T, we lose
                // compile-time eltype info (only known at runtime). Otherwise the
                // result keeps the source array's element type.
                //
                // Rank tracking (Issue #9642): when explicit dims are given, the
                // result's rank is the *number of dims* — never the source
                // array's own rank (see `similar_dims_rank`). Statically binding
                // a call like `rank_dispatch(similar(a, dims...))` to a
                // rank-specialized method using the source's rank previously
                // produced a silently wrong static dispatch (e.g.
                // `similar([1,2,3], 1, 3, 1)` is rank 3, but the 1-D source's
                // rank was propagated instead). When the dims shape is not
                // itself statically countable, the rank is left unknown so
                // `is_rank_unknown_array_julia_type` forces the call site to
                // defer to runtime dispatch instead of guessing.
                let dim_args: &[Expr] = if typed_form { &args[2..] } else { &args[1..] };
                let arr_ty = if typed_form {
                    ValueType::Array
                } else if dim_args.is_empty() {
                    // Issue #10076: the source's own inferred `ValueType`
                    // (via `infer_expr_type`, whose `Expr::ArrayLiteral` arm
                    // now carries the literal's rank) is the correct result
                    // type here for a concrete element type. A source whose
                    // element type is a bare `Any` (`[]`, `[1 "a"; 2 "b"]`)
                    // still reports a rank-erased `ArrayOf(Any, None)` on
                    // purpose: `ArrayOf(Any, Some(n))`'s rank slot is
                    // ambiguous with the "rank known, element type
                    // unresolved" encoding used by
                    // `Expr::Comprehension`/`Expr::MultiComprehension`
                    // (Issue #6817), and the `infer_julia_type` dispatch
                    // bridge's handling of that combination for rank 2+ can
                    // itself throw a spurious `MethodError` instead of
                    // falling back to dynamic dispatch (confirmed
                    // pre-existing on `main`, independent of `similar`, via
                    // a genuinely rank-2 `Any`-element comprehension).
                    // `similar(a)` on an `Any`-element 2-D+ source therefore
                    // still statically mis-binds to the rank-1 method
                    // (silently wrong, matching pre-existing comprehension
                    // behavior) rather than crashing — tracked as a residual
                    // gap in Issue #10206, deliberately not widened here.
                    self.infer_expr_type(&args[0])
                } else {
                    // Only override the rank when the source is precisely known
                    // to be array-family (`ArrayOf`); a source that infers to
                    // something else entirely (e.g. `Memory`/`MemoryOf` inside
                    // Base's own `_array_similar_tuple(a, dims) = similar(memory,
                    // len)`, or a genuinely unresolved `Any`) must keep its own
                    // inferred type verbatim, exactly as the pre-fix code did —
                    // coercing it to the array-family `ValueType::Array` tag
                    // caused a downstream Base `wrap(...)` call to statically
                    // over-bind to a single candidate instead of the correct
                    // runtime-resolved one.
                    match self.infer_expr_type(&args[0]) {
                        ValueType::ArrayOf(elem, _) => {
                            ValueType::ArrayOf(elem, self.similar_dims_rank(dim_args))
                        }
                        other => other,
                    }
                };
                Ok(Some(arr_ty))
            }
            "length" => {
                // Universal length - handles Array, Tuple, Dict, Range, String via CallBuiltin
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
                Ok(Some(ValueType::I64))
            }
            "getindex" => {
                // getindex(collection, indices...) - Julia-compliant indexing
                // s[i] is lowered to getindex(s, i)
                if args.is_empty() {
                    return err(
                        "getindex requires at least 1 argument: getindex(collection, indices...)",
                    );
                }

                let collection_type = self.infer_expr_type(&args[0]);
                let typed_array_element_type = match &collection_type {
                    ValueType::ArrayOf(element_type, _) => Some(element_type.clone()),
                    _ => self.array_julia_type_element_type(&self.infer_julia_type(&args[0])),
                };
                // Typed array literal `T[a, b, ...]`. The non-empty form is
                // lowered to `getindex(T, a, b, ...)`, so recognize a type
                // object target here and store the elements into a
                // `Vector{T}`-typed array.
                //
                // Mirror upstream — `T[a, b, ...]` lowers to
                // `a = Vector{T}(undef, n); a[i] = vals[i]`, where `setindex!`
                // does `convert(T, x)`. Every element takes that route; deciding
                // from a type allowlist drifted whenever convert gained support
                // for another target (Issues #10835/#11779).
                let typed_literal_elem = self.typed_array_literal_element_type(&args[0]);
                if let Some(element_type) = typed_literal_elem {
                    let values = &args[1..];
                    // Upstream evaluates the type expression once, before any
                    // element expression. Preserve that value even if an
                    // element mutates a binding used by the type expression.
                    self.compile_expr(&args[0])?;
                    let target_temp = self.new_temp("typed_literal_target");
                    self.emit(Instr::StoreAny(target_temp.clone()));
                    self.emit_array_wrapper_memory_start(element_type.clone(), values.len());
                    for (index, value) in values.iter().enumerate() {
                        self.emit(Instr::PushI64((index + 1) as i64));
                        // Reload the evaluated target so structured Union and
                        // UnionAll identity survives into convert; a rendered
                        // PushDataType name loses nested parameters (#11783).
                        self.emit(Instr::LoadAny(target_temp.clone()));
                        self.compile_expr(value)?;
                        self.emit(Instr::CallBuiltin(BuiltinId::Convert, 2));
                        self.emit(Instr::MemorySet);
                    }
                    self.emit_array_wrapper_from_memory_on_stack(&[values.len()]);
                    return Ok(Some(ValueType::ArrayOf(element_type, None)));
                }

                match collection_type {
                    ValueType::Dict => {
                        // Dict indexing: getindex(d, key)
                        if args.len() != 2 {
                            return err("Dict indexing requires exactly one key");
                        }
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        // Issue #8480: `d[k]` is `getindex(d, k)`, not
                        // `get(d, k)`. Reuse IndexLoad so StructRef-backed Dicts
                        // dispatch through the same getindex path as Any-typed
                        // receivers.
                        self.emit(Instr::IndexLoad(1));
                        Ok(Some(ValueType::Any))
                    }
                    ValueType::Tuple => {
                        // Tuple indexing: getindex(t, i) or getindex(t, range)
                        if args.len() != 2 {
                            return err("Tuple indexing requires exactly one index");
                        }
                        let index_type = self.infer_expr_type(&args[1]);
                        let index_julia_type = self.infer_julia_type(&args[1]);
                        let is_slice =
                            matches!(&args[1], Expr::Range { .. } | Expr::SliceAll { .. })
                                || matches!(index_type, ValueType::Range)
                                || is_range_like_julia_type(&index_julia_type);
                        let is_index_array = is_julia_array_like_type(&index_julia_type)
                            || self.is_array_wrapper_value_type(&index_type)
                            || matches!(index_type, ValueType::Array | ValueType::ArrayOf(_, _));
                        if is_slice || is_index_array {
                            self.compile_expr(&args[0])?;
                            self.compile_expr(&args[1])?;
                            self.emit(Instr::IndexSlice(1));
                            return Ok(Some(ValueType::Tuple));
                        }
                        // Element-type sharpening for a constant index over a
                        // statically known tuple type (Issue #5183). Computed
                        // before `compile_expr` since inference borrows `self`.
                        let elem_ty = self
                            .tuple_const_index_value_type(&args[0], &args[1])
                            .unwrap_or(ValueType::Any);
                        self.compile_expr(&args[0])?;
                        self.compile_expr_as(&args[1], ValueType::I64)?;
                        self.emit(Instr::TupleGet);
                        Ok(Some(elem_ty))
                    }
                    ValueType::NamedTuple => {
                        // NamedTuple indexing: getindex(nt, i) or getindex(nt, :symbol)
                        // Julia supports both integer index and symbol index for NamedTuples
                        if args.len() != 2 {
                            return err("NamedTuple indexing requires exactly one index");
                        }
                        let index_type = self.infer_expr_type(&args[1]);
                        match index_type {
                            ValueType::Symbol => {
                                // Symbol index: nt[:field]
                                self.compile_expr(&args[0])?;
                                self.compile_expr(&args[1])?;
                                self.emit(Instr::NamedTupleGetBySymbol);
                                Ok(Some(ValueType::Any))
                            }
                            _ => {
                                // Integer index: nt[1]. Sharpen the element type
                                // for a constant index over a concrete
                                // `@NamedTuple{...}` (Issue #5183).
                                let elem_ty = self
                                    .tuple_const_index_value_type(&args[0], &args[1])
                                    .unwrap_or(ValueType::Any);
                                self.compile_expr(&args[0])?;
                                self.compile_expr_as(&args[1], ValueType::I64)?;
                                self.emit(Instr::NamedTupleGetIndex);
                                Ok(Some(elem_ty))
                            }
                        }
                    }
                    ValueType::Pairs => {
                        // Base.Pairs indexing: getindex(pairs, :symbol)
                        // Only symbol index is supported (kwargs[:key])
                        if args.len() != 2 {
                            return err("Pairs indexing requires exactly one index");
                        }
                        let index_type = self.infer_expr_type(&args[1]);
                        self.compile_expr(&args[0])?;
                        match index_type {
                            ValueType::Symbol => {
                                // Symbol index: kwargs[:key]
                                self.compile_expr(&args[1])?;
                                self.emit(Instr::PairsGetBySymbol);
                                Ok(Some(ValueType::Any))
                            }
                            _ => err("Base.Pairs only supports Symbol indexing (kwargs[:key])"),
                        }
                    }
                    ValueType::Str => {
                        // String indexing: getindex(s, i), getindex(s, range), or getindex(s, indices)
                        if args.len() != 2 {
                            return err("String indexing requires exactly one index");
                        }
                        let index_type = self.infer_expr_type(&args[1]);
                        let index_julia_type = self.infer_julia_type(&args[1]);
                        let is_dynamic_index =
                            matches!(index_type, ValueType::Any | ValueType::Struct(_));
                        let is_slice =
                            matches!(&args[1], Expr::Range { .. } | Expr::SliceAll { .. })
                                || is_range_like_julia_type(&index_julia_type)
                                || is_julia_array_like_type(&index_julia_type)
                                || self.is_array_wrapper_value_type(&index_type)
                                || matches!(
                                    index_type,
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
                                );
                        self.compile_expr(&args[0])?;
                        if is_slice {
                            // String slicing: s[2:4], s[:], or s[[1, 3]] returns String
                            self.compile_expr(&args[1])?;
                            self.emit(Instr::IndexSlice(1));
                            Ok(Some(ValueType::Str))
                        } else if is_dynamic_index {
                            // Captured/Any-typed indices are classified at runtime:
                            // integers use scalar String indexing, while range and
                            // vector carriers use IndexSlice (Issue #11629).
                            self.compile_expr(&args[1])?;
                            self.emit(Instr::IndexLoad(1));
                            Ok(Some(ValueType::Any))
                        } else {
                            // String indexing: s[i] returns Char
                            self.compile_expr_as(&args[1], ValueType::I64)?;
                            self.emit(Instr::IndexLoad(1));
                            Ok(Some(ValueType::Char))
                        }
                    }
                    _ if typed_array_element_type.is_some() => {
                        let element_type = typed_array_element_type.ok_or_else(|| {
                            internal_compile_error("guarded typed array element type checked above")
                        })?;
                        let indices = &args[1..];
                        let has_slice = indices.iter().any(|idx| match idx {
                            Expr::Range { .. } | Expr::SliceAll { .. } => true,
                            _ => {
                                let idx_type = self.infer_expr_type(idx);
                                let idx_julia_type = self.infer_julia_type(idx);
                                is_julia_array_like_type(&idx_julia_type)
                                    || self.is_array_wrapper_value_type(&idx_type)
                                    || is_range_like_julia_type(&idx_julia_type)
                                    || matches!(
                                        idx_type,
                                        ValueType::Array
                                            | ValueType::ArrayOf(_, _)
                                            | ValueType::Bool
                                            | ValueType::Range
                                            | ValueType::Rng
                                    )
                            }
                        });

                        self.compile_expr(&args[0])?;
                        for idx in indices {
                            match idx {
                                Expr::Range { .. } | Expr::SliceAll { .. } => {
                                    self.compile_expr(idx)?;
                                }
                                _ => {
                                    let idx_type = self.infer_expr_type(idx);
                                    if matches!(
                                        idx_type,
                                        ValueType::Struct(_)
                                            | ValueType::Any
                                            | ValueType::Array
                                            | ValueType::ArrayOf(_, _)
                                            | ValueType::Bool
                                            | ValueType::Tuple
                                            | ValueType::Str
                                            | ValueType::Symbol
                                            | ValueType::DataType
                                            | ValueType::Range
                                            | ValueType::Rng
                                    ) {
                                        // A DataType-valued index can only be a Dict
                                        // key (no array accepts a type as an index), so
                                        // preserve it and let IndexLoad dispatch to the
                                        // Dict struct at runtime instead of coercing it
                                        // to I64 (Issue #7940).
                                        self.compile_expr(idx)?;
                                    } else {
                                        self.compile_expr_as(idx, ValueType::I64)?;
                                    }
                                }
                            }
                        }

                        if has_slice {
                            self.emit(Instr::IndexSlice(indices.len()));
                            Ok(Some(ValueType::Array))
                        } else if indices.len() == 1
                            && (self.inbounds_context
                                || self.is_proven_inbounds_index(&args[0], &indices[0]))
                        {
                            self.emit(Instr::IndexLoadTypedInbounds(indices.len()));
                            Ok(Some(element_type.to_value_type()))
                        } else {
                            self.emit(Instr::IndexLoadTyped(indices.len()));
                            Ok(Some(element_type.to_value_type()))
                        }
                    }
                    _ => {
                        // Array or unknown type - use IndexLoad/IndexSlice
                        let indices = &args[1..];
                        // Check for slice-like indices: Range, SliceAll, or Array (for logical indexing)
                        // Bool is included because broadcast comparisons (arr .> 2) may be
                        // inferred as Bool when the result is actually a Bool array (Issue #2694)
                        let has_slice = indices.iter().any(|idx| {
                            match idx {
                                Expr::Range { .. } | Expr::SliceAll { .. } => true,
                                _ => {
                                    // Array index could be logical indexing (bool array), index array,
                                    // or a Range variable (Issue #3481)
                                    let idx_type = self.infer_expr_type(idx);
                                    let idx_julia_type = self.infer_julia_type(idx);
                                    is_julia_array_like_type(&idx_julia_type)
                                        || self.is_array_wrapper_value_type(&idx_type)
                                        || is_range_like_julia_type(&idx_julia_type)
                                        || matches!(
                                            idx_type,
                                            ValueType::Array
                                                | ValueType::ArrayOf(_, _)
                                                | ValueType::Bool
                                                | ValueType::Range
                                                | ValueType::Rng
                                        )
                                }
                            }
                        });

                        self.compile_expr(&args[0])?;
                        for idx in indices {
                            match idx {
                                Expr::Range { .. } | Expr::SliceAll { .. } => {
                                    self.compile_expr(idx)?;
                                }
                                _ => {
                                    // Check if index might be a CartesianIndex (struct type), Array,
                                    // Bool array, Range variable, or non-numeric key for Dict indexing (Issue #1814, #3481)
                                    let idx_type = self.infer_expr_type(idx);
                                    if matches!(
                                        idx_type,
                                        ValueType::Struct(_)
                                            | ValueType::Any
                                            | ValueType::Array
                                            | ValueType::ArrayOf(_, _)
                                            | ValueType::Bool
                                            | ValueType::Tuple
                                            | ValueType::Str
                                            | ValueType::Symbol
                                            | ValueType::DataType
                                            | ValueType::Range
                                            | ValueType::Rng
                                    ) {
                                        // A DataType-valued index can only be a Dict
                                        // key (no array accepts a type as an index), so
                                        // preserve it and let IndexLoad dispatch to the
                                        // Dict struct at runtime instead of coercing it
                                        // to I64 (Issue #7940).
                                        self.compile_expr(idx)?;
                                    } else {
                                        self.compile_expr_as(idx, ValueType::I64)?;
                                    }
                                }
                            }
                        }
                        if has_slice {
                            self.emit(Instr::IndexSlice(indices.len()));
                            Ok(Some(ValueType::Any))
                        } else if indices.len() == 1
                            && (self.inbounds_context
                                || self.is_proven_inbounds_index(&args[0], &indices[0]))
                        {
                            self.emit(Instr::IndexLoadInbounds(indices.len()));
                            Ok(Some(ValueType::Any))
                        } else {
                            self.emit(Instr::IndexLoad(indices.len()));
                            Ok(Some(ValueType::Any))
                        }
                    }
                }
            }
            "setindex!" => {
                // setindex!(collection, value, indices...) - Julia-compliant indexed assignment
                // s[i] = v is lowered to setindex!(s, v, i)
                // The zero-index form setindex!(r, v) mutates a Base.RefValue (Issue #5130).
                if args.len() == 2 {
                    // setindex!(ref, value): stack [ref, value], IndexStore(0).
                    self.compile_expr(&args[0])?; // ref (bottom)
                    self.compile_expr(&args[1])?; // value (top)
                    self.emit(Instr::IndexStore(0));
                    // IndexStore leaves the modified collection on the stack; return it.
                    return Ok(Some(ValueType::Any));
                }
                if args.len() < 3 {
                    return err("setindex! requires at least 3 arguments: setindex!(collection, value, indices...)");
                }

                let collection_type = self.infer_expr_type(&args[0]);

                // Julia: setindex!(collection, value, indices...) returns the mutated collection.
                match collection_type {
                    ValueType::Dict => {
                        // Dict assignment: setindex!(d, value, key)
                        self.compile_expr(&args[0])?; // dict
                        self.compile_expr(&args[2])?; // key
                        self.compile_expr(&args[1])?; // value
                        self.emit(Instr::CallBuiltin(BuiltinId::DictSet, 3));
                        // DictSet pushes the modified dict; return it (Issue #3477)
                        Ok(Some(ValueType::Dict))
                    }
                    ct => {
                        // Array or unknown type assignment: setindex!(collection, value, indices...)
                        // When collection type is Any and index is non-numeric (e.g., Str, Symbol),
                        // emit DictSet for runtime Dict dispatch (Issue #1814)
                        let idx_types: Vec<_> = args[2..]
                            .iter()
                            .map(|idx| self.infer_expr_type(idx))
                            .collect();
                        // A DataType-valued key (e.g. `D[T] = v` with `T` a `where`
                        // type parameter) can only target a Dict, so route it through
                        // DictSet just like Str/Symbol keys instead of coercing the key
                        // to I64 in the array path (Issue #7940).
                        let has_non_numeric_idx = args.len() == 3
                            && idx_types.iter().any(|t| {
                                matches!(
                                    t,
                                    ValueType::Tuple
                                        | ValueType::Str
                                        | ValueType::Symbol
                                        | ValueType::DataType
                                )
                            });

                        if has_non_numeric_idx {
                            // Likely Dict assignment: emit DictSet
                            self.compile_expr(&args[0])?; // dict
                            self.compile_expr(&args[2])?; // key
                            self.compile_expr(&args[1])?; // value
                            self.emit(Instr::CallBuiltin(BuiltinId::DictSet, 3));
                            // DictSet pushes modified dict; return it (Issue #3477)
                            Ok(Some(ct))
                        } else {
                            // IndexStore expects stack: [array, idx1, idx2, ..., value] with value on top
                            // For Any-typed indices, compile without I64 coercion so the VM
                            // can dispatch to Dict at runtime (Issue #1814)
                            self.compile_expr(&args[0])?; // array (bottom)
                            for idx in &args[2..] {
                                let idx_type = self.infer_expr_type(idx);
                                if matches!(
                                    idx_type,
                                    ValueType::Any | ValueType::Tuple | ValueType::DataType
                                ) {
                                    // Preserve Any/Tuple/DataType indices so the VM can
                                    // dispatch to Dict at runtime (Issue #1814, #7940).
                                    self.compile_expr(idx)?;
                                } else {
                                    self.compile_expr_as(idx, ValueType::I64)?; // indices
                                }
                            }
                            self.compile_expr(&args[1])?; // value (top)
                            let indices = &args[2..];
                            if indices.len() == 1
                                && (self.inbounds_context
                                    || self.is_proven_inbounds_index(&args[0], &indices[0]))
                            {
                                self.emit(Instr::IndexStoreInbounds(indices.len()));
                            } else {
                                self.emit(Instr::IndexStore(indices.len()));
                            }
                            // IndexStore leaves the modified collection on the stack; return it (Issue #3477)
                            Ok(Some(ct))
                        }
                    }
                }
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Issue #5143: a `Union{...}` type-literal target must map to a `UnionOf`
    // element type so `compile_builtin_array`'s `getindex` arm routes the
    // literal through the verbatim-store branch (no per-member coercion).
    #[test]
    fn heap_julia_type_array_element_type_union_keeps_members() {
        let jt = JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]);
        assert_eq!(
            heap_julia_type_array_element_type(&jt),
            Some(ArrayElementType::UnionOf(vec![
                JuliaType::Int64,
                JuliaType::Float64
            ]))
        );
    }

    #[test]
    fn heap_julia_type_array_element_type_bottom_is_empty_union() {
        assert_eq!(
            heap_julia_type_array_element_type(&JuliaType::Bottom),
            Some(ArrayElementType::UnionOf(Vec::new()))
        );
    }

    #[test]
    fn heap_julia_type_array_element_type_abstract_is_boxed() {
        assert_eq!(
            heap_julia_type_array_element_type(&JuliaType::Real),
            Some(ArrayElementType::Abstract("Real".to_string()))
        );
    }

    #[test]
    fn heap_julia_type_array_element_type_pair_stays_abstract() {
        let jt = JuliaType::Struct("Pair{Int64, Int64}".to_string());
        assert_eq!(
            heap_julia_type_array_element_type(&jt),
            Some(ArrayElementType::Abstract("Pair{Int64, Int64}".to_string()))
        );
    }

    #[test]
    fn heap_julia_type_array_element_type_complex_uses_dedicated_storage() {
        assert_eq!(
            heap_julia_type_array_element_type(&JuliaType::Struct("Complex{Float64}".to_string())),
            Some(ArrayElementType::ComplexF64)
        );
        assert_eq!(
            heap_julia_type_array_element_type(&JuliaType::Struct("Complex{Float32}".to_string())),
            Some(ArrayElementType::ComplexF32)
        );
    }

    #[test]
    fn heap_julia_type_array_element_type_struct_name_union_keeps_body() {
        let jt = JuliaType::Struct("Union{Int64, Float64}".to_string());
        assert_eq!(
            heap_julia_type_array_element_type(&jt),
            Some(ArrayElementType::UnionOf(vec![
                JuliaType::Int64,
                JuliaType::Float64
            ]))
        );
    }
}
