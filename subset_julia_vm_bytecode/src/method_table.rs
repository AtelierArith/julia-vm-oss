//! Method table for multiple dispatch support.
//!
//! Contains MethodSig and MethodTable structures for tracking function
//! methods with type information and performing dispatch. Owned by the shared
//! bytecode crate since Issue #9090 / Issue #8656: the compiler builds and
//! persists these tables (Base cache), while the VM re-loads them for runtime
//! dispatch and reflection. `compile::method_table` remains as a re-export shim
//! for existing compile-side users.

// SAFETY: i32→u32 cast at score computation is guarded by `.max(0)` before the cast.
#![allow(clippy::cast_sign_loss)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ValueType;
use subset_julia_vm_types::inference_core::{
    dispatch_resolver, selection, specificity, CoreAbstract, CoreSubtypeEngine, CoreType,
    CoreTypeVar,
};
use subset_julia_vm_types::ir::core::Function;
use subset_julia_vm_types::types::{
    nominal_family_name, DispatchError, JuliaType, StructHierarchy, TypeExpr, TypeParam,
};

/// A method signature with type information.
///
/// **`core_signature` is the canonical type representation** (Issue #6336):
/// it is the only persisted and in-memory type information for the method
/// signature (plus display-only parameter names), exactly mirroring upstream
/// Julia's `Tuple{...}`-wrapped-in-`UnionAll` method signatures. The historical
/// `params` / `type_params` JuliaType projections were deleted in Issue #6495;
/// callers that still need a JuliaType view reconstruct it cold from
/// `core_signature` through the canonical inverse.
#[derive(Debug, Clone)]
pub struct MethodSig {
    /// Index into the methods list for this function name.
    pub _method_index: usize,
    /// Global function index (for bytecode).
    pub global_index: usize,
    /// Display-only parameter names. Declared types live solely in
    /// `core_signature`.
    pub param_names: Vec<String>,
    /// Inferred return type.
    pub return_type: ValueType,
    /// Parametric return type that preserves element-level type info (Issue #2317).
    /// `ValueType::Tuple` loses element types; this field carries `JuliaType::TupleOf(...)`
    /// when the abstract interpretation engine infers a parametric tuple return type.
    pub return_julia_type: Option<JuliaType>,
    /// True if this method extends a Base operator (e.g., `function Base.:+(...)`)
    /// Base extension methods do NOT shadow builtin operators for primitive types.
    pub is_base_extension: bool,
    /// Complete callable-self pattern used after the owning `MethodTable` has
    /// selected this row as an explicit-parametric inner constructor.
    pub explicit_constructor_type_parameter_names: Vec<String>,
    pub explicit_constructor_type_arguments: Vec<TypeExpr>,
    pub explicit_constructor_type_name: Option<String>,
    /// Canonical structured signature: `Tuple{argtypes...}` wrapped by one
    /// `UnionAll` per `where` parameter.
    pub core_signature: CoreType,
    /// Index of varargs parameter (if any). For `f(a, args...)`, this would be Some(1).
    pub vararg_param_index: Option<usize>,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    pub vararg_fixed_count: Option<usize>,
}

/// Stable identity of one method slot in a generic function (Issue #9784).
///
/// Positional function indices and display-only parameter names are excluded:
/// they change whenever a REPL delta appends a replacement body. The signature
/// uses exactly the same semantic canonicalization as [`MethodTable::add_method`]
/// so two definitions that obey Julia's last-definition-wins rule also compare
/// equal here.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplMethodIdentity {
    function: String,
    core_signature: CoreType,
    vararg_param_index: Option<usize>,
    vararg_fixed_count: Option<usize>,
}

impl ReplMethodIdentity {
    pub fn from_method_sig(function: &str, sig: &MethodSig) -> Self {
        Self {
            function: function.to_string(),
            core_signature: sig.canonical_signature_for_dedup(),
            vararg_param_index: sig.vararg_param_index,
            vararg_fixed_count: sig.vararg_fixed_count,
        }
    }

    pub fn from_function(function: &str, source: &Function) -> Self {
        let params = source
            .params
            .iter()
            .map(|param| (param.name.clone(), param.effective_type()))
            .collect::<Vec<_>>();
        let core_signature =
            MethodSig::compute_core_signature_from_julia_projections(&params, &source.type_params);
        Self::from_parts(
            function.to_string(),
            core_signature,
            source.params.iter().position(|param| param.is_varargs),
            source.params.iter().find_map(|param| param.vararg_count),
        )
    }

    pub fn from_parts(
        function: String,
        core_signature: CoreType,
        vararg_param_index: Option<usize>,
        vararg_fixed_count: Option<usize>,
    ) -> Self {
        let core_signature =
            canonical_method_identity_signature(&core_signature, vararg_param_index);
        Self {
            function,
            core_signature,
            vararg_param_index,
            vararg_fixed_count,
        }
    }

    pub fn function(&self) -> &str {
        &self.function
    }

    pub fn core_signature(&self) -> &CoreType {
        &self.core_signature
    }

    pub fn vararg_param_index(&self) -> Option<usize> {
        self.vararg_param_index
    }

    pub fn vararg_fixed_count(&self) -> Option<usize> {
        self.vararg_fixed_count
    }

    pub fn matches(&self, function: &str, sig: &MethodSig) -> bool {
        self == &Self::from_method_sig(function, sig)
    }
}

/// Why a `MethodSig`'s structured signature projection is unavailable
/// (Issue #8549).
///
/// Production signatures are total: [`MethodSig::from_julia_projections`]
/// derives the canonical `Tuple{...}`-in-`UnionAll` signature structurally,
/// and deserialization rejects any wire payload carrying a defect. Every
/// variant is therefore confined to hidden placeholder constructors used by
/// cross-crate tests ([`MethodSig::bottom_for_tests`] /
/// [`MethodSig::for_tests`] with an explicit non-canonical signature); the
/// dispatch fallback branches
/// that consume [`MethodSig::structured_arg_core_types`] `== None` document
/// their conservative default against this classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureDefect {
    /// `core_signature` is the test-only `Bottom` placeholder.
    BottomPlaceholder,
    /// Stripping the `where` `UnionAll` wrappers does not reach a `Tuple`
    /// (a non-canonical signature shape that no production path produces).
    NonTupleBody,
    /// The canonical argument tuple arity disagrees with the declared
    /// parameter-name row (a stale/skewed projection).
    ArgCountSkew { projected: usize, declared: usize },
}

/// Wire format of [`MethodSig`] (Issue #6336, CACHE_VERSION 45): the type
/// information is carried ONLY by the canonical `core_signature`; parameter
/// names are display data.
#[derive(Serialize, Deserialize)]
struct MethodSigWire {
    _method_index: usize,
    global_index: usize,
    param_names: Vec<String>,
    return_type: ValueType,
    return_julia_type: Option<JuliaType>,
    is_base_extension: bool,
    explicit_constructor_type_parameter_names: Vec<String>,
    explicit_constructor_type_arguments: Vec<TypeExpr>,
    explicit_constructor_type_name: Option<String>,
    core_signature: CoreType,
    vararg_param_index: Option<usize>,
    vararg_fixed_count: Option<usize>,
}

impl Serialize for MethodSig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let core_signature = self.core_signature();
        MethodSigWire {
            _method_index: self._method_index,
            global_index: self.global_index,
            param_names: self.param_names.clone(),
            return_type: self.return_type.clone(),
            return_julia_type: self.return_julia_type.clone(),
            is_base_extension: self.is_base_extension,
            explicit_constructor_type_parameter_names: self
                .explicit_constructor_type_parameter_names
                .clone(),
            explicit_constructor_type_arguments: self.explicit_constructor_type_arguments.clone(),
            explicit_constructor_type_name: self.explicit_constructor_type_name.clone(),
            core_signature,
            vararg_param_index: self.vararg_param_index,
            vararg_fixed_count: self.vararg_fixed_count,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MethodSig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = MethodSigWire::deserialize(deserializer)?;

        let sig = Self {
            _method_index: wire._method_index,
            global_index: wire.global_index,
            param_names: wire.param_names,
            return_type: wire.return_type,
            return_julia_type: wire.return_julia_type,
            is_base_extension: wire.is_base_extension,
            explicit_constructor_type_parameter_names: wire
                .explicit_constructor_type_parameter_names,
            explicit_constructor_type_arguments: wire.explicit_constructor_type_arguments,
            explicit_constructor_type_name: wire.explicit_constructor_type_name,
            core_signature: wire.core_signature,
            vararg_param_index: wire.vararg_param_index,
            vararg_fixed_count: wire.vararg_fixed_count,
        };
        // Production signatures are total (Issue #8549): a wire payload whose
        // canonical signature is the `Bottom` placeholder, a non-tuple shape,
        // or arity-skewed against the parameter-name row would silently
        // degrade dispatch to the conservative fallbacks. Reject it loudly
        // instead; the cache loader surfaces the error and rebuilds from
        // source.
        if let Some(defect) = sig.signature_defect() {
            return Err(serde::de::Error::custom(format!(
                "MethodSig wire payload for global function #{} carries a \
                 non-canonical core signature ({defect:?}); production \
                 signatures must be total (Issue #8549)",
                sig.global_index
            )));
        }
        Ok(sig)
    }
}

fn core_typevar_contains_rigid_identity(var: &CoreTypeVar) -> bool {
    var.rigid_identity.is_some()
        || var
            .lower_bound
            .as_deref()
            .is_some_and(core_contains_rigid_typevar)
        || var
            .upper_bound
            .as_deref()
            .is_some_and(core_contains_rigid_typevar)
}

fn core_contains_rigid_typevar(core: &CoreType) -> bool {
    match core {
        CoreType::TypeVar(var) => core_typevar_contains_rigid_identity(var),
        CoreType::UnionAll { var, body } => {
            core_typevar_contains_rigid_identity(var) || core_contains_rigid_typevar(body)
        }
        CoreType::AbstractUser { parent, .. } => {
            parent.as_deref().is_some_and(core_contains_rigid_typevar)
        }
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().any(core_contains_rigid_typevar)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, field)| core_contains_rigid_typevar(field)),
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => core_contains_rigid_typevar(inner),
        CoreType::VarargLen { element, len } => {
            core_contains_rigid_typevar(element) || core_contains_rigid_typevar(len)
        }
        _ => false,
    }
}

fn project_method_core_type(core: &CoreType) -> JuliaType {
    if !core_contains_rigid_typevar(core) {
        return subset_julia_vm_types::inference_core::core_type_to_julia_type(core);
    }
    match core {
        CoreType::TypeVar(var) if var.rigid_identity.is_some() => JuliaType::RuntimeTypeVar {
            id: var.rigid_identity.expect("guarded rigid MethodSig TypeVar"),
            name: var.name.clone(),
            lower_bound: Box::new(
                var.lower_bound
                    .as_deref()
                    .map(project_method_core_type)
                    .unwrap_or(JuliaType::Bottom),
            ),
            upper_bound: Box::new(
                var.upper_bound
                    .as_deref()
                    .map(project_method_core_type)
                    .unwrap_or(JuliaType::Any),
            ),
        },
        CoreType::Tuple(elements) => {
            JuliaType::TupleOf(elements.iter().map(project_method_core_type).collect())
        }
        CoreType::Union(arms) => {
            JuliaType::Union(arms.iter().map(project_method_core_type).collect())
        }
        CoreType::TypeOf(inner) => JuliaType::TypeOf(Box::new(project_method_core_type(inner))),
        CoreType::Vararg(inner) => JuliaType::RuntimeParametric {
            base: "Vararg".to_string(),
            params: vec![project_method_core_type(inner)],
        },
        CoreType::VarargLen { element, len } => JuliaType::RuntimeParametric {
            base: "Vararg".to_string(),
            params: vec![
                project_method_core_type(element),
                project_method_core_type(len),
            ],
        },
        CoreType::UnionAll { var, body } if var.rigid_identity.is_some() => {
            JuliaType::RuntimeUnionAll {
                var: Box::new(project_method_core_type(&CoreType::TypeVar(var.clone()))),
                body: Box::new(project_method_core_type(body)),
            }
        }
        CoreType::UnionAll { var, .. } if core_typevar_contains_rigid_identity(var) => {
            JuliaType::Any
        }
        CoreType::UnionAll { var, body } => JuliaType::UnionAll {
            var: var.name.clone(),
            lower_bound: var
                .lower_bound
                .as_deref()
                .map(|bound| Box::new(bound.to_julia_name())),
            bound: var
                .upper_bound
                .as_deref()
                .map(|bound| Box::new(bound.to_julia_name())),
            body: Box::new(project_method_core_type(body)),
        },
        CoreType::Struct { name, params } => JuliaType::from_structured_parametric(
            name.clone(),
            params.iter().map(project_method_core_type).collect(),
        ),
        // `core_contains_rigid_typevar` is true here, but the remaining
        // compatibility carriers cannot encode that nested identity without a
        // string round trip (NamedTuple fields, AbstractUser parents, or a
        // non-rigid binder's structured bounds). Keep the projection
        // conservative; `core_signature` remains the sole semantic source.
        // Identity-capable carriers are tracked in Issue #11590.
        _ => JuliaType::Any,
    }
}

fn canonical_method_identity_signature(
    core_signature: &CoreType,
    vararg_param_index: Option<usize>,
) -> CoreType {
    let mut core_vars = Vec::new();
    let mut body = core_signature;
    while let CoreType::UnionAll { var, body: inner } = body {
        core_vars.push(var.clone());
        body = inner;
    }
    let CoreType::Tuple(params) = body else {
        return core_signature.clone();
    };
    if vararg_param_index.is_some()
        && dispatch_resolver::core_match::vararg_param_mentions_type_vars_core(
            params,
            &core_vars,
            vararg_param_index,
        )
    {
        // A single `T` occurrence in a vararg slot is semantically repeated
        // across all vararg arguments, so it is not equivalent to its bound.
        return core_signature.clone();
    }
    core_signature.canonicalize_signature_for_dedup()
}

impl MethodSig {
    fn canonicalize_constructor_self_type_expr(
        expr: &TypeExpr,
        binder_names: &HashMap<&str, String>,
    ) -> TypeExpr {
        match expr {
            TypeExpr::TypeVar(name) => binder_names
                .get(name.as_str())
                .cloned()
                .map(TypeExpr::TypeVar)
                .unwrap_or_else(|| expr.clone()),
            TypeExpr::Parameterized { base, params }
                if matches!(base.as_str(), "Vector" | "Base.Vector") =>
            {
                let mut params: Vec<TypeExpr> = params
                    .iter()
                    .map(|param| Self::canonicalize_constructor_self_type_expr(param, binder_names))
                    .collect();
                params.push(TypeExpr::TypeVar("1".to_string()));
                TypeExpr::Parameterized {
                    base: "Array".to_string(),
                    params,
                }
            }
            TypeExpr::Parameterized { base, params }
                if matches!(base.as_str(), "Matrix" | "Base.Matrix") =>
            {
                let mut params: Vec<TypeExpr> = params
                    .iter()
                    .map(|param| Self::canonicalize_constructor_self_type_expr(param, binder_names))
                    .collect();
                params.push(TypeExpr::TypeVar("2".to_string()));
                TypeExpr::Parameterized {
                    base: "Array".to_string(),
                    params,
                }
            }
            TypeExpr::Parameterized { base, params } => TypeExpr::Parameterized {
                base: if base == "Base.Array" {
                    "Array".to_string()
                } else {
                    base.clone()
                },
                params: params
                    .iter()
                    .map(|param| Self::canonicalize_constructor_self_type_expr(param, binder_names))
                    .collect(),
            },
            TypeExpr::Concrete(JuliaType::VectorOf(element)) => TypeExpr::Parameterized {
                base: "Array".to_string(),
                params: vec![
                    TypeExpr::Concrete((**element).clone()),
                    TypeExpr::TypeVar("1".to_string()),
                ],
            },
            TypeExpr::Concrete(JuliaType::MatrixOf(element)) => TypeExpr::Parameterized {
                base: "Array".to_string(),
                params: vec![
                    TypeExpr::Concrete((**element).clone()),
                    TypeExpr::TypeVar("2".to_string()),
                ],
            },
            TypeExpr::Concrete(JuliaType::Struct(name))
                if name.chars().all(|ch| ch.is_ascii_digit()) =>
            {
                TypeExpr::TypeVar(name.clone())
            }
            TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => expr.clone(),
        }
    }

    /// Reconstruct the method signature Julia uses for constructor identity by
    /// prepending the implicit `Type{Owner{self...}}` argument to the projected
    /// value arguments. Keeping the original `UnionAll` environment makes
    /// self-binder bounds and dependencies participate in semantic equality;
    /// canonical value-signature rules still collapse only genuinely
    /// equivalent covariant spellings (Issue #11019).
    fn constructor_signature_for_dedup(&self) -> CoreType {
        let self_params: Vec<CoreType> = self
            .explicit_constructor_type_arguments
            .iter()
            .map(|expr| {
                CoreType::from(&Self::canonicalize_constructor_self_type_expr(
                    expr,
                    &HashMap::new(),
                ))
            })
            .collect();
        let self_owner = self
            .explicit_constructor_type_name
            .clone()
            .unwrap_or_else(|| "#constructor-self".to_string());
        let mut elements = vec![CoreType::TypeOf(Box::new(CoreType::Struct {
            name: self_owner,
            params: self_params,
        }))];
        elements.extend_from_slice(self.structured_arg_core_types().unwrap_or(&[]));

        let core_vars = self.core_signature_type_vars();
        // `where` declaration order is not method identity. Rebuild the
        // `UnionAll` environment in first-use order across the omitted
        // constructor self and the value signature, so
        // `Foo{T,S}() where {T,S}` and `Foo{T,S}() where {S,T}` deduplicate,
        // while correlations such as `Foo{S,T}(x::T)` remain distinct.
        let mut ordered_names = self.explicit_constructor_type_parameter_names.clone();
        fn collect_occurrences(
            ty: &CoreType,
            declared: &[subset_julia_vm_types::inference_core::CoreTypeVar],
            ordered: &mut Vec<String>,
        ) {
            match ty {
                CoreType::TypeVar(var) | CoreType::UnionAll { var, .. }
                    if declared.iter().any(|candidate| candidate.name == var.name)
                        && !ordered.contains(&var.name) =>
                {
                    ordered.push(var.name.clone());
                }
                _ => {}
            }
            match ty {
                CoreType::AbstractUser { parent, .. } => {
                    if let Some(parent) = parent {
                        collect_occurrences(parent, declared, ordered);
                    }
                }
                CoreType::Struct { params, .. }
                | CoreType::Tuple(params)
                | CoreType::Union(params) => {
                    for param in params {
                        collect_occurrences(param, declared, ordered);
                    }
                }
                CoreType::Vararg(inner) | CoreType::TypeOf(inner) => {
                    collect_occurrences(inner, declared, ordered);
                }
                CoreType::VarargLen { element, len } => {
                    collect_occurrences(element, declared, ordered);
                    collect_occurrences(len, declared, ordered);
                }
                CoreType::NamedTuple(fields) => {
                    for (_, field_type) in fields {
                        collect_occurrences(field_type, declared, ordered);
                    }
                }
                CoreType::UnionAll { body, .. } => {
                    collect_occurrences(body, declared, ordered);
                }
                CoreType::Bottom
                | CoreType::Any
                | CoreType::Primitive(_)
                | CoreType::Abstract(_)
                | CoreType::TypeVar(_)
                | CoreType::Value(_)
                | CoreType::Module(_)
                | CoreType::Named(_) => {}
            }
        }
        for argument in self.structured_arg_core_types().unwrap_or(&[]) {
            collect_occurrences(argument, &core_vars, &mut ordered_names);
        }
        for var in &core_vars {
            if !ordered_names.contains(&var.name) {
                ordered_names.push(var.name.clone());
            }
        }
        let ordered_core_vars: Vec<_> = ordered_names
            .iter()
            .filter_map(|name| core_vars.iter().find(|var| var.name == *name).cloned())
            .collect();
        let mut signature = CoreType::Tuple(elements);
        for var in ordered_core_vars.iter().rev() {
            signature = CoreType::UnionAll {
                var: var.clone(),
                body: Box::new(signature),
            };
        }
        if self.vararg_param_index.is_some()
            && dispatch_resolver::core_match::vararg_param_mentions_type_vars_core(
                self.structured_arg_core_types().unwrap_or(&[]),
                &core_vars,
                self.vararg_param_index,
            )
        {
            signature
        } else {
            signature.canonicalize_signature_for_dedup()
        }
    }

    /// Structured method signature bridge for the shared type core.
    ///
    /// The shape is `Tuple{argtypes...}` wrapped by one `UnionAll` per
    /// `where` type parameter. Existing dispatch still uses the historical
    /// fields while #3828 migrates incrementally, but new code can inspect a
    /// single structured signature instead of recombining `params` and
    /// `type_params` ad hoc.
    #[allow(dead_code)]
    pub fn core_signature(&self) -> CoreType {
        self.core_signature.clone()
    }

    /// Builds a `MethodSig` from lowering-produced JuliaType projections and
    /// immediately discards those projections after deriving the canonical
    /// `core_signature` (Issue #6495, stage 7c-ii-b).
    ///
    /// Every production construction site goes through this constructor
    /// (deserialization additionally validates the wire payload), so a
    /// `Bottom` placeholder signature is never observable outside tests —
    /// enforced by `#[cfg(test)]` on the placeholder constructors and by the
    /// totality assertion below (Issue #8549). This is the single production
    /// construction path that accepts JuliaType projections.
    #[allow(clippy::too_many_arguments)]
    pub fn from_julia_projections(
        method_index: usize,
        global_index: usize,
        params: Vec<(String, JuliaType)>,
        return_type: ValueType,
        return_julia_type: Option<JuliaType>,
        is_base_extension: bool,
        type_params: Vec<TypeParam>,
        vararg_param_index: Option<usize>,
        vararg_fixed_count: Option<usize>,
    ) -> Self {
        let core_signature =
            Self::compute_core_signature_from_julia_projections(&params, &type_params);
        let param_names = params.into_iter().map(|(name, _)| name).collect();
        let sig = Self {
            _method_index: method_index,
            global_index,
            param_names,
            return_type,
            return_julia_type,
            is_base_extension,
            explicit_constructor_type_parameter_names: Vec::new(),
            explicit_constructor_type_arguments: Vec::new(),
            explicit_constructor_type_name: None,
            core_signature,
            vararg_param_index,
            vararg_fixed_count,
        };
        // Canonicalization is total by construction (`Tuple` over the
        // parameter row, one `UnionAll` per `where` param); assert it so a
        // future edit that breaks totality fails loudly instead of degrading
        // dispatch through the conservative fallbacks (Issue #8549). Hidden
        // cross-crate test constructors are the only non-total construction
        // path.
        debug_assert!(
            sig.signature_defect().is_none(),
            "from_julia_projections produced a defective canonical signature: {:?}",
            sig.signature_defect()
        );
        sig
    }

    pub fn param_count(&self) -> usize {
        self.param_names.len()
    }

    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn for_tests(
        method_index: usize,
        global_index: usize,
        params: Vec<(String, JuliaType)>,
        return_type: ValueType,
        return_julia_type: Option<JuliaType>,
        is_base_extension: bool,
        type_params: Vec<TypeParam>,
        core_signature: CoreType,
        vararg_param_index: Option<usize>,
        vararg_fixed_count: Option<usize>,
    ) -> Self {
        let derived_core_signature =
            Self::compute_core_signature_from_julia_projections(&params, &type_params);
        let param_names = params.into_iter().map(|(name, _)| name).collect();
        Self {
            _method_index: method_index,
            global_index,
            param_names,
            return_type,
            return_julia_type,
            is_base_extension,
            explicit_constructor_type_parameter_names: Vec::new(),
            explicit_constructor_type_arguments: Vec::new(),
            explicit_constructor_type_name: None,
            core_signature: if matches!(core_signature, CoreType::Bottom) {
                derived_core_signature
            } else {
                core_signature
            },
            vararg_param_index,
            vararg_fixed_count,
        }
    }

    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn bottom_for_tests(
        method_index: usize,
        global_index: usize,
        params: Vec<(String, JuliaType)>,
        return_type: ValueType,
        return_julia_type: Option<JuliaType>,
        is_base_extension: bool,
        vararg_param_index: Option<usize>,
        vararg_fixed_count: Option<usize>,
    ) -> Self {
        Self {
            _method_index: method_index,
            global_index,
            param_names: params.into_iter().map(|(name, _)| name).collect(),
            return_type,
            return_julia_type,
            is_base_extension,
            explicit_constructor_type_parameter_names: Vec::new(),
            explicit_constructor_type_arguments: Vec::new(),
            explicit_constructor_type_name: None,
            core_signature: CoreType::Bottom,
            vararg_param_index,
            vararg_fixed_count,
        }
    }

    /// Argument core types projected from the structured `core_signature`
    /// (`where` `UnionAll` wrappers stripped).
    ///
    /// Issue #6336/#6495: this is the accessor dispatch readers use instead
    /// of reading stored JuliaType projections. Returns an empty slice for a
    /// test-only `Bottom` placeholder; production methods carry a structured
    /// signature.
    pub fn arg_core_types(&self) -> &[CoreType] {
        let mut sig = &self.core_signature;
        while let CoreType::UnionAll { body, .. } = sig {
            sig = body;
        }
        match sig {
            CoreType::Tuple(elems) => elems.as_slice(),
            _ => &[],
        }
    }

    /// Whether this method came from the Base/prelude portion of the merged IR.
    ///
    /// `is_base_extension` only records syntactic extension of a Base-qualified
    /// function; it is not an origin marker. The compiler preserves origin as
    /// the global function index relative to `Program::base_function_count`, so
    /// #5926 dispatch fences should use this helper when they need Base/user
    /// visibility.
    #[allow(dead_code)]
    pub fn is_base_program_method(&self, base_function_count: usize) -> bool {
        self.global_index < base_function_count
    }

    /// Whether this method can accept a call with `arg_len` positional
    /// arguments (same fixed-arity / vararg rules as the historical
    /// `runtime_type_names_for_arity` string baking, without rendering any
    /// type-name strings). The `CallTypedDispatch`-family payload builders
    /// use this as their arity gate now that candidate payloads carry only
    /// function indices (Issue #6496).
    pub fn accepts_arity(&self, arg_len: usize) -> bool {
        if let Some(vararg_idx) = self.vararg_param_index {
            if arg_len < vararg_idx {
                return false;
            }
            match self.vararg_fixed_count {
                Some(fixed_count) => arg_len == vararg_idx + fixed_count,
                None => true,
            }
        } else {
            self.param_count() == arg_len
        }
    }

    /// Expanded per-arity argument core types projected from the canonical
    /// `core_signature`, using the same fixed-arity and vararg-expansion rules
    /// as dispatch matching (Issue #6495, stage 1).
    ///
    /// This is the CoreType-native source the dispatch matcher migrates onto:
    /// fixed positions map to their declared argument core type, and a
    /// trailing vararg slot is repeated across the remaining call-site
    /// arguments (`compute_core_signature` renders `args...` as the declared
    /// element type inside the argument tuple, so the expansion rules are
    /// identical to the declared parameter row). Returns `None` when the arity
    /// is not accepted, or when the
    /// structured signature is unavailable/skewed (`Bottom` placeholder whose
    /// projection length disagrees with the declared parameter count).
    ///
    /// The accessor-vs-canonical invariant over the whole Base corpus is pinned
    /// by `compile::cache::tests::`
    /// `base_method_signature_accessors_are_canonical_issue_6495`.
    pub fn expanded_core_param_types_for_arity(&self, arg_len: usize) -> Option<Vec<CoreType>> {
        // Structured signature not refreshed (test-only `Bottom` placeholder)
        // or skewed: no CoreType projection available (Issue #8549 —
        // `signature_defect` is the shared availability gate).
        let core_params = self.structured_arg_core_types()?;
        if let Some(vararg_idx) = self.vararg_param_index {
            if let Some(fixed_count) = self.vararg_fixed_count {
                // Vararg{T, N}: exactly vararg_idx fixed params + N varargs.
                if arg_len != vararg_idx + fixed_count {
                    return None;
                }
            } else if arg_len < vararg_idx {
                // Varargs: need at least vararg_idx fixed args.
                return None;
            }
            let vararg_ty = core_params
                .get(vararg_idx)
                .cloned()
                .unwrap_or(CoreType::Any);
            let mut expanded: Vec<CoreType> =
                core_params.iter().take(vararg_idx).cloned().collect();
            for _ in vararg_idx..arg_len {
                expanded.push(vararg_ty.clone());
            }
            Some(expanded)
        } else {
            // No varargs: exact arity required.
            if core_params.len() != arg_len {
                return None;
            }
            Some(core_params.to_vec())
        }
    }

    /// The method's `where` clause as structured [`CoreTypeVar`]s, projected
    /// from the canonical `core_signature` `UnionAll` wrappers (outermost
    /// first, i.e. declaration order — `compute_core_signature` wraps the
    /// argument tuple in reverse declaration order). This is the
    /// CoreType-native counterpart of the `type_params` projection consumed
    /// by `dispatch_resolver::core_match` (Issue #6495, stage 2). Returns an
    /// empty vector when the structured signature is unavailable (`Bottom`
    /// placeholder), which coincides with `arg_core_types()` returning an
    /// empty slice.
    ///
    /// Production consumer since the stage-3 flip: [`method_match_binding_count`]
    /// feeds it to `dispatch_resolver::core_match` (Issue #6495).
    pub fn core_signature_type_vars(
        &self,
    ) -> Vec<subset_julia_vm_types::inference_core::CoreTypeVar> {
        let mut vars = Vec::new();
        let mut sig = &self.core_signature;
        while let CoreType::UnionAll { var, body } = sig {
            vars.push(var.clone());
            sig = body;
        }
        vars
    }

    fn canonical_signature_for_dedup(&self) -> CoreType {
        canonical_method_identity_signature(&self.core_signature, self.vararg_param_index)
    }

    /// Classify why the structured signature projection is unavailable, or
    /// `None` for a canonical (total) signature (Issue #8549).
    ///
    /// This is the single availability gate behind
    /// [`Self::structured_arg_core_types`] /
    /// [`Self::expanded_core_param_types_for_arity`] /
    /// [`CoreDominanceInputs::for_matches`]. Production signatures never
    /// carry a defect: [`Self::from_julia_projections`] derives the canonical
    /// signature structurally (asserted there), and deserialization rejects
    /// defective wire payloads, so every variant is confined to the
    /// `#[cfg(test)]` placeholder constructors.
    pub fn signature_defect(&self) -> Option<SignatureDefect> {
        let mut sig = &self.core_signature;
        while let CoreType::UnionAll { body, .. } = sig {
            sig = body;
        }
        match sig {
            CoreType::Tuple(elems) if elems.len() == self.param_count() => None,
            CoreType::Tuple(elems) => Some(SignatureDefect::ArgCountSkew {
                projected: elems.len(),
                declared: self.param_count(),
            }),
            CoreType::Bottom => Some(SignatureDefect::BottomPlaceholder),
            _ => Some(SignatureDefect::NonTupleBody),
        }
    }

    /// The unexpanded `core_signature`-projected argument core types, or
    /// `None` when the structured signature carries a [`SignatureDefect`]
    /// (test-only `Bottom` placeholder, non-tuple body, or a projection whose
    /// arity disagrees with the declared parameter count) — the same
    /// availability guard as [`Self::expanded_core_param_types_for_arity`]
    /// and [`CoreDominanceInputs::for_matches`]. Dispatch consumers treat
    /// `None` as the test-only placeholder and use conservative defaults
    /// (Issues #6495, #8549).
    pub fn structured_arg_core_types(&self) -> Option<&[CoreType]> {
        if self.signature_defect().is_some() {
            return None;
        }
        Some(self.arg_core_types())
    }

    /// The declared `JuliaType` of parameter `idx`, sourced from the canonical
    /// `core_signature` projection through the canonical inverse
    /// (`inference_core::core_type_to_julia_type`) — Issue #6495, stage 6b-ii.
    /// For deserialized tables the reconstruction equals the lowering-produced
    /// spelling by the #6336 round-trip gates; for in-session tables it can
    /// differ only on non-canonical user spellings (pinned over the Base
    /// corpus by
    /// `compile::cache::tests::base_method_core_binary_heuristics_parity_issue_6495`).
    ///
    /// Stage 7c-ii: the legacy `params` fallback is retired — `None` (a
    /// test-only `Bottom` placeholder, unobservable in production since stage
    /// 7b) reconstructs the unconstrained `Any`.
    ///
    /// Cold-path accessor: reconstructing allocates, so per-method hot filter
    /// loops use dedicated CoreType-native predicates instead.
    pub fn projected_param_julia_type(&self, idx: usize) -> std::borrow::Cow<'_, JuliaType> {
        match self.structured_arg_core_types() {
            Some(cores) => std::borrow::Cow::Owned(project_method_core_type(&cores[idx])),
            None => std::borrow::Cow::Owned(JuliaType::Any),
        }
    }

    /// Specificity of declared parameter `idx`, read from the
    /// `core_signature` projection (Issue #6495, stage 6b-ii).
    /// `JuliaType::specificity` already evaluates `CoreType::from(self)
    /// .specificity()`, so by the stage-6a elementwise image invariant
    /// (`arg_core_types()[i] == CoreType::from(&params[i].1)`, gated by
    /// `base_method_core_tiebreaker_parity_issue_6495`) the projection read is
    /// provably identical to the retired legacy read.
    ///
    /// Stage 7c-ii: the legacy `params` fallback is retired — `None` (a
    /// test-only `Bottom` placeholder) reports the unconstrained specificity 0.
    pub fn param_specificity(&self, idx: usize) -> u8 {
        match self.structured_arg_core_types() {
            Some(cores) => cores[idx].specificity(),
            None => 0,
        }
    }

    /// True when every declared parameter has specificity 0 (an unconstrained
    /// catch-all signature) — same projection sourcing as
    /// [`Self::param_specificity`] (Issue #6495, stage 6b-ii).
    ///
    /// Stage 7c-ii: the legacy `params` fallback is retired — `None` (a
    /// test-only `Bottom` placeholder) conservatively reports `false` so the
    /// catch-all special paths never trigger on a placeholder.
    pub fn all_params_specificity_zero(&self) -> bool {
        match self.structured_arg_core_types() {
            Some(cores) => cores.iter().all(|core| core.specificity() == 0),
            None => false,
        }
    }

    /// Allocation-free count of the `where` type parameters projected from
    /// the canonical `core_signature` `UnionAll` wrappers — the
    /// CoreType-native counterpart of `type_params.len()` consumed by the
    /// `dispatch_inner` fewest-`where`-params tie-breaker (Issue #6495,
    /// stage 6a). Equal to `type_params.len()` whenever the structured
    /// signature is available: at build time `compute_core_signature` wraps
    /// one `UnionAll` per `type_params` entry, and at deserialization
    /// `type_params` is reconstructed one entry per wrapper.
    pub fn core_signature_type_var_count(&self) -> usize {
        let mut count = 0usize;
        let mut sig = &self.core_signature;
        while let CoreType::UnionAll { body, .. } = sig {
            count += 1;
            sig = body;
        }
        count
    }

    /// Core-projection predicate read of the declared parameter governing
    /// call argument `position`, with the same projection-sourcing rules as
    /// [`Self::param_matches_at`]: `false` when the mapped slot is out of
    /// range or the structured signature is unavailable (Issue #6495,
    /// stages 7a/7c-ii).
    pub fn param_matches_at_call_position(
        &self,
        position: usize,
        core_pred: impl FnOnce(&CoreType) -> bool,
    ) -> bool {
        let slot = match self.vararg_param_index {
            Some(vararg_idx) if position >= vararg_idx => vararg_idx,
            _ => position,
        };
        self.param_matches_at(slot, core_pred)
    }

    /// True when the method declares `where` type parameters, read from the
    /// canonical `core_signature` `UnionAll` wrappers (Issue #6495, stage
    /// 7a). Equal to the retired `!type_params.is_empty()` read whenever the
    /// structured signature is available: at build time
    /// `compute_core_signature` wraps one `UnionAll` per `type_params` entry,
    /// and at deserialization `type_params` is reconstructed one entry per
    /// wrapper.
    ///
    /// Stage 7c-ii: the legacy `type_params` fallback is retired — a
    /// test-only `Bottom` placeholder has no `UnionAll` wrappers and reports
    /// `false` (same convention as [`where_param_count`], stage 7c-i).
    pub fn has_where_params(&self) -> bool {
        self.core_signature_type_var_count() != 0
    }

    /// "Any declared parameter satisfies" read over the `core_signature`
    /// projection (Issue #6495, stage 6b-iii). The per-predicate
    /// legacy/core pairs were pinned over the Base corpus by
    /// `compile::cache::tests::base_method_core_call_dispatch_heuristics_parity_issue_6495`.
    ///
    /// Stage 7c-ii: the legacy `params` fallback is retired — `None` (a
    /// test-only `Bottom` placeholder, unobservable in production since
    /// stage 7b) conservatively reports `false`.
    pub fn any_param_matches(&self, core_pred: impl Fn(&CoreType) -> bool) -> bool {
        match self.structured_arg_core_types() {
            Some(cores) => cores.iter().any(core_pred),
            None => false,
        }
    }

    /// "Every declared parameter satisfies" read — same sourcing rules as
    /// [`Self::any_param_matches`] (Issue #6495, stage 6b-iii). Vacuously
    /// true for zero-parameter methods with a structured signature;
    /// conservatively `false` on a test-only `Bottom` placeholder (stage
    /// 7c-ii) so the all-`Any` special paths never trigger on one.
    pub fn all_params_match(&self, core_pred: impl Fn(&CoreType) -> bool) -> bool {
        match self.structured_arg_core_types() {
            Some(cores) => cores.iter().all(core_pred),
            None => false,
        }
    }

    /// Single-slot predicate read over the `core_signature` projection;
    /// `false` when `idx` is out of range (mirroring the `params.get(idx)`
    /// `Option` gates and `zip` truncation of the legacy call-heuristic
    /// readers it replaced — Issue #6495, stage 6b-iii) or when the
    /// structured signature is unavailable (a test-only `Bottom`
    /// placeholder — stage 7c-ii).
    pub fn param_matches_at(&self, idx: usize, core_pred: impl FnOnce(&CoreType) -> bool) -> bool {
        match self.structured_arg_core_types() {
            Some(cores) => cores.get(idx).is_some_and(core_pred),
            None => false,
        }
    }

    /// The full declared parameter row as `JuliaType`s, reconstructed from
    /// the `core_signature` projection through the canonical inverse like
    /// [`Self::projected_param_julia_type`]. Cold-path accessor for
    /// diagnostic payloads (ambiguity candidate listings) — Issue #6495,
    /// stage 6b-iii. Empty on a test-only `Bottom` placeholder (stage
    /// 7c-ii).
    pub fn projected_param_julia_types(&self) -> Vec<JuliaType> {
        match self.structured_arg_core_types() {
            Some(cores) => cores.iter().map(project_method_core_type).collect(),
            None => Vec::new(),
        }
    }

    fn expanded_projected_param_julia_types_for_arity(
        &self,
        arg_len: usize,
    ) -> Option<Vec<JuliaType>> {
        self.expanded_core_param_types_for_arity(arg_len)
            .map(|cores| cores.iter().map(project_method_core_type).collect())
    }

    fn projected_type_params(&self) -> Vec<TypeParam> {
        self.core_signature_type_vars()
            .iter()
            .map(subset_julia_vm_types::inference_core::core_type_var_to_type_param)
            .collect()
    }

    fn compute_core_signature_from_julia_projections(
        params: &[(String, JuliaType)],
        type_params: &[TypeParam],
    ) -> CoreType {
        let sig = CoreType::Tuple(
            params
                .iter()
                .map(|(_, ty)| {
                    subset_julia_vm_types::inference_core::dispatch_resolver::embed_type_param_bounds(
                        subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia(ty),
                        type_params,
                    )
                })
                .collect(),
        );
        let (mut sig, binders) =
            subset_julia_vm_types::inference_core::dispatch_resolver::scope_method_type_params(
                sig,
                type_params,
            );

        for var in binders.into_iter().rev() {
            sig = CoreType::UnionAll {
                var,
                body: Box::new(sig),
            };
        }
        sig
    }
}

/// Struct-hierarchy projection shared by every `MethodTable` in a compile.
///
/// The projection (`struct_parents`, `abstract_parents`, `struct_hierarchy`)
/// is derived from the program's struct/abstract definitions only — it does
/// not depend on the individual method table. Building it per table cloned
/// the full hierarchy 1100+ times per warm run (~37 ms for `println(1+1)`),
/// so compile now builds it once and shares it via `Arc` (Issue #6348).
#[derive(Debug, Default, Clone)]
pub struct MethodTableProjection {
    /// Family names declared via a parentless `abstract type ... end`.
    ///
    /// Historically the projection kept two name-keyed parent maps
    /// (`struct_parents` for #3144 and `abstract_parents` for #5056) that were
    /// pure restrictions of `struct_hierarchy`; Issue #6336 consolidates the
    /// lookups onto the shared hierarchy itself. The single piece of
    /// information the hierarchy cannot answer is *origin*: a parentless
    /// `abstract type` was never projected into the maps, so a subject lookup
    /// for it used to look unknown, and a chain lookup fell through to the
    /// built-in abstract table. This set preserves exactly that exclusion.
    /// Abstract names shadowed by a struct/parametric definition are NOT in
    /// the set (the struct projection took precedence in the old maps).
    parentless_abstract_names: std::collections::HashSet<String>,
    /// Whether the old `struct_parents`/`abstract_parents` projections would
    /// have been non-empty — i.e. the hierarchy has at least one entry other
    /// than a parentless abstract. Precomputed because dispatch consults it
    /// per candidate (the old code's O(1) `is_empty()` checks).
    has_parent_links: bool,
    /// Shared struct hierarchy: the single source of declared-parent links for
    /// both the dispatch subtype walk and the CoreType subtype helpers
    /// (Issue #6336).
    struct_hierarchy: StructHierarchy,
}

impl MethodTableProjection {
    /// Build the projection once from the program-wide hierarchy inputs.
    pub fn build(
        hierarchy: &StructHierarchy,
        concrete_struct_names: &[String],
        parametric_struct_names: &[String],
        abstract_type_names: &[String],
    ) -> Self {
        let struct_families: std::collections::HashSet<&str> = concrete_struct_names
            .iter()
            .chain(parametric_struct_names.iter())
            .map(|name| nominal_family_name(name))
            .collect();

        let parentless_abstract_names: std::collections::HashSet<String> = abstract_type_names
            .iter()
            .filter_map(|name| {
                let family = nominal_family_name(name);
                if struct_families.contains(family) {
                    return None;
                }
                matches!(hierarchy.parent_for(family), Some(None)).then(|| family.to_string())
            })
            .collect();

        let has_parent_links = hierarchy
            .iter()
            .any(|(name, _)| !parentless_abstract_names.contains(name));

        Self {
            parentless_abstract_names,
            has_parent_links,
            struct_hierarchy: hierarchy.clone(),
        }
    }

    /// Whether any declared-parent link is available for the dispatch
    /// struct-ancestry fallbacks (the old maps' `!is_empty()`).
    fn has_parent_links(&self) -> bool {
        self.has_parent_links
    }

    /// Declared parent link for a (family) type name, replicating the
    /// historical `struct_parents ∪ abstract_parents` projection lookup:
    /// `None` = unknown to the projection (try only the built-in abstract
    /// hierarchy, then reject if no known parent chain reaches the target);
    /// `Some(None)` = known root (declared without a parent);
    /// `Some(Some(parent))` = declared parent (possibly parametric, e.g.
    /// `AbsB{T}` — callers strip the parameters).
    fn declared_parent_link(&self, name: &str) -> Option<Option<String>> {
        let family = nominal_family_name(name);
        if self.parentless_abstract_names.contains(family) {
            return None;
        }
        self.struct_hierarchy.parent_for(family)
    }
}

/// Whether a canonical parameter `CoreType` is an array-like receiver
/// (`Array`/`Vector`/`Matrix` or an abstract array family). Used to detect a
/// user `getindex` override on a native array, which must disable the runtime
/// specializer's native-indexing fast path so dispatch reaches the override
/// (Issue #6657).
pub fn core_type_is_array_like(core: &CoreType) -> bool {
    match core {
        CoreType::Struct { name, .. } => {
            let base = name.split('{').next().unwrap_or(name);
            let base = base.rsplit('.').next().unwrap_or(base);
            matches!(base, "Array" | "Vector" | "Matrix")
        }
        CoreType::Abstract(a) => matches!(
            a,
            CoreAbstract::AbstractArray
                | CoreAbstract::AbstractVector
                | CoreAbstract::AbstractMatrix
                | CoreAbstract::DenseArray
        ),
        _ => false,
    }
}

/// Whether a canonical parameter `CoreType` is a parametric array-family type
/// whose element/dimension parameters are free type variables (the Base
/// `getindex(a::Array{T,N}, ...) where {T,N}` signature shape). A genuine *user*
/// `getindex` override is defined on a CONCRETE type (`Vector{Int64}`,
/// `Matrix{Float64}`, a struct), so excluding free-typevar array signatures
/// keeps Base array `getindex` out of the user-override candidate set even when
/// `base_function_count`-based origin classification is unavailable (e.g. a
/// double-merged test program). Issue #6657.
pub fn core_type_is_free_typevar_array(core: &CoreType) -> bool {
    match core {
        CoreType::Struct { name, params } => {
            let base = name.split('{').next().unwrap_or(name);
            let base = base.rsplit('.').next().unwrap_or(base);
            matches!(base, "Array" | "Vector" | "Matrix")
                && params.iter().any(|p| matches!(p, CoreType::TypeVar(_)))
        }
        // A bare `AbstractArray` / `Array` family abstract is likewise not a
        // concrete user override target.
        CoreType::Abstract(a) => matches!(
            a,
            CoreAbstract::AbstractArray
                | CoreAbstract::AbstractVector
                | CoreAbstract::AbstractMatrix
                | CoreAbstract::DenseArray
        ),
        _ => false,
    }
}

#[cfg(test)]
thread_local! {
    /// Measurement counter (Issue #9197 S5): total method candidates visited by
    /// `dispatch_inner_with_filter`'s stage-1 loop since the last reset. Used by
    /// the candidate-scan-reduction unit test only — it and its per-dispatch
    /// increment are `#[cfg(test)]`, so production dispatch pays zero overhead.
    static DISPATCH_CANDIDATE_SCANS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Total method candidates scanned by the dispatch stage-1 loop since the last
/// [`reset_dispatch_candidate_scan_count`] (Issue #9197 S5 measurement hook).
#[cfg(test)]
pub fn dispatch_candidate_scan_count() -> u64 {
    DISPATCH_CANDIDATE_SCANS.with(std::cell::Cell::get)
}

/// Reset the [`dispatch_candidate_scan_count`] counter to zero.
#[cfg(test)]
pub fn reset_dispatch_candidate_scan_count() {
    DISPATCH_CANDIDATE_SCANS.with(|c| c.set(0));
}

/// First-argument dispatch typemap index for `MethodTable`
/// (Issue #9112 primitive bucketing → Issue #9197 S5 typemap extension).
///
/// Partitions `self.methods` by their declared first parameter's nominal type
/// identity so a dispatch call visits only the methods whose first parameter
/// can match the actual first argument, instead of every method. This mirrors
/// the upstream `jl_typemap_level_t` `arg1` / `tname` split
/// (`julia/src/typemap.c`), adapted to sjulia's single-threaded VM: a method
/// declared on a concrete type sits in its own family bucket, and a method
/// declared on an abstract type is reached by walking the actual argument's
/// supertype chain to that abstract's family bucket.
///
/// Buckets each hold method indices in original insertion order (ascending), so
/// a gathered candidate list is definition-order deterministic (the Issue #8641
/// ratchet):
///
/// * `wildcard` — methods whose declared first parameter matches structurally
///   in ways a single nominal family key cannot express: `Any`, a type
///   variable, a `Union` / `UnionAll`, a `Tuple` / `NamedTuple`, `Vararg`,
///   `Type{T}`, a value parameter, `Bottom`, a `Module`, an `AbstractUser`
///   user-declared abstract, or a `Named` opaque spelling the bridge could not
///   structure. Candidates for EVERY dispatch call; always scanned.
/// * `nominal` — methods whose declared first parameter projects to a single
///   nominal family key (a sealed `Primitive`, a `Struct` family, or a builtin
///   `Abstract`). Only the buckets reachable from the actual first argument's
///   own family and supertype chain are consulted.
///
/// ## Resolve coverage (Issue #9197 S5 — landed vs deferred)
///
/// The candidate gather is subtype-complete — and therefore sound (it never
/// drops a matching method) — for a **sealed primitive** actual first argument,
/// whose supertype set is a fixed, complete chain (`Int64 <: Signed <: Integer
/// <: Real <: Number`, `Float64 <: AbstractFloat <: Real <: Number`, ...). For
/// such an argument the gather visits the primitive's own bucket, its builtin
/// abstract-supertype buckets, and `wildcard` — provably every method whose
/// first parameter is a supertype of the primitive, and no struct / container
/// method (a primitive is a subtype of none of those). Because bucketing is
/// only a candidate-set restriction and the scorer still rejects non-matches,
/// the selected method is identical to a full scan.
///
/// Any other actual first-argument kind (struct, container, abstract, `Type{T}`,
/// tuple, or a zero-argument call) falls back to a full scan, unchanged. A
/// complete abstract-container supertype enumeration for a struct argument
/// (e.g. `Vector <: AbstractVector`, a skip-level the nominal builtin walk does
/// not model) needs the structured id relation deferred to Issue #9197 S6/S7;
/// the `vm/dispatch.rs` runtime type-name parsers on that struct-argument
/// resolve path are likewise left for S6/S7.
#[derive(Debug, Clone, Default)]
struct FirstArgIndex {
    /// Methods whose declared first parameter is not a single nominal family
    /// (Any / TypeVar / Union / Tuple / Vararg / Type{T} / AbstractUser /
    /// Named / Module / value / Bottom). Always scanned.
    wildcard: Vec<usize>,
    /// Methods keyed by their declared first parameter's nominal family name
    /// (sealed `Primitive`, `Struct` family, or builtin `Abstract`).
    nominal: HashMap<String, Vec<usize>>,
}

impl FirstArgIndex {
    /// The nominal family bucket key for a declared first-parameter type, or
    /// `None` (→ `wildcard`) when the type does not project to a single nominal
    /// family that participates in the actual-argument supertype walk.
    ///
    /// `Named` and `AbstractUser` are intentionally `None`: a `Named` opaque
    /// spelling (e.g. an unresolved bare `Int` word alias) and a user-declared
    /// abstract are reached through paths the sealed-primitive supertype walk
    /// does not enumerate, so keeping them universal (wildcard) preserves the
    /// pre-#9197-S5 property that a primitive dispatch always considered them.
    fn bucket_key_for(ty: &CoreType) -> Option<String> {
        match ty {
            CoreType::Primitive(p) => Some(p.julia_name().to_string()),
            CoreType::Struct { name, .. } => Some(nominal_family_name(name).to_string()),
            CoreType::Abstract(_) => ty.builtin_type_name().map(str::to_string),
            _ => None,
        }
    }

    /// Build the index from the current ordered method list.
    fn build(methods: &[MethodSig]) -> Self {
        let mut idx = FirstArgIndex::default();
        for (method_pos, method) in methods.iter().enumerate() {
            let first_param_ty = method
                .structured_arg_core_types()
                .and_then(|params| params.first());
            match first_param_ty.and_then(Self::bucket_key_for) {
                Some(key) => idx.nominal.entry(key).or_default().push(method_pos),
                None => idx.wildcard.push(method_pos),
            }
        }
        idx
    }

    /// The complete, ascending candidate index list for a dispatch whose actual
    /// first argument is the sealed primitive `arg0`, or `None` when `arg0` is
    /// not a sealed primitive (the caller then falls back to a full scan).
    ///
    /// Soundness: a sealed primitive's supertype set is the fixed chain walked
    /// by [`CoreType::direct_builtin_supertype_name`] /
    /// [`CoreType::direct_builtin_supertype_name_for_julia_name`], so gathering
    /// `wildcard` plus the primitive's own and every abstract-supertype bucket
    /// is a superset of every method that can match; the scorer then rejects
    /// the non-matches, leaving the selected method identical to a full scan. A
    /// chain that ever breaks before reaching `Any` returns `None` (full scan)
    /// rather than gather a partial — a defensive guard that never fires for the
    /// current sealed primitives.
    fn primitive_candidate_indices(&self, arg0: &CoreType) -> Option<Vec<usize>> {
        let CoreType::Primitive(p) = arg0 else {
            return None;
        };
        let mut indices: Vec<usize> = self.wildcard.clone();
        if let Some(bucket) = self.nominal.get(p.julia_name()) {
            indices.extend_from_slice(bucket);
        }
        // Walk the sealed builtin abstract-supertype chain up to (excluding)
        // `Any`, gathering each family bucket.
        let mut parent = arg0.direct_builtin_supertype_name();
        let mut guard = 0usize;
        loop {
            match parent {
                Some("Any") => break,
                Some(name) => {
                    if let Some(bucket) = self.nominal.get(name) {
                        indices.extend_from_slice(bucket);
                    }
                    guard += 1;
                    if guard > 32 {
                        return None;
                    }
                    parent = CoreType::direct_builtin_supertype_name_for_julia_name(name);
                }
                None => return None,
            }
        }
        // Buckets are disjoint, so sorting yields the ascending, duplicate-free
        // definition-order sequence the selection tie-breakers expect.
        indices.sort_unstable();
        Some(indices)
    }
}

/// Typed key for the Base-cache method-table map (Issue #9197 S7).
///
/// The serialized Base cache maps each **generic-function name** — `"+"`,
/// `"Base.log2"`, a module-qualified `"Module.f"`, a constructor's bare type
/// name `"Foo"`, a nested `"parent#nested"` — to its [`MethodTable`]. Before S7
/// that cache boundary used a bare `String` key: the last string-keyed dispatch
/// structure the interned-type-id epic (#9197) set out to retire, and a latent
/// cross-process non-determinism, because the section-based serialize path
/// (`append_section` in `precompile.rs`) serializes the raw `HashMap` in
/// per-process hash-seed order, bypassing the `sorted_hashmap` serde attribute
/// (the #9473-class determinism concern).
///
/// `MethodTableKey` demotes that bare `String` to a documented, type-safe key,
/// and the serialize path now emits the entries **sorted by this key**, so the
/// section is deterministic by construction. It serializes transparently as its
/// canonical name — a stable, deterministic string, never a per-process id — so
/// the wire is compatible with a plain string key on the deserialize side
/// (bincode `serialize_newtype_struct` forwards to the inner `String`).
///
/// It is deliberately a *generic-function* identity, **not** a `ConcreteTypeId`
/// / concrete-*type* identity: a generic function name is not a concrete type,
/// so folding it into the type-intern registry would be a semantic mismatch.
/// It is the sjulia analogue of upstream Julia owning a method table by an
/// interned generic-function identity (`jl_typename_t` / `typeof(f)`,
/// `julia/src/staticdata.c`) rather than a re-parsed name string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MethodTableKey(String);

impl MethodTableKey {
    /// Wrap a canonical generic-function name as a typed method-table key.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The canonical generic-function name this key wraps. Read accessor for
    /// tests and the future #9199 registry-persistence consumer (the in-memory
    /// dispatch tables still convert away via `into_string` at the boundary);
    /// narrow `dead_code` allow mirrors the Issue #9197 S1 later-slice pattern.
    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the key, yielding the owned canonical name (used at the
    /// cache→compiler boundary where the in-memory dispatch tables remain
    /// `String`-keyed pending the full #9199 registry-persistence conversion).
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<String> for MethodTableKey {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl std::borrow::Borrow<str> for MethodTableKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MethodTableKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which of Julia's distinct constructor self families a struct's own
/// declared inner-constructor method belongs to (Issue #10959, #10962,
/// #10974).
///
/// Julia gives every constructor method an implicit callable-self argument:
/// bare `Foo(...)` uses the `Type{Foo}` family, while explicit `Foo{T}(...)`
/// uses `Type{Foo{T}}`. `MethodSig` projects that self argument away, so an
/// ordinary outer constructor can have an identical projected value
/// signature (arity, bounds, and even `where` parameter count) to either
/// inner-constructor family — origin cannot be reconstructed from the
/// projected signature after the fact. [`MethodTable::constructor_self_family`]
/// is therefore the sole, serialized source of truth for a method's
/// constructor origin: an absent entry means the method is NOT one of this
/// struct's own inner constructors (an ordinary function, or an outer
/// constructor for this struct).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstructorSelfFamily {
    /// `function Foo(...) ... end` / `Foo(...) = ...` declared inside the
    /// struct body: implicit `Type{Foo}` self (a "bare" inner constructor).
    BareInner,
    /// `function Foo{T}(...) where T ... end` declared inside the struct
    /// body: implicit `Type{Foo{T}}` self (an "explicit parametric" inner
    /// constructor).
    ExplicitParametricInner,
}

/// Method table for a function name (supports multiple dispatch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodTable {
    pub name: String,
    pub methods: Arc<Vec<MethodSig>>,
    /// Serialized constructor self-family origin, keyed by
    /// `MethodSig::global_index` (Issue #10959, #10962, #10974). A
    /// `BTreeMap`, not a `HashMap`, so Base-cache bytes are deterministic.
    /// This is the sole source of truth for inner-vs-outer constructor
    /// identity: there is no heuristic fallback (e.g. inspecting `where`
    /// parameter count) once a table exists — a missing entry always means
    /// "not this struct's own inner constructor", never "guess from shape".
    /// Populated exclusively through [`Self::add_inner_constructor_method`];
    /// `#[serde(default)]` lets test-only payloads omit it.
    #[serde(default)]
    constructor_self_families: BTreeMap<usize, ConstructorSelfFamily>,
    /// Compile-session provenance for the typed outer row synthesized by
    /// Julia's default-constructor definition. This is deliberately separate
    /// from `constructor_self_families`: the row has the ordinary bare
    /// `Type{Foo}` callable self, but call lowering may replace its trivial
    /// body with the equivalent raw allocation once dispatch has selected it.
    ///
    /// The qualified owner is part of the provenance because multiple
    /// same-short-name constructors can share one table. Keep it transient to
    /// avoid changing the serialized MethodTable schema; in-memory REPL
    /// snapshots retain it via Clone and the explicit clone helpers below
    /// (Issue #11147).
    #[serde(skip)]
    synthetic_default_outer_owners: BTreeMap<usize, String>,
    /// Qualified owners for which the compiler synthesized Julia's default
    /// constructor family. Unlike the per-row outer marker, declaration
    /// provenance intentionally survives later method replacement: it
    /// separates user synthetic/default dispatch from cached Base constructor
    /// tables kept on W-72's legacy route without leaking across sibling
    /// modules that share this table's short name.
    #[serde(skip)]
    synthetic_default_declaration_owners: BTreeSet<String>,
    /// Shared struct-hierarchy projection (`struct_parents`,
    /// `abstract_parents`, `struct_hierarchy`). Built once per compile and
    /// shared across every table via `Arc` (Issue #6348); serialized Base
    /// caches skip it and compile reprojects through
    /// `set_shared_projection` / `set_struct_hierarchy_projection`
    /// (Issue #6440).
    #[serde(skip)]
    projection: Arc<MethodTableProjection>,
    /// Number of Base/prelude functions in the merged IR.
    ///
    /// This is dispatch context, not method identity, so it is rebuilt by the
    /// compiler instead of serialized in caches. Future #5926 dominance fences
    /// can use it to distinguish Base-origin methods from user methods without
    /// treating `is_base_extension` as an origin marker.
    #[serde(skip)]
    base_function_count: usize,
    /// Dispatch result cache: maps structured argument tuple types to the index
    /// of the best matching method in `self.methods`. Invalidated on
    /// `add_method()`. (Issue #3361, #3892)
    #[serde(skip)]
    dispatch_cache: RefCell<HashMap<CoreType, usize>>,
    /// First-argument dispatch bucket index (Issue #9112).  Built lazily on
    /// the first dispatch call after a method-table mutation; cleared to `None`
    /// by every `add_method` / `add_inner_constructor_method` call.
    #[serde(skip)]
    first_arg_index: RefCell<Option<FirstArgIndex>>,
}

impl MethodTable {
    pub fn new(name: String) -> Self {
        Self {
            name,
            methods: Arc::new(Vec::new()),
            constructor_self_families: BTreeMap::new(),
            synthetic_default_outer_owners: BTreeMap::new(),
            synthetic_default_declaration_owners: BTreeSet::new(),
            projection: Arc::new(MethodTableProjection::default()),
            base_function_count: 0,
            dispatch_cache: RefCell::new(HashMap::new()),
            first_arg_index: RefCell::new(None),
        }
    }

    pub fn clone_for_reprojection(&self) -> Self {
        Self {
            name: self.name.clone(),
            methods: Arc::clone(&self.methods),
            constructor_self_families: self.constructor_self_families.clone(),
            synthetic_default_outer_owners: self.synthetic_default_outer_owners.clone(),
            synthetic_default_declaration_owners: self.synthetic_default_declaration_owners.clone(),
            projection: Arc::new(MethodTableProjection::default()),
            base_function_count: 0,
            dispatch_cache: RefCell::new(HashMap::new()),
            first_arg_index: RefCell::new(None),
        }
    }

    /// Rebuild this table with a filtered/reordered `methods` row set (e.g.
    /// Issue #10959's explicit-inner-only dispatch view), retaining
    /// constructor-self-family origin ONLY for global indices that remain
    /// present — a row filtered out of `methods` must not leave a stale
    /// origin entry behind (Issue #10962, #10974).
    pub fn clone_with_methods_for_compile(&self, methods: Vec<MethodSig>) -> Self {
        let retained_global_indices: HashSet<_> =
            methods.iter().map(|method| method.global_index).collect();
        let constructor_self_families = self
            .constructor_self_families
            .iter()
            .filter(|(global_index, _)| retained_global_indices.contains(global_index))
            .map(|(&global_index, &family)| (global_index, family))
            .collect();
        let synthetic_default_outer_owners = self
            .synthetic_default_outer_owners
            .iter()
            .filter(|(global_index, _)| retained_global_indices.contains(global_index))
            .map(|(&global_index, owner)| (global_index, owner.clone()))
            .collect();
        Self {
            name: self.name.clone(),
            methods: Arc::new(methods),
            constructor_self_families,
            synthetic_default_outer_owners,
            synthetic_default_declaration_owners: self.synthetic_default_declaration_owners.clone(),
            projection: Arc::clone(&self.projection),
            base_function_count: self.base_function_count,
            dispatch_cache: RefCell::new(HashMap::new()),
            first_arg_index: RefCell::new(None),
        }
    }

    pub fn set_base_function_count(&mut self, base_function_count: usize) {
        if self.base_function_count != base_function_count {
            self.base_function_count = base_function_count;
            self.dispatch_cache.borrow_mut().clear();
        }
    }

    #[cfg(test)]
    fn is_base_program_method(&self, method: &MethodSig) -> bool {
        method.is_base_program_method(self.base_function_count)
    }

    pub fn is_base_program_global_index(&self, global_index: usize) -> bool {
        self.base_function_count > 0 && global_index < self.base_function_count
    }

    pub fn base_function_count(&self) -> usize {
        self.base_function_count
    }

    pub fn set_struct_hierarchy_projection(
        &mut self,
        hierarchy: &StructHierarchy,
        concrete_struct_names: &[String],
        parametric_struct_names: &[String],
        abstract_type_names: &[String],
    ) {
        self.set_shared_projection(Arc::new(MethodTableProjection::build(
            hierarchy,
            concrete_struct_names,
            parametric_struct_names,
            abstract_type_names,
        )));
    }

    /// Install a prebuilt, shared projection. Compile builds the projection
    /// once and hands the same `Arc` to every method table instead of
    /// rebuilding (and cloning the full hierarchy) per table (Issue #6348).
    pub fn set_shared_projection(&mut self, projection: Arc<MethodTableProjection>) {
        self.projection = projection;
        self.dispatch_cache.borrow_mut().clear();
    }

    /// Read access to the shared method-table projection (the declared struct
    /// hierarchy). Used by compile-time hierarchy queries that sit outside the
    /// dispatch path, e.g. the binary `==`/`!=` array-routing decision
    /// (Issue #8149). The projection is shared across every table (Issue #6348),
    /// so any table's view answers the same program-wide hierarchy questions.
    pub fn projection(&self) -> &MethodTableProjection {
        &self.projection
    }

    /// Test-only insertion of a declared-parent link into the projection's
    /// shared hierarchy (the old tests' `struct_parents_mut().insert(..)`).
    /// Copy-on-write: unshares the projection if it is shared.
    #[cfg(test)]
    fn insert_parent_link_for_tests(&mut self, name: &str, parent: Option<String>) {
        let projection = Arc::make_mut(&mut self.projection);
        projection.struct_hierarchy.insert(name, parent, Vec::new());
        projection.has_parent_links = true;
    }

    /// Debug-build totality guard for production method registration (Issue
    /// #8549): the only production `MethodSig` constructors are
    /// `from_julia_projections` (derives the canonical signature
    /// structurally) and validated deserialization; the `Bottom` placeholder
    /// constructors are `#[cfg(test)]`. Assert at the registration points so
    /// any future bypass fails loudly in debug builds instead of silently
    /// degrading dispatch through the conservative fallbacks. Unit tests
    /// (`cfg(test)`) register placeholders on purpose to pin those fallbacks,
    /// so the assertion is compiled out there.
    fn debug_assert_signature_total(&self, _sig: &MethodSig) {
        #[cfg(not(test))]
        debug_assert!(
            _sig.signature_defect().is_none(),
            "production MethodSig for `{}` (global #{}) has a defective canonical signature: {:?} (Issue #8549)",
            self.name,
            _sig.global_index,
            _sig.signature_defect()
        );
    }

    /// Add a method to this table.
    /// If a method with the same signature already exists, replace it instead of adding a duplicate.
    pub fn add_method(&mut self, sig: MethodSig) {
        self.debug_assert_signature_total(&sig);

        // A covariant `where` variable used once as a whole parameter is
        // equivalent to its bound (`h(x::T) where {T<:Number}` ≡ `h(x::Number)`),
        // so dedup on the canonicalized signature too — otherwise the two
        // spellings are kept as separate methods and dispatch picks by
        // registration order instead of last-definition-wins (Issue #5383).
        let sig_canonical = sig.canonical_signature_for_dedup();
        let params_match = |existing: &MethodSig| {
            // Only an explicit-parametric inner has a distinct implicit
            // callable-self identity (`Type{Foo{T}}`). A bare inner and an
            // ordinary bare constructor both dispatch on `Type{Foo}`, so an
            // identical later ordinary definition must replace the bare inner
            // under Julia's last-definition-wins rule (Issue #11028).
            if self.is_explicit_parametric_inner_constructor(existing.global_index) {
                return false;
            }

            // A fixed-arity method and a vararg method are distinct signatures even
            // when their projected `params` coincide: `g(x::Int)` is `Tuple{Int}`
            // while `g(x::Int...)` is `Tuple{Vararg{Int}}`. The canonical signature
            // stores the declared element type separately from the vararg metadata, so without this guard a
            // later vararg definition would *replace* an existing fixed method (and
            // vice versa), collapsing the method table to one method and making
            // dispatch order-dependent (Issue #5924). Only dedup methods that share
            // the same vararg structure.
            if existing.vararg_param_index != sig.vararg_param_index
                || existing.vararg_fixed_count != sig.vararg_fixed_count
            {
                return false;
            }

            // Stage 7c-i: the historical JuliaType-projection equality arm is
            // retired. Production construction and deserialization both provide
            // the canonical `core_signature`, so equality is decided on that
            // single source of truth.
            existing.core_signature == sig.core_signature
                || existing.canonical_signature_for_dedup() == sig_canonical
        };

        let existing_ordinary_position = self.methods.iter().position(params_match);
        let new_global_index = sig.global_index;

        // Find and replace an existing ordinary method, or add a new one.
        let methods = Arc::make_mut(&mut self.methods);
        if let Some(pos) = existing_ordinary_position {
            // Replace existing method (user code overrides Base)
            let replaced_global_index = methods[pos].global_index;
            methods[pos] = sig;
            self.synthetic_default_outer_owners
                .remove(&replaced_global_index);
            if replaced_global_index != new_global_index {
                // Keep the serialized origin map transactional even if a
                // bare-inner row is replaced by a later ordinary constructor.
                // Explicit-parametric inner rows remain excluded above because
                // their implicit callable self is distinct (Issue #11028).
                self.constructor_self_families
                    .remove(&replaced_global_index);
            }
        } else {
            // Add new method
            methods.push(sig);
        }

        // Invalidate dispatch cache (Issue #3361) and first-arg index (Issue #9112).
        self.dispatch_cache.borrow_mut().clear();
        *self.first_arg_index.borrow_mut() = None;
    }

    /// Add the typed outer row synthesized for a default constructor and retain
    /// its compile-session provenance. A later ordinary method with the same
    /// signature clears the marker through [`Self::add_method`], preserving
    /// Julia's source-order replacement semantics (Issue #11147).
    pub fn add_synthetic_default_outer_method(&mut self, sig: MethodSig, owner: &str) {
        self.mark_synthetic_default_constructor_declaration(owner);
        let global_index = sig.global_index;
        self.add_method(sig);
        if self
            .methods
            .iter()
            .any(|method| method.global_index == global_index)
        {
            self.synthetic_default_outer_owners
                .insert(global_index, owner.to_string());
        }
    }

    /// Whether `global_index` is the typed outer row synthesized for `owner`'s
    /// default constructor in this compile session (Issue #11147).
    pub fn is_synthetic_default_outer_for_owner(&self, global_index: usize, owner: &str) -> bool {
        self.synthetic_default_outer_owners
            .get(&global_index)
            .is_some_and(|recorded_owner| recorded_owner == owner)
    }

    /// Record that `owner` was produced from a user declaration with
    /// synthesized default constructors. This declaration provenance survives
    /// row replacement but remains owner-exact (Issue #11147).
    pub fn mark_synthetic_default_constructor_declaration(&mut self, owner: &str) {
        self.synthetic_default_declaration_owners
            .insert(owner.to_string());
    }

    pub fn has_synthetic_default_constructor_declaration(&self, owner: &str) -> bool {
        self.synthetic_default_declaration_owners.contains(owner)
    }

    /// Return the ordinary constructor/function row whose projected bare
    /// callable signature collides with `sig`, if any. The compilation
    /// pipeline uses the global index to compare source positions before it
    /// registers a bare inner constructor, because structs and ordinary
    /// methods are collected in separate passes (Issue #11028).
    pub fn ordinary_method_with_same_signature(&self, sig: &MethodSig) -> Option<usize> {
        let sig_canonical = sig.canonical_signature_for_dedup();
        self.methods
            .iter()
            .find(|existing| {
                self.constructor_self_family(existing.global_index)
                    .is_none()
                    && existing.vararg_param_index == sig.vararg_param_index
                    && existing.vararg_fixed_count == sig.vararg_fixed_count
                    && (existing.core_signature == sig.core_signature
                        || existing.canonical_signature_for_dedup() == sig_canonical)
            })
            .map(|method| method.global_index)
    }

    /// Add an inner-constructor `sig` while preserving the implicit callable
    /// self that is absent from the projected value signature. Explicit
    /// parametric inners deduplicate only within `Type{Foo{T}}`; bare inners
    /// share `Type{Foo}` with ordinary constructors and therefore replace an
    /// identical earlier ordinary/bare row. This preserves Julia's
    /// last-definition-wins semantics while keeping the two actual callable
    /// self families distinct (Issue #11028).
    ///
    /// Issue #8121: an explicit-parametric inner constructor
    /// `Foo{T}(args) where {T}` and an outer
    /// constructor `Foo(args)` are DISTINCT methods in upstream Julia (they
    /// differ in the implicit `Type{Foo{T}}` vs `Type{Foo}` self argument), yet
    /// sjulia projects both to the same value-parameter signature. Using the
    /// ordinary `add_method` would let the inner ctor REPLACE the outer (or vice
    /// versa); both must coexist so constructor-aware selection can route bare
    /// `Foo(args)` to the outer and explicit `Foo{T}(args)` to the inner. The
    /// "already present" guard keeps a
    /// struct that is enumerated more than once (e.g. under multiple module
    /// paths) from accumulating duplicate methods. Origin, rather than
    /// `has_where_params`, is required here because an outer constructor may
    /// itself have an identical `where` clause (Issue #10959).
    ///
    /// `family` records `sig` as one of this struct's own declared inner
    /// constructors (Issue #10962, #10974): the mutation is transactional —
    /// the replaced row's stale origin entry (if any) is removed in the same
    /// call that installs the new one, so `constructor_self_families` never
    /// points at a global index absent from `methods`.
    pub fn add_inner_constructor_method(&mut self, sig: MethodSig, family: ConstructorSelfFamily) {
        self.debug_assert_signature_total(&sig);
        let new_global_index = sig.global_index;
        let sig_canonical = sig.canonical_signature_for_dedup();
        let sig_constructor_signature = sig.constructor_signature_for_dedup();
        let existing_inner_position = self.methods.iter().position(|existing| {
            let existing_family = self.constructor_self_family(existing.global_index);
            let same_callable_self = match family {
                ConstructorSelfFamily::ExplicitParametricInner => {
                    existing_family == Some(ConstructorSelfFamily::ExplicitParametricInner)
                }
                // A bare inner and an ordinary constructor have the same
                // implicit `Type{Foo}` callable self. A later bare inner must
                // therefore replace either one when the value signature is
                // identical (Issue #11028).
                ConstructorSelfFamily::BareInner => {
                    existing_family != Some(ConstructorSelfFamily::ExplicitParametricInner)
                }
            };
            same_callable_self
                && existing.vararg_param_index == sig.vararg_param_index
                && existing.vararg_fixed_count == sig.vararg_fixed_count
                && if family == ConstructorSelfFamily::ExplicitParametricInner {
                    let existing_has_complete_self = existing_family
                        == Some(ConstructorSelfFamily::ExplicitParametricInner)
                        && existing.explicit_constructor_type_name.is_some();
                    let sig_has_complete_self = family
                        == ConstructorSelfFamily::ExplicitParametricInner
                        && sig.explicit_constructor_type_name.is_some();
                    if existing_has_complete_self && sig_has_complete_self {
                        // Reconstruct the omitted callable self so correlations,
                        // self-only bounds, aliases, and qualified module owners
                        // all participate in constructor identity. Qualified
                        // nominal owners are exact in this narrow mode; ordinary
                        // type equality remains module-insensitive (Issue #11019).
                        existing
                            .constructor_signature_for_dedup()
                            .is_semantically_equal_with_qualified_nominals(
                                &sig_constructor_signature,
                            )
                    } else {
                        // Base rows intentionally retain legacy constructor
                        // metadata under the registered containment for #11062.
                        // Preserve the former equality rule there (and for old
                        // test/wire rows) so a covariant `::T` remains distinct
                        // from its concrete upper bound (Issue #10993).
                        existing
                            .core_signature
                            .is_semantically_equal(&sig.core_signature)
                    }
                } else {
                    existing.core_signature == sig.core_signature
                        || existing.canonical_signature_for_dedup() == sig_canonical
                }
        });
        if let Some(position) = existing_inner_position {
            let replaced_global_index = self.methods[position].global_index;
            Arc::make_mut(&mut self.methods)[position] = sig;
            self.constructor_self_families
                .remove(&replaced_global_index);
            self.synthetic_default_outer_owners
                .remove(&replaced_global_index);
        } else {
            Arc::make_mut(&mut self.methods).push(sig);
        }
        self.constructor_self_families
            .insert(new_global_index, family);
        self.dispatch_cache.borrow_mut().clear();
        *self.first_arg_index.borrow_mut() = None;
    }

    /// The constructor self-family `global_index` belongs to, if it is one of
    /// this table's own declared inner constructors. `None` for an ordinary
    /// method or an outer constructor for this struct (Issue #10962, #10974).
    pub fn constructor_self_family(&self, global_index: usize) -> Option<ConstructorSelfFamily> {
        self.constructor_self_families.get(&global_index).copied()
    }

    /// Whether `global_index` is any of this table's own declared inner
    /// constructors (either self family).
    pub fn is_inner_constructor(&self, global_index: usize) -> bool {
        self.constructor_self_families.contains_key(&global_index)
    }

    /// Whether `global_index` is specifically an explicit-parametric
    /// (`Foo{T}(...)`-declared) inner constructor.
    pub fn is_explicit_parametric_inner_constructor(&self, global_index: usize) -> bool {
        self.constructor_self_family(global_index)
            == Some(ConstructorSelfFamily::ExplicitParametricInner)
    }

    /// Whether this table has any explicit-parametric inner-constructor row
    /// at all. Explicit `Foo{T}(...)` call-site selection and bare `Foo(...)`
    /// filtering both gate on this before consulting per-method origin
    /// (Issue #10959, #10962, #10974).
    pub fn has_explicit_parametric_inner_constructors(&self) -> bool {
        self.constructor_self_families
            .values()
            .any(|family| *family == ConstructorSelfFamily::ExplicitParametricInner)
    }

    /// Dispatch among only this table's own explicit-parametric-inner-constructor
    /// rows (Issue #10959, #10962, #10974): an explicit `Foo{T}(...)` call
    /// whose type argument is a runtime-forwardable expression must select
    /// one of the struct's own `Foo{T}(...)`-declared inner constructors,
    /// never an outer method or a bare-self inner constructor that happens to
    /// share the projected value signature.
    pub fn dispatch_among_explicit_parametric_inner_constructors(
        &self,
        arg_types: &[JuliaType],
    ) -> Result<&MethodSig, DispatchError> {
        let allowed: HashSet<usize> = self
            .constructor_self_families
            .iter()
            .filter(|(_, family)| **family == ConstructorSelfFamily::ExplicitParametricInner)
            .map(|(global_index, _)| *global_index)
            .collect();
        self.dispatch_among_global_indices(arg_types, &allowed)
    }

    /// Dispatch among ordinary outer-constructor rows only.
    ///
    /// Inner constructors share the same projected value signature in the
    /// compile-time table, but their implicit callable self is a concrete
    /// `Type{Foo}` / `Type{Foo{T}}`. They must not hide the ordinary row that a
    /// bare family call `Foo(args...)` selects (Issue #11147).
    pub fn dispatch_among_outer_constructors(
        &self,
        arg_types: &[JuliaType],
    ) -> Result<&MethodSig, DispatchError> {
        let allowed: HashSet<usize> = self
            .methods
            .iter()
            .filter(|method| !self.is_inner_constructor(method.global_index))
            .map(|method| method.global_index)
            .collect();
        self.dispatch_among_global_indices(arg_types, &allowed)
    }

    /// Check if all methods in this table are Base extensions.
    /// If true, we should prefer builtin operators for primitive types.
    pub fn all_base_extensions(&self) -> bool {
        !self.methods.is_empty() && self.methods.iter().all(|m| m.is_base_extension)
    }

    /// Find the best matching method for the given argument types.
    pub fn dispatch(&self, arg_types: &[JuliaType]) -> Result<&MethodSig, DispatchError> {
        let cache_key = dispatch_resolver::core_tuple_signature_from_julia_types(arg_types);

        // Check dispatch cache first (Issue #3361)
        if let Some(&cached_idx) = self.dispatch_cache.borrow().get(&cache_key) {
            if cached_idx < self.methods.len() {
                return Ok(&self.methods[cached_idx]);
            }
        }

        let result = self.dispatch_inner(arg_types)?;

        // Cache the result by finding its index via pointer comparison
        let result_ptr = result as *const MethodSig;
        for (i, m) in self.methods.iter().enumerate() {
            if std::ptr::eq(m as *const MethodSig, result_ptr) {
                self.dispatch_cache.borrow_mut().insert(cache_key, i);
                break;
            }
        }

        Ok(result)
    }

    /// Dispatch using only methods whose global indices are in `allowed`.
    ///
    /// Constructor lowering uses this when an explicit parametric call must
    /// choose among registered inner constructors without allowing an outer
    /// constructor with the same projected value signature to participate
    /// (Issue #10959). Cloning preserves the table's hierarchy projection, so
    /// ranking remains identical to ordinary dispatch.
    pub fn dispatch_among_global_indices(
        &self,
        arg_types: &[JuliaType],
        allowed: &HashSet<usize>,
    ) -> Result<&MethodSig, DispatchError> {
        let filtered_methods = self
            .methods
            .iter()
            .filter(|method| allowed.contains(&method.global_index))
            .cloned()
            .collect();
        let filtered = self.clone_with_methods_for_compile(filtered_methods);
        let selected_global_index = filtered.dispatch(arg_types)?.global_index;
        self.methods
            .iter()
            .find(|method| method.global_index == selected_global_index)
            .ok_or_else(|| DispatchError::NoMethodFound {
                name: self.name.clone(),
                arg_types: arg_types.to_vec(),
            })
    }

    /// Dispatch among `allowed` methods after positionally binding each
    /// candidate's own `where` variables to `positional_type_args`.
    ///
    /// The temporary projection keeps the original table's hierarchy but
    /// owns fresh methods and dispatch caches. Each candidate validates its
    /// explicit type arguments through the shared CoreType matcher, then has
    /// those bindings substituted into its value-argument signature before
    /// ordinary [`Self::dispatch`] performs applicability, dominance, and
    /// tie-breaking. The selected result is returned from the original table.
    pub fn dispatch_among_global_indices_with_positional_type_args(
        &self,
        arg_types: &[JuliaType],
        allowed: &HashSet<usize>,
        positional_type_args: &[JuliaType],
    ) -> Result<&MethodSig, DispatchError> {
        let projected_methods: Vec<MethodSig> = self
            .methods
            .iter()
            .filter(|method| allowed.contains(&method.global_index))
            .filter_map(|method| {
                prebind_method_positional_type_args(
                    method,
                    positional_type_args,
                    &self.projection.struct_hierarchy,
                )
            })
            .collect();
        let projected = self.clone_with_methods_for_compile(projected_methods);
        let supplied_cores: Vec<CoreType> = positional_type_args
            .iter()
            .map(
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
            )
            .collect();
        let supplied_type_vars: HashMap<String, CoreType> = supplied_cores
            .iter()
            .filter_map(|supplied| match supplied {
                CoreType::TypeVar(var) => Some((
                    core_type_var_base_name(&var.name).to_string(),
                    supplied.clone(),
                )),
                CoreType::Named(name) => Some((name.clone(), supplied.clone())),
                _ => None,
            })
            .collect();
        let semantic_arg_types: Vec<JuliaType> = arg_types
            .iter()
            .map(
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
            )
            .map(|arg| substitute_core_type_vars(&arg, &supplied_type_vars))
            .map(|arg| subset_julia_vm_types::inference_core::core_type_to_julia_type(&arg))
            .collect();
        let selected_global_index = projected.dispatch(&semantic_arg_types)?.global_index;
        self.methods
            .iter()
            .find(|method| method.global_index == selected_global_index)
            .ok_or_else(|| DispatchError::NoMethodFound {
                name: self.name.clone(),
                arg_types: arg_types.to_vec(),
            })
    }

    pub fn signature_matches_arg_types(&self, method: &MethodSig, arg_types: &[JuliaType]) -> bool {
        let arg_cores: Vec<CoreType> = arg_types
            .iter()
            .map(
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
            )
            .collect();
        method_match_binding_count(method, arg_types, &arg_cores, &self.projection).is_some()
    }

    /// Inner dispatch logic (uncached). (Issue #3361)
    ///
    /// Thin adapter onto the shared selection core (Issue #6502): this method
    /// owns only the `MethodSig`-specific *semantics* (signature matching and
    /// scoring, the dominance relations, the tie-breaker predicates); the
    /// selection *control flow* — enumerate → match → dominance → pick — lives
    /// in [`selection`] (`inference_core/selection.rs`).
    fn dispatch_inner(&self, arg_types: &[JuliaType]) -> Result<&MethodSig, DispatchError> {
        // The argument tuple is bridged once per dispatch.
        let arg_cores: Vec<CoreType> = arg_types
            .iter()
            .map(
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
            )
            .collect();
        // Issue #8548 filtering flip: stage-1 candidacy is decided by the
        // CoreType-native typemap filter (subtype over the canonical
        // signature for precise call tuples; imprecise tuples and known
        // engine gaps defer to the scoring matcher — see
        // `typemap_stage1_binding_count`). Scoring stays as ranking only.
        // Flip evidence: zero candidate/selection diffs under
        // `SJULIA_DISPATCH_COMPARE=1` across the dispatch-heavy fixture
        // categories, the dispatch parity harness, and Base compilation.
        let result =
            self.dispatch_inner_with_filter(arg_types, &arg_cores, CandidateFilter::Typemap);
        // Debug compare mode (Issue #8548): run the scoring-path matcher
        // alongside the production typemap filter and report any acceptance /
        // selection divergence on stderr. Default-off diagnostic; the
        // production result is returned unchanged.
        if dispatch_resolver::dispatch_compare_enabled() {
            self.report_dispatch_filter_divergence(arg_types, &arg_cores, &result);
        }
        result.map(|idx| &self.methods[idx])
    }

    /// Typemap-filter stage-1 candidacy for `method` (Issue #8548):
    /// arity-expand the canonical signature, then delegate to
    /// [`dispatch_resolver::typemap_candidate_verdict`]. The
    /// availability/arity gate is the same one the scoring matcher consumes
    /// (`expanded_core_param_types_for_arity`, Issue #8549).
    ///
    /// Returns the `where` binding count for the ranking score when the
    /// method is a candidate, `None` otherwise:
    ///
    /// - [`TypemapVerdict::Accept`]: candidate; the binding count still comes
    ///   from the scoring matcher (`0` if it disagrees — the compare-mode
    ///   evidence over the fixture corpus recorded no such method).
    /// - [`TypemapVerdict::Reject`]: not a candidate.
    /// - [`TypemapVerdict::DeferImprecise`] / `DeferSignature`: an
    ///   intersection question / known engine gap; the scoring matcher's
    ///   runtime-deferral policy decides, unchanged.
    ///
    /// [`TypemapVerdict::Accept`]: dispatch_resolver::TypemapVerdict::Accept
    /// [`TypemapVerdict::Reject`]: dispatch_resolver::TypemapVerdict::Reject
    /// [`TypemapVerdict::DeferImprecise`]: dispatch_resolver::TypemapVerdict::DeferImprecise
    fn typemap_stage1_binding_count(
        &self,
        method: &MethodSig,
        arg_types: &[JuliaType],
        arg_cores: &[CoreType],
    ) -> Option<usize> {
        match self.typemap_stage1_verdict(method, arg_types.len(), arg_cores) {
            dispatch_resolver::TypemapVerdict::Accept => Some(
                method_match_binding_count(method, arg_types, arg_cores, &self.projection)
                    .unwrap_or(0),
            ),
            dispatch_resolver::TypemapVerdict::Reject => {
                let param_types =
                    method.expanded_projected_param_julia_types_for_arity(arg_types.len())?;
                let type_params = method.projected_type_params();
                struct_parents_fallback_match(
                    &param_types,
                    arg_types,
                    &type_params,
                    &self.projection,
                )
            }
            dispatch_resolver::TypemapVerdict::DeferImprecise
            | dispatch_resolver::TypemapVerdict::DeferSignature => {
                method_match_binding_count(method, arg_types, arg_cores, &self.projection)
            }
        }
    }

    fn typemap_stage1_verdict(
        &self,
        method: &MethodSig,
        arg_count: usize,
        arg_cores: &[CoreType],
    ) -> dispatch_resolver::TypemapVerdict {
        let Some(core_params) = method.expanded_core_param_types_for_arity(arg_count) else {
            return dispatch_resolver::TypemapVerdict::Reject;
        };
        let core_vars = method.core_signature_type_vars();
        dispatch_resolver::typemap_candidate_verdict(
            &self.projection.struct_hierarchy,
            &core_params,
            &core_vars,
            arg_cores,
        )
    }

    /// Compare-mode diagnostics (Issue #8548): re-run stage 1 acceptance per
    /// method under both candidate filters, then re-run the whole selection
    /// pipeline under the scoring filter, logging any divergence against the
    /// production typemap result with the stable `SJULIA_DISPATCH_COMPARE`
    /// prefix (candidate-diff / selection-diff lines). Also emits a
    /// `verdict-count` line so #8999 sweeps can inventory how many candidates
    /// still defer by reason code. Only reachable behind
    /// [`dispatch_resolver::dispatch_compare_enabled`].
    fn report_dispatch_filter_divergence(
        &self,
        arg_types: &[JuliaType],
        arg_cores: &[CoreType],
        typemap: &Result<usize, DispatchError>,
    ) {
        let mut accept_count = 0usize;
        let mut reject_count = 0usize;
        let mut defer_imprecise_count = 0usize;
        let mut defer_signature_count = 0usize;
        for method in self.methods.iter() {
            let scoring_accepts =
                method_match_binding_count(method, arg_types, arg_cores, &self.projection)
                    .is_some();
            let verdict = self.typemap_stage1_verdict(method, arg_types.len(), arg_cores);
            match verdict {
                dispatch_resolver::TypemapVerdict::Accept => accept_count += 1,
                dispatch_resolver::TypemapVerdict::Reject => reject_count += 1,
                dispatch_resolver::TypemapVerdict::DeferImprecise => defer_imprecise_count += 1,
                dispatch_resolver::TypemapVerdict::DeferSignature => defer_signature_count += 1,
            }
            let typemap_accepts = match verdict {
                dispatch_resolver::TypemapVerdict::Accept => true,
                dispatch_resolver::TypemapVerdict::Reject => false,
                dispatch_resolver::TypemapVerdict::DeferImprecise
                | dispatch_resolver::TypemapVerdict::DeferSignature => {
                    method_match_binding_count(method, arg_types, arg_cores, &self.projection)
                        .is_some()
                }
            };
            if scoring_accepts != typemap_accepts {
                dispatch_resolver::dispatch_compare_log(format_args!(
                    "SJULIA_DISPATCH_COMPARE candidate-diff fn={} args={} method={} sig={} \
                     scoring={} typemap={}",
                    self.name,
                    render_dispatch_arg_types(arg_types),
                    method.global_index,
                    render_dispatch_arg_types(&method.projected_param_julia_types()),
                    if scoring_accepts { "accept" } else { "reject" },
                    if typemap_accepts { "accept" } else { "reject" },
                ));
            }
        }
        dispatch_resolver::dispatch_compare_log(format_args!(
            "SJULIA_DISPATCH_COMPARE verdict-count fn={} args={} accept={} reject={} \
             defer_imprecise={} defer_signature={}",
            self.name,
            render_dispatch_arg_types(arg_types),
            accept_count,
            reject_count,
            defer_imprecise_count,
            defer_signature_count,
        ));

        let scoring =
            self.dispatch_inner_with_filter(arg_types, arg_cores, CandidateFilter::Scoring);
        let selection_matches = match (&scoring, typemap) {
            (Ok(a), Ok(b)) => a == b,
            (
                Err(DispatchError::NoMethodFound { .. }),
                Err(DispatchError::NoMethodFound { .. }),
            ) => true,
            (
                Err(DispatchError::AmbiguousMethod { .. }),
                Err(DispatchError::AmbiguousMethod { .. }),
            ) => true,
            _ => false,
        };
        if !selection_matches {
            let render = |outcome: &Result<usize, DispatchError>| match outcome {
                Ok(idx) => format!(
                    "method={} sig={}",
                    self.methods[*idx].global_index,
                    render_dispatch_arg_types(&self.methods[*idx].projected_param_julia_types())
                ),
                Err(DispatchError::NoMethodFound { .. }) => "no-method".to_string(),
                Err(DispatchError::AmbiguousMethod { .. }) => "ambiguous".to_string(),
            };
            dispatch_resolver::dispatch_compare_log(format_args!(
                "SJULIA_DISPATCH_COMPARE selection-diff fn={} args={} scoring=[{}] typemap=[{}]",
                self.name,
                render_dispatch_arg_types(arg_types),
                render(&scoring),
                render(typemap),
            ));
        }
    }

    /// Stage 1–4 selection pipeline, parameterized over the candidate filter
    /// (Issue #8548). Returns the selected index into `self.methods` so the
    /// compare mode can diff outcomes structurally.
    fn dispatch_inner_with_filter(
        &self,
        arg_types: &[JuliaType],
        arg_cores: &[CoreType],
        filter: CandidateFilter,
    ) -> Result<usize, DispatchError> {
        // Stage 1: enumerate + match + score. Matching consumes the canonical
        // `core_signature` projections (Issue #6495 stage 3).
        let mut matches: Vec<(&MethodSig, u32)> = Vec::new();
        // Position in `self.methods` for each entry of `matches`.
        let mut matched_method_indices: Vec<usize> = Vec::new();

        // Issue #9112 / #9197 S5: pre-filter candidates by the first-arg type so
        // that large method tables (e.g. `+` with 100+ methods) only visit the
        // buckets whose declared first parameter can match, instead of every
        // method. The gather is subtype-complete (and therefore sound) for a
        // sealed-primitive first argument — it gathers the primitive's own
        // bucket, its builtin abstract-supertype buckets, and `wildcard`, which
        // now excludes every struct/container method a primitive can never
        // match. For any other first-argument kind (struct / container /
        // abstract / `Type{T}` / tuple / zero-arg call) we fall back to a full
        // `0..self.methods.len()` scan — correct, but with the struct-argument
        // typemap gather deferred to Issue #9197 S6/S7 (see `FirstArgIndex`).
        let filtered_indices: Option<Vec<usize>> = match arg_cores.first() {
            Some(first @ CoreType::Primitive(_)) => {
                // Ensure the index is built (lazy, invalidated on mutations).
                let mut slot = self.first_arg_index.borrow_mut();
                if slot.is_none() {
                    *slot = Some(FirstArgIndex::build(&self.methods));
                }
                slot.as_ref().unwrap().primitive_candidate_indices(first)
            }
            _ => None,
        };

        // Unify the two paths: iterate `filtered_indices` if available, else
        // iterate all `0..methods.len()`.
        let all_indices: Vec<usize>;
        let method_indices: &[usize] = if let Some(ref fi) = filtered_indices {
            fi.as_slice()
        } else {
            all_indices = (0..self.methods.len()).collect();
            all_indices.as_slice()
        };

        // Measurement hook (Issue #9197 S5): count the candidate list length
        // once per dispatch — the reduction the first-arg typemap delivers.
        // `#[cfg(test)]` so production dispatch pays nothing.
        #[cfg(test)]
        DISPATCH_CANDIDATE_SCANS.with(|c| c.set(c.get().wrapping_add(method_indices.len() as u64)));

        for &method_idx in method_indices {
            let method = &self.methods[method_idx];
            // Track type variable bindings to ensure the same TypeVar binds to the same type
            // This is needed for methods like f(::Type{T}, ::Type{T}) where T - both args must be same type
            let binding_count = match filter {
                CandidateFilter::Scoring => {
                    let Some(binding_count) =
                        method_match_binding_count(method, arg_types, arg_cores, &self.projection)
                    else {
                        continue;
                    };
                    binding_count
                }
                CandidateFilter::Typemap => {
                    // Typemap path (Issue #8548): candidacy is decided by the
                    // CoreType subtype relation over the canonical signature
                    // (imprecise call tuples and known engine gaps defer to
                    // the scoring matcher); the scoring matcher only supplies
                    // the `where` binding count consumed by the ranking score.
                    let Some(binding_count) =
                        self.typemap_stage1_binding_count(method, arg_types, arg_cores)
                    else {
                        continue;
                    };
                    binding_count
                }
            };

            // Scoring consumes the core projections too (Issue #6495 stage
            // 4). Stage 7c-i: the legacy scorer arm is retired — a method
            // that just matched necessarily expanded
            // (`method_match_binding_count` consumed the same expansion), so
            // the `else` is unreachable; kept as a defensive no-match rather
            // than a panic (PANIC_FREE).
            let Some(core_params) = method.expanded_core_param_types_for_arity(arg_types.len())
            else {
                continue;
            };
            let mut score = dispatch_resolver::core_match::score_core_signature_with_binding_count(
                &core_params,
                arg_cores,
                binding_count,
                method.vararg_param_index.is_some(),
                method.vararg_fixed_count.is_some(),
            );
            let core_vars = method.core_signature_type_vars();
            score.score = score.score.saturating_add(
                dispatch_resolver::core_match::typed_vararg_where_bonus_core(
                    method.structured_arg_core_types().unwrap_or(&[]),
                    &core_vars,
                    method.vararg_param_index,
                ),
            );
            matches.push((method, score.score));
            matched_method_indices.push(method_idx);
        }

        // Stages 2-4 run through the shared pipeline driver
        // (`selection::select_method`, Issue #6502):
        //
        // Stage 2: morespecific dominance pre-checks (Issue #5926). The integer
        // specificity `score` mis-ranks several real specificity relations — a
        // concrete container vs its abstract supertype (`Vector{T}` vs
        // `AbstractVector`), the diagonal `Tuple{T,T}` vs `Tuple{Any,Any}`,
        // bounded `where` params (`Vector{<:Integer}` vs `Vector{<:Real}`), and
        // invariant parametric structs (`Pair{T,T}` vs `Pair{A,B}`) — so the
        // more specific method is either out-ranked (wrong dispatch) or ties
        // into an ambiguity error. Before the score winnowing, consult the
        // where-wrapped `Tuple` subtype order: if exactly one matching method
        // strictly dominates every other match, it is unambiguously the most
        // specific, so select it directly. Otherwise fall through to the score
        // + tie-breakers unchanged.
        //
        // Stage 3: conflicting (mutually-incomparable) `Tuple` vararg patterns
        // are an irreducible ambiguity (Issue #6220).
        //
        // Stage 4: score winnowing + tie-breaker ladder (shared control flow;
        // semantics injected as closures). Ambiguity payloads are indices into
        // `matches`. The projection-consuming tie-breakers (exact-match,
        // Any-count, fewest-`where`-params, ancestry filter,
        // strictly-more-specific) run on the canonical `core_signature`
        // projections; a structured-unavailable method is test-only and takes
        // the conservative fallback paths (Issue #6495).
        let has_any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
        let selected = selection::select_method(
            matches.len(),
            || {
                dominance_precheck_index(
                    &matches,
                    arg_types,
                    arg_cores,
                    &self.projection.struct_hierarchy,
                    self.base_function_count,
                )
            },
            || {
                tuple_vararg_conflicting_match(
                    &matches,
                    arg_types,
                    &self.projection.struct_hierarchy,
                )
                .then(|| (0..matches.len()).collect::<Vec<_>>())
            },
            || match selection::pick_scored_match(
                &matches,
                has_any_arg,
                self.projection.has_parent_links(),
                // Exact signature match: same fixed arity, slot-for-slot equal.
                |m| exact_signature_match(m, arg_cores),
                |m| m.vararg_param_index.is_some(),
                // Any params among the fixed prefix.
                |m| any_param_count_fixed_prefix(m),
                |m| where_param_count(m),
                // Struct ancestry filter (Issue #3144).
                |m| ancestry_filter_passes(m, arg_types, &self.projection),
                |a, b| method_params_strictly_more_specific(a, b),
            ) {
                selection::ScoredPick::Single(idx) => {
                    // An imprecise (statically-`Any`) argument may have matched
                    // a method the runtime value cannot actually satisfy; a
                    // unique max-score winner must still be definitively
                    // selected by the static argument tuple.
                    if has_any_arg
                        && !static_arg_tuple_satisfies_method(
                            matches[idx].0,
                            arg_types,
                            &self.projection.struct_hierarchy,
                        )
                    {
                        selection::Selection::NoMatch
                    } else {
                        selection::Selection::Selected(idx)
                    }
                }
                selection::ScoredPick::TieBroken(idx) => selection::Selection::Selected(idx),
                selection::ScoredPick::Ambiguous(tied) => selection::Selection::Ambiguous(tied),
            },
        );

        match selected {
            selection::Selection::NoMatch => Err(DispatchError::NoMethodFound {
                name: self.name.clone(),
                arg_types: arg_types.to_vec(),
            }),
            selection::Selection::Selected(idx) => Ok(matched_method_indices[idx]),
            selection::Selection::Ambiguous(tied) => Err(DispatchError::AmbiguousMethod {
                name: self.name.clone(),
                arg_types: arg_types.to_vec(),
                // Diagnostic payload: the declared parameter rows, sourced
                // from the canonical `core_signature` projection through the
                // canonical inverse when available (Issue #6495, stage 7a).
                candidates: tied
                    .iter()
                    .map(|&i| matches[i].0.projected_param_julia_types())
                    .collect(),
            }),
        }
    }
}

/// Candidate-filter path for [`MethodTable::dispatch_inner_with_filter`]
/// (Issue #8548, parent #8438).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateFilter {
    /// Production path: the per-slot scoring matcher
    /// ([`method_match_binding_count`]) decides candidacy.
    Scoring,
    /// CoreType-native typemap filter
    /// ([`dispatch_resolver::typemap_candidate_accepts`]) decides candidacy;
    /// scoring only ranks. Compare-mode only until the Issue #8548 flip.
    Typemap,
}

/// Render an argument/parameter row as `Tuple{...}` for the
/// `SJULIA_DISPATCH_COMPARE` diagnostic lines (Issue #8548).
fn render_dispatch_arg_types(arg_types: &[JuliaType]) -> String {
    let names: Vec<String> = arg_types.iter().map(|t| t.name().into_owned()).collect();
    format!("Tuple{{{}}}", names.join(", "))
}

/// Tie-breaker 1 input (most-`Any` preference when an argument is statically
/// `Any`): the number of `Any` parameters among the fixed (non-vararg)
/// prefix, read from the canonical `core_signature` projection (Issue #6495,
/// stage 6a).
///
/// Accepted-divergence note (parity gate + full suite referee, zero hits): a
/// parameter spelled `JuliaType::Struct("Any")` images as `CoreType::Any`, so
/// the core count would include it where the legacy count did not — that
/// spelling is unreachable both from lowering (`from_name` resolves `Any`)
/// and from the canonical deserialization inverse.
/// Tie-breaker 0 input (exact signature match: same fixed arity,
/// slot-for-slot equal): equality is decided on the canonical
/// `core_signature` projection against the once-bridged argument cores, with
/// the legacy `JuliaType` equality as the structured-unavailable fallback
/// (Issue #6495, stage 7a — this resolves the R1 deferral).
///
/// Accepted-divergence note (parity gate + full suite referee, zero hits):
/// `CoreType::from` is not injective, so a parameter whose *spelling* differs
/// from the argument's while sharing its image (e.g. a
/// `JuliaType::Struct("Vector{Int64}")` param against a
/// `JuliaType::VectorOf(Int64)` argument, or a single-letter
/// `JuliaType::Struct("Q")` param — which images as a `TypeVar` — against a
/// same-named `where` variable) now counts as exact where the legacy
/// comparison did not. Those param spellings are unreachable both from
/// lowering (`from_name`/`parse_type_annotation` resolve to the dedicated
/// variants) and from the canonical deserialization inverse; parity over the
/// Base corpus is pinned by `base_method_core_tiebreaker_parity_issue_6495`.
fn exact_signature_match(m: &MethodSig, arg_cores: &[CoreType]) -> bool {
    if m.vararg_param_index.is_some() {
        return false;
    }
    // Stage 7c-i: the legacy `params`-equality fallback is retired — every
    // production `MethodSig` carries a refreshed structured signature (stage
    // 7b), so `None` (a test-only `Bottom` placeholder) conservatively
    // reports "not exact".
    match m.structured_arg_core_types() {
        Some(cores) => {
            cores.len() == arg_cores.len() && cores.iter().zip(arg_cores).all(|(p, a)| p == a)
        }
        None => false,
    }
}

fn any_param_count_fixed_prefix(m: &MethodSig) -> usize {
    // Stage 7c-i: the legacy `params` fallback is retired — `None` (a
    // test-only `Bottom` placeholder, unobservable in production since stage
    // 7b) counts zero `Any` params.
    match m.structured_arg_core_types() {
        Some(cores) => {
            let fixed_count = m.vararg_param_index.unwrap_or(m.param_count());
            cores
                .iter()
                .take(fixed_count)
                .filter(|ty| matches!(ty, CoreType::Any))
                .count()
        }
        None => 0,
    }
}

/// Tie-breaker 4 input (fewest `where` type parameters): the allocation-free
/// `core_signature` `UnionAll` wrapper count (Issue #6495, stage 6a).
///
/// Stage 7c-i: the legacy `type_params.len()` fallback is retired — a
/// test-only `Bottom` placeholder has no `UnionAll` wrappers and counts 0.
fn where_param_count(m: &MethodSig) -> usize {
    m.core_signature_type_var_count()
}

/// Tie-breaker 3 (struct ancestry filter, Issue #3144): every fixed-prefix
/// slot pairing a user abstract parameter with a struct argument must satisfy
/// the declared parent chain.
///
/// The parameter side reads the `core_signature` projection (Issue #6495,
/// stage 6a). This port is image-exact, not merely gate-refereed:
/// `CoreType::AbstractUser` arises ONLY as the image of
/// `JuliaType::AbstractUser` (`CoreType::from_julia_name` never produces it,
/// so no `Struct`-name spelling can enter the arm), and the canonical inverse
/// maps it back to `JuliaType::AbstractUser` — the arm fires for exactly the
/// parameter set the legacy `JuliaType::AbstractUser` arm fired for. The
/// argument side is the caller's argument tuple (not a projection) and stays
/// on `JuliaType`.
fn ancestry_filter_passes(
    m: &MethodSig,
    arg_types: &[JuliaType],
    projection: &MethodTableProjection,
) -> bool {
    // Stage 7c-i: the legacy `params` fallback is retired — `None` (a
    // test-only `Bottom` placeholder, unobservable in production since stage
    // 7b) passes the filter vacuously (no abstract/struct slot pairs to
    // check).
    let Some(cores) = m.structured_arg_core_types() else {
        return true;
    };
    let fixed_count = m.vararg_param_index.unwrap_or(m.param_count());
    cores
        .iter()
        .take(fixed_count)
        .zip(arg_types.iter().take(fixed_count))
        .all(|(param_core, arg_ty)| {
            if let (
                CoreType::AbstractUser {
                    name: abstract_name,
                    ..
                },
                JuliaType::Struct(struct_name),
            ) = (param_core, arg_ty)
            {
                struct_is_subtype_of_abstract(struct_name, abstract_name, projection)
            } else {
                true
            }
        })
}

/// Per-match inputs for the CoreType-native dominance pre-checks (Issue #6495
/// stage 5): the `core_signature`-projected parameter lists plus the
/// projected `where` core type variables, in `matches` order.
struct CoreDominanceInputs<'a> {
    param_lists: Vec<&'a [CoreType]>,
    type_var_lists: Vec<Vec<subset_julia_vm_types::inference_core::CoreTypeVar>>,
}

impl<'a> CoreDominanceInputs<'a> {
    /// `None` when any match lacks a refreshed structured signature (a
    /// `Bottom` placeholder or a skewed projection — test-only since stage
    /// 7b; classified by [`MethodSig::signature_defect`], Issue #8549); the
    /// callers conservatively skip their pre-check instead of falling back to
    /// the retired legacy chain (stage 7c-i).
    fn for_matches(matches: &[(&'a MethodSig, u32)]) -> Option<Self> {
        let param_lists: Vec<&'a [CoreType]> = matches
            .iter()
            .map(|(m, _)| m.structured_arg_core_types())
            .collect::<Option<_>>()?;
        let type_var_lists = matches
            .iter()
            .map(|(m, _)| m.core_signature_type_vars())
            .collect();
        Some(Self {
            param_lists,
            type_var_lists,
        })
    }
}

/// Run the dominance pre-check family in its historical order, returning the
/// first rule that finds a unique dominant match (Issue #5926 family; control
/// flow shared via [`selection::unique_dominant_index`], Issue #6502).
///
/// Issue #6495 (stage 5): the projection-consuming families run on the
/// canonical `core_signature` projections (`arg_core_types` /
/// `core_signature_type_vars`, with the argument tuple bridged once by the
/// caller).
///
/// Stage 7c-i: the legacy `params`/`type_params` fallback chain is retired —
/// every production `MethodSig` carries a refreshed structured signature
/// (stage 7b), so a `None` from [`CoreDominanceInputs::for_matches`] (a
/// test-only `Bottom` placeholder among the matches) conservatively skips
/// the dominance pre-checks and defers to the score path.
fn dominance_precheck_index(
    matches: &[(&MethodSig, u32)],
    arg_types: &[JuliaType],
    arg_cores: &[CoreType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    let inputs = CoreDominanceInputs::for_matches(matches)?;
    dominant_match_index(matches, arg_types, hierarchy, base_function_count)
        .or_else(|| {
            empty_trailing_vararg_dominant_match_index(
                matches,
                arg_types.len(),
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_tuple_vararg_dominant_match_index(
                matches,
                &inputs,
                arg_types,
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_tuple_diagonal_dominant_match_index(
                matches,
                &inputs,
                arg_types,
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_union_actual_dominant_match_index(
                matches,
                &inputs,
                arg_cores,
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_type_value_diagonal_dominant_match_index(
                matches,
                &inputs,
                arg_types,
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_type_vector_diagonal_dominant_match_index(
                matches,
                &inputs,
                arg_types,
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_type_matrix_diagonal_dominant_match_index(
                matches,
                &inputs,
                arg_types,
                hierarchy,
                base_function_count,
            )
        })
        .or_else(|| {
            core_vector_diagonal_dominant_match_index(
                matches,
                &inputs,
                arg_types,
                hierarchy,
                base_function_count,
            )
        })
}

/// Whether method `a`'s fixed parameter list is strictly more specific than
/// `b`'s by the pairwise-subtype rule (Issue #5068).
///
/// Returns true when both signatures have the same fixed arity, every slot of
/// `a` is a subtype of the matching slot of `b`, and at least one slot is a
/// *strict* subtype (`a_i <: b_i` but not `b_i <: a_i`). This mirrors upstream's
/// "more specific method" tie-break and resolves bounded `Type{<:B}` dispatch
/// (`Type{<:Integer}` beats `Type{<:Number}`) where the integer specificity
/// score ties. Methods with varargs are not compared here (returns false).
/// Index of the single matching method whose where-wrapped `Tuple` signature
/// strictly dominates every other match's, if exactly one such method exists
/// (Issue #5926). This is the unambiguous-most-specific fragment of upstream's
/// `morespecific` partial order, decided by the shared subtype engine via
/// [`CoreType::strict_subtype_dominates`]. Returns `None` when no single method
/// dominates all others (a tie / mutually-incomparable set) so the caller falls
/// back to the integer score + tie-breakers.
///
/// Vararg candidates are excluded: [`MethodSig::compute_core_signature`] renders
/// a trailing `args...` as an ordinary fixed parameter rather than a `Vararg`, so
/// its signature is not subtype-faithful — those defer to the score path.
fn dominant_match_index(
    matches: &[(&MethodSig, u32)],
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }
    let sigs: Vec<CoreType> = matches.iter().map(|(m, _)| m.core_signature()).collect();
    // The candidate set for an imprecise (e.g. statically-`Any`) argument
    // conservatively includes methods the runtime value MIGHT match — so
    // `first(::String)` is a candidate for a statically-`Any` argument alongside
    // `first(::Any)`. Committing the override to the more specific of those would
    // be unsound: the value need not actually be a `String`, and codegen would
    // lower a call with the wrong slot layout (`LoadSlotStr: expected String`).
    // Gate the override on the static argument tuple being a SUBTYPE of the
    // chosen method's signature — i.e. the args DEFINITIVELY select it — so an
    // imprecise argument falls through to the score path (which defers the
    // specifics to runtime dispatch) (Issue #5926).
    let arg_tuple = CoreType::Tuple(
        arg_types
            .iter()
            .map(
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
            )
            .collect(),
    );
    selection::unique_dominant_index(
        sigs.len(),
        |i| {
            arg_tuple.is_subtype_of_with_hierarchy(&sigs[i], hierarchy)
                && !base_dominance_crosses_user_candidate(matches, i, base_function_count)
        },
        |i, j| sigs[i].strict_subtype_dominates_with_hierarchy(&sigs[j], hierarchy),
    )
}

/// Resolve the narrow vararg-specificity case where no runtime arguments remain
/// for the trailing vararg slot. Expansion erases the vararg element type from
/// scoring in calls like `f()` for `f(xs::Int64...)` vs `f(xs::Integer...)`;
/// compare the declared element types instead (Issue #6216).
fn empty_trailing_vararg_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    arg_len: usize,
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    let vararg_idx = matches.first()?.0.vararg_param_index?;
    if arg_len != vararg_idx {
        return None;
    }

    // Issue #6336/#6495: read the structured `core_signature` projection.
    // A test-only `Bottom` placeholder projects to an empty slice, failing the
    // arity check and deferring to the score path.
    for (method, _) in matches {
        if method.vararg_param_index != Some(vararg_idx)
            || method.vararg_fixed_count.is_some()
            || method.has_where_params()
            || method.arg_core_types().len() != vararg_idx + 1
        {
            return None;
        }
    }

    let prefix = &matches[0].0.arg_core_types()[..vararg_idx];
    let same_fixed_prefix = matches
        .iter()
        .all(|(method, _)| &method.arg_core_types()[..vararg_idx] == prefix);
    if !same_fixed_prefix {
        return None;
    }

    selection::unique_dominant_index(
        matches.len(),
        |i| !base_dominance_crosses_user_candidate(matches, i, base_function_count),
        |i, j| {
            let candidate_ty = &matches[i].0.arg_core_types()[vararg_idx];
            let other_ty = &matches[j].0.arg_core_types()[vararg_idx];
            candidate_ty.strict_subtype_dominates_with_hierarchy(other_ty, hierarchy)
        },
    )
}

/// Production tuple-vararg ambiguity check (Issue #6495 stage 5): expands the
/// patterns from the `core_signature` projections.
///
/// Stage 7c-i: the legacy-projection fallback is retired — a `None` from
/// [`CoreDominanceInputs::for_matches`] (a test-only `Bottom` placeholder
/// among the matches) conservatively reports "no conflict".
fn tuple_vararg_conflicting_match(
    matches: &[(&MethodSig, u32)],
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(inputs) = CoreDominanceInputs::for_matches(matches) else {
        return false;
    };
    let Some(expanded) = core_tuple_vararg_expansions_for_matches(matches, &inputs, arg_types)
    else {
        return false;
    };
    tuple_vararg_expansions_conflict(&expanded, hierarchy)
}

fn tuple_vararg_expansions_conflict(
    expanded: &[specificity::TupleVarargExpansion],
    hierarchy: &StructHierarchy,
) -> bool {
    for i in 0..expanded.len() {
        for j in (i + 1)..expanded.len() {
            if specificity::tuple_vararg_patterns_conflict(&expanded[i], &expanded[j], hierarchy) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// CoreType-native dominance pre-check families (Issue #6495 stage 5), consuming
// the canonical `core_signature` projections via [`CoreDominanceInputs`].
// ---------------------------------------------------------------------------

fn core_tuple_vararg_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    let expanded = core_tuple_vararg_expansions_for_matches(matches, inputs, arg_types)?;

    selection::unique_dominant_index(
        matches.len(),
        |i| !base_dominance_crosses_user_candidate(matches, i, base_function_count),
        |i, j| specificity::tuple_vararg_pattern_dominates(&expanded[i], &expanded[j], hierarchy),
    )
}

fn core_tuple_vararg_expansions_for_matches(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
) -> Option<Vec<specificity::TupleVarargExpansion>> {
    let [actual_arg] = arg_types else {
        return None;
    };
    let JuliaType::TupleOf(actual_elems) = actual_arg else {
        return None;
    };

    let mut expanded: Vec<specificity::TupleVarargExpansion> = Vec::new();
    for (i, (method, _)) in matches.iter().enumerate() {
        if method.vararg_param_index.is_some() || !inputs.type_var_lists[i].is_empty() {
            return None;
        }
        let [param_ty] = inputs.param_lists[i] else {
            return None;
        };
        let CoreType::Tuple(pattern_elems) = param_ty else {
            return None;
        };
        expanded.push(specificity::core_expand_tuple_vararg_pattern_for_len(
            pattern_elems,
            actual_elems.len(),
        )?);
    }
    Some(expanded)
}

fn core_tuple_diagonal_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }
    let [JuliaType::TupleOf(actual_elems)] = arg_types else {
        return None;
    };

    let patterns: Vec<Option<specificity::TupleDiagonalPattern>> = (0..matches.len())
        .map(|i| {
            specificity::core_repeated_tuple_typevar_pattern(
                inputs.param_lists[i],
                &inputs.type_var_lists[i],
            )
        })
        .collect();

    selection::unique_dominant_index(
        matches.len(),
        |i| {
            !base_dominance_crosses_user_candidate(matches, i, base_function_count)
                && patterns[i].as_ref().is_some_and(|pattern| {
                    specificity::actual_tuple_satisfies_diagonal_pattern(
                        actual_elems,
                        pattern,
                        hierarchy,
                    )
                })
        },
        |i, j| {
            patterns[i].as_ref().is_some_and(|pattern| {
                specificity::core_tuple_diagonal_candidate_dominates_other(
                    inputs.param_lists[j],
                    &inputs.type_var_lists[j],
                    pattern,
                    hierarchy,
                )
            })
        },
    )
}

fn core_union_actual_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_cores: &[CoreType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }

    selection::unique_dominant_index(
        matches.len(),
        |i| {
            !base_dominance_crosses_user_candidate(matches, i, base_function_count)
                && inputs.param_lists[i]
                    .iter()
                    .any(|ty| matches!(ty, CoreType::Union(_)))
        },
        |i, j| {
            specificity::core_union_actual_candidate_dominates(
                inputs.param_lists[i],
                inputs.param_lists[j],
                arg_cores,
                hierarchy,
            )
        },
    )
}

fn core_type_value_diagonal_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if arg_types.len() != 2 || matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }

    let bound_patterns: Vec<Option<(specificity::TypeValueDiagonalPattern, CoreType)>> = (0
        ..matches.len())
        .map(|i| {
            let pattern = specificity::core_type_value_diagonal_pattern(
                inputs.param_lists[i],
                &inputs.type_var_lists[i],
            )?;
            let binding =
                specificity::actual_type_value_diagonal_binding(arg_types, &pattern, hierarchy)?;
            Some((
                pattern,
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia(binding),
            ))
        })
        .collect();

    selection::unique_dominant_index(
        matches.len(),
        |i| {
            !base_dominance_crosses_user_candidate(matches, i, base_function_count)
                && bound_patterns[i].is_some()
        },
        |i, j| {
            bound_patterns[i]
                .as_ref()
                .is_some_and(|(pattern, binding)| {
                    specificity::core_type_value_diagonal_candidate_dominates_other(
                        inputs.param_lists[j],
                        pattern,
                        binding,
                        hierarchy,
                    )
                })
        },
    )
}

fn core_type_vector_diagonal_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if arg_types.len() != 2 || matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }

    let bound_patterns: Vec<Option<(specificity::TypeVectorDiagonalPattern, CoreType)>> = (0
        ..matches.len())
        .map(|i| {
            let pattern = specificity::core_type_vector_diagonal_pattern(
                inputs.param_lists[i],
                &inputs.type_var_lists[i],
            )?;
            let binding =
                specificity::actual_type_vector_diagonal_binding(arg_types, &pattern, hierarchy)?;
            Some((
                pattern,
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia(binding),
            ))
        })
        .collect();

    selection::unique_dominant_index(
        matches.len(),
        |i| {
            !base_dominance_crosses_user_candidate(matches, i, base_function_count)
                && bound_patterns[i].is_some()
        },
        |i, j| {
            bound_patterns[i]
                .as_ref()
                .is_some_and(|(pattern, binding)| {
                    specificity::core_type_vector_diagonal_candidate_dominates_other(
                        inputs.param_lists[j],
                        pattern,
                        binding,
                        hierarchy,
                    )
                })
        },
    )
}

fn core_type_matrix_diagonal_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if arg_types.len() != 2 || matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }

    let bound_patterns: Vec<Option<(specificity::TypeMatrixDiagonalPattern, CoreType)>> = (0
        ..matches.len())
        .map(|i| {
            let pattern = specificity::core_type_matrix_diagonal_pattern(
                inputs.param_lists[i],
                &inputs.type_var_lists[i],
            )?;
            let binding =
                specificity::actual_type_matrix_diagonal_binding(arg_types, &pattern, hierarchy)?;
            Some((
                pattern,
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia(binding),
            ))
        })
        .collect();

    selection::unique_dominant_index(
        matches.len(),
        |i| {
            !base_dominance_crosses_user_candidate(matches, i, base_function_count)
                && bound_patterns[i].is_some()
        },
        |i, j| {
            bound_patterns[i]
                .as_ref()
                .is_some_and(|(pattern, binding)| {
                    specificity::core_type_matrix_diagonal_candidate_dominates_other(
                        inputs.param_lists[j],
                        pattern,
                        binding,
                        hierarchy,
                    )
                })
        },
    )
}

fn core_vector_diagonal_dominant_match_index(
    matches: &[(&MethodSig, u32)],
    inputs: &CoreDominanceInputs<'_>,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
    base_function_count: usize,
) -> Option<usize> {
    if matches.iter().any(|(m, _)| m.vararg_param_index.is_some()) {
        return None;
    }

    let patterns: Vec<Option<specificity::VectorDiagonalPattern<'_>>> = (0..matches.len())
        .map(|i| {
            specificity::core_repeated_vector_typevar_pattern(
                inputs.param_lists[i],
                &inputs.type_var_lists[i],
            )
        })
        .collect();

    selection::unique_dominant_index(
        matches.len(),
        |i| {
            !base_dominance_crosses_user_candidate(matches, i, base_function_count)
                && patterns[i].as_ref().is_some_and(|pattern| {
                    specificity::actual_vector_slots_share_element_type(arg_types, &pattern.slots)
                })
        },
        |i, j| {
            patterns[i].as_ref().is_some_and(|pattern| {
                specificity::core_independent_vector_bounds_are_no_tighter(
                    inputs.param_lists[j],
                    pattern,
                    hierarchy,
                )
            })
        },
    )
}

fn base_dominance_crosses_user_candidate(
    matches: &[(&MethodSig, u32)],
    candidate_idx: usize,
    base_function_count: usize,
) -> bool {
    base_function_count > 0
        && matches[candidate_idx]
            .0
            .is_base_program_method(base_function_count)
        && matches
            .iter()
            .any(|(m, _)| !m.is_base_program_method(base_function_count))
}

fn static_arg_tuple_satisfies_method(
    method: &MethodSig,
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
) -> bool {
    // Issue #6495 (stage 1): read the structured `core_signature` projection
    // directly — this site previously expanded the legacy `params` projection
    // and immediately mapped `CoreType::from` over it, which produces the
    // identical tuple by construction (pinned by the Base-corpus parity gate
    // `base_method_expanded_core_params_parity_issue_6495`). Stage 7c-i: the
    // legacy-expansion bridge fallback is retired (a refreshed structured
    // signature is always available in production — stage 7b).
    let Some(param_cores) = method.expanded_core_param_types_for_arity(arg_types.len()) else {
        return false;
    };
    let mut sig = CoreType::Tuple(param_cores);
    for type_param in method.core_signature_type_vars().into_iter().rev() {
        sig = CoreType::UnionAll {
            var: type_param,
            body: Box::new(sig),
        };
    }
    let arg_tuple = CoreType::Tuple(
        arg_types
            .iter()
            .map(
                subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
            )
            .collect(),
    );
    arg_tuple.is_subtype_of_with_hierarchy(&sig, hierarchy)
}

/// Tie-breaker 5 (Issue #5068): `a` is pairwise strictly more specific than
/// `b` on the shared subtype engine.
///
/// Issue #6495 (stage 6a): the engine always consumed the per-pair
/// `CoreType::from` images of the legacy `params` projection; it now borrows
/// the canonical `core_signature` projection directly (elementwise-equal by
/// construction at build time and by the #6336 round-trip gates at
/// deserialization — re-pinned per method by
/// `base_method_core_tiebreaker_parity_issue_6495`).
///
/// Stage 7c-i: the legacy `CoreType::from` bridge fallback is retired — a
/// test-only `Bottom` placeholder (unobservable in production since stage
/// 7b) is conservatively "not strictly more specific".
fn method_params_strictly_more_specific(a: &MethodSig, b: &MethodSig) -> bool {
    if a.vararg_param_index.is_some() || b.vararg_param_index.is_some() {
        return false;
    }
    let (Some(a_cores), Some(b_cores)) =
        (a.structured_arg_core_types(), b.structured_arg_core_types())
    else {
        return false;
    };
    if a_cores.len() != b_cores.len() {
        return false;
    }
    let subtype = CoreSubtypeEngine::new();
    let mut has_strict = false;
    for (a_ty, b_ty) in a_cores.iter().zip(b_cores.iter()) {
        if !subtype.is_subtype(a_ty, b_ty) {
            return false;
        }
        if !subtype.is_subtype(b_ty, a_ty) {
            has_strict = true;
        }
    }
    has_strict
}

/// Whether a rendered function-singleton type name such as `typeof(sin)` or
/// `typeof(+)` came from a `Value::Function`. These parse to a bare
/// `JuliaType::Struct("typeof(...)")`, so the struct-parents fallback must not
/// treat them as unknown user abstract types (Issue #7334).
fn is_function_singleton_struct_name(name: &str) -> bool {
    name.starts_with("typeof(") && name.ends_with(')')
}

/// Strict registered-subtype check: `true` only when `struct_name` is genuinely
/// registered in the declared/built-in hierarchy as a subtype of `abstract_name`
/// (Issue #8149). An *unknown* struct (no declared parent and no known built-in
/// supertype chain reaching the bound) and a cycle both return `false` — there
/// is NO "conservatively accept unknown struct" branch.
///
/// This is required by the binary `==`/`!=` array-routing decision, which sits on
/// the GLOBAL `==` compile path: a false positive there would mis-route an
/// unrelated `native-array == some-struct` pair through the array `isequal`
/// builtin (a dispatch/codegen-class regression). The walk reuses the same
/// declared-parent (`declared_parent_link`) + built-in supertype
/// (`direct_builtin_supertype_name_for_julia_name`) chain as
/// `struct_is_subtype_of_abstract`, e.g. resolving the registered StaticArrays
/// chain `SVector <: StaticVector <: StaticArray <: AbstractArray` or a user
/// `struct MyArr{T} <: AbstractVector{T}` (`AbstractVector <: AbstractArray`).
pub fn struct_is_registered_subtype_of_abstract(
    struct_name: &str,
    abstract_name: &str,
    projection: &MethodTableProjection,
) -> bool {
    let struct_base = nominal_family_name(struct_name);
    let abstract_name = nominal_family_name(abstract_name);
    if struct_base == abstract_name {
        return true;
    }
    // First parent link. An unknown struct is NOT accepted: fall back only to a
    // *known* built-in supertype (filtering the self-edge), otherwise stop.
    let first_parent = match projection.declared_parent_link(struct_base) {
        Some(parent_opt) => parent_opt,
        None => CoreType::direct_builtin_supertype_name_for_julia_name(struct_base)
            .filter(|builtin_parent| *builtin_parent != struct_base)
            .map(str::to_string),
    };
    let mut current = first_parent;
    let mut visited = 0usize;
    while let Some(parent) = current {
        visited += 1;
        if visited > 64 {
            // Cycle guard: strict — do not accept on overflow.
            return false;
        }
        let parent_base = nominal_family_name(&parent);
        if parent_base == abstract_name {
            return true;
        }
        if parent_base == "Any" {
            return false;
        }
        current = projection
            .declared_parent_link(parent_base)
            .unwrap_or_else(|| {
                CoreType::direct_builtin_supertype_name_for_julia_name(parent_base)
                    .map(str::to_string)
            });
    }
    false
}

/// Check whether a concrete struct is a subtype of an abstract type using the
/// declared/built-in parent hierarchy (Issue #3144, strictened by #8439).
///
/// Walks up the struct's declared parent chain until the abstract type is found
/// (returns true) or the chain ends without a match (returns false). Unknown
/// struct names are rejected rather than treated as possible matches.
fn struct_is_subtype_of_abstract(
    struct_name: &str,
    abstract_name: &str,
    projection: &MethodTableProjection,
) -> bool {
    let struct_base = nominal_family_name(struct_name);
    let abstract_name = nominal_family_name(abstract_name);

    // Issue #7334: a function-singleton type name (`typeof(sin)`, `typeof(+)`)
    // is NOT an unknown user abstract type — it is a known concrete type whose
    // only abstract supertypes are `Function` (`Core.Builtin` for built-in
    // functions) and `Any`, never an array/numeric/range abstract. It parses to
    // a bare `JuliaType::Struct("typeof(sin)")`, so `declared_parent_link` and
    // `direct_builtin_supertype_name_for_julia_name` both miss it and the
    // old unknown-name fallback wrongly reported e.g. `typeof(sin) <:
    // AbstractMatrix` — so `h(sin)` loose-matched (and even won dispatch over
    // the specific `h(::Function)` method) an `h(::AbstractMatrix)` method that
    // upstream Julia rejects. Resolve it through its known built-in `Function`
    // supertype so the chain walk decides correctly.
    if is_function_singleton_struct_name(struct_base) {
        // `typeof(f) <: Function <: Any`, and no other abstract. (A `::Function`
        // parameter is a dedicated `JuliaType::Function`, matched by the primary
        // core matcher, not this `AbstractUser` fallback, so this arm only needs
        // to reject the unrelated abstracts that triggered the loose match.)
        return matches!(nominal_family_name(abstract_name), "Function" | "Any");
    }
    struct_is_registered_subtype_of_abstract(struct_base, abstract_name, projection)
}

/// Parameter-aware match for a parametric ABSTRACT supertype slot
/// (`AbsM{2,2,T}`, `Container{Int64}`) against a concrete struct argument
/// (`ConM{2,2,Float64}`, `IntBox`), Issue #7960.
///
/// Returns `Some(true)`/`Some(false)` only when `abstract_name` carries
/// parameters — the case the bare-family nominal walk
/// (`struct_is_subtype_of_abstract`) cannot decide because it drops parameters,
/// so `AbsM{2,2,T}`/`AbsM{3,3,T}` and `Container{Int64}`/`Container{Float64}`
/// look identical. Returns `None` for a bare abstract so the caller keeps its
/// nominal decision.
///
/// The concrete argument is projected up its declared parent chain to the
/// supertype's instantiation (`ConM{2,2,Float64}` -> `AbsM{2,2,Float64}`) and
/// then matched invariantly against the pattern, binding the method `where`
/// variables and rejecting any value-parameter mismatch.
fn abstract_value_param_match(
    struct_name: &str,
    abstract_name: &str,
    type_params: &[TypeParam],
    projection: &MethodTableProjection,
) -> Option<bool> {
    let pattern = CoreType::from_julia_name_for_dispatch(abstract_name);
    let CoreType::Struct {
        name: pattern_family,
        params: pattern_params,
    } = &pattern
    else {
        return None;
    };
    if pattern_params.is_empty() {
        // No parameters: the nominal family walk already decides this.
        return None;
    }
    let target_family = nominal_family_name(pattern_family).to_string();

    let arg_core = CoreType::from_julia_name_for_dispatch(struct_name);
    let empty_params: &[CoreType] = &[];
    let (arg_family, arg_params) = match &arg_core {
        CoreType::Struct { name, params } => (name.as_str(), params.as_slice()),
        CoreType::Named(name) => (name.as_str(), empty_params),
        _ => return Some(false),
    };
    let mapped = if nominal_family_name(arg_family) == target_family {
        CoreType::Struct {
            name: arg_family.to_string(),
            params: arg_params.to_vec(),
        }
    } else {
        match subset_julia_vm_types::inference_core::registered_instantiated_struct_supertype_in(
            &projection.struct_hierarchy,
            arg_family,
            arg_params,
            &target_family,
        ) {
            Some(mapped) => mapped,
            None => return Some(false),
        }
    };

    let core_vars: Vec<subset_julia_vm_types::inference_core::CoreTypeVar> = type_params
        .iter()
        .map(subset_julia_vm_types::inference_core::CoreTypeVar::from)
        .collect();
    Some(
        dispatch_resolver::core_match::core_signature_match_with_bindings_with_hierarchy(
            std::slice::from_ref(&pattern),
            std::slice::from_ref(&mapped),
            &core_vars,
            &projection.struct_hierarchy,
        )
        .is_some(),
    )
}

/// The Julia type name for a built-in abstract type used as a method parameter,
/// or `None` for any other `JuliaType`. Lets struct arguments be matched against
/// built-in abstract parameters (`::Real`, `::Number`, `::AbstractArray`, ...)
/// via their declared supertype chain, mirroring the existing `AbstractUser`
/// handling (Issue #5363).
///
/// Issue #8229: the numeric-only list silently dropped the non-numeric built-in
/// abstracts, so a user `struct MyVec <: AbstractVector{Float64}` (or a
/// `SubArray` view) did NOT match an `f(::AbstractArray)` method even though
/// `MyVec(...) isa AbstractArray` is `true` at runtime — the compile-time static
/// dispatch concluded "no matching method" and emitted a hard `MethodError`
/// without ever falling back to the runtime dispatch that would have succeeded.
/// The struct's declared parent reaches the abstract only through a built-in
/// *grandparent* link (`AbstractVector <: AbstractArray`), which the primary
/// core matcher does not walk; routing it through this fallback consults
/// `struct_is_subtype_of_abstract`, which does.
///
/// Only `AbstractArray` is added here (alongside the original numeric set): it
/// is the bound #8229 needs, and the struct-parent walk now rejects unknown
/// names that do not have a declared or built-in chain to the requested bound.
fn builtin_abstract_param_name(ty: &JuliaType) -> Option<&'static str> {
    match ty {
        JuliaType::Number => Some("Number"),
        JuliaType::Real => Some("Real"),
        JuliaType::Integer => Some("Integer"),
        JuliaType::Signed => Some("Signed"),
        JuliaType::Unsigned => Some("Unsigned"),
        JuliaType::AbstractFloat => Some("AbstractFloat"),
        JuliaType::AbstractArray => Some("AbstractArray"),
        // Issue #8560: every remaining built-in abstract that a user struct can
        // reach through its DECLARED supertype chain must consult the same
        // struct-parents walk, or compile-time selection silently drops to a
        // less specific method for `struct S <: AbstractString / AbstractChar /
        // AbstractRange / Function (functors) / IO` arguments while runtime
        // dispatch (engine-backed `check_subtype`) accepts them.
        JuliaType::AbstractString => Some("AbstractString"),
        JuliaType::AbstractChar => Some("AbstractChar"),
        JuliaType::AbstractRange => Some("AbstractRange"),
        JuliaType::Function => Some("Function"),
        JuliaType::IO => Some("IO"),
        _ => None,
    }
}

/// Match one method against a call-site argument tuple, returning the
/// `where`-binding count on success (Issue #6495 stage 3).
///
/// Primary matching consumes the canonical `core_signature` projections
/// (`expanded_core_param_types_for_arity` / `core_signature_type_vars`) via
/// the CoreType-native matcher (`dispatch_resolver::core_match`); decision
/// equality with the legacy matcher over the whole Base corpus is pinned by
/// `compile::cache::tests::base_method_core_dispatch_match_parity_issue_6495`.
/// On primary failure the user-defined struct-parents fallback
/// ([`struct_parents_fallback_match`], unported — it walks declared parent
/// links, not signature shapes) runs exactly as before.
///
/// Stage 7c-i: the historical structured-unavailable fallback onto the full
/// legacy pipeline is retired. Every production `MethodSig` carries a
/// refreshed `core_signature` ([`MethodSig::from_julia_projections`] is the
/// only production constructor and [`MethodTable::add_method`] /
/// deserialization refresh unconditionally — stage 7b), so
/// `expanded_core_param_types_for_arity` returns `None` exactly when the
/// arity is rejected — where the legacy expansion returned `None` too (same
/// rules, pinned by `base_method_expanded_core_params_parity_issue_6495`).
fn method_match_binding_count(
    method: &MethodSig,
    arg_types: &[JuliaType],
    arg_cores: &[CoreType],
    projection: &MethodTableProjection,
) -> Option<usize> {
    let core_params = method.expanded_core_param_types_for_arity(arg_types.len())?;

    let core_vars = method.core_signature_type_vars();
    dispatch_resolver::core_match::core_signature_match_with_bindings_with_hierarchy(
        &core_params,
        arg_cores,
        &core_vars,
        &projection.struct_hierarchy,
    )
    .or_else(|| {
        let param_types = method.expanded_projected_param_julia_types_for_arity(arg_types.len())?;
        let type_params = method.projected_type_params();
        struct_parents_fallback_match(&param_types, arg_types, &type_params, projection)
    })
}

fn prebind_method_positional_type_args(
    method: &MethodSig,
    positional_type_args: &[JuliaType],
    hierarchy: &StructHierarchy,
) -> Option<MethodSig> {
    let type_vars = method.core_signature_type_vars();
    if type_vars.len() != positional_type_args.len() {
        return None;
    }

    let supplied_cores: Vec<CoreType> = positional_type_args
        .iter()
        .map(
            subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia,
        )
        .collect();
    let invariant_var_patterns: Vec<CoreType> = type_vars
        .iter()
        .cloned()
        .map(CoreType::TypeVar)
        .map(Box::new)
        .map(CoreType::TypeOf)
        .collect();
    let invariant_supplied_types: Vec<CoreType> = supplied_cores
        .iter()
        .cloned()
        .map(Box::new)
        .map(CoreType::TypeOf)
        .collect();
    dispatch_resolver::core_match::core_signature_match_with_bindings_with_hierarchy(
        &invariant_var_patterns,
        &invariant_supplied_types,
        &type_vars,
        hierarchy,
    )?;

    let mut substitutions = HashMap::new();
    for (var, supplied) in type_vars.iter().zip(supplied_cores) {
        substitutions.insert(var.name.clone(), supplied.clone());
        substitutions.insert(core_type_var_base_name(&var.name).to_string(), supplied);
    }
    let projected_args = method
        .structured_arg_core_types()?
        .iter()
        .map(|arg| substitute_core_type_vars(arg, &substitutions))
        .collect();
    let mut projected = method.clone();
    projected.core_signature = CoreType::Tuple(projected_args);
    debug_assert!(projected.signature_defect().is_none());
    Some(projected)
}

fn core_type_var_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

fn substitute_core_type_vars(ty: &CoreType, substitutions: &HashMap<String, CoreType>) -> CoreType {
    match ty {
        CoreType::TypeVar(var) => substitutions
            .get(&var.name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        CoreType::Named(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_) => ty.clone(),
        CoreType::Struct { name, params } => CoreType::Struct {
            name: name.clone(),
            params: params
                .iter()
                .map(|param| substitute_core_type_vars(param, substitutions))
                .collect(),
        },
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| substitute_core_type_vars(element, substitutions))
                .collect(),
        ),
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|member| substitute_core_type_vars(member, substitutions))
                .collect(),
        ),
        CoreType::Vararg(inner) => {
            CoreType::Vararg(Box::new(substitute_core_type_vars(inner, substitutions)))
        }
        CoreType::VarargLen { element, len } => CoreType::VarargLen {
            element: Box::new(substitute_core_type_vars(element, substitutions)),
            len: Box::new(substitute_core_type_vars(len, substitutions)),
        },
        CoreType::TypeOf(inner) => {
            CoreType::TypeOf(Box::new(substitute_core_type_vars(inner, substitutions)))
        }
        CoreType::NamedTuple(fields) => CoreType::NamedTuple(
            fields
                .iter()
                .map(|(name, field_ty)| {
                    (
                        name.clone(),
                        substitute_core_type_vars(field_ty, substitutions),
                    )
                })
                .collect(),
        ),
        CoreType::UnionAll { var, body } => {
            let mut inner_substitutions = substitutions.clone();
            inner_substitutions.remove(&var.name);
            CoreType::UnionAll {
                var: var.clone(),
                body: Box::new(substitute_core_type_vars(body, &inner_substitutions)),
            }
        }
    }
}

/// User-defined struct-parents fallback (Issues #3144/#5363/#5646): when the
/// binding matcher rejects, retry slot-for-slot against the declared parent
/// chain of struct arguments. Returns `Some(0)` (no `where` bindings) on
/// success.
fn struct_parents_fallback_match(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    type_params: &[TypeParam],
    projection: &MethodTableProjection,
) -> Option<usize> {
    if param_types.len() != arg_types.len() {
        return None;
    }
    if !projection.has_parent_links() {
        return param_types
            .iter()
            .zip(arg_types)
            .any(|(param, arg)| {
                function_singleton_matches_function_param(param, arg)
                    || hierarchy_free_builtin_parent_match(param, arg, type_params)
            })
            .then(|| {
                param_types.iter().zip(arg_types).all(|(param, arg)| {
                    dispatch_resolver::julia_signature_match_with_bindings(
                        std::slice::from_ref(param),
                        std::slice::from_ref(arg),
                        type_params,
                    )
                    .is_some()
                        || function_singleton_matches_function_param(param, arg)
                        || hierarchy_free_builtin_parent_match(param, arg, type_params)
                })
            })
            .and_then(|matched| matched.then_some(0));
    }
    if !param_types
        .iter()
        .zip(arg_types)
        .any(|(param, arg)| needs_struct_parent_fallback(param, arg, type_params, projection))
    {
        return None;
    }
    param_types
        .iter()
        .zip(arg_types)
        .all(|(param, arg)| {
            julia_type_matches_with_struct_parents(param, arg, type_params, projection)
        })
        .then_some(0)
}

fn needs_struct_parent_fallback(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
    projection: &MethodTableProjection,
) -> bool {
    match (param_ty, arg_ty) {
        (param, arg) if function_singleton_matches_function_param(param, arg) => true,
        (JuliaType::AbstractUser(abstract_name, _), JuliaType::Struct(struct_name)) => {
            struct_is_subtype_of_abstract(struct_name, abstract_name, projection)
        }
        (JuliaType::AbstractUser(abstract_name, _), JuliaType::Union(types)) => types
            .iter()
            .all(|ty| arg_type_is_subtype_of_abstract_with_parents(ty, abstract_name, projection)),
        (JuliaType::TypeOf(param_inner), JuliaType::TypeOf(arg_inner)) => {
            needs_struct_parent_fallback(param_inner, arg_inner, type_params, projection)
        }
        (JuliaType::TypeVar(_, Some(bound)), JuliaType::Struct(struct_name)) => {
            struct_is_subtype_of_abstract(struct_name, bound, projection)
        }
        (JuliaType::TypeVar(var_name, None), JuliaType::Struct(struct_name))
            if specificity::find_type_param(type_params, var_name).is_some() =>
        {
            specificity::find_type_param(type_params, var_name)
                .and_then(specificity::type_param_upper_bound)
                .is_some_and(|bound| struct_is_subtype_of_abstract(struct_name, bound, projection))
        }
        (JuliaType::Struct(var_name), JuliaType::Struct(struct_name))
            if specificity::find_type_param(type_params, var_name).is_some() =>
        {
            specificity::find_type_param(type_params, var_name)
                .and_then(specificity::type_param_upper_bound)
                .is_some_and(|bound| struct_is_subtype_of_abstract(struct_name, bound, projection))
        }
        (param, JuliaType::Struct(struct_name)) => {
            native_range_family_exact_match(param, struct_name)
                || abstract_array_parent_match(param, struct_name, type_params, projection)
                    .is_some()
                || abstract_range_parent_match(param, struct_name, type_params).is_some()
                || builtin_abstract_param_name(param).is_some_and(|abstract_name| {
                    struct_is_subtype_of_abstract(struct_name, abstract_name, projection)
                })
        }
        _ => false,
    }
}

fn hierarchy_free_builtin_parent_match(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
) -> bool {
    match arg_ty {
        JuliaType::Struct(struct_name) => {
            native_range_family_exact_match(param_ty, struct_name)
                || abstract_range_parent_match(param_ty, struct_name, type_params).unwrap_or(false)
        }
        _ => false,
    }
}

fn native_range_family_exact_match(param_ty: &JuliaType, struct_name: &str) -> bool {
    matches!(
        (param_ty, nominal_family_name(struct_name)),
        (JuliaType::UnitRange, "UnitRange") | (JuliaType::StepRange, "StepRange")
    )
}

fn function_singleton_matches_function_param(param_ty: &JuliaType, arg_ty: &JuliaType) -> bool {
    matches!(param_ty, JuliaType::Function)
        && matches!(
            arg_ty,
            JuliaType::Struct(name) if name.starts_with("typeof(") && name.ends_with(')')
        )
}

fn julia_type_matches_with_struct_parents(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
    projection: &MethodTableProjection,
) -> bool {
    if dispatch_resolver::julia_signature_match_with_bindings(
        std::slice::from_ref(param_ty),
        std::slice::from_ref(arg_ty),
        type_params,
    )
    .is_some()
    {
        return true;
    }
    if function_singleton_matches_function_param(param_ty, arg_ty) {
        return true;
    }

    match (param_ty, arg_ty) {
        (JuliaType::AbstractUser(abstract_name, _), JuliaType::Struct(struct_name)) => {
            // A parametric abstract supertype (`AbsM{2,2,T}`,
            // `Container{Int64}`) is decided by projecting the concrete subtype
            // up to the supertype and comparing its parameters; the bare-family
            // nominal walk cannot distinguish sibling instantiations
            // (Issue #7960).
            if let Some(matched) =
                abstract_value_param_match(struct_name, abstract_name, type_params, projection)
            {
                return matched;
            }
            struct_is_subtype_of_abstract(struct_name, abstract_name, projection)
        }
        (JuliaType::AbstractUser(abstract_name, _), JuliaType::Union(types)) => types
            .iter()
            .all(|ty| arg_type_is_subtype_of_abstract_with_parents(ty, abstract_name, projection)),
        (JuliaType::TypeOf(param_inner), JuliaType::TypeOf(arg_inner)) => {
            type_object_inner_matches_with_struct_parents(
                param_inner,
                arg_inner,
                type_params,
                projection,
            )
        }
        // A where-bounded type parameter (`f(x::T) where {T<:Shape}`) matched
        // against a struct argument: resolve the struct's declared supertype
        // chain against the bound. The bound carrier appears either as a
        // `TypeVar` with its bound attached, or as a bare `Struct(var_name)`
        // whose bound lives on the method's `type_params`. These mirror the two
        // arms in `needs_struct_parent_fallback` (the gate); without the matching
        // arms here the gate triggers the fallback but the actual match check
        // fell through to `_ => false`, so a PARAMETRIC struct argument
        // (`Circle{Float64}`, which the standard binding resolver cannot relate
        // to a user abstract bound) never matched (Issue #5646).
        (JuliaType::TypeVar(_, Some(bound)), JuliaType::Struct(struct_name)) => {
            struct_is_subtype_of_abstract(struct_name, bound, projection)
        }
        (JuliaType::TypeVar(var_name, None), JuliaType::Struct(struct_name))
            if specificity::find_type_param(type_params, var_name).is_some() =>
        {
            specificity::find_type_param(type_params, var_name)
                .and_then(specificity::type_param_upper_bound)
                .is_some_and(|bound| struct_is_subtype_of_abstract(struct_name, bound, projection))
        }
        (JuliaType::Struct(var_name), JuliaType::Struct(struct_name))
            if specificity::find_type_param(type_params, var_name).is_some() =>
        {
            specificity::find_type_param(type_params, var_name)
                .and_then(specificity::type_param_upper_bound)
                .is_some_and(|bound| struct_is_subtype_of_abstract(struct_name, bound, projection))
        }
        (param, JuliaType::Struct(struct_name)) => {
            if native_range_family_exact_match(param, struct_name) {
                return true;
            }
            if let Some(matched) =
                abstract_array_parent_match(param, struct_name, type_params, projection)
            {
                return matched;
            }
            if let Some(matched) = abstract_range_parent_match(param, struct_name, type_params) {
                return matched;
            }
            builtin_abstract_param_name(param).is_some_and(|abstract_name| {
                struct_is_subtype_of_abstract(struct_name, abstract_name, projection)
            })
        }
        _ => false,
    }
}

fn arg_type_is_subtype_of_abstract_with_parents(
    arg_ty: &JuliaType,
    abstract_name: &str,
    projection: &MethodTableProjection,
) -> bool {
    match arg_ty {
        JuliaType::Struct(struct_name) => {
            struct_is_subtype_of_abstract(struct_name, abstract_name, projection)
        }
        JuliaType::Union(types) => types
            .iter()
            .all(|ty| arg_type_is_subtype_of_abstract_with_parents(ty, abstract_name, projection)),
        _ => false,
    }
}

fn abstract_array_parent_match(
    param_ty: &JuliaType,
    struct_name: &str,
    type_params: &[TypeParam],
    projection: &MethodTableProjection,
) -> Option<bool> {
    if specificity::abstract_vector_param_type(param_ty).is_none()
        && specificity::abstract_matrix_param_type(param_ty).is_none()
    {
        return None;
    }
    let projected_parent = projected_direct_parent_type(struct_name, projection)?;
    let signature_matches = dispatch_resolver::julia_signature_match_with_bindings(
        std::slice::from_ref(param_ty),
        std::slice::from_ref(&projected_parent),
        type_params,
    )
    .is_some();
    Some(signature_matches || projected_parent.is_subtype_of_parametric(param_ty, type_params))
}

fn abstract_range_parent_match(
    param_ty: &JuliaType,
    struct_name: &str,
    type_params: &[TypeParam],
) -> Option<bool> {
    let JuliaType::Struct(param_name) = param_ty else {
        return None;
    };
    if nominal_family_name(param_name) != "AbstractRange" {
        return None;
    }
    if !matches!(
        nominal_family_name(struct_name),
        "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange" | "OneTo"
    ) {
        return Some(false);
    }
    Some(
        dispatch_resolver::julia_signature_match_with_bindings(
            std::slice::from_ref(param_ty),
            std::slice::from_ref(&JuliaType::Struct(struct_name.to_string())),
            type_params,
        )
        .is_some(),
    )
}

fn projected_direct_parent_type(
    struct_name: &str,
    projection: &MethodTableProjection,
) -> Option<JuliaType> {
    let entry = projection.struct_hierarchy.entry(struct_name)?;
    let parent = entry.parent()?;
    let actual_args = parametric_arg_tokens(struct_name);
    let type_params = entry.type_params();
    let parent_args = parametric_arg_tokens(parent);
    if actual_args.is_empty() || type_params.is_empty() || parent_args.is_empty() {
        return Some(JuliaType::from_name_or_struct(parent));
    }
    let parent_base = parent.split('{').next().unwrap_or(parent);
    let rendered = parent_args
        .into_iter()
        .map(|arg| {
            substitute_parent_token(&arg, type_params, &actual_args)
                .unwrap_or_else(|| arg.to_string())
        })
        .collect::<Vec<_>>();
    Some(JuliaType::from_name_or_struct(&format!(
        "{}{{{}}}",
        parent_base,
        rendered.join(", ")
    )))
}

fn substitute_parent_token(
    token: &str,
    type_params: &[String],
    actual_args: &[String],
) -> Option<String> {
    let token = token.trim();
    type_params
        .iter()
        .position(|param| param == token)
        .and_then(|idx| actual_args.get(idx).cloned())
}

fn parametric_arg_tokens(name: &str) -> Vec<String> {
    let Some(start) = name.find('{') else {
        return Vec::new();
    };
    let inner = &name[start + 1..name.len().saturating_sub(1)];
    split_top_level_commas(inner)
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                result.push(s[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    result.push(s[start..].trim().to_string());
    result
}

fn type_object_inner_matches_with_struct_parents(
    param_inner: &JuliaType,
    arg_inner: &JuliaType,
    type_params: &[TypeParam],
    projection: &MethodTableProjection,
) -> bool {
    if dispatch_resolver::julia_signature_match_with_bindings(
        std::slice::from_ref(param_inner),
        std::slice::from_ref(arg_inner),
        type_params,
    )
    .is_some()
    {
        return true;
    }

    match (param_inner, arg_inner) {
        (JuliaType::TypeVar(_, Some(bound)), JuliaType::Struct(struct_name)) => {
            struct_is_subtype_of_abstract(struct_name, bound, projection)
        }
        (JuliaType::TypeVar(var_name, None), JuliaType::Struct(struct_name))
            if specificity::find_type_param(type_params, var_name).is_some() =>
        {
            specificity::find_type_param(type_params, var_name)
                .and_then(specificity::type_param_upper_bound)
                .is_some_and(|bound| struct_is_subtype_of_abstract(struct_name, bound, projection))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subset_julia_vm_types::inference_core::CoreTypeVarId;

    fn explicit_inner_for_test(
        global_index: usize,
        params: Vec<(String, JuliaType)>,
        type_params: Vec<TypeParam>,
        self_args: Vec<TypeExpr>,
    ) -> MethodSig {
        let mut sig = MethodSig::from_julia_projections(
            0,
            global_index,
            params,
            ValueType::Any,
            None,
            false,
            type_params,
            None,
            None,
        );
        sig.explicit_constructor_type_parameter_names = sig
            .core_signature_type_vars()
            .into_iter()
            .map(|var| var.name)
            .collect();
        sig.explicit_constructor_type_arguments = self_args;
        sig.explicit_constructor_type_name = Some("Main.Ctor".to_string());
        sig
    }

    #[test]
    fn constructor_self_dedup_preserves_value_binder_correlation_issue_10959() {
        let type_params = vec![
            TypeParam::new("T".to_string()),
            TypeParam::new("S".to_string()),
        ];
        let first = explicit_inner_for_test(
            1,
            vec![("x".to_string(), JuliaType::TypeVar("T".to_string(), None))],
            type_params.clone(),
            vec![
                TypeExpr::TypeVar("T".to_string()),
                TypeExpr::TypeVar("S".to_string()),
            ],
        );
        let second = explicit_inner_for_test(
            2,
            vec![("x".to_string(), JuliaType::TypeVar("T".to_string(), None))],
            type_params,
            vec![
                TypeExpr::TypeVar("S".to_string()),
                TypeExpr::TypeVar("T".to_string()),
            ],
        );
        let mut table = MethodTable::new("Ctor".to_string());
        table.add_inner_constructor_method(first, ConstructorSelfFamily::ExplicitParametricInner);
        table.add_inner_constructor_method(second, ConstructorSelfFamily::ExplicitParametricInner);
        assert_eq!(table.methods.len(), 2);
    }

    #[test]
    fn constructor_self_dedup_normalizes_vector_array_alias_issue_10959() {
        let first = explicit_inner_for_test(
            1,
            vec![],
            vec![TypeParam::new("S".to_string())],
            vec![TypeExpr::Parameterized {
                base: "Vector".to_string(),
                params: vec![TypeExpr::TypeVar("S".to_string())],
            }],
        );
        let second = explicit_inner_for_test(
            2,
            vec![],
            vec![TypeParam::new("S".to_string())],
            vec![TypeExpr::Parameterized {
                base: "Array".to_string(),
                params: vec![
                    TypeExpr::TypeVar("S".to_string()),
                    TypeExpr::Concrete(JuliaType::Struct("1".to_string())),
                ],
            }],
        );
        let mut table = MethodTable::new("Ctor".to_string());
        table.add_inner_constructor_method(first, ConstructorSelfFamily::ExplicitParametricInner);
        table.add_inner_constructor_method(second, ConstructorSelfFamily::ExplicitParametricInner);
        assert_eq!(table.methods.len(), 1);
        assert_eq!(table.methods[0].global_index, 2);
    }

    #[test]
    fn constructor_self_dedup_preserves_self_binder_bounds_issue_11019() {
        let number = explicit_inner_for_test(
            1,
            vec![],
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            vec![TypeExpr::TypeVar("T".to_string())],
        );
        let string = explicit_inner_for_test(
            2,
            vec![],
            vec![TypeParam::with_upper_bound(
                "S".to_string(),
                "AbstractString".to_string(),
            )],
            vec![TypeExpr::TypeVar("S".to_string())],
        );
        let mut table = MethodTable::new("Ctor".to_string());
        table.add_inner_constructor_method(number, ConstructorSelfFamily::ExplicitParametricInner);
        table.add_inner_constructor_method(string, ConstructorSelfFamily::ExplicitParametricInner);

        assert_eq!(table.methods.len(), 2);
        assert_eq!(table.methods[0].global_index, 1);
        assert_eq!(table.methods[1].global_index, 2);
    }

    #[test]
    fn constructor_self_dedup_preserves_qualified_bound_owner_issue_11019() {
        let owner_a = explicit_inner_for_test(
            1,
            vec![],
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "A.Bound".to_string(),
            )],
            vec![TypeExpr::TypeVar("T".to_string())],
        );
        let owner_b = explicit_inner_for_test(
            2,
            vec![],
            vec![TypeParam::with_upper_bound(
                "S".to_string(),
                "B.Bound".to_string(),
            )],
            vec![TypeExpr::TypeVar("S".to_string())],
        );
        let mut table = MethodTable::new("Ctor".to_string());
        table.add_inner_constructor_method(owner_a, ConstructorSelfFamily::ExplicitParametricInner);
        table.add_inner_constructor_method(owner_b, ConstructorSelfFamily::ExplicitParametricInner);

        assert_eq!(table.methods.len(), 2);
        assert_eq!(table.methods[0].global_index, 1);
        assert_eq!(table.methods[1].global_index, 2);
    }

    /// A single-parameter method whose declared first parameter is `first`.
    /// Passing `CoreType::Bottom` makes `for_tests` derive the real structured
    /// `core_signature` from the projected params (Issue #9197 S5 index tests).
    fn first_arg_method(global_index: usize, first: JuliaType) -> MethodSig {
        MethodSig::for_tests(
            0,
            global_index,
            vec![("x".to_string(), first)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        )
    }

    /// Issues #10775 / #11492: a concrete parametric method is applicable only
    /// when every invariant parameter matches.  The original bug admitted
    /// `Complex{Float32}` / `Complex{Float64}` methods for `Complex{Int64}` and
    /// then exposed HashMap iteration order through the selected method.  Pin
    /// every registration order, while also proving that exact concrete
    /// instantiations remain applicable.
    #[test]
    fn concrete_parametric_complex_methods_reject_other_element_types_in_all_orders_10775() {
        let method = |global_index, param, type_params| {
            MethodSig::from_julia_projections(
                0,
                global_index,
                vec![("z".to_string(), param)],
                ValueType::Any,
                None,
                false,
                type_params,
                None,
                None,
            )
        };
        let build_method = |which| match which {
            0 => method(
                10,
                JuliaType::Struct("Complex{T}".to_string()),
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
            ),
            1 => method(
                20,
                JuliaType::Struct("Complex{Float32}".to_string()),
                vec![],
            ),
            2 => method(
                30,
                JuliaType::Struct("Complex{Float64}".to_string()),
                vec![],
            ),
            _ => unreachable!("the permutation contains only method ids 0..=2"),
        };

        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut table = MethodTable::new("complex_dispatch_10775".to_string());
            for which in order {
                table.add_method(build_method(which));
            }

            for (actual, expected_index) in [
                ("Complex{Int64}", 10),
                ("Complex{Float32}", 20),
                ("Complex{Float64}", 30),
            ] {
                let selected = table
                    .dispatch(&[JuliaType::Struct(actual.to_string())])
                    .unwrap_or_else(|err| {
                        panic!("order {order:?}, actual {actual}: dispatch failed: {err}")
                    });
                assert_eq!(
                    selected.global_index, expected_index,
                    "order {order:?}, actual {actual} selected the wrong method"
                );
            }
        }
    }

    /// Issue #10959: constructor lowering dispatches inside the authoritative
    /// inner-constructor origin subset. The filtered clone must not inherit a
    /// cached method position from the full table, because positions change
    /// when outer methods are removed.
    #[test]
    fn dispatch_among_global_indices_filters_origin_and_clears_cache_10959() {
        let mut table = MethodTable::new("Window".to_string());
        table.add_method(first_arg_method(10, JuliaType::Any));
        table.add_method(first_arg_method(11, JuliaType::Int64));

        assert_eq!(
            table.dispatch(&[JuliaType::Float64]).unwrap().global_index,
            10
        );

        let allowed = HashSet::from([11]);
        assert!(table
            .dispatch_among_global_indices(&[JuliaType::Float64], &allowed)
            .is_err());
        assert_eq!(
            table
                .dispatch_among_global_indices(&[JuliaType::Int64], &allowed)
                .unwrap()
                .global_index,
            11
        );
    }

    #[test]
    fn dispatch_with_positional_type_args_prebinds_and_validates_bounds_10969() {
        let number_method = MethodSig::from_julia_projections(
            0,
            20,
            vec![("value".to_string(), JuliaType::Number)],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            None,
            None,
        );
        let typevar_method = MethodSig::from_julia_projections(
            1,
            21,
            vec![(
                "value".to_string(),
                JuliaType::TypeVar("S".to_string(), Some("Number".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "S".to_string(),
                "Number".to_string(),
            )],
            None,
            None,
        );
        let table = MethodTable::new("ExplicitOverload10969".to_string())
            .clone_with_methods_for_compile(vec![number_method, typevar_method]);

        let allowed = HashSet::from([20, 21]);
        assert_eq!(
            table
                .dispatch_among_global_indices_with_positional_type_args(
                    &[JuliaType::Int64],
                    &allowed,
                    &[JuliaType::Float64],
                )
                .ok()
                .map(|method| method.global_index),
            Some(20)
        );
        assert_eq!(
            table
                .dispatch_among_global_indices_with_positional_type_args(
                    &[JuliaType::Float64],
                    &allowed,
                    &[JuliaType::Float64],
                )
                .ok()
                .map(|method| method.global_index),
            Some(21)
        );
        assert!(table
            .dispatch_among_global_indices_with_positional_type_args(
                &[JuliaType::Int64],
                &allowed,
                &[JuliaType::String],
            )
            .is_err());
    }

    #[test]
    fn dispatch_with_positional_type_args_distinguishes_diagonal_call_10969() {
        let method = MethodSig::from_julia_projections(
            0,
            20,
            vec![
                ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ("y".to_string(), JuliaType::TypeVar("T".to_string(), None)),
            ],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            None,
            None,
        );
        let table = MethodTable::new("Diagonal10969".to_string())
            .clone_with_methods_for_compile(vec![method]);
        let allowed = HashSet::from([20]);
        let caller_t = JuliaType::Struct("T".to_string());
        let declared_t = JuliaType::TypeVar("T".to_string(), Some("Integer".to_string()));

        assert_eq!(
            table
                .dispatch_among_global_indices_with_positional_type_args(
                    &[declared_t.clone(), declared_t.clone()],
                    &allowed,
                    std::slice::from_ref(&caller_t),
                )
                .ok()
                .map(|method| method.global_index),
            Some(20)
        );
        assert!(table
            .dispatch_among_global_indices_with_positional_type_args(
                &[declared_t, JuliaType::Any],
                &allowed,
                &[caller_t],
            )
            .is_err());
    }

    #[test]
    fn explicit_parametric_inner_dedup_preserves_bound_overload_10993() {
        let method = |global_index, binder: &str, param| {
            MethodSig::from_julia_projections(
                0,
                global_index,
                vec![("value".to_string(), param)],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    binder.to_string(),
                    "Number".to_string(),
                )],
                None,
                None,
            )
        };
        let mut table = MethodTable::new("StaticExplicitOverload10993".to_string());

        table.add_inner_constructor_method(
            method(20, "T", JuliaType::Number),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_inner_constructor_method(
            method(
                21,
                "T",
                JuliaType::TypeVar("T".to_string(), Some("Number".to_string())),
            ),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![20, 21],
            "::Number and ::T where T<:Number are distinct explicit-inner methods"
        );

        table.add_inner_constructor_method(
            method(
                22,
                "S",
                JuliaType::TypeVar("S".to_string(), Some("Number".to_string())),
            ),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![20, 22],
            "alpha-equivalent binder redefinition remains last-definition-wins"
        );
    }

    /// An explicit-parametric inner has a distinct callable self from its bare
    /// outer collision. Replacing that inner must retain the outer (Issue
    /// #10959).
    #[test]
    fn explicit_inner_redefinition_replaces_inner_but_preserves_outer_10959() {
        let mut table = MethodTable::new("Window".to_string());
        table.add_method(first_arg_method(10, JuliaType::Int64));

        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_inner_constructor_method(
            first_arg_method(21, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );

        let indices: Vec<_> = table
            .methods
            .iter()
            .map(|method| method.global_index)
            .collect();
        assert_eq!(indices, vec![10, 21]);
        assert_eq!(table.constructor_self_family(10), None);
        assert_eq!(
            table.constructor_self_family(20),
            None,
            "replaced row's origin entry must not linger"
        );
        assert_eq!(
            table.constructor_self_family(21),
            Some(ConstructorSelfFamily::ExplicitParametricInner)
        );
    }

    /// Bare inner and ordinary constructors both dispatch on `Type{Foo}`.
    /// Either insertion direction must therefore replace the identical earlier
    /// row and clear stale origin metadata (Issue #11028).
    #[test]
    fn bare_inner_and_ordinary_constructor_share_last_definition_wins_11028() {
        let mut table = MethodTable::new("Window".to_string());
        table.add_method(first_arg_method(10, JuliaType::Int64));
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::BareInner,
        );

        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![20]
        );
        assert_eq!(table.constructor_self_family(10), None);
        assert_eq!(
            table.constructor_self_family(20),
            Some(ConstructorSelfFamily::BareInner)
        );

        table.add_method(first_arg_method(30, JuliaType::Int64));
        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![30]
        );
        assert_eq!(table.constructor_self_family(20), None);
    }

    /// Origin decisions must use the method table's family carrier even when
    /// the legacy MethodSig booleans are false (as they are for cached Base
    /// constructor rows). A later bare method has a distinct self and cannot
    /// replace an explicit-parametric inner row with the same value projection.
    #[test]
    fn explicit_constructor_family_prevents_bare_add_method_replacement_11019() {
        let mut table = MethodTable::new("Window".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_method(first_arg_method(10, JuliaType::Int64));

        let indices: Vec<_> = table
            .methods
            .iter()
            .map(|method| method.global_index)
            .collect();
        assert_eq!(indices, vec![20, 10]);
        assert_eq!(
            table.constructor_self_family(20),
            Some(ConstructorSelfFamily::ExplicitParametricInner)
        );
        assert_eq!(table.constructor_self_family(10), None);
    }

    /// Issue #10962, #10974: the constructor-self-family carrier must survive
    /// a real serialize -> deserialize round trip (the exact Base-cache
    /// boundary) and a filtered clone must drop origin metadata for rows it
    /// no longer carries — the negative counterpart proves a corrupted/erased
    /// marker is deterministically observable rather than silently
    /// tolerated.
    #[test]
    fn constructor_self_family_round_trips_and_filtered_clone_drops_stale_rows_10962() {
        let mut table = MethodTable::new("Ctor10962".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_inner_constructor_method(
            first_arg_method(21, JuliaType::String),
            ConstructorSelfFamily::BareInner,
        );

        let bytes = bincode::serialize(&table).expect("serialize table");
        let restored: MethodTable = bincode::deserialize(&bytes).expect("deserialize table");
        assert!(restored.is_explicit_parametric_inner_constructor(20));
        assert!(restored.is_inner_constructor(21));
        assert!(!restored.is_explicit_parametric_inner_constructor(21));

        let filtered = restored.clone_with_methods_for_compile(vec![restored.methods[0].clone()]);
        assert!(filtered.is_explicit_parametric_inner_constructor(20));
        assert_eq!(
            filtered.constructor_self_family(21),
            None,
            "a filtered-out method's origin metadata must not survive the filtered clone"
        );
    }

    /// Negative/mutation test (Issue #10962 DoD): flipping or erasing a
    /// cached row's constructor-self-family marker deterministically changes
    /// which method explicit `Foo{T}(...)` dispatch selects — proving the
    /// marker is load-bearing identity, not a heuristic that dispatch can
    /// silently fall back around when it is missing or wrong.
    #[test]
    fn mutated_constructor_self_family_marker_deterministically_changes_dispatch_10962() {
        let mut table = MethodTable::new("Ctor10962Mutation".to_string());
        table.add_inner_constructor_method(
            first_arg_method(30, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        // A distinct value signature (String, not Int64) so ordinary
        // `add_method` dedup treats this as a separate row rather than
        // replacing method 30.
        table.add_method(first_arg_method(31, JuliaType::String));

        assert_eq!(
            table
                .dispatch_among_explicit_parametric_inner_constructors(&[JuliaType::Int64])
                .expect("the registered explicit-parametric inner constructor must be selected")
                .global_index,
            30,
        );

        // Corrupt the cached row: erase the marker entirely (simulating a
        // schema/serialization defect that drops the entry).
        table.constructor_self_families.remove(&30);
        assert!(
            table
                .dispatch_among_explicit_parametric_inner_constructors(&[JuliaType::Int64])
                .is_err(),
            "erasing the origin marker must deterministically remove method 30 from \
             explicit-parametric inner-constructor dispatch, never silently fall back to \
             a `where`-parameter heuristic"
        );

        // Corrupt the cached row the other way: mislabel the ordinary method
        // as an explicit-parametric inner constructor.
        table
            .constructor_self_families
            .insert(31, ConstructorSelfFamily::ExplicitParametricInner);
        assert_eq!(
            table
                .dispatch_among_explicit_parametric_inner_constructors(&[JuliaType::String])
                .expect("method 31 now carries the marker")
                .global_index,
            31,
            "dispatch must follow the typed marker deterministically, including when it is wrong"
        );
    }

    #[test]
    fn constructor_self_family_round_trips_and_filtered_clone_drops_stale_rows_10974(
    ) -> Result<(), String> {
        let mut table = MethodTable::new("Ctor10974".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_inner_constructor_method(
            first_arg_method(21, JuliaType::String),
            ConstructorSelfFamily::BareInner,
        );

        let bytes = bincode::serialize(&table).map_err(|err| err.to_string())?;
        let restored: MethodTable = bincode::deserialize(&bytes).map_err(|err| err.to_string())?;
        assert!(restored.is_explicit_parametric_inner_constructor(20));
        assert!(restored.is_inner_constructor(21));

        let filtered = restored.clone_with_methods_for_compile(vec![restored.methods[0].clone()]);
        assert!(filtered.is_explicit_parametric_inner_constructor(20));
        assert_eq!(filtered.constructor_self_family(21), None);
        Ok(())
    }

    #[test]
    fn same_constructor_self_family_replacement_removes_old_origin_10974() {
        let mut table = MethodTable::new("Ctor10974".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_inner_constructor_method(
            first_arg_method(21, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![21]
        );
        assert_eq!(table.constructor_self_family(20), None);
        assert!(table.is_explicit_parametric_inner_constructor(21));
    }

    #[test]
    fn ordinary_insertion_preserves_constructor_self_family_row_10974() {
        let mut table = MethodTable::new("Ctor10974".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
        table.add_method(first_arg_method(10, JuliaType::Int64));

        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![20, 10]
        );
        assert_eq!(
            table.constructor_self_family(20),
            Some(ConstructorSelfFamily::ExplicitParametricInner)
        );
        assert_eq!(table.constructor_self_family(10), None);
    }

    #[test]
    fn different_constructor_self_families_coexist_for_identical_signatures_10974() {
        let mut table = MethodTable::new("Ctor10974".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::BareInner,
        );
        table.add_inner_constructor_method(
            first_arg_method(21, JuliaType::Int64),
            ConstructorSelfFamily::ExplicitParametricInner,
        );

        assert_eq!(
            table
                .methods
                .iter()
                .map(|method| method.global_index)
                .collect::<Vec<_>>(),
            vec![20, 21]
        );
        assert_eq!(
            table.constructor_self_family(20),
            Some(ConstructorSelfFamily::BareInner)
        );
        assert_eq!(
            table.constructor_self_family(21),
            Some(ConstructorSelfFamily::ExplicitParametricInner)
        );
    }

    #[test]
    fn clone_for_reprojection_preserves_constructor_self_families_10974() {
        let mut table = MethodTable::new("Ctor10974".to_string());
        table.add_inner_constructor_method(
            first_arg_method(20, JuliaType::Int64),
            ConstructorSelfFamily::BareInner,
        );
        table.add_inner_constructor_method(
            first_arg_method(21, JuliaType::String),
            ConstructorSelfFamily::ExplicitParametricInner,
        );

        let clone = table.clone_for_reprojection();
        assert_eq!(
            clone.constructor_self_family(20),
            Some(ConstructorSelfFamily::BareInner)
        );
        assert!(clone.is_explicit_parametric_inner_constructor(21));
    }

    #[test]
    fn synthetic_default_outer_marker_is_transient_transactional_and_filtered_11147() {
        let mut table = MethodTable::new("Ctor11147".to_string());
        table.add_synthetic_default_outer_method(
            first_arg_method(20, JuliaType::Int64),
            "ModuleA.Ctor11147",
        );
        table.add_method(first_arg_method(21, JuliaType::String));

        assert!(table.is_synthetic_default_outer_for_owner(20, "ModuleA.Ctor11147"));
        assert!(!table.is_synthetic_default_outer_for_owner(20, "ModuleB.Ctor11147"));
        assert!(!table.is_synthetic_default_outer_for_owner(21, "ModuleA.Ctor11147"));
        assert!(table.has_synthetic_default_constructor_declaration("ModuleA.Ctor11147"));
        assert!(
            !table.has_synthetic_default_constructor_declaration("ModuleB.Ctor11147"),
            "declaration provenance must not leak across same-short-name owners"
        );
        assert!(
            table
                .clone_for_reprojection()
                .is_synthetic_default_outer_for_owner(20, "ModuleA.Ctor11147"),
            "in-memory compiler reprojection must retain synthetic outer provenance"
        );
        assert!(
            table
                .clone_for_reprojection()
                .has_synthetic_default_constructor_declaration("ModuleA.Ctor11147"),
            "in-memory compiler reprojection must retain declaration provenance"
        );

        let filtered = table.clone_with_methods_for_compile(vec![table.methods[1].clone()]);
        assert!(
            !filtered.is_synthetic_default_outer_for_owner(20, "ModuleA.Ctor11147"),
            "a filtered-out method must not leave stale synthetic provenance"
        );
        assert!(
            filtered.has_synthetic_default_constructor_declaration("ModuleA.Ctor11147"),
            "filtering rows must retain owner-level declaration provenance"
        );

        let bytes = bincode::serialize(&table).expect("serialize transient marker table");
        let restored: MethodTable =
            bincode::deserialize(&bytes).expect("deserialize transient marker table");
        assert!(
            !restored.is_synthetic_default_outer_for_owner(20, "ModuleA.Ctor11147"),
            "the compile-session marker must not change the serialized MethodTable schema"
        );
        assert!(
            !restored.has_synthetic_default_constructor_declaration("ModuleA.Ctor11147"),
            "declaration provenance must also remain compile-session-only"
        );

        table.add_method(first_arg_method(22, JuliaType::Int64));
        assert!(!table.is_synthetic_default_outer_for_owner(20, "ModuleA.Ctor11147"));
        assert!(
            !table.is_synthetic_default_outer_for_owner(22, "ModuleA.Ctor11147"),
            "a later source-written ordinary method replaces, but never inherits, the marker"
        );
        assert!(
            table.has_synthetic_default_constructor_declaration("ModuleA.Ctor11147"),
            "ordinary replacement must retain declaration provenance"
        );

        table.add_synthetic_default_outer_method(
            first_arg_method(23, JuliaType::Float64),
            "ModuleA.Ctor11147",
        );
        table.add_inner_constructor_method(
            first_arg_method(24, JuliaType::Float64),
            ConstructorSelfFamily::BareInner,
        );
        assert!(
            !table.is_synthetic_default_outer_for_owner(23, "ModuleA.Ctor11147"),
            "an inner-constructor replacement must clear stale synthetic outer provenance"
        );
        assert!(
            table.has_synthetic_default_constructor_declaration("ModuleA.Ctor11147"),
            "inner replacement must retain declaration provenance"
        );

        table.add_synthetic_default_outer_method(
            first_arg_method(25, JuliaType::Bool),
            "ModuleA.Ctor11147",
        );
        table.add_synthetic_default_outer_method(
            first_arg_method(26, JuliaType::Bool),
            "ModuleA.Ctor11147",
        );
        assert!(!table.is_synthetic_default_outer_for_owner(25, "ModuleA.Ctor11147"));
        assert!(
            table.is_synthetic_default_outer_for_owner(26, "ModuleA.Ctor11147"),
            "same-signature synthetic regeneration must transfer provenance to the live row"
        );
    }

    /// Issue #9197 S5: the first-arg typemap buckets methods by their declared
    /// first parameter's nominal family (sealed primitive, struct family, or
    /// builtin abstract); `Any` / user-abstract / opaque first params fall to
    /// the always-scanned `wildcard` bucket.
    #[test]
    fn first_arg_index_buckets_by_nominal_family_issue_9197() {
        let methods = vec![
            first_arg_method(0, JuliaType::Int64),
            first_arg_method(1, JuliaType::Number),
            first_arg_method(2, JuliaType::Struct("Complex{Int64}".to_string())),
            first_arg_method(3, JuliaType::Float64),
            first_arg_method(4, JuliaType::Any),
            first_arg_method(5, JuliaType::Real),
            first_arg_method(6, JuliaType::AbstractFloat),
        ];
        let idx = FirstArgIndex::build(&methods);

        assert_eq!(idx.wildcard, vec![4], "`Any` first-param goes to wildcard");
        assert_eq!(idx.nominal.get("Int64"), Some(&vec![0]));
        assert_eq!(idx.nominal.get("Number"), Some(&vec![1]));
        assert_eq!(
            idx.nominal.get("Complex"),
            Some(&vec![2]),
            "a struct first-param is keyed by its bare family name"
        );
        assert_eq!(idx.nominal.get("Float64"), Some(&vec![3]));
        assert_eq!(idx.nominal.get("Real"), Some(&vec![5]));
        assert_eq!(idx.nominal.get("AbstractFloat"), Some(&vec![6]));
    }

    /// Issue #9197 S5: a sealed-primitive first argument gathers a complete,
    /// sound superset — its own bucket, every builtin abstract-supertype
    /// bucket, and `wildcard` — and nothing else (no struct / unrelated-abstract
    /// bucket a primitive can never be a subtype of). This is subtype-resolved
    /// indexing: an `Int64` call finds the method registered on its abstract
    /// supertype `Number`/`Real` while skipping `Complex` / `Float64` /
    /// `AbstractFloat` / `AbstractArray`.
    #[test]
    fn first_arg_gather_is_complete_and_sound_for_primitive_issue_9197() {
        let methods = vec![
            first_arg_method(0, JuliaType::Int64),
            first_arg_method(1, JuliaType::Number),
            first_arg_method(2, JuliaType::Struct("Complex{Int64}".to_string())),
            first_arg_method(3, JuliaType::Float64),
            first_arg_method(4, JuliaType::Any),
            first_arg_method(5, JuliaType::Real),
            first_arg_method(6, JuliaType::AbstractFloat),
            first_arg_method(7, JuliaType::AbstractArray),
        ];
        let idx = FirstArgIndex::build(&methods);

        // Int64 <: Signed <: Integer <: Real <: Number : gather Int64, Real,
        // Number buckets + wildcard; NOT Complex/Float64/AbstractFloat/Array.
        let int_arg = CoreType::from(&JuliaType::Int64);
        assert_eq!(
            idx.primitive_candidate_indices(&int_arg),
            Some(vec![0, 1, 4, 5]),
        );

        // Float64 <: AbstractFloat <: Real <: Number : gather Float64,
        // AbstractFloat, Real, Number buckets + wildcard; NOT Int64/Complex.
        let f64_arg = CoreType::from(&JuliaType::Float64);
        assert_eq!(
            idx.primitive_candidate_indices(&f64_arg),
            Some(vec![1, 3, 4, 5, 6]),
        );

        // A non-primitive (struct) first argument gets no gather — the caller
        // falls back to a full scan (struct-arg typemap deferred to S6/S7).
        let struct_arg = CoreType::from(&JuliaType::Struct("Complex{Int64}".to_string()));
        assert_eq!(idx.primitive_candidate_indices(&struct_arg), None);
    }

    /// Issue #9197 S5: end-to-end dispatch through the gather is unchanged — the
    /// supertype method is never dropped, and the more-specific concrete method
    /// still wins by specificity.
    #[test]
    fn first_arg_gather_preserves_specificity_ordering_issue_9197() {
        let mut table = MethodTable::new("f".to_string());
        table.add_method(first_arg_method(10, JuliaType::Real));
        table.add_method(first_arg_method(20, JuliaType::Int64));

        // Int64 gathers both the `Real` supertype method and the exact `Int64`
        // method; the scorer picks the more specific `Int64` (global 20).
        assert_eq!(
            table.dispatch(&[JuliaType::Int64]).unwrap().global_index,
            20,
        );
        // A Float64 argument matches only the `Real` family method — proving the
        // supertype bucket is reached (not dropped) by the gather.
        assert_eq!(
            table.dispatch(&[JuliaType::Float64]).unwrap().global_index,
            10,
        );
    }

    /// Issue #9197 S5 acceptance: the candidate-scan reduction. On a
    /// representative dispatch-heavy table (primitive same-type methods, numeric
    /// abstracts, several struct methods a primitive can never match, and one
    /// `Any` catch-all), an `Int64` dispatch scans only the gathered candidates
    /// instead of every method.
    #[test]
    fn first_arg_index_reduces_candidate_scan_count_issue_9197() {
        let mut table = MethodTable::new("plus".to_string());
        for m in [
            first_arg_method(0, JuliaType::Int64),
            first_arg_method(1, JuliaType::Float64),
            first_arg_method(2, JuliaType::Bool),
            first_arg_method(3, JuliaType::Number),
            first_arg_method(4, JuliaType::Real),
            first_arg_method(5, JuliaType::AbstractFloat),
            first_arg_method(6, JuliaType::Struct("Complex{Int64}".to_string())),
            first_arg_method(7, JuliaType::Struct("Complex{Float64}".to_string())),
            first_arg_method(8, JuliaType::Struct("Rational{Int64}".to_string())),
            first_arg_method(9, JuliaType::Struct("Quaternion{Float64}".to_string())),
            first_arg_method(10, JuliaType::AbstractArray),
            first_arg_method(11, JuliaType::Any),
        ] {
            table.add_method(m);
        }
        assert_eq!(table.methods.len(), 12);

        // Int64 gathers: Int64(0), Real(4), Number(3), Any/wildcard(11) = 4;
        // it skips the two Complex, the Rational, the Quaternion, Float64, Bool,
        // AbstractFloat, and AbstractArray methods — 8 of the 12 methods.
        reset_dispatch_candidate_scan_count();
        let _ = table.dispatch(&[JuliaType::Int64]);
        let scanned = dispatch_candidate_scan_count();
        assert_eq!(
            scanned, 4,
            "Int64 dispatch must scan only the gathered candidates, not all 12",
        );
        assert!(
            scanned < table.methods.len() as u64,
            "the first-arg typemap must reduce the candidate scan (12 -> {scanned})",
        );
    }

    /// Build a projection from explicit `(name, declared parent)` links — the
    /// test-side replacement for hand-built `struct_parents` maps now that
    /// lookups go through the shared `StructHierarchy` (Issue #6336).
    fn projection_from_parent_links(links: &[(&str, Option<&str>)]) -> MethodTableProjection {
        let mut hierarchy = StructHierarchy::new();
        for (name, parent) in links {
            hierarchy.insert(*name, parent.map(str::to_string), Vec::new());
        }
        MethodTableProjection {
            parentless_abstract_names: std::collections::HashSet::new(),
            has_parent_links: !links.is_empty(),
            struct_hierarchy: hierarchy,
        }
    }

    fn projection_from_parent_links_with_params(
        links: &[(&str, Option<&str>, &[&str])],
    ) -> MethodTableProjection {
        let mut hierarchy = StructHierarchy::new();
        for (name, parent, params) in links {
            hierarchy.insert(
                *name,
                parent.map(str::to_string),
                params.iter().map(|param| (*param).to_string()).collect(),
            );
        }
        MethodTableProjection {
            parentless_abstract_names: std::collections::HashSet::new(),
            has_parent_links: !links.is_empty(),
            struct_hierarchy: hierarchy,
        }
    }

    #[test]
    fn struct_parent_fallback_projects_abstract_matrix_params_issue_8350() {
        let projection = projection_from_parent_links_with_params(&[(
            "MatrixAsVectorIssue",
            Some("AbstractMatrix{T}"),
            &["T", "V"],
        )]);
        let arg = JuliaType::from_name_or_struct("MatrixAsVectorIssue{Float64, Vector{Float64}}");
        let matrix_param = JuliaType::from_name_or_struct("AbstractMatrix{<:Real}");
        let vector_param = JuliaType::from_name_or_struct("AbstractVector{<:Real}");
        let projected = projected_direct_parent_type(
            "MatrixAsVectorIssue{Float64, Vector{Float64}}",
            &projection,
        )
        .expect("parent should project");

        assert_eq!(
            projected,
            JuliaType::from_name_or_struct("AbstractMatrix{Float64}")
        );
        assert!(projected.is_subtype_of_parametric(&matrix_param, &[]));

        assert!(julia_type_matches_with_struct_parents(
            &matrix_param,
            &arg,
            &[],
            &projection
        ));
        assert!(!julia_type_matches_with_struct_parents(
            &vector_param,
            &arg,
            &[],
            &projection
        ));
    }

    /// Issue #7266: a built-in struct family (`Vector`/`Matrix`/`Array`/`Dict`/
    /// `Set`/ranges) is never in the user-declared hierarchy. It must walk its
    /// known BUILT-IN supertype
    /// chain (`Vector -> DenseArray -> AbstractArray -> Any`) and conclude it is
    /// NOT a subtype of an unrelated abstract scalar bound — while still
    /// satisfying its genuine array-family abstract supertypes. Issue #8439
    /// also requires genuinely unknown names to be rejected.
    #[test]
    fn builtin_struct_family_does_not_loose_match_scalar_abstract_issue_7266() {
        let projection = MethodTableProjection::default();

        // The core bug: Vector is NOT a subtype of any abstract SCALAR bound.
        assert!(!struct_is_subtype_of_abstract(
            "Vector",
            "Integer",
            &projection
        ));
        assert!(!struct_is_subtype_of_abstract(
            "Vector",
            "Real",
            &projection
        ));
        assert!(!struct_is_subtype_of_abstract(
            "Vector",
            "Number",
            &projection
        ));
        assert!(!struct_is_subtype_of_abstract(
            "Matrix",
            "Integer",
            &projection
        ));
        assert!(!struct_is_subtype_of_abstract(
            "Array",
            "Integer",
            &projection
        ));
        assert!(!struct_is_subtype_of_abstract("Dict", "Real", &projection));
        assert!(!struct_is_subtype_of_abstract("Set", "Number", &projection));
        assert!(!struct_is_subtype_of_abstract(
            "UnitRange",
            "Integer",
            &projection
        ));

        // ...but it IS a subtype of its genuine array-family abstract supertypes.
        assert!(struct_is_subtype_of_abstract(
            "Vector",
            "AbstractArray",
            &projection
        ));
        assert!(struct_is_subtype_of_abstract(
            "Vector",
            "DenseArray",
            &projection
        ));
        assert!(struct_is_subtype_of_abstract(
            "Matrix",
            "AbstractArray",
            &projection
        ));
        assert!(struct_is_subtype_of_abstract(
            "Dict",
            "AbstractDict",
            &projection
        ));

        // Issue #8439: an unknown name must not be accepted as a subtype of an
        // arbitrary abstract bound.
        assert!(!struct_is_subtype_of_abstract(
            "TotallyUnknownUserType",
            "Real",
            &projection
        ));
    }

    /// Issue #8149: the binary `==`/`!=` array-routing decision uses the same
    /// strict registered-subtype semantics as `struct_is_subtype_of_abstract`.
    /// It must (a) resolve a genuinely registered `<: AbstractArray` struct through
    /// the declared + built-in chain, and (b) return `false` for any unknown or
    /// unrelated struct so an unrelated `native-array == struct` pair on the
    /// global `==` path is never mis-routed.
    #[test]
    fn strict_abstractarray_subtype_predicate_no_conservative_accept_issue_8149() {
        // User structs registered as AbstractArray subtypes (multi-level through
        // the built-in `AbstractVector/AbstractMatrix -> AbstractArray` chain, and
        // a direct `<: AbstractArray`).
        let projection = projection_from_parent_links(&[
            ("MyVec", Some("AbstractVector")),
            ("MyMat", Some("AbstractMatrix")),
            ("MyArr", Some("AbstractArray")),
            // A non-array user struct under a parentless user abstract.
            ("MyNum", Some("MyReal")),
            ("MyReal", None),
        ]);
        assert!(struct_is_registered_subtype_of_abstract(
            "MyVec",
            "AbstractArray",
            &projection
        ));
        assert!(struct_is_registered_subtype_of_abstract(
            "MyMat",
            "AbstractArray",
            &projection
        ));
        assert!(struct_is_registered_subtype_of_abstract(
            "MyArr",
            "AbstractArray",
            &projection
        ));
        // Parametric names are reduced to their family before the walk.
        assert!(struct_is_registered_subtype_of_abstract(
            "MyVec{Float64}",
            "AbstractArray",
            &projection
        ));

        // A registered non-array struct is NOT an AbstractArray subtype.
        assert!(!struct_is_registered_subtype_of_abstract(
            "MyNum",
            "AbstractArray",
            &projection
        ));

        // Issue #8439: the regular dispatch helper now shares the strict
        // no-unknown-accept behavior, so both paths reject unrelated unknowns.
        assert!(!struct_is_subtype_of_abstract(
            "TotallyUnknownStruct",
            "AbstractArray",
            &projection
        ));
        assert!(!struct_is_registered_subtype_of_abstract(
            "TotallyUnknownStruct",
            "AbstractArray",
            &projection
        ));

        // Built-in array families still resolve through their known supertype
        // chain (`Vector -> DenseArray -> AbstractArray`).
        assert!(struct_is_registered_subtype_of_abstract(
            "Vector",
            "AbstractArray",
            &MethodTableProjection::default()
        ));
        // ...and a built-in array family is NOT an unrelated scalar abstract.
        assert!(!struct_is_registered_subtype_of_abstract(
            "Vector",
            "Integer",
            &MethodTableProjection::default()
        ));
    }

    /// Issue #6336/#6495: the `MethodSig` wire format carries only the
    /// canonical `core_signature` (+ display `param_names`); deserialization
    /// must preserve that canonical identity without rebuilding stored
    /// `JuliaType` projections.
    #[test]
    fn method_sig_serde_preserves_canonical_signature_issue_6336() {
        let shapes: Vec<(
            Vec<(String, JuliaType)>,
            Vec<TypeParam>,
            Option<usize>,
            Option<usize>,
        )> = vec![
            // f(x::T, y::Vector{T}) where T<:Number
            (
                vec![
                    ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                    (
                        "y".to_string(),
                        JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                ],
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Number".to_string(),
                )],
                None,
                None,
            ),
            // f(t::Tuple{Int64, Vararg{Int64}}) — tuple-vararg pattern element
            (
                vec![(
                    "t".to_string(),
                    JuliaType::TupleOf(vec![
                        JuliaType::Int64,
                        JuliaType::Struct("Vararg{Int64}".to_string()),
                    ]),
                )],
                vec![],
                None,
                None,
            ),
            // f(a, args::Int64...) with fixed count (Vararg{T, N} style)
            (
                vec![
                    ("a".to_string(), JuliaType::Any),
                    ("args".to_string(), JuliaType::Int64),
                ],
                vec![],
                Some(1),
                Some(2),
            ),
            // f(v::Vector{Vector{Int64}}, w::AbstractVector{Vector{Int64}})
            (
                vec![
                    (
                        "v".to_string(),
                        JuliaType::VectorOf(Box::new(JuliaType::VectorOf(Box::new(
                            JuliaType::Int64,
                        )))),
                    ),
                    (
                        "w".to_string(),
                        JuliaType::Struct("AbstractVector{Vector{Int64}}".to_string()),
                    ),
                ],
                vec![],
                None,
                None,
            ),
            // f(::Type{T}, ::T) where T<:Integer — type/value diagonal
            (
                vec![
                    (
                        "ty".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ],
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Integer".to_string(),
                )],
                None,
                None,
            ),
            // f(v::AbstractVector{<:Integer}, p::Pair{Int64, String})
            (
                vec![
                    (
                        "v".to_string(),
                        JuliaType::Struct("AbstractVector{<:Integer}".to_string()),
                    ),
                    (
                        "p".to_string(),
                        JuliaType::Struct("Pair{Int64, String}".to_string()),
                    ),
                ],
                vec![],
                None,
                None,
            ),
        ];

        for (params, type_params, vararg_param_index, vararg_fixed_count) in shapes {
            let param_names = params
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            let sig = MethodSig::from_julia_projections(
                0,
                7,
                params,
                ValueType::Any,
                None,
                false,
                type_params,
                vararg_param_index,
                vararg_fixed_count,
            );
            let projected_params = sig.projected_param_julia_types();
            let projected_type_params = sig.projected_type_params();

            let bytes = bincode::serialize(&sig).expect("serialize MethodSig");
            let restored: MethodSig = bincode::deserialize(&bytes).expect("deserialize MethodSig");

            assert_eq!(restored.param_names, param_names, "display parameter names");
            assert_eq!(restored.projected_param_julia_types(), projected_params);
            assert_eq!(restored.projected_type_params(), projected_type_params);
            assert_eq!(restored.core_signature, sig.core_signature);
            assert_eq!(restored.global_index, 7);
            assert_eq!(restored.vararg_param_index, vararg_param_index);
            assert_eq!(restored.vararg_fixed_count, vararg_fixed_count);
        }
    }

    #[test]
    fn method_sig_cache_roundtrip_preserves_bound_and_free_typevar_identity_issue_10460() {
        let sig = MethodSig::from_julia_projections(
            0,
            7,
            vec![(
                "pair".to_string(),
                JuliaType::TupleOf(vec![
                    JuliaType::TypeVar("T".to_string(), None),
                    JuliaType::RuntimeTypeVar {
                        id: 104_600,
                        name: "T".to_string(),
                        lower_bound: Box::new(JuliaType::Bottom),
                        upper_bound: Box::new(JuliaType::TypeVar("T".to_string(), None)),
                    },
                ]),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Real".to_string(),
            )],
            None,
            None,
        );

        let bytes = bincode::serialize(&sig).expect("serialize MethodSig cache carrier");
        let restored: MethodSig =
            bincode::deserialize(&bytes).expect("deserialize MethodSig cache carrier");
        assert_eq!(restored.core_signature, sig.core_signature);
        assert!(matches!(
            restored.projected_param_julia_types().as_slice(),
            [JuliaType::TupleOf(pair)]
                if matches!(pair.as_slice(), [JuliaType::TypeVar(..), JuliaType::RuntimeTypeVar { id: 104_600, .. }])
        ));

        let CoreType::UnionAll { var, body } = restored.core_signature else {
            panic!("restored signature must retain its UnionAll binder");
        };
        let CoreType::Tuple(elements) = body.as_ref() else {
            panic!("restored signature must retain its argument tuple");
        };
        let [CoreType::Tuple(pair)] = elements.as_slice() else {
            panic!("restored signature must retain its tuple parameter");
        };
        let [CoreType::TypeVar(bound_reference), CoreType::TypeVar(free)] = pair.as_slice() else {
            panic!("restored signature must retain both TypeVar references");
        };
        assert!(matches!(var.typevar_id(), CoreTypeVarId::Scoped(_)));
        assert_eq!(bound_reference.typevar_id(), var.typevar_id());
        assert_eq!(free.typevar_id(), CoreTypeVarId::Rigid(104_600));
        assert_eq!(
            free.upper_bound.as_deref(),
            Some(&CoreType::TypeVar(bound_reference.clone()))
        );

        let rigid_len = CoreType::TypeVar(CoreTypeVar::unscoped("N").with_rigid_identity(104_601));
        let projected_vararg = project_method_core_type(&CoreType::VarargLen {
            element: Box::new(CoreType::Primitive(
                subset_julia_vm_types::inference_core::CorePrimitive::Int64,
            )),
            len: Box::new(rigid_len),
        });
        assert!(matches!(
            projected_vararg,
            JuliaType::RuntimeParametric { base, params }
                if base == "Vararg"
                    && matches!(params.as_slice(), [JuliaType::Int64, JuliaType::RuntimeTypeVar { id: 104_601, .. }])
        ));

        let rigid_nested =
            CoreType::TypeVar(CoreTypeVar::unscoped("Nested").with_rigid_identity(104_602));
        assert_eq!(
            project_method_core_type(&CoreType::NamedTuple(vec![(
                "field".to_string(),
                rigid_nested.clone(),
            )])),
            JuliaType::Any,
            "an unrepresentable rigid identity must widen, not become a same-name source TypeVar"
        );
        assert_eq!(
            project_method_core_type(&CoreType::AbstractUser {
                name: "IdentityParent10460".to_string(),
                parent: Some(Box::new(rigid_nested.clone())),
            }),
            JuliaType::Any
        );
        assert_eq!(
            project_method_core_type(&CoreType::UnionAll {
                var:
                    CoreTypeVar::with_bounds("Source", None, Some(Box::new(rigid_nested.clone())),)
                        .with_scope_id(104_603),
                body: Box::new(CoreType::Any),
            }),
            JuliaType::Any,
            "a source UnionAll whose bound contains a rigid identity must fail closed"
        );
        assert_eq!(
            project_method_core_type(&CoreType::TypeVar(CoreTypeVar::with_bounds(
                "Source",
                None,
                Some(Box::new(rigid_nested)),
            ))),
            JuliaType::Any
        );

        let bounded_vector = MethodSig::from_julia_projections(
            0,
            8,
            vec![(
                "value".to_string(),
                JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            None,
            None,
        );
        let params = bounded_vector
            .expanded_core_param_types_for_arity(1)
            .expect("fixed-arity production signature");
        let args = vec![
            subset_julia_vm_types::inference_core::dispatch_resolver::dispatch_core_type_from_julia(
                &JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            ),
        ];
        assert!(
            dispatch_resolver::core_match::core_signature_match_with_bindings_with_hierarchy(
                &params,
                &args,
                &bounded_vector.core_signature_type_vars(),
                &StructHierarchy::new(),
            )
            .is_some(),
            "scoped production where-binders must remain applicable"
        );

        let dependent_bound = MethodSig::from_julia_projections(
            0,
            9,
            vec![(
                "value".to_string(),
                JuliaType::TypeVar("T".to_string(), Some("Vector{S}".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![
                TypeParam::with_upper_bound("S".to_string(), "Number".to_string()),
                TypeParam::with_upper_bound("T".to_string(), "Vector{S}".to_string()),
            ],
            None,
            None,
        );
        let params = dependent_bound
            .expanded_core_param_types_for_arity(1)
            .expect("fixed-arity dependent production signature");
        assert!(
            dispatch_resolver::core_match::core_signature_match_with_bindings_with_hierarchy(
                &params,
                &args,
                &dependent_bound.core_signature_type_vars(),
                &StructHierarchy::new(),
            )
            .is_some(),
            "dependent scoped where-binders must remain applicable"
        );

        let mut table = MethodTable::new("dependent_scoped_10460".to_string());
        table.add_method(MethodSig::from_julia_projections(
            0,
            10,
            vec![("value".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            None,
            None,
        ));
        table.add_method(dependent_bound);
        assert_eq!(
            table
                .dispatch(&[JuliaType::VectorOf(Box::new(JuliaType::Int64))])
                .expect("dependent bounded method must dispatch")
                .global_index,
            9,
            "dependent bounded method must outrank Any"
        );
    }

    /// Issue #6495: arity expansion reads only the canonical
    /// `core_signature`; the JuliaType view is reconstructed from the same
    /// expanded core row.
    #[test]
    fn expanded_core_param_types_match_canonical_inverse_issue_6495() {
        let shapes: Vec<(
            Vec<(String, JuliaType)>,
            Vec<TypeParam>,
            Option<usize>,
            Option<usize>,
        )> = vec![
            // f(x::Int64, y::Vector{T}) where T
            (
                vec![
                    ("x".to_string(), JuliaType::Int64),
                    (
                        "y".to_string(),
                        JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                ],
                vec![TypeParam::new("T".to_string())],
                None,
                None,
            ),
            // f(a::String, args::Int64...) — unbounded trailing varargs
            (
                vec![
                    ("a".to_string(), JuliaType::String),
                    ("args".to_string(), JuliaType::Int64),
                ],
                vec![],
                Some(1),
                None,
            ),
            // f(a, args::T...) where T<:Real with fixed count (Vararg{T, 2})
            (
                vec![
                    ("a".to_string(), JuliaType::Any),
                    (
                        "args".to_string(),
                        JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
                    ),
                ],
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                Some(1),
                Some(2),
            ),
            // zero-prefix pure varargs: f(args...)
            (
                vec![("args".to_string(), JuliaType::Any)],
                vec![],
                Some(0),
                None,
            ),
        ];

        for (params, type_params, vararg_param_index, vararg_fixed_count) in shapes {
            let sig = MethodSig::for_tests(
                0,
                7,
                params.clone(),
                ValueType::Any,
                None,
                false,
                type_params,
                CoreType::Bottom,
                vararg_param_index,
                vararg_fixed_count,
            );
            assert!(sig.structured_arg_core_types().is_some());
            for arity in 0..=params.len() + 2 {
                let expanded_core = sig.expanded_core_param_types_for_arity(arity);
                let expanded_projected = sig.expanded_projected_param_julia_types_for_arity(arity);
                let core_as_julia = expanded_core.as_ref().map(|cores| {
                    cores
                        .iter()
                        .map(subset_julia_vm_types::inference_core::core_type_to_julia_type)
                        .collect::<Vec<_>>()
                });
                assert_eq!(
                    expanded_projected, core_as_julia,
                    "arity {arity} expansion must derive from the canonical core row"
                );
            }
        }
    }

    /// Issue #6495: the `dispatch_inner` tie-breaker inputs read the canonical
    /// `core_signature` projection. A test-only `Bottom` placeholder takes the
    /// conservative defaults.
    #[test]
    fn tiebreaker_inputs_read_canonical_signature_issue_6495() {
        let projection = projection_from_parent_links(&[
            ("Circle", Some("Shape")),
            ("Square", Some("NotShape")),
            ("NotShape", None),
        ]);

        // f(x, s::Shape, args::Any...) where T — the vararg `Any` slot must
        // NOT count toward the fixed-prefix Any count.
        let params = vec![
            ("x".to_string(), JuliaType::Any),
            (
                "s".to_string(),
                JuliaType::AbstractUser("Shape".to_string(), Some("Any".to_string())),
            ),
            ("args".to_string(), JuliaType::Any),
        ];
        let bottom = MethodSig::bottom_for_tests(
            0,
            7,
            params.clone(),
            ValueType::Any,
            None,
            false,
            Some(2),
            None,
        );
        let circle_args = vec![
            JuliaType::Any,
            JuliaType::Struct("Circle".to_string()),
            JuliaType::Int64,
        ];
        let square_args = vec![
            JuliaType::Any,
            JuliaType::Struct("Square".to_string()),
            JuliaType::Int64,
        ];

        assert_eq!(bottom.structured_arg_core_types(), None);
        assert_eq!(any_param_count_fixed_prefix(&bottom), 0);
        assert_eq!(where_param_count(&bottom), 0);
        assert!(ancestry_filter_passes(&bottom, &circle_args, &projection));
        assert!(ancestry_filter_passes(&bottom, &square_args, &projection));

        let sig = MethodSig::for_tests(
            0,
            7,
            params,
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            Some(2),
            None,
        );
        assert!(sig.structured_arg_core_types().is_some());
        assert_eq!(sig.core_signature_type_var_count(), 1);
        assert_eq!(any_param_count_fixed_prefix(&sig), 1);
        assert_eq!(where_param_count(&sig), 1);
        assert!(ancestry_filter_passes(&sig, &circle_args, &projection));
        assert!(!ancestry_filter_passes(&sig, &square_args, &projection));
    }

    /// Issue #6495: the strictly-more-specific tie-breaker borrows the
    /// canonical `core_signature` projection; a test-only `Bottom` placeholder
    /// is conservatively "not strictly more specific".
    #[test]
    fn strictly_more_specific_reads_canonical_signature_issue_6495() {
        let make = |ty: JuliaType| {
            MethodSig::for_tests(
                0,
                7,
                vec![("x".to_string(), ty)],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let bottom_narrow = MethodSig::bottom_for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            None,
            None,
        );
        let bottom_wide = MethodSig::bottom_for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Real)],
            ValueType::Any,
            None,
            false,
            None,
            None,
        );
        assert!(!method_params_strictly_more_specific(
            &bottom_narrow,
            &bottom_wide
        ));
        assert!(!method_params_strictly_more_specific(
            &bottom_wide,
            &bottom_narrow
        ));

        let narrow = make(JuliaType::Int64);
        let wide = make(JuliaType::Real);
        assert!(method_params_strictly_more_specific(&narrow, &wide));
        assert!(!method_params_strictly_more_specific(&wide, &narrow));
        assert!(!method_params_strictly_more_specific(&narrow, &narrow));
    }

    /// Issue #6495: projection accessors reconstruct from canonical
    /// `core_signature`; a test-only `Bottom` placeholder reports conservative
    /// defaults.
    #[test]
    fn param_projection_accessors_read_canonical_signature_issue_6495() {
        let make_params = |tys: Vec<JuliaType>| {
            tys.into_iter()
                .enumerate()
                .map(|(i, ty)| (format!("x{i}"), ty))
                .collect::<Vec<_>>()
        };
        let shapes = vec![
            vec![JuliaType::Int64, JuliaType::Any],
            vec![JuliaType::Any, JuliaType::Any],
            vec![
                JuliaType::Struct("Complex{Float64}".to_string()),
                JuliaType::Real,
            ],
            vec![JuliaType::VectorOf(Box::new(JuliaType::Int64))],
        ];
        for tys in shapes {
            let params = make_params(tys.clone());
            let bottom = MethodSig::bottom_for_tests(
                0,
                7,
                params.clone(),
                ValueType::Any,
                None,
                false,
                None,
                None,
            );
            assert!(bottom.structured_arg_core_types().is_none());
            for i in 0..bottom.param_count() {
                assert_eq!(bottom.param_specificity(i), 0);
                assert_eq!(
                    bottom.projected_param_julia_type(i).as_ref(),
                    &JuliaType::Any
                );
            }
            assert!(!bottom.all_params_specificity_zero());

            let sig = MethodSig::for_tests(
                0,
                7,
                params,
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            );
            assert!(sig.structured_arg_core_types().is_some());
            for (i, ty) in tys.iter().enumerate() {
                let core = CoreType::from(ty);
                let expected =
                    subset_julia_vm_types::inference_core::core_type_to_julia_type(&core);
                assert_eq!(sig.param_specificity(i), core.specificity());
                assert_eq!(sig.projected_param_julia_type(i).as_ref(), &expected);
            }
            assert_eq!(
                sig.all_params_specificity_zero(),
                tys.iter()
                    .map(CoreType::from)
                    .all(|core| core.specificity() == 0)
            );
        }
    }

    /// Issue #8549: a zero-parameter `Bottom` placeholder must be classified
    /// as structurally unavailable. The historical length-equality gate
    /// (`arg_core_types().len() == param_count()`) passed vacuously for zero
    /// parameters (`[].len() == 0`), so a zero-param placeholder leaked
    /// through as an "available" empty signature row.
    #[test]
    fn zero_param_bottom_placeholder_is_unavailable_issue_8549() {
        let bottom =
            MethodSig::bottom_for_tests(0, 7, vec![], ValueType::Any, None, false, None, None);
        assert_eq!(bottom.param_count(), 0);
        assert_eq!(
            bottom.signature_defect(),
            Some(SignatureDefect::BottomPlaceholder)
        );
        assert!(
            bottom.structured_arg_core_types().is_none(),
            "a Bottom placeholder has no structured signature, even at arity 0"
        );
        assert!(
            bottom.expanded_core_param_types_for_arity(0).is_none(),
            "a Bottom placeholder must not expand to an empty parameter row"
        );
    }

    /// Issue #8549 (fallback reason 1): the test-only `Bottom` placeholder is
    /// the canonical "structurally unavailable" signature; every conservative
    /// dispatch fallback keys off this classification.
    #[test]
    fn signature_defect_classifies_bottom_placeholder_issue_8549() {
        let bottom = MethodSig::bottom_for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            None,
            None,
        );
        assert_eq!(
            bottom.signature_defect(),
            Some(SignatureDefect::BottomPlaceholder)
        );
        assert!(bottom.structured_arg_core_types().is_none());
    }

    /// Issue #8549 (fallback reason 2): a canonical tuple whose arity
    /// disagrees with the declared parameter-name row is a skewed projection
    /// and must be reported as such (not silently treated as available or as
    /// a placeholder).
    #[test]
    fn signature_defect_classifies_arg_count_skew_issue_8549() {
        // `for_tests` keeps a non-Bottom explicit signature verbatim: a
        // two-element canonical tuple against a one-name parameter row.
        let skewed = MethodSig::for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Tuple(vec![
                CoreType::from(&JuliaType::Int64),
                CoreType::from(&JuliaType::Int64),
            ]),
            None,
            None,
        );
        assert_eq!(
            skewed.signature_defect(),
            Some(SignatureDefect::ArgCountSkew {
                projected: 2,
                declared: 1,
            })
        );
        assert!(skewed.structured_arg_core_types().is_none());
        assert!(skewed.expanded_core_param_types_for_arity(1).is_none());
        assert!(skewed.expanded_core_param_types_for_arity(2).is_none());
    }

    /// Issue #8549 (fallback reason 3): stripping the `where` `UnionAll`
    /// wrappers must reach a `Tuple`; any other body shape is non-canonical.
    #[test]
    fn signature_defect_classifies_non_tuple_body_issue_8549() {
        let non_tuple = MethodSig::for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Any,
            None,
            None,
        );
        assert_eq!(
            non_tuple.signature_defect(),
            Some(SignatureDefect::NonTupleBody)
        );
        assert!(non_tuple.structured_arg_core_types().is_none());
    }

    /// Issue #8549: the single production constructor derives the canonical
    /// signature structurally — no shape may produce a defect.
    #[test]
    fn production_constructor_signatures_are_total_issue_8549() {
        let shapes: Vec<(
            Vec<(String, JuliaType)>,
            Vec<TypeParam>,
            Option<usize>,
            Option<usize>,
        )> = vec![
            // f()
            (vec![], vec![], None, None),
            // f(x::Int64)
            (
                vec![("x".to_string(), JuliaType::Int64)],
                vec![],
                None,
                None,
            ),
            // f(x::T) where {T<:Real}
            (
                vec![(
                    "x".to_string(),
                    JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
                )],
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                None,
                None,
            ),
            // f(a::String, args::Int64...)
            (
                vec![
                    ("a".to_string(), JuliaType::String),
                    ("args".to_string(), JuliaType::Int64),
                ],
                vec![],
                Some(1),
                None,
            ),
        ];
        for (params, type_params, vararg_param_index, vararg_fixed_count) in shapes {
            let sig = MethodSig::from_julia_projections(
                0,
                7,
                params.clone(),
                ValueType::Any,
                None,
                false,
                type_params,
                vararg_param_index,
                vararg_fixed_count,
            );
            assert_eq!(
                sig.signature_defect(),
                None,
                "production signature must be total for params {params:?}"
            );
            assert!(sig.structured_arg_core_types().is_some());
        }
    }

    /// Issue #8549: deserialization is a production construction path, so a
    /// wire payload carrying a defective canonical signature (test-only
    /// `Bottom` placeholder or a skewed projection) must be rejected loudly
    /// instead of silently degrading dispatch; canonical payloads still
    /// round-trip.
    #[test]
    fn deserialize_rejects_defective_wire_signatures_issue_8549() {
        let bottom = MethodSig::bottom_for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            None,
            None,
        );
        let bytes = bincode::serialize(&bottom).expect("serialize placeholder");
        let err = bincode::deserialize::<MethodSig>(&bytes)
            .expect_err("Bottom placeholder wire payload must be rejected");
        assert!(
            err.to_string().contains("Issue #8549"),
            "rejection must name the totality contract: {err}"
        );

        let skewed = MethodSig::for_tests(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Tuple(vec![
                CoreType::from(&JuliaType::Int64),
                CoreType::from(&JuliaType::Int64),
            ]),
            None,
            None,
        );
        let bytes = bincode::serialize(&skewed).expect("serialize skewed sig");
        bincode::deserialize::<MethodSig>(&bytes)
            .expect_err("skewed wire payload must be rejected");

        let canonical = MethodSig::from_julia_projections(
            0,
            7,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::Any,
            None,
            false,
            vec![],
            None,
            None,
        );
        let bytes = bincode::serialize(&canonical).expect("serialize canonical sig");
        let restored: MethodSig =
            bincode::deserialize(&bytes).expect("canonical payload round-trips");
        assert_eq!(restored.core_signature(), canonical.core_signature());
        assert_eq!(restored.signature_defect(), None);
    }

    /// Issue #6495: the exact-signature tie-breaker compares the canonical
    /// fixed-arity core row; a test-only `Bottom` placeholder is never exact.
    #[test]
    fn exact_signature_match_reads_canonical_signature_issue_6495() {
        let make_params = |tys: Vec<JuliaType>| {
            tys.into_iter()
                .enumerate()
                .map(|(i, ty)| (format!("x{i}"), ty))
                .collect::<Vec<_>>()
        };
        let arg_sets: Vec<Vec<JuliaType>> = vec![
            vec![JuliaType::Int64, JuliaType::Real],
            vec![JuliaType::Int64, JuliaType::Int64],
            vec![JuliaType::Int64],
            vec![JuliaType::VectorOf(Box::new(JuliaType::Int64))],
        ];
        let shapes: Vec<(Vec<JuliaType>, Option<usize>)> = vec![
            (vec![JuliaType::Int64, JuliaType::Real], None),
            (vec![JuliaType::Int64, JuliaType::Int64], None),
            (vec![JuliaType::VectorOf(Box::new(JuliaType::Int64))], None),
            (vec![JuliaType::Int64, JuliaType::Any], Some(1)),
        ];
        for (tys, vararg) in shapes {
            let params = make_params(tys);
            let bottom = MethodSig::bottom_for_tests(
                0,
                7,
                params.clone(),
                ValueType::Any,
                None,
                false,
                vararg,
                None,
            );
            assert!(bottom.structured_arg_core_types().is_none());
            let sig = MethodSig::for_tests(
                0,
                7,
                params,
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                vararg,
                None,
            );
            assert!(sig.structured_arg_core_types().is_some());
            for args in &arg_sets {
                let arg_cores: Vec<CoreType> = args.iter().map(CoreType::from).collect();
                assert!(!exact_signature_match(&bottom, &arg_cores));
                let expected = sig.vararg_param_index.is_none()
                    && sig.arg_core_types().len() == arg_cores.len()
                    && sig
                        .arg_core_types()
                        .iter()
                        .zip(&arg_cores)
                        .all(|(param, arg)| param == arg);
                assert_eq!(
                    exact_signature_match(&sig, &arg_cores),
                    expected,
                    "exact-match diverges for {:?} vs {args:?}",
                    sig.projected_param_julia_types(),
                );
            }
        }
    }

    /// Issue #6495: the vararg-aware `param_matches_at_call_position` maps call
    /// positions over the canonical core row.
    #[test]
    fn param_matches_at_call_position_reads_canonical_signature_issue_6495() {
        fn is_abstract_array_family_for_test(core: &CoreType) -> bool {
            matches!(
                core,
                CoreType::Abstract(
                    CoreAbstract::AbstractArray
                        | CoreAbstract::AbstractVector
                        | CoreAbstract::AbstractMatrix
                        | CoreAbstract::DenseArray
                )
            ) || matches!(
                core.to_julia_name()
                    .split('{')
                    .next()
                    .and_then(|name| name.rsplit('.').next()),
                Some(
                    "AbstractArray"
                        | "AbstractVector"
                        | "AbstractMatrix"
                        | "DenseArray"
                        | "DenseVector"
                        | "DenseMatrix"
                )
            )
        }

        let params = vec![
            ("x".to_string(), JuliaType::Int64),
            ("arrays".to_string(), JuliaType::AbstractArray),
        ];
        let bottom = MethodSig::bottom_for_tests(
            0,
            7,
            params.clone(),
            ValueType::Any,
            None,
            false,
            Some(1),
            None,
        );
        let expected = [false, true, true, true];

        assert!(bottom.structured_arg_core_types().is_none());
        for position in 0..expected.len() {
            assert!(
                !bottom.param_matches_at_call_position(position, is_abstract_array_family_for_test),
                "position {position} must be false on a Bottom placeholder"
            );
        }

        let sig = MethodSig::for_tests(
            0,
            7,
            params,
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            Some(1),
            None,
        );
        assert!(sig.structured_arg_core_types().is_some());
        for (position, want) in expected.iter().enumerate() {
            let got =
                sig.param_matches_at_call_position(position, is_abstract_array_family_for_test);
            assert_eq!(got, *want, "position {position} diverges");
        }

        let fixed_bottom = MethodSig::bottom_for_tests(
            0,
            8,
            vec![(
                "a".to_string(),
                JuliaType::Struct("AbstractVector{Int64}".to_string()),
            )],
            ValueType::Any,
            None,
            false,
            None,
            None,
        );
        assert!(!fixed_bottom.param_matches_at_call_position(0, is_abstract_array_family_for_test));
        let fixed = MethodSig::for_tests(
            0,
            8,
            vec![(
                "a".to_string(),
                JuliaType::Struct("AbstractVector{Int64}".to_string()),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        );
        assert!(fixed.param_matches_at_call_position(0, is_abstract_array_family_for_test));
        assert!(!fixed.param_matches_at_call_position(1, is_abstract_array_family_for_test));
    }

    /// Issue #6495: `has_where_params` reads the `UnionAll` wrappers in the
    /// canonical signature.
    #[test]
    fn has_where_params_reads_canonical_signature_issue_6495() {
        for type_params in [vec![], vec![TypeParam::new("T".to_string())]] {
            let expected = !type_params.is_empty();
            let bottom = MethodSig::bottom_for_tests(
                0,
                7,
                vec![("x".to_string(), JuliaType::TypeVar("T".to_string(), None))],
                ValueType::Any,
                None,
                false,
                None,
                None,
            );
            assert!(bottom.structured_arg_core_types().is_none());
            assert!(!bottom.has_where_params());

            let sig = MethodSig::for_tests(
                0,
                7,
                vec![("x".to_string(), JuliaType::TypeVar("T".to_string(), None))],
                ValueType::Any,
                None,
                false,
                type_params,
                CoreType::Bottom,
                None,
                None,
            );
            assert!(sig.structured_arg_core_types().is_some());
            assert_eq!(sig.has_where_params(), expected);
        }
    }

    /// Test that the scoring constants maintain their invariant relationship.
    /// The penalty for Any-arg matching specific-param must fully negate the
    /// bonus for exact primitive match to ensure methods with Any parameters
    /// are preferred when argument type is unknown.
    #[test]
    fn test_scoring_constants_invariant() {
        // The invariant: penalty should fully negate bonus
        assert_eq!(
            dispatch_resolver::EXACT_PRIMITIVE_MATCH_BONUS
                + dispatch_resolver::ANY_ARG_SPECIFIC_PARAM_PENALTY,
            0,
            "ANY_ARG_SPECIFIC_PARAM_PENALTY must equal -EXACT_PRIMITIVE_MATCH_BONUS"
        );
    }

    #[test]
    fn test_union_struct_parent_fallback_matches_abstract_issue_5605() {
        let projection = projection_from_parent_links(&[
            ("HasLength", Some("IteratorSize")),
            ("SizeUnknown", Some("IteratorSize")),
        ]);

        let param = JuliaType::AbstractUser("IteratorSize".to_string(), Some("Any".to_string()));
        let arg = JuliaType::Union(vec![
            JuliaType::Struct("HasLength".to_string()),
            JuliaType::Struct("SizeUnknown".to_string()),
        ]);

        assert!(needs_struct_parent_fallback(&param, &arg, &[], &projection));
        assert!(julia_type_matches_with_struct_parents(
            &param,
            &arg,
            &[],
            &projection
        ));
    }

    #[test]
    fn test_parametric_struct_where_bound_dispatch_issue_5646() {
        // struct Circle{T<:Real} <: Shape  +  struct Box{T} end (no parent).
        // Parametric structs are seeded into the hierarchy under their base
        // name (with `None` for parentless ones) by compile/mod.rs.
        let projection = projection_from_parent_links(&[("Circle", Some("Shape")), ("Box", None)]);

        // f(x::T) where {T<:Shape} : the bound carrier is a TypeVar.
        let param = JuliaType::TypeVar("T".to_string(), Some("Shape".to_string()));
        let circle = JuliaType::Struct("Circle{Float64}".to_string());
        let boxx = JuliaType::Struct("Box{Int64}".to_string());

        // Parametric Circle{Float64} matches the bound — gate AND match agree.
        assert!(needs_struct_parent_fallback(
            &param,
            &circle,
            &[],
            &projection
        ));
        assert!(julia_type_matches_with_struct_parents(
            &param,
            &circle,
            &[],
            &projection
        ));
        // Unrelated Box{Int64} must NOT match (declared no parent).
        assert!(!julia_type_matches_with_struct_parents(
            &param,
            &boxx,
            &[],
            &projection
        ));

        // Same via the bare `Struct(var_name)` carrier, bound on `type_params`.
        let param_var = JuliaType::Struct("T".to_string());
        let type_params = vec![TypeParam::with_bound("T".to_string(), "Shape".to_string())];
        assert!(julia_type_matches_with_struct_parents(
            &param_var,
            &circle,
            &type_params,
            &projection
        ));
        assert!(!julia_type_matches_with_struct_parents(
            &param_var,
            &boxx,
            &type_params,
            &projection
        ));
    }

    #[test]
    fn struct_hierarchy_projection_preserves_legacy_parent_maps_issue_5920() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "Main.Car{Int64}",
            Some("MotorVehicle".to_string()),
            Vec::new(),
        );
        hierarchy.insert(
            "Circle{T}",
            Some("Shape{T}".to_string()),
            vec!["T".to_string()],
        );
        hierarchy.insert("Box{T}", None, vec!["T".to_string()]);
        hierarchy.insert("MotorVehicle", Some("Vehicle".to_string()), Vec::new());

        let mut table = MethodTable::new("f".to_string());
        table.set_struct_hierarchy_projection(
            &hierarchy,
            &["Main.Car{Int64}".to_string()],
            &["Circle{T}".to_string(), "Box{T}".to_string()],
            &["MotorVehicle".to_string()],
        );

        assert_eq!(
            table.projection.declared_parent_link("Car"),
            Some(Some("MotorVehicle".to_string()))
        );
        assert_eq!(
            table.projection.declared_parent_link("Circle"),
            Some(Some("Shape{T}".to_string()))
        );
        assert_eq!(table.projection.declared_parent_link("Box"), Some(None));
        assert_eq!(
            table.projection.declared_parent_link("MotorVehicle"),
            Some(Some("Vehicle".to_string()))
        );
        assert!(table.projection.has_parent_links());
        assert_eq!(table.projection.struct_hierarchy, hierarchy);
    }

    /// Issue #6348: compile builds the struct-hierarchy projection once and
    /// shares the same `Arc` across all method tables; installing it must not
    /// rebuild or clone per table.
    #[test]
    fn shared_projection_is_arc_shared_across_tables_issue_6348() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Car", Some("MotorVehicle".to_string()), Vec::new());
        hierarchy.insert("MotorVehicle", Some("Vehicle".to_string()), Vec::new());

        let projection = Arc::new(MethodTableProjection::build(
            &hierarchy,
            &["Car".to_string()],
            &[],
            &["MotorVehicle".to_string()],
        ));

        let mut table_a = MethodTable::new("f".to_string());
        let mut table_b = MethodTable::new("g".to_string());
        table_a.set_shared_projection(Arc::clone(&projection));
        table_b.set_shared_projection(Arc::clone(&projection));

        assert!(Arc::ptr_eq(&table_a.projection, &table_b.projection));
        assert_eq!(
            table_a.projection.declared_parent_link("Car"),
            Some(Some("MotorVehicle".to_string()))
        );
        assert_eq!(
            table_b.projection.declared_parent_link("MotorVehicle"),
            Some(Some("Vehicle".to_string()))
        );
    }

    #[test]
    fn test_method_sig_core_signature_wraps_where_params() {
        let sig = MethodSig::for_tests(
            0,
            0,
            vec![(
                "x".to_string(),
                JuliaType::Struct("Rational{T}".to_string()),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Integer".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        );

        let core_sig = sig.core_signature();
        let CoreType::UnionAll { var, body } = core_sig else {
            panic!("expected UnionAll-wrapped signature");
        };
        assert_eq!(var.name, "T");
        assert_eq!(
            var.upper_bound.as_deref(),
            Some(&CoreType::Abstract(
                subset_julia_vm_types::inference_core::CoreAbstract::Integer
            ))
        );
        assert!(matches!(*body, CoreType::Tuple(_)));
    }

    #[test]
    fn issue_5926_method_sig_where_wrap_preserves_nested_vector_typevar() {
        let vector_sig = MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Struct("Vector{T}".to_string()))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        );
        let abstract_vector_sig = MethodSig::for_tests(
            1,
            1,
            vec![(
                "x".to_string(),
                JuliaType::AbstractUser(
                    "AbstractVector".to_string(),
                    Some("AbstractArray".to_string()),
                ),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        );

        let vector_core = vector_sig.core_signature();
        let abstract_vector_core = abstract_vector_sig.core_signature();
        let hierarchy = StructHierarchy::new();
        assert!(
            vector_core.is_subtype_of_with_hierarchy(&abstract_vector_core, &hierarchy),
            "Tuple{{Vector{{T}}}} where T must subtype Tuple{{AbstractVector}}"
        );
        assert!(
            vector_core.strict_subtype_dominates_with_hierarchy(&abstract_vector_core, &hierarchy),
            "where-wrapped nested Vector{{T}} signature must be strictly more \
             specific than the AbstractVector fallback"
        );
    }

    #[test]
    fn issue_5926_method_origin_uses_global_index_not_base_extension_flag() {
        let method = |global_index: usize, is_base_extension: bool| {
            MethodSig::for_tests(
                0,
                global_index,
                vec![("x".to_string(), JuliaType::Any)],
                ValueType::Any,
                None,
                is_base_extension,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let base_function_count = 5;
        assert!(
            method(4, false).is_base_program_method(base_function_count),
            "Base-origin methods are identified by global_index, even when \
             they are not syntactic Base extensions"
        );
        assert!(
            !method(5, true).is_base_program_method(base_function_count),
            "user methods may syntactically extend Base but are not part of the \
             Base/prelude origin partition"
        );
        assert!(
            !method(0, false).is_base_program_method(0),
            "a program with no Base prefix has no Base-origin methods"
        );
    }

    #[test]
    fn issue_5926_method_table_tracks_base_function_count_for_origin_fences() {
        let method = |global_index: usize, is_base_extension: bool, param_type: JuliaType| {
            MethodSig::for_tests(
                0,
                global_index,
                vec![("x".to_string(), param_type)],
                ValueType::Any,
                None,
                is_base_extension,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let mut table = MethodTable::new("origin_5926".to_string());
        table.add_method(method(4, false, JuliaType::Any));
        table.add_method(method(5, true, JuliaType::Int64));

        table.set_base_function_count(5);
        assert!(
            table.is_base_program_method(&table.methods[0]),
            "method index below Program::base_function_count is Base-origin"
        );
        assert!(
            !table.is_base_program_method(&table.methods[1]),
            "a user method that syntactically extends Base is still user-origin"
        );

        table.set_base_function_count(0);
        assert!(
            !table.is_base_program_method(&table.methods[0]),
            "zero base_function_count means no Base-origin partition is visible"
        );
    }

    #[test]
    fn issue_5926_method_table_origin_fence_blocks_base_dominance_override() {
        let mut table = MethodTable::new("origin_fence_5926".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Struct("Vector{T}".to_string()))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![(
                "x".to_string(),
                JuliaType::AbstractUser(
                    "AbstractVector".to_string(),
                    Some("AbstractArray".to_string()),
                ),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.set_base_function_count(1);

        let matches = vec![(&table.methods[0], 1), (&table.methods[1], 2)];
        let arg_types = vec![JuliaType::VectorOf(Box::new(JuliaType::Int64))];
        assert_eq!(
            dominant_match_index(&matches, &arg_types, &StructHierarchy::new(), 0),
            Some(0),
            "without a Base prefix, Vector{{T}} is the unique dominance winner"
        );
        assert_eq!(
            dominant_match_index(&matches, &arg_types, &StructHierarchy::new(), 1),
            None,
            "a Base-origin dominance winner must not cross over a user-origin \
             candidate through the #5926 pre-check; callers fall back to the \
             existing score path until full morespecific integration is safe"
        );
    }

    #[test]
    fn add_method_stores_structured_signature_as_primary_identity() {
        let mut table = MethodTable::new("f".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Struct("T".to_string()))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Integer".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("x".to_string(), JuliaType::Struct("T".to_string()))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Real".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table.methods.len(),
            2,
            "same parameter projection but different UnionAll bounds must not replace each other"
        );
        assert!(matches!(
            table.methods[0].core_signature,
            CoreType::UnionAll { .. }
        ));
        assert_ne!(
            table.methods[0].core_signature,
            table.methods[1].core_signature
        );
    }

    #[test]
    fn repl_method_identity_is_signature_stable_9784() {
        let sig = |global_index, params, type_params, vararg_param_index| {
            MethodSig::from_julia_projections(
                0,
                global_index,
                params,
                ValueType::Any,
                None,
                false,
                type_params,
                vararg_param_index,
                None,
            )
        };
        let int_at_10 = sig(10, vec![("x".to_string(), JuliaType::Int64)], vec![], None);
        let int_at_99 = sig(
            99,
            vec![("value".to_string(), JuliaType::Int64)],
            vec![],
            None,
        );
        let left = ReplMethodIdentity::from_method_sig("identity_9784", &int_at_10);
        let right = ReplMethodIdentity::from_method_sig("identity_9784", &int_at_99);
        assert_eq!(
            left, right,
            "global indices and parameter names are not identity"
        );
        assert_ne!(
            left,
            ReplMethodIdentity::from_method_sig("other_9784", &int_at_99),
            "the generic-function binding is part of identity"
        );

        let two_args = sig(
            100,
            vec![
                ("x".to_string(), JuliaType::Int64),
                ("y".to_string(), JuliaType::Int64),
            ],
            vec![],
            None,
        );
        assert_ne!(
            right,
            ReplMethodIdentity::from_method_sig("identity_9784", &two_args)
        );

        let vararg = sig(
            101,
            vec![("xs".to_string(), JuliaType::Int64)],
            vec![],
            Some(0),
        );
        assert_ne!(
            right,
            ReplMethodIdentity::from_method_sig("identity_9784", &vararg),
            "fixed and vararg methods occupy distinct slots"
        );

        let where_t = sig(
            102,
            vec![("x".to_string(), JuliaType::TypeVar("T".to_string(), None))],
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Real".to_string(),
            )],
            None,
        );
        let where_s = sig(
            103,
            vec![("x".to_string(), JuliaType::TypeVar("S".to_string(), None))],
            vec![TypeParam::with_upper_bound(
                "S".to_string(),
                "Real".to_string(),
            )],
            None,
        );
        assert_eq!(
            ReplMethodIdentity::from_method_sig("identity_9784", &where_t),
            ReplMethodIdentity::from_method_sig("identity_9784", &where_s),
            "alpha-equivalent where binders denote one method slot"
        );
    }

    /// `from_julia_projections` must derive the canonical `core_signature`
    /// eagerly and identically to the test helper that derives from the same
    /// JuliaType projections. Production sites all construct through it, so a
    /// `Bottom` placeholder is never observable outside tests (Issue #6495).
    #[test]
    fn from_julia_projections_eagerly_derives_core_signature_issue_6495() {
        let shapes: Vec<(
            Vec<(String, JuliaType)>,
            Vec<TypeParam>,
            Option<usize>,
            Option<usize>,
        )> = vec![
            // f() — zero params must still be structured (empty Tuple, not Bottom)
            (vec![], vec![], None, None),
            // f(x::Int64, y)
            (
                vec![
                    ("x".to_string(), JuliaType::Int64),
                    ("y".to_string(), JuliaType::Any),
                ],
                vec![],
                None,
                None,
            ),
            // f(v::Vector{T}, x::T) where T<:Real, with vararg metadata
            (
                vec![
                    (
                        "v".to_string(),
                        JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ],
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                Some(1),
                Some(2),
            ),
        ];
        for (params, type_params, vararg_param_index, vararg_fixed_count) in shapes {
            let literal = MethodSig::for_tests(
                3,
                7,
                params.clone(),
                ValueType::Any,
                None,
                true,
                type_params.clone(),
                CoreType::Bottom,
                vararg_param_index,
                vararg_fixed_count,
            );

            let constructed = MethodSig::from_julia_projections(
                3,
                7,
                params,
                ValueType::Any,
                None,
                true,
                type_params,
                vararg_param_index,
                vararg_fixed_count,
            );

            assert_eq!(constructed.core_signature, literal.core_signature);
            assert!(
                constructed.structured_arg_core_types().is_some(),
                "constructor must never leave a Bottom placeholder: {:?}",
                constructed.core_signature
            );
            assert_eq!(
                constructed.projected_param_julia_types(),
                literal.projected_param_julia_types()
            );
            assert_eq!(
                constructed.projected_type_params(),
                literal.projected_type_params()
            );
            assert_eq!(constructed._method_index, 3);
            assert_eq!(constructed.global_index, 7);
            assert!(constructed.is_base_extension);
            assert_eq!(constructed.vararg_param_index, vararg_param_index);
            assert_eq!(constructed.vararg_fixed_count, vararg_fixed_count);
        }
    }

    #[test]
    fn fixed_method_not_replaced_by_vararg_and_beats_it_issue_5924() {
        // `g(x::Int)` and `g(x::Int...)` are distinct signatures (`Tuple{Int}` vs
        // `Tuple{Vararg{Int}}`). They must NOT dedup each other, and the
        // exact-arity fixed method must win for `g(1)` regardless of declaration
        // order (Issue #5924).
        let fixed = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![("x".to_string(), JuliaType::Int64)],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let vararg = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![("x".to_string(), JuliaType::Int64)],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                Some(0),
                None,
            )
        };

        // Fixed declared first, then vararg.
        let mut table = MethodTable::new("g".to_string());
        table.add_method(fixed(100));
        table.add_method(vararg(200));
        assert_eq!(
            table.methods.len(),
            2,
            "fixed and vararg methods must not replace each other"
        );
        assert!(
            matches!(
                table.dispatch(&[JuliaType::Int64]).map(|m| m.global_index),
                Ok(100)
            ),
            "g(1) must select the exact-arity fixed method (fixed-first order)"
        );

        // Vararg declared first, then fixed (order independence).
        let mut table = MethodTable::new("g".to_string());
        table.add_method(vararg(200));
        table.add_method(fixed(100));
        assert_eq!(table.methods.len(), 2);
        assert!(
            matches!(
                table.dispatch(&[JuliaType::Int64]).map(|m| m.global_index),
                Ok(100)
            ),
            "g(1) must select the exact-arity fixed method (vararg-first order)"
        );

        // Control: two args have no exact fixed match, so the vararg method wins.
        assert!(
            matches!(
                table
                    .dispatch(&[JuliaType::Int64, JuliaType::Int64])
                    .map(|m| m.global_index),
                Ok(200)
            ),
            "g(2, 3) must route to the vararg method"
        );
    }

    #[test]
    fn partial_parametric_struct_fixed_vararg_beats_generic_vararg_issue_8407() {
        let generic = MethodSig::for_tests(
            0,
            10,
            vec![
                ("x".to_string(), JuliaType::Any),
                ("ys".to_string(), JuliaType::Any),
            ],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            Some(1),
            None,
        );
        let specific = MethodSig::for_tests(
            1,
            20,
            vec![
                (
                    "x".to_string(),
                    JuliaType::Struct("QuadGK.BatchIntegrand{Y, Nothing}".to_string()),
                ),
                ("y".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ("z".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                (
                    "rest".to_string(),
                    JuliaType::TypeVar("T".to_string(), None),
                ),
            ],
            ValueType::I64,
            None,
            false,
            vec![
                TypeParam::new("Y".to_string()),
                TypeParam::new("T".to_string()),
            ],
            CoreType::Bottom,
            Some(3),
            None,
        );
        let mut table = MethodTable::new("myq".to_string());
        table.add_method(generic);
        table.add_method(specific);

        let actual_batch = JuliaType::Struct(
            "QuadGK.BatchIntegrand{Float64, Nothing, Vector{Float64}, Vector{Nothing}, typeof(f!)}"
                .to_string(),
        );
        assert_eq!(
            table
                .dispatch(&[actual_batch, JuliaType::Float64, JuliaType::Float64])
                .expect("dispatch")
                .global_index,
            20
        );
    }

    #[test]
    fn empty_trailing_vararg_compares_declared_element_type_issue_6216() {
        let pure_vararg = |idx: usize, ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![("xs".to_string(), ty)],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                Some(0),
                None,
            )
        };

        let mut table = MethodTable::new("empty_vararg_specificity_6216".to_string());
        table.add_method(pure_vararg(10, JuliaType::Int64));
        table.add_method(pure_vararg(20, JuliaType::Integer));
        assert_eq!(
            table.dispatch(&[]).unwrap().global_index,
            10,
            "f() must keep the declared Vararg element type in specificity"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::Int64, JuliaType::Int64])
                .unwrap()
                .global_index,
            10,
            "non-empty varargs already score the repeated element type"
        );

        let mut reversed = MethodTable::new("empty_vararg_specificity_reversed_6216".to_string());
        reversed.add_method(pure_vararg(20, JuliaType::Integer));
        reversed.add_method(pure_vararg(10, JuliaType::Int64));
        assert_eq!(
            reversed.dispatch(&[]).unwrap().global_index,
            10,
            "empty vararg specificity must not depend on declaration order"
        );

        let leading_fixed_vararg = |idx: usize, ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("head".to_string(), JuliaType::String),
                    ("xs".to_string(), ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                Some(1),
                None,
            )
        };
        let mut prefixed = MethodTable::new("empty_prefixed_vararg_specificity_6216".to_string());
        prefixed.add_method(leading_fixed_vararg(30, JuliaType::Integer));
        prefixed.add_method(leading_fixed_vararg(40, JuliaType::Int64));
        assert_eq!(
            prefixed
                .dispatch(&[JuliaType::String])
                .unwrap()
                .global_index,
            40,
            "a call with only the fixed prefix still compares trailing Vararg elements"
        );
    }

    #[test]
    fn typed_vararg_where_beats_untyped_vararg_fallback_issue_8565() {
        let typed_diag = MethodSig::for_tests(
            0,
            10,
            vec![("xs".to_string(), JuliaType::TypeVar("T".to_string(), None))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            Some(0),
            None,
        );
        let untyped_fallback = MethodSig::for_tests(
            1,
            20,
            vec![("xs".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            Some(0),
            None,
        );

        let mut table = MethodTable::new("typed_vararg_where_8565".to_string());
        table.add_method(typed_diag);
        table.add_method(untyped_fallback);

        for args in [
            vec![],
            vec![JuliaType::Int64],
            vec![JuliaType::Int64, JuliaType::Int64],
            vec![JuliaType::Float64, JuliaType::Float64, JuliaType::Float64],
        ] {
            assert_eq!(table.dispatch(&args).unwrap().global_index, 10, "{args:?}");
        }
        assert_eq!(
            table
                .dispatch(&[JuliaType::Int64, JuliaType::Float64])
                .unwrap()
                .global_index,
            20,
            "heterogeneous varargs do not satisfy the diagonal T... binding"
        );
    }

    #[test]
    fn tuple_vararg_specificity_uses_actual_tuple_shape_issue_6218() {
        let tuple_method = |idx: usize, elems: Vec<JuliaType>| {
            MethodSig::for_tests(
                0,
                idx,
                vec![("x".to_string(), JuliaType::TupleOf(elems))],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let all_int = || vec![JuliaType::Struct("Vararg{Int64}".to_string())];
        let int_any_tail = || {
            vec![
                JuliaType::Int64,
                JuliaType::Struct("Vararg{Any}".to_string()),
            ]
        };

        let mut table = MethodTable::new("tuple_vararg_specificity_6218".to_string());
        table.add_method(tuple_method(10, all_int()));
        table.add_method(tuple_method(20, int_any_tail()));
        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64])])
                .unwrap()
                .global_index,
            10,
            "Tuple{{Vararg{{Int64}}}} wins when the fixed prefix consumes the only slot"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])])
                .unwrap()
                .global_index,
            10,
            "Tuple{{Vararg{{Int64}}}} expands to the stricter actual tuple shape"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::String
                ])])
                .unwrap()
                .global_index,
            20,
            "mixed tails still use the fixed-prefix Vararg{{Any}} fallback"
        );

        let mut reversed = MethodTable::new("tuple_vararg_specificity_reversed_6218".to_string());
        reversed.add_method(tuple_method(20, int_any_tail()));
        reversed.add_method(tuple_method(10, all_int()));
        assert_eq!(
            reversed
                .dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])])
                .unwrap()
                .global_index,
            10,
            "tuple vararg specificity must not depend on declaration order"
        );

        let mut same_tail = MethodTable::new("tuple_vararg_same_tail_6218".to_string());
        same_tail.add_method(tuple_method(30, all_int()));
        same_tail.add_method(tuple_method(
            40,
            vec![
                JuliaType::Int64,
                JuliaType::Struct("Vararg{Int64}".to_string()),
            ],
        ));
        assert_eq!(
            same_tail
                .dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])])
                .unwrap()
                .global_index,
            40,
            "when expanded shape and Vararg element are equal, existing fixed-prefix scoring wins"
        );
    }

    #[test]
    fn tuple_vararg_conflicting_specificity_stays_ambiguous_issue_6220() {
        let tuple_method = |idx: usize, elems: Vec<JuliaType>| {
            MethodSig::for_tests(
                0,
                idx,
                vec![("x".to_string(), JuliaType::TupleOf(elems))],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("tuple_vararg_ambiguity_6220".to_string());
        table.add_method(tuple_method(
            10,
            vec![JuliaType::Struct("Vararg{Integer}".to_string())],
        ));
        table.add_method(tuple_method(
            20,
            vec![
                JuliaType::Int64,
                JuliaType::Struct("Vararg{Any}".to_string()),
            ],
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![])])
                .unwrap()
                .global_index,
            10,
            "empty tuple only matches Tuple{{Vararg{{Integer}}}}"
        );
        assert!(
            matches!(
                table.dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64])]),
                Err(DispatchError::AmbiguousMethod { .. })
            ),
            "single Int tuple has prefix-slot and Vararg-element specificity in opposite directions"
        );
        assert!(
            matches!(
                table.dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])]),
                Err(DispatchError::AmbiguousMethod { .. })
            ),
            "all-Int tuples remain ambiguous when prefix and Vararg specificity conflict"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::String
                ])])
                .unwrap()
                .global_index,
            20,
            "mixed tail only matches the fixed-prefix fallback"
        );
    }

    #[test]
    fn union_actual_arm_specificity_issue_6231() {
        let method = |idx: usize, ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![("x".to_string(), ty)],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut int_string_vs_integer =
            MethodTable::new("union_actual_arm_int_string_6231".to_string());
        int_string_vs_integer.add_method(method(
            10,
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]),
        ));
        int_string_vs_integer.add_method(method(20, JuliaType::Integer));
        assert_eq!(
            int_string_vs_integer
                .dispatch(&[JuliaType::Int64])
                .unwrap()
                .global_index,
            10,
            "Union{{Int64,String}} is more specific than Integer for Int64"
        );
        assert_eq!(
            int_string_vs_integer
                .dispatch(&[JuliaType::String])
                .unwrap()
                .global_index,
            10,
            "String only matches the finite Union method"
        );

        let mut integer_string_vs_real =
            MethodTable::new("union_actual_arm_integer_string_6231".to_string());
        integer_string_vs_real.add_method(method(
            30,
            JuliaType::Union(vec![JuliaType::Integer, JuliaType::String]),
        ));
        integer_string_vs_real.add_method(method(40, JuliaType::Real));
        assert_eq!(
            integer_string_vs_real
                .dispatch(&[JuliaType::Int64])
                .unwrap()
                .global_index,
            30,
            "Union{{Integer,String}} is more specific than Real for integer arguments"
        );
        assert_eq!(
            integer_string_vs_real
                .dispatch(&[JuliaType::Float64])
                .unwrap()
                .global_index,
            40,
            "Float64 does not satisfy the Union{{Integer,String}} arm"
        );

        let mut real_string_vs_integer =
            MethodTable::new("union_actual_arm_real_string_6231".to_string());
        real_string_vs_integer.add_method(method(
            50,
            JuliaType::Union(vec![JuliaType::Real, JuliaType::String]),
        ));
        real_string_vs_integer.add_method(method(60, JuliaType::Integer));
        assert_eq!(
            real_string_vs_integer
                .dispatch(&[JuliaType::Int64])
                .unwrap()
                .global_index,
            60,
            "an unrelated concrete Union arm must not make Union{{Real,String}} beat Integer"
        );

        let mut exact_int_vs_union = MethodTable::new("union_actual_arm_exact_6231".to_string());
        exact_int_vs_union.add_method(method(
            70,
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]),
        ));
        exact_int_vs_union.add_method(method(80, JuliaType::Int64));
        assert_eq!(
            exact_int_vs_union
                .dispatch(&[JuliaType::Int64])
                .unwrap()
                .global_index,
            80,
            "an exact concrete method remains more specific than a finite Union containing it"
        );
    }

    #[test]
    fn type_value_diagonal_beats_fixed_supertype_issue_6233() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty.clone()))),
                    ("x".to_string(), ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_value_diagonal_6233".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(20, JuliaType::Integer));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::Int64,
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Int64 selects the diagonal Type{{T}}, T method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::Int64,
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Integer method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::Float64,
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 satisfies only the diagonal method"
        );

        let mut exact = MethodTable::new("type_value_diagonal_exact_6233".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(40, JuliaType::Int64));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::Int64,
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, Int64 method remains more specific than the diagonal"
        );
    }

    #[test]
    fn exact_type_object_method_rejects_unrelated_type_object_issue_10782() {
        let sig = MethodSig::for_tests(
            0,
            10,
            vec![(
                "t".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::Struct(
                    "LayoutPredicateDispatchBox3911".to_string(),
                ))),
            )],
            ValueType::Bool,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        );
        let mut table = MethodTable::new("isbitstype".to_string());
        table.add_method(sig);

        assert!(
            matches!(
                table.dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Int64))]),
                Err(DispatchError::NoMethodFound { .. })
            ),
            "a method declared as Type{{LayoutPredicateDispatchBox3911}} must not match Type{{Int64}}"
        );
    }

    #[test]
    fn type_vector_diagonal_beats_fixed_supertype_issue_6235() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, elem: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), JuliaType::VectorOf(Box::new(elem))),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_vector_diagonal_6235".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Vector{{Int64}} selects the diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Vector{{<:Real}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 vector satisfies only the diagonal method"
        );

        let mut exact = MethodTable::new("type_vector_diagonal_exact_6235".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(40, JuliaType::Int64, JuliaType::Int64));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, Vector{{Int64}} method remains more specific"
        );
    }

    #[test]
    fn type_abstract_vector_diagonal_beats_fixed_supertype_issue_6239() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::Struct("AbstractVector{T}".to_string()),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, vec_ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), vec_ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_abstract_vector_diagonal_6239".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::Struct("AbstractVector{<:Real}".to_string()),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Vector{{Int64}} selects the AbstractVector diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractVector{{<:Real}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 vector satisfies only the AbstractVector diagonal method"
        );

        let mut exact = MethodTable::new("type_abstract_vector_diagonal_exact_6239".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(
            40,
            JuliaType::Int64,
            JuliaType::Struct("AbstractVector{Int64}".to_string()),
        ));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractVector{{Int64}} method remains more specific"
        );
    }

    #[test]
    fn type_abstract_array_rank1_diagonal_beats_fixed_supertype_issue_6245() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::Struct("AbstractArray{T,1}".to_string()),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, array_ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), array_ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_abstract_array_rank1_diagonal_6245".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::Struct("AbstractArray{<:Real,1}".to_string()),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Vector{{Int64}} selects the AbstractArray rank-1 diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,1}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 vector satisfies only the AbstractArray rank-1 diagonal method"
        );

        let mut exact =
            MethodTable::new("type_abstract_array_rank1_diagonal_exact_6245".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(
            40,
            JuliaType::Int64,
            JuliaType::Struct("AbstractArray{Int64,1}".to_string()),
        ));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractArray{{Int64,1}} method remains more specific"
        );
    }

    #[test]
    fn type_abstract_array_rank_omitted_diagonal_beats_fixed_supertype_issue_6247() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::Struct("AbstractArray{T}".to_string()),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, array_ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), array_ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_abstract_array_rank_omitted_6247".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::Struct("AbstractArray{<:Real}".to_string()),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "rank-omitted AbstractArray diagonal method covers concrete Vector{{Int64}} actuals"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "rank-omitted AbstractArray diagonal method covers concrete Matrix{{Int64}} actuals"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 vector satisfies only the rank-omitted AbstractArray diagonal method"
        );

        let mut exact = MethodTable::new("type_abstract_array_rank_omitted_exact_6247".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(
            40,
            JuliaType::Int64,
            JuliaType::Struct("AbstractArray{Int64}".to_string()),
        ));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractArray{{Int64}} method remains more specific for vectors"
        );
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractArray{{Int64}} method remains more specific for matrices"
        );
    }

    #[test]
    fn type_abstract_array_rank_typevar_diagonal_beats_fixed_supertype_issue_6249() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::Struct("AbstractArray{T,N}".to_string()),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![
                    TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
                    TypeParam::new("N".to_string()),
                ],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed =
            |idx: usize, ty: JuliaType, array_ty: JuliaType, type_params: Vec<TypeParam>| {
                MethodSig::for_tests(
                    0,
                    idx,
                    vec![
                        ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                        ("x".to_string(), array_ty),
                    ],
                    ValueType::Any,
                    None,
                    false,
                    type_params,
                    CoreType::Bottom,
                    None,
                    None,
                )
            };

        let mut table = MethodTable::new("type_abstract_array_rank_typevar_6249".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::Struct("AbstractArray{<:Real,N}".to_string()),
            vec![TypeParam::new("N".to_string())],
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "rank-TypeVar AbstractArray diagonal method covers concrete Vector{{Int64}} actuals"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "rank-TypeVar AbstractArray diagonal method covers concrete Matrix{{Int64}} actuals"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,N}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 vector satisfies only the rank-TypeVar AbstractArray diagonal method"
        );

        let mut exact = MethodTable::new("type_abstract_array_rank_typevar_exact_6249".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(
            40,
            JuliaType::Int64,
            JuliaType::Struct("AbstractArray{Int64,N}".to_string()),
            vec![TypeParam::new("N".to_string())],
        ));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractArray{{Int64,N}} method remains more specific for vectors"
        );
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractArray{{Int64,N}} method remains more specific for matrices"
        );
    }

    #[test]
    fn type_matrix_diagonal_beats_fixed_supertype_issue_6237() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::MatrixOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, elem: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), JuliaType::MatrixOf(Box::new(elem))),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_matrix_diagonal_6237".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Matrix{{Int64}} selects the diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Matrix{{<:Real}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 matrix satisfies only the diagonal method"
        );

        let mut exact = MethodTable::new("type_matrix_diagonal_exact_6237".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(40, JuliaType::Int64, JuliaType::Int64));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, Matrix{{Int64}} method remains more specific"
        );
    }

    #[test]
    fn type_abstract_matrix_diagonal_beats_fixed_supertype_issue_6240() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::Struct("AbstractMatrix{T}".to_string()),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, mat_ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), mat_ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_abstract_matrix_diagonal_6240".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::Struct("AbstractMatrix{<:Real}".to_string()),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Matrix{{Int64}} selects the AbstractMatrix diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractMatrix{{<:Real}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 matrix satisfies only the AbstractMatrix diagonal method"
        );

        let mut exact = MethodTable::new("type_abstract_matrix_diagonal_exact_6240".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(
            40,
            JuliaType::Int64,
            JuliaType::Struct("AbstractMatrix{Int64}".to_string()),
        ));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractMatrix{{Int64}} method remains more specific"
        );
    }

    #[test]
    fn type_abstract_array_rank2_diagonal_beats_fixed_supertype_issue_6243() {
        let diagonal = |idx: usize| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    (
                        "t".to_string(),
                        JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                    ),
                    (
                        "x".to_string(),
                        JuliaType::Struct("AbstractArray{T,2}".to_string()),
                    ),
                ],
                ValueType::Any,
                None,
                false,
                vec![TypeParam::with_upper_bound(
                    "T".to_string(),
                    "Real".to_string(),
                )],
                CoreType::Bottom,
                None,
                None,
            )
        };
        let fixed = |idx: usize, ty: JuliaType, array_ty: JuliaType| {
            MethodSig::for_tests(
                0,
                idx,
                vec![
                    ("t".to_string(), JuliaType::TypeOf(Box::new(ty))),
                    ("x".to_string(), array_ty),
                ],
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            )
        };

        let mut table = MethodTable::new("type_abstract_array_rank2_diagonal_6243".to_string());
        table.add_method(diagonal(10));
        table.add_method(fixed(
            20,
            JuliaType::Integer,
            JuliaType::Struct("AbstractArray{<:Real,2}".to_string()),
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "concrete Type{{Int64}} plus Matrix{{Int64}} selects the AbstractArray rank-2 diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Integer)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            20,
            "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,2}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            10,
            "Float64 matrix satisfies only the AbstractArray rank-2 diagonal method"
        );

        let mut exact =
            MethodTable::new("type_abstract_array_rank2_diagonal_exact_6243".to_string());
        exact.add_method(diagonal(30));
        exact.add_method(fixed(
            40,
            JuliaType::Int64,
            JuliaType::Struct("AbstractArray{Int64,2}".to_string()),
        ));
        assert_eq!(
            exact
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Int64)),
                    JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            40,
            "an exact Type{{Int64}}, AbstractArray{{Int64,2}} method remains more specific"
        );
    }

    #[test]
    fn vector_diagonal_beats_independent_bounds_issue_6229() {
        let mut table = MethodTable::new("vector_diagonal_6229".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![
                (
                    "x".to_string(),
                    JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
                (
                    "y".to_string(),
                    JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
            ],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Real".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            0,
            20,
            vec![
                (
                    "x".to_string(),
                    JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                        "_".to_string(),
                        Some("Real".to_string()),
                    ))),
                ),
                (
                    "y".to_string(),
                    JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                        "_".to_string(),
                        Some("Real".to_string()),
                    ))),
                ),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ])
                .unwrap()
                .global_index,
            10,
            "same concrete vector element type must select the diagonal Vector{{T}} method"
        );
        assert_eq!(
            table
                .dispatch(&[
                    JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                    JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                ])
                .unwrap()
                .global_index,
            20,
            "mixed vector element types do not satisfy the repeated T binding"
        );
    }

    #[test]
    fn matrix_covariant_bound_dispatch_rejects_float_issue_4020() {
        let mut table = MethodTable::new("f".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![("A".to_string(), JuliaType::Any)],
            ValueType::F64,
            Some(JuliaType::Float64),
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![(
                "A".to_string(),
                JuliaType::MatrixOf(Box::new(JuliaType::TypeVar(
                    "_".to_string(),
                    Some("Integer".to_string()),
                ))),
            )],
            ValueType::I64,
            Some(JuliaType::Int64),
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        assert!(matches!(
            table
                .dispatch(&[JuliaType::MatrixOf(Box::new(JuliaType::Int64))])
                .map(|m| m.global_index),
            Ok(11)
        ));
        assert!(matches!(
            table
                .dispatch(&[JuliaType::MatrixOf(Box::new(JuliaType::Float64))])
                .map(|m| m.global_index),
            Ok(10)
        ));
        assert!(matches!(
            table.dispatch(&[JuliaType::Any]).map(|m| m.global_index),
            Ok(10)
        ));
    }

    #[test]
    fn issue_5926_method_table_dominance_selects_vector_over_abstractvector() {
        let mut table = MethodTable::new("fam1_5926".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "x".to_string(),
                JuliaType::AbstractUser(
                    "AbstractVector".to_string(),
                    Some("AbstractArray".to_string()),
                ),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![("x".to_string(), JuliaType::Struct("Vector{T}".to_string()))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::VectorOf(Box::new(JuliaType::Int64))])
                .expect("Vector{Int64} should dispatch")
                .global_index,
            11,
            "compile-time dispatch must use the #5926 dominance pre-check so \
             Vector{{T}} wins over the AbstractVector fallback"
        );
    }

    #[test]
    fn issue_5926_method_table_dominance_selects_diagonal_over_any_any() {
        let mut table = MethodTable::new("diagonal_5926".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![
                ("x".to_string(), JuliaType::Any),
                ("y".to_string(), JuliaType::Any),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![
                ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ("y".to_string(), JuliaType::TypeVar("T".to_string(), None)),
            ],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::Int64, JuliaType::Int64])
                .expect("same-typed args should dispatch")
                .global_index,
            11,
            "compile-time dispatch must use the #5926 dominance pre-check so \
             Tuple{{T,T}} wins over Tuple{{Any,Any}} for same-typed args"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::Int64, JuliaType::Float64])
                .expect("mixed args should dispatch")
                .global_index,
            10,
            "mixed args do not satisfy the diagonal rule, so the Any fallback wins"
        );
    }

    #[test]
    fn tuple_bounded_fallback_after_diagonal_rejects_mixed_issue_6251() {
        let mut table = MethodTable::new("tuple_bounded_fallback_6251".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "x".to_string(),
                JuliaType::TupleOf(vec![
                    JuliaType::TypeVar("T".to_string(), None),
                    JuliaType::TypeVar("T".to_string(), None),
                ]),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Real".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            20,
            vec![(
                "x".to_string(),
                JuliaType::TupleOf(vec![
                    JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
                    JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
                ]),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])])
                .unwrap()
                .global_index,
            10,
            "homogeneous concrete real tuple keeps the diagonal method"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Float64,
                ])])
                .unwrap()
                .global_index,
            20,
            "mixed real tuple falls back to Tuple{{<:Real,<:Real}}"
        );
        assert!(
            table
                .dispatch(&[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::String,
                ])])
                .is_err(),
            "non-Real tuple element must not match the bounded fallback"
        );
    }

    #[test]
    fn issue_5926_method_table_preserves_bounded_typevar_over_untyped_any_5375() {
        let mut table = MethodTable::new("bounded_5926".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![(
                "x".to_string(),
                JuliaType::TypeVar("T".to_string(), Some("Number".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::Int64])
                .expect("Int64 should dispatch")
                .global_index,
            11,
            "compile-time dispatch must preserve the #5375 regression: \
             T<:Number beats the untyped Any fallback"
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::String])
                .expect("String should fall back to Any")
                .global_index,
            10,
            "non-Number arguments should still use the untyped fallback"
        );
    }

    #[test]
    fn typevar_bound_checks_use_coretype_structured_families() {
        let param_types = vec![JuliaType::Struct("T".to_string())];
        let type_params = vec![TypeParam::with_upper_bound(
            "T<:AbstractVector".to_string(),
            "T<:AbstractVector".to_string(),
        )];

        assert_eq!(
            dispatch_resolver::julia_signature_match_with_bindings(
                &param_types,
                &[JuliaType::VectorOf(Box::new(JuliaType::Int64))],
                &type_params,
            ),
            Some(1)
        );
        assert_eq!(
            dispatch_resolver::julia_signature_match_with_bindings(
                &param_types,
                &[JuliaType::Dict],
                &type_params
            ),
            None
        );
    }

    /// A concrete-projected parameter (`f(sz::Integer)`) and a covariant
    /// `where` variable used once with the same bound (`f(sz::T) where
    /// {T<:Integer}`) describe the *same* signature upstream
    /// (`Tuple{T} where T<:Integer == Tuple{Integer}`), so the later definition
    /// redefines the earlier one rather than coexisting as a dispatch tie
    /// (Issue #5383).
    #[test]
    fn equivalent_unionall_projection_redefines_concrete_signature_issue_5383() {
        let mut table = MethodTable::new("Channel".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("sz".to_string(), JuliaType::Integer)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("sz".to_string(), JuliaType::TypeVar("T".to_string(), None))],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Integer".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));

        // Equivalent signatures dedup to a single, last-defined method.
        assert_eq!(table.methods.len(), 1);
        let selected = table.dispatch(&[JuliaType::Int64]).expect("dispatch");
        assert_eq!(selected.global_index, 1);
    }

    #[test]
    fn test_repeated_type_var_requires_exact_binding() {
        let param_types = vec![
            JuliaType::Struct("T".to_string()),
            JuliaType::Struct("T".to_string()),
        ];
        let type_params = vec![TypeParam::new("T".to_string())];

        assert_eq!(
            dispatch_resolver::julia_signature_match_with_bindings(
                &param_types,
                &[
                    JuliaType::BigInt,
                    JuliaType::Struct("Rational{Int64}".to_string())
                ],
                &type_params,
            ),
            None
        );
        assert_eq!(
            dispatch_resolver::julia_signature_match_with_bindings(
                &param_types,
                &[
                    JuliaType::Struct("Rational".to_string()),
                    JuliaType::Struct("Rational{Int64}".to_string())
                ],
                &type_params,
            ),
            None
        );
    }

    /// Test that when argument type is Any, methods with Any parameters are
    /// preferred over methods with specific parameters (Issue #1665).
    #[test]
    fn test_any_arg_prefers_any_param_method() {
        let mut table = MethodTable::new("f".to_string());

        // Method 1: f(::Any)
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // Method 2: f(::Int64) - more specific
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // When called with Any, should prefer f(::Any)
        let result = table.dispatch(&[JuliaType::Any]);
        assert!(result.is_ok(), "Dispatch should succeed");
        let method = result.unwrap();
        assert_eq!(
            *method.projected_param_julia_type(0),
            JuliaType::Any,
            "Should select f(::Any) when argument type is Any"
        );
    }

    #[test]
    fn test_any_arg_single_specific_method_defers_runtime_5984() {
        let mut table = MethodTable::new("h".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::String)],
            ValueType::Str,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        assert!(
            matches!(
                table.dispatch(&[JuliaType::Any]),
                Err(subset_julia_vm_types::types::DispatchError::NoMethodFound { .. })
            ),
            "a statically Any argument must not commit to a single ::String method"
        );
        assert!(
            matches!(
                table.dispatch(&[JuliaType::Int64]),
                Err(subset_julia_vm_types::types::DispatchError::NoMethodFound { .. })
            ),
            "::Int64 must not match a lone ::String method"
        );
    }

    /// Test that when argument type is Int64, methods with Int64 parameters
    /// are still preferred over methods with Any parameters.
    #[test]
    fn test_concrete_arg_prefers_concrete_param_method() {
        let mut table = MethodTable::new("f".to_string());

        // Method 1: f(::Any)
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // Method 2: f(::Int64)
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // When called with Int64, should prefer f(::Int64)
        let result = table.dispatch(&[JuliaType::Int64]);
        assert!(result.is_ok(), "Dispatch should succeed");
        let method = result.unwrap();
        assert_eq!(
            *method.projected_param_julia_type(0),
            JuliaType::Int64,
            "Should select f(::Int64) when argument type is Int64"
        );
    }

    #[test]
    fn test_string_arg_prefers_string_param_over_any_issue_4309() {
        let mut table = MethodTable::new("f".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("x".to_string(), JuliaType::String)],
            ValueType::Str,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        let method = table
            .dispatch(&[JuliaType::String])
            .expect("String call should dispatch");
        assert_eq!(
            method.global_index, 1,
            "String argument should select f(::String), not f(::Any)"
        );
    }

    #[test]
    fn test_typevar_binding_reuse_rejects_mismatched_convert_identity() {
        let mut table = MethodTable::new("convert".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![
                (
                    "_".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
                ("x".to_string(), JuliaType::TypeVar("T".to_string(), None)),
            ],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        assert!(
            table
                .dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::Float64,
                ])
                .is_ok(),
            "convert(::Type{{T}}, x::T) should match when T is reused consistently"
        );

        assert!(
            matches!(
                table.dispatch(&[
                    JuliaType::TypeOf(Box::new(JuliaType::Float64)),
                    JuliaType::Int64,
                ]),
                Err(subset_julia_vm_types::types::DispatchError::NoMethodFound { .. })
            ),
            "convert(::Type{{T}}, x::T) must not match convert(Float64, 1)"
        );
    }

    #[test]
    fn test_type_any_singleton_dispatch_specificity_issue_4131() {
        let mut table = MethodTable::new("f".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![("T".to_string(), JuliaType::Type)],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![("T".to_string(), JuliaType::TypeOf(Box::new(JuliaType::Any)))],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        let any_method = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Any))])
            .expect("Any type object should dispatch");
        assert_eq!(any_method.global_index, 11);

        let int_method = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Int64))])
            .expect("Int64 type object should dispatch");
        assert_eq!(int_method.global_index, 10);
    }

    #[test]
    fn type_object_parametric_numeric_dispatch_uses_bound_method_issue_5365() {
        let mut table = MethodTable::new("eltype".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "_".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            )],
            ValueType::DataType,
            None,
            false,
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![("_".to_string(), JuliaType::DataType)],
            ValueType::DataType,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.insert_parent_link_for_tests("Complex", Some("Number".to_string()));
        table.insert_parent_link_for_tests("Rational", Some("Real".to_string()));
        table.insert_parent_link_for_tests("Real", Some("Number".to_string()));

        let complex_arg =
            JuliaType::TypeOf(Box::new(JuliaType::Struct("Complex{Float64}".to_string())));
        let projected_params = table.methods[0]
            .expanded_projected_param_julia_types_for_arity(1)
            .expect("bounded method should project one argument");
        let projected_type_params = table.methods[0].projected_type_params();
        assert!(
            needs_struct_parent_fallback(
                &projected_params[0],
                &complex_arg,
                &projected_type_params,
                table.projection(),
            ),
            "projected_params={projected_params:?} projected_type_params={projected_type_params:?}"
        );
        assert!(julia_type_matches_with_struct_parents(
            &projected_params[0],
            &complex_arg,
            &projected_type_params,
            table.projection(),
        ));
        assert!(
            table
                .signature_matches_arg_types(&table.methods[0], std::slice::from_ref(&complex_arg)),
            "bounded Type{{T<:Number}} method must match Complex{{Float64}} type object"
        );
        assert!(
            table
                .signature_matches_arg_types(&table.methods[1], std::slice::from_ref(&complex_arg)),
            "generic DataType method must also match type objects"
        );

        let complex = table
            .dispatch(std::slice::from_ref(&complex_arg))
            .expect("Complex{Float64} type object should dispatch");
        assert_eq!(complex.global_index, 10);

        let rational = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "Rational{Int64}".to_string(),
            )))])
            .expect("Rational{Int64} type object should dispatch");
        assert_eq!(rational.global_index, 10);
    }

    #[test]
    fn test_type_object_dispatch_ignores_value_level_array_patterns_issue_6251() {
        let mut table = MethodTable::new("IteratorSize".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![("x".to_string(), JuliaType::Type)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            2,
            12,
            vec![("x".to_string(), JuliaType::TypeOf(Box::new(JuliaType::Any)))],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            3,
            13,
            vec![(
                "x".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::Struct("LinRange{T}".to_string()))),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            4,
            14,
            vec![(
                "x".to_string(),
                JuliaType::Struct("Array{T, 1}".to_string()),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            5,
            15,
            vec![(
                "x".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                    "T".to_string(),
                    None,
                ))))),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            6,
            16,
            vec![(
                "x".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::MatrixOf(Box::new(JuliaType::TypeVar(
                    "T".to_string(),
                    None,
                ))))),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        let selected = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                JuliaType::Int64,
            ))))])
            .expect("Vector type object should dispatch");
        assert_eq!(selected.global_index, 15);
    }

    #[test]
    fn test_type_any_tuple_method_beats_generic_typevar_issue_4574() {
        let mut table = MethodTable::new("_array_undef_from_dims".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![
                (
                    "typ".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::Any)),
                ),
                ("dims".to_string(), JuliaType::Tuple),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![
                (
                    "typ".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
                ("dims".to_string(), JuliaType::Tuple),
            ],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        let selected = table
            .dispatch(&[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ])
            .expect("Type{Any}, tuple dims should dispatch");
        assert_eq!(selected.global_index, 10);

        let selected = table
            .dispatch(&[
                JuliaType::TypeOf(Box::new(JuliaType::Symbol)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ])
            .expect("Type{Symbol}, tuple dims should dispatch");
        assert_eq!(selected.global_index, 11);
    }

    #[test]
    fn test_pair_type_parametric_method_beats_bare_pair_issue_4636() {
        let mut table = MethodTable::new("_array_undef_from_dims".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![
                (
                    "typ".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair".to_string()))),
                ),
                ("dims".to_string(), JuliaType::Tuple),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![
                (
                    "typ".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair{K,V}".to_string()))),
                ),
                ("dims".to_string(), JuliaType::Tuple),
            ],
            ValueType::Any,
            None,
            false,
            vec![
                TypeParam::new("K".to_string()),
                TypeParam::new("V".to_string()),
            ],
            CoreType::Bottom,
            None,
            None,
        ));

        let selected = table
            .dispatch(&[
                JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair{Int64,Int8}".to_string()))),
                JuliaType::TupleOf(vec![JuliaType::Int64]),
            ])
            .expect("Pair{Int64,Int8} type object should dispatch");
        assert_eq!(selected.global_index, 11);
    }

    #[test]
    fn test_typevar_singleton_beats_bare_type_issue_4131() {
        let mut table = MethodTable::new("g".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![("T".to_string(), JuliaType::Type)],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![(
                "T".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            )],
            ValueType::I64,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        let any_method = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Any))])
            .expect("Any type object should dispatch");
        assert_eq!(any_method.global_index, 11);

        let int_method = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Int64))])
            .expect("Int64 type object should dispatch");
        assert_eq!(int_method.global_index, 11);
    }

    /// Test that the issue #1665 scenario works correctly:
    /// map(f::Function, A) should be preferred over map(f::Function, x::Int64)
    /// when the second argument has unknown type.
    #[test]
    fn test_issue_1665_map_dispatch_with_unknown_type() {
        let mut table = MethodTable::new("map".to_string());

        // Method 1: map(f::Function, A) - generic version
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![
                ("f".to_string(), JuliaType::Function),
                ("A".to_string(), JuliaType::Any),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // Method 2: map(f::Function, x::Int64) - scalar version
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![
                ("f".to_string(), JuliaType::Function),
                ("x".to_string(), JuliaType::Int64),
            ],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // When called with (Function, Any), should prefer map(f::Function, A)
        let result = table.dispatch(&[JuliaType::Function, JuliaType::Any]);
        assert!(result.is_ok(), "Dispatch should succeed");
        let method = result.unwrap();
        assert_eq!(
            *method.projected_param_julia_type(1),
            JuliaType::Any,
            "Should select map(f::Function, A) when second argument type is Any (Issue #1665)"
        );
    }

    #[test]
    fn function_singleton_matches_function_param_without_parent_links_issue_9741() {
        let mut table = MethodTable::new("plot".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![
                ("x".to_string(), JuliaType::Any),
                ("y".to_string(), JuliaType::Any),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![
                ("x".to_string(), JuliaType::Any),
                ("f".to_string(), JuliaType::Function),
            ],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        let method = table
            .dispatch(&[
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::Struct("typeof(cos)".to_string()),
            ])
            .expect("function singleton should dispatch");
        assert_eq!(
            method.global_index, 1,
            "typeof(f) singleton arguments must prefer ::Function over the Any fallback"
        );
    }

    /// Test that dispatch correctly resolves sibling abstract type methods
    /// using struct parent information (Issue #3144).
    ///
    /// Scenario:
    ///   abstract type Vehicle end
    ///   abstract type MotorVehicle <: Vehicle end
    ///   abstract type NonMotorVehicle <: Vehicle end
    ///   struct Car <: MotorVehicle ...
    ///
    ///   vehicle_type(::MotorVehicle) and vehicle_type(::NonMotorVehicle) are both registered.
    ///   Calling vehicle_type(Car) should select the MotorVehicle method, NOT report ambiguity.
    #[test]
    fn test_abstract_sibling_dispatch_uses_struct_parents_issue_3144() {
        let mut table = MethodTable::new("vehicle_type".to_string());

        // Method 1: vehicle_type(::MotorVehicle) — global_index 10
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "v".to_string(),
                JuliaType::AbstractUser("MotorVehicle".to_string(), Some("Vehicle".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // Method 2: vehicle_type(::NonMotorVehicle) — global_index 11
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![(
                "v".to_string(),
                JuliaType::AbstractUser("NonMotorVehicle".to_string(), Some("Vehicle".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // Without struct parent info, dispatch must not fabricate a match to
        // either sibling abstract type (Issue #5617).
        let no_parent_result = table.dispatch(&[JuliaType::Struct("Car".to_string())]);
        assert!(
            matches!(
                no_parent_result,
                Err(subset_julia_vm_types::types::DispatchError::NoMethodFound { .. })
            ),
            "Without struct_parents, dispatch should wait for explicit parent metadata"
        );

        // Add struct parent info: Car <: MotorVehicle, Bicycle <: NonMotorVehicle
        table.insert_parent_link_for_tests("Car", Some("MotorVehicle".to_string()));
        table.insert_parent_link_for_tests("Bicycle", Some("NonMotorVehicle".to_string()));
        // Also need abstract types in the map (they appear in the parent chain)
        table.insert_parent_link_for_tests("MotorVehicle", Some("Vehicle".to_string()));
        table.insert_parent_link_for_tests("NonMotorVehicle", Some("Vehicle".to_string()));

        // With struct parent info: Car -> dispatch to MotorVehicle method (global_index 10)
        let car_result = table.dispatch(&[JuliaType::Struct("Car".to_string())]);
        assert!(
            car_result.is_ok(),
            "With struct_parents, dispatch of Car should succeed, got: {:?}",
            car_result
        );
        assert_eq!(
            car_result.unwrap().global_index,
            10,
            "Car should dispatch to MotorVehicle method (global_index 10)"
        );

        // Bicycle -> dispatch to NonMotorVehicle method (global_index 11)
        let bike_result = table.dispatch(&[JuliaType::Struct("Bicycle".to_string())]);
        assert!(
            bike_result.is_ok(),
            "Dispatch of Bicycle should succeed, got: {:?}",
            bike_result
        );
        assert_eq!(
            bike_result.unwrap().global_index,
            11,
            "Bicycle should dispatch to NonMotorVehicle method (global_index 11)"
        );
    }

    #[test]
    fn parametric_abstract_parent_dispatch_uses_instantiated_parent_issue_9472() {
        let mut table = MethodTable::new("describe".to_string());
        table.set_shared_projection(Arc::new(projection_from_parent_links_with_params(&[
            ("Container", Some("Any"), &["T"]),
            ("IntBox", Some("Container{Int64}"), &[]),
            ("FloatBox", Some("Container{Float64}"), &[]),
        ])));

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "_".to_string(),
                JuliaType::AbstractUser("Container{Int64}".to_string(), Some("Any".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![(
                "_".to_string(),
                JuliaType::AbstractUser("Container{Float64}".to_string(), Some("Any".to_string())),
            )],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        let param =
            JuliaType::AbstractUser("Container{Int64}".to_string(), Some("Any".to_string()));
        let arg = JuliaType::Struct("IntBox".to_string());
        assert!(
            needs_struct_parent_fallback(&param, &arg, &[], table.projection()),
            "parametric abstract should need the struct-parent fallback"
        );
        assert!(
            julia_type_matches_with_struct_parents(&param, &arg, &[], table.projection()),
            "parametric abstract fallback should match the instantiated parent"
        );

        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("IntBox".to_string())])
                .expect("IntBox should match Container{Int64}")
                .global_index,
            10
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("FloatBox".to_string())])
                .expect("FloatBox should match Container{Float64}")
                .global_index,
            11
        );
    }

    #[test]
    fn abstract_range_typevar_method_matches_range_family_issue_10150() {
        let mut table = MethodTable::new("range_t".to_string());
        table.insert_parent_link_for_tests("UnitRange", Some("AbstractRange".to_string()));
        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "r".to_string(),
                JuliaType::Struct("AbstractRange{T}".to_string()),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("UnitRange{Int64}".to_string())])
                .expect("UnitRange{Int64} should match AbstractRange{T}")
                .global_index,
            10
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("StepRange{BigInt, Int64}".to_string())])
                .expect("StepRange{BigInt,Int64} should match AbstractRange{T}")
                .global_index,
            10
        );
        assert!(
            table
                .dispatch(&[JuliaType::Struct("LogRange{Float64}".to_string())])
                .is_err(),
            "LogRange is not an AbstractRange in sjulia's range family lattice"
        );
    }

    #[test]
    fn abstract_range_typevar_method_matches_without_parent_links_issue_10150() {
        let mut table = MethodTable::new("range_t".to_string());
        table.add_method(MethodSig::from_julia_projections(
            0,
            10,
            vec![(
                "r".to_string(),
                JuliaType::Struct("AbstractRange{T}".to_string()),
            )],
            ValueType::Any,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            None,
            None,
        ));

        assert!(!table.projection.has_parent_links());
        assert_eq!(
            table
                .methods
                .first()
                .expect("method should be registered")
                .projected_param_julia_types(),
            vec![JuliaType::Struct("AbstractRange{T}".to_string())]
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("UnitRange{Int64}".to_string())])
                .expect("UnitRange{Int64} should match AbstractRange{T} without parent links")
                .global_index,
            10
        );
        assert!(
            table
                .dispatch(&[JuliaType::Struct("LogRange{Float64}".to_string())])
                .is_err(),
            "LogRange must not use the AbstractRange hierarchy-free fallback"
        );
    }

    #[test]
    fn native_range_param_matches_parametric_range_struct_issue_10150() {
        let mut table = MethodTable::new("collect".to_string());
        table.add_method(MethodSig::from_julia_projections(
            0,
            10,
            vec![("r".to_string(), JuliaType::UnitRange)],
            ValueType::Any,
            None,
            false,
            vec![],
            None,
            None,
        ));
        table.add_method(MethodSig::from_julia_projections(
            1,
            11,
            vec![("r".to_string(), JuliaType::StepRange)],
            ValueType::Any,
            None,
            false,
            vec![],
            None,
            None,
        ));

        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("UnitRange{Int64}".to_string())])
                .expect("UnitRange{Int64} should match bare UnitRange")
                .global_index,
            10
        );
        assert_eq!(
            table
                .dispatch(&[JuliaType::Struct("StepRange{Int64, Int64}".to_string())])
                .expect("StepRange{Int64,Int64} should match bare StepRange")
                .global_index,
            11
        );
        assert!(
            table
                .dispatch(&[JuliaType::Struct("StepRangeLen{Float64}".to_string())])
                .is_err(),
            "StepRangeLen must not match bare StepRange"
        );
    }

    #[test]
    fn test_covariant_type_dispatch_uses_struct_parents_issue_3877() {
        let mut table = MethodTable::new("type_name".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![(
                "T".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
                    "_".to_string(),
                    Some("Animal".to_string()),
                ))),
            )],
            ValueType::Str,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![(
                "T".to_string(),
                JuliaType::TypeOf(Box::new(JuliaType::Struct("Dog".to_string()))),
            )],
            ValueType::Str,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        table.insert_parent_link_for_tests("Dog", Some("Animal".to_string()));
        table.insert_parent_link_for_tests("Cat", Some("Animal".to_string()));

        let dog = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "Dog".to_string(),
            )))])
            .expect("Dog should match exact Type{Dog}");
        assert_eq!(dog.global_index, 11);

        let cat = table
            .dispatch(&[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "Cat".to_string(),
            )))])
            .expect("Cat should match Type{<:Animal}");
        assert_eq!(cat.global_index, 10);
    }

    #[test]
    fn test_struct_parent_fallback_preserves_typevar_diagonal_rule_issue_3881() {
        let mut table = MethodTable::new("promote_type".to_string());

        table.add_method(MethodSig::for_tests(
            0,
            10,
            vec![
                (
                    "T".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
                (
                    "T".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
            ],
            ValueType::DataType,
            None,
            false,
            vec![TypeParam::new("T".to_string())],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            11,
            vec![
                (
                    "T".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                ),
                (
                    "S".to_string(),
                    JuliaType::TypeOf(Box::new(JuliaType::TypeVar("S".to_string(), None))),
                ),
            ],
            ValueType::DataType,
            None,
            false,
            vec![
                TypeParam::new("T".to_string()),
                TypeParam::new("S".to_string()),
            ],
            CoreType::Bottom,
            None,
            None,
        ));

        table.insert_parent_link_for_tests("Cat", Some("Animal".to_string()));

        let mixed = table
            .dispatch(&[
                JuliaType::TypeOf(Box::new(JuliaType::Float32)),
                JuliaType::TypeOf(Box::new(JuliaType::Bool)),
            ])
            .expect("mixed Type arguments should dispatch to the two-TypeVar method");
        assert_eq!(mixed.global_index, 11);
    }

    /// Test that dispatch cache returns the same result on second call (Issue #3361).
    #[test]
    fn test_dispatch_cache_hit() {
        let mut table = MethodTable::new("g".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // First call populates cache
        let r1 = table.dispatch(&[JuliaType::Int64]);
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap().global_index, 0);

        // Second call should hit cache and return the same result
        let r2 = table.dispatch(&[JuliaType::Int64]);
        assert!(r2.is_ok());
        assert_eq!(r2.unwrap().global_index, 0);

        // Verify cache is populated
        assert_eq!(table.dispatch_cache.borrow().len(), 1);
        assert!(
            table
                .dispatch_cache
                .borrow()
                .contains_key(&CoreType::Tuple(vec![CoreType::from(&JuliaType::Int64)])),
            "dispatch cache should be keyed by structured CoreType tuple"
        );
    }

    /// Test that dispatch cache is invalidated when a method is added (Issue #3361).
    #[test]
    fn test_dispatch_cache_invalidation() {
        let mut table = MethodTable::new("h".to_string());
        table.add_method(MethodSig::for_tests(
            0,
            0,
            vec![("x".to_string(), JuliaType::Any)],
            ValueType::Any,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));

        // Populate cache
        let _ = table.dispatch(&[JuliaType::Int64]);
        assert_eq!(table.dispatch_cache.borrow().len(), 1);

        // Add a more specific method — cache should be cleared
        table.add_method(MethodSig::for_tests(
            1,
            1,
            vec![("x".to_string(), JuliaType::Int64)],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            None,
            None,
        ));
        assert!(
            table.dispatch_cache.borrow().is_empty(),
            "Cache should be cleared after add_method"
        );

        // Now dispatch should find the more specific method
        let r = table.dispatch(&[JuliaType::Int64]);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().global_index, 1);
    }

    /// Issue #9197 S7: the typed Base-cache method-table key wraps a canonical
    /// generic-function name losslessly, orders lexicographically by that name
    /// (so a sort produces a deterministic, hash-seed-independent layout), and
    /// serializes transparently as its inner string (wire-compatible with the
    /// pre-S7 bare `String` key on the deserialize side).
    #[test]
    fn method_table_key_wraps_name_orders_and_serializes_transparently_issue_9197_s7() {
        let k = MethodTableKey::new("Base.log2");
        assert_eq!(k.as_str(), "Base.log2");
        assert_eq!(k.clone().into_string(), "Base.log2");
        assert_eq!(MethodTableKey::from("+".to_string()).as_str(), "+");

        // Deterministic ordering by canonical name.
        let mut keys: Vec<MethodTableKey> = ["Module.f", "+", "Base.log2", "Foo", "parent#nested"]
            .iter()
            .map(|s| MethodTableKey::new(*s))
            .collect();
        keys.sort();
        let sorted: Vec<&str> = keys.iter().map(MethodTableKey::as_str).collect();
        assert_eq!(
            sorted,
            ["+", "Base.log2", "Foo", "Module.f", "parent#nested"]
        );

        // Transparent bincode encoding: MethodTableKey and the bare String
        // produce byte-identical wire, so a v88 cache written with typed keys
        // decodes with the same section framing the pre-S7 String key used.
        use bincode::Options;
        let codec = || {
            bincode::DefaultOptions::new()
                .with_varint_encoding()
                .allow_trailing_bytes()
        };
        let via_key = codec()
            .serialize(&MethodTableKey::new("Base.log2"))
            .unwrap();
        let via_string = codec().serialize(&"Base.log2".to_string()).unwrap();
        assert_eq!(via_key, via_string);
        let round: MethodTableKey = codec().deserialize(&via_string).unwrap();
        assert_eq!(round, MethodTableKey::new("Base.log2"));
    }
}
