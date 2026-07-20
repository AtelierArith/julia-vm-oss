use super::{CoreAbstract, CorePrimitive, CoreType, CoreTypeVar};

impl From<&crate::types::JuliaType> for CoreType {
    fn from(ty: &crate::types::JuliaType) -> Self {
        use crate::types::JuliaType as JT;
        match ty {
            JT::Int8 => Self::Primitive(CorePrimitive::Int8),
            JT::Int16 => Self::Primitive(CorePrimitive::Int16),
            JT::Int32 => Self::Primitive(CorePrimitive::Int32),
            JT::Int64 => Self::Primitive(CorePrimitive::Int64),
            JT::Int128 => Self::Primitive(CorePrimitive::Int128),
            JT::BigInt => Self::Primitive(CorePrimitive::BigInt),
            JT::UInt8 => Self::Primitive(CorePrimitive::UInt8),
            JT::UInt16 => Self::Primitive(CorePrimitive::UInt16),
            JT::UInt32 => Self::Primitive(CorePrimitive::UInt32),
            JT::UInt64 => Self::Primitive(CorePrimitive::UInt64),
            JT::UInt128 => Self::Primitive(CorePrimitive::UInt128),
            JT::Bool => Self::Primitive(CorePrimitive::Bool),
            JT::Float16 => Self::Primitive(CorePrimitive::Float16),
            JT::Float32 => Self::Primitive(CorePrimitive::Float32),
            JT::Float64 => Self::Primitive(CorePrimitive::Float64),
            JT::BigFloat => Self::Primitive(CorePrimitive::BigFloat),
            JT::String => Self::Primitive(CorePrimitive::String),
            JT::Char => Self::Primitive(CorePrimitive::Char),
            JT::Symbol => Self::Primitive(CorePrimitive::Symbol),
            JT::Nothing => Self::Primitive(CorePrimitive::Nothing),
            JT::Missing => Self::Primitive(CorePrimitive::Missing),
            JT::Any => Self::Any,
            JT::Bottom => Self::Bottom,
            JT::Number => Self::Abstract(CoreAbstract::Number),
            JT::Real => Self::Abstract(CoreAbstract::Real),
            JT::Integer => Self::Abstract(CoreAbstract::Integer),
            JT::Signed => Self::Abstract(CoreAbstract::Signed),
            JT::Unsigned => Self::Abstract(CoreAbstract::Unsigned),
            JT::AbstractFloat => Self::Abstract(CoreAbstract::AbstractFloat),
            JT::AbstractString => Self::Abstract(CoreAbstract::AbstractString),
            JT::AbstractChar => Self::Abstract(CoreAbstract::AbstractChar),
            JT::AbstractArray => Self::Abstract(CoreAbstract::AbstractArray),
            JT::AbstractRange => Self::Abstract(CoreAbstract::AbstractRange),
            JT::Function => Self::Abstract(CoreAbstract::Function),
            JT::IO => Self::Abstract(CoreAbstract::IO),
            JT::Type => Self::Abstract(CoreAbstract::Type),
            JT::DataType => Self::Abstract(CoreAbstract::DataType),
            JT::Array => Self::Struct {
                name: "Array".to_string(),
                params: vec![],
            },
            JT::VectorOf(elem) => Self::Struct {
                name: "Vector".to_string(),
                params: vec![Self::from(elem.as_ref())],
            },
            JT::MatrixOf(elem) => Self::Struct {
                name: "Matrix".to_string(),
                params: vec![Self::from(elem.as_ref())],
            },
            JT::Tuple => Self::Struct {
                name: "Tuple".to_string(),
                params: vec![],
            },
            JT::TupleOf(elements) => {
                Self::Tuple(elements.iter().map(Self::from).collect::<Vec<_>>())
            }
            JT::NamedTuple => Self::Struct {
                name: "NamedTuple".to_string(),
                params: vec![],
            },
            JT::Dict => Self::Struct {
                name: "Dict".to_string(),
                params: vec![],
            },
            JT::Set => Self::Struct {
                name: "Set".to_string(),
                params: vec![],
            },
            JT::UnitRange => Self::Struct {
                name: "UnitRange".to_string(),
                params: vec![],
            },
            JT::StepRange => Self::Struct {
                name: "StepRange".to_string(),
                params: vec![],
            },
            JT::Struct(name) => Self::from_julia_name(name),
            JT::Module => Self::Module("Module".to_string()),
            JT::Expr | JT::QuoteNode | JT::LineNumberNode | JT::GlobalRef => {
                Self::from_julia_name(&ty.to_string())
            }
            JT::Enum(_) => Self::Named(ty.to_string()),
            JT::Pairs | JT::Generator | JT::IOBuffer => Self::from_julia_name(&ty.to_string()),
            JT::AbstractUser(name, parent) => Self::AbstractUser {
                name: name.clone(),
                parent: parent
                    .as_ref()
                    .map(|p| Box::new(Self::from_julia_name(p.as_str()))),
            },
            JT::TypeVar(name, upper_bound) => Self::TypeVar(CoreTypeVar::with_bounds(
                name.clone(),
                upper_bound
                    .as_ref()
                    .and_then(|b| core_lower_bound_from_typevar_bound(name, b)),
                upper_bound.as_ref().and_then(|b| {
                    if b.trim().starts_with(">:") {
                        None
                    } else {
                        core_upper_bound_from_name(b)
                    }
                }),
            )),
            JT::RuntimeTypeVar { .. } => {
                core_type_from_runtime_semantic(ty, &mut RuntimeSemanticContext::new(ty, false))
            }
            JT::RuntimeParametric { base, params } => match (base.as_str(), params.as_slice()) {
                ("Vararg", [element]) => Self::Vararg(Box::new(Self::from(element))),
                ("Vararg", [element, len]) => Self::VarargLen {
                    element: Box::new(Self::from(element)),
                    len: Box::new(Self::from(len)),
                },
                _ => Self::Struct {
                    name: base.clone(),
                    params: params.iter().map(Self::from).collect(),
                },
            },
            JT::Union(types) => Self::Union(types.iter().map(Self::from).collect()),
            JT::TypeOf(inner) => Self::TypeOf(Box::new(Self::from(inner.as_ref()))),
            JT::UnionAll { .. } => {
                core_type_from_runtime_semantic(ty, &mut RuntimeSemanticContext::new(ty, false))
            }
            JT::RuntimeUnionAll { .. } => {
                core_type_from_runtime_semantic(ty, &mut RuntimeSemanticContext::new(ty, false))
            }
        }
    }
}

impl CoreType {
    /// Convert a runtime Julia type into the semantic core while retaining an
    /// explicit user owner whenever sibling qualified families make that owner
    /// identity-bearing. This is the structured counterpart of
    /// [`Self::from_julia_name_for_dispatch`] for already-parsed `JuliaType`
    /// values (Issue #10460).
    pub fn from_julia_type_preserving_owner(ty: &crate::types::JuliaType) -> Self {
        core_type_from_runtime_semantic(ty, &mut RuntimeSemanticContext::new(ty, true))
    }
}

struct RuntimeSemanticContext {
    binders: Vec<(u64, CoreTypeVar)>,
    named_binders: Vec<CoreTypeVar>,
    reserved_names: std::collections::HashSet<String>,
    assigned_names: std::collections::HashSet<String>,
    preserve_user_owner: bool,
    next_scope_id: u32,
}

impl RuntimeSemanticContext {
    fn new(ty: &crate::types::JuliaType, preserve_user_owner: bool) -> Self {
        let mut reserved_names = std::collections::HashSet::new();
        collect_runtime_binder_names(ty, &mut reserved_names);
        Self {
            binders: Vec::new(),
            named_binders: Vec::new(),
            reserved_names,
            assigned_names: std::collections::HashSet::new(),
            preserve_user_owner,
            next_scope_id: 1,
        }
    }

    fn fresh_scope_id(&mut self) -> u32 {
        let scope_id = self.next_scope_id;
        self.next_scope_id = self.next_scope_id.checked_add(1).unwrap_or(1);
        scope_id
    }

    fn assign_binder_name(&mut self, name: &str) -> String {
        if self.assigned_names.insert(name.to_string()) {
            return name.to_string();
        }
        let mut suffix = 1;
        loop {
            let candidate = format!("{name}{suffix}");
            if !self.reserved_names.contains(&candidate)
                && self.assigned_names.insert(candidate.clone())
            {
                return candidate;
            }
            suffix += 1;
        }
    }
}

fn collect_runtime_binder_names(
    ty: &crate::types::JuliaType,
    names: &mut std::collections::HashSet<String>,
) {
    use crate::types::JuliaType as JT;

    match ty {
        JT::RuntimeUnionAll { var, body } => {
            if let JT::RuntimeTypeVar { name, .. } = var.as_ref() {
                names.insert(name.clone());
            }
            collect_runtime_binder_names(body, names);
        }
        JT::RuntimeParametric { params, .. } | JT::TupleOf(params) | JT::Union(params) => {
            for param in params {
                collect_runtime_binder_names(param, names);
            }
        }
        JT::VectorOf(inner) | JT::MatrixOf(inner) | JT::TypeOf(inner) => {
            collect_runtime_binder_names(inner, names)
        }
        JT::UnionAll { body, .. } => collect_runtime_binder_names(body, names),
        _ => {}
    }
}

fn core_type_from_runtime_semantic(
    ty: &crate::types::JuliaType,
    context: &mut RuntimeSemanticContext,
) -> CoreType {
    use crate::types::JuliaType as JT;

    match ty {
        JT::TypeVar(name, upper_bound) => {
            if let Some(binder) = context
                .named_binders
                .iter()
                .rev()
                .find(|binder| binder.name == *name)
            {
                // Bounds belong to the enclosing UnionAll binder. Its body
                // carries an unbounded reference, matching from_julia_name.
                return CoreType::TypeVar(
                    CoreTypeVar::unscoped(binder.name.clone()).with_scope_id(binder.scope_id),
                );
            }
            let lower = upper_bound
                .as_ref()
                .and_then(|source| {
                    core_lower_bound_from_typevar_bound(name, source)
                        .map(|bound| bind_named_core_type(*bound, context, source))
                })
                .map(Box::new);
            let upper = upper_bound
                .as_ref()
                .filter(|bound| !bound.trim().starts_with(">:"))
                .and_then(|source| {
                    core_upper_bound_from_name(source)
                        .map(|bound| bind_named_core_type(*bound, context, source))
                })
                .map(Box::new);
            CoreType::TypeVar(CoreTypeVar::with_bounds(name.clone(), lower, upper))
        }
        JT::RuntimeTypeVar {
            id,
            name,
            lower_bound,
            upper_bound,
        } => {
            if let Some((_, binder)) = context
                .binders
                .iter()
                .rev()
                .find(|(bound_id, _)| bound_id == id)
            {
                let mut reference = binder.clone();
                reference.lower_bound = None;
                reference.upper_bound = None;
                return CoreType::TypeVar(reference);
            }
            let var = CoreTypeVar::with_bounds(
                name.clone(),
                (!matches!(lower_bound.as_ref(), JT::Bottom))
                    .then(|| Box::new(core_type_from_runtime_semantic(lower_bound, context))),
                (!matches!(upper_bound.as_ref(), JT::Any))
                    .then(|| Box::new(core_type_from_runtime_semantic(upper_bound, context))),
            );
            CoreType::TypeVar(var.with_rigid_identity(*id))
        }
        JT::RuntimeUnionAll { var, body } => {
            let JT::RuntimeTypeVar {
                id,
                name,
                lower_bound,
                upper_bound,
            } = var.as_ref()
            else {
                return CoreType::Any;
            };
            let alias = context.assign_binder_name(name);
            let scope_id = context.fresh_scope_id();

            let placeholder = CoreTypeVar::unscoped(alias.clone()).with_scope_id(scope_id);
            context.binders.push((*id, placeholder));
            let lower_bound = (!matches!(lower_bound.as_ref(), JT::Bottom))
                .then(|| Box::new(core_type_from_runtime_semantic(lower_bound, context)));
            let upper_bound = (!matches!(upper_bound.as_ref(), JT::Any))
                .then(|| Box::new(core_type_from_runtime_semantic(upper_bound, context)));
            context.binders.pop();

            let binder =
                CoreTypeVar::with_bounds(alias, lower_bound, upper_bound).with_scope_id(scope_id);
            context.binders.push((*id, binder.clone()));
            let body = core_type_from_runtime_semantic(body, context);
            context.binders.pop();
            CoreType::UnionAll {
                var: binder,
                body: Box::new(body),
            }
        }
        JT::RuntimeParametric { base, params } => match (base.as_str(), params.as_slice()) {
            ("Vararg", [element]) => {
                CoreType::Vararg(Box::new(core_type_from_runtime_semantic(element, context)))
            }
            ("Vararg", [element, len]) => CoreType::VarargLen {
                element: Box::new(core_type_from_runtime_semantic(element, context)),
                len: Box::new(core_type_from_runtime_semantic(len, context)),
            },
            _ => CoreType::Struct {
                name: base.clone(),
                params: params
                    .iter()
                    .map(|param| core_type_from_runtime_semantic(param, context))
                    .collect(),
            },
        },
        JT::VectorOf(inner) => CoreType::Struct {
            name: "Vector".to_string(),
            params: vec![core_type_from_runtime_semantic(inner, context)],
        },
        JT::MatrixOf(inner) => CoreType::Struct {
            name: "Matrix".to_string(),
            params: vec![core_type_from_runtime_semantic(inner, context)],
        },
        JT::TupleOf(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| core_type_from_runtime_semantic(element, context))
                .collect(),
        ),
        JT::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|ty| core_type_from_runtime_semantic(ty, context))
                .collect(),
        ),
        JT::TypeOf(inner) => {
            CoreType::TypeOf(Box::new(core_type_from_runtime_semantic(inner, context)))
        }
        JT::UnionAll {
            var,
            lower_bound,
            bound,
            body,
        } => {
            let scope_id = context.fresh_scope_id();
            let lower_bound = lower_bound
                .as_ref()
                .and_then(|source| {
                    core_lower_bound_from_name(source)
                        .map(|bound| bind_named_core_type(*bound, context, source))
                })
                .map(Box::new);
            let upper_bound = bound
                .as_ref()
                .and_then(|source| {
                    core_upper_bound_from_name(source)
                        .map(|bound| bind_named_core_type(*bound, context, source))
                })
                .map(Box::new);
            let binder = CoreTypeVar::with_bounds(var.clone(), lower_bound, upper_bound)
                .with_scope_id(scope_id);
            context.named_binders.push(binder.clone());
            let body = core_type_from_runtime_semantic(body, context);
            context.named_binders.pop();
            CoreType::UnionAll {
                var: binder,
                body: Box::new(body),
            }
        }
        JT::Struct(name) if context.preserve_user_owner => {
            let core = CoreType::from_julia_name_for_dispatch(name);
            bind_named_core_type(core, context, name)
        }
        _ => CoreType::from(ty),
    }
}

fn bind_named_core_type(
    mut ty: CoreType,
    context: &RuntimeSemanticContext,
    source: &str,
) -> CoreType {
    // Legacy nested `UnionAll` stores declared bounds as names. When it sits
    // inside an identity-bearing RuntimeUnionAll, those names are resolved in
    // the enclosing runtime binder scope before any nested named binders.
    for (_, binder) in &context.binders {
        if legacy_bound_references_bare_name(source, &binder.name) {
            ty.rebind_source_where_binder(&binder.name);
        }
        ty = super::substitute_typevar_bound(&ty, binder);
    }
    for binder in &context.named_binders {
        if legacy_bound_references_bare_name(source, &binder.name) {
            ty.rebind_source_where_binder(&binder.name);
        }
        ty = super::substitute_typevar_bound(&ty, binder);
    }
    ty
}

fn legacy_bound_references_bare_name(source: &str, binder_name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            if &source[start..index] == binder_name {
                let mut previous = start;
                while previous > 0 && bytes[previous - 1].is_ascii_whitespace() {
                    previous -= 1;
                }
                if previous == 0 || bytes[previous - 1] != b'.' {
                    return true;
                }
            }
        } else {
            index += 1;
        }
    }
    false
}

/// Structured `TypeExpr → CoreType` conversion (Issue #6720).
///
/// The direct, **string-free** resolver that lets lowering project a structured
/// `TypeExpr` into the structured `CoreType` hub. It replaces the
/// `to_string()` + `from_name_or_struct` round-trip
/// (`TYPE_REPRESENTATIONS.md` §3.3.1 / conversion #34) that collapses a
/// parametric application's params into an opaque `JuliaType::Struct(String)`:
/// a parametric `TypeExpr::Parameterized` now lands as a structured
/// `CoreType::Struct { name, params }` (or `CoreType::Tuple` / canonical
/// `CoreType::Union`), so the parameter structure survives into the hub.
///
/// Behaviour contract: `core_type_to_julia_type(&CoreType::from(te))` equals the
/// former `JuliaType::from_name_or_struct(&te.to_string())` for the
/// lowering-produced `TypeExpr` shapes (pinned by
/// `type_expr::tests::to_julia_type_lossy_matches_string_round_trip_issue_6720`).
impl From<&crate::types::TypeExpr> for CoreType {
    fn from(te: &crate::types::TypeExpr) -> Self {
        use crate::types::TypeExpr as TE;
        match te {
            // `Concrete` already holds a parsed `JuliaType`; reuse the canonical
            // `JuliaType → CoreType` hub directly (no render + reparse).
            TE::Concrete(jt) => CoreType::from(jt),
            // TypeExpr already carries the type-parameter environment decision:
            // a TypeVar leaf is explicit and must not be re-derived from the
            // spelling of `name` (Issue #9563).
            TE::TypeVar(name) => CoreType::TypeVar(CoreTypeVar::unscoped(name.clone())),
            // Runtime expressions remain textual because they can denote value
            // parameters or runtime-evaluated type objects.
            TE::RuntimeExpr(expr) => CoreType::from_julia_name(expr),
            TE::Parameterized { base, params } => {
                if base == "NamedTuple" {
                    return CoreType::from(&crate::types::JuliaType::from_name_or_struct(
                        &te.to_string(),
                    ));
                }
                let core_params: Vec<CoreType> = params.iter().map(CoreType::from).collect();
                match base.as_str() {
                    // `Tuple{...}` is a structural tuple, not a nominal struct.
                    "Tuple" => CoreType::Tuple(core_params),
                    // `Union{...}` must be canonicalized (flatten / dedup /
                    // subtype-absorb / sort, Issue #5066) to match the
                    // `from_name_or_struct` parse of the rendered union.
                    "Union" => {
                        let members: Vec<crate::types::JuliaType> =
                            params.iter().map(|p| p.to_julia_type_lossy()).collect();
                        CoreType::from(&crate::types::canonicalize_union(members))
                    }
                    // Everything else is a nominal type application whose params
                    // are preserved structurally (`Pair{Int64, String}`,
                    // `Vector{Int64}`, user `MyBox{T}`, …). `core_type_to_julia_type`
                    // inverts the known containers (`Vector`/`Matrix`/…) and
                    // renders the rest back to a canonical `Struct(name)` spelling.
                    _ => CoreType::Struct {
                        name: base.clone(),
                        params: core_params,
                    },
                }
            }
        }
    }
}

impl From<&crate::types::TypeParam> for CoreTypeVar {
    fn from(param: &crate::types::TypeParam) -> Self {
        Self::with_bounds(
            param.name.clone(),
            param
                .lower_bound
                .as_ref()
                .and_then(|b| core_lower_bound_from_name(b)),
            param
                .get_upper_bound()
                .and_then(|b| core_upper_bound_from_name(b)),
        )
    }
}

// NOTE: The `From<&aot::types::StaticType> for CoreType` impl has been moved
// to the main `subset_julia_vm` crate's aot bridge module per
// ADR_BACKEND_STRATEGY.md consequence 1 (local-type rule) and CRATE_SPLIT.md
// §4.3. This keeps `subset_julia_vm_types` free of AoT dependencies (Issue
// #8655).

/// Inverse of the `JuliaType → CoreType` bridge for the canonical spellings
/// produced by lowering (Issue #6336).
///
/// `JuliaType → CoreType` is not injective in general (e.g. `JT::Pairs` and
/// `JT::Struct("Pairs")` share one image), so this inverse picks, for every
/// `CoreType` shape, the canonical spelling used by cold projection accessors
/// — `Expr` resolves through `JuliaType::from_name` to the dedicated variant,
/// while `Pairs` has no `from_name` arm and stays a `Struct` spelling, etc.
/// The choice is *pinned over the whole Base corpus* by
/// `compile::cache::tests::base_method_signature_accessors_are_canonical_issue_6495`,
/// which fails the suite if any accessor stops matching this inverse.
///
/// Production use: cold JuliaType views derived from a `MethodSig`
/// `core_signature` for diagnostics and the small compatibility rules that
/// still operate on JuliaType (Issue #6495).
pub fn core_type_to_julia_type(core: &CoreType) -> crate::types::JuliaType {
    use crate::types::JuliaType as JT;
    match core {
        CoreType::Bottom => JT::Bottom,
        CoreType::Any => JT::Any,
        CoreType::Primitive(p) => match p {
            CorePrimitive::Int8 => JT::Int8,
            CorePrimitive::Int16 => JT::Int16,
            CorePrimitive::Int32 => JT::Int32,
            CorePrimitive::Int64 => JT::Int64,
            CorePrimitive::Int128 => JT::Int128,
            CorePrimitive::BigInt => JT::BigInt,
            CorePrimitive::UInt8 => JT::UInt8,
            CorePrimitive::UInt16 => JT::UInt16,
            CorePrimitive::UInt32 => JT::UInt32,
            CorePrimitive::UInt64 => JT::UInt64,
            CorePrimitive::UInt128 => JT::UInt128,
            CorePrimitive::Bool => JT::Bool,
            CorePrimitive::Float16 => JT::Float16,
            CorePrimitive::Float32 => JT::Float32,
            CorePrimitive::Float64 => JT::Float64,
            CorePrimitive::BigFloat => JT::BigFloat,
            CorePrimitive::String => JT::String,
            CorePrimitive::Char => JT::Char,
            CorePrimitive::Symbol => JT::Symbol,
            CorePrimitive::Nothing => JT::Nothing,
            CorePrimitive::Missing => JT::Missing,
        },
        CoreType::Abstract(a) => match a {
            CoreAbstract::Number => JT::Number,
            CoreAbstract::Real => JT::Real,
            CoreAbstract::Integer => JT::Integer,
            CoreAbstract::Signed => JT::Signed,
            CoreAbstract::Unsigned => JT::Unsigned,
            CoreAbstract::AbstractFloat => JT::AbstractFloat,
            CoreAbstract::AbstractString => JT::AbstractString,
            CoreAbstract::AbstractChar => JT::AbstractChar,
            CoreAbstract::AbstractArray => JT::AbstractArray,
            CoreAbstract::AbstractRange => JT::AbstractRange,
            CoreAbstract::Function => JT::Function,
            CoreAbstract::IO => JT::IO,
            CoreAbstract::Type => JT::Type,
            CoreAbstract::DataType => JT::DataType,
            // Abstract families WITHOUT a dedicated `JuliaType` variant keep
            // their `from_name_or_struct` spelling (`JuliaType::Struct(name)`).
            other => JT::Struct(CoreType::Abstract(other.clone()).to_julia_name()),
        },
        CoreType::AbstractUser { name, parent } => {
            JT::AbstractUser(name.clone(), parent.as_ref().map(|p| p.to_julia_name()))
        }
        CoreType::Module(_) => JT::Module,
        CoreType::Tuple(elems) => JT::TupleOf(elems.iter().map(core_type_to_julia_type).collect()),
        CoreType::Union(arms) => JT::Union(arms.iter().map(core_type_to_julia_type).collect()),
        CoreType::TypeOf(inner) => JT::TypeOf(Box::new(core_type_to_julia_type(inner))),
        CoreType::TypeVar(var) => JT::TypeVar(
            var.name.clone(),
            match (&var.lower_bound, &var.upper_bound) {
                (Some(lb), None) => Some(format!(">:{}", lb.to_julia_name())),
                (None, Some(ub)) => Some(ub.to_julia_name()),
                (Some(lb), Some(ub)) => {
                    Some(format!("{}<:{}", lb.to_julia_name(), ub.to_julia_name()))
                }
                (None, None) => None,
            },
        ),
        CoreType::UnionAll { var, body } => JT::UnionAll {
            var: var.name.clone(),
            lower_bound: var
                .lower_bound
                .as_ref()
                .map(|b| Box::new(b.to_julia_name())),
            bound: var
                .upper_bound
                .as_ref()
                .map(|b| Box::new(b.to_julia_name())),
            body: Box::new(core_type_to_julia_type(body)),
        },
        CoreType::Struct { name, params } => match (name.as_str(), params.len()) {
            ("Vector", 1) => JT::VectorOf(Box::new(core_type_to_julia_type(&params[0]))),
            ("Matrix", 1) => JT::MatrixOf(Box::new(core_type_to_julia_type(&params[0]))),
            ("Tuple", 0) => JT::Tuple,
            ("Array", 0) => JT::Array,
            ("Set", 0) => JT::Set,
            ("Dict", 0) => JT::Dict,
            ("NamedTuple", 0) => JT::NamedTuple,
            ("UnitRange", 0) => JT::UnitRange,
            ("StepRange", 0) => JT::StepRange,
            ("Generator", 0) => JT::Generator,
            ("IOBuffer", 0) => JT::IOBuffer,
            // The macro-system types resolve through `JuliaType::from_name` to
            // dedicated variants, so a `Struct` spelling of these names can
            // never be lowering-produced.
            ("Expr", 0) => JT::Expr,
            ("QuoteNode", 0) => JT::QuoteNode,
            ("LineNumberNode", 0) => JT::LineNumberNode,
            ("GlobalRef", 0) => JT::GlobalRef,
            _ => JT::Struct(render_canonical_struct_name(name, params)),
        },
        other => JT::Struct(other.to_julia_name()),
    }
}

/// Inverse of `CoreTypeVar::from(&TypeParam)` for the canonical bound
/// spellings, used to reconstruct `MethodSig.type_params` from the
/// `core_signature` `UnionAll` wrappers (Issue #6336).
pub fn core_type_var_to_type_param(var: &CoreTypeVar) -> crate::types::TypeParam {
    let upper = var.upper_bound.as_ref().map(|b| b.to_julia_name());
    let lower = var.lower_bound.as_ref().map(|b| b.to_julia_name());
    crate::types::TypeParam {
        name: var.name.clone(),
        upper_bound: upper.clone(),
        lower_bound: lower,
        // Legacy mirror of `upper_bound` (every production constructor keeps
        // them in sync; the field is `#[serde(skip)]`).
        bound: upper,
    }
}

/// Render a parametric struct spelling the way lowering writes it
/// (`AbstractVector{<:Integer}`, `Rational{T}`, `Pair{Int64, String}`):
/// bounded typevars keep their `<:` form instead of the `to_julia_name`
/// bare-variable rendering.
fn render_canonical_struct_name(name: &str, params: &[CoreType]) -> String {
    if params.is_empty() {
        return name.to_string();
    }
    let rendered: Vec<String> = params.iter().map(render_canonical_param).collect();
    format!("{name}{{{}}}", rendered.join(", "))
}

fn render_canonical_param(param: &CoreType) -> String {
    match param {
        CoreType::TypeVar(v) => match (&v.name, &v.lower_bound, &v.upper_bound) {
            (n, None, Some(ub)) if n == "_" => format!("<:{}", ub.to_julia_name()),
            (n, None, Some(ub)) => format!("{n}<:{}", ub.to_julia_name()),
            (n, Some(lb), None) if n == "_" => format!(">:{}", lb.to_julia_name()),
            (n, Some(lb), None) => format!("{n}>:{}", lb.to_julia_name()),
            (n, Some(lb), Some(ub)) => {
                format!("{}<:{n}<:{}", lb.to_julia_name(), ub.to_julia_name())
            }
            (n, None, None) => n.clone(),
        },
        CoreType::Struct { name, params } => render_canonical_struct_name(name, params),
        other => other.to_julia_name(),
    }
}

fn core_upper_bound_from_name(bound: &str) -> Option<Box<CoreType>> {
    let normalized = bound
        .rsplit_once("<:")
        .map_or(bound, |(_, upper)| upper)
        .trim();
    (!normalized.is_empty() && normalized != "<:")
        .then(|| Box::new(CoreType::from_julia_name(normalized)))
}

fn core_lower_bound_from_name(bound: &str) -> Option<Box<CoreType>> {
    let normalized = bound
        .split_once(">:")
        .map_or(bound, |(_, lower)| lower)
        .trim();
    (!normalized.is_empty() && normalized != ">:")
        .then(|| Box::new(CoreType::from_julia_name(normalized)))
}

fn core_lower_bound_from_typevar_bound(name: &str, bound: &str) -> Option<Box<CoreType>> {
    let trimmed = bound.trim();
    if trimmed.starts_with(">:") {
        return core_lower_bound_from_name(trimmed);
    }
    let (lower, _) = trimmed.split_once("<:")?;
    let lower = lower.trim();
    (!lower.is_empty() && lower != name).then(|| Box::new(CoreType::from_julia_name(lower)))
}

#[cfg(test)]
mod type_expr_to_core_tests {
    use super::*;
    use crate::types::{JuliaType, TypeExpr};

    /// Issue #6720: a parametric `TypeExpr::Parameterized` lands as a structured
    /// `CoreType::Struct { name, params }` — the parameters survive as real
    /// `CoreType`s instead of being collapsed into an opaque name string.
    #[test]
    fn parameterized_lands_as_structured_struct_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "Pair".to_string(),
            params: vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::Concrete(JuliaType::String),
            ],
        };
        assert_eq!(
            CoreType::from(&te),
            CoreType::Struct {
                name: "Pair".to_string(),
                params: vec![
                    CoreType::Primitive(CorePrimitive::Int64),
                    CoreType::Primitive(CorePrimitive::String),
                ],
            }
        );
    }

    /// `Tuple{...}` is structural, not nominal.
    #[test]
    fn parameterized_tuple_lands_as_core_tuple_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "Tuple".to_string(),
            params: vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::Concrete(JuliaType::Float64),
            ],
        };
        assert_eq!(
            CoreType::from(&te),
            CoreType::Tuple(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::Float64),
            ])
        );
    }

    /// `Union{...}` is canonicalized (sorted/deduped) to match the parser.
    #[test]
    fn parameterized_union_is_canonicalized_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "Union".to_string(),
            params: vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::Concrete(JuliaType::Nothing),
            ],
        };
        // Canonical order puts the singleton `Nothing` first (Issue #5066).
        assert_eq!(
            CoreType::from(&te),
            CoreType::Union(vec![
                CoreType::Primitive(CorePrimitive::Nothing),
                CoreType::Primitive(CorePrimitive::Int64),
            ])
        );
    }

    /// Nested parametric params recurse structurally (no string round-trip).
    #[test]
    fn nested_parameterized_recurses_structurally_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "MyBox".to_string(),
            params: vec![TypeExpr::Parameterized {
                base: "Pair".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::Concrete(JuliaType::Float64),
                ],
            }],
        };
        assert_eq!(
            CoreType::from(&te),
            CoreType::Struct {
                name: "MyBox".to_string(),
                params: vec![CoreType::Struct {
                    name: "Pair".to_string(),
                    params: vec![
                        CoreType::Primitive(CorePrimitive::Int64),
                        CoreType::Primitive(CorePrimitive::Float64),
                    ],
                }],
            }
        );
    }

    #[test]
    fn runtime_unionall_core_projection_binds_only_declared_ids_10613() {
        let runtime_var = |id, name: &str| JuliaType::RuntimeTypeVar {
            id,
            name: name.to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let wrapper = |bound_id, free_id| {
            let bound = runtime_var(bound_id, "T");
            JuliaType::RuntimeUnionAll {
                var: Box::new(bound.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "STFree".to_string(),
                    params: vec![bound, runtime_var(free_id, "F")],
                }),
            }
        };

        let first = CoreType::from(&wrapper(1, 10));
        let alpha_equivalent = CoreType::from(&wrapper(2, 10));
        let distinct_free = CoreType::from(&wrapper(3, 11));

        assert!(first.is_subtype_of(&alpha_equivalent));
        assert!(alpha_equivalent.is_subtype_of(&first));
        assert!(!first.is_subtype_of(&distinct_free));
        assert!(!distinct_free.is_subtype_of(&first));

        let free_underscore = |id| JuliaType::RuntimeTypeVar {
            id,
            name: "_".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let underscore_a = CoreType::from(&free_underscore(20));
        let underscore_b = CoreType::from(&free_underscore(21));
        assert!(!underscore_a.is_subtype_of(&underscore_b));
        assert!(!underscore_b.is_subtype_of(&underscore_a));
    }

    #[test]
    fn runtime_parametric_vararg_projects_to_canonical_core_shape_10460() -> Result<(), String> {
        let binder = JuliaType::RuntimeTypeVar {
            id: 10460,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let wrapper = JuliaType::RuntimeUnionAll {
            var: Box::new(binder.clone()),
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::RuntimeParametric {
                base: "Vararg".to_string(),
                params: vec![binder.clone()],
            }])),
        };

        let core_wrapper = CoreType::from(&wrapper);
        let CoreType::UnionAll { var, body } = core_wrapper else {
            return Err("runtime wrapper must remain a structural UnionAll".to_string());
        };
        let CoreType::Tuple(elements) = body.as_ref() else {
            return Err("runtime tuple body must remain structural".to_string());
        };
        let [CoreType::Vararg(element)] = elements.as_slice() else {
            return Err("runtime Vararg must not degrade to an invariant Struct".to_string());
        };
        let CoreType::TypeVar(element_var) = element.as_ref() else {
            return Err("Vararg element must reference the enclosing binder".to_string());
        };
        assert_eq!(element_var.scope_id, var.scope_id);

        let standalone = JuliaType::RuntimeParametric {
            base: "Vararg".to_string(),
            params: vec![JuliaType::Int64, JuliaType::Struct("3".to_string())],
        };
        let standalone = CoreType::from(&standalone);
        let CoreType::VarargLen { element, len } = standalone else {
            return Err("standalone runtime Vararg{T,N} must use canonical VarargLen".to_string());
        };
        assert_eq!(element.as_ref(), &CoreType::Primitive(CorePrimitive::Int64));
        assert_eq!(
            len.as_ref(),
            &CoreType::Value(crate::CoreValueParam::Int(3))
        );

        let free_element = JuliaType::RuntimeTypeVar {
            id: 10461,
            name: "E".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let direct_vararg = CoreType::from(&JuliaType::RuntimeParametric {
            base: "Vararg".to_string(),
            params: vec![free_element],
        });
        let CoreType::Vararg(element) = direct_vararg else {
            return Err("direct runtime Vararg must use the canonical shape".to_string());
        };
        let CoreType::TypeVar(element) = element.as_ref() else {
            return Err("direct runtime Vararg element must retain TypeVar identity".to_string());
        };
        assert_eq!(element.rigid_identity, Some(10461));

        let wrapped_len = |free_len_id| JuliaType::RuntimeUnionAll {
            var: Box::new(binder.clone()),
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::RuntimeParametric {
                base: "Vararg".to_string(),
                params: vec![
                    binder.clone(),
                    JuliaType::RuntimeTypeVar {
                        id: free_len_id,
                        name: "N".to_string(),
                        lower_bound: Box::new(JuliaType::Bottom),
                        upper_bound: Box::new(JuliaType::Any),
                    },
                ],
            }])),
        };
        let first = CoreType::from(&wrapped_len(10462));
        let second = CoreType::from(&wrapped_len(10463));
        let CoreType::UnionAll { var, body } = &first else {
            return Err("wrapped VarargLen must preserve the enclosing UnionAll".to_string());
        };
        let CoreType::Tuple(elements) = body.as_ref() else {
            return Err("wrapped VarargLen owner must remain a tuple".to_string());
        };
        let [CoreType::VarargLen { element, len }] = elements.as_slice() else {
            return Err("wrapped runtime Vararg{T,N} must use VarargLen".to_string());
        };
        let CoreType::TypeVar(element_var) = element.as_ref() else {
            return Err(
                "wrapped VarargLen element must reference the enclosing binder".to_string(),
            );
        };
        let CoreType::TypeVar(len_var) = len.as_ref() else {
            return Err("wrapped VarargLen length must retain its free runtime ID".to_string());
        };
        assert_eq!(element_var.scope_id, var.scope_id);
        assert_eq!(len_var.rigid_identity, Some(10462));
        assert_ne!(first, second);
        Ok(())
    }
}
