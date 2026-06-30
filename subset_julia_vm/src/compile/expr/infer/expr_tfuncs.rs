//! Adapters from expression-level inference to the shared tfunc registry.
//!
//! Issue #5919 tracks replacing duplicated ad-hoc return-type gates in
//! `infer_expr_type` and `infer_julia_type` with declarative transfer
//! functions. This module is the strangler adapter: selected return rules are
//! evaluated through `compile::tfuncs` and then translated back to the two
//! legacy inference representations.

use crate::inference_core::CorePrimitive;
use crate::inference_core::{CoreAbstract, CoreType};
use std::sync::OnceLock;

use super::shared;
use crate::compile::context::SharedCompileContext;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::tfuncs::{register_all, StructIdLookup, TFuncContext, TransferFunctions};
use crate::compile::types::parse_parametric_call;
use crate::ir::core::Expr;
use crate::types::{JuliaType, TypeExpr};
use crate::vm::value::{array_element_type_to_julia_type, julia_array_type_for_ndims};
use crate::vm::{ArrayElementType, ValueType};

static EXPR_TFUNCS: OnceLock<TransferFunctions> = OnceLock::new();

fn registry() -> &'static TransferFunctions {
    EXPR_TFUNCS.get_or_init(|| {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);
        registry
    })
}

/// [`StructIdLookup`] view over the compile-side struct tables (Issue #5922).
///
/// This is the expression-inference counterpart of the abstract-interp
/// engine's `StructTypeInfo` table: it lets the shared registry resolve
/// constructor results (`complex`, default struct constructors) against
/// `SharedCompileContext` without the registry depending on its concrete
/// shape.
pub(super) struct SharedCtxStructIds<'a>(pub(super) &'a SharedCompileContext);

impl std::fmt::Debug for SharedCtxStructIds<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedCtxStructIds")
            .field(&"<SharedCompileContext>")
            .finish()
    }
}

fn shared_ctx_struct_type_id(ctx: &SharedCompileContext, name: &str) -> Option<usize> {
    ctx.struct_table.get(name).map(|info| info.type_id)
}

fn shared_ctx_instantiation_of(
    ctx: &SharedCompileContext,
    base_name: &str,
) -> Option<(String, usize)> {
    let prefix = format!("{}{{", base_name);
    ctx.struct_table
        .iter()
        .filter(|(name, _)| name.starts_with(&prefix))
        .min_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(name, info)| (name.clone(), info.type_id))
}

impl StructIdLookup for SharedCtxStructIds<'_> {
    fn struct_type_id(&self, name: &str) -> Option<usize> {
        shared_ctx_struct_type_id(self.0, name)
    }

    fn instantiation_of(&self, base_name: &str) -> Option<(String, usize)> {
        shared_ctx_instantiation_of(self.0, base_name)
    }
}

/// Narrow mutable instantiation interface for parametric constructor
/// return-type resolution (Issue #5922 wave 5).
///
/// The earlier waves migrated the *exact struct-table entry* constructor rule
/// into the shared registry (via [`StructIdLookup`]); the parametric arms were
/// deferred because `SharedCompileContext::resolve_instantiation` instantiates
/// the concrete struct-table entry **on demand** and therefore needs `&mut`.
/// This trait is that narrow `&mut` seam. The resolution rules themselves stay
/// in this adapter (NOT in the generic registry dispatch): wave 2 established
/// that pushing struct-ctor rules into registry-wide dispatch over-matches
/// engine call sites (`Base.Generator` collapsed to a struct id).
pub(super) trait StructInstantiation: StructIdLookup {
    /// Type arguments inferred from the default constructor's argument types,
    /// or `None` when inference fails.
    fn infer_ctor_type_args(
        &self,
        base_name: &str,
        arg_types: &[JuliaType],
    ) -> Option<Vec<JuliaType>>;
    /// Resolve (or create on demand) the concrete instantiation's type id.
    fn resolve_instantiation(&mut self, base_name: &str, type_args: &[JuliaType]) -> Option<usize>;
    /// Resolve (or create on demand) a concrete instantiation whose parameters
    /// include value-level type parameters such as `1` or `true`.
    fn resolve_instantiation_with_type_expr(
        &mut self,
        _base_name: &str,
        _type_args: &[TypeExpr],
    ) -> Option<usize> {
        None
    }
    /// Any type id whose base name matches (exact entry, instantiation table,
    /// or struct defs) — `SharedCompileContext::get_struct_type_id`.
    fn base_struct_type_id(&self, base_name: &str) -> Option<usize>;
}

/// [`StructInstantiation`] view over `SharedCompileContext` (Issue #5922).
pub(super) struct SharedCtxInstantiation<'a>(pub(super) &'a mut SharedCompileContext);

impl std::fmt::Debug for SharedCtxInstantiation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedCtxInstantiation")
            .field(&"<SharedCompileContext>")
            .finish()
    }
}

impl StructIdLookup for SharedCtxInstantiation<'_> {
    fn struct_type_id(&self, name: &str) -> Option<usize> {
        shared_ctx_struct_type_id(self.0, name)
    }

    fn instantiation_of(&self, base_name: &str) -> Option<(String, usize)> {
        shared_ctx_instantiation_of(self.0, base_name)
    }
}

impl StructInstantiation for SharedCtxInstantiation<'_> {
    fn infer_ctor_type_args(
        &self,
        base_name: &str,
        arg_types: &[JuliaType],
    ) -> Option<Vec<JuliaType>> {
        self.0.infer_type_args(base_name, arg_types).ok()
    }

    fn resolve_instantiation(&mut self, base_name: &str, type_args: &[JuliaType]) -> Option<usize> {
        self.0.resolve_instantiation(base_name, type_args).ok()
    }

    fn resolve_instantiation_with_type_expr(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
    ) -> Option<usize> {
        self.0
            .resolve_instantiation_with_type_expr(base_name, type_args)
            .ok()
    }

    fn base_struct_type_id(&self, base_name: &str) -> Option<usize> {
        self.0.get_struct_type_id(base_name)
    }
}

/// ValueType-path resolution for parametric struct constructors
/// (Issue #5922).
///
/// Resolution order pins the legacy gate: inferred type args → exact concrete
/// struct-table entry (`Base{A, B}`) → on-demand instantiation → any existing
/// instantiation of the base (deterministic smallest name; the legacy gate
/// used HashMap iteration order here) → `Any`. When type-arg inference fails,
/// fall back to any type id registered under the base name.
pub(super) fn infer_value_parametric_struct_ctor(
    function: &str,
    inst: &mut dyn StructInstantiation,
    arg_types: &[JuliaType],
) -> ValueType {
    let Some(type_args) = inst.infer_ctor_type_args(function, arg_types) else {
        return inst
            .base_struct_type_id(function)
            .map(ValueType::Struct)
            .unwrap_or(ValueType::Any);
    };
    let type_arg_names: Vec<String> = type_args.iter().map(|t| t.name().to_string()).collect();
    let concrete_name = if type_arg_names.is_empty() {
        function.to_string()
    } else {
        format!("{}{{{}}}", function, type_arg_names.join(", "))
    };
    if let Some(type_id) = inst.struct_type_id(&concrete_name) {
        return ValueType::Struct(type_id);
    }
    // Instantiate on demand so type inference can be precise for arrays.
    if let Some(type_id) = inst.resolve_instantiation(function, &type_args) {
        return ValueType::Struct(type_id);
    }
    inst.instantiation_of(function)
        .map(|(_, type_id)| ValueType::Struct(type_id))
        .unwrap_or(ValueType::Any)
}

/// ValueType-path resolution for the parametric `Rational` constructor
/// (Issue #5922): exact struct-table entry first, then any type id registered
/// under the `Rational` base name.
pub(super) fn infer_value_rational_ctor(
    function: &str,
    inst: &dyn StructInstantiation,
) -> ValueType {
    if let Some(type_id) = inst.struct_type_id(function) {
        ValueType::Struct(type_id)
    } else if let Some(type_id) = inst.base_struct_type_id("Rational") {
        ValueType::Struct(type_id)
    } else {
        ValueType::Any
    }
}

pub(super) fn infer_value_view_call(
    function: &str,
    value_args: &[ValueType],
    julia_args: &[JuliaType],
    inst: &mut dyn StructInstantiation,
) -> Option<ValueType> {
    if !matches!(function, "view" | "Base.view") || value_args.len() != 2 {
        return None;
    }
    if !is_unit_range_argument(&value_args[1], julia_args.get(1)?) {
        return None;
    }

    let element_type = vector_element_julia_type(&value_args[0], &julia_args[0])?;
    if matches!(element_type, JuliaType::Any) {
        return None;
    }

    let unit_range_int = TypeExpr::Parameterized {
        base: "UnitRange".to_string(),
        params: vec![TypeExpr::Concrete(JuliaType::Int64)],
    };
    let subarray_args = vec![
        TypeExpr::Concrete(element_type.clone()),
        TypeExpr::TypeVar("1".to_string()),
        TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::Concrete(element_type)],
        },
        TypeExpr::Parameterized {
            base: "Tuple".to_string(),
            params: vec![unit_range_int],
        },
        TypeExpr::TypeVar("true".to_string()),
    ];
    inst.resolve_instantiation_with_type_expr("SubArray", &subarray_args)
        .map(ValueType::Struct)
}

pub(super) fn infer_julia_view_call(function: &str, julia_args: &[JuliaType]) -> Option<JuliaType> {
    if !matches!(function, "view" | "Base.view") || julia_args.len() != 2 {
        return None;
    }
    if !is_unit_range_julia_type(julia_args.get(1)?) {
        return None;
    }
    let element_type = vector_element_julia_type_from_julia(&julia_args[0])?;
    if matches!(element_type, JuliaType::Any) {
        return None;
    }
    Some(JuliaType::Struct(format!(
        "SubArray{{{}, 1, Vector{{{}}}, Tuple{{UnitRange{{Int64}}}}, true}}",
        element_type.name(),
        element_type.name()
    )))
}

fn is_unit_range_argument(value_arg: &ValueType, julia_arg: &JuliaType) -> bool {
    matches!(value_arg, ValueType::Range) || is_unit_range_julia_type(julia_arg)
}

fn is_unit_range_julia_type(julia_arg: &JuliaType) -> bool {
    matches!(julia_arg, JuliaType::UnitRange)
        || matches!(julia_arg, JuliaType::Struct(name) if name.starts_with("UnitRange{"))
}

fn vector_element_julia_type(value_arg: &ValueType, julia_arg: &JuliaType) -> Option<JuliaType> {
    match value_arg {
        ValueType::ArrayOf(element_type, Some(1) | None) => {
            Some(array_element_type_to_julia_type(element_type))
        }
        _ => vector_element_julia_type_from_julia(julia_arg),
    }
}

fn vector_element_julia_type_from_julia(julia_arg: &JuliaType) -> Option<JuliaType> {
    match julia_arg {
        JuliaType::VectorOf(element_type) => Some((**element_type).clone()),
        JuliaType::Struct(name) if name.starts_with("Vector{") => parse_parametric_call(name)
            .and_then(|(base, args)| {
                (base == "Vector" && args.len() == 1).then(|| args[0].to_julia_type_lossy())
            }),
        _ => None,
    }
}

/// ValueType-path resolution for `{`-instantiated constructor names such as
/// `Val{1}()` / `Point{Int64}()` (Issue #5922). `resolved_base_name` is the
/// caller-resolved (possibly module-qualified) parametric base name.
///
/// The type-argument parse is the legacy naive top-level `,` split — nested
/// braced parameters were not supported by the gate this replaces either.
pub(super) fn infer_value_instantiated_ctor(
    function: &str,
    resolved_base_name: &str,
    inst: &mut dyn StructInstantiation,
) -> ValueType {
    if let Some(type_id) = inst.struct_type_id(function) {
        return ValueType::Struct(type_id);
    }
    let Some(open) = function.find('{') else {
        return ValueType::Any;
    };
    let type_args_str = &function[open + 1..function.len() - 1];
    let type_args: Vec<JuliaType> = type_args_str
        .split(',')
        .map(|s| JuliaType::from_name_or_struct(s.trim()))
        .collect();
    if let Some(type_id) = inst.resolve_instantiation(resolved_base_name, &type_args) {
        return ValueType::Struct(type_id);
    }
    inst.instantiation_of(resolved_base_name)
        .map(|(_, type_id)| ValueType::Struct(type_id))
        .unwrap_or(ValueType::Any)
}

/// Map a constructor-result lattice type to the legacy `ValueType`
/// representation.
///
/// Deliberately bypasses `ValueType::from(&LatticeType)`: the bridge aliases
/// `Complex{Float64}` to `ValueType::ComplexF64`, but the legacy gates this
/// adapter replaces returned `ValueType::Struct(type_id)`. Pin that behavior
/// (Issue #5922).
fn constructor_lattice_to_value_type(result: &LatticeType) -> Option<ValueType> {
    match result {
        LatticeType::Concrete(ConcreteType::Struct { type_id, .. }) => {
            Some(ValueType::Struct(*type_id))
        }
        _ => None,
    }
}

/// ValueType-path adapter for the lowercase `complex` constructor
/// (Issue #5922).
pub(super) fn infer_value_complex_call(ids: &dyn StructIdLookup) -> Option<ValueType> {
    let ctx = TFuncContext::with_struct_ids(ids);
    let result = registry().infer_return_type_with_context("complex", &[], &ctx);
    constructor_lattice_to_value_type(&result)
}

/// JuliaType-path adapter for the lowercase `complex` constructor
/// (Issue #5922).
///
/// The legacy gate unconditionally returned `Struct("Complex{Float64}")`;
/// keep that as the fallback when the struct table has no Complex entry so
/// the two representation paths cannot drift apart.
pub(super) fn infer_julia_complex_call(ids: &dyn StructIdLookup) -> JuliaType {
    let ctx = TFuncContext::with_struct_ids(ids);
    match registry().infer_return_type_with_context("complex", &[], &ctx) {
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => JuliaType::Struct(name),
        _ => JuliaType::Struct("Complex{Float64}".to_string()),
    }
}

fn is_dict_constructor_name(function: &str) -> bool {
    function == "Dict" || function.starts_with("Dict{")
}

fn explicit_dict_constructor_type_args(function: &str) -> Option<Vec<JuliaType>> {
    let (base, type_args) = parse_parametric_call(function)?;
    if base != "Dict" || type_args.len() != 2 {
        return None;
    }
    Some(
        type_args
            .iter()
            .map(|ty| ty.to_julia_type_lossy())
            .collect(),
    )
}

fn dict_struct_julia_type_from_lattice(result: &LatticeType) -> Option<JuliaType> {
    let LatticeType::Concrete(ConcreteType::Dict { key, value }) = result else {
        return None;
    };
    let key_ty =
        crate::compile::bridge::lattice_to_julia_type(&LatticeType::Concrete((**key).clone()));
    let value_ty =
        crate::compile::bridge::lattice_to_julia_type(&LatticeType::Concrete((**value).clone()));
    Some(JuliaType::Struct(format!(
        "Dict{{{},{}}}",
        key_ty.name(),
        value_ty.name()
    )))
}

fn dict_constructor_arg_lattice(arg: &JuliaType) -> LatticeType {
    if let JuliaType::Struct(name) = arg {
        if name.split('{').next() == Some("Pair") {
            return LatticeType::Concrete(ConcreteType::Struct {
                name: name.clone(),
                type_id: 0,
            });
        }
    }
    julia_type_to_lattice(arg)
}

fn dict_struct_value_type(
    inst: &mut dyn StructInstantiation,
    dict_ty: &JuliaType,
) -> Option<ValueType> {
    let JuliaType::Struct(name) = dict_ty else {
        return None;
    };
    let (base, type_args) = parse_parametric_call(name)?;
    if base != "Dict" || type_args.len() != 2 {
        return None;
    }
    let julia_args: Vec<JuliaType> = type_args
        .iter()
        .map(|ty| ty.to_julia_type_lossy())
        .collect();
    inst.resolve_instantiation("Dict", &julia_args)
        .map(ValueType::Struct)
}

/// ValueType-path adapter for public `Dict` construction (Issue #6619).
pub(super) fn infer_value_dict_constructor_call(
    function: &str,
    arg_types: &[JuliaType],
    inst: &mut dyn StructInstantiation,
) -> Option<ValueType> {
    if !is_dict_constructor_name(function) {
        return None;
    }

    if let Some(type_args) = explicit_dict_constructor_type_args(function) {
        return Some(
            inst.resolve_instantiation("Dict", &type_args)
                .map(ValueType::Struct)
                .unwrap_or(ValueType::Any),
        );
    }

    let lattice_args: Vec<LatticeType> =
        arg_types.iter().map(dict_constructor_arg_lattice).collect();
    let result = registry().infer_return_type("Dict", &lattice_args);
    let Some(dict_ty) = dict_struct_julia_type_from_lattice(&result) else {
        return Some(ValueType::Any);
    };
    Some(dict_struct_value_type(inst, &dict_ty).unwrap_or(ValueType::Any))
}

/// JuliaType-path adapter for public `Dict` construction (Issue #6619).
pub(super) fn infer_julia_dict_constructor_call<F>(
    function: &str,
    args: &[Expr],
    infer_arg: &mut F,
) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    if !is_dict_constructor_name(function) {
        return None;
    }

    if let Some(type_args) = explicit_dict_constructor_type_args(function) {
        return Some(JuliaType::Struct(format!(
            "Dict{{{},{}}}",
            type_args[0].name(),
            type_args[1].name()
        )));
    }

    let lattice_args: Vec<LatticeType> = args
        .iter()
        .map(|arg| dict_constructor_arg_lattice(&infer_arg(arg)))
        .collect();
    let result = registry().infer_return_type("Dict", &lattice_args);
    dict_struct_julia_type_from_lattice(&result).or(Some(JuliaType::Any))
}

fn is_set_constructor_name(function: &str) -> bool {
    function == "Set" || function.starts_with("Set{")
}

/// `Set{T}` explicit element type argument, if the call names one.
fn explicit_set_constructor_type_arg(function: &str) -> Option<JuliaType> {
    let (base, type_args) = parse_parametric_call(function)?;
    if base != "Set" || type_args.len() != 1 {
        return None;
    }
    Some(type_args[0].to_julia_type_lossy())
}

fn set_struct_julia_type_from_lattice(result: &LatticeType) -> Option<JuliaType> {
    let LatticeType::Concrete(ConcreteType::Set { element }) = result else {
        return None;
    };
    let element_ty =
        crate::compile::bridge::lattice_to_julia_type(&LatticeType::Concrete((**element).clone()));
    Some(JuliaType::Struct(format!("Set{{{}}}", element_ty.name())))
}

fn set_struct_value_type(
    inst: &mut dyn StructInstantiation,
    set_ty: &JuliaType,
) -> Option<ValueType> {
    let JuliaType::Struct(name) = set_ty else {
        return None;
    };
    let (base, type_args) = parse_parametric_call(name)?;
    if base != "Set" || type_args.len() != 1 {
        return None;
    }
    let element_ty = type_args[0].to_julia_type_lossy();
    inst.resolve_instantiation("Set", &[element_ty])
        .map(ValueType::Struct)
}

/// ValueType-path adapter for public `Set` construction (Issue #6721). Mirrors
/// `infer_value_dict_constructor_call`: `Set([...])` / `Set{T}(...)` infer to the
/// pure-Julia `Set{T}` struct instantiation so user `Set{T}` methods dispatch.
pub(super) fn infer_value_set_constructor_call(
    function: &str,
    arg_types: &[JuliaType],
    inst: &mut dyn StructInstantiation,
) -> Option<ValueType> {
    if !is_set_constructor_name(function) {
        return None;
    }

    if let Some(element_ty) = explicit_set_constructor_type_arg(function) {
        return Some(
            inst.resolve_instantiation("Set", &[element_ty])
                .map(ValueType::Struct)
                .unwrap_or(ValueType::Any),
        );
    }

    let lattice_args: Vec<LatticeType> = arg_types.iter().map(julia_type_to_lattice).collect();
    let result = registry().infer_return_type("Set", &lattice_args);
    let Some(set_ty) = set_struct_julia_type_from_lattice(&result) else {
        return Some(ValueType::Any);
    };
    Some(set_struct_value_type(inst, &set_ty).unwrap_or(ValueType::Any))
}

/// JuliaType-path adapter for public `Set` construction (Issue #6721).
pub(super) fn infer_julia_set_constructor_call<F>(
    function: &str,
    args: &[Expr],
    infer_arg: &mut F,
) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    if !is_set_constructor_name(function) {
        return None;
    }

    if let Some(element_ty) = explicit_set_constructor_type_arg(function) {
        return Some(JuliaType::Struct(format!("Set{{{}}}", element_ty.name())));
    }

    let lattice_args: Vec<LatticeType> = args
        .iter()
        .map(|arg| julia_type_to_lattice(&infer_arg(arg)))
        .collect();
    let result = registry().infer_return_type("Set", &lattice_args);
    set_struct_julia_type_from_lattice(&result).or(Some(JuliaType::Any))
}

/// ValueType-path adapter for `LinearAlgebra.f(args...)` module calls
/// (Issue #5922).
///
/// The result-shape rules live in the shared registry under
/// `LinearAlgebra.`-qualified keys (see `register_linear_algebra`), so they
/// fire only for module-qualified call sites; bare `det` / `transpose` / ...
/// keep their builtin-op / method-dispatch routing. The legacy nested match
/// ignored argument types, hence the empty argument list here.
///
/// The Array mapping is pinned: the legacy gate returned the unparameterized
/// `ValueType::Array` for inv/eigvals/transpose, not `ArrayOf(Any)`.
#[cfg(test)]
fn infer_value_linear_algebra_call(function: &str) -> Option<ValueType> {
    let key = format!("LinearAlgebra.{function}");
    let result = general_tfunc_result(&key, std::iter::empty(), 0)?;
    Some(match &result {
        LatticeType::Concrete(ConcreteType::Array { .. }) => ValueType::Array,
        other => ValueType::from(other),
    })
}

/// JuliaType-path rule for bare `collect(itr)` (Issue #5922).
///
/// Routed through the registry's `collect` tfunc with the legacy results
/// pinned:
/// - `UnitRange` / `StepRange` arguments collect to `Vector{Int64}` (the
///   JuliaType range carriers do not track an element type; the legacy gate
///   pinned Int64),
/// - a `VectorOf` argument keeps its element type,
/// - everything else stays the bare `Array`.
fn infer_julia_collect_call<F>(args: &[Expr], infer_arg: &mut F) -> JuliaType
where
    F: FnMut(&Expr) -> JuliaType,
{
    let Some(first_expr) = args.first() else {
        return JuliaType::Array;
    };
    let first = infer_arg(first_expr);
    let first_lattice = match &first {
        JuliaType::UnitRange | JuliaType::StepRange => LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        }),
        other => julia_type_to_lattice(other),
    };
    // The legacy gate only consulted the first argument (e.g. `collect(T,
    // itr)` still keyed off `args[0]`), so present the call as unary to the
    // registry's exact-arity `collect` rule.
    if let Some(result) = general_tfunc_result("collect", [first_lattice], 1) {
        if let Some(julia) = lattice_to_julia_type(&result) {
            return julia;
        }
    }
    match first {
        JuliaType::VectorOf(elem) => JuliaType::VectorOf(elem),
        _ => JuliaType::Array,
    }
}

/// ValueType-path adapter for default (exact struct-table entry) struct
/// constructors (Issue #5922).
///
/// Parametric instantiation (`infer_type_args` + `resolve_instantiation`)
/// mutates `SharedCompileContext`, so it stays at the call site; this covers
/// the non-parametric arm through the registry's shared constructor rule.
pub(super) fn infer_value_struct_constructor_call(
    function: &str,
    ids: &dyn StructIdLookup,
) -> Option<ValueType> {
    let ctx = TFuncContext::with_struct_ids(ids);
    let result = TransferFunctions::struct_constructor_result(function, &ctx)?;
    constructor_lattice_to_value_type(&result)
}

#[derive(Clone)]
enum FixedFallback {
    String,
    Bool,
    Concrete(ConcreteType),
    ConcreteDeferStructAny(ConcreteType),
}

fn normalized_first_arg_tfunc_name(function: &str) -> Option<&str> {
    let name = function.strip_prefix("Base.").unwrap_or(function);
    match name {
        "replace" | "repeat" | "reverse" | "abs" | "abs2" | "sign" => Some(name),
        _ => None,
    }
}

fn normalized_value_general_tfunc(function: &str) -> Option<(&str, FixedFallback)> {
    let name = function.strip_prefix("Base.").unwrap_or(function);
    match name {
        "string" | "uppercase" | "lowercase" | "join" | "repr" | "strip" | "lstrip" | "rstrip"
        | "chomp" | "chop" | "take!" | "takestring!" | "sprint" | "sprintf" | "lowercasefirst"
        | "uppercasefirst" | "escape_string" | "chopprefix" | "chopsuffix" | "lpad" | "rpad"
        | "bitstring" | "ascii" | "unescape_string" => Some((name, FixedFallback::String)),
        "isa" => Some((name, FixedFallback::Bool)),
        // `haskey` is a regular, user-overridable function: a custom receiver
        // may return a non-Bool value (Issue #6610). Defer to runtime dispatch
        // when the receiver type is unknown; a concrete Dict still infers Bool.
        "haskey" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Bool,
            ))),
        )),
        "isless" | "isnan" | "isinf" | "isfinite" | "isinteger" | "iseven" | "isodd"
        | "isnothing" | "ismissing" => Some((name, FixedFallback::Bool)),
        "length" | "size" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "ndims" | "count" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "sqrt" | "sin" | "cos" | "exp" | "log" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh"
        | "atanh" | "log2" | "log10" | "log1p" | "expm1" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "signbit" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Bool,
            ))),
        )),
        "min" | "max" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "floor" | "ceil" | "round" | "trunc" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "prod" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "mean" | "std" | "var" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        // gcd/lcm preserve BigInt and default to Int64 (Issues #5922, #2383).
        "gcd" | "lcm" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "big" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
        )),
        "IOBuffer" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO))),
        )),
        "typeof" | "promote_type" | "promote_rule" | "eltype" | "keytype" | "valtype" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::DataType {
                name: "DataType".to_string(),
            }),
        )),
        _ => normalized_constructor_tfunc(name),
    }
}

fn normalized_julia_general_tfunc(function: &str) -> Option<(&str, FixedFallback)> {
    let name = function.strip_prefix("Base.").unwrap_or(function);
    match name {
        "string" | "uppercase" | "lowercase" | "join" | "repr" | "strip" | "lstrip" | "rstrip"
        | "chomp" | "chop" | "take!" | "takestring!" | "sprint" | "lowercasefirst"
        | "uppercasefirst" | "escape_string" | "chopprefix" | "chopsuffix" | "lpad" | "rpad"
        | "bitstring" | "ascii" | "unescape_string" => Some((name, FixedFallback::String)),
        "startswith" | "endswith" | "contains" | "occursin" | "isa" | "isless" | "isnan"
        | "isinf" | "isfinite" | "isinteger" | "iseven" | "isodd" | "isnothing" | "ismissing" => {
            Some((name, FixedFallback::Bool))
        }
        // `haskey` is user-overridable: defer to runtime dispatch for unknown
        // receivers so a custom non-Bool return is not coerced (Issue #6610).
        "haskey" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Bool,
            ))),
        )),
        "length" | "size" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "ndims" | "count" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "div" | "rem" | "mod" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "sqrt" | "sin" | "cos" | "exp" | "log" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh"
        | "atanh" | "log2" | "log10" | "log1p" | "expm1" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "signbit" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Bool,
            ))),
        )),
        "min" | "max" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "floor" | "ceil" | "round" | "trunc" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "prod" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "mean" | "std" | "var" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        )),
        "Int" => Some((
            "Int64",
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        // The legacy gate deferred IOBuffer to method dispatch for Struct/Any
        // arguments; keep that behavior (Issue #5922).
        "IOBuffer" => Some((
            name,
            FixedFallback::ConcreteDeferStructAny(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::IO,
            ))),
        )),
        // Int64-result helpers: hash, floored/ceiling division, and date/time
        // accessors keep their legacy unconditional Int64 result (Issue #5922).
        "hash" | "fld" | "cld" | "year" | "month" | "day" | "hour" | "minute" | "second"
        | "dayofweek" | "dayofyear" | "week" | "days" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )),
        "big" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
        )),
        // rand()/randn() resolve to Float64 through the registry; argument
        // forms fall back to the legacy unparameterized Array result, exactly
        // like the gate this entry replaced (Issue #5922).
        "rand" | "randn" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None,
            }),
        )),
        // trues/falses are total in the registry; the fallback is unreachable.
        "trues" | "falses" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::Struct {
                name: "BitVector".to_string(),
                type_id: 0,
            }),
        )),
        "typeof" | "promote_type" | "promote_rule" | "eltype" | "keytype" | "valtype" => Some((
            name,
            FixedFallback::Concrete(ConcreteType::DataType {
                name: "DataType".to_string(),
            }),
        )),
        _ => normalized_constructor_tfunc(name),
    }
}

/// Arity-gated rules for the JuliaType path.
///
/// `isequal(x)` (single arg) is the curried predicate form (a function), NOT a
/// Bool — only the 2-arg `isequal(x, y)` returns Bool. Without this gate
/// `filter(isequal(2), v)` would infer the predicate as Bool and fail to
/// dispatch (Issue #5662).
fn normalized_julia_arity_gated_tfunc(
    function: &str,
    argc: usize,
) -> Option<(&str, FixedFallback)> {
    let name = function.strip_prefix("Base.").unwrap_or(function);
    match name {
        "isequal" if argc == 2 => Some((name, FixedFallback::Bool)),
        _ => None,
    }
}

fn normalized_constructor_tfunc(name: &str) -> Option<(&str, FixedFallback)> {
    // Platform-native word-size aliases. `Int`/`UInt` are NOT fixed-width type
    // names, so they must resolve to the native signed/unsigned word type
    // (Issue #8198). Without this the ValueType inference of `Int(x)` fell to
    // `Any` — the JuliaType path special-cases `Int` separately, but the value
    // path drops straight here — so `Int[expr for ...]` produced a
    // `Vector{Any}` instead of `Vector{Int}` (the fixed-width `Int32[...]`
    // worked because `Int32` is in the table below). Mirrors the compile path's
    // `value_type_for_type_name` (compile/expr/builtin.rs).
    match name {
        "Int" => {
            let prim = if crate::types::native_int_type_name() == "Int32" {
                ("Int32", CorePrimitive::Int32)
            } else {
                ("Int64", CorePrimitive::Int64)
            };
            return Some((
                prim.0,
                FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(prim.1))),
            ));
        }
        "UInt" => {
            let prim = if crate::types::native_uint_type_name() == "UInt32" {
                ("UInt32", CorePrimitive::UInt32)
            } else {
                ("UInt64", CorePrimitive::UInt64)
            };
            return Some((
                prim.0,
                FixedFallback::Concrete(ConcreteType::Core(CoreType::Primitive(prim.1))),
            ));
        }
        _ => {}
    }
    let concrete = match name {
        "Int8" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
        "Int16" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
        "Int32" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        "Int64" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        "Int128" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),
        "UInt8" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        "UInt16" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
        "UInt32" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
        "UInt64" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        "UInt128" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)),
        "Float16" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)),
        "Float32" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        "Float64" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        "BigInt" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)),
        "BigFloat" => ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)),
        _ => return None,
    };
    Some((name, FixedFallback::Concrete(concrete)))
}

fn normalized_array_constructor_tfunc(function: &str) -> Option<&str> {
    let name = function.strip_prefix("Base.").unwrap_or(function);
    match name {
        "fill" | "zeros" | "ones" => Some(name),
        _ => None,
    }
}

fn normalized_type_object_tfunc(function: &str) -> Option<&str> {
    let name = function.strip_prefix("Base.").unwrap_or(function);
    match name {
        "typemin" | "typemax" => Some(name),
        _ => None,
    }
}

pub(super) fn is_array_constructor_call(function: &str) -> bool {
    normalized_array_constructor_tfunc(function).is_some()
}

pub(super) fn is_type_object_call(function: &str) -> bool {
    normalized_type_object_tfunc(function).is_some()
}

fn first_arg_tfunc_result(name: &str, argc: usize, first_arg: LatticeType) -> Option<LatticeType> {
    let rule = registry().rule(name)?;
    if !rule.accepts_arity(argc) {
        return None;
    }

    let mut args = Vec::with_capacity(argc);
    args.push(first_arg);
    args.resize(argc, LatticeType::Top);

    let result = (rule.eval)(&args);
    if matches!(result, LatticeType::Top) {
        None
    } else {
        Some(result)
    }
}

fn general_tfunc_result<I>(name: &str, args: I, argc: usize) -> Option<LatticeType>
where
    I: IntoIterator<Item = LatticeType>,
{
    let rule = registry().rule(name)?;
    if !rule.accepts_arity(argc) {
        return None;
    }

    let result = (rule.eval)(&args.into_iter().collect::<Vec<_>>());
    if matches!(result, LatticeType::Top) {
        None
    } else {
        Some(result)
    }
}

fn array_constructor_dims_args<'a>(
    name: &str,
    args: &'a [Expr],
    inferred_args: &[JuliaType],
) -> &'a [Expr] {
    match name {
        "fill" => args.get(1..).unwrap_or(&[]),
        "zeros" | "ones" => match inferred_args.first() {
            Some(JuliaType::TypeOf(_) | JuliaType::DataType | JuliaType::Type) => {
                args.get(1..).unwrap_or(&[])
            }
            _ => args,
        },
        _ => args,
    }
}

fn dims_rank_from_args(args: &[Expr]) -> Option<usize> {
    match args {
        [Expr::TupleLiteral { elements, .. }] => Some(elements.len()),
        [] => None,
        _ => Some(args.len()),
    }
}

fn array_element_concrete_from_lattice(result: &LatticeType) -> Option<&ConcreteType> {
    match result {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => Some(element),
        _ => None,
    }
}

fn value_array_element_from_concrete(
    element: &ConcreteType,
    struct_type_id: impl FnMut(&str) -> Option<usize>,
) -> Option<ArrayElementType> {
    match element {
        ConcreteType::Struct { type_id, .. } if *type_id != 0 => {
            Some(ArrayElementType::StructOf(*type_id))
        }
        _ => {
            let julia_element = julia_type_from_concrete_type(element);
            shared::array_element_type_for_julia_type(&julia_element, struct_type_id)
        }
    }
}

fn type_object_tfunc_result(name: &str, julia_args: &[JuliaType]) -> Option<LatticeType> {
    match julia_args {
        [JuliaType::TypeOf(_)] => general_tfunc_result(
            name,
            [julia_type_to_lattice(&julia_args[0])],
            julia_args.len(),
        ),
        _ => None,
    }
}

pub(super) fn infer_value_type_object_call(
    function: &str,
    julia_args: &[JuliaType],
) -> Option<ValueType> {
    let name = normalized_type_object_tfunc(function)?;
    let result = type_object_tfunc_result(name, julia_args)?;
    Some(ValueType::from(&result))
}

pub(super) fn infer_julia_type_object_call<F>(
    function: &str,
    args: &[Expr],
    mut infer_arg: F,
) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    let name = normalized_type_object_tfunc(function)?;
    let inferred_args = args.iter().map(&mut infer_arg).collect::<Vec<_>>();
    let result = type_object_tfunc_result(name, &inferred_args)?;
    lattice_to_julia_type(&result)
}

pub(super) fn infer_value_array_constructor_call<FS>(
    function: &str,
    args: &[Expr],
    value_args: &[ValueType],
    julia_args: &[JuliaType],
    struct_type_id: FS,
) -> Option<ValueType>
where
    FS: FnMut(&str) -> Option<usize>,
{
    let name = normalized_array_constructor_tfunc(function)?;
    if value_args.len() != args.len() || julia_args.len() != args.len() {
        return None;
    }
    let lattice_args = (0..args.len())
        .map(|idx| match name {
            "zeros" | "ones" if idx == 0 => julia_type_to_lattice(&julia_args[idx]),
            _ => LatticeType::from(&value_args[idx]),
        })
        .collect::<Vec<_>>();
    let result = general_tfunc_result(name, lattice_args, args.len())?;
    let element = array_element_concrete_from_lattice(&result)?;
    let element =
        value_array_element_from_concrete(element, struct_type_id).unwrap_or(ArrayElementType::Any);
    Some(ValueType::ArrayOf(element, None))
}

pub(super) fn infer_julia_array_constructor_call<F>(
    function: &str,
    args: &[Expr],
    mut infer_arg: F,
) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    let name = normalized_array_constructor_tfunc(function)?;
    let inferred_args = args.iter().map(&mut infer_arg).collect::<Vec<_>>();
    let result = general_tfunc_result(
        name,
        inferred_args.iter().map(julia_type_to_lattice),
        inferred_args.len(),
    )?;
    let element = array_element_concrete_from_lattice(&result)
        .map(julia_type_from_concrete_type)
        .unwrap_or(JuliaType::Any);
    let dims = array_constructor_dims_args(name, args, &inferred_args);
    Some(
        dims_rank_from_args(dims)
            .map(|rank| julia_array_type_for_ndims(element, rank))
            .unwrap_or(JuliaType::Array),
    )
}

/// How a leading argument to `rand` selects the array element type / dimensions.
enum RandLead {
    /// A leading `Float64` type object: `rand(Float64, dims...)` samples `Float64`.
    Float64TypeObject,
    /// A leading integer type object whose array runtime is not yet element-faithful
    /// (`rand(Int, n)` currently produces `Vector{Float64}` at runtime — a separate
    /// `RandIntArray` bug). Inference must DEFER to the legacy unparameterized
    /// `Array` so it never disagrees with the value the VM actually builds
    /// (Issue #7307).
    DeferTypeObject,
    /// No leading type object: the first argument is a dimension (or a
    /// collection/RNG handled by the integer-dimension check downstream).
    None,
}

/// Classify the leading argument of a `rand(...)` call.
///
/// The builtin emitter keys off the same syntactic `Expr::Var` names
/// (`builtin.rs::BuiltinOp::Rand`), so this classification must agree with the
/// instruction that is actually emitted (Issue #7307).
fn rand_lead_kind(arg: &Expr) -> RandLead {
    match arg {
        Expr::Var(name, _) if name == "Int" || name == "Int64" => RandLead::DeferTypeObject,
        Expr::Var(name, _) if name == "Float64" => RandLead::Float64TypeObject,
        _ => RandLead::None,
    }
}

/// Rank-aware result type for the array forms of `rand` / `randn` (Issue #7307).
///
/// `rand(n)` is `Vector{Float64}` and `rand(n, m)` is `Matrix{Float64}` upstream,
/// but the registry's `tfunc_rand` stays conservative (returns `Top` for any
/// argument) and the legacy adapter pinned the *unparameterized* `Array` (rank
/// unknown). An unranked `Array` does not match the static `scatter(y::Vector)` /
/// `plot(y::Vector)` methods in the bundled Plots package, so `scatter(rand(5))`
/// raised a spurious `MethodError`. Recovering the rank from the scalar dimension
/// arguments — exactly as `zeros`/`ones` do via `dims_rank_from_args` — both fixes
/// dispatch and matches upstream's parametric result.
///
/// The element type is always pinned to `Float64`: only the `Float64` forms
/// (`rand(n)`, `rand(Float64, n)`, all `randn`) are handled here, because their
/// inferred element type agrees with the value the VM actually builds. The
/// `rand(Int, n)` form is deliberately deferred — the `RandIntArray` runtime
/// currently produces a `Float64` array, so pinning `Vector{Int64}` would make
/// inference disagree with the runtime and break downstream indexing.
///
/// Returns `None` (defer to the legacy `Array` fallback) for forms whose shape we
/// cannot pin from the call alone: a leading integer type object (`rand(Int, n)`,
/// see above), an explicit-RNG first argument (`rand(rng, ...)`), or a
/// non-integer first argument such as a collection (`rand(itr)` /
/// `rand([1.0, 2.0])`, whose result is the element type, not an array).
fn infer_rand_array_julia_type<F>(function: &str, args: &[Expr], infer_arg: F) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    let name = function.strip_prefix("Base.").unwrap_or(function);
    if name != "rand" && name != "randn" {
        return None;
    }
    infer_rand_array_julia_type_for(name == "randn", args, infer_arg)
}

/// Core of [`infer_rand_array_julia_type`], shared with the `Expr::Builtin`
/// inference arm where `rand`/`randn` are represented as `BuiltinOp::Rand`/`Randn`
/// rather than `Expr::Call` (Issue #7307).
pub(in crate::compile) fn infer_rand_array_julia_type_for<F>(
    is_randn: bool,
    args: &[Expr],
    mut infer_arg: F,
) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    if args.is_empty() {
        // `rand()` / `randn()` are scalar Float64 (handled elsewhere).
        return None;
    }

    // `randn` always samples Float64; `rand` honours a leading type object.
    let dims: &[Expr] = if is_randn {
        args
    } else {
        match rand_lead_kind(&args[0]) {
            RandLead::DeferTypeObject => return None,
            RandLead::Float64TypeObject => &args[1..],
            RandLead::None => args,
        }
    };

    // `rand(Float64)` (type object only) samples a scalar, not an array — defer.
    let first = dims.first()?;

    // A leading non-integer argument means a collection/RNG form, not a
    // dimension list — defer to the legacy fallback.
    let first_ty = infer_arg(first);
    if !rand_dim_arg_is_integer(first, &first_ty) {
        return None;
    }

    let rank = dims_rank_from_args(dims)?;
    Some(julia_array_type_for_ndims(JuliaType::Float64, rank))
}

/// Whether a `rand`/`randn` argument is a scalar integer dimension (vs. a
/// collection or RNG). A tuple-literal dimension list (`rand((2, 3))`) is also a
/// dimension form.
fn rand_dim_arg_is_integer(arg: &Expr, inferred: &JuliaType) -> bool {
    if matches!(arg, Expr::TupleLiteral { .. }) {
        return true;
    }
    matches!(
        inferred,
        JuliaType::Int64
            | JuliaType::Int32
            | JuliaType::Int16
            | JuliaType::Int8
            | JuliaType::Int128
            | JuliaType::UInt64
            | JuliaType::UInt32
            | JuliaType::UInt16
            | JuliaType::UInt8
            | JuliaType::UInt128
    )
}

/// `ValueType` counterpart of [`infer_rand_array_julia_type`] (Issue #7307).
///
/// Mirrors the rank-aware decision but yields a `ValueType::ArrayOf(F64, rank)`
/// so the slot/value channel agrees with the JuliaType dispatch channel. The
/// element type is always `F64` for the handled forms; `rand(Int, n)` (and the
/// RNG/collection forms) defer for the same reason as the JuliaType helper.
fn infer_rand_array_value_type<F>(function: &str, args: &[Expr], infer_arg: F) -> Option<ValueType>
where
    F: FnMut(&Expr) -> ValueType,
{
    let name = function.strip_prefix("Base.").unwrap_or(function);
    if name != "rand" && name != "randn" {
        return None;
    }
    infer_rand_array_value_type_for(name == "randn", args, infer_arg)
}

/// Core of [`infer_rand_array_value_type`], shared with the `Expr::Builtin`
/// inference arm (`BuiltinOp::Rand`/`Randn`) — Issue #7307.
pub(in crate::compile) fn infer_rand_array_value_type_for<F>(
    is_randn: bool,
    args: &[Expr],
    mut infer_arg: F,
) -> Option<ValueType>
where
    F: FnMut(&Expr) -> ValueType,
{
    if args.is_empty() {
        return None;
    }

    let dims: &[Expr] = if is_randn {
        args
    } else {
        match rand_lead_kind(&args[0]) {
            RandLead::DeferTypeObject => return None,
            RandLead::Float64TypeObject => &args[1..],
            RandLead::None => args,
        }
    };

    let first = dims.first()?;
    if !rand_dim_arg_is_integer_value(first, &infer_arg(first)) {
        return None;
    }
    let rank = dims_rank_from_args(dims)?;
    Some(ValueType::ArrayOf(ArrayElementType::F64, Some(rank)))
}

/// `ValueType` flavour of [`rand_dim_arg_is_integer`].
fn rand_dim_arg_is_integer_value(arg: &Expr, inferred: &ValueType) -> bool {
    if matches!(arg, Expr::TupleLiteral { .. }) {
        return true;
    }
    matches!(
        inferred,
        ValueType::I64
            | ValueType::I32
            | ValueType::I16
            | ValueType::I8
            | ValueType::I128
            | ValueType::U64
            | ValueType::U32
            | ValueType::U16
            | ValueType::U8
            | ValueType::U128
    )
}

fn is_value_array_family(ty: &ValueType) -> bool {
    matches!(ty, ValueType::Array | ValueType::ArrayOf(_, _))
}

pub(super) fn infer_value_type_call<F>(
    function: &str,
    args: &[Expr],
    mut infer_arg: F,
) -> Option<ValueType>
where
    F: FnMut(&Expr) -> ValueType,
{
    // `rand(n)` / `randn(n, m)` recover a ranked `ArrayOf` result so the value
    // channel agrees with the JuliaType dispatch channel (Issue #7307).
    if let Some(inferred) = infer_rand_array_value_type(function, args, &mut infer_arg) {
        return Some(inferred);
    }

    if let Some(name) = normalized_first_arg_tfunc_name(function) {
        let Some(first_arg) = args.first() else {
            return legacy_value_type_fallback(name, None);
        };

        let first = infer_arg(first_arg);
        let first_lattice = LatticeType::from(&first);
        let Some(result) = first_arg_tfunc_result(name, args.len(), first_lattice.clone()) else {
            return legacy_value_type_fallback(name, Some(first));
        };

        if result == first_lattice && is_value_array_family(&first) {
            return Some(first);
        }

        return Some(ValueType::from(&result));
    }

    let (name, fallback) = normalized_value_general_tfunc(function)?;
    let inferred_args = args.iter().map(&mut infer_arg).collect::<Vec<_>>();
    if fixed_value_fallback_returns_any_for_struct_any(&fallback)
        && inferred_args
            .iter()
            .any(|ty| matches!(ty, ValueType::Struct(_) | ValueType::Any))
    {
        return Some(ValueType::Any);
    }

    let result = general_tfunc_result(
        name,
        inferred_args.iter().map(LatticeType::from),
        inferred_args.len(),
    );
    Some(result.map_or_else(
        || legacy_fixed_value_type_fallback(fallback),
        |result| ValueType::from(&result),
    ))
}

fn legacy_value_type_fallback(name: &str, first: Option<ValueType>) -> Option<ValueType> {
    match name {
        "replace" | "repeat" => Some(match first {
            Some(ty) if is_value_array_family(&ty) => ty,
            _ => ValueType::Str,
        }),
        "reverse" => Some(first.unwrap_or(ValueType::F64)),
        "abs" | "abs2" | "sign" => match first {
            Some(ValueType::BigInt) => Some(ValueType::BigInt),
            Some(ValueType::I128) => Some(ValueType::I128),
            Some(ValueType::I64) => Some(ValueType::I64),
            Some(ValueType::F32) => Some(ValueType::F32),
            Some(ValueType::F16) => Some(ValueType::F16),
            // User structs and runtime-unknown arguments must defer to later
            // rules / runtime dispatch: a user-extended method (e.g.
            // `abs(h::Holder)`) may return a non-numeric value, and assuming
            // `Float64` here made `abs(x) == "s"` constant-fold to `false`
            // at the String-vs-non-String equality shortcut (Issue #6539).
            // Complex magnitudes do not pass through this fallback — the
            // registry tfunc (`tfunc_abs`) already resolves named
            // `Complex{...}` lattice types to `Float64`.
            Some(ValueType::Struct(_) | ValueType::Any | ValueType::Union(_)) => None,
            _ => Some(ValueType::F64),
        },
        _ => None,
    }
}

fn legacy_fixed_value_type_fallback(fallback: FixedFallback) -> ValueType {
    match fallback {
        FixedFallback::String => ValueType::Str,
        FixedFallback::Bool => ValueType::Bool,
        FixedFallback::Concrete(concrete) | FixedFallback::ConcreteDeferStructAny(concrete) => {
            ValueType::from(&LatticeType::Concrete(concrete))
        }
    }
}

fn fixed_value_fallback_returns_any_for_struct_any(fallback: &FixedFallback) -> bool {
    matches!(fallback, FixedFallback::ConcreteDeferStructAny(_))
}

fn is_julia_array_family(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
    )
}

pub(super) fn infer_julia_type_call<F>(
    function: &str,
    args: &[Expr],
    mut infer_arg: F,
) -> Option<JuliaType>
where
    F: FnMut(&Expr) -> JuliaType,
{
    if let Some(inferred) = infer_julia_type_object_call(function, args, &mut infer_arg) {
        return Some(inferred);
    }

    // Bare-name only: the legacy gate matched `collect` (not `Base.collect`),
    // which keeps Call-position `Base.collect` on the method-dispatch path
    // (Issue #5922).
    if function == "collect" {
        return Some(infer_julia_collect_call(args, &mut infer_arg));
    }

    // `rand(n)` / `randn(n, m)` recover a rank-aware `Vector`/`Matrix` result so
    // a native-array carrier dispatches like a literal `Vector` (Issue #7307).
    // Forms we cannot pin (RNG/collection arguments) fall through to the legacy
    // unparameterized `Array` fallback below.
    if let Some(inferred) = infer_rand_array_julia_type(function, args, &mut infer_arg) {
        return Some(inferred);
    }

    if let Some(name) = normalized_first_arg_tfunc_name(function) {
        let first = args.first().map(&mut infer_arg)?;
        let first_lattice = julia_type_to_lattice(&first);
        let Some(result) = first_arg_tfunc_result(name, args.len(), first_lattice.clone()) else {
            return legacy_julia_type_fallback(name, first);
        };

        if result == first_lattice && is_julia_array_family(&first) {
            return Some(first);
        }

        return lattice_to_julia_type(&result);
    }

    let (name, fallback) = normalized_julia_arity_gated_tfunc(function, args.len())
        .or_else(|| normalized_julia_general_tfunc(function))?;
    let inferred_args = args.iter().map(&mut infer_arg).collect::<Vec<_>>();
    if fixed_julia_fallback_defers_struct_any(&fallback)
        && inferred_args
            .iter()
            .any(|ty| matches!(ty, JuliaType::Struct(_) | JuliaType::Any))
    {
        return None;
    }

    let result = general_tfunc_result(
        name,
        inferred_args.iter().map(julia_type_to_lattice),
        inferred_args.len(),
    );
    let fallback_on_top = fallback.clone();
    Some(result.map_or_else(
        || legacy_fixed_julia_type_fallback(fallback_on_top),
        |result| {
            lattice_to_julia_type(&result)
                .unwrap_or_else(|| legacy_fixed_julia_type_fallback(fallback))
        },
    ))
}

fn legacy_julia_type_fallback(name: &str, first: JuliaType) -> Option<JuliaType> {
    if matches!(first, JuliaType::Struct(_) | JuliaType::Any) {
        return None;
    }

    match name {
        "replace" | "repeat" => Some(if is_julia_array_family(&first) {
            first
        } else {
            JuliaType::String
        }),
        "reverse" => Some(first),
        "abs" | "abs2" | "sign" => first.is_builtin_numeric().then_some(first),
        _ => None,
    }
}

fn legacy_fixed_julia_type_fallback(fallback: FixedFallback) -> JuliaType {
    match fallback {
        FixedFallback::String => JuliaType::String,
        FixedFallback::Bool => JuliaType::Bool,
        FixedFallback::Concrete(concrete) | FixedFallback::ConcreteDeferStructAny(concrete) => {
            julia_type_from_concrete_type(&concrete)
        }
    }
}

fn fixed_julia_fallback_defers_struct_any(fallback: &FixedFallback) -> bool {
    matches!(
        fallback,
        FixedFallback::String | FixedFallback::Bool | FixedFallback::ConcreteDeferStructAny(_)
    )
}

/// `JuliaType → LatticeType` for tfunc arguments (Issues #5922, #5916).
///
/// The shared concrete mapping is delegated to the canonical
/// `bridge::julia_type_to_lattice` (the last hand-rolled copy of this edge was
/// deleted here in #5916/#5922 wave 5). The adapter keeps a small set of
/// **explicit pinned edges** that intentionally diverge from the canonical
/// converter because the legacy gates this adapter replaced depend on them:
///
/// - **Dispatch-deferral edges → `Top`**: `Struct(name)` (the table-free
///   canonical would produce a `Struct{type_id: 0}` placeholder, which would
///   make first-arg identity tfuncs fire for user structs instead of deferring
///   to method dispatch), `Signed`/`Unsigned` (canonical widens to the
///   `Integer` marker), and `Bottom`. `Bottom` is pinned independently of the
///   canonical converter's Bottom edge, which is in flux (Issue #6523) — this
///   adapter must keep deferring regardless of how that issue resolves.
/// - **Type objects**: `TypeOf(T) → DataType{name: T}` and
///   `DataType`/`Type → DataType{"DataType"}` (canonical: `Top`). The
///   `typemin`/`typemax`/`zeros`/`ones` rules read the element type out of the
///   `DataType` name, so this edge is load-bearing.
/// - **Legacy result pinnings** kept exactly as the deleted local copy mapped
///   them (canonical: `Top` unless noted): `AbstractString → String`,
///   `AbstractChar → Char`, `AbstractArray → Array{Any}`,
///   `NamedTuple → NamedTuple{}`,
///   `UnitRange`/`StepRange`/`AbstractRange → Range{Any}` (canonical pins
///   `Range{Int64}`; the `collect` seam pre-converts ranges itself),
///   `Module`/`Function`/`IO`/`IOBuffer`/metaprogramming nodes/`Pairs`/
///   `Generator{Any}`/`Enum{name}`. Each of these still changes at least one
///   routed adapter output relative to canonical `Top` (e.g. the
///   `min`/`max`/`reverse` identity tfuncs return the concrete pin), so all are
///   load-bearing — proven by the adapter-level
///   `pin_audit_load_bearing_arms_diverge_dead_arms_match` test (Issue #6600).
/// - `VectorOf`/`MatrixOf` recurse through this wrapper (projected to
///   `ConcreteType`) so element positions keep the same pinned edges
///   (`Vector{MyStruct}` stays `Array{Any}`, not `Array{Struct{…}}`).
///
/// **Pins removed (Issue #6600).** `TupleOf(_)` previously pinned `Tuple{}`
/// (element types dropped); it is now **delegated** to the canonical converter
/// (which keeps the structured `Tuple{…}` elements). This is behavior-neutral
/// at the adapter level: every routed julia-path entry point that reads a tuple
/// argument projects it back through `julia_type_from_concrete_type`, which
/// collapses any `Tuple{…}` (empty or not) to the bare `JuliaType::Tuple`, and
/// the only element-sensitive rule (`length` → `Const(Int64(n))`) is widened to
/// `JuliaType::Int64` by `lattice_to_julia_type` regardless of `n`. Removing the
/// pin shrinks the divergence surface (verified by the audit test).
///
/// `Union` is **delegated** to the canonical converter (empty → `Bottom`,
/// contains-`Any` → `Top`, otherwise a structural `Union`): the old local copy
/// collapsed every union to `Top`, which §3.6 of
/// `docs/vm/TYPE_REPRESENTATIONS.md` lists as the union-loss bug this
/// delegation fixes. Union arguments that reach identity tfuncs now defer to
/// method dispatch instead of falling into fixed-result fallbacks.
fn julia_type_to_lattice(ty: &JuliaType) -> LatticeType {
    // Test-only hook (Issue #6600 pin audit): when a `JuliaType` variant is
    // registered for delegation, route it straight to the canonical bridge so
    // the audit can measure the *adapter-level* effect of removing one pin at a
    // time. Zero cost in production (compiled out).
    #[cfg(test)]
    if tests::should_delegate_pin(ty) {
        return crate::compile::bridge::julia_type_to_lattice(ty);
    }
    match ty {
        // Dispatch-deferral edges (see doc comment above).
        JuliaType::Struct(_) | JuliaType::Signed | JuliaType::Unsigned | JuliaType::Bottom => {
            LatticeType::Top
        }
        // Type objects: load-bearing for typemin/typemax/zeros/ones.
        JuliaType::TypeOf(inner) => LatticeType::Concrete(ConcreteType::DataType {
            name: inner.name().to_string(),
        }),
        JuliaType::DataType | JuliaType::Type => LatticeType::Concrete(ConcreteType::DataType {
            name: "DataType".to_string(),
        }),
        // Legacy pinnings more precise than (or diverging from) the canonical.
        JuliaType::AbstractString => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String),
        )),
        JuliaType::AbstractChar => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }
        JuliaType::AbstractArray => LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
            ndims: None,
        }),
        JuliaType::VectorOf(element) | JuliaType::MatrixOf(element) => {
            let element = match julia_type_to_lattice(element) {
                LatticeType::Concrete(concrete) => concrete,
                _ => ConcreteType::Core(CoreType::Any),
            };
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(element),
                ndims: None,
            })
        }
        JuliaType::NamedTuple => {
            LatticeType::Concrete(ConcreteType::NamedTuple { fields: Vec::new() })
        }
        JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange => {
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
            })
        }
        JuliaType::Module => LatticeType::Concrete(ConcreteType::Module {
            name: "Module".to_string(),
        }),
        JuliaType::Function => LatticeType::Concrete(ConcreteType::Function {
            name: "Function".to_string(),
        }),
        JuliaType::IO | JuliaType::IOBuffer => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)))
        }
        JuliaType::Expr => LatticeType::Concrete(ConcreteType::Expr),
        JuliaType::QuoteNode => LatticeType::Concrete(ConcreteType::QuoteNode),
        JuliaType::LineNumberNode => LatticeType::Concrete(ConcreteType::LineNumberNode),
        JuliaType::GlobalRef => LatticeType::Concrete(ConcreteType::GlobalRef),
        JuliaType::Pairs => LatticeType::Concrete(ConcreteType::Pairs),
        JuliaType::Generator => LatticeType::Concrete(ConcreteType::Generator {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
        }),
        JuliaType::Enum(name) => LatticeType::Concrete(ConcreteType::Enum { name: name.clone() }),
        // Everything else (primitive numerics, String/Char, Array, bare Tuple,
        // Dict/Set, Nothing/Missing, abstract numeric markers, Symbol, Any,
        // Union, typevars, UnionAll) shares the canonical mapping.
        _ => crate::compile::bridge::julia_type_to_lattice(ty),
    }
}

fn lattice_to_julia_type(ty: &LatticeType) -> Option<JuliaType> {
    match ty {
        LatticeType::Concrete(concrete) => Some(julia_type_from_concrete_type(concrete)),
        LatticeType::Const(value) => Some(julia_type_from_concrete_type(&value.to_concrete_type())),
        _ => None,
    }
}

fn julia_type_from_concrete_type(ty: &ConcreteType) -> JuliaType {
    match ty {
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)) => JuliaType::Int8,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)) => JuliaType::Int16,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)) => JuliaType::Int32,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)) => JuliaType::Int64,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)) => JuliaType::Int128,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)) => JuliaType::BigInt,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)) => JuliaType::UInt8,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)) => JuliaType::UInt16,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)) => JuliaType::UInt32,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)) => JuliaType::UInt64,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)) => JuliaType::UInt128,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)) => JuliaType::Bool,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)) => JuliaType::Float16,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)) => JuliaType::Float32,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)) => JuliaType::Float64,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)) => JuliaType::BigFloat,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)) => JuliaType::String,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)) => JuliaType::Char,
        ConcreteType::Array { element, .. } => {
            if matches!(**element, ConcreteType::Core(CoreType::Any)) {
                JuliaType::Array
            } else {
                JuliaType::VectorOf(Box::new(julia_type_from_concrete_type(element)))
            }
        }
        ConcreteType::Tuple { .. } | ConcreteType::TupleVararg { .. } => JuliaType::Tuple,
        ConcreteType::NamedTuple { .. } => JuliaType::NamedTuple,
        ConcreteType::Dict { .. } => JuliaType::Dict,
        ConcreteType::Set { .. } => JuliaType::Set,
        ConcreteType::Range { .. } => JuliaType::AbstractRange,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)) => JuliaType::Nothing,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing)) => JuliaType::Missing,
        ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)) => JuliaType::Number,
        ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer)) => JuliaType::Integer,
        ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat)) => {
            JuliaType::AbstractFloat
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)) => JuliaType::Symbol,
        ConcreteType::Struct { name, .. } => JuliaType::Struct(name.clone()),
        ConcreteType::Module { .. } => JuliaType::Module,
        ConcreteType::DataType { .. } => JuliaType::DataType,
        ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)) => JuliaType::IO,
        ConcreteType::Function { .. }
        | ConcreteType::Closure { .. }
        | ConcreteType::ComposedFunction { .. } => JuliaType::Function,
        ConcreteType::Expr => JuliaType::Expr,
        ConcreteType::QuoteNode => JuliaType::QuoteNode,
        ConcreteType::LineNumberNode => JuliaType::LineNumberNode,
        ConcreteType::GlobalRef => JuliaType::GlobalRef,
        ConcreteType::Regex => JuliaType::Struct("Regex".to_string()),
        ConcreteType::RegexMatch => JuliaType::Struct("RegexMatch".to_string()),
        ConcreteType::Enum { name } => JuliaType::Enum(name.clone()),
        ConcreteType::UnionOf(_)
        | ConcreteType::Core(CoreType::Any)
        | ConcreteType::Generator { .. } => JuliaType::Any,
        ConcreteType::Pairs => JuliaType::Struct("Pairs".to_string()),

        // Core variants not yet folded to dedicated arms (Issue #6720,
        // Slice-2 step-1a) widen to Any.
        ConcreteType::Core(_) => JuliaType::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_type_for_array_element(element: ArrayElementType) -> ValueType {
        ValueType::ArrayOf(element, None)
    }

    fn lit() -> Expr {
        Expr::Literal(
            crate::ir::core::Literal::Nothing,
            crate::span::Span::new(0, 0, 1, 1, 1, 1),
        )
    }

    fn tuple_lit(len: usize) -> Expr {
        Expr::TupleLiteral {
            elements: (0..len).map(|_| lit()).collect(),
            span: crate::span::Span::new(0, 0, 1, 1, 1, 1),
        }
    }

    fn type_of(ty: JuliaType) -> JuliaType {
        JuliaType::TypeOf(Box::new(ty))
    }

    fn value_array_constructor(
        function: &str,
        args: &[Expr],
        value_args: &[ValueType],
        julia_args: &[JuliaType],
    ) -> Option<ValueType> {
        infer_value_array_constructor_call(function, args, value_args, julia_args, |_| None)
    }

    fn pair_lit() -> Expr {
        Expr::Pair {
            key: Box::new(lit()),
            value: Box::new(lit()),
            span: crate::span::Span::new(0, 0, 1, 1, 1, 1),
        }
    }

    // Issue #6619: public Dict construction reports the pure-Julia struct type.
    #[test]
    fn julia_dict_constructor_returns_struct_type_and_widens_unknown_iterables() {
        assert_eq!(
            infer_julia_dict_constructor_call("Dict", &[], &mut |_| JuliaType::Any),
            Some(JuliaType::Struct("Dict{Any,Any}".to_string()))
        );
        assert_eq!(
            infer_julia_dict_constructor_call("Dict", &[pair_lit(), pair_lit()], &mut |_| {
                JuliaType::Struct("Pair{String,Int64}".to_string())
            }),
            Some(JuliaType::Struct("Dict{String,Int64}".to_string()))
        );
        assert_eq!(
            infer_julia_dict_constructor_call("Dict{String, Int64}", &[], &mut |_| {
                JuliaType::Any
            }),
            Some(JuliaType::Struct("Dict{String,Int64}".to_string()))
        );
        // Unknown iterable element types widen; they must not become
        // `JuliaType::Dict` / legacy `Value::Dict`.
        assert_eq!(
            infer_julia_dict_constructor_call("Dict", &[lit()], &mut |_| JuliaType::Any),
            Some(JuliaType::Any)
        );
        // Non-Dict names never match.
        assert_eq!(
            infer_julia_dict_constructor_call("Set", &[], &mut |_| JuliaType::Any),
            None
        );
    }

    // Issue #5922: LinearAlgebra module-call result shapes route through the
    // registry's `LinearAlgebra.`-qualified rules.
    #[test]
    fn linear_algebra_module_call_shapes_match_legacy_gate() {
        assert_eq!(infer_value_linear_algebra_call("det"), Some(ValueType::F64));
        assert_eq!(
            infer_value_linear_algebra_call("cond"),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_linear_algebra_call("rank"),
            Some(ValueType::I64)
        );
        for f in ["svd", "qr", "eigen", "cholesky"] {
            assert_eq!(
                infer_value_linear_algebra_call(f),
                Some(ValueType::NamedTuple)
            );
        }
        assert_eq!(
            infer_value_linear_algebra_call("lu"),
            Some(ValueType::Tuple)
        );
        // Pinned: the legacy gate returned the unparameterized Array, not
        // ArrayOf(Any).
        for f in ["inv", "eigvals", "transpose"] {
            assert_eq!(infer_value_linear_algebra_call(f), Some(ValueType::Array));
        }
        // Unknown LinearAlgebra functions stay dynamically typed at the call
        // site (the adapter defers).
        assert_eq!(infer_value_linear_algebra_call("norm"), None);
    }

    // Issue #5922: bare `collect` keeps its legacy element pinning through
    // the registry's collect tfunc.
    #[test]
    fn julia_collect_pins_range_and_vector_element_types() {
        let args = vec![lit()];
        assert_eq!(
            infer_julia_type_call("collect", &args, |_| JuliaType::UnitRange),
            Some(JuliaType::VectorOf(Box::new(JuliaType::Int64)))
        );
        assert_eq!(
            infer_julia_type_call("collect", &args, |_| JuliaType::StepRange),
            Some(JuliaType::VectorOf(Box::new(JuliaType::Int64)))
        );
        assert_eq!(
            infer_julia_type_call("collect", &args, |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Float64))
            }),
            Some(JuliaType::VectorOf(Box::new(JuliaType::Float64)))
        );
        assert_eq!(
            infer_julia_type_call("collect", &args, |_| JuliaType::Any),
            Some(JuliaType::Array)
        );
        assert_eq!(
            infer_julia_type_call("collect", &[], |_| JuliaType::Any),
            Some(JuliaType::Array)
        );
        // `collect(T, itr)` keys off the first argument, like the legacy gate.
        assert_eq!(
            infer_julia_type_call("collect", &[lit(), lit()], |_| JuliaType::UnitRange),
            Some(JuliaType::VectorOf(Box::new(JuliaType::Int64)))
        );
        // Bare-name only: Base.collect stays on the method-dispatch path.
        assert_eq!(
            infer_julia_type_call("Base.collect", &args, |_| JuliaType::UnitRange),
            None
        );
    }

    // Issue #5922/#7307: bare rand()/randn() resolve to scalar Float64; the
    // scalar-dimension array forms recover a rank-aware Vector/Matrix so a
    // native-array carrier dispatches like a literal Vector.
    #[test]
    fn julia_rand_randn_keep_arity_split() {
        assert_eq!(
            infer_julia_type_call("rand", &[], |_| JuliaType::Any),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("randn", &[], |_| JuliaType::Any),
            Some(JuliaType::Float64)
        );
        // `rand(n)` / `randn(n, m)` with integer dimensions are ranked
        // `Vector`/`Matrix{Float64}` (Issue #7307).
        assert_eq!(
            infer_julia_type_call("rand", &[lit()], |_| JuliaType::Int64),
            Some(JuliaType::VectorOf(Box::new(JuliaType::Float64)))
        );
        assert_eq!(
            infer_julia_type_call("randn", &[lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::MatrixOf(Box::new(JuliaType::Float64)))
        );
        // `rand(Float64, n)` strips the type object then ranks the dims.
        let float64_var = Expr::Var(
            "Float64".to_string(),
            crate::span::Span::new(0, 0, 1, 1, 1, 1),
        );
        assert_eq!(
            infer_julia_type_call("rand", &[float64_var, lit()], |arg| {
                if matches!(arg, Expr::Var(n, _) if n == "Float64") {
                    JuliaType::DataType
                } else {
                    JuliaType::Int64
                }
            }),
            Some(JuliaType::VectorOf(Box::new(JuliaType::Float64)))
        );
        // `rand(Int, n)` is deferred (the RandIntArray runtime is not yet
        // element-faithful), so inference keeps the legacy unparameterized Array.
        let int_var = Expr::Var("Int".to_string(), crate::span::Span::new(0, 0, 1, 1, 1, 1));
        assert_eq!(
            infer_julia_type_call("rand", &[int_var, lit()], |arg| {
                if matches!(arg, Expr::Var(n, _) if n == "Int") {
                    JuliaType::DataType
                } else {
                    JuliaType::Int64
                }
            }),
            Some(JuliaType::Array)
        );
        // A non-integer first argument (collection/RNG) defers to the legacy
        // Array fallback.
        assert_eq!(
            infer_julia_type_call("rand", &[lit()], |_| JuliaType::Array),
            Some(JuliaType::Array)
        );
    }

    // Issue #7307: the ValueType channel mirrors the JuliaType ranking so the
    // slot/value type agrees with dispatch.
    #[test]
    fn value_rand_randn_rank_aware() {
        assert_eq!(
            infer_value_type_call("rand", &[lit()], |_| ValueType::I64),
            Some(ValueType::ArrayOf(ArrayElementType::F64, Some(1)))
        );
        assert_eq!(
            infer_value_type_call("randn", &[lit(), lit()], |_| ValueType::I64),
            Some(ValueType::ArrayOf(ArrayElementType::F64, Some(2)))
        );
        // Collection/RNG arg defers: `rand`/`randn` are not in the ValueType
        // general-tfunc table, so the call-level adapter returns `None` and the
        // `Expr::Builtin` inference arm (mod.rs) supplies the legacy `Array`.
        assert_eq!(
            infer_value_type_call("rand", &[lit()], |_| ValueType::Array),
            None
        );
    }

    #[test]
    fn value_type_adapter_preserves_array_subjects() {
        let args = vec![lit(), lit()];
        let inferred = infer_value_type_call("replace", &args, |_| {
            value_type_for_array_element(ArrayElementType::I64)
        });
        assert_eq!(
            inferred,
            Some(ValueType::ArrayOf(ArrayElementType::I64, None))
        );
    }

    #[test]
    fn julia_type_adapter_preserves_matrix_subjects() {
        let args = vec![lit()];
        let matrix = JuliaType::MatrixOf(Box::new(JuliaType::String));
        let inferred = infer_julia_type_call("Base.reverse", &args, |_| matrix.clone());
        assert_eq!(inferred, Some(matrix));
    }

    #[test]
    fn repeat_string_subject_returns_string_in_both_representations() {
        let args = vec![lit()];
        assert_eq!(
            infer_value_type_call("repeat", &args, |_| ValueType::Str),
            Some(ValueType::Str)
        );
        assert_eq!(
            infer_julia_type_call("repeat", &args, |_| JuliaType::String),
            Some(JuliaType::String)
        );
    }

    #[test]
    fn value_type_adapter_keeps_legacy_top_fallbacks() {
        let args = vec![lit()];
        assert_eq!(
            infer_value_type_call("replace", &args, |_| ValueType::Any),
            Some(ValueType::Str)
        );
        assert_eq!(
            infer_value_type_call("reverse", &args, |_| ValueType::Any),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("repeat", &[], |_| ValueType::Any),
            Some(ValueType::Str)
        );
    }

    #[test]
    fn julia_type_adapter_keeps_dispatch_deferred_for_struct_and_any() {
        let args = vec![lit()];
        assert_eq!(
            infer_julia_type_call("replace", &args, |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("reverse", &args, |_| { JuliaType::Struct("S".to_string()) }),
            None
        );
    }

    #[test]
    fn first_arg_tfuncs_cover_abs_abs2_sign_widths_and_fallbacks() {
        let args = vec![lit()];
        assert_eq!(
            infer_value_type_call("sign", &args, |_| ValueType::U8),
            Some(ValueType::U8)
        );
        assert_eq!(
            infer_value_type_call("Base.abs2", &args, |_| ValueType::F32),
            Some(ValueType::F32)
        );
        // User structs and runtime-unknown args defer to later rules /
        // runtime dispatch, matching the JuliaType channel below: a
        // user-extended `abs` may return a non-numeric value (Issue #6539).
        assert_eq!(
            infer_value_type_call("abs", &args, |_| ValueType::Struct(1)),
            None
        );
        assert_eq!(
            infer_value_type_call("abs", &args, |_| ValueType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("sign", &args, |_| JuliaType::UInt8),
            Some(JuliaType::UInt8)
        );
        assert_eq!(
            infer_julia_type_call("Base.abs2", &args, |_| JuliaType::Float32),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("abs", &args, |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("sign", &args, |_| { JuliaType::Struct("S".to_string()) }),
            None
        );
    }

    #[test]
    fn general_value_tfuncs_cover_string_return_gates() {
        let args = vec![lit()];
        assert_eq!(
            infer_value_type_call("string", &[], |_| ValueType::Any),
            Some(ValueType::Str)
        );
        assert_eq!(
            infer_value_type_call("uppercase", &args, |_| ValueType::Char),
            Some(ValueType::Char)
        );
        assert_eq!(
            infer_value_type_call("join", &[lit(), lit(), lit()], |_| ValueType::Array),
            Some(ValueType::Str)
        );
        assert_eq!(
            infer_value_type_call("Base.strip", &args, |_| ValueType::Str),
            Some(ValueType::Str)
        );
        assert_eq!(
            infer_value_type_call("sprintf", &args, |_| ValueType::Any),
            Some(ValueType::Str)
        );
        assert_eq!(
            infer_value_type_call("bitstring", &args, |_| ValueType::Struct(7)),
            Some(ValueType::Str)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_isa_haskey_bool_gates() {
        assert_eq!(
            infer_value_type_call("isa", &[lit(), lit()], |_| ValueType::I64),
            Some(ValueType::Bool)
        );
        // `haskey` over a concrete Dict receiver still infers Bool.
        assert_eq!(
            infer_value_type_call("Base.haskey", &[lit(), lit()], |_| ValueType::Dict),
            Some(ValueType::Bool)
        );
        // `haskey` over an Any-typed receiver defers to runtime dispatch so a
        // user override returning a non-Bool value is not coerced (Issue #6610).
        assert_eq!(
            infer_value_type_call("Base.haskey", &[lit(), lit()], |_| ValueType::Any),
            Some(ValueType::Any)
        );
        // Arity-mismatched / argless calls keep the Bool fallback.
        assert_eq!(
            infer_value_type_call("haskey", &[], |_| ValueType::Any),
            Some(ValueType::Bool)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_predicate_bool_gates() {
        assert_eq!(
            infer_value_type_call("isnan", &[lit()], |_| ValueType::F64),
            Some(ValueType::Bool)
        );
        assert_eq!(
            infer_value_type_call("Base.isless", &[lit(), lit()], |_| ValueType::I64),
            Some(ValueType::Bool)
        );
        assert_eq!(
            infer_value_type_call("isnothing", &[lit()], |_| ValueType::Any),
            Some(ValueType::Bool)
        );
        assert_eq!(
            infer_value_type_call("iseven", &[], |_| ValueType::Any),
            Some(ValueType::Bool)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_length_gate_with_int64_fallback() {
        assert_eq!(
            infer_value_type_call("length", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::I64, None)
            }),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("Base.length", &[lit()], |_| ValueType::Any),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("length", &[], |_| ValueType::Any),
            Some(ValueType::I64)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_size_gate_with_tuple_result_and_int64_fallback() {
        assert_eq!(
            infer_value_type_call("size", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::I64, None)
            }),
            Some(ValueType::Tuple)
        );

        let mut idx = 0;
        assert_eq!(
            infer_value_type_call("Base.size", &[lit(), lit()], |_| {
                let ty = if idx == 0 {
                    ValueType::ArrayOf(ArrayElementType::I64, None)
                } else {
                    ValueType::I64
                };
                idx += 1;
                ty
            }),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("size", &[lit()], |_| ValueType::Any),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("size", &[], |_| ValueType::Any),
            Some(ValueType::I64)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_int64_result_gates() {
        assert_eq!(
            infer_value_type_call("ndims", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::I64, None)
            }),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("Base.count", &[lit(), lit()], |_| ValueType::Any),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("count", &[], |_| ValueType::Any),
            Some(ValueType::I64)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_prod_gate_with_array_element_types() {
        assert_eq!(
            infer_value_type_call("prod", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::I64, None)
            }),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("Base.prod", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::F32, None)
            }),
            Some(ValueType::F32)
        );
        assert_eq!(
            infer_value_type_call("prod", &[lit()], |_| ValueType::Any),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("prod", &[], |_| ValueType::Any),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_statistics_float64_gates_with_struct_any_defer() {
        assert_eq!(
            infer_value_type_call("mean", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::I64, None)
            }),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_type_call("Base.std", &[lit()], |_| {
                ValueType::ArrayOf(ArrayElementType::F32, None)
            }),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_type_call("var", &[lit()], |_| ValueType::Struct(7)),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("mean", &[], |_| ValueType::Any),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_rounding_gates_with_struct_any_defer() {
        assert_eq!(
            infer_value_type_call("floor", &[lit()], |_| ValueType::I64),
            Some(ValueType::I64)
        );
        assert_eq!(
            infer_value_type_call("ceil", &[lit()], |_| ValueType::F32),
            Some(ValueType::F32)
        );
        assert_eq!(
            infer_value_type_call("round", &[lit()], |_| ValueType::BigInt),
            Some(ValueType::BigInt)
        );
        assert_eq!(
            infer_value_type_call("trunc", &[lit()], |_| ValueType::F16),
            Some(ValueType::F16)
        );
        assert_eq!(
            infer_value_type_call("round", &[], |_| ValueType::Any),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_type_call("floor", &[lit()], |_| ValueType::Any),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("ceil", &[lit()], |_| ValueType::Struct(7)),
            Some(ValueType::Any)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_unary_math_float_widths_with_struct_any_defer() {
        assert_eq!(
            infer_value_type_call("sqrt", &[lit()], |_| ValueType::I64),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_type_call("sin", &[lit()], |_| ValueType::F32),
            Some(ValueType::F32)
        );
        assert_eq!(
            infer_value_type_call("Base.log", &[lit()], |_| ValueType::F16),
            Some(ValueType::F16)
        );
        assert_eq!(
            infer_value_type_call("exp", &[lit()], |_| ValueType::Struct(7)),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("tan", &[lit()], |_| ValueType::F32),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_type_call("Base.log2", &[lit()], |_| ValueType::I64),
            Some(ValueType::F64)
        );
        assert_eq!(
            infer_value_type_call("expm1", &[lit()], |_| ValueType::Struct(7)),
            Some(ValueType::Any)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_signbit_bool_with_struct_any_defer() {
        assert_eq!(
            infer_value_type_call("signbit", &[lit()], |_| ValueType::I8),
            Some(ValueType::Bool)
        );
        assert_eq!(
            infer_value_type_call("Base.signbit", &[lit()], |_| ValueType::F32),
            Some(ValueType::Bool)
        );
        assert_eq!(
            infer_value_type_call("signbit", &[lit()], |_| ValueType::Struct(7)),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("signbit", &[], |_| ValueType::Any),
            Some(ValueType::Bool)
        );
    }

    #[test]
    fn general_value_tfuncs_cover_min_max_promotion_with_struct_any_defer() {
        assert_eq!(
            infer_value_type_call("min", &[lit(), lit()], |_| ValueType::I8),
            Some(ValueType::I8)
        );

        let mut idx = 0;
        assert_eq!(
            infer_value_type_call("Base.max", &[lit(), lit()], |_| {
                let ty = if idx == 0 {
                    ValueType::I8
                } else {
                    ValueType::I16
                };
                idx += 1;
                ty
            }),
            Some(ValueType::I16)
        );
        assert_eq!(
            infer_value_type_call("max", &[lit(), lit()], |_| ValueType::F32),
            Some(ValueType::F32)
        );
        assert_eq!(
            infer_value_type_call("min", &[lit(), lit()], |_| ValueType::Struct(7)),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_type_call("max", &[], |_| ValueType::Any),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_string_and_bool_gates() {
        assert_eq!(
            infer_julia_type_call("lowercase", &[lit()], |_| JuliaType::Char),
            Some(JuliaType::Char)
        );
        assert_eq!(
            infer_julia_type_call("contains", &[lit(), lit()], |_| JuliaType::String),
            Some(JuliaType::Bool)
        );
        let mut idx = 0;
        assert_eq!(
            infer_julia_type_call("occursin", &[lit(), lit()], |_| {
                let ty = if idx == 0 {
                    JuliaType::Char
                } else {
                    JuliaType::String
                };
                idx += 1;
                ty
            }),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("Base.strip", &[lit()], |_| JuliaType::String),
            Some(JuliaType::String)
        );
        assert_eq!(
            infer_julia_type_call("sprint", &[lit(), lit()], |_| JuliaType::Function),
            Some(JuliaType::String)
        );
        assert_eq!(
            infer_julia_type_call("bitstring", &[lit()], |_| {
                JuliaType::Struct("S".to_string())
            }),
            None
        );
        assert_eq!(
            infer_julia_type_call("sprintf", &[lit()], |_| JuliaType::String),
            None
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_isa_haskey_bool_gates_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("isa", &[lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("Base.haskey", &[lit(), lit()], |_| JuliaType::Dict),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("haskey", &[lit(), lit()], |_| JuliaType::Struct(
                "D".to_string()
            )),
            None
        );
        assert_eq!(
            infer_julia_type_call("isa", &[], |_| JuliaType::Any),
            Some(JuliaType::Bool)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_predicate_bool_gates_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("isnan", &[lit()], |_| JuliaType::Float64),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("Base.isless", &[lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("isnothing", &[lit()], |_| JuliaType::Nothing),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("ismissing", &[lit()], |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("iseven", &[], |_| JuliaType::Any),
            Some(JuliaType::Bool)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_length_gate_with_int64_fallback() {
        assert_eq!(
            infer_julia_type_call("length", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Int64))
            }),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("Base.length", &[lit()], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("length", &[], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_size_gate_with_tuple_result_and_int64_fallback() {
        assert_eq!(
            infer_julia_type_call("size", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Int64))
            }),
            Some(JuliaType::Tuple)
        );

        let mut idx = 0;
        assert_eq!(
            infer_julia_type_call("Base.size", &[lit(), lit()], |_| {
                let ty = if idx == 0 {
                    JuliaType::VectorOf(Box::new(JuliaType::Int64))
                } else {
                    JuliaType::Int64
                };
                idx += 1;
                ty
            }),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("size", &[lit()], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("size", &[], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_int64_result_gates() {
        assert_eq!(
            infer_julia_type_call("ndims", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Int64))
            }),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("Base.count", &[lit(), lit()], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("count", &[], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_prod_gate_with_array_element_types() {
        assert_eq!(
            infer_julia_type_call("prod", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Int64))
            }),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("Base.prod", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Float32))
            }),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("prod", &[lit()], |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("prod", &[], |_| JuliaType::Any),
            Some(JuliaType::Float64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_statistics_float64_gates_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("mean", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Int64))
            }),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("Base.std", &[lit()], |_| {
                JuliaType::VectorOf(Box::new(JuliaType::Float32))
            }),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("var", &[lit()], |_| JuliaType::Struct("S".to_string())),
            None
        );
        assert_eq!(
            infer_julia_type_call("mean", &[], |_| JuliaType::Any),
            Some(JuliaType::Float64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_rounding_gates_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("floor", &[lit()], |_| JuliaType::Int64),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("ceil", &[lit()], |_| JuliaType::Float32),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("round", &[lit()], |_| JuliaType::BigInt),
            Some(JuliaType::BigInt)
        );
        assert_eq!(
            infer_julia_type_call("trunc", &[lit()], |_| JuliaType::Float16),
            Some(JuliaType::Float16)
        );
        assert_eq!(
            infer_julia_type_call("round", &[], |_| JuliaType::Any),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("floor", &[lit()], |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("ceil", &[lit()], |_| JuliaType::Struct("S".to_string())),
            None
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_min_max_promotion_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("min", &[lit(), lit()], |_| JuliaType::Int8),
            Some(JuliaType::Int8)
        );

        let mut idx = 0;
        assert_eq!(
            infer_julia_type_call("Base.max", &[lit(), lit()], |_| {
                let ty = if idx == 0 {
                    JuliaType::Int8
                } else {
                    JuliaType::Int16
                };
                idx += 1;
                ty
            }),
            Some(JuliaType::Int16)
        );
        assert_eq!(
            infer_julia_type_call("max", &[lit(), lit()], |_| JuliaType::Float32),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("min", &[lit(), lit()], |_| JuliaType::Struct(
                "S".to_string()
            )),
            None
        );
        assert_eq!(
            infer_julia_type_call("max", &[], |_| JuliaType::Any),
            Some(JuliaType::Float64)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_signbit_bool_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("signbit", &[lit()], |_| JuliaType::Int8),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("Base.signbit", &[lit()], |_| JuliaType::Float32),
            Some(JuliaType::Bool)
        );
        assert_eq!(
            infer_julia_type_call("signbit", &[lit()], |_| JuliaType::Struct("S".to_string())),
            None
        );
        assert_eq!(
            infer_julia_type_call("signbit", &[], |_| JuliaType::Any),
            Some(JuliaType::Bool)
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_unary_math_float_widths_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("sqrt", &[lit()], |_| JuliaType::Int64),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("cos", &[lit()], |_| JuliaType::Float32),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("Base.log", &[lit()], |_| JuliaType::Float16),
            Some(JuliaType::Float16)
        );
        assert_eq!(
            infer_julia_type_call("exp", &[lit()], |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("sin", &[lit()], |_| { JuliaType::Struct("S".to_string()) }),
            None
        );
        assert_eq!(
            infer_julia_type_call("tan", &[lit()], |_| JuliaType::Float32),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("Base.log10", &[lit()], |_| JuliaType::Int64),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            infer_julia_type_call("expm1", &[lit()], |_| JuliaType::Any),
            None
        );
    }

    #[test]
    fn general_julia_tfuncs_cover_div_rem_mod_widths_with_struct_any_defer() {
        assert_eq!(
            infer_julia_type_call("div", &[lit(), lit()], |_| JuliaType::UInt8),
            Some(JuliaType::UInt8)
        );
        assert_eq!(
            infer_julia_type_call("rem", &[lit(), lit()], |_| JuliaType::Int128),
            Some(JuliaType::Int128)
        );
        assert_eq!(
            infer_julia_type_call("Base.mod", &[lit(), lit()], |_| JuliaType::Float32),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("div", &[lit()], |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("rem", &[lit(), lit()], |_| {
                JuliaType::Struct("S".to_string())
            }),
            None
        );
    }

    #[test]
    fn general_julia_tfuncs_keep_struct_dispatch_deferred() {
        assert_eq!(
            infer_julia_type_call("startswith", &[lit(), lit()], |_| {
                JuliaType::Struct("Regex".to_string())
            }),
            None
        );
    }

    #[test]
    fn constructor_tfuncs_cover_value_type_gates() {
        assert_eq!(
            infer_value_type_call("Float16", &[lit()], |_| ValueType::F64),
            Some(ValueType::F16)
        );
        assert_eq!(
            infer_value_type_call("BigFloat", &[], |_| ValueType::Any),
            Some(ValueType::BigFloat)
        );
    }

    #[test]
    fn constructor_tfuncs_cover_julia_type_gates_without_struct_defer() {
        assert_eq!(
            infer_julia_type_call("Int", &[lit()], |_| JuliaType::Any),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("UInt32", &[lit()], |_| {
                JuliaType::Struct("S".to_string())
            }),
            Some(JuliaType::UInt32)
        );
        assert_eq!(
            infer_julia_type_call("BigInt", &[], |_| JuliaType::Any),
            Some(JuliaType::BigInt)
        );
    }

    #[test]
    fn type_object_tfuncs_cover_typemin_typemax_value_type_gates() {
        assert_eq!(
            infer_value_type_object_call("typemin", &[type_of(JuliaType::UInt8)]),
            Some(ValueType::U8)
        );
        assert_eq!(
            infer_value_type_object_call("Base.typemax", &[type_of(JuliaType::Bool)]),
            Some(ValueType::Bool)
        );
        assert_eq!(
            infer_value_type_object_call("typemax", &[JuliaType::Int64]),
            None
        );
    }

    #[test]
    fn type_object_tfuncs_cover_typemin_typemax_julia_type_gates() {
        assert_eq!(
            infer_julia_type_call("typemin", &[lit()], |_| type_of(JuliaType::Float32)),
            Some(JuliaType::Float32)
        );
        assert_eq!(
            infer_julia_type_call("Base.typemax", &[lit()], |_| type_of(JuliaType::Int128)),
            Some(JuliaType::Int128)
        );
        assert_eq!(
            infer_julia_type_call("typemin", &[lit()], |_| JuliaType::Float32),
            None
        );
    }

    #[test]
    fn array_constructor_tfuncs_cover_value_type_zeros_and_ones() {
        let args = vec![lit(), lit()];
        assert_eq!(
            value_array_constructor(
                "zeros",
                &args,
                &[ValueType::DataType, ValueType::I64],
                &[type_of(JuliaType::Int16), JuliaType::Int64],
            ),
            Some(ValueType::ArrayOf(ArrayElementType::I16, None))
        );

        assert_eq!(
            value_array_constructor(
                "Base.ones",
                &[lit()],
                &[ValueType::I64],
                &[JuliaType::Int64],
            ),
            Some(ValueType::ArrayOf(ArrayElementType::F64, None))
        );
    }

    #[test]
    fn array_constructor_tfuncs_cover_value_type_fill_and_complex_type_objects() {
        assert_eq!(
            value_array_constructor(
                "fill",
                &[lit(), lit()],
                &[ValueType::F32, ValueType::I64],
                &[JuliaType::Float32, JuliaType::Int64],
            ),
            Some(ValueType::ArrayOf(ArrayElementType::F32, None))
        );

        let args = vec![lit(), lit()];
        assert_eq!(
            value_array_constructor(
                "zeros",
                &args,
                &[ValueType::DataType, ValueType::I64],
                &[
                    type_of(JuliaType::Struct("Complex{Float64}".to_string())),
                    JuliaType::Int64,
                ],
            ),
            Some(ValueType::ArrayOf(ArrayElementType::ComplexF64, None))
        );
    }

    #[test]
    fn array_constructor_tfuncs_cover_julia_type_rank_and_element_type() {
        let args = vec![lit(), lit(), lit()];
        let mut idx = 0;
        assert_eq!(
            infer_julia_array_constructor_call("zeros", &args, |_| {
                let ty = if idx == 0 {
                    type_of(JuliaType::UInt8)
                } else {
                    JuliaType::Int64
                };
                idx += 1;
                ty
            }),
            Some(JuliaType::MatrixOf(Box::new(JuliaType::UInt8)))
        );

        assert_eq!(
            infer_julia_array_constructor_call("zeros", &[tuple_lit(2)], |_| JuliaType::Tuple),
            Some(JuliaType::MatrixOf(Box::new(JuliaType::Float64)))
        );
        assert_eq!(
            infer_julia_array_constructor_call("fill", &[lit(), tuple_lit(2)], |_| {
                JuliaType::Float32
            }),
            Some(JuliaType::MatrixOf(Box::new(JuliaType::Float32)))
        );
        assert_eq!(
            infer_julia_array_constructor_call("Base.ones", &[lit(), lit()], |_| {
                JuliaType::Int64
            }),
            Some(JuliaType::MatrixOf(Box::new(JuliaType::Float64)))
        );
    }

    #[test]
    fn value_type_adapter_routes_gcd_lcm_with_bigint_preservation() {
        let args = vec![lit(), lit()];
        assert_eq!(
            infer_value_type_call("gcd", &args, |_| ValueType::BigInt),
            Some(ValueType::BigInt)
        );
        assert_eq!(
            infer_value_type_call("lcm", &args, |_| ValueType::I64),
            Some(ValueType::I64)
        );
        // Unknown argument types keep the legacy Int64 default.
        assert_eq!(
            infer_value_type_call("gcd", &args, |_| ValueType::Any),
            Some(ValueType::I64)
        );
    }

    #[test]
    fn value_type_adapter_routes_big_iobuffer_and_datatype_helpers() {
        assert_eq!(
            infer_value_type_call("big", &[lit()], |_| ValueType::F64),
            Some(ValueType::BigFloat)
        );
        assert_eq!(
            infer_value_type_call("big", &[lit()], |_| ValueType::I64),
            Some(ValueType::BigInt)
        );
        // Legacy default: unknown argument widens to BigInt.
        assert_eq!(
            infer_value_type_call("big", &[lit()], |_| ValueType::Any),
            Some(ValueType::BigInt)
        );
        assert_eq!(
            infer_value_type_call("IOBuffer", &[], |_| ValueType::Any),
            Some(ValueType::IO)
        );
        for helper in [
            "typeof",
            "promote_type",
            "promote_rule",
            "eltype",
            "keytype",
            "valtype",
        ] {
            assert_eq!(
                infer_value_type_call(helper, &[lit()], |_| ValueType::I64),
                Some(ValueType::DataType),
                "{helper} must infer DataType"
            );
        }
    }

    #[test]
    fn julia_type_adapter_gates_isequal_on_arity_and_dispatch() {
        // 2-arg isequal over primitives infers Bool.
        assert_eq!(
            infer_julia_type_call("isequal", &[lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::Bool)
        );
        // 1-arg curried form must stay uninferred (Issue #5662).
        assert_eq!(
            infer_julia_type_call("isequal", &[lit()], |_| JuliaType::Int64),
            None
        );
        // Struct arguments defer to method dispatch.
        assert_eq!(
            infer_julia_type_call("isequal", &[lit(), lit()], |_| {
                JuliaType::Struct("S".to_string())
            }),
            None
        );
    }

    #[test]
    fn julia_type_adapter_routes_int64_result_helpers() {
        assert_eq!(
            infer_julia_type_call("hash", &[lit()], |_| JuliaType::Int64),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("fld", &[lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            infer_julia_type_call("cld", &[lit(), lit()], |_| JuliaType::Float64),
            Some(JuliaType::Int64)
        );
        // Legacy gate was unconditional: struct args still infer Int64.
        assert_eq!(
            infer_julia_type_call("year", &[lit()], |_| JuliaType::Struct("Date".to_string())),
            Some(JuliaType::Int64)
        );
    }

    #[test]
    fn julia_type_adapter_routes_big_iobuffer_trues_and_datatype_helpers() {
        assert_eq!(
            infer_julia_type_call("big", &[lit()], |_| JuliaType::Float64),
            Some(JuliaType::BigFloat)
        );
        assert_eq!(
            infer_julia_type_call("big", &[lit()], |_| JuliaType::Struct("S".to_string())),
            Some(JuliaType::BigInt)
        );
        assert_eq!(
            infer_julia_type_call("IOBuffer", &[], |_| JuliaType::Any),
            Some(JuliaType::IO)
        );
        // Legacy gate deferred IOBuffer for Struct/Any arguments.
        assert_eq!(
            infer_julia_type_call("IOBuffer", &[lit()], |_| JuliaType::Any),
            None
        );
        assert_eq!(
            infer_julia_type_call("trues", &[lit()], |_| JuliaType::Int64),
            Some(JuliaType::Struct("BitVector".to_string()))
        );
        assert_eq!(
            infer_julia_type_call("falses", &[lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::Struct("BitMatrix".to_string()))
        );
        assert_eq!(
            infer_julia_type_call("trues", &[lit(), lit(), lit()], |_| JuliaType::Int64),
            Some(JuliaType::Struct("BitArray{3}".to_string()))
        );
        for helper in [
            "typeof",
            "promote_type",
            "promote_rule",
            "eltype",
            "keytype",
            "valtype",
        ] {
            assert_eq!(
                infer_julia_type_call(helper, &[lit()], |_| JuliaType::Int64),
                Some(JuliaType::DataType),
                "{helper} must infer DataType"
            );
        }
    }

    /// Stub struct-identity lookup for the constructor adapters (Issue #5922).
    struct StubIds {
        complex_f64: Option<usize>,
        point: Option<usize>,
    }

    impl StructIdLookup for StubIds {
        fn struct_type_id(&self, name: &str) -> Option<usize> {
            match name {
                "Complex{Float64}" => self.complex_f64,
                "Point" => self.point,
                _ => None,
            }
        }

        fn instantiation_of(&self, _base_name: &str) -> Option<(String, usize)> {
            None
        }
    }

    #[test]
    fn complex_value_adapter_pins_struct_type_id_not_complexf64() {
        let ids = StubIds {
            complex_f64: Some(42),
            point: None,
        };
        // The bridge would alias Complex{Float64} to ValueType::ComplexF64;
        // the legacy gate returned Struct(type_id) and must keep doing so.
        assert_eq!(infer_value_complex_call(&ids), Some(ValueType::Struct(42)));

        let ids = StubIds {
            complex_f64: None,
            point: None,
        };
        assert_eq!(infer_value_complex_call(&ids), None);
    }

    #[test]
    fn complex_julia_adapter_keeps_legacy_complex_f64_fallback() {
        let ids = StubIds {
            complex_f64: Some(42),
            point: None,
        };
        assert_eq!(
            infer_julia_complex_call(&ids),
            JuliaType::Struct("Complex{Float64}".to_string())
        );

        // No Complex struct registered: the legacy hardcoded result is pinned.
        let ids = StubIds {
            complex_f64: None,
            point: None,
        };
        assert_eq!(
            infer_julia_complex_call(&ids),
            JuliaType::Struct("Complex{Float64}".to_string())
        );
    }

    #[test]
    fn value_dict_constructor_returns_struct_type_and_widens_unknown_iterables() {
        let mut inst = StubInstantiation {
            instantiation: Some(77),
            ..StubInstantiation::default()
        };

        assert_eq!(
            infer_value_dict_constructor_call("Dict", &[], &mut inst),
            Some(ValueType::Struct(77))
        );
        assert_eq!(
            inst.resolved_calls.borrow().last(),
            Some(&("Dict".to_string(), vec![JuliaType::Any, JuliaType::Any]))
        );

        assert_eq!(
            infer_value_dict_constructor_call(
                "Dict",
                &[
                    JuliaType::Struct("Pair{String,Int64}".to_string()),
                    JuliaType::Struct("Pair{String,Int64}".to_string())
                ],
                &mut inst
            ),
            Some(ValueType::Struct(77))
        );
        assert_eq!(
            infer_value_dict_constructor_call("Dict{String, Int64}", &[], &mut inst),
            Some(ValueType::Struct(77))
        );
        // Unknown iterable element types widen; they must not become legacy
        // `ValueType::Dict`.
        assert_eq!(
            infer_value_dict_constructor_call("Dict", &[JuliaType::Any], &mut inst),
            Some(ValueType::Any)
        );
        assert_eq!(
            infer_value_dict_constructor_call("Set", &[], &mut inst),
            None
        );
    }

    #[test]
    fn struct_constructor_adapter_resolves_exact_entries_only() {
        let ids = StubIds {
            complex_f64: None,
            point: Some(9),
        };
        assert_eq!(
            infer_value_struct_constructor_call("Point", &ids),
            Some(ValueType::Struct(9))
        );
        assert_eq!(infer_value_struct_constructor_call("Missing", &ids), None);
    }

    /// Scripted [`StructInstantiation`] stub for the parametric constructor
    /// adapters (Issue #5922 wave 5).
    #[derive(Default)]
    struct StubInstantiation {
        /// Exact struct-table entries: name → type_id.
        exact: Vec<(&'static str, usize)>,
        /// infer_ctor_type_args result (None = inference failure).
        type_args: Option<Vec<JuliaType>>,
        /// resolve_instantiation result (None = failure).
        instantiation: Option<usize>,
        /// base_struct_type_id result.
        base_id: Option<usize>,
        /// instantiation_of result.
        any_instantiation: Option<(String, usize)>,
        resolved_calls: std::cell::RefCell<Vec<(String, Vec<JuliaType>)>>,
        resolved_type_expr_calls: std::cell::RefCell<Vec<(String, Vec<TypeExpr>)>>,
    }

    impl StructIdLookup for StubInstantiation {
        fn struct_type_id(&self, name: &str) -> Option<usize> {
            self.exact
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, id)| *id)
        }

        fn instantiation_of(&self, _base_name: &str) -> Option<(String, usize)> {
            self.any_instantiation.clone()
        }
    }

    impl StructInstantiation for StubInstantiation {
        fn infer_ctor_type_args(
            &self,
            _base_name: &str,
            _arg_types: &[JuliaType],
        ) -> Option<Vec<JuliaType>> {
            self.type_args.clone()
        }

        fn resolve_instantiation(
            &mut self,
            base_name: &str,
            type_args: &[JuliaType],
        ) -> Option<usize> {
            self.resolved_calls
                .borrow_mut()
                .push((base_name.to_string(), type_args.to_vec()));
            self.instantiation
        }

        fn resolve_instantiation_with_type_expr(
            &mut self,
            base_name: &str,
            type_args: &[TypeExpr],
        ) -> Option<usize> {
            self.resolved_type_expr_calls
                .borrow_mut()
                .push((base_name.to_string(), type_args.to_vec()));
            self.instantiation
        }

        fn base_struct_type_id(&self, _base_name: &str) -> Option<usize> {
            self.base_id
        }
    }

    #[test]
    fn array_like_view_constructor_contract_infers_concrete_subarray_8246() {
        // Prevention (Issue #8246): array-like wrappers need both a compile-time
        // AbstractArray subtype and runtime equality normalization. This pins the
        // compile-time half for the #8240 shape so `view(...) == view(...)` cannot
        // drift back to `Any` and generic identity equality.
        let mut inst = StubInstantiation {
            instantiation: Some(91),
            ..Default::default()
        };
        let value_args = [
            ValueType::ArrayOf(ArrayElementType::I64, Some(1)),
            ValueType::Range,
        ];
        let julia_args = [
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::UnitRange,
        ];

        assert_eq!(
            infer_value_view_call("view", &value_args, &julia_args, &mut inst),
            Some(ValueType::Struct(91))
        );

        let calls = inst.resolved_type_expr_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "SubArray");
        assert_eq!(
            calls[0].1,
            vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::TypeVar("1".to_string()),
                TypeExpr::Parameterized {
                    base: "Vector".to_string(),
                    params: vec![TypeExpr::Concrete(JuliaType::Int64)],
                },
                TypeExpr::Parameterized {
                    base: "Tuple".to_string(),
                    params: vec![TypeExpr::Parameterized {
                        base: "UnitRange".to_string(),
                        params: vec![TypeExpr::Concrete(JuliaType::Int64)],
                    }],
                },
                TypeExpr::TypeVar("true".to_string()),
            ]
        );

        assert_eq!(
            infer_julia_view_call("Base.view", &julia_args),
            Some(JuliaType::Struct(
                "SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}".to_string()
            ))
        );
        assert_eq!(
            infer_julia_view_call("view", &[JuliaType::Any, JuliaType::UnitRange]),
            None,
            "unknown array-like constructors must stay dynamic, not fake a concrete wrapper"
        );
    }

    // Issue #5922 wave 5: parametric ctor resolution order is pinned —
    // exact concrete entry → on-demand instantiation → any instantiation →
    // Any; inference failure falls back to the base-name id.
    #[test]
    fn parametric_struct_ctor_pins_legacy_resolution_order() {
        // Exact concrete entry wins; no instantiation performed.
        let mut inst = StubInstantiation {
            type_args: Some(vec![JuliaType::Int64]),
            exact: vec![("Point{Int64}", 3)],
            instantiation: Some(4),
            ..Default::default()
        };
        assert_eq!(
            infer_value_parametric_struct_ctor("Point", &mut inst, &[JuliaType::Int64]),
            ValueType::Struct(3)
        );
        assert!(inst.resolved_calls.borrow().is_empty());

        // No exact entry → instantiate on demand.
        let mut inst = StubInstantiation {
            type_args: Some(vec![JuliaType::Int64]),
            instantiation: Some(4),
            ..Default::default()
        };
        assert_eq!(
            infer_value_parametric_struct_ctor("Point", &mut inst, &[JuliaType::Int64]),
            ValueType::Struct(4)
        );

        // Instantiation failure → any existing instantiation of the base.
        let mut inst = StubInstantiation {
            type_args: Some(vec![JuliaType::Int64]),
            any_instantiation: Some(("Point{Float64}".to_string(), 5)),
            ..Default::default()
        };
        assert_eq!(
            infer_value_parametric_struct_ctor("Point", &mut inst, &[JuliaType::Int64]),
            ValueType::Struct(5)
        );

        // Type-arg inference failure → base-name type id, else Any.
        let mut inst = StubInstantiation {
            base_id: Some(6),
            ..Default::default()
        };
        assert_eq!(
            infer_value_parametric_struct_ctor("Point", &mut inst, &[]),
            ValueType::Struct(6)
        );
        let mut inst = StubInstantiation::default();
        assert_eq!(
            infer_value_parametric_struct_ctor("Point", &mut inst, &[]),
            ValueType::Any
        );
    }

    #[test]
    fn rational_ctor_prefers_exact_entry_then_base_id() {
        let inst = StubInstantiation {
            exact: vec![("Rational{Int64}", 11)],
            base_id: Some(12),
            ..Default::default()
        };
        assert_eq!(
            infer_value_rational_ctor("Rational{Int64}", &inst),
            ValueType::Struct(11)
        );
        let inst = StubInstantiation {
            base_id: Some(12),
            ..Default::default()
        };
        assert_eq!(
            infer_value_rational_ctor("Rational{Int64}", &inst),
            ValueType::Struct(12)
        );
        let inst = StubInstantiation::default();
        assert_eq!(infer_value_rational_ctor("Rational", &inst), ValueType::Any);
    }

    #[test]
    fn instantiated_ctor_parses_type_args_and_pins_fallback_chain() {
        // Exact entry wins.
        let mut inst = StubInstantiation {
            exact: vec![("Val{2}", 21)],
            ..Default::default()
        };
        assert_eq!(
            infer_value_instantiated_ctor("Val{2}", "Val", &mut inst),
            ValueType::Struct(21)
        );
        assert!(inst.resolved_calls.borrow().is_empty());

        // No exact entry → instantiate with the parsed type args against the
        // RESOLVED base name.
        let mut inst = StubInstantiation {
            instantiation: Some(22),
            ..Default::default()
        };
        assert_eq!(
            infer_value_instantiated_ctor("Point{Int64}", "MyGeometry.Point", &mut inst),
            ValueType::Struct(22)
        );
        assert_eq!(
            inst.resolved_calls.borrow().as_slice(),
            &[("MyGeometry.Point".to_string(), vec![JuliaType::Int64])]
        );

        // Instantiation failure → any instantiation → Any.
        let mut inst = StubInstantiation {
            any_instantiation: Some(("Val{1}".to_string(), 23)),
            ..Default::default()
        };
        assert_eq!(
            infer_value_instantiated_ctor("Val{9}", "Val", &mut inst),
            ValueType::Struct(23)
        );
        let mut inst = StubInstantiation::default();
        assert_eq!(
            infer_value_instantiated_ctor("Val{9}", "Val", &mut inst),
            ValueType::Any
        );
    }

    // Issue #5916 / #5922 wave 5: the adapter's JuliaType→LatticeType edge
    // delegates the shared concrete mapping to bridge::julia_type_to_lattice
    // and pins its adapter-specific divergences explicitly.
    #[test]
    fn julia_type_to_lattice_delegates_shared_mapping_to_canonical_bridge() {
        for ty in [
            JuliaType::Int64,
            JuliaType::UInt8,
            JuliaType::Float32,
            JuliaType::BigInt,
            JuliaType::Bool,
            JuliaType::String,
            JuliaType::Char,
            JuliaType::Array,
            JuliaType::Dict,
            JuliaType::Set,
            JuliaType::Nothing,
            JuliaType::Missing,
            JuliaType::Number,
            JuliaType::Real,
            JuliaType::Integer,
            JuliaType::AbstractFloat,
            JuliaType::Symbol,
            JuliaType::Tuple,
            JuliaType::Any,
        ] {
            assert_eq!(
                julia_type_to_lattice(&ty),
                crate::compile::bridge::julia_type_to_lattice(&ty),
                "{ty:?} must share the canonical mapping"
            );
        }
    }

    #[test]
    fn julia_type_to_lattice_pins_dispatch_deferral_edges_to_top() {
        // Struct names must stay Top so struct args keep deferring to method
        // dispatch (the table-free canonical produces Struct{type_id: 0}).
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Struct("S".to_string())),
            LatticeType::Top
        );
        // Signed/Unsigned deferral (canonical widens to the Integer marker).
        assert_eq!(julia_type_to_lattice(&JuliaType::Signed), LatticeType::Top);
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Unsigned),
            LatticeType::Top
        );
        // Bottom is pinned to Top INDEPENDENTLY of the canonical converter's
        // Bottom edge, which is in flux (Issue #6523).
        assert_eq!(julia_type_to_lattice(&JuliaType::Bottom), LatticeType::Top);
    }

    #[test]
    fn julia_type_to_lattice_pins_type_object_and_legacy_edges() {
        // TypeOf carries the inner type name — load-bearing for
        // typemin/typemax/zeros/ones element resolution.
        assert_eq!(
            julia_type_to_lattice(&type_of(JuliaType::Int16)),
            LatticeType::Concrete(ConcreteType::DataType {
                name: "Int16".to_string()
            })
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::DataType),
            LatticeType::Concrete(ConcreteType::DataType {
                name: "DataType".to_string()
            })
        );
        // Legacy pinnings the canonical maps to Top (or to a different shape).
        assert_eq!(
            julia_type_to_lattice(&JuliaType::AbstractString),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::IO),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::UnitRange),
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Any))
            })
        );
        // TupleOf is now DELEGATED to the canonical converter (Issue #6600):
        // it keeps the structured `Tuple{…}` elements instead of the old
        // `Tuple{}` pin. This is adapter-neutral (see the pin-audit test) —
        // every julia-path entry point collapses any `Tuple{…}` back to the
        // bare `JuliaType::Tuple`.
        let tuple = JuliaType::TupleOf(vec![JuliaType::Int64]);
        assert_eq!(
            julia_type_to_lattice(&tuple),
            crate::compile::bridge::julia_type_to_lattice(&tuple)
        );
        assert_eq!(
            julia_type_to_lattice(&tuple),
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))]
            })
        );
        // Element positions recurse through the wrapper: a struct element
        // stays Any (deferral), not Struct{type_id: 0}.
        assert_eq!(
            julia_type_to_lattice(&JuliaType::VectorOf(Box::new(JuliaType::Struct(
                "S".to_string()
            )))),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            })
        );
    }

    #[test]
    fn julia_type_to_lattice_adopts_canonical_union_handling() {
        // The old local copy collapsed every union to Top (the union-loss bug
        // listed in TYPE_REPRESENTATIONS.md §3.6); unions now delegate.
        let union = JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]);
        assert_eq!(
            julia_type_to_lattice(&union),
            crate::compile::bridge::julia_type_to_lattice(&union)
        );
        assert!(matches!(
            julia_type_to_lattice(&union),
            LatticeType::Union(_)
        ));
        // A union containing Any still widens to Top.
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Union(vec![JuliaType::Int64, JuliaType::Any])),
            LatticeType::Top
        );
    }

    /// All rule names that `infer_julia_type_call` / `infer_value_type_call` /
    /// the array-constructor / type-object / `collect` adapters route a
    /// `julia_type_to_lattice`-converted argument through. A pin is only
    /// load-bearing if, for one of these names, the local pinned lattice value
    /// produces a *different* registry result than the canonical-delegated one.
    fn routed_rule_names() -> Vec<&'static str> {
        vec![
            // first-arg tfuncs
            "replace",
            "repeat",
            "reverse",
            "abs",
            "abs2",
            "sign",
            // general value/julia tfuncs (string/bool/numeric/etc.)
            "string",
            "uppercase",
            "lowercase",
            "join",
            "repr",
            "strip",
            "lstrip",
            "rstrip",
            "chomp",
            "chop",
            "take!",
            "takestring!",
            "sprint",
            "sprintf",
            "lowercasefirst",
            "uppercasefirst",
            "escape_string",
            "chopprefix",
            "chopsuffix",
            "lpad",
            "rpad",
            "bitstring",
            "ascii",
            "unescape_string",
            "startswith",
            "endswith",
            "contains",
            "occursin",
            "isa",
            "haskey",
            "isless",
            "isnan",
            "isinf",
            "isfinite",
            "isinteger",
            "iseven",
            "isodd",
            "isnothing",
            "ismissing",
            "isequal",
            "length",
            "size",
            "ndims",
            "count",
            "div",
            "rem",
            "mod",
            "sqrt",
            "sin",
            "cos",
            "exp",
            "log",
            "tan",
            "asin",
            "acos",
            "atan",
            "sinh",
            "cosh",
            "tanh",
            "asinh",
            "acosh",
            "atanh",
            "log2",
            "log10",
            "log1p",
            "expm1",
            "signbit",
            "min",
            "max",
            "floor",
            "ceil",
            "round",
            "trunc",
            "prod",
            "mean",
            "std",
            "var",
            "gcd",
            "lcm",
            "big",
            "IOBuffer",
            "typeof",
            "promote_type",
            "promote_rule",
            "eltype",
            "keytype",
            "valtype",
            "Int",
            "hash",
            "fld",
            "cld",
            "year",
            "month",
            "day",
            "hour",
            "minute",
            "second",
            "dayofweek",
            "dayofyear",
            "week",
            "days",
            "rand",
            "randn",
            "trues",
            "falses",
            // numeric/array/type-object constructors
            "Int8",
            "Int16",
            "Int32",
            "Int64",
            "Int128",
            "UInt8",
            "UInt16",
            "UInt32",
            "UInt64",
            "UInt128",
            "Float16",
            "Float32",
            "Float64",
            "BigInt",
            "BigFloat",
            "fill",
            "zeros",
            "ones",
            "typemin",
            "typemax",
            "collect",
        ]
    }

    /// Every `JuliaType` arm pinned in the local `julia_type_to_lattice`
    /// adapter (i.e. every arm that does NOT fall through to the canonical
    /// `bridge::julia_type_to_lattice`).
    fn pinned_arms() -> Vec<JuliaType> {
        vec![
            JuliaType::Struct("S".to_string()),
            JuliaType::Signed,
            JuliaType::Unsigned,
            JuliaType::Bottom,
            type_of(JuliaType::Int64),
            JuliaType::DataType,
            JuliaType::Type,
            JuliaType::AbstractString,
            JuliaType::AbstractChar,
            JuliaType::AbstractArray,
            JuliaType::VectorOf(Box::new(JuliaType::Struct("S".to_string()))),
            JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
            JuliaType::TupleOf(vec![JuliaType::Int64]),
            JuliaType::NamedTuple,
            JuliaType::UnitRange,
            JuliaType::StepRange,
            JuliaType::AbstractRange,
            JuliaType::Module,
            JuliaType::Function,
            JuliaType::IO,
            JuliaType::IOBuffer,
            JuliaType::Expr,
            JuliaType::QuoteNode,
            JuliaType::LineNumberNode,
            JuliaType::GlobalRef,
            JuliaType::Pairs,
            JuliaType::Generator,
            JuliaType::Enum("E".to_string()),
        ]
    }

    // -------------------------------------------------------------------
    // Issue #6600 pin audit: adapter-level dead-vs-load-bearing measurement.
    // -------------------------------------------------------------------

    use std::cell::RefCell;

    thread_local! {
        /// Set of `JuliaType` discriminant tags the converter should delegate
        /// to the canonical bridge instead of applying its local pin. Used only
        /// by the pin-audit test to measure the adapter-level effect of
        /// removing one pin.
        static DELEGATED_PINS: RefCell<Vec<JuliaType>> = const { RefCell::new(Vec::new()) };
    }

    /// Whether the converter should delegate `ty` to canonical (audit hook).
    pub(super) fn should_delegate_pin(ty: &JuliaType) -> bool {
        DELEGATED_PINS.with(|set| {
            set.borrow()
                .iter()
                .any(|d| std::mem::discriminant(d) == std::mem::discriminant(ty))
        })
    }

    fn with_delegated_pin<R>(ty: &JuliaType, f: impl FnOnce() -> R) -> R {
        DELEGATED_PINS.with(|set| set.borrow_mut().push(ty.clone()));
        let result = f();
        DELEGATED_PINS.with(|set| set.borrow_mut().clear());
        result
    }

    /// Drive every julia-path adapter entry point that consumes the local
    /// converter, returning a stable snapshot string for a single inferred
    /// argument type. This is the *adapter-level* observable (post-fallback),
    /// not the raw registry-rule result.
    fn julia_adapter_outputs(arg: &JuliaType) -> String {
        let mut out = String::new();
        let one = vec![lit()];
        let two = vec![lit(), lit()];
        // infer_julia_type_call covers first-arg tfuncs, general tfuncs,
        // collect, and type-object calls internally.
        for name in routed_rule_names() {
            for args in [&one, &two] {
                let r = infer_julia_type_call(name, args, |_| arg.clone());
                out.push_str(&format!("{name}/{}:{r:?}\n", args.len()));
            }
        }
        // Array constructors with the arg in the (element-type) first slot.
        for name in ["fill", "zeros", "ones"] {
            for args in [&one, &two] {
                let r = infer_julia_array_constructor_call(name, args, |_| arg.clone());
                out.push_str(&format!("arr:{name}/{}:{r:?}\n", args.len()));
            }
        }
        out
    }

    /// Pin-audit (Issue #6600): the authoritative dead-vs-load-bearing ledger.
    ///
    /// For each pinned arm we compare the *adapter-level* output of every
    /// julia-path entry point under the local pin vs. under canonical
    /// delegation. An arm is **load-bearing** iff delegating it changes any
    /// adapter output. A pin listed in `load_bearing` MUST diverge; a pin not
    /// listed MUST be adapter-equivalent to canonical (so it is dead and the
    /// production converter may delegate it without behavior change).
    #[test]
    fn pin_audit_load_bearing_arms_diverge_dead_arms_match() {
        // Expected load-bearing verdict per arm (adapter-level). Everything
        // pinned is load-bearing EXCEPT the two dead arms proven by the audit:
        //  - `TupleOf` (now delegated; element types never surface), and
        //  - `Vector`/`Matrix` whose element is itself a concrete canonical type
        //    (local and canonical element recursion agree, so the wrapper is a
        //    no-op). `Vector{MyStruct}` IS load-bearing (local element→Any vs
        //    canonical→Struct{0}).
        let load_bearing = |ty: &JuliaType| -> bool {
            match ty {
                JuliaType::TupleOf(_) => false,
                JuliaType::VectorOf(element) | JuliaType::MatrixOf(element) => {
                    julia_type_to_lattice(element)
                        != crate::compile::bridge::julia_type_to_lattice(element)
                }
                JuliaType::Struct(_)
                | JuliaType::Signed
                | JuliaType::Unsigned
                | JuliaType::Bottom
                | JuliaType::TypeOf(_)
                | JuliaType::DataType
                | JuliaType::Type
                | JuliaType::AbstractString
                | JuliaType::AbstractChar
                | JuliaType::AbstractArray
                | JuliaType::NamedTuple
                | JuliaType::UnitRange
                | JuliaType::StepRange
                | JuliaType::AbstractRange
                | JuliaType::Module
                | JuliaType::Function
                | JuliaType::IO
                | JuliaType::IOBuffer
                | JuliaType::Expr
                | JuliaType::QuoteNode
                | JuliaType::LineNumberNode
                | JuliaType::GlobalRef
                | JuliaType::Pairs
                | JuliaType::Generator
                | JuliaType::Enum(_) => true,
                // Any other variant is not a pinned arm (delegated already).
                _ => false,
            }
        };

        let mut report = String::new();
        let mut mismatches = Vec::new();
        for ty in pinned_arms() {
            let pinned = julia_adapter_outputs(&ty);
            let delegated = with_delegated_pin(&ty, || julia_adapter_outputs(&ty));
            let diverges = pinned != delegated;
            report.push_str(&format!(
                "{ty:?}: adapter_diverges={diverges} load_bearing={}\n",
                load_bearing(&ty)
            ));
            if diverges != load_bearing(&ty) {
                // Show the first differing line for the mismatching arm.
                let first_diff = pinned
                    .lines()
                    .zip(delegated.lines())
                    .find(|(a, b)| a != b)
                    .map(|(a, b)| format!("  pinned={a}  delegated={b}"))
                    .unwrap_or_default();
                mismatches.push(format!(
                    "{ty:?} (load_bearing={}):\n{first_diff}",
                    load_bearing(&ty)
                ));
            }
        }
        assert!(
            mismatches.is_empty(),
            "pin ledger mismatches:\n{}\n--- summary ---\n{report}",
            mismatches.join("\n")
        );
    }
}
