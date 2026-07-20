//! Shared expression-inference projections.
//!
//! The compiler should infer Julia-level shape first and project to VM storage
//! tags only at codegen boundaries. This module starts that migration for
//! scalar literals by carrying both the lattice value and the corresponding
//! `CoreType`.

use crate::bytecode::{ArrayElementType, ValueType};
use crate::compile::lattice::types::LatticeType;
use crate::compile::{
    function_name_to_binary_op, is_reducible_nary_operator, parse_parametric_call,
};
use crate::inference_core::{core_type_to_julia_type, CoreType};
use crate::ir::core::{BinaryOp, Expr, Literal};
use crate::types::JuliaType;

#[derive(Debug)]
pub(super) struct InferredExpr {
    lattice_type: LatticeType,
    core_type: CoreType,
}

impl InferredExpr {
    pub(super) fn value_type(&self) -> ValueType {
        crate::runtime_types::bridge::lattice_to_value_type(&self.lattice_type)
    }

    pub(super) fn julia_type(&self) -> JuliaType {
        core_type_to_julia_type(&self.core_type)
    }
}

pub(super) fn infer_scalar_literal(lit: &Literal) -> Option<InferredExpr> {
    let value_type = match lit {
        Literal::Int(_) => ValueType::I64,
        Literal::Int128(_) => ValueType::I128,
        Literal::BigInt(_) => ValueType::BigInt,
        Literal::BigFloat(_) => ValueType::BigFloat,
        Literal::Bool(_) => ValueType::Bool,
        Literal::Float(_) => ValueType::F64,
        Literal::Float32(_) => ValueType::F32,
        Literal::Float16(_) => ValueType::F16,
        Literal::Str(_) => ValueType::Str,
        Literal::StrBytes(_) => ValueType::Str,
        Literal::Char(_) => ValueType::Char,
        Literal::CharMalformed(_) => ValueType::Char,
        Literal::Nothing => ValueType::Nothing,
        Literal::Missing => ValueType::Missing,
        Literal::Module(_) => ValueType::Module,
        Literal::DataType(_) => ValueType::DataType,
        Literal::Array(_, _)
        | Literal::ArrayI64(_, _)
        | Literal::ArrayBool(_, _)
        | Literal::Struct(_, _)
        | Literal::Undef
        | Literal::Symbol(_)
        | Literal::Expr { .. }
        | Literal::QuoteNode(_)
        | Literal::LineNumberNode { .. }
        | Literal::Regex { .. }
        | Literal::Enum { .. } => return None,
    };

    let lattice_type = crate::runtime_types::bridge::value_type_to_lattice(&value_type);
    let core_type = CoreType::from(&lattice_type);
    Some(InferredExpr {
        lattice_type,
        core_type,
    })
}

pub(super) fn folded_nary_operator_call(function: &str, args: &[Expr]) -> Option<Expr> {
    if args.len() <= 2 || !is_reducible_nary_operator(function) {
        return None;
    }
    let binary_op = function_name_to_binary_op(function)?;
    Some(fold_binary_op_args(binary_op, args))
}

fn fold_binary_op_args(binary_op: BinaryOp, args: &[Expr]) -> Expr {
    let span = args[0].span();
    let mut folded = Expr::BinaryOp {
        op: binary_op,
        left: Box::new(args[0].clone()),
        right: Box::new(args[1].clone()),
        span,
    };
    for arg in args.iter().skip(2) {
        folded = Expr::BinaryOp {
            op: binary_op,
            left: Box::new(folded),
            right: Box::new(arg.clone()),
            span: arg.span(),
        };
    }
    folded
}

pub(super) fn is_truncated_result_call(
    function: &str,
    args: &[Expr],
    kwargs: &[(crate::ir::core::InternedStr, Expr)],
) -> bool {
    matches!(function, "truncated" | "Distributions.truncated")
        && (args.len() >= 2
            || kwargs
                .iter()
                .any(|(_, value)| !matches!(value, Expr::Literal(Literal::Nothing, _))))
}

pub(super) fn array_element_type_for_julia_type(
    ty: &JuliaType,
    mut struct_type_id: impl FnMut(&str) -> Option<usize>,
) -> Option<ArrayElementType> {
    if ty.contains_runtime_typevar() || ty.contains_unionall() {
        return Some(ArrayElementType::Structured(Box::new(ty.clone())));
    }

    match ty {
        JuliaType::Int8 => Some(ArrayElementType::I8),
        JuliaType::Int16 => Some(ArrayElementType::I16),
        JuliaType::Int32 => Some(ArrayElementType::I32),
        JuliaType::Int64 => Some(ArrayElementType::I64),
        JuliaType::Int128 => Some(ArrayElementType::I128),
        JuliaType::UInt8 => Some(ArrayElementType::U8),
        JuliaType::UInt16 => Some(ArrayElementType::U16),
        JuliaType::UInt32 => Some(ArrayElementType::U32),
        JuliaType::UInt64 => Some(ArrayElementType::U64),
        JuliaType::UInt128 => Some(ArrayElementType::U128),
        // Issue #9301: Float16 has a boxed storage tag (like I128/U128), so an
        // array whose element JuliaType is Float16 narrows like F32/F64.
        JuliaType::Float16 => Some(ArrayElementType::F16),
        JuliaType::Float32 => Some(ArrayElementType::F32),
        JuliaType::Float64 => Some(ArrayElementType::F64),
        JuliaType::Number => Some(ArrayElementType::Abstract("Number".to_string())),
        JuliaType::Real => Some(ArrayElementType::Abstract("Real".to_string())),
        JuliaType::Integer => Some(ArrayElementType::Abstract("Integer".to_string())),
        JuliaType::Signed => Some(ArrayElementType::Abstract("Signed".to_string())),
        JuliaType::Unsigned => Some(ArrayElementType::Abstract("Unsigned".to_string())),
        JuliaType::AbstractFloat => Some(ArrayElementType::Abstract("AbstractFloat".to_string())),
        JuliaType::Bool => Some(ArrayElementType::Bool),
        JuliaType::Bottom => Some(ArrayElementType::UnionOf(Vec::new())),
        JuliaType::String => Some(ArrayElementType::String),
        JuliaType::Char => Some(ArrayElementType::Char),
        JuliaType::Any => Some(ArrayElementType::Any),
        // Issue #6720: store the structured union members directly.
        JuliaType::Union(types) => Some(ArrayElementType::UnionOf(types.clone())),
        JuliaType::Struct(name) if name == "Complex{Float64}" || name == "ComplexF64" => {
            Some(ArrayElementType::ComplexF64)
        }
        JuliaType::Struct(name) if name == "Complex{Float32}" || name == "ComplexF32" => {
            Some(ArrayElementType::ComplexF32)
        }
        JuliaType::Struct(name) if name.starts_with("Union{") && name.ends_with('}') => {
            Some(ArrayElementType::union_from_body(&name[6..name.len() - 1]))
        }
        JuliaType::Struct(name) => {
            let core = CoreType::from(ty);
            let base_name = core.nominal_base_name().unwrap_or(name);
            struct_type_id(name)
                .or_else(|| struct_type_id(base_name))
                .map(ArrayElementType::StructOf)
        }
        _ => None,
    }
}

pub(super) fn memory_constructor_julia_type(function: &str) -> Option<JuliaType> {
    let function = function.strip_prefix("Base.").unwrap_or(function);
    let (base, type_args) = parse_parametric_call(function)?;
    if base != "Memory" || type_args.len() != 1 {
        return None;
    }

    let elem = JuliaType::from_name_or_struct(&type_args[0].to_string());
    Some(JuliaType::Struct(format!("Memory{{{}}}", elem.name())))
}

pub(super) fn memory_constructor_value_type(
    function: &str,
    struct_type_id: impl FnMut(&str) -> Option<usize>,
) -> Option<ValueType> {
    let JuliaType::Struct(memory_name) = memory_constructor_julia_type(function)? else {
        return Some(ValueType::Memory);
    };
    let elem_name = memory_name
        .strip_prefix("Memory{")
        .and_then(|name| name.strip_suffix('}'))?;
    let elem_type = JuliaType::from_name_or_struct(elem_name);
    let element = array_element_type_for_julia_type(&elem_type, struct_type_id)
        .unwrap_or(ArrayElementType::Any);
    Some(ValueType::MemoryOf(element))
}

/// A constant 1-based integer index, when `idx` is an integer literal.
///
/// Tuple/`NamedTuple` element-type sharpening (Issue #5183) is only sound when
/// the index is a compile-time constant, since each position can carry a
/// different element type. A non-literal index (`t[i]`) must stay dynamic.
pub(super) fn const_tuple_index(idx: &Expr) -> Option<i64> {
    match idx {
        Expr::Literal(Literal::Int(k), _) => Some(*k),
        _ => None,
    }
}

/// The element `JuliaType` at a constant 1-based position of a statically known
/// tuple/`NamedTuple` type, when in bounds (Issue #5183).
///
/// `infer_expr_type`/`infer_julia_type` previously discarded tuple element
/// types — `t[k]`, `first(t)`, `last(t)`, and destructuring positions all
/// collapsed to `Any`, even though the `JuliaType::TupleOf(elem_types)` recorded
/// for the local already carries every element type. Recovering the precise
/// element type lets the codegen typer keep `(a, b) = f()`-style multi-value
/// returns type-stable at the use site.
///
/// Handles:
/// - `JuliaType::TupleOf(elem_types)` — positional element types from a tuple
///   literal / typed local.
/// - concrete `@NamedTuple{a::T1, b::T2}` (a `JuliaType::Struct`) — positional
///   field types, matching `nt[k]` integer indexing.
///
/// Returns `None` (caller falls back to the existing dynamic `Any` path) for the
/// bare `Tuple`/`NamedTuple`, out-of-range indices, non-positive indices, or any
/// tuple whose element list ends in a `Vararg{T}` marker (the length is not
/// statically fixed, so a constant index past the fixed prefix is unsound).
pub(super) fn tuple_element_julia_type(container: &JuliaType, one_based: i64) -> Option<JuliaType> {
    // 1-based index; reject non-positive before converting to a `usize` offset.
    let zero_based = usize::try_from(one_based.checked_sub(1)?).ok()?;
    let elem_types = tuple_field_julia_types(container)?;
    elem_types.get(zero_based).cloned()
}

/// Positional element `JuliaType`s of a statically known tuple/`NamedTuple`
/// type, or `None` when the element list is not a fully fixed concrete sequence.
fn tuple_field_julia_types(container: &JuliaType) -> Option<Vec<JuliaType>> {
    match container {
        JuliaType::TupleOf(elem_types) => {
            // A trailing `Vararg{T}` leaf means the arity is not fixed; refuse to
            // index, since a constant position may land inside the vararg tail.
            if let Some(last) = elem_types.last() {
                if crate::types::unbounded_vararg_element(last).is_some() {
                    return None;
                }
            }
            Some(elem_types.clone())
        }
        JuliaType::Struct(name) if name.starts_with("@NamedTuple{") && name.ends_with('}') => {
            let body = &name["@NamedTuple{".len()..name.len() - 1];
            if body.trim().is_empty() {
                return Some(Vec::new());
            }
            let mut field_types = Vec::new();
            for field in split_named_tuple_fields(body) {
                // Each field is `name::Type`; an `Any`-typed field omits `::`.
                let ty = match field.split_once("::") {
                    Some((_, ty_str)) => JuliaType::from_name_or_struct(ty_str.trim()),
                    None => JuliaType::Any,
                };
                field_types.push(ty);
            }
            Some(field_types)
        }
        _ => None,
    }
}

/// Split the body of a `@NamedTuple{...}` into its top-level `name::Type`
/// fields, respecting nested `{}`/`()` so parametric field types
/// (`x::Tuple{Int, Int}`) are not split on their inner commas.
fn split_named_tuple_fields(body: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(body[start..i].trim());
                start = i + c.len_utf8();
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn const_index_only_matches_integer_literals() {
        assert_eq!(
            const_tuple_index(&Expr::Literal(Literal::Int(2), span())),
            Some(2)
        );
        assert_eq!(
            const_tuple_index(&Expr::Var("i".to_string().into(), span())),
            None
        );
        assert_eq!(
            const_tuple_index(&Expr::Literal(Literal::Float(1.0), span())),
            None
        );
    }

    #[test]
    fn tuple_element_type_sharpens_by_position() {
        let t = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Float64]);
        assert_eq!(tuple_element_julia_type(&t, 1), Some(JuliaType::Int64));
        assert_eq!(tuple_element_julia_type(&t, 2), Some(JuliaType::Float64));
    }

    #[test]
    fn tuple_element_type_rejects_out_of_range_and_nonpositive() {
        let t = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::String]);
        assert_eq!(tuple_element_julia_type(&t, 0), None);
        assert_eq!(tuple_element_julia_type(&t, -1), None);
        assert_eq!(tuple_element_julia_type(&t, 3), None);
    }

    #[test]
    fn bare_tuple_type_does_not_sharpen() {
        assert_eq!(tuple_element_julia_type(&JuliaType::Tuple, 1), None);
        assert_eq!(tuple_element_julia_type(&JuliaType::NamedTuple, 1), None);
        assert_eq!(tuple_element_julia_type(&JuliaType::Any, 1), None);
    }

    #[test]
    fn vararg_tail_tuple_does_not_sharpen() {
        // Tuple{Int64, Vararg{Float64}} — fixed prefix is Int64, but the arity is
        // unbounded, so indexing must stay dynamic.
        let t = JuliaType::TupleOf(vec![
            JuliaType::Int64,
            JuliaType::Struct("Vararg{Float64}".to_string()),
        ]);
        assert_eq!(tuple_element_julia_type(&t, 1), None);
    }

    #[test]
    fn concrete_named_tuple_sharpens_by_position() {
        let nt = JuliaType::Struct("@NamedTuple{a::Int64, b::Float64}".to_string());
        assert_eq!(tuple_element_julia_type(&nt, 1), Some(JuliaType::Int64));
        assert_eq!(tuple_element_julia_type(&nt, 2), Some(JuliaType::Float64));
        assert_eq!(tuple_element_julia_type(&nt, 3), None);
    }

    #[test]
    fn named_tuple_with_parametric_field_type_not_split_on_inner_comma() {
        let nt = JuliaType::Struct("@NamedTuple{a::Tuple{Int64, Int64}, b::Float64}".to_string());
        assert_eq!(
            tuple_element_julia_type(&nt, 1),
            Some(JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]))
        );
        assert_eq!(tuple_element_julia_type(&nt, 2), Some(JuliaType::Float64));
    }

    #[test]
    fn scalar_literal_inference_projects_from_lattice_and_core() {
        let inferred = infer_scalar_literal(&Literal::Int(1)).expect("literal inference");
        assert_eq!(inferred.value_type(), ValueType::I64);
        assert_eq!(inferred.julia_type(), JuliaType::Int64);

        let inferred = infer_scalar_literal(&Literal::Float32(1.0)).expect("literal inference");
        assert_eq!(inferred.value_type(), ValueType::F32);
        assert_eq!(inferred.julia_type(), JuliaType::Float32);

        assert!(infer_scalar_literal(&Literal::Symbol("x".to_string())).is_none());
    }

    #[test]
    fn inferred_expr_projects_structured_unionall_without_name_reparse_issue_10460() {
        let binder = crate::inference_core::CoreTypeVar::with_bounds(
            "T",
            Some(Box::new(CoreType::Primitive(
                crate::inference_core::CorePrimitive::Int64,
            ))),
            Some(Box::new(CoreType::Abstract(
                crate::inference_core::CoreAbstract::Real,
            ))),
        );
        let inferred = InferredExpr {
            lattice_type: LatticeType::Top,
            core_type: CoreType::UnionAll {
                var: binder.clone(),
                body: Box::new(CoreType::Struct {
                    name: "Owner.Box".to_string(),
                    params: vec![CoreType::TypeVar(binder)],
                }),
            },
        };

        assert!(matches!(
            inferred.julia_type(),
            JuliaType::UnionAll {
                var,
                lower_bound: Some(lower),
                bound: Some(upper),
                body,
            } if var == "T"
                && lower.as_str() == "Int64"
                && upper.as_str() == "Real"
                && matches!(
                    body.as_ref(),
                    JuliaType::Struct(name) if name == "Owner.Box{Int64<:T<:Real}"
                )
        ));
    }

    #[test]
    fn compile_array_eltype_preserves_partial_unionall_identity_issue_10460() {
        let binder = JuliaType::RuntimeTypeVar {
            id: 104_603,
            name: "N".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let partial = JuliaType::RuntimeUnionAll {
            var: Box::new(binder.clone()),
            body: Box::new(JuliaType::RuntimeParametric {
                base: "Partial10460".to_string(),
                params: vec![JuliaType::Int8, binder],
            }),
        };

        assert_eq!(
            array_element_type_for_julia_type(&partial, |_| None),
            Some(ArrayElementType::Structured(Box::new(partial)))
        );
    }
}
