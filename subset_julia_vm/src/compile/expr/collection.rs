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
use crate::ir::core::{decode_tuple_comprehension_binding, Expr, Stmt};
use crate::span::Span;
use crate::vm::value::GeneratorCallable;
use crate::vm::{ArrayElementType, Instr, ValueType};

use super::super::{err, CResult, CoreCompiler, TypeExpr};

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
        (BigInt, BigInt) | (I128, I128) | (U128, U128) | (F16, F16) | (BigFloat, BigFloat) => {
            Some(ArrayElementType::Any)
        }
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
        ArrayElementType::F32 | ArrayElementType::F64 => {
            Some(&["AbstractFloat", "Real", "Number", "Any"])
        }
        ArrayElementType::Abstract(name) => match name.as_str() {
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
        ValueType::F32 => Some(ArrayElementType::F32),
        ValueType::F64 => Some(ArrayElementType::F64),
        ValueType::Bool => Some(ArrayElementType::Bool),
        ValueType::Str => Some(ArrayElementType::String),
        ValueType::Char => Some(ArrayElementType::Char),
        ValueType::Symbol => Some(ArrayElementType::Symbol),
        _ => None,
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
        "Float64" => Some(ArrayElementType::F64),
        "Float32" => Some(ArrayElementType::F32),
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
    /// (`ctor_arg_bound_type_vars`). Explicit-only parameters are not yet
    /// plumbed from the call site, so they are treated as unresolvable here and
    /// handled by the legacy runtime path (Issue #5059).
    pub(in super::super) fn type_expr_is_resolvable(&self, type_arg: &TypeExpr) -> bool {
        match type_arg {
            TypeExpr::Concrete(_) => true,
            TypeExpr::TypeVar(name) => self.ctor_arg_bound_type_vars.contains(name.as_str()),
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
                if params
                    .iter()
                    .any(|param| self.type_expr_requires_runtime_value(param))
                {
                    for param in params {
                        self.emit_type_expr_value_for_array_alloc(Some(param))?;
                    }
                    self.emit(Instr::ConstructParametricType(base.clone(), params.len()));
                } else {
                    let type_name = format!(
                        "{}{{{}}}",
                        base,
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
        let is_runtime_type = self.locals.get(function) == Some(&ValueType::DataType);
        if !is_type_binding && !is_runtime_type {
            return None;
        }
        let type_arg = TypeExpr::TypeVar(function.clone());
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

        let result_var = self.new_temp("comp_result");
        let iter_var = self.new_temp("comp_iter");
        let idx_var = self.new_temp("comp_idx");
        let len_var = self.new_temp("comp_len");
        let mut iter_typejoin_element_type = None;

        // Step 1: Infer iterator element type and register loop variable (Issue #2125)
        // For ranges like 1:5, the element type is I64. For arrays, use the element type.
        let iter_elem_type = match iter {
            Expr::Range { start, .. } => {
                // Infer element type from the range start expression
                self.infer_expr_type(start)
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

        // Step 2: Infer body type (now uses properly typed loop variable)
        let body_type = plain_unary_var_call(body, var)
            .and_then(|(function, _)| {
                self.infer_local_unary_call_return_type(function, &iter_elem_type)
            })
            .unwrap_or_else(|| self.infer_expr_type(body));
        let runtime_typejoin_result = forced_elem.is_none()
            && matches!(body_type, ValueType::Any)
            && (plain_unary_var_call(body, var)
                .map(|(function, _)| self.locals.contains_key(function))
                .unwrap_or(false)
                || matches!(iter_elem_type, ValueType::Any));

        // Step 3: Create empty result array with appropriate type (Issue #2125)
        // Fallback for unknown body types used to be `ArrayElementType::F64`,
        // which silently coerced non-numeric Any-typed bodies (e.g. the
        // result of `convert(Any, x)` or any call returning `Any`) into a
        // `Vector{Float64}` with coerced element values. Defaulting to
        // `Any` instead matches upstream Julia's "preserve the value, not
        // a guessed shape" behavior (Issue #4822).
        let forced_elem_set = forced_elem.is_some() || forced_runtime_elem.is_some();
        let array_elem_type = if let Some(forced) = forced_elem {
            // Typed comprehension `T[...]` with an explicit target element type
            // whose body type cannot be inferred statically (Issue #5040).
            forced
        } else if runtime_typejoin_result {
            ArrayElementType::UnionOf(Vec::new())
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

        // Compile iterator (can be Array or Range)
        let iter_type = self.compile_expr(iter)?;
        self.locals.insert(iter_var.clone(), iter_type);
        // Use StoreAny/LoadAny to handle both Array and Range iterators
        self.emit(Instr::StoreAny(iter_var.clone()));

        // Get length (via CallBuiltin) - works for both Array and Range
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        self.emit(Instr::StoreI64(len_var.clone()));

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

        // Compute body and push to result (type-aware, Issue #2125)
        let temp_val = self.new_temp("comp_val");
        if runtime_typejoin_result {
            self.compile_expr(body)?;
            self.emit(Instr::StoreAny(temp_val.clone()));
            self.emit(Instr::LoadArray(result_var.clone()));
            self.emit(Instr::LoadAny(temp_val.clone()));
            self.emit(Instr::ArrayPushTypejoin);
            self.emit(Instr::StoreArray(result_var.clone()));
        } else if forced_elem_set
            || matches!(
                array_elem_type,
                ArrayElementType::Any
                    | ArrayElementType::UnionOf(_)
                    | ArrayElementType::Abstract(_)
            )
        {
            // Forced-eltype typed comprehensions (Issue #5040): the body
            // (`convert(T, x)`) already yields a value of the target element
            // type, so push it through the generic boxed path. The array was
            // allocated as a Memory-backed `Array{T}` wrapper, so the runtime
            // eltype stays exactly `T` while each converted element is stored as-is.
            self.compile_expr(body)?;
            self.emit(Instr::StoreAny(temp_val.clone()));
            self.emit(Instr::LoadArray(result_var.clone()));
            self.emit(Instr::LoadAny(temp_val.clone()));
            self.emit(Instr::ArrayPush);
            self.emit(Instr::StoreArray(result_var.clone()));
        } else {
            match body_type {
                ValueType::Tuple => {
                    // Tuple: compile as-is and store as Any
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
                ValueType::Str => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.clone()));
                    self.emit(Instr::LoadAny(temp_val.clone()));
                }
                ValueType::Char => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.clone()));
                    self.emit(Instr::LoadAny(temp_val.clone()));
                }
                ValueType::Symbol => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.clone()));
                    self.emit(Instr::LoadAny(temp_val.clone()));
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
                | ValueType::F32 => {
                    self.compile_expr(body)?;
                    self.emit(Instr::StoreAny(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.clone()));
                    self.emit(Instr::LoadAny(temp_val.clone()));
                }
                _ => {
                    // Default: F64
                    self.compile_expr_as(body, ValueType::F64)?;
                    self.emit(Instr::StoreF64(temp_val.clone()));
                    self.emit(Instr::LoadArray(result_var.clone()));
                    self.emit(Instr::LoadF64(temp_val.clone()));
                }
            }
            self.emit(Instr::ArrayPush);
            self.emit(Instr::StoreArray(result_var.clone()));
        }

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

        // Load result and return appropriate type (Issue #2125)
        self.emit(Instr::LoadArray(result_var));
        if forced_runtime_elem.is_some() {
            Ok(ValueType::Array)
        } else {
            Ok(ValueType::ArrayOf(array_elem_type, None))
        }
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
        let result_var = self.new_temp("comp_result");
        let iterable_var = self.new_temp("comp_iterable");
        let state_var = self.new_temp("comp_state");
        let iter_result_var = self.new_temp("comp_iter_result");
        let elem_var = self.new_temp("comp_elem");
        let temp_val = self.new_temp("comp_val");
        let array_elem_type = ArrayElementType::UnionOf(Vec::new());

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
            self.emit(Instr::StoreAny(var.clone()));
            self.locals.insert(var.clone(), ValueType::Any);
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

        self.emit(Instr::LoadArray(result_var));
        Ok(ValueType::ArrayOf(array_elem_type, None))
    }

    /// Compile a multi-variable comprehension: [expr for var1 in iter1, var2 in iter2, ...]
    /// Produces a flat array via nested loops (cartesian product). Issue #2143.
    pub(in super::super) fn compile_multi_comprehension(
        &mut self,
        body: &Expr,
        iterations: &[(String, Expr)],
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
        iterations: &[(String, Expr)],
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
        flatten: bool,
    ) -> CResult<ValueType> {
        if flatten {
            return self.compile_flatten_comprehension(body, iterations, filter, forced_elem);
        }
        let result_var = self.new_temp("mcomp_result");

        // Register all loop variables for type inference
        for (var, iter) in iterations {
            let iter_elem_type = match iter {
                Expr::Range { start, .. } => self.infer_expr_type(start),
                _ => {
                    let iter_ty = self.infer_expr_type(iter);
                    match iter_ty {
                        ValueType::ArrayOf(ref elem, _) => match elem {
                            ArrayElementType::I64 => ValueType::I64,
                            ArrayElementType::F64 => ValueType::F64,
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
            self.locals.insert(var.clone(), iter_elem_type);
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
                ValueType::F32 => ArrayElementType::F32,
                ValueType::F64 => ArrayElementType::F64,
                ValueType::Bool => ArrayElementType::Bool,
                ValueType::Str => ArrayElementType::String,
                ValueType::Char => ArrayElementType::Char,
                ValueType::Symbol => ArrayElementType::Symbol,
                _ => ArrayElementType::Any,
            }
        };
        self.emit_empty_array_wrapper(array_elem_type.clone(), &[0]);
        self.locals.insert(
            result_var.clone(),
            ValueType::ArrayOf(array_elem_type.clone(), None),
        );
        self.emit(Instr::StoreArray(result_var.clone()));

        // For each iteration clause, compile the iterator and prepare loop vars
        let n = iterations.len();
        let mut iter_vars = Vec::with_capacity(n);
        let mut idx_vars = Vec::with_capacity(n);
        let mut len_vars = Vec::with_capacity(n);

        for (_, iter_expr) in iterations {
            let iter_var = self.new_temp("mcomp_iter");
            let idx_var = self.new_temp("mcomp_idx");
            let len_var = self.new_temp("mcomp_len");

            // Compile and store iterator
            self.compile_expr(iter_expr)?;
            self.locals.insert(iter_var.clone(), ValueType::Any);
            self.emit(Instr::StoreAny(iter_var.clone()));

            // Get length
            self.emit(Instr::LoadAny(iter_var.clone()));
            self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
            self.emit(Instr::StoreI64(len_var.clone()));

            // Initialize index to 1
            self.emit(Instr::PushI64(1));
            self.emit(Instr::StoreI64(idx_var.clone()));

            iter_vars.push(iter_var);
            idx_vars.push(idx_var);
            len_vars.push(len_var);
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
            self.emit(Instr::StoreAny(var.clone()));
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
        let temp_val = self.new_temp("mcomp_val");
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
        iterations: &[(String, Expr)],
        filter: Option<&Expr>,
        forced_elem: Option<ArrayElementType>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("fcomp_result");

        // Register all loop variables for type inference (mirrors the cartesian
        // path). Element type is taken from each iterator independently.
        for (var, iter) in iterations {
            let iter_elem_type = match iter {
                Expr::Range { start, .. } => self.infer_expr_type(start),
                _ => {
                    let iter_ty = self.infer_expr_type(iter);
                    match iter_ty {
                        ValueType::ArrayOf(ref elem, _) => match elem {
                            ArrayElementType::I64 => ValueType::I64,
                            ArrayElementType::F64 => ValueType::F64,
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
            self.locals.insert(var.clone(), iter_elem_type);
        }

        let body_type = self.infer_expr_type(body);
        let array_elem_type = if let Some(forced) = forced_elem {
            forced
        } else {
            match body_type {
                ValueType::Tuple => ArrayElementType::Any,
                ValueType::I64 => ArrayElementType::I64,
                ValueType::F32 => ArrayElementType::F32,
                ValueType::F64 => ArrayElementType::F64,
                ValueType::Bool => ArrayElementType::Bool,
                ValueType::Str => ArrayElementType::String,
                ValueType::Char => ArrayElementType::Char,
                ValueType::Symbol => ArrayElementType::Symbol,
                _ => ArrayElementType::Any,
            }
        };

        // Allocate the empty 1-D result and push into it at the innermost loop.
        self.emit_empty_array_wrapper(array_elem_type.clone(), &[0]);
        self.locals.insert(
            result_var.clone(),
            ValueType::ArrayOf(array_elem_type.clone(), None),
        );
        self.emit(Instr::StoreArray(result_var.clone()));

        self.emit_flatten_levels(iterations, 0, body, &body_type, filter, &result_var)?;

        self.emit(Instr::LoadArray(result_var));
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
        iterations: &[(String, Expr)],
        level: usize,
        body: &Expr,
        body_type: &ValueType,
        filter: Option<&Expr>,
        result_var: &str,
    ) -> CResult<()> {
        if level == iterations.len() {
            // Innermost: apply the filter, then compute the body and push it.
            let j_skip = if let Some(filter_expr) = filter {
                self.compile_expr_as(filter_expr, ValueType::Bool)?;
                let j = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                Some(j)
            } else {
                None
            };

            let temp_val = self.new_temp("fcomp_val");
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
            return Ok(());
        }

        let (var, iter_expr) = &iterations[level];
        let iter_var = self.new_temp("fcomp_iter");
        let idx_var = self.new_temp("fcomp_idx");
        let len_var = self.new_temp("fcomp_len");

        // Compile the iterator INSIDE the enclosing loops so a dependent inner
        // range (`for j in 1:i`) is re-evaluated for each outer value.
        self.compile_expr(iter_expr)?;
        self.locals.insert(iter_var.clone(), ValueType::Any);
        self.emit(Instr::StoreAny(iter_var.clone()));

        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        self.emit(Instr::StoreI64(len_var.clone()));

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
        self.emit(Instr::StoreAny(var.clone()));

        self.emit_flatten_levels(iterations, level + 1, body, body_type, filter, result_var)?;

        // Increment index and loop back.
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var.clone()));
        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit, exit_label);
        Ok(())
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
                if self.locals.contains_key(map_function)
                    || self.locals.contains_key(predicate_function)
                {
                    // Local callables may carry closure state. Do not lower
                    // either side of a filtered generator to a bare function
                    // index, because that drops the captured environment.
                } else {
                    let map_ref = Expr::FunctionRef {
                        name: map_function.to_string(),
                        span: map_span,
                    };
                    let predicate_ref = Expr::FunctionRef {
                        name: predicate_function.to_string(),
                        span: predicate_span,
                    };
                    if let (Ok(map_func_index), Ok(predicate_func_index)) = (
                        self.resolve_function_ref(&map_ref),
                        self.resolve_function_ref(&predicate_ref),
                    ) {
                        if let Some(ValueType::ArrayOf(result_element_type, _)) =
                            self.infer_map_call_return_type(&map_ref, iter)
                        {
                            self.compile_expr(iter)?;
                            self.emit(Instr::MakeGenerator(Box::new(
                                crate::vm::MakeGeneratorOperands {
                                    callable: GeneratorCallable::FilteredFunctionIndex {
                                        map_func_index,
                                        predicate_func_index,
                                    },
                                    result_element_type: Some(result_element_type),
                                },
                            )));
                            return Ok(ValueType::Generator);
                        }
                    }
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
                    let callable = Expr::Var(function.to_string(), span);
                    self.compile_expr(&callable)?;
                    self.compile_expr(iter)?;
                    self.emit(Instr::MakeGeneratorRuntime(false, None));
                    return Ok(ValueType::Generator);
                } else {
                    let func_ref = Expr::FunctionRef {
                        name: function.to_string(),
                        span,
                    };
                    if let Ok(func_index) = self.resolve_function_ref(&func_ref) {
                        if let Some(ValueType::ArrayOf(result_element_type, _)) =
                            self.infer_map_call_return_type(&func_ref, iter)
                        {
                            self.compile_expr(iter)?;
                            self.emit(Instr::MakeGenerator(Box::new(
                                crate::vm::MakeGeneratorOperands {
                                    callable: GeneratorCallable::FunctionIndex(func_index),
                                    result_element_type: Some(result_element_type),
                                },
                            )));
                            return Ok(ValueType::Generator);
                        }
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
        // `Vector{Int64}` / `Array{Int64, 1}` must use the same lowered form
        // the parser produces for a *static* parametric type used as a value:
        // `typeof`-tagged string literal (`BuiltinOp::TypeOf`). Using
        // `Expr::Var("Vector{Int64}")` directly would fail to resolve
        // (`is_builtin_type_name` only knows bare names).
        let ctor_expr = if ctor_name.contains('{') {
            Expr::Builtin {
                name: crate::ir::core::BuiltinOp::TypeOf,
                args: vec![Expr::Literal(
                    crate::ir::core::Literal::Str(ctor_name.to_string()),
                    span,
                )],
                span,
            }
        } else {
            Expr::Var(ctor_name.to_string(), span)
        };
        let args_tuple = Expr::TupleLiteral {
            elements: vec![arg.clone()],
            span,
        };
        let method_error = Expr::Call {
            function: "MethodError".to_string(),
            args: vec![ctor_expr, args_tuple],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
        let throw_call = Expr::Call {
            function: "throw".to_string(),
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
                        JuliaType::Float32 => ArrayElementType::F32,
                        JuliaType::Bottom => ArrayElementType::UnionOf(Vec::new()),
                        JuliaType::Bool => ArrayElementType::Bool,
                        JuliaType::Char => ArrayElementType::Char,
                        JuliaType::Symbol => ArrayElementType::Symbol,
                        JuliaType::String => ArrayElementType::String,
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
                        "Float16" => ArrayElementType::Any, // No native F16 ArrayData; store as Any
                        "Bool" => ArrayElementType::Bool,
                        "Char" => ArrayElementType::Char,
                        "Symbol" => ArrayElementType::Symbol,
                        "String" => ArrayElementType::String,
                        "ComplexF64" => ArrayElementType::ComplexF64,
                        "Union{}" | "Bottom" => ArrayElementType::UnionOf(Vec::new()),
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
            let arg_is_range =
                matches!(
                    arg_julia_type,
                    crate::types::JuliaType::UnitRange
                        | crate::types::JuliaType::StepRange
                        | crate::types::JuliaType::AbstractRange
                ) || matches!(self.infer_expr_type(&args[0]), crate::vm::ValueType::Range);
            // For the no-type-args case (`Vector(1:3)`), route through
            // RangeCollect which yields the natural element type
            // (Int64 for `1:3`, Float64 for `1.0:3.0`).
            if arg_is_range && type_args.is_empty() {
                self.compile_expr(&args[0])?;
                self.emit(crate::vm::Instr::CallBuiltin(
                    crate::builtins::BuiltinId::RangeCollect,
                    1,
                ));
                return Ok(ValueType::ArrayOf(elem_type, None));
            }
            // For the typed case (`Vector{T}(arg)`), synthesize the
            // upstream Julia form `T[T(x) for x in arg]` so the
            // existing typed-comprehension compile path materializes
            // and per-element converts. Covers both the Range arg
            // (Issue #4811 — Pure-Julia `Vector{T}(::AbstractRange)`
            // exists but is unreachable from this intercept) and the
            // Array arg with differing eltype (Issue #4816 — previous
            // no-op shallow copy silently kept the source eltype).
            //
            // Conditions:
            //   - target T is a known concrete type (not Any),
            //   - arg is a range, OR arg is an array whose source
            //     eltype is known to differ from T, OR arg eltype is
            //     unknown (be conservative and convert).
            // When source eltype matches T exactly, the existing
            // no-op fast path is preserved (no comprehension overhead).
            if !type_args.is_empty() && !matches!(elem_type, ArrayElementType::Any) {
                let source_value_type = self.infer_expr_type(&args[0]);
                let needs_conversion = arg_is_range
                    || match &source_value_type {
                        crate::vm::ValueType::ArrayOf(source_elem, _) => *source_elem != elem_type,
                        _ => true,
                    };
                if needs_conversion {
                    let arg_span = args[0].span();
                    let type_name = type_args[0].to_string();
                    let var_name = "__sjvm_vec_ctor_x".to_string();
                    let comp = Expr::Comprehension {
                        body: Box::new(Expr::Call {
                            function: type_name,
                            args: vec![Expr::Var(var_name.clone(), arg_span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span: arg_span,
                        }),
                        var: var_name,
                        iter: Box::new(args[0].clone()),
                        filter: None,
                        span: arg_span,
                    };
                    self.compile_expr(&comp)?;
                    return Ok(ValueType::ArrayOf(elem_type, None));
                }
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
                    function: "_vector_any_collect".to_string(),
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

    /// Compile a Memory{T}(n) constructor call.
    /// Supports:
    /// - Empty memory: Memory{Int64}() → zero-length
    /// - Sized memory: Memory{Float64}(n) → n-element undef-initialized
    /// - With undef: Memory{Int64}(undef, n) → n-element undef-initialized
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
                    _ => ArrayElementType::Any,
                },
                TypeExpr::Parameterized { base, params } => match base.as_str() {
                    "Union" => union_type_params_to_body(params)
                        .map(|body| ArrayElementType::union_from_body(&body))
                        .unwrap_or(ArrayElementType::Any),
                    "Pair" => pair_type_name(base, params)
                        .map(ArrayElementType::Abstract)
                        .unwrap_or(ArrayElementType::Any),
                    _ => ArrayElementType::Any,
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
            // Memory{T}(n) → undef-initialized memory with n elements
            // Check if the arg is `undef` (Memory{T}(undef) is a Julia pattern but means 0-length)
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");
            if is_undef {
                if let Some(type_arg) = &runtime_type_arg {
                    self.emit_type_expr_value_for_array_alloc(Some(type_arg))?;
                    self.emit(Instr::PushI64(0));
                    self.emit(Instr::NewMemoryDynamicTyped);
                } else {
                    self.emit(Instr::NewMemory(elem_type.clone(), 0));
                }
            } else {
                // Compile the size argument and use dynamic NewMemory
                // For now, if it's a literal integer, use static size
                if let Some(type_arg) = &runtime_type_arg {
                    self.emit_type_expr_value_for_array_alloc(Some(type_arg))?;
                    self.compile_expr_as(&args[0], ValueType::I64)?;
                    self.emit(Instr::NewMemoryDynamicTyped);
                } else if let Expr::Literal(crate::ir::core::Literal::Int(n), _) = &args[0] {
                    if *n < 0 {
                        return err(format!("invalid Memory length: {}", n));
                    }
                    self.emit(Instr::NewMemory(elem_type.clone(), *n as usize));
                } else {
                    // Dynamic size: compile size expression, then emit NewMemoryDynamic
                    self.compile_expr_as(&args[0], ValueType::I64)?;
                    self.emit(Instr::NewMemoryDynamic(elem_type.clone()));
                }
            }
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
