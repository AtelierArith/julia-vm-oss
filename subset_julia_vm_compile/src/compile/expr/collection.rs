//! Collection compilation (arrays, dicts, Memory, comprehensions).
//!
//! Handles compilation of:
//! - Array comprehensions
//! - Dict constructors
//! - Memory{T} constructors

// SAFETY: i64→usize casts are from integer literals in the source AST, which are
// non-negative by construction (Memory{T} constructor sizes).
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::bytecode::{ArrayElementType, GeneratorCallableSpec, Instr, ValueType};
use crate::ir::core::{
    decode_tuple_comprehension_binding, BinaryOp, Expr, Function, Literal, Stmt,
};
use crate::span::Span;

use super::super::{err, parse_parametric_call, CResult, CoreCompiler, TypeExpr};

/// Names assigned by a comprehension expression belong to the comprehension's
/// hard scope, except for names explicitly declared `global` in that same
/// expression. Later iterator expressions are included because flatten-form
/// comprehensions evaluate them inside the preceding binders' scope.
fn comprehension_assignment_owner_names<'a>(
    body: &'a Expr,
    filter: Option<&'a Expr>,
    later_iters: impl IntoIterator<Item = &'a Expr>,
) -> Vec<String> {
    let mut assigned = std::collections::HashSet::new();
    let mut declared_globals = std::collections::HashSet::new();
    for expr in std::iter::once(body).chain(filter).chain(later_iters) {
        super::collect_let_expr_assignment_names(expr, &mut assigned);
        super::collect_let_expr_declared_globals(expr, &mut declared_globals);
    }
    assigned.retain(|name| !declared_globals.contains(name));
    let mut names: Vec<_> = assigned.into_iter().collect();
    names.sort();
    names
}

fn comprehension_iter_uses_iterate_protocol(iter_value_type: &ValueType) -> bool {
    // The indexed loop is an optimization for representations whose public
    // iteration order is exactly their 1-based linear indexing order. Every
    // other value must use Julia's `iterate` protocol. In particular, an
    // imported constructor call is conservatively typed as `Any`; assuming
    // `getindex` for that dynamic result breaks iterate-only values such as
    // `Iterators.Partition` (Issue #10442).
    !matches!(
        iter_value_type,
        ValueType::Array
            | ValueType::ArrayOf(_, _)
            | ValueType::Memory
            | ValueType::MemoryOf(_)
            | ValueType::Range
            | ValueType::Tuple
            | ValueType::NamedTuple
    )
}

fn union_type_params_to_body(params: &[TypeExpr]) -> Option<String> {
    if params.is_empty() {
        return Some(String::new());
    }

    let mut names = Vec::with_capacity(params.len());
    for param in params {
        names.push(param.as_simple_type_name()?);
    }
    names.sort_by_key(|name| match name.as_str() {
        "Nothing" => 0,
        "Missing" => 1,
        _ => 2,
    });
    Some(names.join(", "))
}

fn numeric_typejoin_array_element_type(
    left: &ValueType,
    right: &ValueType,
) -> Option<ArrayElementType> {
    use ValueType::{
        BigFloat, BigInt, Bool, F16, F32, F64, I128, I16, I32, I64, I8, U128, U16, U32, U64, U8,
    };

    fn is_signed_integer(ty: &ValueType) -> bool {
        matches!(
            ty,
            ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::I128
                | ValueType::BigInt
        )
    }

    fn is_unsigned_integer(ty: &ValueType) -> bool {
        matches!(
            ty,
            ValueType::U8 | ValueType::U16 | ValueType::U32 | ValueType::U64 | ValueType::U128
        )
    }

    fn is_integer(ty: &ValueType) -> bool {
        matches!(ty, ValueType::Bool) || is_signed_integer(ty) || is_unsigned_integer(ty)
    }

    fn is_float(ty: &ValueType) -> bool {
        matches!(
            ty,
            ValueType::F16 | ValueType::F32 | ValueType::F64 | ValueType::BigFloat
        )
    }

    match (left, right) {
        (Bool, Bool) => Some(ArrayElementType::Bool),
        (I8, I8) => Some(ArrayElementType::I8),
        (I16, I16) => Some(ArrayElementType::I16),
        (I32, I32) => Some(ArrayElementType::I32),
        (I64, I64) => Some(ArrayElementType::I64),
        (U8, U8) => Some(ArrayElementType::U8),
        (U16, U16) => Some(ArrayElementType::U16),
        (U32, U32) => Some(ArrayElementType::U32),
        (U64, U64) => Some(ArrayElementType::U64),
        (F32, F32) => Some(ArrayElementType::F32),
        (F64, F64) => Some(ArrayElementType::F64),
        // Issue #9301: Float16 has a dedicated (boxed) storage tag, so a
        // homogeneous `[Float16(1), Float16(2)]` narrows to `Vector{Float16}`
        // like F32/F64 rather than widening to `Vector{Any}`.
        (F16, F16) => Some(ArrayElementType::F16),
        (BigInt, BigInt) => Some(ArrayElementType::Abstract("BigInt".to_string())),
        (BigFloat, BigFloat) => Some(ArrayElementType::Abstract("BigFloat".to_string())),
        (I128, I128) | (U128, U128) => Some(ArrayElementType::Any),
        _ if is_signed_integer(left) && is_signed_integer(right) => {
            Some(ArrayElementType::Abstract("Signed".to_string()))
        }
        _ if is_integer(left) && is_integer(right) => {
            Some(ArrayElementType::Abstract("Integer".to_string()))
        }
        _ if is_float(left) && is_float(right) => {
            Some(ArrayElementType::Abstract("AbstractFloat".to_string()))
        }
        _ if (is_integer(left) || is_float(left)) && (is_integer(right) || is_float(right)) => {
            Some(ArrayElementType::Abstract("Real".to_string()))
        }
        _ => None,
    }
}

fn collect_typejoin_array_element_type(
    left: ArrayElementType,
    right: ArrayElementType,
) -> ArrayElementType {
    if left == right {
        return left;
    }
    if let Some(common) = typejoin_numeric_abstract_name(&left, &right) {
        return ArrayElementType::Abstract(common.to_string());
    }
    ArrayElementType::Any
}

fn typejoin_numeric_abstract_name(
    left: &ArrayElementType,
    right: &ArrayElementType,
) -> Option<&'static str> {
    let left_chain = numeric_abstract_chain(left)?;
    let right_chain = numeric_abstract_chain(right)?;
    left_chain
        .iter()
        .find(|candidate| right_chain.contains(candidate))
        .copied()
}

fn numeric_abstract_chain(element_type: &ArrayElementType) -> Option<&'static [&'static str]> {
    match element_type {
        ArrayElementType::Bool => Some(&["Integer", "Real", "Number", "Any"]),
        ArrayElementType::I8
        | ArrayElementType::I16
        | ArrayElementType::I32
        | ArrayElementType::I64
        | ArrayElementType::I128 => Some(&["Signed", "Integer", "Real", "Number", "Any"]),
        ArrayElementType::U8
        | ArrayElementType::U16
        | ArrayElementType::U32
        | ArrayElementType::U64
        | ArrayElementType::U128 => Some(&["Unsigned", "Integer", "Real", "Number", "Any"]),
        ArrayElementType::F16 | ArrayElementType::F32 | ArrayElementType::F64 => {
            Some(&["AbstractFloat", "Real", "Number", "Any"])
        }
        ArrayElementType::Abstract(name) => match name.as_str() {
            "BigInt" => Some(&["BigInt", "Signed", "Integer", "Real", "Number", "Any"]),
            "BigFloat" => Some(&["BigFloat", "AbstractFloat", "Real", "Number", "Any"]),
            "Signed" => Some(&["Signed", "Integer", "Real", "Number", "Any"]),
            "Unsigned" => Some(&["Unsigned", "Integer", "Real", "Number", "Any"]),
            "Integer" => Some(&["Integer", "Real", "Number", "Any"]),
            "AbstractFloat" => Some(&["AbstractFloat", "Real", "Number", "Any"]),
            "Real" => Some(&["Real", "Number", "Any"]),
            "Number" => Some(&["Number", "Any"]),
            "Any" => Some(&["Any"]),
            _ => None,
        },
        _ => None,
    }
}

fn tuple_literal_typejoin_element_type(element_types: &[ValueType]) -> Option<ArrayElementType> {
    let (first, rest) = element_types.split_first()?;
    let mut joined = value_type_to_array_element_type(first).unwrap_or(ArrayElementType::Any);
    for element_type in rest {
        let right = value_type_to_array_element_type(element_type).unwrap_or(ArrayElementType::Any);
        joined = collect_typejoin_array_element_type(joined, right);
    }
    Some(joined)
}

fn numeric_union_typejoin_array_element_type(types: &[ValueType]) -> Option<ArrayElementType> {
    let (first, rest) = types.split_first()?;
    let mut joined = value_type_to_array_element_type(first)?;
    numeric_abstract_chain(&joined)?;
    for element_type in rest {
        let right = value_type_to_array_element_type(element_type)?;
        numeric_abstract_chain(&right)?;
        joined = collect_typejoin_array_element_type(joined, right);
    }
    Some(joined)
}

fn value_type_to_array_element_type(value_type: &ValueType) -> Option<ArrayElementType> {
    match value_type {
        ValueType::I8 => Some(ArrayElementType::I8),
        ValueType::I16 => Some(ArrayElementType::I16),
        ValueType::I32 => Some(ArrayElementType::I32),
        ValueType::I64 => Some(ArrayElementType::I64),
        ValueType::U8 => Some(ArrayElementType::U8),
        ValueType::U16 => Some(ArrayElementType::U16),
        ValueType::U32 => Some(ArrayElementType::U32),
        ValueType::U64 => Some(ArrayElementType::U64),
        ValueType::BigInt => Some(ArrayElementType::Abstract("BigInt".to_string())),
        ValueType::F16 => Some(ArrayElementType::F16),
        ValueType::F32 => Some(ArrayElementType::F32),
        ValueType::F64 => Some(ArrayElementType::F64),
        ValueType::BigFloat => Some(ArrayElementType::Abstract("BigFloat".to_string())),
        ValueType::Bool => Some(ArrayElementType::Bool),
        ValueType::Str => Some(ArrayElementType::String),
        ValueType::Char => Some(ArrayElementType::Char),
        ValueType::Symbol => Some(ArrayElementType::Symbol),
        ValueType::ArrayOf(elem_type, Some(ndims)) => Some(ArrayElementType::Abstract(
            array_type_name_for_ndims(elem_type, *ndims),
        )),
        _ => None,
    }
}

fn array_type_name_for_ndims(elem_type: &ArrayElementType, ndims: usize) -> String {
    let elem_name = elem_type.julia_type_name();
    match ndims {
        1 => format!("Vector{{{elem_name}}}"),
        2 => format!("Matrix{{{elem_name}}}"),
        n => format!("Array{{{elem_name}, {n}}}"),
    }
}

fn array_element_type_from_constructor_name(name: &str) -> Option<ArrayElementType> {
    match name {
        "Int" if crate::types::native_int_type_name() == "Int32" => Some(ArrayElementType::I32),
        "Int" | "Int64" => Some(ArrayElementType::I64),
        "Int8" => Some(ArrayElementType::I8),
        "Int16" => Some(ArrayElementType::I16),
        "Int32" => Some(ArrayElementType::I32),
        "UInt" if crate::types::native_uint_type_name() == "UInt32" => Some(ArrayElementType::U32),
        "UInt" | "UInt64" => Some(ArrayElementType::U64),
        "UInt8" => Some(ArrayElementType::U8),
        "UInt16" => Some(ArrayElementType::U16),
        "UInt32" => Some(ArrayElementType::U32),
        "BigInt" => Some(ArrayElementType::Abstract("BigInt".to_string())),
        "Float64" => Some(ArrayElementType::F64),
        "Float32" => Some(ArrayElementType::F32),
        "Float16" => Some(ArrayElementType::F16),
        "BigFloat" => Some(ArrayElementType::Abstract("BigFloat".to_string())),
        "Number" => Some(ArrayElementType::Abstract("Number".to_string())),
        "Real" => Some(ArrayElementType::Abstract("Real".to_string())),
        "Integer" => Some(ArrayElementType::Abstract("Integer".to_string())),
        "Signed" => Some(ArrayElementType::Abstract("Signed".to_string())),
        "Unsigned" => Some(ArrayElementType::Abstract("Unsigned".to_string())),
        "AbstractFloat" => Some(ArrayElementType::Abstract("AbstractFloat".to_string())),
        "Bool" => Some(ArrayElementType::Bool),
        "String" => Some(ArrayElementType::String),
        "Char" => Some(ArrayElementType::Char),
        "Any" => Some(ArrayElementType::Any),
        _ if name.starts_with("SubArray{") => Some(ArrayElementType::Abstract(name.to_string())),
        _ => None,
    }
}

fn typed_array_literal_element_type(iter: &Expr) -> Option<ArrayElementType> {
    let Expr::Index { array, indices, .. } = iter else {
        return None;
    };
    if indices
        .iter()
        .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }))
    {
        return None;
    }
    let Expr::Var(name, _) = array.as_ref() else {
        return None;
    };
    array_element_type_from_constructor_name(name)
}

fn pair_type_name(base: &str, params: &[TypeExpr]) -> Option<String> {
    if base != "Pair" {
        return None;
    }
    if params.is_empty() {
        Some("Pair".to_string())
    } else {
        Some(TypeExpr::format_parameterized("Pair", params))
    }
}

fn subarray_type_name(base: &str, params: &[TypeExpr]) -> Option<String> {
    if base != "SubArray" || params.is_empty() {
        return None;
    }
    Some(TypeExpr::format_parameterized("SubArray", params))
}

/// Preserve the declared element type of an empty typed array whose element
/// type is a *concrete* parametric type with no dedicated storage tag, e.g.
/// `UnitRange{Int64}`, `Vector{Int}`, `Tuple{Int64,String}` (Issue #6768).
///
/// Without this, `Vector{UnitRange{Int64}}()` and `UnitRange{Int64}[]` widened
/// the element type to `Any`, so `typeof` reported `Vector{Any}` instead of
/// `Vector{UnitRange{Int64}}`. Concrete numeric / Complex / Pair / SubArray
/// element types are handled by their own arms before this fallback is reached.
///
/// Returns `None` (caller falls back to `Any`) when the type still mentions a
/// type variable, since an unbound parameter cannot be a concrete eltype.
fn concrete_parametric_abstract_element_type(
    base: &str,
    params: &[TypeExpr],
) -> Option<ArrayElementType> {
    if params.is_empty() || !params.iter().all(TypeExpr::is_concrete) {
        return None;
    }
    // Reuse the canonical TypeExpr display so the stored name matches the
    // surface syntax (`UnitRange{Int64}`, `Vector{Int64}`, ...). The name is
    // re-parsed through `JuliaType::from_name_or_struct` for `typeof`/`eltype`,
    // which normalizes aliases like `Int` -> `Int64`.
    Some(ArrayElementType::Abstract(TypeExpr::format_parameterized(
        base, params,
    )))
}

fn plain_unary_var_call<'a>(expr: &'a Expr, var: &str) -> Option<(&'a str, Span)> {
    if let Expr::Call {
        function,
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    } = expr
    {
        let plain_unary_var_call = kwargs.is_empty()
            && kwargs_splat_mask.iter().all(|&is_splat| !is_splat)
            && splat_mask.iter().all(|&is_splat| !is_splat)
            && args.len() == 1
            && matches!(&args[0], Expr::Var(arg, _) if arg == var);
        if plain_unary_var_call {
            return Some((function.as_str(), *span));
        }
    }
    None
}

/// Whether `name` is the pure-Julia `Iterators.Filter` constructor (Issue #9200
/// S3), tolerating the bare `Filter`, a parametric `Filter{...}`, and a qualified
/// `Base.Iterators.Filter` spelling.
fn is_filter_ctor_name(name: &str) -> bool {
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    base == "Filter"
}

/// Extract the callable name of a by-value function reference (`Var` /
/// `FunctionRef`), used to re-express the desugared filtered generator's
/// by-value `map` / `pred` arguments as unary calls (Issue #9200 S3).
fn callable_ref_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(name, _) | Expr::FunctionRef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn is_lifted_generator_predicate_name(name: &str) -> bool {
    let name = name.strip_prefix("function ").unwrap_or(name);
    let leaf = name.rsplit('#').next().unwrap_or(name);
    leaf.starts_with("__gen_pred_")
}

fn is_static_filter_predicate_call(function: &str, arg_count: usize) -> bool {
    if arg_count != 1 {
        return false;
    }
    let normalized = function
        .strip_prefix("function ")
        .unwrap_or(function)
        .strip_prefix("Base.")
        .unwrap_or_else(|| function.strip_prefix("function ").unwrap_or(function));
    matches!(
        normalized,
        "iszero" | "isone" | "signbit" | "iseven" | "isodd"
    )
}

fn expr_names_type(expr: &Expr, expected: &str) -> bool {
    matches!(expr, Expr::Var(name, _) | Expr::FunctionRef { name, .. } if name == expected)
}

fn expr_static_array_element_type(expr: &Expr) -> Option<ArrayElementType> {
    match expr {
        Expr::Var(name, _) | Expr::FunctionRef { name, .. } => {
            array_element_type_from_constructor_name(name)
        }
        _ => None,
    }
}

fn nonnegative_integer_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(Literal::Int(value), _) => *value >= 0,
        Expr::Literal(Literal::Int128(value), _) => *value >= 0,
        Expr::Literal(Literal::BigInt(value), _) => !value.starts_with('-'),
        Expr::Literal(Literal::Bool(_), _) => true,
        _ => false,
    }
}

fn literal_i128(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Int(value), _) => Some(i128::from(*value)),
        Expr::Literal(Literal::Int128(value), _) => Some(*value),
        Expr::Literal(Literal::Bool(value), _) => Some(if *value { 1 } else { 0 }),
        _ => None,
    }
}

fn expr_is_compile_time_false_bool(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(Literal::Bool(false), _) => true,
        Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand,
            ..
        } => matches!(operand.as_ref(), Expr::Literal(Literal::Bool(true), _)),
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let Some(left) = literal_i128(left) else {
                return false;
            };
            let Some(right) = literal_i128(right) else {
                return false;
            };
            match op {
                BinaryOp::Lt => left >= right,
                BinaryOp::Gt => left <= right,
                BinaryOp::Le => left > right,
                BinaryOp::Ge => left < right,
                BinaryOp::Eq => left != right,
                BinaryOp::Ne => left == right,
                _ => false,
            }
        }
        _ => false,
    }
}

fn preserves_type_for_positive_integer_power(base_type: &ValueType, exponent: &Expr) -> bool {
    nonnegative_integer_literal(exponent)
        && matches!(
            base_type,
            ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::BigInt
                | ValueType::F16
                | ValueType::F32
                | ValueType::F64
                | ValueType::BigFloat
        )
}

/// Build a plain unary call `func(var)` — the shape `compile_generator_expr`'s
/// filtered path recognizes via `plain_unary_var_call` (Issue #9200 S3).
fn unary_var_call(func: &str, var: &str, span: Span) -> Expr {
    Expr::Call {
        function: func.to_string().into(),
        args: vec![Expr::Var(var.to_string().into(), span)],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Join a comprehension iterator range's `start`/`step`/`stop` inferred types
/// into the loop-variable element type (Issue #9293).
///
/// The element type of a range is NOT taken from `start` alone: for `0:0.5:6`
/// the start `0` is `I64` but the runtime elements are `Float64`, so pinning the
/// loop variable to `I64` makes the element store fail with `expected I64, got
/// Float64`. This mirrors the `Stmt::For` `needs_typed_range` gate fixed for
/// Issue #9291 (compile/stmt.rs): a float anywhere in the range makes the
/// elements float, and an `Any`-inferred *step* means the runtime step (and thus
/// element) type is statically unknown, so we route through the runtime-typed
/// (typejoin) `Any` element path.
///
/// Scoping matches the for-head fix: only the *step* being `Any` diverts to the
/// `Any` path. An `Any` *bound* on an otherwise-integer range (e.g. `1:n` in
/// `[i for i in 1:n]`) keeps the current `start`-derived element type so the hot
/// integer-typed slot path is preserved.
fn range_comprehension_elem_type(
    start_ty: ValueType,
    step_ty: Option<ValueType>,
    stop_ty: ValueType,
) -> ValueType {
    // An `Any`-inferred explicit step (e.g. `0:st:6` with an unannotated `st`, or
    // a computed float step whose operands infer `Any`) makes the runtime element
    // type unknown -> runtime-typed path.
    if matches!(step_ty, Some(ValueType::Any)) {
        return ValueType::Any;
    }

    let is_std_float =
        |t: &ValueType| matches!(t, ValueType::F64 | ValueType::F32 | ValueType::F16);
    let is_float = |t: &ValueType| is_std_float(t) || matches!(t, ValueType::BigFloat);

    // Collect the distinct float width across start/step/stop. A range is float
    // iff any component is float; when they share a single standard float width
    // (F16/F32/F64) that width IS the element type (matches the runtime range's
    // element type). Mixed float widths (e.g. `0:0.5f0:6.0`) or a `BigFloat`
    // component fall back to the runtime typejoin path, decoupling from the exact
    // range-promotion rule.
    let mut float_width: Option<ValueType> = None;
    let mut mixed_float = false;
    let mut has_bigfloat = false;
    for ty in [Some(&start_ty), step_ty.as_ref(), Some(&stop_ty)]
        .into_iter()
        .flatten()
    {
        if matches!(ty, ValueType::BigFloat) {
            has_bigfloat = true;
        }
        if is_float(ty) {
            match &float_width {
                None => float_width = Some(ty.clone()),
                Some(existing) if existing == ty => {}
                Some(_) => mixed_float = true,
            }
        }
    }

    match float_width {
        Some(width) if !mixed_float && !has_bigfloat && is_std_float(&width) => width,
        Some(_) => ValueType::Any,
        // No float component and the step is not `Any`: keep the previous
        // `start`-derived behavior (preserves the integer/char typed-slot path).
        None => start_ty,
    }
}

impl CoreCompiler<'_> {
    fn type_expr_requires_runtime_value(&self, type_arg: &TypeExpr) -> bool {
        match type_arg {
            TypeExpr::TypeVar(name) => {
                self.current_type_param_index.contains_key(name.as_str())
                    || self.locals.get(name) == Some(&ValueType::DataType)
                    || self.locals.get(name) == Some(&ValueType::Any)
            }
            TypeExpr::Parameterized { params, .. } => params
                .iter()
                .any(|param| self.type_expr_requires_runtime_value(param)),
            TypeExpr::RuntimeExpr(_) => true,
            TypeExpr::Concrete(_) => false,
        }
    }

    /// Push the runtime `DataType` value denoted by a `TypeExpr`, resolving
    /// `where`-clause type variables through the active frame's type bindings.
    /// Used to materialize the explicit type parameters of `new{A,B}(...)`
    /// inside a parametric inner constructor (Issue #5059).
    pub(in super::super) fn compile_type_expr_as_value(
        &mut self,
        type_arg: &TypeExpr,
    ) -> CResult<()> {
        self.emit_type_expr_value_for_array_alloc(Some(type_arg))
    }

    /// Whether a `new{...}` type argument can be resolved to a concrete
    /// `DataType` from the current constructor frame: a literal concrete type,
    /// or a `where`-clause type variable recoverable from an argument
    /// (`ctor_arg_bound_type_vars`), or an explicit constructor-self binder
    /// supplied by CallStaticParametric / the callable DataType
    /// (`ctor_self_bound_type_vars`, Issue #10959).
    pub(in super::super) fn type_expr_is_resolvable(&self, type_arg: &TypeExpr) -> bool {
        match type_arg {
            TypeExpr::Concrete(_) => true,
            TypeExpr::TypeVar(name) => {
                self.ctor_arg_bound_type_vars.contains(name.as_str())
                    || self.ctor_self_bound_type_vars.contains(name.as_str())
            }
            TypeExpr::Parameterized { params, .. } => params
                .iter()
                .all(|param| self.type_expr_is_resolvable(param)),
            // A runtime expression (e.g. `new{elem_type(R), ...}`) is resolved by
            // evaluating it in the constructor frame at runtime, so the explicit
            // `NewDynamicParametricStruct` path can build the concrete parametric
            // type from the computed `DataType` values instead of collapsing to
            // `{Any}` via the legacy `NewParametricStruct` fallback (Issue #7935).
            TypeExpr::RuntimeExpr(_) => true,
        }
    }

    fn emit_type_expr_value_for_array_alloc(&mut self, type_arg: Option<&TypeExpr>) -> CResult<()> {
        match type_arg {
            Some(TypeExpr::Concrete(jt)) => {
                self.emit(Instr::PushDataType(jt.name().to_string()));
            }
            Some(TypeExpr::TypeVar(name)) => {
                let is_type_binding = self.current_type_param_index.contains_key(name.as_str());
                let is_runtime_type = self.locals.get(name) == Some(&ValueType::DataType)
                    || self.locals.get(name) == Some(&ValueType::Any);
                if is_type_binding {
                    self.emit(Instr::LoadTypeBinding(name.clone()));
                } else if is_runtime_type {
                    self.emit(Instr::LoadAny(name.clone()));
                } else {
                    self.emit(Instr::PushDataType(name.clone()));
                }
            }
            Some(TypeExpr::Parameterized { base, params }) => {
                let explicit_base_owner = base
                    .strip_prefix("Base.")
                    .is_some_and(|name| self.shared_ctx.base_parametric_structs.contains_key(name));
                let resolved_base = if explicit_base_owner {
                    base.clone()
                } else {
                    self.resolve_parametric_struct_name(base)
                        .unwrap_or_else(|| base.clone())
                };
                if params
                    .iter()
                    .any(|param| self.type_expr_requires_runtime_value(param))
                {
                    for param in params {
                        self.emit_type_expr_value_for_array_alloc(Some(param))?;
                    }
                    self.emit(Instr::ConstructParametricType(resolved_base, params.len()));
                } else {
                    let type_name = format!(
                        "{}{{{}}}",
                        resolved_base,
                        params
                            .iter()
                            .map(|param| param.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    self.emit(Instr::PushDataType(type_name));
                }
            }
            Some(TypeExpr::RuntimeExpr(expr_str)) => {
                if let Ok(expr) = crate::lowering::lower_expr_from_text(expr_str) {
                    self.compile_expr(&expr)?;
                } else {
                    self.emit(Instr::LoadAny(expr_str.clone()));
                }
            }
            None => {
                self.emit(Instr::PushDataType("Any".to_string()));
            }
        }
        Ok(())
    }

    fn emit_array_undef_from_dims_call(
        &mut self,
        type_arg: Option<&TypeExpr>,
        dim_args: &[Expr],
        tuple_dims: bool,
    ) -> CResult<()> {
        self.emit_type_expr_value_for_array_alloc(type_arg)?;
        if tuple_dims {
            self.compile_expr(&dim_args[0])?;
        } else {
            for dim_arg in dim_args {
                self.compile_expr_as(dim_arg, ValueType::I64)?;
            }
            self.emit(Instr::NewTuple(dim_args.len()));
        }
        self.emit(Instr::PushFunction("_array_undef_from_dims".to_string()));
        self.emit(Instr::CallFunctionVariable(2));
        Ok(())
    }

    fn infer_tail_stmt_type(&mut self, stmt: &Stmt) -> Option<ValueType> {
        match stmt {
            Stmt::Expr { expr, .. }
            | Stmt::Return {
                value: Some(expr), ..
            } => Some(self.infer_expr_type(expr)),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                let then_ty = then_branch
                    .stmts
                    .last()
                    .and_then(|stmt| self.infer_tail_stmt_type(stmt))?;
                let else_ty = else_branch
                    .as_ref()
                    .and_then(|branch| branch.stmts.last())
                    .and_then(|stmt| self.infer_tail_stmt_type(stmt))?;
                if then_ty == else_ty {
                    Some(then_ty)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn infer_empty_collection_body_type(
        &mut self,
        body: &Expr,
        var: &str,
        iter_elem_type: &ValueType,
        depth: usize,
    ) -> Option<ValueType> {
        if depth > 4 {
            return None;
        }

        match body {
            Expr::Var(name, _) if name == var => Some(iter_elem_type.clone()),
            Expr::BinaryOp {
                op: BinaryOp::Pow,
                left,
                right,
                ..
            } => {
                let base_type = self
                    .infer_empty_collection_body_type(left, var, iter_elem_type, depth + 1)
                    .or_else(|| {
                        let ty = self.infer_expr_type(left);
                        (!matches!(ty, ValueType::Any)).then_some(ty)
                    })?;
                preserves_type_for_positive_integer_power(&base_type, right).then_some(base_type)
            }
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|&is_splat| !is_splat)
                && splat_mask.iter().all(|&is_splat| !is_splat) =>
            {
                if function == "convert" && args.len() == 2 {
                    if expr_names_type(&args[0], "Any") {
                        return self.infer_empty_collection_body_type(
                            &args[1],
                            var,
                            iter_elem_type,
                            depth + 1,
                        );
                    }
                    if let Some(element_type) = expr_static_array_element_type(&args[0]) {
                        if !matches!(element_type, ArrayElementType::Any) {
                            return Some(element_type.to_value_type());
                        }
                    }
                }

                if let Some(element_type) = array_element_type_from_constructor_name(function) {
                    if !matches!(element_type, ArrayElementType::Any) {
                        return Some(element_type.to_value_type());
                    }
                }

                let arg_types: Option<Vec<ValueType>> = args
                    .iter()
                    .map(|arg| {
                        self.infer_empty_collection_body_type(arg, var, iter_elem_type, depth + 1)
                            .or_else(|| {
                                let ty = self.infer_expr_type(arg);
                                (!matches!(ty, ValueType::Any)).then_some(ty)
                            })
                    })
                    .collect();
                self.infer_named_call_return_type(function, &arg_types?, depth + 1)
            }
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let then_ty = self
                    .infer_empty_collection_body_type(then_expr, var, iter_elem_type, depth + 1)
                    .or_else(|| {
                        let ty = self.infer_expr_type(then_expr);
                        (!matches!(ty, ValueType::Any)).then_some(ty)
                    })?;
                let else_ty = self
                    .infer_empty_collection_body_type(else_expr, var, iter_elem_type, depth + 1)
                    .or_else(|| {
                        let ty = self.infer_expr_type(else_expr);
                        (!matches!(ty, ValueType::Any)).then_some(ty)
                    })?;
                if then_ty == else_ty {
                    Some(then_ty)
                } else if numeric_typejoin_array_element_type(&then_ty, &else_ty).is_some() {
                    Some(ValueType::ArrayOf(
                        numeric_typejoin_array_element_type(&then_ty, &else_ty)?,
                        None,
                    ))
                } else {
                    None
                }
            }
            _ => {
                let ty = self.infer_expr_type(body);
                (!matches!(ty, ValueType::Any)).then_some(ty)
            }
        }
    }

    fn infer_named_call_return_type(
        &mut self,
        function: &str,
        arg_types: &[ValueType],
        depth: usize,
    ) -> Option<ValueType> {
        if depth > 4 {
            return None;
        }

        let normalized_func_name = function.strip_prefix("function ").unwrap_or(function);
        if arg_types.len() == 1 {
            if matches!(
                normalized_func_name,
                "iszero" | "isone" | "signbit" | "iseven" | "isodd"
            ) {
                return Some(ValueType::Bool);
            }
            if matches!(normalized_func_name, "identity" | "abs" | "abs2" | "-") {
                return arg_types.first().cloned();
            }
        }

        if let Some(func_ir) = self.function_ir_for_empty_collection_callable(function) {
            if let Some(inferred) =
                self.infer_function_ir_return_type_for_empty_collection(&func_ir, arg_types, depth)
            {
                return Some(inferred);
            }
        }

        let table = self.method_tables.get(function)?;
        let julia_arg_types: Vec<_> = arg_types
            .iter()
            .map(|arg_type| self.value_type_to_julia_type(arg_type))
            .collect();
        let method = table.dispatch(&julia_arg_types).ok()?;
        if !matches!(method.return_type, ValueType::Any) {
            return Some(method.return_type.clone());
        }

        let func_ir = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&method.global_index)?
            .clone();
        let inferred = self.infer_shared_function_return_type_with_arg_types(&func_ir, arg_types);
        if !matches!(inferred, ValueType::Any) {
            return Some(inferred);
        }

        self.infer_function_ir_return_type_for_empty_collection(&func_ir, arg_types, depth)
    }

    fn infer_function_ir_return_type_for_empty_collection(
        &mut self,
        func_ir: &Function,
        arg_types: &[ValueType],
        depth: usize,
    ) -> Option<ValueType> {
        if func_ir.params.len() != arg_types.len() {
            return None;
        }
        let Stmt::Return {
            value: Some(expr), ..
        } = func_ir.body.stmts.first()?
        else {
            return None;
        };

        let old_bindings: Vec<_> = func_ir
            .params
            .iter()
            .zip(arg_types.iter())
            .map(|(param, arg_type)| {
                (
                    param.name.clone(),
                    self.locals.insert(param.name.clone(), arg_type.clone()),
                )
            })
            .collect();
        let inferred = self.infer_empty_collection_body_type(expr, "", &ValueType::Any, depth + 1);
        for (name, old) in old_bindings {
            match old {
                Some(value) => {
                    self.locals.insert(name, value);
                }
                None => {
                    self.locals.remove(&name);
                }
            }
        }
        inferred
    }

    fn function_ir_for_empty_collection_callable(&self, function: &str) -> Option<Function> {
        let lexical_index = self.current_function_name.as_ref().and_then(|current| {
            let segments: Vec<&str> = current.split('#').collect();
            (1..=segments.len()).rev().find_map(|depth| {
                let nested = format!("{}#{function}", segments[..depth].join("#"));
                self.shared_ctx.function_indices.get(&nested).copied()
            })
        });
        let module_index = self.current_module_path.as_ref().and_then(|module_path| {
            self.shared_ctx
                .function_indices
                .get(&format!("{module_path}.{function}"))
                .copied()
        });
        let global_index = lexical_index
            .or(module_index)
            .or_else(|| self.shared_ctx.function_indices.get(function).copied())?;
        self.shared_ctx
            .function_ir_by_global_index
            .get(&global_index)
            .cloned()
    }

    fn stmt_has_nontransparent_filter_call(&self, stmt: &Stmt, depth: usize) -> bool {
        if depth > 8 {
            return true;
        }
        match stmt {
            Stmt::Block(block) => block
                .stmts
                .iter()
                .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1)),
            Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
                var.starts_with("__sjulia_inline_arg_")
                    || self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Stmt::Expr { expr: value, .. }
            | Stmt::Return {
                value: Some(value), ..
            } => {
                expr_is_compile_time_false_bool(value)
                    || self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.expr_has_nontransparent_filter_call(start, depth + 1)
                    || self.expr_has_nontransparent_filter_call(end, depth + 1)
                    || step.as_ref().is_some_and(|step| {
                        self.expr_has_nontransparent_filter_call(step, depth + 1)
                    })
                    || body
                        .stmts
                        .iter()
                        .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
            }
            Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
                self.expr_has_nontransparent_filter_call(iterable, depth + 1)
                    || body
                        .stmts
                        .iter()
                        .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expr_has_nontransparent_filter_call(condition, depth + 1)
                    || body
                        .stmts
                        .iter()
                        .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expr_has_nontransparent_filter_call(condition, depth + 1)
                    || then_branch
                        .stmts
                        .iter()
                        .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
                    || else_branch.as_ref().is_some_and(|branch| {
                        branch
                            .stmts
                            .iter()
                            .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
                    })
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => [
                Some(try_block),
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|block| {
                block
                    .stmts
                    .iter()
                    .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
            }),
            Stmt::Test { condition, .. } => {
                self.expr_has_nontransparent_filter_call(condition, depth + 1)
            }
            Stmt::TestSet { body, .. } => body
                .stmts
                .iter()
                .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1)),
            Stmt::TestThrows { expr, .. } => {
                self.expr_has_nontransparent_filter_call(expr, depth + 1)
            }
            Stmt::IndexAssign { indices, value, .. } => {
                indices
                    .iter()
                    .any(|idx| self.expr_has_nontransparent_filter_call(idx, depth + 1))
                    || self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Stmt::FieldAssign { value, .. } | Stmt::DestructuringAssign { value, .. } => {
                self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Stmt::DictAssign { key, value, .. } => {
                self.expr_has_nontransparent_filter_call(key, depth + 1)
                    || self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Stmt::Timed { body, .. } => body
                .stmts
                .iter()
                .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1)),
            Stmt::Return { value: None, .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Meta { .. }
            | Stmt::LocalDecl { .. }
            | Stmt::FunctionDef { .. }
            | Stmt::EvalFunctionDef { .. }
            | Stmt::Using { .. }
            | Stmt::Export { .. }
            | Stmt::Global { .. }
            | Stmt::Label { .. }
            | Stmt::Goto { .. }
            | Stmt::EnumDef { .. }
            | Stmt::RuntimeNominalDef { .. } => false,
        }
    }

    fn lifted_predicate_has_nontransparent_filter_call(
        &self,
        function: &str,
        depth: usize,
    ) -> Option<bool> {
        if !is_lifted_generator_predicate_name(function) {
            return None;
        }
        let func_ir = self.function_ir_for_empty_collection_callable(function)?;
        Some(
            func_ir
                .body
                .stmts
                .iter()
                .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1)),
        )
    }

    fn expr_has_nontransparent_filter_call(&self, expr: &Expr, depth: usize) -> bool {
        if depth > 8 {
            return true;
        }
        match expr {
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => {
                let child_has_call = args
                    .iter()
                    .any(|arg| self.expr_has_nontransparent_filter_call(arg, depth + 1))
                    || kwargs
                        .iter()
                        .any(|(_, arg)| self.expr_has_nontransparent_filter_call(arg, depth + 1));
                if child_has_call {
                    return true;
                }
                if !kwargs.is_empty()
                    || splat_mask.iter().any(|&is_splat| is_splat)
                    || kwargs_splat_mask.iter().any(|&is_splat| is_splat)
                {
                    return true;
                }
                if is_static_filter_predicate_call(function, args.len()) {
                    return false;
                }
                self.lifted_predicate_has_nontransparent_filter_call(function, depth)
                    .unwrap_or(true)
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => {
                let child_has_call = args
                    .iter()
                    .any(|arg| self.expr_has_nontransparent_filter_call(arg, depth + 1))
                    || kwargs
                        .iter()
                        .any(|(_, arg)| self.expr_has_nontransparent_filter_call(arg, depth + 1));
                if child_has_call {
                    return true;
                }
                if module == "Base"
                    && kwargs.is_empty()
                    && splat_mask.iter().all(|&is_splat| !is_splat)
                    && kwargs_splat_mask.iter().all(|&is_splat| !is_splat)
                    && is_static_filter_predicate_call(function, args.len())
                {
                    return false;
                }
                true
            }
            Expr::BinaryOp { left, right, .. } => {
                self.expr_has_nontransparent_filter_call(left, depth + 1)
                    || self.expr_has_nontransparent_filter_call(right, depth + 1)
            }
            Expr::UnaryOp { operand, .. }
            | Expr::FieldAccess {
                object: operand, ..
            }
            | Expr::QuoteLiteral {
                constructor: operand,
                ..
            }
            | Expr::ReturnExpr {
                value: Some(operand),
                ..
            } => self.expr_has_nontransparent_filter_call(operand, depth + 1),
            Expr::Builtin { args, .. }
            | Expr::ArrayLiteral { elements: args, .. }
            | Expr::TupleLiteral { elements: args, .. } => args
                .iter()
                .any(|arg| self.expr_has_nontransparent_filter_call(arg, depth + 1)),
            Expr::Index { array, indices, .. } => {
                self.expr_has_nontransparent_filter_call(array, depth + 1)
                    || indices
                        .iter()
                        .any(|arg| self.expr_has_nontransparent_filter_call(arg, depth + 1))
            }
            Expr::Range {
                start, step, stop, ..
            } => {
                self.expr_has_nontransparent_filter_call(start, depth + 1)
                    || step.as_ref().is_some_and(|step| {
                        self.expr_has_nontransparent_filter_call(step, depth + 1)
                    })
                    || self.expr_has_nontransparent_filter_call(stop, depth + 1)
            }
            Expr::Comprehension {
                body, iter, filter, ..
            }
            | Expr::Generator {
                body, iter, filter, ..
            } => {
                self.expr_has_nontransparent_filter_call(body, depth + 1)
                    || self.expr_has_nontransparent_filter_call(iter, depth + 1)
                    || filter.as_ref().is_some_and(|filter| {
                        self.expr_has_nontransparent_filter_call(filter, depth + 1)
                    })
            }
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                self.expr_has_nontransparent_filter_call(body, depth + 1)
                    || iterations
                        .iter()
                        .any(|(_, iter)| self.expr_has_nontransparent_filter_call(iter, depth + 1))
                    || filter.as_ref().is_some_and(|filter| {
                        self.expr_has_nontransparent_filter_call(filter, depth + 1)
                    })
            }
            Expr::NamedTupleLiteral { fields, .. } => fields
                .iter()
                .any(|(_, value)| self.expr_has_nontransparent_filter_call(value, depth + 1)),
            Expr::Pair { key, value, .. } => {
                self.expr_has_nontransparent_filter_call(key, depth + 1)
                    || self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
                self.expr_has_nontransparent_filter_call(key, depth + 1)
                    || self.expr_has_nontransparent_filter_call(value, depth + 1)
            }),
            Expr::LetBlock { bindings, body, .. } => {
                bindings.iter().any(|(name, value)| {
                    name.starts_with("__sjulia_inline_arg_")
                        || self.expr_has_nontransparent_filter_call(value, depth + 1)
                }) || body
                    .stmts
                    .iter()
                    .any(|stmt| self.stmt_has_nontransparent_filter_call(stmt, depth + 1))
            }
            Expr::StringConcat { parts, .. } => parts
                .iter()
                .any(|part| self.expr_has_nontransparent_filter_call(part, depth + 1)),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                self.expr_has_nontransparent_filter_call(condition, depth + 1)
                    || self.expr_has_nontransparent_filter_call(then_expr, depth + 1)
                    || self.expr_has_nontransparent_filter_call(else_expr, depth + 1)
            }
            Expr::New { args, .. } => args
                .iter()
                .any(|arg| self.expr_has_nontransparent_filter_call(arg, depth + 1)),
            Expr::DynamicTypeConstruct {
                base_expr,
                type_args,
                ..
            } => {
                base_expr
                    .as_ref()
                    .is_some_and(|base| self.expr_has_nontransparent_filter_call(base, depth + 1))
                    || type_args
                        .iter()
                        .any(|arg| self.expr_has_nontransparent_filter_call(arg, depth + 1))
            }
            Expr::AssignExpr { value, .. } => {
                self.expr_has_nontransparent_filter_call(value, depth + 1)
            }
            Expr::Convert { operand, .. } => {
                self.expr_has_nontransparent_filter_call(operand, depth + 1)
            }
            Expr::Literal(_, _)
            | Expr::Var(_, _)
            | Expr::TypedEmptyArray { .. }
            | Expr::SliceAll { .. }
            | Expr::FunctionRef { .. }
            | Expr::ReturnExpr { value: None, .. }
            | Expr::BreakExpr { .. }
            | Expr::ContinueExpr { .. } => false,
        }
    }

    fn filtered_generator_result_element_type(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter_expr: &Expr,
    ) -> Option<ArrayElementType> {
        if self.expr_has_nontransparent_filter_call(filter_expr, 0) {
            return None;
        }
        let iter_elem_type = self.generator_iter_element_type(iter);
        self.empty_collection_body_element_type(body, var, &iter_elem_type)
    }

    fn empty_collection_body_element_type(
        &mut self,
        body: &Expr,
        var: &str,
        iter_elem_type: &ValueType,
    ) -> Option<ArrayElementType> {
        let inferred = self.infer_empty_collection_body_type(body, var, iter_elem_type, 0)?;
        match inferred {
            ValueType::ArrayOf(element_type, _) => Some(element_type),
            ValueType::Union(types) => numeric_union_typejoin_array_element_type(&types),
            other => value_type_to_array_element_type(&other),
        }
    }

    fn generator_iter_element_type(&mut self, iter: &Expr) -> ValueType {
        match iter {
            Expr::Range {
                start, step, stop, ..
            } => {
                let start_ty = self.infer_expr_type(start);
                let step_ty = step.as_ref().map(|s| self.infer_expr_type(s));
                let stop_ty = self.infer_expr_type(stop);
                range_comprehension_elem_type(start_ty, step_ty, stop_ty)
            }
            Expr::TupleLiteral { elements, .. } => {
                let element_types: Vec<ValueType> = elements
                    .iter()
                    .map(|element| self.infer_expr_type(element))
                    .collect();
                if let Some((first, rest)) = element_types.split_first() {
                    if rest.iter().all(|element_type| element_type == first) {
                        first.clone()
                    } else {
                        ValueType::Any
                    }
                } else {
                    ValueType::Any
                }
            }
            _ => {
                let iter_ty = typed_array_literal_element_type(iter)
                    .map(|e| ValueType::ArrayOf(e, None))
                    .unwrap_or_else(|| self.infer_expr_type(iter));
                match iter_ty {
                    ValueType::ArrayOf(ref elem, _) => elem.to_value_type(),
                    _ => ValueType::Any,
                }
            }
        }
    }

    fn infer_local_unary_call_return_type(
        &mut self,
        function: &str,
        arg_type: &ValueType,
    ) -> Option<ValueType> {
        if !self.locals.contains_key(function) {
            return None;
        }

        let arg_julia_type = self.value_type_to_julia_type(arg_type);
        let global_index = self
            .method_tables
            .get(function)?
            .dispatch(&[arg_julia_type])
            .ok()?
            .global_index;
        let func_ir = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&global_index)?
            .clone();
        let param = func_ir.params.first()?;
        if func_ir.params.len() != 1 || param.is_varargs {
            return None;
        }

        let param_name = param.name.clone();
        let old_param_type = self.locals.insert(param_name.clone(), arg_type.clone());
        let inferred = func_ir
            .body
            .stmts
            .last()
            .and_then(|stmt| self.infer_tail_stmt_type(stmt));

        match old_param_type {
            Some(old) => {
                self.locals.insert(param_name, old);
            }
            None => {
                self.locals.remove(&param_name);
            }
        }

        match inferred {
            Some(ValueType::Any) | None => None,
            other => other,
        }
    }

    fn comprehension_typejoin_element_type(
        &mut self,
        body: &Expr,
        inferred_body_type: &ValueType,
    ) -> Option<ArrayElementType> {
        let Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } = body
        else {
            return None;
        };

        let then_type = self.infer_expr_type(then_expr);
        let else_type = self.infer_expr_type(else_expr);
        if then_type == else_type {
            return None;
        }

        let joined = numeric_typejoin_array_element_type(&then_type, &else_type)?;
        if matches!(joined, ArrayElementType::F64) && matches!(inferred_body_type, ValueType::F64) {
            return None;
        }
        Some(joined)
    }

    /// Compile an integer-typed range (`start:stop` or `start:step:stop`) whose
    /// runtime `stop` bound may arrive as a `Float`, coercing that bound to `Int`
    /// with upstream `UnitRange{Int}` / `StepRange{Int}` last-element semantics
    /// (Issue #9321).
    ///
    /// The caller only takes this path on the I64 element fast path, so `start`
    /// is statically `Int64`-typed and `step` is a concrete integer; coercing the
    /// bound keeps the resulting lazy `Range` fully integer-valued (it iterates
    /// `I64` elements) while its length still matches upstream's float range
    /// (`length(1:5.5) == length(1:5) == 5`). The bound is compiled in its
    /// natural type (preserving a runtime `Float`) so `CoerceRangeStopI64` can
    /// `floor`/`ceil` it toward the step direction — a strict `I64` load would
    /// instead type-error on the `Float`. The `start`/`step` operands pushed
    /// beneath the bound double as the instruction's direction operands: a
    /// non-finite / out-of-`Int64` bound in the counting direction raises the
    /// upstream `InexactError`, while the empty direction coerces to an empty
    /// range (Issue #9377).
    fn compile_integer_range_with_coerced_stop(
        &mut self,
        start: &Expr,
        step: Option<&Expr>,
        stop: &Expr,
    ) -> CResult<ValueType> {
        let explicit_step = step.is_some();
        self.compile_expr_as(start, ValueType::I64)?;
        if let Some(step_expr) = step {
            self.compile_expr_as(step_expr, ValueType::I64)?;
        } else {
            self.emit(Instr::PushI64(1));
        }
        self.compile_expr(stop)?;
        self.emit(Instr::CoerceRangeStopI64);
        self.emit(if explicit_step {
            Instr::MakeStepRangeLazy
        } else {
            Instr::MakeRangeLazy
        });
        Ok(ValueType::Range)
    }

    /// Compile a comprehension iterator, applying the Issue #9321 integer-range
    /// bound coercion when the iterator is an integer-typed range
    /// (`iter_elem_type == I64`) whose runtime `stop` bound infers `Any` (e.g.
    /// `[i for i in 1:n]` with `n = 5.5`).
    ///
    /// Such a range keeps the `Int64` element fast path (per #9291/#9293), but a
    /// runtime `Float` bound would otherwise build a `Float` range whose elements
    /// crash the `I64` element store / slotized `I64` arithmetic. Coercing the
    /// bound to `Int` (via [`Self::compile_integer_range_with_coerced_stop`])
    /// keeps the elements `I64` while the length still matches upstream's `Float`
    /// range (`length(1:5.5) == length(1:5) == 5`). Shared by the single-var,
    /// cartesian, and flatten comprehension arms so the three stay consistent.
    /// All other iterators (arrays, float ranges, statically integer bounds)
    /// compile unchanged.
    fn compile_comprehension_range_iter(
        &mut self,
        iter: &Expr,
        iter_elem_type: &ValueType,
    ) -> CResult<ValueType> {
        if let Expr::Range {
            start, step, stop, ..
        } = iter
        {
            if *iter_elem_type == ValueType::I64
                && matches!(self.infer_expr_type(stop), ValueType::Any)
            {
                return self.compile_integer_range_with_coerced_stop(start, step.as_deref(), stop);
            }
        }
        self.compile_expr(iter)
    }

    fn emit_store_comprehension_len_i64(&mut self, iter_var: &str, len_var: String) {
        self.emit(Instr::LoadAny(iter_var.to_string()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        self.emit(Instr::CallBuiltin(BuiltinId::Int64, 1));
        self.emit(Instr::StoreI64(len_var));
    }

    pub(in super::super) fn compile_comprehension(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let runtime_elem = self.runtime_typed_comprehension_element_type(body);
        self.compile_comprehension_with_elem_inner(
            body,
            var,
            iter,
            filter,
            None,
            runtime_elem.as_ref(),
        )
    }

    fn runtime_typed_comprehension_element_type(&self, body: &Expr) -> Option<TypeExpr> {
        let Expr::Call { function, .. } = body else {
            return None;
        };
        let is_type_binding = self
            .current_type_param_index
            .contains_key(function.as_str());
        let is_runtime_type = self.locals.get(function.as_str()) == Some(&ValueType::DataType);
        if !is_type_binding && !is_runtime_type {
            let (base, params) = parse_parametric_call(function.as_str())?;
            let type_arg = TypeExpr::Parameterized { base, params };
            return self
                .type_expr_requires_runtime_value(&type_arg)
                .then_some(type_arg);
        }
        let type_arg = TypeExpr::TypeVar(function.to_string());
        self.type_expr_requires_runtime_value(&type_arg)
            .then_some(type_arg)
    }

    /// Compile a comprehension, optionally forcing the result element type.
    ///
    /// When `forced_elem` is `Some(t)`, the result `Vector{t}` is allocated
    /// with that exact element type and every element is pushed through the
    /// generic `ArrayPush` path (the body is expected to already produce a
    /// value of type `t`, e.g. via a `convert(t, x)` wrapper). This is used by
    /// the typed-comprehension intercept (`T[expr for x in iter]`) for element
    /// types whose body type cannot be inferred statically (e.g. `convert`
    /// returns `Any` at compile time) so the runtime eltype still matches
    /// upstream Julia exactly (Issue #5040).
    pub(in super::super) fn compile_comprehension_with_elem(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
    ) -> CResult<ValueType> {
        self.compile_comprehension_with_elem_inner(body, var, iter, filter, forced_elem, None)
    }

    pub(in super::super) fn compile_comprehension_with_runtime_elem(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
        forced_runtime_elem: &TypeExpr,
    ) -> CResult<ValueType> {
        self.compile_comprehension_with_elem_inner(
            body,
            var,
            iter,
            filter,
            None,
            Some(forced_runtime_elem),
        )
    }

    fn compile_comprehension_with_elem_inner(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
        forced_runtime_elem: Option<&TypeExpr>,
    ) -> CResult<ValueType> {
        if let Some(tuple_vars) = decode_tuple_comprehension_binding(var) {
            return self.compile_tuple_destructuring_comprehension(body, &tuple_vars, iter, filter);
        }

        let previous_locals = self.locals.clone();
        let previous_initialized_locals = self.initialized_locals.clone();
        let previous_julia_type_locals = self.julia_type_locals.clone();
        let previous_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
        let previous_mixed_type_vars = self.mixed_type_vars.clone();

        let result = self.compile_single_comprehension_scoped(
            body,
            var,
            iter,
            filter,
            forced_elem,
            forced_runtime_elem,
            &previous_locals,
            &previous_initialized_locals,
            &previous_julia_type_locals,
            &previous_known_any_rank_array_locals,
            &previous_mixed_type_vars,
        );

        self.locals = previous_locals;
        self.initialized_locals = previous_initialized_locals;
        self.julia_type_locals = previous_julia_type_locals;
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals;
        self.mixed_type_vars = previous_mixed_type_vars;
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_single_comprehension_scoped(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
        forced_runtime_elem: Option<&TypeExpr>,
        previous_locals: &std::collections::HashMap<String, ValueType>,
        previous_initialized_locals: &std::collections::HashSet<String>,
        previous_julia_type_locals: &std::collections::HashMap<String, crate::types::JuliaType>,
        previous_known_any_rank_array_locals: &std::collections::HashSet<String>,
        previous_mixed_type_vars: &std::collections::HashSet<String>,
    ) -> CResult<ValueType> {
        let assignment_owners =
            comprehension_assignment_owner_names(body, filter, std::iter::empty());
        // Issue #10984 / #10903: `var` is a fresh binding for the
        // comprehension's lifetime. If it already shadows a live outer
        // local, save the outer value/type state now, before the
        // comprehension's own binding overwrites it below, and restore it at
        // the single exit point of whichever runtime path (iterate-protocol
        // or range/array fast path) actually gets compiled.
        let shadow = if self.explicit_lexical_scopes {
            None
        } else {
            Some(self.shadow_local_enter(var)?)
        };

        let result_var = self.new_temp("comp_result");
        let iter_var = self.new_temp("comp_iter");
        let idx_var = self.new_temp("comp_idx");
        let len_var = self.new_temp("comp_len");
        let state_var = self.new_temp("comp_state");
        let iter_result_var = self.new_temp("comp_iter_result");
        let explicit_lexical = self.explicit_lexical_scopes;
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![
                result_var.clone(),
                iter_var.clone(),
                idx_var.clone(),
                len_var.clone(),
                state_var.clone(),
                iter_result_var.clone(),
            ]);
        }
        let mut iter_typejoin_element_type = None;
        let iter_value_type = self.infer_expr_type(iter);
        let iter_uses_iterate_protocol = comprehension_iter_uses_iterate_protocol(&iter_value_type);

        // Step 1: Infer iterator element type and register loop variable (Issue #2125)
        // For ranges like 1:5, the element type is I64. For arrays, use the element type.
        let iter_elem_type = match iter {
            Expr::Range {
                start, step, stop, ..
            } => {
                // Join the element type across start/step/stop, not from `start`
                // alone: `0:0.5:6` has an `I64` start but `Float64` elements
                // (Issue #9293). Mirrors the `Stmt::For` gate fixed for #9291.
                let start_ty = self.infer_expr_type(start);
                let step_ty = step.as_ref().map(|s| self.infer_expr_type(s));
                let stop_ty = self.infer_expr_type(stop);
                range_comprehension_elem_type(start_ty, step_ty, stop_ty)
            }
            Expr::TupleLiteral { elements, .. } => {
                let element_types: Vec<ValueType> = elements
                    .iter()
                    .map(|element| self.infer_expr_type(element))
                    .collect();
                if let Some((first, rest)) = element_types.split_first() {
                    if rest.iter().all(|element_type| element_type == first) {
                        first.clone()
                    } else {
                        iter_typejoin_element_type =
                            tuple_literal_typejoin_element_type(&element_types);
                        ValueType::Any
                    }
                } else {
                    ValueType::Any
                }
            }
            _ => {
                let iter_ty = typed_array_literal_element_type(iter)
                    .map(|e| ValueType::ArrayOf(e, None))
                    .unwrap_or_else(|| self.infer_expr_type(iter));
                match iter_ty {
                    ValueType::ArrayOf(ref elem, _) => elem.to_value_type(),
                    _ => ValueType::Any,
                }
            }
        };
        self.locals.insert(var.to_owned(), iter_elem_type.clone());
        // Truthful inside the comprehension body (the element store dominates
        // every body evaluation) — lets a nested same-name shadowing
        // construct emit its guarded save (Issue #10984 hardening;
        // `shadow_local_exit` restores the pre-enter membership).
        self.initialized_locals.insert(var.to_owned());
        for name in &assignment_owners {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
        }

        // Step 2: Infer body type (now uses properly typed loop variable)
        let body_type = plain_unary_var_call(body, var)
            .and_then(|(function, _)| {
                self.infer_local_unary_call_return_type(function, &iter_elem_type)
            })
            .unwrap_or_else(|| self.infer_expr_type(body));
        let forced_elem_set = forced_elem.is_some() || forced_runtime_elem.is_some();
        let runtime_typejoin_result = !forced_elem_set
            && match &body_type {
                ValueType::Any => true,
                ValueType::Union(types) => {
                    numeric_union_typejoin_array_element_type(types).is_none()
                }
                _ => false,
            };
        // Issue #10315: runtime type-join can narrow an unresolved body from
        // the compiler's `Any` placeholder to a concrete runtime eltype. Keep
        // the known rank while marking the element as unresolved so an
        // assigned comprehension does not become a proven `Vector{Any}` and
        // statically bind the wrong overload.
        let result_rank = runtime_typejoin_result.then_some(1);
        let runtime_typejoin_empty_element_type = if runtime_typejoin_result {
            self.empty_collection_body_element_type(body, var, &iter_elem_type)
        } else {
            None
        };

        // Step 3: Create empty result array with appropriate type (Issue #2125)
        // Fallback for unknown body types used to be `ArrayElementType::F64`,
        // which silently coerced non-numeric Any-typed bodies (e.g. the
        // result of `convert(Any, x)` or any call returning `Any`) into a
        // `Vector{Float64}` with coerced element values. For untyped
        // comprehensions whose body still infers as `Any` or as a non-numeric
        // `Union`, collect through the runtime typejoin push path so non-empty
        // results narrow from observed values just like `collect(generator)`
        // (Issue #9385). Empty results have no observed values, so carry a
        // separate conservative body-eltype default when one is defensible
        // (Issue #9789).
        let array_elem_type = if let Some(forced) = forced_elem {
            // Typed comprehension `T[...]` with an explicit target element type
            // whose body type cannot be inferred statically (Issue #5040).
            forced
        } else if runtime_typejoin_result {
            runtime_typejoin_empty_element_type.unwrap_or(ArrayElementType::Any)
        } else {
            self.comprehension_typejoin_element_type(body, &body_type)
                .or_else(|| match body {
                    Expr::Var(name, _) if name == var => iter_typejoin_element_type.clone(),
                    _ => None,
                })
                .or_else(|| match &body_type {
                    ValueType::Union(types) => numeric_union_typejoin_array_element_type(types),
                    _ => None,
                })
                .or_else(|| value_type_to_array_element_type(&body_type))
                .unwrap_or(ArrayElementType::Any)
        };

        // Type inference above needs the binder's temporary type, but the
        // iterator expression is outside that binder's lexical scope. Restore
        // the exact enclosing compiler state before emitting the iterator;
        // the binder is reintroduced only after its runtime owner is entered.
        self.locals = previous_locals.clone();
        self.initialized_locals = previous_initialized_locals.clone();
        self.julia_type_locals = previous_julia_type_locals.clone();
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals.clone();
        self.mixed_type_vars = previous_mixed_type_vars.clone();

        if let Some(type_arg) = forced_runtime_elem {
            // `T[expr for ...]` where `T` is a method type parameter must
            // allocate `Vector{T}` using the active frame's type binding rather
            // than falling back to `Vector{Any}` (Issue #8364).
            self.emit_type_expr_value_for_array_alloc(Some(type_arg))?;
            self.emit(Instr::PushI64(0));
            self.emit(Instr::NewMemoryDynamicTyped);
            self.emit_array_wrapper_from_memory_on_stack(&[0]);
            self.locals.insert(result_var.clone(), ValueType::Array);
        } else {
            self.emit_empty_array_wrapper(array_elem_type.clone(), &[0]);
            self.locals.insert(
                result_var.clone(),
                ValueType::ArrayOf(array_elem_type.clone(), None),
            );
        }
        self.emit(Instr::StoreArray(result_var.clone()));

        if iter_uses_iterate_protocol {
            let iterable_ty = self.infer_julia_type(iter);
            let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

            self.compile_expr(iter)?;
            self.emit(Instr::StoreAny(iter_var.clone()));
            if explicit_lexical {
                let mut owner_names = vec![var.to_owned()];
                owner_names.extend(assignment_owners.iter().cloned());
                self.enter_explicit_lexical_scope(owner_names);
            }
            self.locals.insert(var.to_owned(), iter_elem_type.clone());
            self.initialized_locals.insert(var.to_owned());
            for name in &assignment_owners {
                self.locals.insert(name.clone(), ValueType::Any);
                self.initialized_locals.remove(name);
            }

            self.emit(Instr::LoadAny(iter_var.clone()));
            if use_pure_julia_iterate {
                self.emit_iterate_call_1(&iterable_ty)?;
            } else {
                self.emit(Instr::IterateFirst);
            }
            self.emit(Instr::StoreAny(iter_result_var.clone()));

            self.emit(Instr::LoadAny(iter_result_var.clone()));
            self.emit(Instr::IsNothing);
            let j_continue_first = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            let j_exit_first = self.here();
            self.emit(Instr::Jump(usize::MAX));

            let continue_after_first_check = self.here();
            self.patch_jump(j_continue_first, continue_after_first_check);

            self.emit(Instr::LoadAny(iter_result_var.clone()));
            self.emit(Instr::TupleSecond);
            self.emit(Instr::StoreAny(state_var.clone()));
            self.emit(Instr::LoadAny(iter_result_var.clone()));
            self.emit(Instr::TupleFirst);
            self.emit(Instr::StoreAny(var.to_owned()));
            self.locals.insert(var.to_owned(), iter_elem_type.clone());

            let loop_start = self.here();

            let j_skip = if let Some(filter_expr) = filter {
                self.compile_expr_as(filter_expr, ValueType::Bool)?;
                let j = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                Some(j)
            } else {
                None
            };

            self.emit_comprehension_body_push(
                body,
                &body_type,
                &result_var,
                runtime_typejoin_result,
                forced_elem_set,
                &array_elem_type,
            )?;

            if let Some(j) = j_skip {
                let skip_label = self.here();
                self.patch_jump(j, skip_label);
            }

            self.emit(Instr::LoadAny(iter_var));
            self.emit(Instr::LoadAny(state_var.clone()));
            if use_pure_julia_iterate {
                self.emit_iterate_call_2(&iterable_ty)?;
            } else {
                self.emit(Instr::IterateNext);
            }
            self.emit(Instr::StoreAny(iter_result_var.clone()));

            self.emit(Instr::LoadAny(iter_result_var.clone()));
            self.emit(Instr::IsNothing);
            let j_continue_next = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            let j_exit_next = self.here();
            self.emit(Instr::Jump(usize::MAX));

            let continue_after_next_check = self.here();
            self.patch_jump(j_continue_next, continue_after_next_check);

            self.emit(Instr::LoadAny(iter_result_var.clone()));
            self.emit(Instr::TupleSecond);
            self.emit(Instr::StoreAny(state_var));
            self.emit(Instr::LoadAny(iter_result_var));
            self.emit(Instr::TupleFirst);
            self.emit(Instr::StoreAny(var.to_owned()));
            self.locals.insert(var.to_owned(), iter_elem_type.clone());
            self.emit(Instr::Jump(loop_start));

            let exit_label = self.here();
            self.patch_jump(j_exit_first, exit_label);
            self.patch_jump(j_exit_next, exit_label);

            // Issue #10984 / #10903: restore a shadowed outer local, if any.
            if explicit_lexical {
                self.exit_explicit_lexical_scope();
            } else if let Some(shadow) = shadow {
                self.shadow_local_exit(shadow);
            }
            self.emit(Instr::LoadArray(result_var));
            if explicit_lexical {
                self.exit_explicit_lexical_scope();
            }
            if forced_runtime_elem.is_some() {
                return Ok(ValueType::Array);
            }
            return Ok(ValueType::ArrayOf(array_elem_type, result_rank));
        }

        // Compile iterator (can be Array or Range). Issue #9321: an
        // integer-typed range whose bound infers `Any` (e.g. `[i for i in 1:n]`)
        // keeps this I64 element fast path but coerces a runtime `Float` bound to
        // `Int` so the elements stay `I64` — see `compile_comprehension_range_iter`.
        let iter_type = self.compile_comprehension_range_iter(iter, &iter_elem_type)?;
        self.locals.insert(iter_var.clone(), iter_type);
        // Use StoreAny/LoadAny to handle both Array and Range iterators
        self.emit(Instr::StoreAny(iter_var.clone()));
        if explicit_lexical {
            let mut owner_names = vec![var.to_owned()];
            owner_names.extend(assignment_owners.iter().cloned());
            self.enter_explicit_lexical_scope(owner_names);
        }
        self.locals.insert(var.to_owned(), iter_elem_type.clone());
        self.initialized_locals.insert(var.to_owned());
        for name in &assignment_owners {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
        }

        // Get internal loop length. Public `length(::UnitRange{BigInt})`
        // returns BigInt, but comprehension loop counters and allocation sizes
        // are Int-sized just like upstream array construction.
        self.emit_store_comprehension_len_i64(&iter_var, len_var.clone());

        // Pre-size the backing storage for a filter-free comprehension whose
        // final length is now known in `len_var` (Issue #5186). Without a filter
        // the result has exactly `len_var` elements, so reserving up front lets
        // the per-iteration `ArrayPush` skip the O(log n) reallocations of
        // capacity-0 growth. With a filter the final length is unknown, so we
        // keep growing on demand. `ReserveArray` is a pure capacity hint (a
        // non-positive length is a no-op), so this never changes results.
        if filter.is_none() {
            self.emit(Instr::LoadArray(result_var.clone()));
            self.emit(Instr::LoadI64(len_var.clone()));
            self.emit(Instr::ReserveArray);
            self.emit(Instr::StoreArray(result_var.clone()));
        }

        // Initialize index
        self.emit(Instr::PushI64(1));
        self.emit(Instr::StoreI64(idx_var.clone()));

        let loop_start = self.here();

        // Check if done
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::LoadI64(len_var.clone()));
        self.emit(Instr::GtI64);
        let j_continue = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_label = self.here();
        self.patch_jump(j_continue, continue_label);

        // Get current element (use Any to handle Array, Range, and other types)
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::IndexLoad(1));
        self.emit(Instr::StoreAny(var.to_owned()));
        self.locals.insert(var.to_owned(), iter_elem_type);

        // Apply filter if present
        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        self.emit_comprehension_body_push(
            body,
            &body_type,
            &result_var,
            runtime_typejoin_result,
            forced_elem_set,
            &array_elem_type,
        )?;

        // Skip label
        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        // Increment index
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var.clone()));

        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit, exit_label);

        // Issue #10984 / #10903: restore a shadowed outer local, if any.
        if explicit_lexical {
            self.exit_explicit_lexical_scope();
        } else if let Some(shadow) = shadow {
            self.shadow_local_exit(shadow);
        }

        // Load result and return appropriate type (Issue #2125)
        self.emit(Instr::LoadArray(result_var));
        if explicit_lexical {
            self.exit_explicit_lexical_scope();
        }
        if forced_runtime_elem.is_some() {
            Ok(ValueType::Array)
        } else {
            Ok(ValueType::ArrayOf(array_elem_type, result_rank))
        }
    }

    fn emit_comprehension_body_push(
        &mut self,
        body: &Expr,
        body_type: &ValueType,
        result_var: &str,
        runtime_typejoin_result: bool,
        forced_elem_set: bool,
        array_elem_type: &ArrayElementType,
    ) -> CResult<()> {
        let temp_val = self.new_temp("comp_val");
        let temp_scope = self.enter_explicit_lexical_scope(vec![temp_val.clone()]);
        let result = (|| {
            if runtime_typejoin_result {
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.to_string()));
                self.emit(Instr::LoadAny(temp_val));
                self.emit(Instr::ArrayPushTypejoin);
                self.emit(Instr::StoreArray(result_var.to_string()));
                return Ok(());
            }

            if forced_elem_set
                || matches!(
                    array_elem_type,
                    ArrayElementType::Any
                        | ArrayElementType::UnionOf(_)
                        | ArrayElementType::Abstract(_)
                        | ArrayElementType::Structured(_)
                )
            {
                // Forced-eltype typed comprehensions (Issue #5040): the body
                // (`convert(T, x)`) already yields a value of the target element
                // type, so push it through the generic boxed path. The array was
                // allocated as a Memory-backed `Array{T}` wrapper, so the runtime
                // eltype stays exactly `T` while each converted element is stored as-is.
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.to_string()));
                self.emit(Instr::LoadAny(temp_val));
                self.emit(Instr::ArrayPush);
                self.emit(Instr::StoreArray(result_var.to_string()));
                return Ok(());
            }

            match body_type {
                ValueType::Tuple => {
                    // Tuple: compile as-is and store as Any
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadAny(temp_val));
                }
                ValueType::I64 => {
                    self.compile_expr_as(body, ValueType::I64)?;
                    self.emit(Instr::StoreI64(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadI64(temp_val));
                }
                ValueType::Bool => {
                    self.compile_expr_as(body, ValueType::Bool)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadAny(temp_val));
                }
                ValueType::Str => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadAny(temp_val));
                }
                ValueType::Char => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadAny(temp_val));
                }
                ValueType::Symbol => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadAny(temp_val));
                }
                ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::F16
                | ValueType::F32 => {
                    // F16 must ride this value-preserving boxed path: the `_`
                    // fallback below coerces the body to F64, which widened
                    // `[Float16(i) for i in 1:3]` elements to Float64 even
                    // though the container tag was Float16 (Issue #9382).
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadAny(temp_val));
                }
                _ => {
                    // Default: F64
                    self.compile_expr_as(body, ValueType::F64)?;
                    self.emit(Instr::StoreF64(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.to_string()));
                    self.emit(Instr::LoadF64(temp_val));
                }
            }
            self.emit(Instr::ArrayPush);
            self.emit(Instr::StoreArray(result_var.to_string()));
            Ok(())
        })();
        if temp_scope {
            self.exit_explicit_lexical_scope();
        }
        result
    }

    /// Compile `[body for (a, b) in iterable]` through Julia's iteration
    /// protocol, matching statement `for (a, b) in ...` and supporting Dict
    /// Pair iteration without requiring indexable collection storage.
    fn compile_tuple_destructuring_comprehension(
        &mut self,
        body: &Expr,
        tuple_vars: &[String],
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let previous_locals = self.locals.clone();
        let previous_initialized_locals = self.initialized_locals.clone();
        let previous_julia_type_locals = self.julia_type_locals.clone();
        let previous_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
        let previous_mixed_type_vars = self.mixed_type_vars.clone();

        let result =
            self.compile_tuple_destructuring_comprehension_scoped(body, tuple_vars, iter, filter);

        self.locals = previous_locals;
        self.initialized_locals = previous_initialized_locals;
        self.julia_type_locals = previous_julia_type_locals;
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals;
        self.mixed_type_vars = previous_mixed_type_vars;
        result
    }

    fn compile_tuple_destructuring_comprehension_scoped(
        &mut self,
        body: &Expr,
        tuple_vars: &[String],
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let assignment_owners =
            comprehension_assignment_owner_names(body, filter, std::iter::empty());
        // Issue #10984 / #10903: each destructured var is a fresh binding for
        // this comprehension's lifetime. Save any shadowed outer locals now,
        // before the destructure below overwrites them, and restore them at
        // the comprehension's single exit convergence point (`exit_label`).
        // Mirrors `Stmt::ForEachTuple`'s identical treatment in `stmt.rs`.
        let shadows: Vec<_> = if self.explicit_lexical_scopes {
            Vec::new()
        } else {
            tuple_vars
                .iter()
                .map(|v| self.shadow_local_enter(v))
                .collect::<CResult<_>>()?
        };

        let result_var = self.new_temp("comp_result");
        let iterable_var = self.new_temp("comp_iterable");
        let state_var = self.new_temp("comp_state");
        let iter_result_var = self.new_temp("comp_iter_result");
        let elem_var = self.new_temp("comp_elem");
        let temp_val = self.new_temp("comp_val");
        let array_elem_type = ArrayElementType::UnionOf(Vec::new());
        let explicit_lexical = self.explicit_lexical_scopes;
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![
                result_var.clone(),
                iterable_var.clone(),
                state_var.clone(),
                iter_result_var.clone(),
                elem_var.clone(),
                temp_val.clone(),
            ]);
        }

        self.emit_empty_array_wrapper(array_elem_type.clone(), &[0]);
        self.locals.insert(
            result_var.clone(),
            ValueType::ArrayOf(array_elem_type.clone(), None),
        );
        self.emit(Instr::StoreArray(result_var.clone()));

        let iterable_ty = self.infer_julia_type(iter);
        let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);
        self.compile_expr(iter)?;
        self.emit(Instr::StoreAny(iterable_var.clone()));
        if explicit_lexical {
            let mut owner_names = tuple_vars.to_vec();
            owner_names.extend(assignment_owners.iter().cloned());
            self.enter_explicit_lexical_scope(owner_names);
        }
        for var in tuple_vars {
            self.locals.insert(var.clone(), ValueType::Any);
            self.initialized_locals.insert(var.clone());
        }
        for name in &assignment_owners {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
        }

        self.emit(Instr::LoadAny(iterable_var.clone()));
        if use_pure_julia_iterate {
            self.emit_iterate_call_1(&iterable_ty)?;
        } else {
            self.emit(Instr::IterateFirst);
        }
        self.emit(Instr::StoreAny(iter_result_var.clone()));

        self.emit(Instr::LoadAny(iter_result_var.clone()));
        self.emit(Instr::IsNothing);
        let j_continue_first = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit_first = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_after_first_check = self.here();
        self.patch_jump(j_continue_first, continue_after_first_check);

        self.emit(Instr::LoadAny(iter_result_var.clone()));
        self.emit(Instr::TupleSecond);
        self.emit(Instr::StoreAny(state_var.clone()));
        self.emit(Instr::LoadAny(iter_result_var.clone()));
        self.emit(Instr::TupleFirst);
        self.emit(Instr::StoreAny(elem_var.clone()));

        let loop_start = self.here();

        for (index, var) in tuple_vars.iter().enumerate() {
            self.emit(Instr::LoadAny(elem_var.clone()));
            self.emit(Instr::PushI64((index + 1) as i64));
            self.emit(Instr::TupleGet);
            self.emit(Instr::StoreAny(var.to_string()));
            self.locals.insert(var.to_string(), ValueType::Any);
            // Truthful inside the body (the destructure store above
            // dominates every body evaluation) — lets a nested same-name
            // shadowing construct emit its guarded save (Issue #10984
            // hardening; exit restores pre-enter membership).
            self.initialized_locals.insert(var.to_string());
        }

        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        self.compile_expr(body)?;
        self.emit(Instr::StoreAny(temp_val.clone()));
        self.emit(Instr::LoadArray(result_var.clone()));
        self.emit(Instr::LoadAny(temp_val));
        self.emit(Instr::ArrayPushTypejoin);
        self.emit(Instr::StoreArray(result_var.clone()));

        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        self.emit(Instr::LoadAny(iterable_var));
        self.emit(Instr::LoadAny(state_var.clone()));
        if use_pure_julia_iterate {
            self.emit_iterate_call_2(&iterable_ty)?;
        } else {
            self.emit(Instr::IterateNext);
        }
        self.emit(Instr::StoreAny(iter_result_var.clone()));

        self.emit(Instr::LoadAny(iter_result_var.clone()));
        self.emit(Instr::IsNothing);
        let j_continue_next = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit_next = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_after_next_check = self.here();
        self.patch_jump(j_continue_next, continue_after_next_check);

        self.emit(Instr::LoadAny(iter_result_var.clone()));
        self.emit(Instr::TupleSecond);
        self.emit(Instr::StoreAny(state_var));
        self.emit(Instr::LoadAny(iter_result_var));
        self.emit(Instr::TupleFirst);
        self.emit(Instr::StoreAny(elem_var));
        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit_first, exit_label);
        self.patch_jump(j_exit_next, exit_label);

        if explicit_lexical {
            self.exit_explicit_lexical_scope();
        } else {
            for shadow in shadows {
                self.shadow_local_exit(shadow);
            }
        }

        self.emit(Instr::LoadArray(result_var));
        if explicit_lexical {
            self.exit_explicit_lexical_scope();
        }
        // Issue #10315: `UnionOf([])` is the runtime collector's internal
        // bottom sentinel, not a statically proven `Vector{Union{}}`. Tuple
        // comprehensions always push through runtime type-join, so expose the
        // same rank-known/element-unresolved result as the general path.
        Ok(ValueType::ArrayOf(ArrayElementType::Any, Some(1)))
    }

    /// Compile a multi-variable comprehension: [expr for var1 in iter1, var2 in iter2, ...]
    /// Produces a flat array via nested loops (cartesian product). Issue #2143.
    pub(in super::super) fn compile_multi_comprehension(
        &mut self,
        body: &Expr,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        filter: Option<&Expr>,
        flatten: bool,
    ) -> CResult<ValueType> {
        self.compile_multi_comprehension_with_elem(body, iterations, filter, None, flatten)
    }

    /// Multi-iterator comprehension, optionally forcing the result element
    /// type. See `compile_comprehension_with_elem` for the rationale; this is
    /// the multi-iterator (`Matrix{T}`) analogue used by the typed
    /// comprehension intercept for `Bool`/`Char`/`Symbol`/`String` whose body
    /// has been rewritten to `convert(T, x)` (Issue #5040).
    ///
    /// `flatten == true` selects the whitespace `for ... for ...` flatten form,
    /// which yields a 1-D `Vector` (`Iterators.flatten` semantics) rather than
    /// the comma form's N-D cartesian array (Issue #8014).
    pub(in super::super) fn compile_multi_comprehension_with_elem(
        &mut self,
        body: &Expr,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
        flatten: bool,
    ) -> CResult<ValueType> {
        if flatten {
            return self.compile_flatten_comprehension(body, iterations, filter, forced_elem);
        }

        let previous_locals = self.locals.clone();
        let previous_initialized_locals = self.initialized_locals.clone();
        let previous_julia_type_locals = self.julia_type_locals.clone();
        let previous_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
        let previous_mixed_type_vars = self.mixed_type_vars.clone();

        let result = self.compile_cartesian_comprehension_scoped(
            body,
            iterations,
            filter,
            forced_elem,
            &previous_locals,
            &previous_initialized_locals,
            &previous_julia_type_locals,
            &previous_known_any_rank_array_locals,
            &previous_mixed_type_vars,
        );

        self.locals = previous_locals;
        self.initialized_locals = previous_initialized_locals;
        self.julia_type_locals = previous_julia_type_locals;
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals;
        self.mixed_type_vars = previous_mixed_type_vars;
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_cartesian_comprehension_scoped(
        &mut self,
        body: &Expr,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
        previous_locals: &std::collections::HashMap<String, ValueType>,
        previous_initialized_locals: &std::collections::HashSet<String>,
        previous_julia_type_locals: &std::collections::HashMap<String, crate::types::JuliaType>,
        previous_known_any_rank_array_locals: &std::collections::HashSet<String>,
        previous_mixed_type_vars: &std::collections::HashSet<String>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("mcomp_result");
        // Comma-form cartesian iterator expressions are evaluated outside the
        // comprehension binding owners; only its body/filter introduce locals.
        let assignment_owners =
            comprehension_assignment_owner_names(body, filter, std::iter::empty());

        // Register all loop variables for type inference. Keep each iterator's
        // inferred element type so the compile loop below can apply the Issue
        // #9321 integer-range bound coercion (`compile_comprehension_range_iter`).
        let mut iter_elem_types = Vec::with_capacity(iterations.len());
        for (var, iter) in iterations {
            let iter_elem_type = match iter {
                Expr::Range {
                    start, step, stop, ..
                } => {
                    // Join start/step/stop, not `start` alone (Issue #9293).
                    let start_ty = self.infer_expr_type(start);
                    let step_ty = step.as_ref().map(|s| self.infer_expr_type(s));
                    let stop_ty = self.infer_expr_type(stop);
                    range_comprehension_elem_type(start_ty, step_ty, stop_ty)
                }
                _ => {
                    let iter_ty = self.infer_expr_type(iter);
                    match iter_ty {
                        ValueType::ArrayOf(ref elem, _) => match elem {
                            ArrayElementType::I64 => ValueType::I64,
                            ArrayElementType::F64 => ValueType::F64,
                            ArrayElementType::F16 => ValueType::F16, // Issue #9301
                            ArrayElementType::F32 => ValueType::F32,
                            ArrayElementType::Bool => ValueType::Bool,
                            ArrayElementType::String => ValueType::Str,
                            ArrayElementType::Char => ValueType::Char,
                            ArrayElementType::Symbol => ValueType::Symbol,
                            _ => ValueType::Any,
                        },
                        _ => ValueType::Any,
                    }
                }
            };
            self.locals.insert(var.to_string(), iter_elem_type.clone());
            iter_elem_types.push(iter_elem_type);
        }
        for name in &assignment_owners {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
        }

        // Infer body type with all loop variables registered
        let body_type = self.infer_expr_type(body);

        // Create empty result array with appropriate type. The
        // fallback for unmatched body types used to be
        // `ArrayElementType::F64`, which silently coerced Any-typed
        // bodies (e.g. multi-op expressions like `i+j+k` that body
        // inference cannot resolve) into an `Array{Float64, N}` with
        // coerced element values. Defaulting to `Any` instead matches
        // upstream Julia's "preserve the value, not a guessed shape"
        // behavior (Issue #4836, sibling of #4822 / PR #4823 for the
        // single-iterator path).
        let array_elem_type = if let Some(forced) = forced_elem {
            // Typed comprehension `T[expr for i in R1, j in R2]` (Issue #5040):
            // the body is `convert(T, expr)` which infers as `Any` at compile
            // time, so honor the explicit target element type here. The push
            // path below takes the generic `_` (Any) arm, storing each already
            // converted element as-is into the Memory-backed `Array{T}` matrix.
            forced
        } else {
            match body_type {
                ValueType::Tuple => ArrayElementType::Any,
                ValueType::I64 => ArrayElementType::I64,
                ValueType::F16 => ArrayElementType::F16, // Issue #9301
                ValueType::F32 => ArrayElementType::F32,
                ValueType::F64 => ArrayElementType::F64,
                ValueType::Bool => ArrayElementType::Bool,
                ValueType::Str => ArrayElementType::String,
                ValueType::Char => ArrayElementType::Char,
                ValueType::Symbol => ArrayElementType::Symbol,
                _ => ArrayElementType::Any,
            }
        };

        // Binder types above exist only to infer later iterators and the body.
        // Cartesian iterator expressions are emitted outside the new binder
        // owners, so restore the exact enclosing metadata before codegen.
        self.locals = previous_locals.clone();
        self.initialized_locals = previous_initialized_locals.clone();
        self.julia_type_locals = previous_julia_type_locals.clone();
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals.clone();
        self.mixed_type_vars = previous_mixed_type_vars.clone();

        let n = iterations.len();
        let mut iter_vars = Vec::with_capacity(n);
        let mut idx_vars = Vec::with_capacity(n);
        let mut len_vars = Vec::with_capacity(n);
        for _ in 0..n {
            iter_vars.push(self.new_temp("mcomp_iter"));
            idx_vars.push(self.new_temp("mcomp_idx"));
            len_vars.push(self.new_temp("mcomp_len"));
        }
        let temp_val = self.new_temp("mcomp_val");
        let explicit_lexical = self.explicit_lexical_scopes;
        let hidden_scope = if explicit_lexical {
            let mut hidden_names = vec![result_var.clone(), temp_val.clone()];
            hidden_names.extend(iter_vars.iter().cloned());
            hidden_names.extend(idx_vars.iter().cloned());
            hidden_names.extend(len_vars.iter().cloned());
            self.enter_explicit_lexical_scope(hidden_names)
        } else {
            false
        };

        self.emit_empty_array_wrapper(array_elem_type.clone(), &[0]);
        self.locals.insert(
            result_var.clone(),
            ValueType::ArrayOf(array_elem_type.clone(), None),
        );
        self.emit(Instr::StoreArray(result_var.clone()));

        // For each iteration clause, compile the iterator and prepare loop vars
        for (i, (_, iter_expr)) in iterations.iter().enumerate() {
            let iter_var = &iter_vars[i];
            let idx_var = &idx_vars[i];
            let len_var = &len_vars[i];

            // Compile and store iterator. Issue #9321: coerce a runtime `Float`
            // bound of an integer range to `Int` (as in the single-var arm) so
            // `IndexLoad` yields `I64` elements matching the loop var's inferred
            // `I64` type — otherwise `[i+j for i in 1:n, j in 1:2]` with `n = 5.5`
            // feeds a `Float` `i` into the slotized `I64` body arithmetic.
            self.compile_comprehension_range_iter(iter_expr, &iter_elem_types[i])?;
            self.locals.insert(iter_var.clone(), ValueType::Any);
            self.emit(Instr::StoreAny(iter_var.clone()));

            // Get internal loop length; see the single-comprehension arm for
            // the BigInt-range length boundary.
            self.emit_store_comprehension_len_i64(iter_var, len_var.clone());

            // Initialize index to 1
            self.emit(Instr::PushI64(1));
            self.emit(Instr::StoreI64(idx_var.clone()));
        }

        let binder_scope = if explicit_lexical {
            let mut owner_names: Vec<_> =
                iterations.iter().map(|(var, _)| var.to_string()).collect();
            owner_names.extend(assignment_owners.iter().cloned());
            self.enter_explicit_lexical_scope(owner_names)
        } else {
            false
        };
        for ((var, _), elem_ty) in iterations.iter().zip(&iter_elem_types) {
            self.locals.insert(var.to_string(), elem_ty.clone());
            self.initialized_locals.insert(var.to_string());
        }
        for name in &assignment_owners {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
        }

        // Generate nested loops: outermost = LAST iteration (column-major order)
        // Julia: [f(i,j) for i in 1:3, j in 1:3] iterates as (1,1),(2,1),(3,1),(1,2),...
        let mut loop_starts = Vec::with_capacity(n);
        let mut j_exits = Vec::with_capacity(n);

        for ri in (0..n).rev() {
            let loop_start = self.here();
            loop_starts.push(loop_start);

            // Check if done: idx > len
            self.emit(Instr::LoadI64(idx_vars[ri].clone()));
            self.emit(Instr::LoadI64(len_vars[ri].clone()));
            self.emit(Instr::GtI64);
            let j_continue = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            let j_exit = self.here();
            self.emit(Instr::Jump(usize::MAX));
            j_exits.push(j_exit);

            let continue_label = self.here();
            self.patch_jump(j_continue, continue_label);
        }

        // At the innermost level: bind all loop variables to current elements
        for (i, (var, _)) in iterations.iter().enumerate() {
            self.emit(Instr::LoadAny(iter_vars[i].clone()));
            self.emit(Instr::LoadI64(idx_vars[i].clone()));
            self.emit(Instr::IndexLoad(1));
            self.emit(Instr::StoreAny(var.to_string()));
        }

        // Apply filter if present
        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        // Compute body and push to result
        match body_type {
            ValueType::Tuple => {
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::I64 => {
                self.compile_expr_as(body, ValueType::I64)?;
                self.emit(Instr::StoreI64(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadI64(temp_val.clone()));
            }
            ValueType::Bool => {
                self.compile_expr_as(body, ValueType::Bool)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::Str | ValueType::Char | ValueType::Symbol => {
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::F64 => {
                // Explicit F64 body: keep the F64-typed push so the
                // result remains `Matrix{Float64}` (Issue #4836:
                // moved out of the `_` fallback so unknown-Any
                // bodies no longer ride this Float coercion path).
                self.compile_expr_as(body, ValueType::F64)?;
                self.emit(Instr::StoreF64(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadF64(temp_val.clone()));
            }
            _ => {
                // Any-typed body: preserve the value as-is via
                // StoreAny/LoadAny instead of forcing it through
                // F64 coercion. Pairs with the `array_elem_type`
                // fallback change above (Issue #4836).
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
        }
        self.emit(Instr::ArrayPush);
        self.emit(Instr::StoreArray(result_var.clone()));

        // Skip label for filter
        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        // Close nested loops: innermost first (loop_vars[0]), outermost last
        // loop_starts/j_exits were pushed in reverse: index 0 = outermost, n-1 = innermost
        for close_i in 0..n {
            let lv_idx = close_i;
            let ls_idx = n - 1 - close_i;

            // Increment index for this loop level
            self.emit(Instr::LoadI64(idx_vars[lv_idx].clone()));
            self.emit(Instr::PushI64(1));
            self.emit(Instr::AddI64);
            self.emit(Instr::StoreI64(idx_vars[lv_idx].clone()));

            // Jump back to this loop's start
            self.emit(Instr::Jump(loop_starts[ls_idx]));

            // Patch exit jump
            let exit_label = self.here();
            self.patch_jump(j_exits[ls_idx], exit_label);

            // Reset inner loop indices when outer loop iterates
            if close_i < n - 1 {
                self.emit(Instr::PushI64(1));
                self.emit(Instr::StoreI64(idx_vars[lv_idx].clone()));
            }
        }

        // Load result; for n>=2 iterations, reshape the flat array
        // into a Matrix (or higher-dim Array) matching upstream Julia's
        // `[expr for i in R1, j in R2]` shape `(length(R1), length(R2))`
        // (Issue #4779). The nested-loop iteration above already lays
        // elements out in column-major order (outermost = last
        // iteration), so the flat array's element order is exactly
        // what column-major reshape expects.
        self.emit(Instr::LoadArray(result_var));
        if n >= 2 {
            for len_var in &len_vars {
                self.emit(Instr::LoadI64(len_var.clone()));
            }
            self.emit(Instr::CallBuiltin(BuiltinId::Reshape, 1 + n));
        }
        if binder_scope {
            self.exit_explicit_lexical_scope();
        }
        if hidden_scope {
            self.exit_explicit_lexical_scope();
        }
        // Issue #6817: a multi-iterator comprehension produces an `n`-dimensional
        // array (`n` = number of iterator clauses), so report the rank to the
        // type system; otherwise typed dispatch mis-selected a `::Vector` method
        // for a 2-D `Matrix` result.
        Ok(ValueType::ArrayOf(array_elem_type, Some(n)))
    }

    /// Compile the whitespace `for ... for ...` flatten comprehension
    /// `[expr for i in R1 for j in R2 ...]` (Issue #8014).
    ///
    /// Unlike the comma cartesian form this is `Iterators.flatten` semantics: a
    /// flat 1-D `Vector`, the iterators nest with `iterations[0]` OUTERMOST, and
    /// each inner iterator is re-evaluated per outer step so dependent ranges
    /// (`for i in 1:3 for j in 1:i`) work. `iterations` arrives in
    /// outermost→innermost order from lowering.
    fn compile_flatten_comprehension(
        &mut self,
        body: &Expr,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
    ) -> CResult<ValueType> {
        let previous_locals = self.locals.clone();
        let previous_initialized_locals = self.initialized_locals.clone();
        let previous_julia_type_locals = self.julia_type_locals.clone();
        let previous_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
        let previous_mixed_type_vars = self.mixed_type_vars.clone();

        let result = self.compile_flatten_comprehension_scoped(
            body,
            iterations,
            filter,
            forced_elem,
            &previous_locals,
            &previous_initialized_locals,
            &previous_julia_type_locals,
            &previous_known_any_rank_array_locals,
            &previous_mixed_type_vars,
        );

        self.locals = previous_locals;
        self.initialized_locals = previous_initialized_locals;
        self.julia_type_locals = previous_julia_type_locals;
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals;
        self.mixed_type_vars = previous_mixed_type_vars;
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_flatten_comprehension_scoped(
        &mut self,
        body: &Expr,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
        previous_locals: &std::collections::HashMap<String, ValueType>,
        previous_initialized_locals: &std::collections::HashSet<String>,
        previous_julia_type_locals: &std::collections::HashMap<String, crate::types::JuliaType>,
        previous_known_any_rank_array_locals: &std::collections::HashSet<String>,
        previous_mixed_type_vars: &std::collections::HashSet<String>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("fcomp_result");
        let assignment_owners = comprehension_assignment_owner_names(
            body,
            filter,
            iterations.iter().skip(1).map(|(_, iter)| iter),
        );

        // Register all loop variables for type inference (mirrors the cartesian
        // path). Element type is taken from each iterator independently.
        let mut iter_elem_types = Vec::with_capacity(iterations.len());
        for (var, iter) in iterations {
            let iter_elem_type = match iter {
                Expr::Range {
                    start, step, stop, ..
                } => {
                    // Join start/step/stop, not `start` alone (Issue #9293).
                    let start_ty = self.infer_expr_type(start);
                    let step_ty = step.as_ref().map(|s| self.infer_expr_type(s));
                    let stop_ty = self.infer_expr_type(stop);
                    range_comprehension_elem_type(start_ty, step_ty, stop_ty)
                }
                _ => {
                    let iter_ty = self.infer_expr_type(iter);
                    match iter_ty {
                        ValueType::ArrayOf(ref elem, _) => match elem {
                            ArrayElementType::I64 => ValueType::I64,
                            ArrayElementType::F64 => ValueType::F64,
                            ArrayElementType::F16 => ValueType::F16, // Issue #9301
                            ArrayElementType::F32 => ValueType::F32,
                            ArrayElementType::Bool => ValueType::Bool,
                            ArrayElementType::String => ValueType::Str,
                            ArrayElementType::Char => ValueType::Char,
                            ArrayElementType::Symbol => ValueType::Symbol,
                            _ => ValueType::Any,
                        },
                        _ => ValueType::Any,
                    }
                }
            };
            self.locals.insert(var.to_string(), iter_elem_type.clone());
            self.initialized_locals.insert(var.to_string());
            iter_elem_types.push(iter_elem_type);
        }
        for name in &assignment_owners {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
        }

        let body_type = self.infer_expr_type(body);
        let array_elem_type = if let Some(forced) = forced_elem {
            forced
        } else {
            match body_type {
                ValueType::Tuple => ArrayElementType::Any,
                ValueType::I64 => ArrayElementType::I64,
                ValueType::F16 => ArrayElementType::F16, // Issue #9301
                ValueType::F32 => ArrayElementType::F32,
                ValueType::F64 => ArrayElementType::F64,
                ValueType::Bool => ArrayElementType::Bool,
                ValueType::Str => ArrayElementType::String,
                ValueType::Char => ArrayElementType::Char,
                ValueType::Symbol => ArrayElementType::Symbol,
                _ => ArrayElementType::Any,
            }
        };

        // Inference intentionally sees preceding binders so a dependent inner
        // iterator can be typed. Runtime codegen starts from the exact outer
        // state and reintroduces each binder only after its own iterator has
        // been evaluated.
        self.locals = previous_locals.clone();
        self.initialized_locals = previous_initialized_locals.clone();
        self.julia_type_locals = previous_julia_type_locals.clone();
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals.clone();
        self.mixed_type_vars = previous_mixed_type_vars.clone();

        let explicit_lexical = self.explicit_lexical_scopes;
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![result_var.clone()]);
        }

        // Allocate the empty 1-D result and push into it at the innermost loop.
        self.emit_empty_array_wrapper(array_elem_type.clone(), &[0]);
        self.locals.insert(
            result_var.clone(),
            ValueType::ArrayOf(array_elem_type.clone(), None),
        );
        self.emit(Instr::StoreArray(result_var.clone()));

        self.emit_flatten_levels(
            iterations,
            &iter_elem_types,
            0,
            body,
            &body_type,
            filter,
            &result_var,
            &assignment_owners,
        )?;

        self.emit(Instr::LoadArray(result_var));
        if explicit_lexical {
            self.exit_explicit_lexical_scope();
        }
        // Flatten form is always a 1-D `Vector` regardless of clause count.
        Ok(ValueType::ArrayOf(array_elem_type, Some(1)))
    }

    /// Recursively emit one nested loop level of a flatten comprehension. At
    /// `level == iterations.len()` the (optionally filtered) body is pushed to
    /// `result_var`; otherwise this emits the loop for `iterations[level]` —
    /// re-evaluating its iterator inside any enclosing loops — and recurses for
    /// the inner levels (Issue #8014).
    fn emit_flatten_levels(
        &mut self,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        iter_elem_types: &[ValueType],
        level: usize,
        body: &Expr,
        body_type: &ValueType,
        filter: Option<&Expr>,
        result_var: &str,
        assignment_owners: &[String],
    ) -> CResult<()> {
        if level == iterations.len() {
            let temp_val = self.new_temp("fcomp_val");
            let temp_scope = self.enter_explicit_lexical_scope(vec![temp_val.clone()]);
            let result = (|| {
                // Innermost: apply the filter, then compute the body and push it.
                let j_skip = if let Some(filter_expr) = filter {
                    self.compile_expr_as(filter_expr, ValueType::Bool)?;
                    let j = self.here();
                    self.emit(Instr::JumpIfZero(usize::MAX));
                    Some(j)
                } else {
                    None
                };

                // Mirror the cartesian path's per-body-type push so element handling
                // is identical between the two comprehension forms.
                match body_type {
                    ValueType::I64 => {
                        self.compile_expr_as(body, ValueType::I64)?;
                        self.emit(Instr::StoreI64(temp_val.clone()));
                        self.emit(Instr::LoadArray(result_var.to_string()));
                        self.emit(Instr::LoadI64(temp_val.clone()));
                    }
                    ValueType::F64 => {
                        self.compile_expr_as(body, ValueType::F64)?;
                        self.emit(Instr::StoreF64(temp_val.clone()));
                        self.emit(Instr::LoadArray(result_var.to_string()));
                        self.emit(Instr::LoadF64(temp_val.clone()));
                    }
                    _ => {
                        // Tuple / Bool / Str / Char / Symbol / Any: preserve the
                        // value as-is via StoreAny/LoadAny.
                        self.compile_expr(body)?;
                        self.emit(Instr::StoreAny(temp_val.clone()));
                        self.emit(Instr::LoadArray(result_var.to_string()));
                        self.emit(Instr::LoadAny(temp_val.clone()));
                    }
                }
                self.emit(Instr::ArrayPush);
                self.emit(Instr::StoreArray(result_var.to_string()));

                if let Some(j) = j_skip {
                    let skip_label = self.here();
                    self.patch_jump(j, skip_label);
                }
                Ok(())
            })();
            if temp_scope {
                self.exit_explicit_lexical_scope();
            }
            return result;
        }

        let previous_locals = self.locals.clone();
        let previous_initialized_locals = self.initialized_locals.clone();
        let previous_julia_type_locals = self.julia_type_locals.clone();
        let previous_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
        let previous_mixed_type_vars = self.mixed_type_vars.clone();

        let (var, iter_expr) = &iterations[level];
        let iter_var = self.new_temp("fcomp_iter");
        let idx_var = self.new_temp("fcomp_idx");
        let len_var = self.new_temp("fcomp_len");
        let hidden_scope = self.enter_explicit_lexical_scope(vec![
            iter_var.clone(),
            idx_var.clone(),
            len_var.clone(),
        ]);

        // Compile the iterator INSIDE the enclosing loops so a dependent inner
        // range (`for j in 1:i`) is re-evaluated for each outer value. Issue
        // #9321: coerce a runtime `Float` bound of an integer range to `Int` (as
        // in the single-var / cartesian arms) so `IndexLoad` yields `I64`
        // elements. The loop var's inferred element type was registered in
        // `compile_flatten_comprehension`.
        let iter_elem_type = iter_elem_types
            .get(level)
            .cloned()
            .unwrap_or(ValueType::Any);
        self.compile_comprehension_range_iter(iter_expr, &iter_elem_type)?;
        self.locals.insert(iter_var.clone(), ValueType::Any);
        self.emit(Instr::StoreAny(iter_var.clone()));
        let mut owner_names = vec![var.to_string()];
        if level == 0 {
            owner_names.extend(assignment_owners.iter().cloned());
        }
        let binder_scope = self.enter_explicit_lexical_scope(owner_names);
        self.locals.insert(var.to_string(), iter_elem_type);
        self.initialized_locals.insert(var.to_string());
        if level == 0 {
            for name in assignment_owners {
                self.locals.insert(name.clone(), ValueType::Any);
                self.initialized_locals.remove(name);
            }
        }

        self.emit_store_comprehension_len_i64(&iter_var, len_var.clone());

        self.emit(Instr::PushI64(1));
        self.emit(Instr::StoreI64(idx_var.clone()));

        // Loop: while idx <= len { bind var; recurse; idx += 1 }
        let loop_start = self.here();
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::LoadI64(len_var.clone()));
        self.emit(Instr::GtI64);
        let j_continue = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_label = self.here();
        self.patch_jump(j_continue, continue_label);

        // Bind the loop variable to the current element.
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::IndexLoad(1));
        self.emit(Instr::StoreAny(var.to_string()));

        self.emit_flatten_levels(
            iterations,
            iter_elem_types,
            level + 1,
            body,
            body_type,
            filter,
            result_var,
            assignment_owners,
        )?;

        // Increment index and loop back.
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var.clone()));
        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit, exit_label);
        if binder_scope {
            self.exit_explicit_lexical_scope();
        }
        if hidden_scope {
            self.exit_explicit_lexical_scope();
        }

        self.locals = previous_locals;
        self.initialized_locals = previous_initialized_locals;
        self.julia_type_locals = previous_julia_type_locals;
        self.known_any_rank_array_locals = previous_known_any_rank_array_locals;
        self.mixed_type_vars = previous_mixed_type_vars;
        Ok(())
    }

    /// Compile a callable-by-name (a lifted `__gen_body_N` / `__gen_pred_N`
    /// function, or any resolvable callable) as a RUNTIME callable `Value` on
    /// the stack. Locals compile via `Var` (which yields the bound
    /// `Function`/`Closure`); non-locals compile via `FunctionRef` (which emits
    /// `CreateClosure` for capturing module-level lambdas). Used to build a lazy
    /// filtered generator whose body/predicate carry a captured environment
    /// (Issue #9271).
    fn compile_generator_runtime_callable(&mut self, name: &str, span: Span) -> CResult<()> {
        if self.locals.contains_key(name) {
            self.compile_expr(&Expr::Var(name.to_string().into(), span))?;
        } else {
            self.compile_expr(&Expr::FunctionRef {
                name: name.to_string().into(),
                span,
            })?;
        }
        Ok(())
    }

    /// Issue #9200 S3: collapse the desugared FILTERED-generator call shape
    /// `Generator(map, Filter(pred, base))` — emitted by lowering's
    /// `desugar_filtered_generator` — back onto the native filtered-generator
    /// representation.
    ///
    /// The lowering passes `map` / `pred` as by-value lifted function references
    /// (`__gen_body_N` / `__gen_pred_N`, or `identity`). This re-expresses them as
    /// the `map(var)` / `pred(var)` unary-call shape that
    /// `compile_generator_expr`'s filtered path recognizes, reusing the proven
    /// lazy `FilteredFunctionIndex` / `MakeGeneratorRuntimeFiltered` paths (Issue
    /// #9127 / #9271) — so laziness (side-effect ordering, error timing) and every
    /// consumer stay identical to the pre-desugar path. This is the filtered
    /// analogue of S2's `BuiltinOp::Generator` interception: only the *lowered
    /// shape* changed to upstream's `Generator(map, Filter(pred, iter))`; the
    /// native `Value::Generator` boundary is unchanged. S5/S6 will retire this
    /// collapse for a genuine lazy `Base.Generator` over a real `Iterators.Filter`
    /// driven purely by the pure-Julia iterate protocol.
    ///
    /// Returns `Ok(None)` when `args` is not the S3 filtered shape (the caller
    /// falls through to the generic `Generator(...)` construction).
    pub(in super::super) fn try_compile_generator_over_filter(
        &mut self,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        if args.len() != 2 {
            return Ok(None);
        }
        // args[1] must be a `Filter(pred, base)` construction call with no
        // kwargs / splats.
        let Expr::Call {
            function,
            args: filter_args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } = &args[1]
        else {
            return Ok(None);
        };
        if !is_filter_ctor_name(function)
            || filter_args.len() != 2
            || !kwargs.is_empty()
            || splat_mask.iter().any(|&b| b)
            || kwargs_splat_mask.iter().any(|&b| b)
        {
            return Ok(None);
        }
        let (Some(map_name), Some(pred_name)) = (
            callable_ref_name(&args[0]),
            callable_ref_name(&filter_args[0]),
        ) else {
            return Ok(None);
        };
        let base = &filter_args[1];
        let span = *span;

        // Re-express as `(map(v) for v in base if pred(v))`. `v` is a fresh
        // argument name for the lifted unary `map` / `pred`; their own parameter
        // is the original loop variable, so any fresh name binds correctly.
        let fresh_var = format!("__gen_fv_{}", span.start);
        let body = unary_var_call(map_name, &fresh_var, span);
        let filter = unary_var_call(pred_name, &fresh_var, span);
        self.compile_generator_expr(&body, &fresh_var, base, Some(&filter), span)
            .map(Some)
    }

    /// Compile a generator expression: (expr for var in iter) or (expr for var in iter if cond)
    /// Creates a Value::Generator that wraps the underlying iterator and function.
    ///
    /// For generators where the body is a simple function call like `f(x)`,
    /// we try to resolve the function and create a true lazy generator.
    /// Otherwise, we fall back to eager evaluation wrapped in a Generator type.
    pub(in super::super) fn compile_generator_expr(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
        _span: Span,
    ) -> CResult<ValueType> {
        if let Some(filter_expr) = filter {
            if let (Some((map_function, map_span)), Some((predicate_function, predicate_span))) = (
                plain_unary_var_call(body, var),
                plain_unary_var_call(filter_expr, var),
            ) {
                let map_captures = self
                    .scoped_closure_captures(map_function)
                    .is_some_and(|(_, captures)| !captures.is_empty());
                let predicate_captures = self
                    .scoped_closure_captures(predicate_function)
                    .is_some_and(|(_, captures)| !captures.is_empty());
                let map_ref = Expr::FunctionRef {
                    name: map_function.to_string().into(),
                    span: map_span,
                };
                let predicate_ref = Expr::FunctionRef {
                    name: predicate_function.to_string().into(),
                    span: predicate_span,
                };
                let resolved_function_indexes = match (
                    self.resolve_function_ref(&map_ref),
                    self.resolve_function_ref(&predicate_ref),
                ) {
                    (Ok(map_func_index), Ok(predicate_func_index)) => {
                        Some((map_func_index, predicate_func_index))
                    }
                    _ => None,
                };
                if map_captures
                    || predicate_captures
                    || self.locals.contains_key(map_function)
                    || self.locals.contains_key(predicate_function)
                {
                    // Issue #9271: the lifted `__gen_body_N` / `__gen_pred_N`
                    // functions live in a function scope (so they are locals)
                    // and cannot always be resolved as function indexes. If a
                    // callable captures enclosing locals, a bare function index
                    // (`FilteredFunctionIndex`) would drop the environment; if a
                    // local callable is present, keep the filtered generator
                    // LAZY by carrying runtime callable values. A static index is
                    // only safe when no lexical callable can carry an environment;
                    // the result-element hint remains available to the runtime
                    // instruction for empty collections (Issue #10137).
                    let result_element_type =
                        self.filtered_generator_result_element_type(body, var, iter, filter_expr);
                    // Push predicate, then map, then iter; the VM pops iter,
                    // map, predicate (see `Instr::MakeGeneratorRuntimeFiltered`).
                    self.compile_generator_runtime_callable(predicate_function, predicate_span)?;
                    self.compile_generator_runtime_callable(map_function, map_span)?;
                    self.compile_expr(iter)?;
                    self.emit(Instr::MakeGeneratorRuntimeFiltered(result_element_type));
                    return Ok(ValueType::Generator);
                } else if let Some((map_func_index, predicate_func_index)) =
                    resolved_function_indexes
                {
                    // Issue #9127: keep the filtered generator LAZY even when
                    // the mapped element type cannot be inferred statically
                    // (e.g. a lifted `__gen_body_N` whose body calls a global
                    // holding a runtime closure). The runtime derives the
                    // element type from produced values; a `None` here just
                    // means Generator collection observes the first value
                    // before choosing the result element type.
                    let result_element_type =
                        self.filtered_generator_result_element_type(body, var, iter, filter_expr);
                    self.compile_expr(iter)?;
                    self.emit(Instr::MakeGenerator(Box::new(
                        crate::bytecode::MakeGeneratorOperands {
                            callable: GeneratorCallableSpec::FilteredFunctionIndex {
                                map_func_index,
                                predicate_func_index,
                            },
                            result_element_type,
                        },
                    )));
                    return Ok(ValueType::Generator);
                }
            }
        }

        if filter.is_none() {
            if let Some((function, span)) = plain_unary_var_call(body, var) {
                if self.locals.contains_key(function) {
                    // Local callables may be closures with captured values.
                    // Preserve the runtime callable so Generator collection can
                    // discover result element type from produced values instead
                    // of coercing an unknown static body type to Float64.
                    let result_element_type = {
                        let iter_elem_type = self.generator_iter_element_type(iter);
                        self.empty_collection_body_element_type(body, var, &iter_elem_type)
                    };
                    let callable = Expr::Var(function.to_string().into(), span);
                    self.compile_expr(&callable)?;
                    self.compile_expr(iter)?;
                    self.emit(Instr::MakeGeneratorRuntime(false, result_element_type));
                    return Ok(ValueType::Generator);
                } else {
                    let func_ref = Expr::FunctionRef {
                        name: function.to_string().into(),
                        span,
                    };
                    // A module-level lambda (e.g. inside a top-level `let` /
                    // `@testset` block, including the lifted `__gen_body_N`
                    // functions from Issue #9103) may CAPTURE enclosing
                    // locals. The bare `FunctionIndex` callable drops the
                    // captured environment, so capture-carrying functions
                    // must go through the `FunctionRef` compile path below,
                    // which emits `CreateClosure` for them.
                    let has_captures = self
                        .scoped_closure_captures(function)
                        .is_some_and(|(_, captures)| !captures.is_empty());
                    if let Ok(func_index) = self.resolve_function_ref(&func_ref) {
                        if !has_captures {
                            let result_element_type = {
                                let iter_elem_type = self.generator_iter_element_type(iter);
                                self.empty_collection_body_element_type(body, var, &iter_elem_type)
                            };
                            if let Some(result_element_type) = result_element_type {
                                self.compile_expr(iter)?;
                                self.emit(Instr::MakeGenerator(Box::new(
                                    crate::bytecode::MakeGeneratorOperands {
                                        callable: GeneratorCallableSpec::FunctionIndex(func_index),
                                        result_element_type: Some(result_element_type),
                                    },
                                )));
                                return Ok(ValueType::Generator);
                            }
                        }
                        // Issue #9103: a resolvable global callable that
                        // captures enclosing locals, or whose element type
                        // cannot be inferred (e.g. the iterator is a global
                        // variable), must still produce a LAZY generator.
                        // Route it through the runtime-callable path: the
                        // `FunctionRef` compile emits `CreateClosure` when
                        // the callable captures, and the runtime derives the
                        // element type from produced values.
                        let result_element_type = {
                            let iter_elem_type = self.generator_iter_element_type(iter);
                            self.empty_collection_body_element_type(body, var, &iter_elem_type)
                        };
                        self.compile_expr(&func_ref)?;
                        self.compile_expr(iter)?;
                        self.emit(Instr::MakeGeneratorRuntime(false, result_element_type));
                        return Ok(ValueType::Generator);
                    }
                }
            }
        }

        // Issue #3966: collect(Generator(f, iter)) now applies `f(x)` through
        // the VM HOF path, but the general `iterate(::Generator)` protocol is
        // still synchronous and cannot enter function frames. Keep generator
        // expression syntax on the eager collect-compatible wrapper until
        // full async iterator protocol support lands.
        let result_var = self.new_temp("gen_result");

        // Compile as comprehension to get the array result
        let arr_type = self.compile_comprehension(body, var, iter, filter)?;
        let _ = arr_type; // Ignore the array type, we're wrapping it

        // Store the array temporarily
        self.emit(Instr::StoreArray(result_var.clone()));

        // Load and wrap in Generator
        self.emit(Instr::LoadArray(result_var));
        self.emit(Instr::WrapInGenerator);

        Ok(ValueType::Generator)
    }

    /// Returns `true` when `expr` is (or is statically known to be) a `Tuple`
    /// value, used to reject `Array{T}(::Tuple)` / `Vector{T}(::Tuple)`
    /// constructions that have no upstream method (Issue #5041). Matches the
    /// literal `(a, b, ...)` shape and the inferred `Tuple` value/Julia type.
    /// `NamedTuple` is intentionally *not* matched here: it is a distinct shape
    /// handled by other paths.
    fn expr_is_tuple_arg(&mut self, expr: &Expr) -> bool {
        if matches!(expr, Expr::TupleLiteral { .. }) {
            return true;
        }
        if matches!(self.infer_expr_type(expr), ValueType::Tuple) {
            return true;
        }
        matches!(
            self.infer_julia_type(expr),
            crate::types::JuliaType::Tuple | crate::types::JuliaType::TupleOf(_)
        )
    }

    /// Synthesize the runtime `MethodError` that upstream Julia raises for
    /// `Array{T}(::Tuple)` / `Vector{T}(::Tuple)` — there is no such
    /// constructor method (Issue #5041). Emits `throw(MethodError(ctor,
    /// (arg,)))` so the error is a *catchable* runtime `MethodError` (matching
    /// `@test_throws MethodError` and try/catch), with the constructor type
    /// object as the error's `f` field. For typed forms (`Vector{Int64}`) this
    /// renders exactly like upstream: `no method matching
    /// Vector{Int64}(::Tuple{Int64, Int64, Int64})`.
    fn compile_array_ctor_tuple_method_error(
        &mut self,
        ctor_name: &str,
        arg: &Expr,
    ) -> CResult<ValueType> {
        let span = arg.span();
        // Build the constructor type-object expression so `MethodError.f`
        // carries the constructor that failed to match. A bare alias like
        // `Vector` is a builtin type name (compiles to a `PushDataType`
        // value via the `Expr::Var` path), while a parametric spelling like
        // `Vector{Int64}` / `Array{Int64, 1}` needs an explicit `DataType`
        // literal (`Literal::DataType`, which compiles to `Instr::PushDataType`
        // directly). Using `Expr::Var("Vector{Int64}")` directly would fail to
        // resolve (`is_builtin_type_name` only knows bare names).
        //
        // A prior revision used `Expr::Builtin { name: BuiltinOp::TypeOf, args:
        // [Literal::Str(ctor_name)] }` here, copying an AST shape that only a
        // handful of *lowering*-stage call sites (e.g. `T[]` empty-array-literal
        // recognition) specially pattern-match back into a type name. Reached
        // through the general `compile_expr` dispatcher instead, that shape
        // compiles as literally written — push the string, then call the
        // `typeof` builtin on it — so `MethodError.f` held `String` (the type of
        // the string literal) instead of the constructor's `DataType`, e.g.
        // `Vector{Int64}((1, 2, 3))` raised `MethodError(String, ((1, 2, 3),))`
        // instead of `MethodError(Vector{Int64}, ((1, 2, 3),))` (Issue #10404).
        let ctor_expr = if ctor_name.contains('{') {
            Expr::Literal(
                crate::ir::core::Literal::DataType(ctor_name.to_string()),
                span,
            )
        } else {
            Expr::Var(ctor_name.to_string().into(), span)
        };
        let args_tuple = Expr::TupleLiteral {
            elements: vec![arg.clone()],
            span,
        };
        let method_error = Expr::Call {
            function: "MethodError".to_string().into(),
            args: vec![ctor_expr, args_tuple],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
        let throw_call = Expr::Call {
            function: "throw".to_string().into(),
            args: vec![method_error],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
        self.compile_expr(&throw_call)?;
        // `throw` does not return; report the constructor's nominal element type
        // so downstream type inference stays well-typed on the unreachable path.
        Ok(ValueType::ArrayOf(ArrayElementType::Any, None))
    }

    /// Compile an Array/Vector constructor call: Array{Int64}(), Vector{Float64}(), etc.
    /// Supports:
    /// - Empty arrays: Vector{Int64}(), Array{Float64}()
    /// - Array conversion: Vector{Int64}(existing_array)
    /// - Uninitialized arrays: Vector{Float64}(undef, n), Array{Int64}(undef, m, n)
    ///
    /// `ctor_name` is the user-written constructor spelling (e.g. `"Vector"`,
    /// `"Vector{Int64}"`, `"Array{Int64, 1}"`) used to build the `MethodError`
    /// raised for the unsupported `(::Tuple)` argument shape (Issue #5041).
    pub(in super::super) fn compile_array_constructor(
        &mut self,
        type_args: &[TypeExpr],
        args: &[Expr],
        ctor_name: &str,
    ) -> CResult<ValueType> {
        // Determine the element type from type_args
        let elem_type = if type_args.is_empty() {
            ArrayElementType::Any
        } else {
            match &type_args[0] {
                TypeExpr::Concrete(jt) => {
                    use crate::types::JuliaType;
                    match jt {
                        JuliaType::Int64 | JuliaType::Integer => ArrayElementType::I64,
                        JuliaType::Int8 => ArrayElementType::I8,
                        JuliaType::Int16 => ArrayElementType::I16,
                        JuliaType::Int32 => ArrayElementType::I32,
                        JuliaType::UInt8 => ArrayElementType::U8,
                        JuliaType::UInt16 => ArrayElementType::U16,
                        JuliaType::UInt32 => ArrayElementType::U32,
                        JuliaType::UInt64 => ArrayElementType::U64,
                        JuliaType::Float64 | JuliaType::AbstractFloat => ArrayElementType::F64,
                        JuliaType::Float16 => ArrayElementType::F16, // Issue #9301
                        JuliaType::Float32 => ArrayElementType::F32,
                        JuliaType::Bottom => ArrayElementType::UnionOf(Vec::new()),
                        JuliaType::Bool => ArrayElementType::Bool,
                        JuliaType::Char => ArrayElementType::Char,
                        JuliaType::Symbol => ArrayElementType::Symbol,
                        JuliaType::String => ArrayElementType::String,
                        JuliaType::Struct(name) if name == "Complex{Float64}" => {
                            ArrayElementType::ComplexF64
                        }
                        JuliaType::Struct(name) if name == "Complex{Float32}" => {
                            ArrayElementType::ComplexF32
                        }
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::TypeVar(name) => {
                    // Try to resolve known type names (Issue #2218: support all numeric types)
                    match name.as_str() {
                        "Int" if crate::types::native_int_type_name() == "Int32" => {
                            ArrayElementType::I32
                        }
                        "Int64" | "Int" => ArrayElementType::I64,
                        "Int8" => ArrayElementType::I8,
                        "Int16" => ArrayElementType::I16,
                        "Int32" => ArrayElementType::I32,
                        "UInt8" => ArrayElementType::U8,
                        "UInt16" => ArrayElementType::U16,
                        "UInt32" => ArrayElementType::U32,
                        "UInt" if crate::types::native_uint_type_name() == "UInt32" => {
                            ArrayElementType::U32
                        }
                        "UInt64" | "UInt" => ArrayElementType::U64,
                        "Float64" => ArrayElementType::F64,
                        "Float32" => ArrayElementType::F32,
                        // Issue #9301: Float16 now has a dedicated (boxed) storage
                        // tag, so it narrows like F32/F64 instead of widening to Any.
                        "Float16" => ArrayElementType::F16,
                        "Bool" => ArrayElementType::Bool,
                        "Char" => ArrayElementType::Char,
                        "Symbol" => ArrayElementType::Symbol,
                        "String" => ArrayElementType::String,
                        "ComplexF64" => ArrayElementType::ComplexF64,
                        "ComplexF32" => ArrayElementType::ComplexF32,
                        "Union{}" => ArrayElementType::UnionOf(Vec::new()),
                        "Pair" => ArrayElementType::Abstract("Pair".to_string()),
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::Parameterized { base, params } => {
                    // Handle Complex{Float64}, Tuple{}, etc.
                    match base.as_str() {
                        "Complex" if !params.is_empty() => match &params[0] {
                            TypeExpr::TypeVar(inner) => match inner.as_str() {
                                "Float64" => ArrayElementType::ComplexF64,
                                "Float32" => ArrayElementType::ComplexF32,
                                _ => ArrayElementType::Any,
                            },
                            TypeExpr::Concrete(jt) => {
                                use crate::types::JuliaType;
                                match jt {
                                    JuliaType::Float64 => ArrayElementType::ComplexF64,
                                    JuliaType::Float32 => ArrayElementType::ComplexF32,
                                    _ => ArrayElementType::Any,
                                }
                            }
                            _ => ArrayElementType::Any,
                        },
                        "Tuple" if params.is_empty() => ArrayElementType::TupleOf(Vec::new()),
                        "Union" => union_type_params_to_body(params)
                            .map(|body| ArrayElementType::union_from_body(&body))
                            .unwrap_or(ArrayElementType::Any),
                        "Pair" => pair_type_name(base, params)
                            .map(ArrayElementType::Abstract)
                            .unwrap_or(ArrayElementType::Any),
                        "SubArray" => subarray_type_name(base, params)
                            .map(ArrayElementType::Abstract)
                            .unwrap_or(ArrayElementType::Any),
                        // Preserve concrete parametric eltypes that have no
                        // dedicated storage tag (`UnitRange{Int64}`,
                        // `Vector{Int}`, ...) instead of widening to `Any`
                        // (Issue #6768).
                        _ => concrete_parametric_abstract_element_type(base, params)
                            .unwrap_or(ArrayElementType::Any),
                    }
                }
                TypeExpr::RuntimeExpr(_) => ArrayElementType::Any, // Runtime expressions can't be resolved at compile time
            }
        };

        // Detect `Array{T}(undef, dims...)` where `T` is a runtime DataType
        // variable rather than a known compile-time type. In that case the
        // compile-time result is still imprecise even though Pure Julia
        // `_array_undef_from_dims(T, dims)` preserves the runtime element type
        // through `similar(Array{T}, dims)` (Issue #4018).
        let has_runtime_type_arg = type_args
            .first()
            .is_some_and(|type_arg| self.type_expr_requires_runtime_value(type_arg));

        if args.is_empty() {
            // Create an empty Memory-backed Array wrapper (Issue #6649).
            if let Some(type_arg) = type_args
                .first()
                .filter(|type_arg| self.type_expr_requires_runtime_value(type_arg))
            {
                // Build `Memory{T}` for a runtime element type, then finalize
                // into the `Array{T,N}` wrapper natively. No `Array` `DataType`
                // is pushed: `emit_array_wrapper_from_memory_on_stack` now emits
                // a native `FinalizeArray` rather than the pure-Julia
                // `wrap(::Type{Array}, ...)` call that consumed it (Issue #6846).
                self.emit_type_expr_value_for_array_alloc(Some(type_arg))?;
                self.emit(Instr::PushI64(0));
                self.emit(Instr::NewMemoryDynamicTyped);
                self.emit_array_wrapper_from_memory_on_stack(&[0]);
                Ok(ValueType::Array)
            } else {
                self.emit_empty_array_wrapper(elem_type.clone(), &[0]);
                Ok(ValueType::ArrayOf(elem_type, None))
            }
        } else if args.len() == 1 {
            // Upstream Julia has *no* `Array{T}(::Tuple)` / `Vector{T}(::Tuple)`
            // constructor method — `Vector{Int}((1,2,3))` raises a `MethodError`
            // (the correct spellings are `collect((1,2,3))` or `Int[(1,2,3)...]`).
            // The single-arg intercept below treats any iterable-ish argument as
            // an array/range to materialize, which previously silently built a
            // vector from a tuple. Guard against the Tuple shape and synthesize
            // the same catchable runtime `MethodError(ctor, (tuple,))` upstream
            // raises, with the constructor type object as the error's `f`
            // (renders `no method matching Vector{Int64}(::Tuple{...})`). The
            // legitimate Range / Array / typed-comprehension paths below are
            // unaffected. (Issue #5041)
            if self.expr_is_tuple_arg(&args[0]) {
                return self.compile_array_ctor_tuple_method_error(ctor_name, &args[0]);
            }
            if let Some(type_arg) = type_args
                .first()
                .filter(|type_arg| self.type_expr_requires_runtime_value(type_arg))
            {
                if let Expr::Comprehension {
                    body,
                    var,
                    iter,
                    filter,
                    ..
                } = &args[0]
                {
                    return self.compile_comprehension_with_runtime_elem(
                        body,
                        var,
                        iter,
                        filter.as_deref(),
                        type_arg,
                    );
                }
            }
            // Typed comprehension intercept (Issue #5040): the lowering for
            // `Bool[...]` / `Char[...]` / `Symbol[...]` / `String[...]`
            // rewrites the body to `convert(T, expr)` and wraps the whole
            // comprehension in `Vector{T}(...)`. Compile that comprehension
            // directly with the forced element type so the result `Vector{T}`
            // is allocated with eltype `T` (the `convert` body returns `Any`
            // at compile time, which would otherwise mis-infer the eltype).
            if !type_args.is_empty() && !matches!(elem_type, ArrayElementType::Any) {
                match &args[0] {
                    Expr::Comprehension {
                        body,
                        var,
                        iter,
                        filter,
                        ..
                    } => {
                        return self.compile_comprehension_with_elem(
                            body,
                            var,
                            iter,
                            filter.as_deref(),
                            Some(elem_type),
                        );
                    }
                    Expr::MultiComprehension {
                        body,
                        iterations,
                        filter,
                        flatten,
                        ..
                    } => {
                        // Multi-iterator typed comprehension `T[expr for i in
                        // R1, j in R2]` builds a `Matrix{T}` upstream, not a
                        // `Vector{T}`. Compile it directly with the forced
                        // element type (the lowering already wrapped the body
                        // in `convert(T, expr)`). Issue #5040. The whitespace
                        // flatten form `T[expr for i in R1 for j in R2]` instead
                        // builds a `Vector{T}` (Issue #8014).
                        return self.compile_multi_comprehension_with_elem(
                            body,
                            iterations,
                            filter.as_deref(),
                            Some(elem_type),
                            *flatten,
                        );
                    }
                    _ => {}
                }
            }
            // Array{T}(arr) / Vector{T}(arr) - copy/convert an existing
            // array OR materialize a Range. Issue #4810: if the arg is
            // (or might be) a Range, route through `collect(arg)` so
            // the Pure-Julia `Vector{T}(::AbstractRange)` method or
            // the VM RangeCollect materializes the elements. The
            // previous "shallow copy" placeholder returned the range
            // unchanged, leaving `Vector(1:3)` as `1:3` instead of
            // `[1, 2, 3]`.
            let arg_julia_type = self.infer_julia_type(&args[0]);
            let arg_is_range = matches!(
                arg_julia_type,
                crate::types::JuliaType::UnitRange
                    | crate::types::JuliaType::StepRange
                    | crate::types::JuliaType::AbstractRange
            ) || matches!(
                self.infer_expr_type(&args[0]),
                crate::bytecode::ValueType::Range
            );
            // For the no-type-args case (`Vector(1:3)`), route through
            // RangeCollect which yields the natural element type
            // (Int64 for `1:3`, Float64 for `1.0:3.0`).
            if arg_is_range && type_args.is_empty() {
                self.compile_expr(&args[0])?;
                self.emit(crate::bytecode::Instr::CallBuiltin(
                    crate::builtins::BuiltinId::RangeCollect,
                    1,
                ));
                return Ok(ValueType::ArrayOf(elem_type, None));
            }
            // `Vector(arr)` / `Array(arr)` copies an existing vector upstream;
            // it is not an identity constructor. Route the untyped non-range
            // case through `collect(arg)` so direct syntax matches the dynamic
            // `Vector(::AbstractVector)` method used by HOF call sites such as
            // `map(Vector, xs)` (Issue #10085).
            if type_args.is_empty() {
                let arg_span = args[0].span();
                let collect_call = Expr::Call {
                    function: "collect".to_string().into(),
                    args: vec![args[0].clone()],
                    kwargs: Vec::new(),
                    splat_mask: Vec::new(),
                    kwargs_splat_mask: Vec::new(),
                    span: arg_span,
                };
                return self.compile_expr(&collect_call);
            }
            // For the typed case (`Vector{T}(arg)`), synthesize the
            // upstream Julia form `T[T(x) for x in arg]` so the
            // existing typed-comprehension compile path materializes
            // and per-element converts. Covers both the Range arg
            // (Issue #4811 — Pure-Julia `Vector{T}(::AbstractRange)`
            // exists but is unreachable from this intercept) and the
            // Array arg with differing eltype (Issue #4816 — previous
            // no-op shallow copy silently kept the source eltype).
            // Issue #10085: when the source eltype already matches T, the
            // constructor must still allocate a fresh vector. Use `collect`
            // for that exact-eltype copy path; synthesize `T[T(x) for x in
            // arg]` only when conversion is required.
            //
            // Conditions:
            //   - target T is a known concrete type (not Any),
            //   - arg is a range or iterable array-like value.
            if !type_args.is_empty() && !matches!(elem_type, ArrayElementType::Any) {
                let source_value_type = self.infer_expr_type(&args[0]);
                match &source_value_type {
                    crate::bytecode::ValueType::ArrayOf(source_elem, _)
                        if *source_elem == elem_type && !arg_is_range =>
                    {
                        let arg_span = args[0].span();
                        let collect_call = Expr::Call {
                            function: "collect".to_string().into(),
                            args: vec![args[0].clone()],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span: arg_span,
                        };
                        return self.compile_expr(&collect_call);
                    }
                    _ => {}
                }

                let arg_span = args[0].span();
                let type_name = type_args[0].to_string();
                let var_name = "__sjvm_vec_ctor_x".to_string();
                let comp = Expr::Comprehension {
                    body: Box::new(Expr::Call {
                        function: type_name.into(),
                        args: vec![Expr::Var(var_name.clone().into(), arg_span)],
                        kwargs: Vec::new(),
                        splat_mask: Vec::new(),
                        kwargs_splat_mask: Vec::new(),
                        span: arg_span,
                    }),
                    var: var_name.into(),
                    iter: Box::new(args[0].clone()),
                    filter: None,
                    span: arg_span,
                };
                self.compile_expr(&comp)?;
                return Ok(ValueType::ArrayOf(elem_type, None));
            }
            // `Vector{Any}(arr)`: route through the Pure-Julia helper
            // `_vector_any_collect(arr)` so the result is a
            // `Vector{Any}` with each source element boxed (Issue #4818).
            // Cannot synthesize a typed comprehension here because
            // `Any[x for x in arr]` lowers to a body wrapped in `Any(x)`,
            // which is not a defined Julia constructor. The helper
            // allocates via the `Vector{Any}(undef, n)` intercept and
            // assigns each element via plain indexed store. Gated on
            // the literal type name "Any" so unknown user types
            // (TypeVar/where-clause names that also fall through to
            // `ArrayElementType::Any`) keep the existing no-op path.
            if !type_args.is_empty()
                && matches!(elem_type, ArrayElementType::Any)
                && type_args[0].to_string() == "Any"
            {
                let arg_span = args[0].span();
                let helper_call = Expr::Call {
                    function: "_vector_any_collect".to_string().into(),
                    args: vec![args[0].clone()],
                    kwargs: Vec::new(),
                    splat_mask: Vec::new(),
                    kwargs_splat_mask: Vec::new(),
                    span: arg_span,
                };
                self.compile_expr(&helper_call)?;
                return Ok(ValueType::ArrayOf(elem_type, None));
            }
            self.compile_expr(&args[0])?;
            Ok(ValueType::ArrayOf(elem_type, None))
        } else {
            // Check if first argument is `undef` - this is the Array{T}(undef, dims...) pattern
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");

            if is_undef {
                // Array{T}(undef, dims...) - route public allocation through
                // Pure Julia `_array_undef_from_dims(T, dims)` so the visible
                // constructor uses `similar(Array{T}, dims)` before hitting the
                // retained Memory-first VM primitive boundary (Issue #4018).
                let dim_count = args.len() - 1;
                let tuple_dims = dim_count == 1
                    && (matches!(&args[1], Expr::TupleLiteral { .. })
                        || matches!(self.infer_expr_type(&args[1]), ValueType::Tuple));
                self.emit_array_undef_from_dims_call(type_args.first(), &args[1..], tuple_dims)?;
                if has_runtime_type_arg {
                    Ok(ValueType::Array)
                } else {
                    Ok(ValueType::ArrayOf(elem_type, None))
                }
            } else {
                err("Array/Vector constructor with multiple arguments not yet supported (expected undef as first argument)")
            }
        }
    }

    /// Compile a Memory{T}(...) constructor call.
    /// Supports (matching upstream `base/genericmemory.jl`):
    /// - Empty memory: Memory{Int64}() → zero-length
    /// - With undef: Memory{Int64}(undef, n) → n-element undef-initialized
    ///
    /// The single-argument forms `Memory{T}(n::Int)` and `Memory{T}(undef)` are
    /// NOT upstream constructors (both are MethodErrors upstream) and compile to
    /// a catchable runtime `MethodError` (Issue #10324 item 3).
    fn memory_struct_element_type(&self, name: &str) -> Option<ArrayElementType> {
        let base_name = name.split('{').next().unwrap_or(name);
        let type_id = self
            .shared_ctx
            .get_struct_type_id(name)
            .or_else(|| self.shared_ctx.get_struct_type_id(base_name))?;
        if name.contains('{') && !crate::bytecode::value::is_rational_type_name(name) {
            return Some(ArrayElementType::Abstract(name.to_string()));
        }
        Some(ArrayElementType::StructOf(type_id))
    }

    pub(in super::super) fn compile_memory_constructor(
        &mut self,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Determine the element type from type_args (same logic as Array)
        let elem_type = if type_args.is_empty() {
            ArrayElementType::Any
        } else {
            match &type_args[0] {
                TypeExpr::Concrete(jt) => {
                    use crate::types::JuliaType;
                    match jt {
                        JuliaType::Int64 => ArrayElementType::I64,
                        JuliaType::Int8 => ArrayElementType::I8,
                        JuliaType::Int16 => ArrayElementType::I16,
                        JuliaType::Int32 => ArrayElementType::I32,
                        JuliaType::UInt8 => ArrayElementType::U8,
                        JuliaType::UInt16 => ArrayElementType::U16,
                        JuliaType::UInt32 => ArrayElementType::U32,
                        JuliaType::UInt64 => ArrayElementType::U64,
                        JuliaType::Float64 => ArrayElementType::F64,
                        JuliaType::Float16 => ArrayElementType::F16, // Issue #9301
                        JuliaType::Float32 => ArrayElementType::F32,
                        JuliaType::Number => ArrayElementType::Abstract("Number".to_string()),
                        JuliaType::Real => ArrayElementType::Abstract("Real".to_string()),
                        JuliaType::Integer => ArrayElementType::Abstract("Integer".to_string()),
                        JuliaType::Signed => ArrayElementType::Abstract("Signed".to_string()),
                        JuliaType::Unsigned => ArrayElementType::Abstract("Unsigned".to_string()),
                        JuliaType::AbstractFloat => {
                            ArrayElementType::Abstract("AbstractFloat".to_string())
                        }
                        JuliaType::Bool => ArrayElementType::Bool,
                        JuliaType::Char => ArrayElementType::Char,
                        JuliaType::Symbol => ArrayElementType::Symbol,
                        JuliaType::String => ArrayElementType::String,
                        JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => {
                            ArrayElementType::Abstract(jt.name().to_string())
                        }
                        JuliaType::Struct(name) => self
                            .memory_struct_element_type(name)
                            .unwrap_or(ArrayElementType::Any),
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::TypeVar(name) => match name.as_str() {
                    "Int" if crate::types::native_int_type_name() == "Int32" => {
                        ArrayElementType::I32
                    }
                    "Int64" | "Int" => ArrayElementType::I64,
                    "Int8" => ArrayElementType::I8,
                    "Int16" => ArrayElementType::I16,
                    "Int32" => ArrayElementType::I32,
                    "UInt8" => ArrayElementType::U8,
                    "UInt16" => ArrayElementType::U16,
                    "UInt32" => ArrayElementType::U32,
                    "UInt" if crate::types::native_uint_type_name() == "UInt32" => {
                        ArrayElementType::U32
                    }
                    "UInt64" | "UInt" => ArrayElementType::U64,
                    "Float64" => ArrayElementType::F64,
                    "Float16" => ArrayElementType::F16, // Issue #9301
                    "Float32" => ArrayElementType::F32,
                    "Number" => ArrayElementType::Abstract("Number".to_string()),
                    "Real" => ArrayElementType::Abstract("Real".to_string()),
                    "Integer" => ArrayElementType::Abstract("Integer".to_string()),
                    "Signed" => ArrayElementType::Abstract("Signed".to_string()),
                    "Unsigned" => ArrayElementType::Abstract("Unsigned".to_string()),
                    "AbstractFloat" => ArrayElementType::Abstract("AbstractFloat".to_string()),
                    "Bool" => ArrayElementType::Bool,
                    "Char" => ArrayElementType::Char,
                    "Symbol" => ArrayElementType::Symbol,
                    "String" => ArrayElementType::String,
                    "Pair" => ArrayElementType::Abstract("Pair".to_string()),
                    _ => self
                        .memory_struct_element_type(name)
                        .unwrap_or(ArrayElementType::Any),
                },
                TypeExpr::Parameterized { base, params } => match base.as_str() {
                    "Union" => union_type_params_to_body(params)
                        .map(|body| ArrayElementType::union_from_body(&body))
                        .unwrap_or(ArrayElementType::Any),
                    "Pair" => pair_type_name(base, params)
                        .map(ArrayElementType::Abstract)
                        .unwrap_or(ArrayElementType::Any),
                    _ => concrete_parametric_abstract_element_type(base, params)
                        .or_else(|| {
                            self.memory_struct_element_type(&TypeExpr::format_parameterized(
                                base, params,
                            ))
                        })
                        .unwrap_or(ArrayElementType::Any),
                },
                _ => ArrayElementType::Any,
            }
        };

        // Detect `Memory{T}(n)` where `T` is a runtime DataType value from a
        // where-clause or local variable. This mirrors the `Array{T}(undef, ...)`
        // path above and lets Pure Julia code allocate `Memory{S}` from
        // `similar(a, S, dims...)` just like upstream `base/genericmemory.jl`.
        let runtime_type_arg = type_args
            .first()
            .filter(|type_arg| self.type_expr_requires_runtime_value(type_arg))
            .cloned();

        let result_type = if runtime_type_arg.is_some() {
            ValueType::Memory
        } else {
            ValueType::MemoryOf(elem_type.clone())
        };

        if args.is_empty() {
            // Memory{T}() → empty memory with zero length
            if let Some(type_arg) = &runtime_type_arg {
                self.emit_type_expr_value_for_array_alloc(Some(type_arg))?;
                self.emit(Instr::PushI64(0));
                self.emit(Instr::NewMemoryDynamicTyped);
            } else {
                self.emit(Instr::NewMemory(elem_type.clone(), 0));
            }
            Ok(result_type)
        } else if args.len() == 1 {
            // Upstream has no single-argument `Memory{T}` constructor: both
            // `Memory{T}(n::Int)` and `Memory{T}(undef)` are MethodErrors — the
            // sized form is spelled `Memory{T}(undef, n)`. Emit a catchable
            // runtime MethodError (Issue #10324 item 3) instead of the former
            // sjulia extension that allocated `n` undef elements. The argument is
            // still compiled (and popped) so its side effects and any error it
            // raises happen before the MethodError, matching upstream arg eval.
            //
            // `undef` is not a resolvable global in sjulia (Issue #10737): it is
            // recognized only syntactically, by every other call site in this
            // file matching the literal `Expr::Var("undef", _)` pattern (see the
            // 2-argument branch below and `Array`/`Vector` handling above) rather
            // than by evaluating it as an ordinary variable reference. Compiling
            // `args[0]` generically here used to send `undef` through normal
            // variable resolution, which raised `UndefVarError` before the
            // intended `MethodError` was ever reached — upstream evaluates
            // `undef` to the `UndefInitializer()` singleton first, so the
            // MethodError is upstream's actual outcome (Issue #10354's
            // fixture-fallout measurement, `memory/memory_single_arg_methoderror_10324.jl`;
            // see docs/vm/EXCEPTION_PARITY.md). Emit the same `PushUndef`
            // sentinel used elsewhere in this file instead of a generic
            // variable load so the argument evaluates (matching upstream arg
            // eval order) without depending on `undef` being a real global.
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");
            if is_undef {
                self.emit(Instr::PushUndef);
            } else {
                self.compile_expr(&args[0])?;
            }
            self.emit(Instr::Pop);
            let type_display = if type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "{{{}}}",
                    type_args
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let arg_desc = if is_undef {
                "::UndefInitializer"
            } else {
                "::Int64"
            };
            self.emit(Instr::ThrowMethodError(format!(
                "no method matching Memory{}({})",
                type_display, arg_desc
            )));
            Ok(result_type)
        } else if args.len() == 2 {
            // Memory{T}(undef, n) → undef-initialized memory with n elements
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");
            if is_undef {
                if let Some(type_arg) = &runtime_type_arg {
                    self.emit_type_expr_value_for_array_alloc(Some(type_arg))?;
                    self.compile_memory_dim_to_i64(&args[1])?;
                    self.emit(Instr::NewMemoryDynamicTyped);
                } else if let Expr::Literal(crate::ir::core::Literal::Int(n), _) = &args[1] {
                    if *n < 0 {
                        return err(format!("invalid Memory length: {}", n));
                    }
                    self.emit(Instr::NewMemory(elem_type.clone(), *n as usize));
                } else {
                    self.compile_memory_dim_to_i64(&args[1])?;
                    self.emit(Instr::NewMemoryDynamic(elem_type.clone()));
                }
                Ok(result_type)
            } else {
                err("Memory{T} constructor with 2 arguments requires `undef` as first argument")
            }
        } else {
            err("Memory{T} constructor takes at most 2 arguments")
        }
    }

    /// Compile a `Memory{T}` size argument to an `I64` on the stack.
    ///
    /// `Memory` is one-dimensional, so upstream accepts its size as a 1-tuple
    /// (`Memory{T}(undef, (n,))`, `base/genericmemory.jl`) in addition to a bare
    /// scalar `n` (Issue #6688). A 1-element tuple literal is unwrapped at compile
    /// time; a value dynamically typed as a tuple (e.g. `dims = (n,)`) has its
    /// first element extracted at runtime. A multi-element tuple literal is
    /// rejected (`Memory` has no multi-dimensional form — upstream raises a
    /// `MethodError`).
    fn compile_memory_dim_to_i64(&mut self, arg: &Expr) -> CResult<()> {
        if let Expr::TupleLiteral { elements, .. } = arg {
            return match elements.as_slice() {
                [only] => self.compile_expr_as(only, ValueType::I64),
                _ => err(format!(
                    "Memory{{T}} is one-dimensional but was given {} dimensions",
                    elements.len()
                )),
            };
        }
        if matches!(self.infer_expr_type(arg), ValueType::Tuple) {
            // Dynamic 1-tuple of dims: take its first element, coerce to I64.
            self.compile_expr(arg)?;
            self.emit(Instr::PushI64(1));
            self.emit(Instr::TupleGet);
            self.emit(Instr::DynamicToI64);
            return Ok(());
        }
        self.compile_expr_as(arg, ValueType::I64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comprehension_iter_uses_iterate_protocol_for_dynamic_and_nonindexable_values_10607() {
        assert!(comprehension_iter_uses_iterate_protocol(
            &ValueType::Generator
        ));
        assert!(comprehension_iter_uses_iterate_protocol(
            &ValueType::Struct(42)
        ));
        assert!(comprehension_iter_uses_iterate_protocol(&ValueType::Any));
        assert!(comprehension_iter_uses_iterate_protocol(&ValueType::Union(
            vec![
                ValueType::ArrayOf(ArrayElementType::I64, None),
                ValueType::Struct(42),
            ]
        )));
        assert!(comprehension_iter_uses_iterate_protocol(&ValueType::Str));
        assert!(comprehension_iter_uses_iterate_protocol(&ValueType::Set));
    }

    #[test]
    fn comprehension_iter_keeps_index_loop_for_known_indexable_representations_10607() {
        assert!(!comprehension_iter_uses_iterate_protocol(&ValueType::Array));
        assert!(!comprehension_iter_uses_iterate_protocol(
            &ValueType::ArrayOf(ArrayElementType::I64, None)
        ));
        assert!(!comprehension_iter_uses_iterate_protocol(
            &ValueType::MemoryOf(ArrayElementType::I64)
        ));
        assert!(!comprehension_iter_uses_iterate_protocol(&ValueType::Range));
        assert!(!comprehension_iter_uses_iterate_protocol(&ValueType::Tuple));
        assert!(!comprehension_iter_uses_iterate_protocol(
            &ValueType::NamedTuple
        ));
    }
}
