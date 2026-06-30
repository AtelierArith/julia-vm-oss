//! Shared structured Julia type core.
//!
//! This is the first migration point for Issues #3826 and #3829.  The goal is
//! not to replace every existing type representation in one step; instead this
//! module provides a loss-minimising semantic shape that compiler, VM-facing,
//! and AoT code can convert into before asking common questions.

use super::PrimitiveNumeric;
use crate::types::StructHierarchy;
use std::collections::HashMap;

mod convert;
mod intersect;
mod r#match;
mod registry;
mod repr;
mod subtype;

use self::r#match::{
    core_type_matches_pattern, core_type_matches_pattern_in, TypeVarBindingState, TypeVarVariance,
};
pub(crate) use convert::{core_type_to_julia_type, core_type_var_to_type_param};
use registry::{
    registered_instantiated_struct_parent_in, registered_struct_is_subtype_of_in,
    registered_struct_parent_in,
};
pub use registry::{
    registered_instantiated_struct_supertype_in, registered_nominal_subtype_decision_in,
    registered_struct_parent_existential_in, registered_struct_parent_family_decision_in,
};
pub use repr::{
    is_core_builtin_function_name, is_core_builtin_singleton_type_name, CoreAbstract,
    CorePrimitive, CoreType, CoreTypeVar, CoreValueParam, CORE_BUILTIN_FUNCTION_NAMES,
};

/// Cap for the [`CoreType::from_julia_name`] memoization cache (Issue #6846).
/// Far above the distinct-type-name count of any realistic program; only a
/// pathological name-synthesizing workload could approach it.
const FROM_JULIA_NAME_CACHE_CAP: usize = 100_000;

thread_local! {
    /// Thread-local memoization for [`CoreType::from_julia_name`] (Issue #6846).
    /// Keyed on the rendered type-name string; the parse is a pure function of
    /// that string, so a cache hit is always correct. Thread-local keeps it
    /// lock-free on the single-threaded VM hot path and isolated per worker.
    static FROM_JULIA_NAME_CACHE: std::cell::RefCell<HashMap<String, CoreType>> =
        std::cell::RefCell::new(HashMap::new());
}

impl CoreType {
    pub fn primitive_numeric(&self) -> Option<PrimitiveNumeric> {
        match self {
            Self::Primitive(p) => p.primitive_numeric(),
            _ => None,
        }
    }

    pub fn is_primitive_numeric(&self) -> bool {
        self.primitive_numeric().is_some()
    }

    /// Built-in concrete value types that existing dispatch bridges treat as
    /// primitive, exact-match candidates. This is broader than
    /// [`CoreType::primitive_numeric`] because it includes non-numeric leaf
    /// values such as `String` and `Char`, and boxed numeric leaf types such as
    /// `BigInt` / `BigFloat`.
    pub fn is_builtin_dispatch_primitive(&self) -> bool {
        matches!(
            self,
            Self::Primitive(
                CorePrimitive::Bool
                    | CorePrimitive::Int8
                    | CorePrimitive::Int16
                    | CorePrimitive::Int32
                    | CorePrimitive::Int64
                    | CorePrimitive::Int128
                    | CorePrimitive::UInt8
                    | CorePrimitive::UInt16
                    | CorePrimitive::UInt32
                    | CorePrimitive::UInt64
                    | CorePrimitive::UInt128
                    | CorePrimitive::Float16
                    | CorePrimitive::Float32
                    | CorePrimitive::Float64
                    | CorePrimitive::BigInt
                    | CorePrimitive::BigFloat
                    | CorePrimitive::String
                    | CorePrimitive::Char
            )
        )
    }

    pub fn is_builtin_abstract_numeric(&self) -> bool {
        matches!(
            self,
            Self::Abstract(
                CoreAbstract::Number
                    | CoreAbstract::Real
                    | CoreAbstract::Integer
                    | CoreAbstract::Signed
                    | CoreAbstract::Unsigned
                    | CoreAbstract::AbstractFloat
            )
        )
    }

    /// Numeric abstract supertypes that can accept narrow integer values and
    /// therefore must not force compile-time widening to `Int64`.
    pub fn is_builtin_abstract_integer_accepting(&self) -> bool {
        matches!(
            self,
            Self::Abstract(
                CoreAbstract::Number
                    | CoreAbstract::Real
                    | CoreAbstract::Integer
                    | CoreAbstract::Signed
                    | CoreAbstract::Unsigned
            )
        )
    }

    /// Existing compile-time dispatch uses "primitive" to mean exact
    /// primitive leaves plus abstract numeric supertypes that can safely defer
    /// runtime validation. Keep that policy centralized here.
    pub fn is_builtin_dispatch_primitive_or_abstract_numeric(&self) -> bool {
        self.is_builtin_dispatch_primitive() || self.is_builtin_abstract_numeric()
    }

    pub fn builtin_sizeof_bytes(&self) -> Option<usize> {
        match self {
            Self::Primitive(p) => p.builtin_sizeof_bytes(),
            _ => None,
        }
    }

    pub fn builtin_sizeof_bytes_for_julia_name(name: &str) -> Option<usize> {
        let normalized = match name {
            "Int" => crate::types::native_int_type_name(),
            "UInt" => crate::types::native_uint_type_name(),
            other => other,
        };
        Self::from_julia_name(normalized).builtin_sizeof_bytes()
    }

    pub fn is_builtin_bits_type(&self) -> bool {
        matches!(
            self,
            Self::Primitive(
                CorePrimitive::Bool
                    | CorePrimitive::Int8
                    | CorePrimitive::Int16
                    | CorePrimitive::Int32
                    | CorePrimitive::Int64
                    | CorePrimitive::Int128
                    | CorePrimitive::UInt8
                    | CorePrimitive::UInt16
                    | CorePrimitive::UInt32
                    | CorePrimitive::UInt64
                    | CorePrimitive::UInt128
                    | CorePrimitive::Float16
                    | CorePrimitive::Float32
                    | CorePrimitive::Float64
                    | CorePrimitive::Char
                    | CorePrimitive::Nothing
                    | CorePrimitive::Missing
            )
        )
    }

    pub fn is_builtin_bits_type_for_julia_name(name: &str) -> bool {
        matches!(name, "Int" | "UInt") || Self::from_julia_name(name).is_builtin_bits_type()
    }

    pub fn is_builtin_primitive_datatype(&self) -> bool {
        matches!(
            self,
            Self::Primitive(
                CorePrimitive::Bool
                    | CorePrimitive::Int8
                    | CorePrimitive::Int16
                    | CorePrimitive::Int32
                    | CorePrimitive::Int64
                    | CorePrimitive::Int128
                    | CorePrimitive::UInt8
                    | CorePrimitive::UInt16
                    | CorePrimitive::UInt32
                    | CorePrimitive::UInt64
                    | CorePrimitive::UInt128
                    | CorePrimitive::Float16
                    | CorePrimitive::Float32
                    | CorePrimitive::Float64
                    | CorePrimitive::Char
            )
        )
    }

    pub fn is_builtin_primitive_datatype_for_julia_name(name: &str) -> bool {
        if matches!(name, "Int" | "UInt") {
            return false;
        }
        Self::from_julia_name(name).is_builtin_primitive_datatype()
    }

    pub fn is_builtin_abstract_datatype(&self) -> bool {
        // A *parametric* abstract container (`AbstractVector{Int64}`,
        // `AbstractDict{String,Int}`, ...) is retained as a `Struct` so its
        // invariant element parameter survives subtyping (Issues #5047 / #5563),
        // yet it is still an abstract DataType upstream (`isabstracttype` is
        // true). Recognize that form here so reflection stays in parity (the
        // bare, parameter-free spelling stays `Abstract`). Issue #5564.
        if let Self::Struct { name, .. } = self {
            if is_parametric_container_abstract_name(base_type_name(name)) {
                return true;
            }
        }
        !matches!(self, Self::Abstract(CoreAbstract::DataType))
            && matches!(self, Self::Any | Self::Abstract(_) | Self::TypeOf(_))
    }

    pub fn is_builtin_abstract_datatype_for_julia_name(name: &str) -> bool {
        Self::from_julia_name(name).is_builtin_abstract_datatype()
    }

    pub fn is_builtin_concrete_datatype(&self) -> bool {
        match self {
            Self::Primitive(_) | Self::Abstract(CoreAbstract::DataType) => true,
            Self::Module(_) => true,
            Self::Struct { name, params } => builtin_struct_datatype_is_concrete(name, params),
            Self::Tuple(elements) => {
                !elements.is_empty() && elements.iter().all(type_parameter_is_fully_specified)
            }
            Self::Named(name) => matches!(base_type_name(name), "Module" | "Binding"),
            _ => false,
        }
    }

    pub fn is_builtin_concrete_datatype_for_julia_name(name: &str) -> bool {
        Self::from_julia_name(name).is_builtin_concrete_datatype()
    }

    /// Returns whether this type denotes a concrete Julia type in the compiler's
    /// user-facing type projection.
    ///
    /// This is intentionally broader than [`CoreType::is_builtin_concrete_datatype`]:
    /// the latter answers reflection facts for known builtin `DataType`s, while
    /// this predicate is used by compiler dispatch checks that also see
    /// user-defined structs through `JuliaType::Struct`.
    pub fn is_concrete_type(&self) -> bool {
        match self {
            Self::Primitive(_) | Self::Abstract(CoreAbstract::DataType) | Self::Module(_) => true,
            Self::Struct { params, .. } => params.iter().all(type_parameter_is_fully_specified),
            Self::Tuple(elements) => elements.iter().all(type_parameter_is_fully_specified),
            Self::NamedTuple(fields) => fields
                .iter()
                .all(|(_, ty)| type_parameter_is_fully_specified(ty)),
            Self::TypeOf(_) => true,
            Self::Named(name) => !is_type_variable_name(name),
            Self::Any
            | Self::Bottom
            | Self::Abstract(_)
            | Self::AbstractUser { .. }
            | Self::Vararg(_)
            | Self::VarargLen { .. }
            | Self::Union(_)
            | Self::TypeVar(_)
            | Self::Value(_)
            | Self::UnionAll { .. } => false,
        }
    }

    pub fn is_builtin_mutable_datatype(&self) -> bool {
        match self {
            Self::Primitive(
                CorePrimitive::BigInt | CorePrimitive::String | CorePrimitive::Symbol,
            )
            | Self::Abstract(CoreAbstract::DataType) => true,
            Self::Module(_) => true,
            Self::Struct { name, .. } => {
                matches!(
                    base_type_name(name),
                    "Array" | "Vector" | "Matrix" | "Dict" | "IOBuffer" | "Expr"
                )
            }
            Self::Named(name) => matches!(base_type_name(name), "Module"),
            _ => false,
        }
    }

    pub fn is_builtin_mutable_datatype_for_julia_name(name: &str) -> bool {
        Self::from_julia_name(name).is_builtin_mutable_datatype()
    }

    pub fn is_builtin_struct_datatype(&self) -> bool {
        match self {
            Self::Primitive(
                CorePrimitive::BigInt
                | CorePrimitive::BigFloat
                | CorePrimitive::String
                | CorePrimitive::Symbol
                | CorePrimitive::Nothing
                | CorePrimitive::Missing,
            )
            | Self::Abstract(CoreAbstract::DataType)
            | Self::Module(_)
            | Self::Struct { .. }
            | Self::Tuple(_)
            | Self::NamedTuple(_) => true,
            Self::Named(name) => matches!(base_type_name(name), "Module" | "UnionAll" | "Binding"),
            _ => false,
        }
    }

    pub fn is_builtin_struct_datatype_for_julia_name(name: &str) -> bool {
        Self::from_julia_name(name).is_builtin_struct_datatype()
    }

    /// Canonicalize a method `core_signature` — a `Tuple` optionally wrapped in
    /// one `UnionAll` per `where` type parameter — for **redefinition dedup**.
    ///
    /// In covariant (value) position a `where` type variable that appears
    /// *exactly once* and only as a whole top-level parameter is equivalent to
    /// its upper bound, because `Tuple{T} where T<:Number == Tuple{Number}`
    /// upstream. Such variables are substituted with their bound (`Any` when
    /// unbounded) and their `UnionAll` is dropped, so e.g.
    /// `h(x::T) where {T<:Number}` and `h(x::Number)` canonicalize to the same
    /// `Tuple{Number}` and the later definition replaces the earlier one
    /// (Issue #5383). Type variables used diagonally (multiple occurrences,
    /// e.g. `f(x::T, y::T)`) or nested inside an invariant parameter
    /// (`Vector{T}`) are preserved untouched, so genuinely distinct methods are
    /// never merged.
    pub(crate) fn canonicalize_signature_for_dedup(&self) -> Self {
        // Peel the `where` UnionAll layers (outer-to-inner == type-param order).
        let mut peeled_vars: Vec<CoreTypeVar> = Vec::new();
        let mut body = self;
        while let Self::UnionAll { var, body: inner } = body {
            peeled_vars.push(var.clone());
            body = inner.as_ref();
        }
        if peeled_vars.is_empty() {
            return self.clone();
        }
        let Self::Tuple(elements) = body else {
            return self.clone();
        };

        // The authoritative upper bound of a `where` variable lives on the peeled
        // `UnionAll` var; the body element may carry `None` when the parameter was
        // spelled as a bare `Struct("T")`, so resolve bounds from the peeled vars.
        let peeled_bounds: HashMap<&str, Self> = peeled_vars
            .iter()
            .map(|v| {
                (
                    v.name.as_str(),
                    v.upper_bound.as_deref().cloned().unwrap_or(Self::Any),
                )
            })
            .collect();

        // Count occurrences of each peeled variable across the whole body.
        let mut counts: HashMap<String, usize> = HashMap::new();
        count_core_typevar_names(body, &mut counts);

        // Substitute single-occurrence top-level type variables with their bound.
        let collapsed = Self::Tuple(
            elements
                .iter()
                .map(|elem| match elem {
                    Self::TypeVar(var) if counts.get(&var.name).copied() == Some(1) => {
                        peeled_bounds
                            .get(var.name.as_str())
                            .cloned()
                            .unwrap_or_else(|| elem.clone())
                    }
                    other => other.clone(),
                })
                .collect(),
        );

        // Re-wrap only the variables that still occur in the collapsed body,
        // preserving the original nesting order (first type param == outermost).
        let mut remaining: HashMap<String, usize> = HashMap::new();
        count_core_typevar_names(&collapsed, &mut remaining);
        let mut result = collapsed;
        for var in peeled_vars.into_iter().rev() {
            if remaining.contains_key(&var.name) {
                result = Self::UnionAll {
                    var,
                    body: Box::new(result),
                };
            }
        }
        result
    }

    /// Conservative join for the shared core.
    pub fn typejoin(&self, other: &Self) -> Self {
        if self.is_subtype_of(other) {
            return other.clone();
        }
        if other.is_subtype_of(self) {
            return self.clone();
        }
        if let (Self::Tuple(elements), Self::Tuple(other_elements)) = (self, other) {
            if elements.len() == other_elements.len()
                && !elements
                    .iter()
                    .any(|t| matches!(t, Self::Vararg(_) | Self::VarargLen { .. }))
                && !other_elements
                    .iter()
                    .any(|t| matches!(t, Self::Vararg(_) | Self::VarargLen { .. }))
            {
                return Self::Tuple(
                    elements
                        .iter()
                        .zip(other_elements.iter())
                        .map(|(a, b)| a.typejoin(b))
                        .collect(),
                );
            }
        }
        normalize_union(vec![self.clone(), other.clone()])
    }

    /// Direct built-in supertype name for context-free reflection. Use
    /// `direct_builtin_supertype_name_with_hierarchy` when pure-Julia struct
    /// parents are available.
    pub fn direct_builtin_supertype_name(&self) -> Option<&'static str> {
        match self {
            Self::Any => Some("Any"),
            Self::Bottom => Some("Any"),
            Self::Primitive(primitive) => primitive_direct_supertype_name(primitive),
            Self::Abstract(abstract_ty) => abstract_direct_supertype_name(abstract_ty),
            Self::Struct { name, params } => struct_direct_supertype_name(name, !params.is_empty()),
            Self::Tuple(_) => Some("Tuple"),
            Self::NamedTuple(_) => Some("NamedTuple"),
            Self::TypeOf(_) => Some("Type"),
            Self::Union(_) | Self::UnionAll { .. } => Some("Any"),
            Self::AbstractUser { .. }
            | Self::Vararg(_)
            | Self::VarargLen { .. }
            | Self::TypeVar(_)
            | Self::Value(_)
            | Self::Module(_)
            | Self::Named(_) => None,
        }
    }

    pub fn direct_builtin_supertype_name_with_hierarchy(
        &self,
        hierarchy: &StructHierarchy,
    ) -> Option<&'static str> {
        match self {
            Self::Any => Some("Any"),
            Self::Bottom => Some("Any"),
            Self::Primitive(primitive) => primitive_direct_supertype_name(primitive),
            Self::Abstract(abstract_ty) => abstract_direct_supertype_name(abstract_ty),
            Self::Struct { name, params } => {
                struct_direct_supertype_name_in(hierarchy, name, !params.is_empty())
            }
            Self::Tuple(_) => Some("Tuple"),
            Self::NamedTuple(_) => Some("NamedTuple"),
            Self::TypeOf(_) => Some("Type"),
            Self::Union(_) | Self::UnionAll { .. } => Some("Any"),
            Self::AbstractUser { .. }
            | Self::Vararg(_)
            | Self::VarargLen { .. }
            | Self::TypeVar(_)
            | Self::Value(_)
            | Self::Module(_)
            | Self::Named(_) => None,
        }
    }

    pub fn direct_builtin_supertype_name_for_julia_name(name: &str) -> Option<&'static str> {
        Self::from_julia_name(name).direct_builtin_supertype_name()
    }

    pub fn direct_builtin_supertype_name_for_julia_name_with_hierarchy(
        name: &str,
        hierarchy: &StructHierarchy,
    ) -> Option<&'static str> {
        Self::from_julia_name(name).direct_builtin_supertype_name_with_hierarchy(hierarchy)
    }

    pub fn builtin_type_name(&self) -> Option<&'static str> {
        match self {
            Self::Any => Some("Any"),
            Self::Bottom => Some("Union{}"),
            Self::Primitive(primitive) => primitive_type_name(primitive),
            Self::Abstract(abstract_ty) => abstract_type_name(abstract_ty),
            Self::Struct { name, .. } => struct_type_name(name),
            Self::Tuple(_) => Some("Tuple"),
            Self::NamedTuple(_) => Some("NamedTuple"),
            Self::TypeOf(_) => Some("Type"),
            Self::Union(_) => Some("Union"),
            Self::UnionAll { .. } => Some("UnionAll"),
            Self::AbstractUser { .. }
            | Self::Vararg(_)
            | Self::VarargLen { .. }
            | Self::TypeVar(_)
            | Self::Value(_)
            | Self::Module(_)
            | Self::Named(_) => None,
        }
    }

    /// Structural bare family name for the nominal variants that carry a name.
    ///
    /// Returns the family name with the module prefix and any parametric
    /// `{...}` arguments stripped (e.g. `Base.Iterators.Zip{...}` -> `Zip`),
    /// read directly from the structured representation rather than rendering
    /// the type back to a Julia name string and re-parsing it. This is the
    /// `core_signature`-structured replacement for the CallDynamic family
    /// fallback's old `to_julia_name()` -> `extract_base_type` ->
    /// `strip_module_prefix` round-trip (Issue #6593). Non-nominal variants
    /// (`Any`, `Tuple`, `Union`, ...) return `None`.
    pub fn nominal_family_name(&self) -> Option<&str> {
        match self {
            Self::Struct { name, .. }
            | Self::AbstractUser { name, .. }
            | Self::Named(name)
            | Self::Module(name) => Some(nominal_family_name(name)),
            _ => None,
        }
    }

    pub fn to_julia_name(&self) -> String {
        match self {
            Self::Any => "Any".to_string(),
            Self::Bottom => "Union{}".to_string(),
            Self::Primitive(primitive) => {
                primitive_type_name(primitive).unwrap_or("Any").to_string()
            }
            Self::Abstract(abstract_ty) => {
                abstract_type_name(abstract_ty).unwrap_or("Any").to_string()
            }
            Self::AbstractUser { name, .. } | Self::Module(name) | Self::Named(name) => {
                name.clone()
            }
            Self::Struct { name, params } => format_parametric_name(name, params),
            Self::Tuple(elements) => format_parametric_name("Tuple", elements),
            Self::Vararg(inner) => format!("Vararg{{{}}}", inner.to_julia_name()),
            Self::VarargLen { element, len } => {
                format!(
                    "Vararg{{{}, {}}}",
                    element.to_julia_name(),
                    len.to_julia_name()
                )
            }
            Self::NamedTuple(fields) => format_concrete_named_tuple_name(fields),
            Self::Union(types) => format_parametric_name("Union", types),
            Self::TypeOf(inner) => format!("Type{{{}}}", inner.to_julia_name()),
            Self::TypeVar(var) => var.name.clone(),
            Self::Value(value) => value.to_julia_name(),
            Self::UnionAll { var, body } => {
                let var_name = if let Some(bound) = &var.upper_bound {
                    format!("{}<:{}", var.name, bound.to_julia_name())
                } else {
                    var.name.clone()
                };
                format!("{} where {}", body.to_julia_name(), var_name)
            }
        }
    }

    pub fn type_parameters(&self) -> Vec<Self> {
        match self {
            Self::Struct { params, .. } | Self::Tuple(params) | Self::Union(params) => {
                params.clone()
            }
            Self::TypeOf(inner) | Self::Vararg(inner) => vec![inner.as_ref().clone()],
            Self::VarargLen { element, len } => {
                vec![element.as_ref().clone(), len.as_ref().clone()]
            }
            Self::NamedTuple(fields) => fields.iter().map(|(_, ty)| ty.clone()).collect(),
            Self::UnionAll { var, .. } => vec![Self::TypeVar(var.clone())],
            _ => vec![],
        }
    }

    pub fn builtin_field_metadata(&self) -> Option<Vec<(&'static str, Self)>> {
        let name = self.to_julia_name();
        // Base.RefValue{T} / Ref{T} have a single field `x` (Issue #5130).
        // Matched on the bare base name (module prefix and type params stripped) so
        // parametric instantiations (`RefValue{Int64}`) resolve, keeping
        // fieldnames/fieldcount and the generic `show` fallback consistent with
        // upstream `Base.RefValue{T}(value)`.
        let bare = base_type_name(name.split('{').next().unwrap_or(&name));
        if matches!(bare, "RefValue" | "Ref") {
            return Some(vec![("x", Self::Any)]);
        }
        Some(match name.as_str() {
            "LineNumberNode" => vec![
                ("line", Self::Primitive(CorePrimitive::Int64)),
                (
                    "file",
                    Self::Union(vec![
                        Self::Primitive(CorePrimitive::Nothing),
                        Self::Primitive(CorePrimitive::Symbol),
                    ]),
                ),
            ],
            "Expr" => vec![
                ("head", Self::Primitive(CorePrimitive::Symbol)),
                (
                    "args",
                    Self::Struct {
                        name: "Vector".to_string(),
                        params: vec![Self::Any],
                    },
                ),
            ],
            "QuoteNode" => vec![("value", Self::Any)],
            "GlobalRef" => vec![
                ("mod", Self::Module("Module".to_string())),
                ("name", Self::Primitive(CorePrimitive::Symbol)),
                (
                    "binding",
                    Self::Struct {
                        name: "Core.Binding".to_string(),
                        params: vec![],
                    },
                ),
            ],
            _ => return None,
        })
    }

    pub fn builtin_supertype_chain_names(&self) -> Option<Vec<&'static str>> {
        let mut chain = Vec::new();
        let mut current = self.clone();
        let first = current.builtin_type_name()?;
        chain.push(first);

        for _ in 0..64 {
            let parent = current.direct_builtin_supertype_name()?;
            if chain.last().copied() != Some(parent) {
                chain.push(parent);
            }
            if parent == "Any" {
                break;
            }
            current = Self::from_julia_name(parent);
        }

        Some(chain)
    }

    pub fn direct_builtin_subtype_names(&self) -> Option<Vec<&'static str>> {
        let parent_name = self.builtin_type_name()?;
        Some(direct_builtin_subtype_names(parent_name))
    }

    pub fn direct_builtin_subtype_names_for_julia_name(name: &str) -> Option<Vec<&'static str>> {
        Self::from_julia_name(name).direct_builtin_subtype_names()
    }

    /// Parse a rendered Julia type name (e.g. `"Array{Float64, 1}"`) into a
    /// structured [`CoreType`].
    ///
    /// Issue #6846: this is a **pure** function of `name` (the only external
    /// input, `native_int_type_name()`, is a build-time constant), and it is on
    /// the hot dynamic-dispatch path — every runtime `<:` / candidate match of
    /// an `Array{T,N}` wrapper (whose dispatch identity is its rendered
    /// `struct_name` string) re-parses the same name through the O(n)
    /// `split_trailing_where` scanner, `parse_parametric_name` (called *twice*
    /// per invocation, once via `parse_named_tuple_type_name`), and
    /// `parse_core_value_param`'s `format!`. Memoizing the parse on a
    /// thread-local string→`CoreType` cache replaces that whole chain with a
    /// single clone for repeated names (`norm([x,y])` over a 10000-point grid
    /// hammers `"Array{Float64, 1}"`). Recursive sub-parses re-enter this cached
    /// entry, so nested params (`Float64`, `1`) are memoized too.
    pub fn from_julia_name(name: &str) -> Self {
        FROM_JULIA_NAME_CACHE.with(|cache| {
            if let Some(cached) = cache.borrow().get(name) {
                return cached.clone();
            }
            let parsed = Self::from_julia_name_uncached(name);
            let mut cache = cache.borrow_mut();
            // Safety valve against unbounded growth for pathological programs
            // that synthesize an ever-growing set of distinct type-name strings
            // (e.g. deep `Box{Box{...}}` metaprogramming). Normal programs see a
            // tiny, bounded set of names, so this never trips in practice.
            if cache.len() >= FROM_JULIA_NAME_CACHE_CAP {
                cache.clear();
            }
            cache.insert(name.to_string(), parsed.clone());
            parsed
        })
    }

    fn from_julia_name_uncached(name: &str) -> Self {
        if let Some(named_tuple) = parse_named_tuple_type_name(name) {
            return named_tuple;
        }

        // A value-position `where` expression (#5569) renders its UnionAll value
        // via `JuliaType::name()` as `Body where V` (or, for several bound
        // variables, the right-nested chain `Body where V2 where V1`). Re-parse
        // that surface syntax into a `CoreType::UnionAll` so the exists-right
        // subtype solver (`matches_unionall_pattern`, with the bounds + diagonal
        // logic) actually fires when the runtime `<:` routes a `where`-value
        // through `from_julia_name`. Without this the `where` clause was silently
        // dropped and the bound vars behaved like `Any`, so e.g.
        // `Tuple{Int,String} <: (Tuple{T,T} where T)` wrongly returned true
        // (Issue #5047). The rightmost top-level `where` binds the outermost
        // variable, matching upstream `Body where {A,B} == (Body where B) where A`.
        if let Some((body, var)) = split_trailing_where(name) {
            if let Some(vars) = parse_where_var_list(var) {
                let mut result = Self::from_julia_name(body);
                for var in vars.into_iter().rev() {
                    result = Self::UnionAll {
                        var,
                        body: Box::new(result),
                    };
                }
                return result;
            }
            return Self::UnionAll {
                var: parse_where_var(var),
                body: Box::new(Self::from_julia_name(body)),
            };
        }
        match name {
            "Any" => Self::Any,
            "Union{}" => Self::Bottom,
            "Bool" => Self::Primitive(CorePrimitive::Bool),
            "Int8" => Self::Primitive(CorePrimitive::Int8),
            "Int16" => Self::Primitive(CorePrimitive::Int16),
            "Int32" => Self::Primitive(CorePrimitive::Int32),
            "Int64" => Self::Primitive(CorePrimitive::Int64),
            "Int128" => Self::Primitive(CorePrimitive::Int128),
            // `Int` / `UInt` are platform-word aliases (`Int64` / `UInt64` on
            // 64-bit targets, `Int32` / `UInt32` on 32-bit targets). They appear
            // in rendered type names — notably nested parametric params
            // (`Box{Box{Int}}`, where only the outer level may normalize), and in
            // user-written `where` bound clauses (`T<:Int`). Resolve them here so
            // Core subtype and dispatch paths see the native concrete type
            // instead of an opaque `Named("Int")` / `Named("UInt")`.
            "Int" => Self::from_julia_name(crate::types::native_int_type_name()),
            "UInt" => Self::from_julia_name(crate::types::native_uint_type_name()),
            "UInt8" => Self::Primitive(CorePrimitive::UInt8),
            "UInt16" => Self::Primitive(CorePrimitive::UInt16),
            "UInt32" => Self::Primitive(CorePrimitive::UInt32),
            "UInt64" => Self::Primitive(CorePrimitive::UInt64),
            "UInt128" => Self::Primitive(CorePrimitive::UInt128),
            "Float16" => Self::Primitive(CorePrimitive::Float16),
            "Float32" => Self::Primitive(CorePrimitive::Float32),
            "Float64" => Self::Primitive(CorePrimitive::Float64),
            "BigInt" => Self::Primitive(CorePrimitive::BigInt),
            "BigFloat" => Self::Primitive(CorePrimitive::BigFloat),
            "String" => Self::Primitive(CorePrimitive::String),
            "Char" => Self::Primitive(CorePrimitive::Char),
            "Symbol" => Self::Primitive(CorePrimitive::Symbol),
            "Nothing" => Self::Primitive(CorePrimitive::Nothing),
            "Missing" => Self::Primitive(CorePrimitive::Missing),
            "Number" => Self::Abstract(CoreAbstract::Number),
            "Real" => Self::Abstract(CoreAbstract::Real),
            "Integer" => Self::Abstract(CoreAbstract::Integer),
            "Signed" => Self::Abstract(CoreAbstract::Signed),
            "Unsigned" => Self::Abstract(CoreAbstract::Unsigned),
            "AbstractFloat" => Self::Abstract(CoreAbstract::AbstractFloat),
            "AbstractString" => Self::Abstract(CoreAbstract::AbstractString),
            "AbstractChar" => Self::Abstract(CoreAbstract::AbstractChar),
            "AbstractArray" => Self::Abstract(CoreAbstract::AbstractArray),
            "AbstractVector" => Self::Abstract(CoreAbstract::AbstractVector),
            "AbstractMatrix" => Self::Abstract(CoreAbstract::AbstractMatrix),
            "DenseArray" => Self::Abstract(CoreAbstract::DenseArray),
            "AbstractDict" => Self::Abstract(CoreAbstract::AbstractDict),
            "AbstractSet" => Self::Abstract(CoreAbstract::AbstractSet),
            "AbstractRange" => Self::Abstract(CoreAbstract::AbstractRange),
            "AbstractUnitRange" => Self::Abstract(CoreAbstract::AbstractUnitRange),
            "Function" => Self::Abstract(CoreAbstract::Function),
            // Issue #5129: `Core.Builtin` (and the bare `Builtin` alias) is the
            // abstract supertype of genuine built-in functions, `<: Function`.
            "Core.Builtin" | "Builtin" => Self::Abstract(CoreAbstract::Builtin),
            "IO" => Self::Abstract(CoreAbstract::IO),
            "Type" => Self::Abstract(CoreAbstract::Type),
            "DataType" => Self::Abstract(CoreAbstract::DataType),
            _ => {
                if let Some((var_name, upper_bound)) = split_top_level_subtype_bound(name) {
                    let var_name = var_name.trim();
                    let upper_bound = upper_bound.trim();
                    return Self::TypeVar(CoreTypeVar {
                        name: if var_name.is_empty() {
                            "_".to_string()
                        } else {
                            var_name.to_string()
                        },
                        lower_bound: None,
                        upper_bound: (!upper_bound.is_empty())
                            .then(|| Box::new(Self::from_julia_name(upper_bound))),
                    });
                }

                let (base, params) = parse_parametric_name(name);
                if params.is_empty() {
                    // An explicitly empty parametric form like `Tuple{}` (the type
                    // of the empty tuple `()`, where `typeof(()) === Tuple{}`) must
                    // stay in the `Tuple` family so dispatch can match it against a
                    // bare `::Tuple` parameter. Without this, `from_julia_name`
                    // fell through to `Named("Tuple{}")` and `show(io, ())`
                    // mis-dispatched to the generic struct fallback, printing
                    // `Tuple{}()` instead of `()` (Issue #4739 / #4737 family).
                    if base == "Tuple" && name != base {
                        Self::Tuple(vec![])
                    } else if let Some(value) = parse_core_value_param(name) {
                        Self::Value(value)
                    } else if is_callable_singleton_type_name(name) {
                        // Issue #5129: function-singleton type names such as
                        // `typeof(<:)` / `typeof(>:)` embed the `<:`/`>:`
                        // operator and must be recognized as callable singletons
                        // BEFORE the bounded-typevar `split_once("<:")` branch,
                        // which would otherwise mis-parse `typeof(<:)` as a
                        // `TypeVar { name: "typeof(", upper_bound: ")" }`.
                        Self::Struct {
                            name: base_type_name(name).to_string(),
                            params: vec![],
                        }
                    } else if is_type_variable_name(name) {
                        Self::TypeVar(CoreTypeVar {
                            name: name.to_string(),
                            lower_bound: None,
                            upper_bound: None,
                        })
                    } else if is_known_struct_family(name) || is_callable_singleton_type_name(name)
                    {
                        Self::Struct {
                            name: base_type_name(name).to_string(),
                            params: vec![],
                        }
                    } else {
                        Self::Named(name.to_string())
                    }
                } else {
                    let parsed_params: Vec<Self> =
                        params.iter().map(|p| Self::from_julia_name(p)).collect();
                    match base {
                        "Union" => normalize_union(parsed_params),
                        "Tuple" => Self::Tuple(parsed_params),
                        "Vararg" => {
                            let mut iter = parsed_params.into_iter();
                            let element = iter.next().unwrap_or(Self::Any);
                            if let Some(len) = iter.next() {
                                Self::VarargLen {
                                    element: Box::new(element),
                                    len: Box::new(len),
                                }
                            } else {
                                Self::Vararg(Box::new(element))
                            }
                        }
                        "NTuple" if parsed_params.len() == 1 => {
                            let len = parsed_params.into_iter().next().unwrap_or(Self::Any);
                            Self::Tuple(vec![Self::VarargLen {
                                element: Box::new(Self::Any),
                                len: Box::new(len),
                            }])
                        }
                        "NTuple" if parsed_params.len() == 2 => {
                            let mut iter = parsed_params.into_iter();
                            let len = iter.next().unwrap_or(Self::Any);
                            let element = iter.next().unwrap_or(Self::Any);
                            Self::Tuple(vec![Self::VarargLen {
                                element: Box::new(element),
                                len: Box::new(len),
                            }])
                        }
                        "Type" => {
                            let inner = parsed_params.into_iter().next().unwrap_or(Self::Any);
                            Self::TypeOf(Box::new(inner))
                        }
                        // A parametric *abstract container* name such as
                        // `AbstractVector{Int64}` or `AbstractArray{T,N}` must
                        // RETAIN its invariant element/dimension parameter so
                        // subtyping can enforce it: upstream containers are
                        // invariant, so `Vector{Float64} <: AbstractVector{Int64}`
                        // is false. Routing these through `Self::Abstract` silently
                        // dropped the parameter, making the relation trivially true
                        // (Issue #5047). The bare, parameter-free spellings (e.g.
                        // `AbstractVector`) are handled in the no-param arm above and
                        // keep their covariant `Self::Abstract` representation.
                        base if is_parametric_container_abstract_name(base_type_name(base)) => {
                            Self::Struct {
                                name: base_type_name(base).to_string(),
                                params: parsed_params,
                            }
                        }
                        _ => core_abstract_from_name(base_type_name(base)).map_or_else(
                            || Self::Struct {
                                name: base_type_name(base).to_string(),
                                params: parsed_params,
                            },
                            Self::Abstract,
                        ),
                    }
                }
            }
        }
    }
}

fn primitive_type_name(primitive: &CorePrimitive) -> Option<&'static str> {
    use CorePrimitive as P;
    Some(match primitive {
        P::Bool => "Bool",
        P::Int8 => "Int8",
        P::Int16 => "Int16",
        P::Int32 => "Int32",
        P::Int64 => "Int64",
        P::Int128 => "Int128",
        P::UInt8 => "UInt8",
        P::UInt16 => "UInt16",
        P::UInt32 => "UInt32",
        P::UInt64 => "UInt64",
        P::UInt128 => "UInt128",
        P::Float16 => "Float16",
        P::Float32 => "Float32",
        P::Float64 => "Float64",
        P::BigInt => "BigInt",
        P::BigFloat => "BigFloat",
        P::String => "String",
        P::Char => "Char",
        P::Symbol => "Symbol",
        P::Nothing => "Nothing",
        P::Missing => "Missing",
    })
}

fn abstract_type_name(abstract_ty: &CoreAbstract) -> Option<&'static str> {
    use CoreAbstract as A;
    Some(match abstract_ty {
        A::Number => "Number",
        A::Real => "Real",
        A::Integer => "Integer",
        A::Signed => "Signed",
        A::Unsigned => "Unsigned",
        A::AbstractFloat => "AbstractFloat",
        A::AbstractString => "AbstractString",
        A::AbstractChar => "AbstractChar",
        A::AbstractArray => "AbstractArray",
        A::AbstractVector => "AbstractVector",
        A::AbstractMatrix => "AbstractMatrix",
        A::DenseArray => "DenseArray",
        A::AbstractDict => "AbstractDict",
        A::AbstractSet => "AbstractSet",
        A::AbstractRange => "AbstractRange",
        A::AbstractUnitRange => "AbstractUnitRange",
        A::Function => "Function",
        A::Builtin => "Core.Builtin",
        A::IO => "IO",
        A::Type => "Type",
        A::DataType => "DataType",
    })
}

fn struct_type_name(name: &str) -> Option<&'static str> {
    Some(match base_type_name(name) {
        "Array" => "Array",
        "Vector" => "Vector",
        "Matrix" => "Matrix",
        "BitArray" => "BitArray",
        "BitVector" => "BitVector",
        "BitMatrix" => "BitMatrix",
        "SubArray" => "SubArray",
        "ReshapedArray" => "ReshapedArray",
        "Tuple" => "Tuple",
        "NamedTuple" => "NamedTuple",
        "Dict" => "Dict",
        "Set" => "Set",
        "Complex" => "Complex",
        "Rational" => "Rational",
        "Irrational" => "Irrational",
        "UnitRange" => "UnitRange",
        "StepRange" => "StepRange",
        "StepRangeLen" => "StepRangeLen",
        "LinRange" => "LinRange",
        "LogRange" => "LogRange",
        "OneTo" => "OneTo",
        "IOBuffer" => "IOBuffer",
        "Pair" => "Pair",
        "Pairs" => "Pairs",
        "Fix1" => "Fix1",
        "Fix2" => "Fix2",
        "Generator" => "Generator",
        "Memory" => "Memory",
        "MemoryRef" => "MemoryRef",
        "VersionNumber" => "VersionNumber",
        _ => return None,
    })
}

fn format_parametric_name(base: &str, params: &[CoreType]) -> String {
    if params.is_empty() {
        base.to_string()
    } else {
        let param_names: Vec<String> = params.iter().map(CoreType::to_julia_name).collect();
        format!("{}{{{}}}", base, param_names.join(", "))
    }
}

fn format_concrete_named_tuple_name(fields: &[(String, CoreType)]) -> String {
    let field_names = fields
        .iter()
        .map(|(name, ty)| {
            if matches!(ty, CoreType::Any) {
                name.clone()
            } else {
                format!("{}::{}", name, ty.to_julia_name())
            }
        })
        .collect::<Vec<_>>();
    format!("@NamedTuple{{{}}}", field_names.join(", "))
}

fn builtin_struct_datatype_is_concrete(name: &str, params: &[CoreType]) -> bool {
    let fully_specified = params.iter().all(type_parameter_is_fully_specified);
    match base_type_name(name) {
        "IOBuffer" | "VersionNumber" | "Expr" | "QuoteNode" | "LineNumberNode" | "GlobalRef"
        | "Binding" => params.is_empty(),
        "BitVector" | "BitMatrix" => params.is_empty(),
        "Complex" | "Rational" | "Irrational" | "Vector" | "Matrix" | "Set" | "UnitRange"
        | "OneTo" | "StepRangeLen" | "LinRange" | "LogRange" | "Memory" | "MemoryRef" => {
            params.len() == 1 && fully_specified
        }
        "BitArray" => params.len() == 1 && fully_specified,
        "Array" | "StepRange" | "Dict" | "Pair" | "NamedTuple" => {
            params.len() == 2 && fully_specified
        }
        "Fix1" | "Fix2" => params.len() == 2 && fully_specified,
        "ReshapedArray" => params.len() == 4 && fully_specified,
        "SubArray" => params.len() == 5 && fully_specified,
        "Tuple" => !params.is_empty() && fully_specified,
        _ => false,
    }
}

/// Whether `inner` (the `T` of a `Type{T}`) denotes a nominal Julia `DataType`,
/// i.e. whether `Type{T} <: DataType` holds. Concrete and abstract nominal types
/// (`Int`, `Integer`, `Any`, user structs) and fully-applied parametric types
/// (`Vector{Int}`, `Vector{Real}`) qualify; a `Union`, a bare parametric (a
/// `UnionAll`, represented as a parameter-free `Struct`), a free `TypeVar` (the
/// rigid variable a `Type{<:Bound}` leaves behind), and other non-nominal shapes
/// do not (Issue #5048).
fn core_type_inner_is_datatype(inner: &CoreType) -> bool {
    match inner {
        CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Any
        | CoreType::AbstractUser { .. } => true,
        // User-defined nominal types lower to `Named`; a free type-variable name
        // (e.g. the `T` of `Type{<:Real}`) is not a DataType.
        CoreType::Named(name) => !is_type_variable_name(name),
        // An applied parametric type (`Vector{Int}`, `Array{Int,2}`) is a DataType
        // when every parameter is fully specified; a bare parametric carries no
        // parameters and is a `UnionAll`.
        CoreType::Struct { params, .. } => {
            !params.is_empty() && params.iter().all(type_parameter_is_fully_specified)
        }
        CoreType::Tuple(elements) => {
            !elements.is_empty() && elements.iter().all(type_parameter_is_fully_specified)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, ty)| type_parameter_is_fully_specified(ty)),
        CoreType::TypeVar(_)
        | CoreType::Union(_)
        | CoreType::UnionAll { .. }
        | CoreType::TypeOf(_)
        | CoreType::Value(_)
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. }
        | CoreType::Module(_)
        | CoreType::Bottom => false,
    }
}

fn type_parameter_is_fully_specified(param: &CoreType) -> bool {
    match param {
        CoreType::TypeVar(_)
        | CoreType::UnionAll { .. }
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. } => false,
        CoreType::Struct { params, .. } => params.iter().all(type_parameter_is_fully_specified),
        CoreType::Tuple(elements) | CoreType::Union(elements) => {
            elements.iter().all(type_parameter_is_fully_specified)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, ty)| type_parameter_is_fully_specified(ty)),
        CoreType::TypeOf(inner) => type_parameter_is_fully_specified(inner),
        CoreType::Value(_) => true,
        _ => true,
    }
}

fn primitive_direct_supertype_name(primitive: &CorePrimitive) -> Option<&'static str> {
    use CorePrimitive as P;
    Some(match primitive {
        P::Bool => "Integer",
        P::Int8 | P::Int16 | P::Int32 | P::Int64 | P::Int128 | P::BigInt => "Signed",
        P::UInt8 | P::UInt16 | P::UInt32 | P::UInt64 | P::UInt128 => "Unsigned",
        P::Float16 | P::Float32 | P::Float64 | P::BigFloat => "AbstractFloat",
        P::String => "AbstractString",
        P::Char => "AbstractChar",
        P::Symbol | P::Nothing | P::Missing => "Any",
    })
}

fn abstract_direct_supertype_name(abstract_ty: &CoreAbstract) -> Option<&'static str> {
    use CoreAbstract as A;
    Some(match abstract_ty {
        A::Number
        | A::AbstractString
        | A::AbstractChar
        | A::AbstractArray
        | A::AbstractDict
        | A::AbstractSet
        | A::Function
        | A::IO
        | A::Type => "Any",
        A::Real => "Number",
        A::Integer => "Real",
        A::Signed | A::Unsigned => "Integer",
        A::AbstractFloat => "Real",
        A::AbstractVector | A::AbstractMatrix | A::DenseArray => "AbstractArray",
        A::AbstractRange => "AbstractVector",
        A::AbstractUnitRange => "AbstractRange",
        A::DataType => "Type",
        // Issue #5129: `Core.Builtin <: Function` (julia/base/boot.jl).
        A::Builtin => "Function",
    })
}

/// Substitute every `TypeVar` node named `var.name` inside `body` with the
/// bound-carrying `var` itself, returning the rewritten type. This realizes the
/// forall-LEFT "fresh RIGID variable confined to its bounds" step: the body
/// produced by `from_julia_name` spells the bound occurrence as an UNBOUNDED
/// typevar (the `where` clause keeps the bound only on the peeled `var`), so
/// rewriting the body lets the declared bound flow into the subtype check.
///
/// A nested `UnionAll` that re-binds the SAME name shadows the outer variable,
/// so substitution does not descend into such a subtree (its body is left
/// untouched). Distinct nested `where` variables are unaffected and recurse
/// normally. The function is a structural clone-and-rewrite; when `var.name`
/// does not occur, the result is value-equal to the original body (Issue #5047).
fn substitute_typevar_bound(body: &CoreType, var: &CoreTypeVar) -> CoreType {
    match body {
        CoreType::TypeVar(v) if v.name == var.name => CoreType::TypeVar(var.clone()),
        CoreType::TypeVar(_)
        | CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => body.clone(),
        CoreType::Struct { name, params } => CoreType::Struct {
            name: name.clone(),
            params: params
                .iter()
                .map(|p| substitute_typevar_bound(p, var))
                .collect(),
        },
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|e| substitute_typevar_bound(e, var))
                .collect(),
        ),
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|t| substitute_typevar_bound(t, var))
                .collect(),
        ),
        CoreType::Vararg(inner) => CoreType::Vararg(Box::new(substitute_typevar_bound(inner, var))),
        CoreType::VarargLen { element, len } => CoreType::VarargLen {
            element: Box::new(substitute_typevar_bound(element, var)),
            len: Box::new(substitute_typevar_bound(len, var)),
        },
        CoreType::TypeOf(inner) => CoreType::TypeOf(Box::new(substitute_typevar_bound(inner, var))),
        CoreType::NamedTuple(fields) => CoreType::NamedTuple(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_typevar_bound(ty, var)))
                .collect(),
        ),
        // A nested `where` that re-binds the same name shadows `var`; leave its
        // body untouched. Otherwise rewrite inside the inner body.
        CoreType::UnionAll {
            var: inner_var,
            body: inner_body,
        } => {
            if inner_var.name == var.name {
                body.clone()
            } else {
                CoreType::UnionAll {
                    var: inner_var.clone(),
                    body: Box::new(substitute_typevar_bound(inner_body, var)),
                }
            }
        }
    }
}

fn substitute_typevars(body: &CoreType, substitutions: &HashMap<String, CoreType>) -> CoreType {
    match body {
        CoreType::TypeVar(var) => substitutions
            .get(&var.name)
            .cloned()
            .unwrap_or_else(|| body.clone()),
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => body.clone(),
        CoreType::Struct { name, params } => CoreType::Struct {
            name: name.clone(),
            params: params
                .iter()
                .map(|p| substitute_typevars(p, substitutions))
                .collect(),
        },
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|e| substitute_typevars(e, substitutions))
                .collect(),
        ),
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|ty| substitute_typevars(ty, substitutions))
                .collect(),
        ),
        CoreType::Vararg(inner) => {
            CoreType::Vararg(Box::new(substitute_typevars(inner, substitutions)))
        }
        CoreType::VarargLen { element, len } => CoreType::VarargLen {
            element: Box::new(substitute_typevars(element, substitutions)),
            len: Box::new(substitute_typevars(len, substitutions)),
        },
        CoreType::TypeOf(inner) => {
            CoreType::TypeOf(Box::new(substitute_typevars(inner, substitutions)))
        }
        CoreType::NamedTuple(fields) => CoreType::NamedTuple(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_typevars(ty, substitutions)))
                .collect(),
        ),
        CoreType::UnionAll { var, body } => {
            let mut inner_substitutions = substitutions.clone();
            inner_substitutions.remove(&var.name);
            CoreType::UnionAll {
                var: var.clone(),
                body: Box::new(substitute_typevars(body, &inner_substitutions)),
            }
        }
    }
}

/// Accumulate, per name, how many times each `TypeVar` appears as a node within
/// `core` (recursing into struct/tuple/union params, varargs, named-tuple
/// fields, and the upper bounds of other type variables). Used by
/// [`CoreType::canonicalize_signature_for_dedup`] to detect single-occurrence
/// `where` variables (Issue #5383).
fn count_core_typevar_names(core: &CoreType, counts: &mut HashMap<String, usize>) {
    match core {
        CoreType::TypeVar(var) => {
            *counts.entry(var.name.clone()).or_insert(0) += 1;
            if let Some(ub) = var.upper_bound.as_deref() {
                count_core_typevar_names(ub, counts);
            }
        }
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            for p in params {
                count_core_typevar_names(p, counts);
            }
        }
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => {
            count_core_typevar_names(inner, counts);
        }
        CoreType::VarargLen { element, len } => {
            count_core_typevar_names(element, counts);
            count_core_typevar_names(len, counts);
        }
        CoreType::NamedTuple(fields) => {
            for (_, ty) in fields {
                count_core_typevar_names(ty, counts);
            }
        }
        CoreType::UnionAll { var, body } => {
            if let Some(ub) = var.upper_bound.as_deref() {
                count_core_typevar_names(ub, counts);
            }
            count_core_typevar_names(body, counts);
        }
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => {}
    }
}

/// Built-in abstract `child <: parent` by walking the built-in supertype chain
/// (e.g. `Real <: Number <: Any`). Both are built-in abstract type names.
fn builtin_abstract_name_is_subtype_of(child: &str, parent: &str) -> bool {
    let mut current = child.to_string();
    for _ in 0..32 {
        if current == parent {
            return true;
        }
        match CoreType::direct_builtin_supertype_name_for_julia_name(&current) {
            Some(next) => current = next.to_string(),
            None => return false,
        }
    }
    false
}

/// The `&'static str` form of a built-in abstract type name, or `None`.
fn builtin_abstract_static_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "Number" => "Number",
        "Real" => "Real",
        "Integer" => "Integer",
        "Signed" => "Signed",
        "Unsigned" => "Unsigned",
        "AbstractFloat" => "AbstractFloat",
        "Any" => "Any",
        _ => return None,
    })
}

fn builtin_struct_direct_supertype_name(name: &str, is_parametric: bool) -> Option<&'static str> {
    Some(match (base_type_name(name), is_parametric) {
        ("Array", true) => "Array",
        ("Vector", true) => "Vector",
        ("Matrix", true) => "Matrix",
        ("BitArray", true) => "BitArray",
        ("BitVector", true) => "BitVector",
        ("BitMatrix", true) => "BitMatrix",
        ("Tuple", true) => "Tuple",
        ("Array" | "Vector" | "Matrix", false) => "DenseArray",
        ("BitVector", false) => "AbstractVector",
        ("BitMatrix", false) => "AbstractMatrix",
        ("BitArray", false) => "AbstractArray",
        ("SubArray" | "ReshapedArray", _) => "AbstractArray",
        ("Tuple", false) => "Any",
        ("NamedTuple", true) => "NamedTuple",
        ("NamedTuple", false) => "Any",
        ("Dict", _) => "AbstractDict",
        ("Set", _) => "AbstractSet",
        ("UnitRange", true) | ("StepRange", true) => "AbstractRange",
        ("UnitRange" | "OneTo", false) => "AbstractUnitRange",
        ("StepRange" | "StepRangeLen" | "LinRange", false) => "AbstractRange",
        ("OneTo", true) => "AbstractUnitRange",
        ("StepRangeLen" | "LinRange", true) => "AbstractRange",
        ("LogRange", _) => "AbstractVector",
        ("IOBuffer", _) => "IO",
        ("Pair" | "Pairs" | "Generator" | "VersionNumber", _) => "Any",
        _ => return None,
    })
}

fn struct_direct_supertype_name(name: &str, is_parametric: bool) -> Option<&'static str> {
    // Context-free subtype checks only know the built-in container/range
    // hierarchy. Pure-Julia struct parents such as `Complex <: Number` are
    // resolved by `struct_direct_supertype_name_in` with an explicit hierarchy.
    builtin_struct_direct_supertype_name(name, is_parametric)
}

fn struct_direct_supertype_name_in(
    hierarchy: &StructHierarchy,
    name: &str,
    is_parametric: bool,
) -> Option<&'static str> {
    builtin_struct_direct_supertype_name(name, is_parametric).or_else(|| {
        registered_struct_parent_in(hierarchy, name)
            .as_deref()
            .and_then(builtin_abstract_static_name)
    })
}

fn direct_builtin_subtype_names(parent_name: &str) -> Vec<&'static str> {
    match base_type_name(parent_name) {
        "Any" => vec![
            "Number",
            "AbstractString",
            "AbstractChar",
            "AbstractArray",
            "AbstractDict",
            "AbstractSet",
            "Function",
            "IO",
            "Type",
            "Tuple",
            "NamedTuple",
            "Symbol",
            "Nothing",
            "Missing",
            "Pair",
            "Pairs",
            "Generator",
            "VersionNumber",
        ],
        "Number" => vec!["Real", "Complex"],
        "Real" => vec!["AbstractFloat", "Integer", "Rational"],
        "Integer" => vec!["Bool", "Signed", "Unsigned"],
        "Signed" => vec!["Int8", "Int16", "Int32", "Int64", "Int128", "BigInt"],
        "Unsigned" => vec!["UInt8", "UInt16", "UInt32", "UInt64", "UInt128"],
        "AbstractFloat" => vec!["Float16", "Float32", "Float64", "BigFloat"],
        "AbstractString" => vec!["String"],
        "AbstractChar" => vec!["Char"],
        "AbstractArray" => vec!["DenseArray", "AbstractVector", "AbstractMatrix", "BitArray"],
        "AbstractVector" => vec!["AbstractRange", "LogRange", "BitVector"],
        "AbstractMatrix" => vec!["BitMatrix"],
        "DenseArray" => vec!["Array", "Vector", "Matrix"],
        "AbstractDict" => vec!["Dict"],
        "AbstractSet" => vec!["Set"],
        "AbstractRange" => vec!["AbstractUnitRange", "StepRange", "StepRangeLen", "LinRange"],
        "AbstractUnitRange" => vec!["UnitRange", "OneTo"],
        "IO" => vec!["IOBuffer"],
        "Type" => vec!["DataType"],
        _ => vec![],
    }
}

fn core_abstract_from_name(name: &str) -> Option<CoreAbstract> {
    Some(match base_type_name(name) {
        "Number" => CoreAbstract::Number,
        "Real" => CoreAbstract::Real,
        "Integer" => CoreAbstract::Integer,
        "Signed" => CoreAbstract::Signed,
        "Unsigned" => CoreAbstract::Unsigned,
        "AbstractFloat" => CoreAbstract::AbstractFloat,
        "AbstractString" => CoreAbstract::AbstractString,
        "AbstractChar" => CoreAbstract::AbstractChar,
        "AbstractArray" => CoreAbstract::AbstractArray,
        "AbstractVector" => CoreAbstract::AbstractVector,
        "AbstractMatrix" => CoreAbstract::AbstractMatrix,
        "DenseArray" => CoreAbstract::DenseArray,
        "AbstractDict" => CoreAbstract::AbstractDict,
        "AbstractSet" => CoreAbstract::AbstractSet,
        "AbstractRange" => CoreAbstract::AbstractRange,
        "AbstractUnitRange" => CoreAbstract::AbstractUnitRange,
        "Function" => CoreAbstract::Function,
        "Core.Builtin" | "Builtin" => CoreAbstract::Builtin,
        "IO" => CoreAbstract::IO,
        "Type" => CoreAbstract::Type,
        "DataType" => CoreAbstract::DataType,
        _ => return None,
    })
}

fn primitive_is_subtype_of_abstract(primitive: &CorePrimitive, abstract_ty: &CoreAbstract) -> bool {
    use CoreAbstract as A;
    use CorePrimitive as P;
    match abstract_ty {
        A::Number => matches!(
            primitive,
            P::Bool
                | P::Int8
                | P::Int16
                | P::Int32
                | P::Int64
                | P::Int128
                | P::BigInt
                | P::UInt8
                | P::UInt16
                | P::UInt32
                | P::UInt64
                | P::UInt128
                | P::Float16
                | P::Float32
                | P::Float64
                | P::BigFloat
        ),
        A::Real => matches!(
            primitive,
            P::Bool
                | P::Int8
                | P::Int16
                | P::Int32
                | P::Int64
                | P::Int128
                | P::BigInt
                | P::UInt8
                | P::UInt16
                | P::UInt32
                | P::UInt64
                | P::UInt128
                | P::Float16
                | P::Float32
                | P::Float64
                | P::BigFloat
        ),
        A::Integer => matches!(
            primitive,
            P::Bool
                | P::Int8
                | P::Int16
                | P::Int32
                | P::Int64
                | P::Int128
                | P::BigInt
                | P::UInt8
                | P::UInt16
                | P::UInt32
                | P::UInt64
                | P::UInt128
        ),
        A::Signed => matches!(
            primitive,
            P::Int8 | P::Int16 | P::Int32 | P::Int64 | P::Int128 | P::BigInt
        ),
        A::Unsigned => matches!(
            primitive,
            P::UInt8 | P::UInt16 | P::UInt32 | P::UInt64 | P::UInt128
        ),
        A::AbstractFloat => matches!(
            primitive,
            P::Float16 | P::Float32 | P::Float64 | P::BigFloat
        ),
        A::AbstractString => matches!(primitive, P::String),
        A::AbstractChar => matches!(primitive, P::Char),
        A::AbstractArray
        | A::AbstractVector
        | A::AbstractMatrix
        | A::DenseArray
        | A::AbstractDict
        | A::AbstractSet
        | A::AbstractRange
        | A::AbstractUnitRange
        | A::Function
        | A::Builtin
        | A::IO
        | A::Type
        | A::DataType => false,
    }
}

fn abstract_is_subtype_of(left: &CoreAbstract, right: &CoreAbstract) -> bool {
    if left == right {
        return true;
    }

    let mut current = left.clone();
    for _ in 0..64 {
        let Some(parent_name) = abstract_direct_supertype_name(&current) else {
            return false;
        };
        let parent = CoreType::from_julia_name(parent_name);
        let CoreType::Abstract(parent_abstract) = parent else {
            return false;
        };
        if &parent_abstract == right {
            return true;
        }
        current = parent_abstract;
    }

    false
}

/// Decide `Type{A} <: Type{B}` (Issue #5068).
///
/// `Type{T}` is invariant in `T` (a `DataType` parameter), so for a *concrete*
/// `B` the relation reduces to `A === B` (i.e. `A <: B && B <: A`). Only the
/// covariant spelling `Type{<:B}` — represented as `Type{TypeVar(_, <:B)}` —
/// reduces to `A <: B`, matching upstream `jl_type_type` / the manual's
/// "Type{T}" section. An unbounded `TypeVar` (`Type{T} where T` ≡ `Type`)
/// accepts any type `A`.
fn core_type_is_subtype_with_lookup(
    left: &CoreType,
    right: &CoreType,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    match hierarchy {
        Some(hierarchy) => left.is_subtype_of_with_hierarchy(right, hierarchy),
        None => left.is_subtype_of(right),
    }
}

fn core_type_matches_pattern_with_lookup(
    actual: &CoreType,
    pattern: &CoreType,
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    variance: TypeVarVariance,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    match hierarchy {
        Some(hierarchy) => {
            core_type_matches_pattern_in(hierarchy, actual, pattern, scope, bindings, variance)
        }
        None => core_type_matches_pattern(actual, pattern, scope, bindings, variance),
    }
}

fn core_typeof_inner_subtype(inner: &CoreType, other_inner: &CoreType) -> bool {
    core_typeof_inner_subtype_with_lookup(inner, other_inner, None)
}

fn core_typeof_inner_subtype_in(
    hierarchy: &StructHierarchy,
    inner: &CoreType,
    other_inner: &CoreType,
) -> bool {
    core_typeof_inner_subtype_with_lookup(inner, other_inner, Some(hierarchy))
}

fn core_typeof_inner_subtype_with_lookup(
    inner: &CoreType,
    other_inner: &CoreType,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    match other_inner {
        CoreType::TypeVar(var) => var
            .upper_bound
            .as_deref()
            .is_none_or(|ub| core_type_is_subtype_with_lookup(inner, ub, hierarchy)),
        _ => {
            core_type_is_subtype_with_lookup(inner, other_inner, hierarchy)
                && core_type_is_subtype_with_lookup(other_inner, inner, hierarchy)
        }
    }
}

fn struct_is_subtype_of_abstract(
    name: &str,
    params: &[CoreType],
    abstract_ty: &CoreAbstract,
) -> bool {
    struct_is_subtype_of_abstract_with_lookup(name, params, abstract_ty, None)
}

fn struct_is_subtype_of_abstract_in(
    hierarchy: &StructHierarchy,
    name: &str,
    params: &[CoreType],
    abstract_ty: &CoreAbstract,
) -> bool {
    struct_is_subtype_of_abstract_with_lookup(name, params, abstract_ty, Some(hierarchy))
}

fn registered_struct_is_subtype_of_with_lookup(
    name: &str,
    target: &str,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    match hierarchy {
        Some(hierarchy) => registered_struct_is_subtype_of_in(hierarchy, name, target),
        None => false,
    }
}

fn registered_instantiated_struct_parent_with_lookup(
    name: &str,
    params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> Option<CoreType> {
    match hierarchy {
        Some(hierarchy) => registered_instantiated_struct_parent_in(hierarchy, name, params),
        None => None,
    }
}

fn struct_is_subtype_of_abstract_with_lookup(
    name: &str,
    params: &[CoreType],
    abstract_ty: &CoreAbstract,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    use CoreAbstract as A;
    match abstract_ty {
        // Issue #5157: derived from the supplied struct hierarchy
        // (Complex/Rational and user `struct S <: Real/Integer/...`), not
        // hardcoded. Covers the built-in numeric abstract hierarchy
        // transitively (e.g. Rational <: Real <: Number).
        A::Number => registered_struct_is_subtype_of_with_lookup(name, "Number", hierarchy),
        A::Real => registered_struct_is_subtype_of_with_lookup(name, "Real", hierarchy),
        A::Integer => registered_struct_is_subtype_of_with_lookup(name, "Integer", hierarchy),
        A::Signed => registered_struct_is_subtype_of_with_lookup(name, "Signed", hierarchy),
        A::Unsigned => registered_struct_is_subtype_of_with_lookup(name, "Unsigned", hierarchy),
        A::AbstractFloat => {
            registered_struct_is_subtype_of_with_lookup(name, "AbstractFloat", hierarchy)
        }
        // Built-in array-family names match directly; a user struct whose
        // declared parent chain reaches the built-in array abstract is resolved
        // by walking the (instantiated) parent chain to its array-family
        // ancestor, mirroring the numeric arms above and the parameterized
        // `AbstractArray{T}` path through `struct_params_are_subtype_with_lookup`
        // (Issue #7787; companion to #7728).
        A::AbstractArray => {
            array_family_dim(name).is_some()
                || user_struct_array_ancestor(name, params, hierarchy).is_some()
        }
        A::DenseArray => {
            // `DenseArray` is more specific than `AbstractArray`, so a user type
            // only qualifies when its array-family ancestor is itself a (non
            // wrapper) `DenseArray` or more concrete. `AbsContainer <:
            // AbstractArray{...}` therefore stays NOT `<: DenseArray`.
            let direct = array_family_dim(name).is_some()
                && !array_family_is_wrapper(name)
                && array_family_abstractness(name) <= array_family_abstractness("DenseArray");
            direct
                || user_struct_array_ancestor(name, params, hierarchy).is_some_and(
                    |(ancestor, _)| {
                        !array_family_is_wrapper(&ancestor)
                            && array_family_abstractness(&ancestor)
                                <= array_family_abstractness("DenseArray")
                    },
                )
        }
        A::AbstractVector => {
            array_family_struct_has_abstract_rank(name, params, 1)
                || user_struct_array_ancestor_has_rank(name, params, 1, hierarchy)
        }
        A::AbstractMatrix => {
            array_family_struct_has_abstract_rank(name, params, 2)
                || user_struct_array_ancestor_has_rank(name, params, 2, hierarchy)
        }
        A::AbstractDict => {
            matches!(base_type_name(name), "Dict")
                || registered_struct_is_subtype_of_with_lookup(name, "AbstractDict", hierarchy)
        }
        A::AbstractSet => matches!(base_type_name(name), "Set"),
        // Route the range-family names through the same directional name
        // lattice used for struct-vs-struct range subtyping, so parametric
        // *abstract* spellings (`AbstractUnitRange{Int64}`, `AbstractRange{T}`
        // — represented as `Struct` to keep their invariant element parameter)
        // also resolve: `AbstractUnitRange{Int64} <: AbstractRange` is true
        // upstream, while `LogRange <: AbstractRange` stays false (Issue #5921).
        A::AbstractRange => range_family_name_subtype_allowed(name, "AbstractRange"),
        A::AbstractUnitRange => range_family_name_subtype_allowed(name, "AbstractUnitRange"),
        A::Function => is_callable_singleton_type_name(name),
        // Issue #5129: only built-in function singletons are `<: Core.Builtin`.
        A::Builtin => is_core_builtin_singleton_type_name(name),
        A::IO => matches!(base_type_name(name), "IOBuffer"),
        _ => false,
    }
}

fn array_family_struct_has_abstract_rank(name: &str, params: &[CoreType], rank: i64) -> bool {
    if array_family_dim(name).is_none()
        || array_family_abstractness(name) > array_family_abstractness("AbstractArray")
    {
        return false;
    }
    let (_, actual_rank) = array_family_element_and_rank(name, params);
    actual_rank == Some(rank)
}

/// Walk the *instantiated* declared-parent chain of a user struct (substituting
/// the struct's actual parameters into each parent template, exactly like the
/// parameterized `struct_params_are_subtype_with_lookup` path) until it reaches
/// an array-family ancestor, returning that ancestor's base name and parameters.
///
/// This lets the bare, parameter-free array-abstract arms recognize a user type
/// whose chain reaches `AbstractArray{T,N}` — e.g.
/// `abstract type AbsContainer{T} <: AbstractArray{T,2} end;
///  struct MyArr{T} <: AbsContainer{T} ... end` gives
/// `MyArr{Float64} <: AbstractArray` (Issue #7787). `name` is the user struct
/// name (its own family name is NOT array-family; built-in array names are
/// handled directly by the callers, so this returns `None` for them to avoid a
/// redundant walk). Returns `None` when there is no hierarchy, the chain has no
/// array-family ancestor, or it exceeds the depth guard.
fn user_struct_array_ancestor(
    name: &str,
    params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> Option<(String, Vec<CoreType>)> {
    if array_family_dim(name).is_some() {
        return None;
    }
    let mut current_name = name.to_string();
    let mut current_params = params.to_vec();
    for _ in 0..64 {
        let parent = registered_instantiated_struct_parent_with_lookup(
            &current_name,
            &current_params,
            hierarchy,
        )?;
        let (parent_name, parent_params) = match parent {
            CoreType::Struct {
                name: pname,
                params: pparams,
            } => (pname, pparams),
            // A bare (parameter-free) built-in array abstract such as
            // `AbstractArray` parses to `CoreType::Abstract`; map it back to its
            // family name so the rank/abstractness checks below still apply.
            CoreType::Abstract(ref ab) => match abstract_array_family_name(ab) {
                Some(family) => (family.to_string(), Vec::new()),
                None => return None,
            },
            _ => return None,
        };
        if array_family_dim(base_type_name(&parent_name)).is_some() {
            return Some((base_type_name(&parent_name).to_string(), parent_params));
        }
        current_name = parent_name;
        current_params = parent_params;
    }
    None
}

/// The built-in array-family family name backing a `CoreAbstract`, or `None` for
/// non-array abstracts.
fn abstract_array_family_name(abstract_ty: &CoreAbstract) -> Option<&'static str> {
    Some(match abstract_ty {
        CoreAbstract::AbstractArray => "AbstractArray",
        CoreAbstract::AbstractVector => "AbstractVector",
        CoreAbstract::AbstractMatrix => "AbstractMatrix",
        CoreAbstract::DenseArray => "DenseArray",
        _ => return None,
    })
}

/// Whether a user struct's array-family ancestor (see `user_struct_array_ancestor`)
/// has the given fixed `rank` (1 for `AbstractVector`, 2 for `AbstractMatrix`).
/// The ancestor must be `<: AbstractArray` and its rank must be statically known
/// and equal to `rank`.
fn user_struct_array_ancestor_has_rank(
    name: &str,
    params: &[CoreType],
    rank: i64,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    let Some((ancestor, ancestor_params)) = user_struct_array_ancestor(name, params, hierarchy)
    else {
        return false;
    };
    if array_family_abstractness(&ancestor) > array_family_abstractness("AbstractArray") {
        return false;
    }
    let (_, actual_rank) = array_family_element_and_rank(&ancestor, &ancestor_params);
    actual_rank == Some(rank)
}

fn is_callable_singleton_type_name(name: &str) -> bool {
    name.starts_with("typeof(") && name.ends_with(')')
}

fn struct_family_subtype(name: &str, other: &str) -> bool {
    matches!(
        (base_type_name(name), base_type_name(other)),
        ("Vector" | "Matrix", "Array")
    )
}

fn struct_params_are_subtype(
    name: &str,
    params: &[CoreType],
    other_name: &str,
    other_params: &[CoreType],
) -> bool {
    struct_params_are_subtype_with_lookup(name, params, other_name, other_params, None)
}

fn struct_params_are_subtype_in(
    hierarchy: &StructHierarchy,
    name: &str,
    params: &[CoreType],
    other_name: &str,
    other_params: &[CoreType],
) -> bool {
    struct_params_are_subtype_with_lookup(name, params, other_name, other_params, Some(hierarchy))
}

fn struct_params_are_subtype_with_lookup(
    name: &str,
    params: &[CoreType],
    other_name: &str,
    other_params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    let name_base = base_type_name(name);
    let other_base = base_type_name(other_name);

    // Array-family subtyping (concrete `Array`/`Vector`/`Matrix` and abstract
    // `AbstractArray`/`AbstractVector`/`AbstractMatrix`/`DenseArray`) is invariant
    // in both element type and rank, so route every array-family pair through one
    // helper (Issue #5047). This subsumes the former `Array{T,N} <: Array{T}`
    // special case and the `Vector/Matrix/Array` cross-relations, and correctly
    // handles same-name forms whose parameter arity differs, e.g.
    // `AbstractArray{Int,2} <: AbstractArray{Int}` (true) — which the generic
    // exact-equality path below would reject.
    if array_family_dim(name_base).is_some() && array_family_dim(other_base).is_some() {
        // The array-family name lattice is directional (element/rank aside).
        // Concrete `Array`/`Vector`/`Matrix` pass through the dense layer, while
        // wrapper arrays (`SubArray`, `ReshapedArray`) jump directly to
        // `AbstractArray{T,N}` and are not `DenseArray`s (Issue #5615).
        if !array_family_name_subtype_allowed(name_base, other_base) {
            return false;
        }
        if other_params.is_empty() {
            // A bare supertype still constrains rank when its NAME pins one:
            // `Vector` is `Array{T,1} where T` and `Matrix` is `Array{T,2} where
            // T`, so a rank-1 type is NOT a subtype of a bare `Matrix` (and a
            // rank-2 type is not a subtype of a bare `Vector`). Only the
            // rank-free names (`Array`/`AbstractArray`/`DenseArray`/`BitArray`)
            // match any rank when written bare. Previously this shortcut returned
            // `true` for every array-family pair, so e.g. `Vector <: Matrix`,
            // `Array{Int64,1} <: Matrix`, and `[1,2,3] isa Matrix` were all
            // spuriously true (Issue #6814).
            return match array_family_dim(other_base) {
                Some(Some(required_rank)) => {
                    array_family_element_and_rank(name_base, params).1 == Some(required_rank)
                }
                _ => true,
            };
        }
        return array_family_invariant_subtype_with_lookup(
            name_base,
            params,
            other_base,
            other_params,
            hierarchy,
        );
    }

    if name_base == other_base {
        return other_params.is_empty()
            || (params.len() >= other_params.len()
                && params
                    .iter()
                    .zip(other_params.iter())
                    .all(|(actual, expected)| {
                        struct_param_matches_pattern_with_lookup(actual, expected, hierarchy)
                    }));
    }

    if let Some(parent) = registered_instantiated_struct_parent_with_lookup(name, params, hierarchy)
    {
        let target = CoreType::Struct {
            name: other_base.to_string(),
            params: other_params.to_vec(),
        };
        return core_type_is_subtype_with_lookup(&parent, &target, hierarchy);
    }

    match (name_base, other_base) {
        // Ref subtyping (Issue #5130). `Ref` is the abstract supertype of the
        // concrete `RefValue`. Permitted relations (not the reverse `Ref <: RefValue`):
        //   RefValue{T} <: Ref, RefValue{T} <: RefValue, RefValue{T} <: Ref{T},
        //   Ref{T} <: Ref.
        // When the supertype carries a (concrete) element parameter the elements
        // must agree; a bare supertype (no params) matches any element type.
        ("RefValue", "Ref" | "RefValue") | ("Ref", "Ref") => {
            ref_element_params_match_with_lookup(params, other_params, hierarchy)
        }
        // Parametric concrete container `<:` its PARAMETRIZED abstract supertype
        // with EQUAL invariant parameters, generalizing the array-family path
        // above to the remaining built-in containers (`Dict <: AbstractDict`,
        // `Set <: AbstractSet`). Upstream containers are invariant in their
        // parameters, and the abstract supertype shares the concrete type's
        // parameter list positionally, so `Dict{String,Int} <:
        // AbstractDict{String,Int}` is true while `Dict{String,Int} <:
        // AbstractDict{String,Real}` is false. A bare (parameter-free) abstract
        // supertype stays covariant and matches any parameters (Issue #5564).
        (sub, sup)
            if core_abstract_from_name(sup).is_some_and(|abstract_ty| {
                struct_is_subtype_of_abstract_with_lookup(sub, params, &abstract_ty, hierarchy)
            }) =>
        {
            container_invariant_params_match_with_lookup(params, other_params, hierarchy)
        }
        _ => struct_family_subtype(name, other_name) && other_params.is_empty(),
    }
}

fn struct_param_matches_pattern_with_lookup(
    actual: &CoreType,
    expected: &CoreType,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if let CoreType::TypeVar(var) = expected {
        if let Some(lower) = var.lower_bound.as_deref() {
            if !core_type_is_subtype_with_lookup(lower, actual, hierarchy) {
                return false;
            }
        }
        if let Some(upper) = var.upper_bound.as_deref() {
            if !core_type_is_subtype_with_lookup(actual, upper, hierarchy) {
                return false;
            }
        }
        return true;
    }

    actual == expected
        || (core_type_is_subtype_with_lookup(actual, expected, hierarchy)
            && core_type_is_subtype_with_lookup(expected, actual, hierarchy))
}

/// Invariant parameter matching for a parametric concrete container against its
/// parametrized abstract supertype (`Dict{K,V} <: AbstractDict{K,V}`,
/// `Set{T} <: AbstractSet{T}`). The abstract supertype shares the concrete
/// type's parameter list positionally, so:
/// - a bare supertype (no params) is covariant and matches anything;
/// - otherwise arity must match and each shared parameter is invariant
///   (compared by equality, seeing through typevar patterns and aliases).
fn container_invariant_params_match_with_lookup(
    params: &[CoreType],
    other_params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if other_params.is_empty() {
        return true;
    }
    params.len() == other_params.len()
        && params
            .iter()
            .zip(other_params.iter())
            .all(|(actual, expected)| {
                array_unionall_element_param_matches_with_lookup(actual, expected, hierarchy)
            })
}

/// Fixed rank of an array-family type name, distinguishing fixed-rank aliases
/// from the rank-`N` parametric forms (Issue #5047):
/// - `Some(Some(1))` for `Vector` / `DenseVector` / `AbstractVector`,
/// - `Some(Some(2))` for `Matrix` / `DenseMatrix` / `AbstractMatrix`,
/// - `Some(None)` for `Array` / `AbstractArray` / `DenseArray` and wrapper
///   arrays (`SubArray`, `ReshapedArray`) whose rank comes from the second type
///   parameter, or is a free `N` when absent),
/// - `None` for anything that is not an array-family type.
fn array_family_dim(name: &str) -> Option<Option<i64>> {
    match base_type_name(name) {
        "Vector" | "DenseVector" | "AbstractVector" | "BitVector" | "AbstractRange"
        | "AbstractUnitRange" | "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange"
        | "OneTo" | "LogRange" => Some(Some(1)),
        "Matrix" | "DenseMatrix" | "AbstractMatrix" | "BitMatrix" => Some(Some(2)),
        "Array" | "AbstractArray" | "DenseArray" | "BitArray" | "SubArray" | "ReshapedArray" => {
            Some(None)
        }
        _ => None,
    }
}

/// Abstractness rank of an array-family type name, used to enforce the directional
/// (abstract <-> concrete) array-family subtype relation independently of element
/// type and rank (Issue #5640):
/// - `0` for the concrete leaves `Array` / `Vector` / `Matrix`,
/// - `1` for the dense abstract layer `DenseArray` / `DenseVector` / `DenseMatrix`,
/// - `2` for the top abstract layer `AbstractArray` / `AbstractVector` /
///   `AbstractMatrix` and wrapper arrays that subtype it directly.
///
/// A subtype must be equal-or-more-concrete than its supertype, so a valid
/// array-family `A <: B` requires `abstractness(A) <= abstractness(B)`. Non
/// array-family names return `0`; callers gate on `array_family_dim` first.
fn array_family_abstractness(name: &str) -> u8 {
    match base_type_name(name) {
        "Array" | "Vector" | "Matrix" => 0,
        "DenseArray" | "DenseVector" | "DenseMatrix" => 1,
        "AbstractArray" | "AbstractVector" | "AbstractMatrix" | "BitArray" | "BitVector"
        | "BitMatrix" | "SubArray" | "ReshapedArray" | "AbstractRange" | "AbstractUnitRange"
        | "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange" | "OneTo" | "LogRange" => 2,
        _ => 0,
    }
}

fn array_family_is_wrapper(name: &str) -> bool {
    matches!(base_type_name(name), "SubArray" | "ReshapedArray")
}

fn array_family_name_subtype_allowed(name: &str, other_name: &str) -> bool {
    let name = base_type_name(name);
    let other_name = base_type_name(other_name);

    if bitarray_family_dim(name).is_some() || bitarray_family_dim(other_name).is_some() {
        return bitarray_family_name_subtype_allowed(name, other_name);
    }

    if matches!(other_name, "SubArray" | "ReshapedArray") {
        return name == other_name;
    }

    if matches!(name, "SubArray" | "ReshapedArray") {
        return matches!(
            other_name,
            "AbstractArray" | "AbstractVector" | "AbstractMatrix"
        );
    }

    if range_family_dim(name).is_some() || range_family_dim(other_name).is_some() {
        return range_family_name_subtype_allowed(name, other_name);
    }

    array_family_abstractness(name) <= array_family_abstractness(other_name)
}

fn bitarray_family_dim(name: &str) -> Option<Option<i64>> {
    match base_type_name(name) {
        "BitVector" => Some(Some(1)),
        "BitMatrix" => Some(Some(2)),
        "BitArray" => Some(None),
        _ => None,
    }
}

fn bitarray_family_name_subtype_allowed(name: &str, other_name: &str) -> bool {
    let name_is_bitarray = bitarray_family_dim(name).is_some();
    let other_is_bitarray = bitarray_family_dim(other_name).is_some();

    match (name_is_bitarray, other_is_bitarray) {
        (true, true) => true,
        (true, false) => matches!(
            other_name,
            "AbstractArray" | "AbstractVector" | "AbstractMatrix"
        ),
        (false, true) => false,
        (false, false) => true,
    }
}

fn range_family_dim(name: &str) -> Option<i64> {
    match base_type_name(name) {
        "AbstractRange" | "AbstractUnitRange" | "UnitRange" | "StepRange" | "StepRangeLen"
        | "LinRange" | "OneTo" | "LogRange" => Some(1),
        _ => None,
    }
}

fn range_family_name_subtype_allowed(name: &str, other_name: &str) -> bool {
    let name = base_type_name(name);
    let other_name = base_type_name(other_name);

    if name == other_name {
        return true;
    }

    match name {
        "AbstractRange" => matches!(other_name, "AbstractVector" | "AbstractArray"),
        "AbstractUnitRange" => matches!(
            other_name,
            "AbstractRange" | "AbstractVector" | "AbstractArray"
        ),
        "UnitRange" | "OneTo" => matches!(
            other_name,
            "AbstractUnitRange" | "AbstractRange" | "AbstractVector" | "AbstractArray"
        ),
        "StepRange" | "StepRangeLen" | "LinRange" => {
            matches!(
                other_name,
                "AbstractRange" | "AbstractVector" | "AbstractArray"
            )
        }
        "LogRange" => matches!(other_name, "AbstractVector" | "AbstractArray"),
        _ => false,
    }
}

/// Element type and (when known) rank of an array-family `name{params...}`.
/// The element is `params[0]` when present; the rank is the alias's fixed rank,
/// otherwise the integer value of `params[1]` for the rank-`N` forms.
fn array_family_element_and_rank(
    name: &str,
    params: &[CoreType],
) -> (Option<CoreType>, Option<i64>) {
    if let Some(rank) = bitarray_family_dim(name) {
        let rank = match rank {
            Some(fixed) => Some(fixed),
            None => match params.first() {
                Some(CoreType::Value(CoreValueParam::Int(n))) => Some(*n),
                _ => None,
            },
        };
        return (Some(CoreType::Primitive(CorePrimitive::Bool)), rank);
    }

    let element = params.first().cloned();
    let rank = match array_family_dim(name) {
        Some(Some(fixed)) => Some(fixed),
        Some(None) => match params.get(1) {
            Some(CoreType::Value(CoreValueParam::Int(n))) => Some(*n),
            _ => None,
        },
        None => None,
    };
    (element, rank)
}

/// Invariant subtyping between two array-family types (concrete `Array`/`Vector`/
/// `Matrix` and abstract `AbstractArray`/`AbstractVector`/`AbstractMatrix`/
/// `DenseArray`), used for the parametric abstract supertype cases such as
/// `Vector{Int} <: AbstractVector{Int}` and `Vector{Float64} <:
/// AbstractVector{Int64}` (Issue #5047).
///
/// Julia arrays are invariant in their element type: when the supertype carries
/// an element parameter the elements must be *equal* (not merely a subtype),
/// matching `array_unionall_element_param_matches`'s typevar-pattern semantics
/// for `where`-bound element variables. The rank is likewise invariant: when the
/// supertype pins a rank it must equal the subtype's rank. A supertype that
/// omits a parameter (bare element, or a rank-`N` form with no `N`) leaves that
/// dimension unconstrained.
fn array_family_invariant_subtype_with_lookup(
    name: &str,
    params: &[CoreType],
    other_name: &str,
    other_params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    let (sub_elem, sub_rank) = array_family_element_and_rank(name, params);
    let (sup_elem, sup_rank) = array_family_element_and_rank(other_name, other_params);

    // Rank: a supertype that pins a rank requires an equal subtype rank. When the
    // subtype's rank is unknown (a free `N`) it cannot be shown to match a pinned
    // supertype rank, so be conservative and reject.
    if let Some(sup_r) = sup_rank {
        if sub_rank != Some(sup_r) {
            return false;
        }
    }

    // Element: invariant when the supertype carries one. A supertype with no
    // element parameter leaves the element unconstrained (covariant bare case is
    // handled elsewhere, but a rank-only supertype is still permissive here).
    match (sub_elem, sup_elem) {
        (_, None) => true,
        (Some(actual), Some(expected)) => {
            array_unionall_element_param_matches_with_lookup(&actual, &expected, hierarchy)
        }
        // Supertype pins an element but the subtype exposes none: cannot prove
        // invariant equality, so reject.
        (None, Some(_)) => false,
    }
}

fn array_family_pattern_params_match(
    actual_name: &str,
    actual_params: &[CoreType],
    pattern_name: &str,
    pattern_params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
) -> bool {
    array_family_pattern_params_match_with_lookup(
        actual_name,
        actual_params,
        pattern_name,
        pattern_params,
        scope,
        bindings,
        None,
    )
}

fn array_family_pattern_params_match_in(
    hierarchy: &StructHierarchy,
    actual_name: &str,
    actual_params: &[CoreType],
    pattern_name: &str,
    pattern_params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
) -> bool {
    array_family_pattern_params_match_with_lookup(
        actual_name,
        actual_params,
        pattern_name,
        pattern_params,
        scope,
        bindings,
        Some(hierarchy),
    )
}

fn array_family_pattern_params_match_with_lookup(
    actual_name: &str,
    actual_params: &[CoreType],
    pattern_name: &str,
    pattern_params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if pattern_params.is_empty() {
        return true;
    }

    let (actual_elem, actual_rank) = array_family_element_and_rank(actual_name, actual_params);
    let (pattern_elem, pattern_rank) = array_family_element_and_rank(pattern_name, pattern_params);

    if let Some(pattern_rank) = pattern_rank {
        if actual_rank != Some(pattern_rank) {
            return false;
        }
    }

    match (actual_elem, pattern_elem) {
        (_, None) => true,
        (Some(actual), Some(pattern)) => core_type_matches_pattern_with_lookup(
            &actual,
            &pattern,
            scope,
            bindings,
            TypeVarVariance::Invariant,
            hierarchy,
        ),
        (None, Some(_)) => false,
    }
}

/// Element-parameter matching for `RefValue{T} <: Ref{E}` (Issue #5130).
/// A bare supertype (`Ref` / `RefValue` with no parameter) matches any element.
fn ref_element_params_match_with_lookup(
    params: &[CoreType],
    other_params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if other_params.is_empty() {
        return true;
    }
    match (params.first(), other_params.first()) {
        (Some(actual), Some(expected)) => {
            array_unionall_element_param_matches_with_lookup(actual, expected, hierarchy)
        }
        // Supertype has an element parameter but the concrete RefValue does not
        // expose one: be permissive (element unknown).
        _ => true,
    }
}

fn array_unionall_element_param_matches(actual: &CoreType, expected: &CoreType) -> bool {
    array_unionall_element_param_matches_with_lookup(actual, expected, None)
}

fn array_unionall_element_param_matches_with_lookup(
    actual: &CoreType,
    expected: &CoreType,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if core_type_contains_typevar(expected) {
        let mut scope = HashMap::new();
        let mut bindings = TypeVarBindingState::default();
        return core_type_matches_pattern_with_lookup(
            actual,
            expected,
            &mut scope,
            &mut bindings,
            TypeVarVariance::Invariant,
            hierarchy,
        ) && bindings.satisfies_diagonal_rule();
    }
    actual == expected
}

fn core_type_contains_typevar(ty: &CoreType) -> bool {
    match ty {
        CoreType::TypeVar(_) => true,
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().any(core_type_contains_typevar)
        }
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => core_type_contains_typevar(inner),
        CoreType::VarargLen { element, len } => {
            core_type_contains_typevar(element) || core_type_contains_typevar(len)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, field_ty)| core_type_contains_typevar(field_ty)),
        CoreType::UnionAll { var, body } => {
            var.lower_bound
                .as_deref()
                .is_some_and(core_type_contains_typevar)
                || var
                    .upper_bound
                    .as_deref()
                    .is_some_and(core_type_contains_typevar)
                || core_type_contains_typevar(body)
        }
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => false,
    }
}

/// Abstract container/array types whose parametric spelling carries an
/// *invariant* element (and, for arrays, dimension) parameter that subtyping
/// must enforce: `AbstractVector{Int}`, `AbstractArray{T,N}`, etc. These are
/// retained as `CoreType::Struct { name, params }` by `from_julia_name` so that
/// `Vector{Float64} <: AbstractVector{Int64}` correctly reduces to an invariant
/// element check (false) rather than dropping the parameter (Issue #5047). The
/// bare, parameter-free spelling is unaffected and stays `CoreType::Abstract`.
fn is_parametric_container_abstract_name(name: &str) -> bool {
    matches!(
        base_type_name(name),
        "AbstractArray"
            | "AbstractVector"
            | "AbstractMatrix"
            | "DenseArray"
            | "AbstractDict"
            | "AbstractSet"
            | "AbstractRange"
            | "AbstractUnitRange"
    )
}

fn is_known_struct_family(name: &str) -> bool {
    matches!(
        base_type_name(name),
        "Array"
            | "Vector"
            | "Matrix"
            | "BitArray"
            | "BitVector"
            | "BitMatrix"
            | "SubArray"
            | "ReshapedArray"
            | "DenseVector"
            | "DenseMatrix"
            | "Tuple"
            | "NamedTuple"
            | "Dict"
            | "Set"
            | "Complex"
            | "Rational"
            | "Irrational"
            | "Diagonal"
            | "UnitRange"
            | "StepRange"
            | "StepRangeLen"
            | "LinRange"
            | "LogRange"
            | "OneTo"
            | "IOBuffer"
            | "Pair"
            | "Pairs"
            | "Fix1"
            | "Fix2"
            | "Enumerate"
            | "Zip"
            | "Zip3"
            | "Zip4"
            | "Zip5"
            | "Zip6"
            | "Zip7"
            | "Rest"
            | "Take"
            | "Drop"
            | "TakeWhile"
            | "DropWhile"
            | "Filter"
            | "Flatten"
            | "FlatMap"
            | "Generator"
            | "Memory"
            | "MemoryRef"
            | "Ref"
            | "RefValue"
            | "VersionNumber"
            | "Expr"
            | "QuoteNode"
            | "LineNumberNode"
            | "GlobalRef"
            | "Binding"
    )
}

/// Whether a tuple element list is the universal `Tuple{Vararg{Any}}` shape:
/// a single trailing `Vararg` whose element type is `Any` and with no fixed
/// length. This is the canonical form of the bare `Tuple` datatype upstream
/// (`Tuple === Tuple{Vararg{Any}}`), so only this exact shape is a supertype of
/// bare `Tuple` (Issue #5061).
fn is_universal_vararg_tuple(elements: &[CoreType]) -> bool {
    matches!(elements, [CoreType::Vararg(inner)] if matches!(inner.as_ref(), CoreType::Any))
}

fn tuple_params_match(tuple: &CoreType, params: &[CoreType]) -> bool {
    tuple_params_match_with_lookup(tuple, params, None)
}

fn tuple_params_match_in(
    hierarchy: &StructHierarchy,
    tuple: &CoreType,
    params: &[CoreType],
) -> bool {
    tuple_params_match_with_lookup(tuple, params, Some(hierarchy))
}

fn tuple_params_match_with_lookup(
    tuple: &CoreType,
    params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    let CoreType::Tuple(elements) = tuple else {
        return false;
    };
    tuple_elements_match_with_lookup(elements, params, hierarchy)
}

fn tuple_elements_match(elements: &[CoreType], params: &[CoreType]) -> bool {
    tuple_elements_match_with_lookup(elements, params, None)
}

fn tuple_elements_match_in(
    hierarchy: &StructHierarchy,
    elements: &[CoreType],
    params: &[CoreType],
) -> bool {
    tuple_elements_match_with_lookup(elements, params, Some(hierarchy))
}

fn tuple_elements_match_with_lookup(
    elements: &[CoreType],
    params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    let mut scope = HashMap::new();
    let mut bindings = TypeVarBindingState::default();
    tuple_elements_match_with_bindings_with_lookup(
        elements,
        params,
        &mut scope,
        &mut bindings,
        hierarchy,
    ) && bindings.satisfies_diagonal_rule()
}

fn named_tuple_fields_match(fields: &[(String, CoreType)], params: &[(String, CoreType)]) -> bool {
    named_tuple_fields_match_with_lookup(fields, params, None)
}

fn named_tuple_fields_match_in(
    hierarchy: &StructHierarchy,
    fields: &[(String, CoreType)],
    params: &[(String, CoreType)],
) -> bool {
    named_tuple_fields_match_with_lookup(fields, params, Some(hierarchy))
}

fn named_tuple_fields_match_with_lookup(
    fields: &[(String, CoreType)],
    params: &[(String, CoreType)],
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    fields.len() == params.len()
        && fields
            .iter()
            .zip(params)
            .all(|((field_name, field_ty), (param_name, param_ty))| {
                field_name == param_name
                    && core_type_is_subtype_with_lookup(field_ty, param_ty, hierarchy)
                    && core_type_is_subtype_with_lookup(param_ty, field_ty, hierarchy)
            })
}

fn named_tuple_marker_params_match(fields: &[(String, CoreType)], params: &[CoreType]) -> bool {
    let marker_names = match params {
        [] => return true,
        [marker] => named_tuple_marker_param_names(marker),
        _ => return false,
    };

    marker_names.is_none_or(|names| {
        fields.len() == names.len()
            && fields
                .iter()
                .zip(names)
                .all(|((field_name, _), marker_name)| field_name == &marker_name)
    })
}

/// Extract literal field names from a names-only `NamedTuple{(:a, :b)}`
/// marker. Returns `None` for non-literal marker params such as
/// `NamedTuple{names}`, which are unconstrained at this layer (Issue #5890).
fn named_tuple_marker_param_names(marker: &CoreType) -> Option<Vec<String>> {
    match marker {
        CoreType::Named(raw) => parse_named_tuple_marker_names(raw),
        CoreType::Tuple(elements) => elements
            .iter()
            .map(|element| match element {
                CoreType::Value(CoreValueParam::Symbol(name)) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn parse_named_tuple_marker_names(raw: &str) -> Option<Vec<String>> {
    let inner = raw.strip_prefix('(')?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.strip_prefix(':').map(str::to_string))
        .collect()
}

fn tuple_elements_match_with_bindings(
    elements: &[CoreType],
    params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
) -> bool {
    tuple_elements_match_with_bindings_with_lookup(elements, params, scope, bindings, None)
}

fn tuple_elements_match_with_bindings_in(
    hierarchy: &StructHierarchy,
    elements: &[CoreType],
    params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
) -> bool {
    tuple_elements_match_with_bindings_with_lookup(
        elements,
        params,
        scope,
        bindings,
        Some(hierarchy),
    )
}

fn tuple_elements_match_with_bindings_with_lookup(
    elements: &[CoreType],
    params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    // Flatten concrete-length `Vararg{T, N}` / `NTuple{N, T}` on either side so
    // both directions of `NTuple{3, T} <: Tuple{T, T, T}` reduce to the plain
    // fixed-arity tuple comparison (Issue #5062). Type-variable lengths survive
    // the flattening and fall through to the binding-aware paths below.
    if let Some(expanded_elements) = expand_concrete_vararg_len(elements) {
        return tuple_elements_match_with_bindings_with_lookup(
            &expanded_elements,
            params,
            scope,
            bindings,
            hierarchy,
        );
    }
    if let Some(expanded_params) = expand_concrete_vararg_len(params) {
        return tuple_elements_match_with_bindings_with_lookup(
            elements,
            &expanded_params,
            scope,
            bindings,
            hierarchy,
        );
    }

    if let Some((
        CoreType::VarargLen {
            element: pattern_element,
            len: pattern_len,
        },
        pattern_fixed,
    )) = params.split_last()
    {
        return tuple_elements_match_fixed_vararg_len(
            elements,
            pattern_fixed,
            pattern_element,
            pattern_len,
            scope,
            bindings,
            hierarchy,
        );
    }

    let (actual_fixed, actual_vararg) = split_trailing_vararg(elements);
    let (pattern_fixed, pattern_vararg) = split_trailing_vararg(params);

    if pattern_vararg.is_none() {
        return actual_vararg.is_none()
            && actual_fixed.len() == pattern_fixed.len()
            && actual_fixed
                .iter()
                .zip(pattern_fixed.iter())
                .all(|(element, param)| {
                    core_type_matches_pattern_with_lookup(
                        element,
                        param,
                        scope,
                        bindings,
                        TypeVarVariance::Covariant,
                        hierarchy,
                    )
                });
    }

    if actual_fixed.len() < pattern_fixed.len() {
        return false;
    }

    let fixed_match = actual_fixed
        .iter()
        .take(pattern_fixed.len())
        .zip(pattern_fixed.iter())
        .all(|(element, param)| {
            core_type_matches_pattern_with_lookup(
                element,
                param,
                scope,
                bindings,
                TypeVarVariance::Covariant,
                hierarchy,
            )
        });
    if !fixed_match {
        return false;
    }

    let Some(pattern_vararg_ty) = pattern_vararg else {
        return false;
    };

    let extra_fixed_match = actual_fixed
        .iter()
        .skip(pattern_fixed.len())
        .all(|element| {
            core_type_matches_pattern_with_lookup(
                element,
                pattern_vararg_ty,
                scope,
                bindings,
                TypeVarVariance::Covariant,
                hierarchy,
            )
        });
    if !extra_fixed_match {
        return false;
    }

    actual_vararg.is_none_or(|actual_vararg_ty| {
        core_type_matches_pattern_with_lookup(
            actual_vararg_ty,
            pattern_vararg_ty,
            scope,
            bindings,
            TypeVarVariance::Covariant,
            hierarchy,
        )
    })
}

fn tuple_elements_match_fixed_vararg_len(
    elements: &[CoreType],
    pattern_fixed: &[CoreType],
    pattern_element: &CoreType,
    pattern_len: &CoreType,
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if elements.len() < pattern_fixed.len() {
        return false;
    }

    let extra_len = elements.len() - pattern_fixed.len();
    let Ok(extra_len) = i64::try_from(extra_len) else {
        return false;
    };
    let len_actual = CoreType::Value(CoreValueParam::Int(extra_len));
    if !core_type_matches_pattern_with_lookup(
        &len_actual,
        pattern_len,
        scope,
        bindings,
        TypeVarVariance::Invariant,
        hierarchy,
    ) {
        return false;
    }

    pattern_fixed
        .iter()
        .zip(elements.iter())
        .all(|(param, element)| {
            core_type_matches_pattern_with_lookup(
                element,
                param,
                scope,
                bindings,
                TypeVarVariance::Covariant,
                hierarchy,
            )
        })
        && elements.iter().skip(pattern_fixed.len()).all(|element| {
            core_type_matches_pattern_with_lookup(
                element,
                pattern_element,
                scope,
                bindings,
                TypeVarVariance::Covariant,
                hierarchy,
            )
        })
}

/// Expand any `Vararg{T, N}` element whose length `N` is a concrete `Int`
/// into `N` copies of `T`, mirroring Julia's identity
/// `Tuple{Vararg{T, N}} === Tuple{T, T, ..., T}` (and the `NTuple{N, T}`
/// alias, which `from_julia_name` already lowers to the same shape).
///
/// Returns `None` when no expansion is needed so callers can keep operating
/// on the borrowed slice. Elements with a type-variable length (e.g. the
/// open `Vararg{T, N}` where `N` is free) are left untouched; those are still
/// handled by the trailing-`VarargLen` pattern machinery and by
/// `split_trailing_vararg`'s open-tail logic (Issue #5062).
fn expand_concrete_vararg_len(elements: &[CoreType]) -> Option<Vec<CoreType>> {
    if !elements
        .iter()
        .any(|e| matches!(e, CoreType::VarargLen { len, .. } if concrete_vararg_len(len).is_some()))
    {
        return None;
    }

    let mut expanded = Vec::with_capacity(elements.len());
    for element in elements {
        match element {
            CoreType::VarargLen {
                element: inner,
                len,
            } => match concrete_vararg_len(len) {
                Some(count) => {
                    for _ in 0..count {
                        expanded.push(inner.as_ref().clone());
                    }
                }
                None => expanded.push(element.clone()),
            },
            other => expanded.push(other.clone()),
        }
    }
    Some(expanded)
}

/// Extract the concrete non-negative repetition count from a `Vararg`/`NTuple`
/// length parameter, or `None` if it is a type variable or otherwise unknown.
fn concrete_vararg_len(len: &CoreType) -> Option<usize> {
    match len {
        CoreType::Value(CoreValueParam::Int(n)) if *n >= 0 => usize::try_from(*n).ok(),
        _ => None,
    }
}

fn split_trailing_vararg(elements: &[CoreType]) -> (&[CoreType], Option<&CoreType>) {
    if let Some((CoreType::Vararg(vararg_ty), fixed)) = elements.split_last() {
        (fixed, Some(vararg_ty.as_ref()))
    } else {
        (elements, None)
    }
}

fn core_type_is_concrete_diagonal(ty: &CoreType) -> bool {
    match ty {
        CoreType::Primitive(_) | CoreType::Value(_) | CoreType::Module(_) => true,
        // A rigid type variable (introduced by a forall-LEFT `where`, Issue
        // #5047) denotes ONE fixed — if opaque — type, so it satisfies the
        // diagonal rule's "single consistent value" requirement. Concretely this
        // makes `(Tuple{T,T} where T<:Integer) <: (Tuple{S,S} where S<:Real)`
        // true (∀ rigid T<:Integer ∃ S<:Real, S=T). This is distinct from a bare
        // abstract type such as `Real`: `Tuple{Real,Real} <: (Tuple{T,T} where
        // T)` stays false because its diagonal actual is `Abstract(Real)`, not a
        // `TypeVar`. Two DIFFERENT rigid vars (`Tuple{T,U}`) are already rejected
        // earlier by the binding-equality check in `bind_or_check`.
        CoreType::TypeVar(_) => true,
        CoreType::Struct { params, .. } => {
            !params.is_empty() && params.iter().all(core_type_is_concrete_diagonal)
        }
        CoreType::Tuple(elements) => {
            !elements.is_empty() && elements.iter().all(core_type_is_concrete_diagonal)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, field_ty)| core_type_is_concrete_diagonal(field_ty)),
        CoreType::TypeOf(inner) => core_type_is_concrete_diagonal(inner),
        CoreType::VarargLen { element, len } => {
            core_type_is_concrete_diagonal(element) && core_type_is_concrete_diagonal(len)
        }
        _ => false,
    }
}

fn base_type_name(name: &str) -> &str {
    name.rfind('.').map_or(name, |idx| &name[idx + 1..])
}

fn nominal_family_name(name: &str) -> &str {
    let base = base_type_name(name);
    base.split('{').next().unwrap_or(base)
}

/// Resolve a bare `Int`/`UInt` word alias that survives as an opaque
/// `CoreType::Named` into its native concrete primitive. Returns `None` for any
/// other type. Used by subtype fallbacks that must keep older `Named` spellings
/// compatible with native word aliases (Issue #5047, Issue #6097).
fn resolve_named_word_alias(ty: &CoreType) -> Option<CoreType> {
    match ty {
        CoreType::Named(name) => match base_type_name(name) {
            "Int" => Some(CoreType::from_julia_name(
                crate::types::native_int_type_name(),
            )),
            "UInt" => Some(CoreType::from_julia_name(
                crate::types::native_uint_type_name(),
            )),
            _ => None,
        },
        _ => None,
    }
}

fn is_type_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

fn parse_core_value_param(name: &str) -> Option<CoreValueParam> {
    let trimmed = name.trim();
    if let Some(value) = parse_typed_signed_int_value_param(trimmed) {
        return Some(value);
    }
    if let Some(value) = parse_typed_unsigned_int_value_param(trimmed) {
        return Some(value);
    }
    if trimmed == "true" {
        return Some(CoreValueParam::Bool(true));
    }
    if trimmed == "false" {
        return Some(CoreValueParam::Bool(false));
    }
    if let Some(symbol) = trimmed.strip_prefix(':') {
        if !symbol.is_empty()
            && symbol
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '!')
        {
            return Some(CoreValueParam::Symbol(symbol.to_string()));
        }
    }
    if let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Some(CoreValueParam::String(inner.to_string()));
    }
    trimmed.parse::<i64>().ok().map(CoreValueParam::Int)
}

fn parse_typed_signed_int_value_param(token: &str) -> Option<CoreValueParam> {
    for bits in [8_u16, 16, 32, 128] {
        let prefix = format!("Int{bits}(");
        let Some(inner) = token
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        let value = inner.parse::<i128>().ok()?;
        let in_range = match bits {
            8 => i8::try_from(value).is_ok(),
            16 => i16::try_from(value).is_ok(),
            32 => i32::try_from(value).is_ok(),
            128 => true,
            _ => false,
        };
        if !in_range {
            return None;
        }
        return Some(CoreValueParam::SignedInt { bits, value });
    }
    None
}

fn parse_typed_unsigned_int_value_param(token: &str) -> Option<CoreValueParam> {
    let (bits, value) = if let Some(digits) = token.strip_prefix("0x") {
        let bits = match digits.len() {
            2 => 8,
            4 => 16,
            8 => 32,
            16 => 64,
            32 => 128,
            _ => return None,
        };
        (bits, u128::from_str_radix(digits, 16).ok()?)
    } else {
        let mut parsed = None;
        for bits in [8_u16, 16, 32, 64, 128] {
            let prefix = format!("UInt{bits}(");
            let Some(inner) = token
                .strip_prefix(&prefix)
                .and_then(|value| value.strip_suffix(')'))
            else {
                continue;
            };
            parsed = Some((bits, inner.parse::<u128>().ok()?));
            break;
        }
        parsed?
    };
    let in_range = match bits {
        8 => u8::try_from(value).is_ok(),
        16 => u16::try_from(value).is_ok(),
        32 => u32::try_from(value).is_ok(),
        64 => u64::try_from(value).is_ok(),
        128 => true,
        _ => false,
    };
    if !in_range {
        return None;
    }
    Some(CoreValueParam::UnsignedInt { bits, value })
}

fn normalize_union(types: Vec<CoreType>) -> CoreType {
    let mut normalized = Vec::new();
    for ty in types {
        match ty {
            CoreType::Bottom => {}
            CoreType::Union(inner) => normalized.extend(inner),
            other => {
                if !normalized.contains(&other) {
                    normalized.push(other);
                }
            }
        }
    }
    match normalized.len() {
        0 => CoreType::Bottom,
        1 => normalized.pop().unwrap_or(CoreType::Bottom),
        _ => CoreType::Union(normalized),
    }
}

/// Split a rendered UnionAll surface name `Body where V` at the **rightmost**
/// top-level ` where ` keyword, returning `(body, var_spec)`. The rightmost
/// clause binds the outermost variable, matching the right-nested chain that
/// `JuliaType::name()` emits for several variables (`Body where V2 where V1`,
/// where `V1` is outermost) as well as upstream's
/// `Body where {A, B} == (Body where B) where A`. Returns `None` when there is
/// no top-level ` where ` — a ` where ` nested inside `{...}` / `(...)` /
/// `[...]` / a string literal is ignored so e.g. a hypothetical
/// `Tuple{typeof(where)}` is never mis-split (Issue #5047).
fn split_trailing_where(name: &str) -> Option<(&str, &str)> {
    const KW: &str = " where ";
    let bytes = name.as_bytes();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut last: Option<usize> = None;
    let mut idx = 0usize;
    while idx < name.len() {
        let ch = bytes[idx] as char;
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            idx += 1;
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ' ' if brace_depth == 0
                && paren_depth == 0
                && bracket_depth == 0
                && name[idx..].starts_with(KW) =>
            {
                last = Some(idx);
            }
            _ => {}
        }
        idx += 1;
    }
    let split = last?;
    let body = name[..split].trim();
    let var = name[split + KW.len()..].trim();
    if body.is_empty() || var.is_empty() {
        return None;
    }
    Some((body, var))
}

fn split_top_level_subtype_bound(name: &str) -> Option<(&str, &str)> {
    let bytes = name.as_bytes();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut idx = 0usize;
    while idx + 1 < name.len() {
        let ch = bytes[idx] as char;
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            idx += 1;
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '<' if brace_depth == 0
                && paren_depth == 0
                && bracket_depth == 0
                && bytes[idx + 1] as char == ':' =>
            {
                let left = name[..idx].trim();
                let right = name[idx + 2..].trim();
                if !right.is_empty() {
                    return Some((left, right));
                }
            }
            _ => {}
        }
        idx += 1;
    }
    None
}

/// Parse a single `where` variable specifier into a `CoreTypeVar`. Handles the
/// bare `T`, the lower-bounded `L<:T`, the upper-bounded `T<:U`, and the
/// double-bounded `L<:T<:U` spellings rendered by `JuliaType::name()`. The
/// variable name itself is not validated against the typevar-name heuristic so
/// multi-letter `where` variables (`Foo`) still bind (Issue #5047).
fn parse_where_var(spec: &str) -> CoreTypeVar {
    // Strip an enclosing `{...}` in case a single-variable clause was rendered
    // in brace form (`Body where {T<:Real}`).
    let spec = spec
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .map_or(spec, str::trim);

    let parts: Vec<&str> = spec.split("<:").map(str::trim).collect();
    let (name, lower_bound, upper_bound) = match parts.as_slice() {
        // `T <: S <: U` can be the display form for `T` whose upper bound is
        // the bounded type variable `S<:U` (for example
        // `where {S<:Real, T<:S<:Real}`). Treat a type-variable-looking left
        // side as the declared variable, not as a concrete lower bound.
        [name, upper_name, upper] if core_where_part_is_typevar_name(name) => (
            *name,
            None,
            Some(Box::new(CoreType::TypeVar(parse_where_var(&format!(
                "{upper_name}<:{upper}"
            ))))),
        ),
        // `L <: T <: U`
        [lower, name, upper] => (
            *name,
            (!lower.is_empty()).then(|| Box::new(CoreType::from_julia_name(lower))),
            (!upper.is_empty()).then(|| Box::new(CoreType::from_julia_name(upper))),
        ),
        // `T <: U` (upper-bounded) — the common spelling.
        [name, upper] => (
            *name,
            None,
            (!upper.is_empty()).then(|| Box::new(CoreType::from_julia_name(upper))),
        ),
        // bare `T`
        _ => (spec, None, None),
    };
    CoreTypeVar {
        name: if name.is_empty() {
            "_".to_string()
        } else {
            name.to_string()
        },
        lower_bound,
        upper_bound,
    }
}

fn parse_where_var_list(spec: &str) -> Option<Vec<CoreTypeVar>> {
    let inner = spec.strip_prefix('{')?.strip_suffix('}')?;
    let mut vars = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let var = inner[start..idx].trim();
                if !var.is_empty() {
                    vars.push(parse_where_var(var));
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let var = inner[start..].trim();
    if !var.is_empty() {
        vars.push(parse_where_var(var));
    }
    (!vars.is_empty()).then_some(vars)
}

fn core_where_part_is_typevar_name(name: &str) -> bool {
    matches!(CoreType::from_julia_name(name), CoreType::TypeVar(_))
}

/// Split a parametric type name `Base{p1, p2, ...}` into its base name and the
/// raw top-level parameter tokens, using the same tokenizer as
/// [`CoreType::from_julia_name`].
///
/// Issue #6336: this is the ONE central name tokenizer — runtime reflection
/// callers (e.g. `vm/type_objects.rs`) reuse it instead of maintaining their
/// own `find('{')` / comma-splitting copies. Returns `(name, [])` when the
/// name is not a well-formed parametric instantiation.
pub(crate) fn parse_parametric_type_name(name: &str) -> (&str, Vec<&str>) {
    parse_parametric_name(name)
}

fn parse_parametric_name(name: &str) -> (&str, Vec<&str>) {
    let Some(start) = name.find('{') else {
        return (name, vec![]);
    };
    if !name.ends_with('}') {
        return (name, vec![]);
    }
    let base = &name[..start];
    let inner = &name[start + 1..name.len() - 1];
    if inner.trim().is_empty() {
        return (base, vec![]);
    }

    let mut params = Vec::new();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut current_start = 0usize;
    for (idx, ch) in inner.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                params.push(inner[current_start..idx].trim());
                current_start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    params.push(inner[current_start..].trim());
    (base, params)
}

fn parse_named_tuple_type_name(name: &str) -> Option<CoreType> {
    let (base, params) = parse_parametric_name(name);
    match base {
        "@NamedTuple" => {
            let parsed_fields = params
                .iter()
                .map(|field| parse_concrete_named_tuple_field(field))
                .collect::<Option<Vec<_>>>()?;
            Some(CoreType::NamedTuple(parsed_fields))
        }
        "NamedTuple" => parse_typelevel_concrete_named_tuple(&params),
        _ => None,
    }
}

fn parse_concrete_named_tuple_field(field: &str) -> Option<(String, CoreType)> {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((name, ty)) = trimmed.split_once("::") {
        let name = name.trim();
        (!name.is_empty()).then(|| (name.to_string(), CoreType::from_julia_name(ty.trim())))
    } else {
        Some((trimmed.to_string(), CoreType::Any))
    }
}

fn parse_typelevel_concrete_named_tuple(params: &[&str]) -> Option<CoreType> {
    let [names_param, types_param] = params else {
        return None;
    };
    let names = parse_named_tuple_marker_names(names_param.trim())?;
    let CoreType::Tuple(field_types) = CoreType::from_julia_name(types_param.trim()) else {
        return None;
    };
    if names.len() != field_types.len() {
        return None;
    }

    Some(CoreType::NamedTuple(
        names.into_iter().zip(field_types).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JuliaType;

    #[test]
    fn julia_type_primitive_numeric_bridge() {
        let core = CoreType::from(&JuliaType::Float32);
        assert_eq!(core.primitive_numeric(), Some(PrimitiveNumeric::Float32));
        assert!(core.is_primitive_numeric());
        assert!(!CoreType::from(&JuliaType::BigInt).is_primitive_numeric());
    }

    /// Issue #6593: the structural family-name accessor returns the bare family
    /// (module prefix + parametric params stripped) for the nominal variants
    /// carrying a name, without rendering parametric params to a string. This is
    /// the `core_signature`-structured replacement for the old
    /// `to_julia_name()` -> `extract_base_type` -> `strip_module_prefix`
    /// round-trip in the CallDynamic family fallback.
    #[test]
    fn nominal_family_name_strips_module_and_params_issue_6593() {
        // Bare struct keeps its name.
        assert_eq!(
            CoreType::Struct {
                name: "Drop".to_string(),
                params: vec![],
            }
            .nominal_family_name(),
            Some("Drop")
        );
        // Parametric struct: params are NOT rendered into the family name.
        assert_eq!(
            CoreType::Struct {
                name: "Drop".to_string(),
                params: vec![CoreType::Any],
            }
            .nominal_family_name(),
            Some("Drop")
        );
        // Module-qualified parametric struct strips both module prefix and params.
        assert_eq!(
            CoreType::Struct {
                name: "Base.Iterators.Zip".to_string(),
                params: vec![CoreType::Any, CoreType::Any],
            }
            .nominal_family_name(),
            Some("Zip")
        );
        // `Named` sentinels (e.g. native-iterator legacy names) also resolve.
        assert_eq!(
            CoreType::Named("Base.Generator".to_string()).nominal_family_name(),
            Some("Generator")
        );
        // A `Named` carrying params in its spelling still strips them.
        assert_eq!(
            CoreType::Named("Enumerate{Vector{Int64}}".to_string()).nominal_family_name(),
            Some("Enumerate")
        );
        // AbstractUser and Module carry a nominal name too.
        assert_eq!(
            CoreType::AbstractUser {
                name: "Animal".to_string(),
                parent: None,
            }
            .nominal_family_name(),
            Some("Animal")
        );
        assert_eq!(
            CoreType::Module("Main".to_string()).nominal_family_name(),
            Some("Main")
        );
        // Non-nominal variants have no family name.
        assert_eq!(CoreType::Any.nominal_family_name(), None);
        assert_eq!(CoreType::Bottom.nominal_family_name(), None);
        assert_eq!(
            CoreType::Tuple(vec![CoreType::Any]).nominal_family_name(),
            None
        );
    }

    #[test]
    fn typeof_subtype_is_invariant_for_concrete_inner_issue_5068() {
        let type_int = CoreType::TypeOf(Box::new(CoreType::Primitive(CorePrimitive::Int64)));
        let type_integer = CoreType::TypeOf(Box::new(CoreType::Abstract(CoreAbstract::Integer)));
        // `Type{Int} <: Type{Integer}` is false: `Type{T}` is invariant in `T`.
        assert!(!type_int.is_subtype_of(&type_integer));
        // `Type{Int} <: Type{Int}` holds.
        assert!(type_int.is_subtype_of(&type_int.clone()));

        // `Type{Int} <: Type{<:Number}` reduces to `Int <: Number` (covariant).
        let type_le_number = CoreType::TypeOf(Box::new(CoreType::TypeVar(CoreTypeVar {
            name: "_".to_string(),
            lower_bound: None,
            upper_bound: Some(Box::new(CoreType::Abstract(CoreAbstract::Number))),
        })));
        assert!(type_int.is_subtype_of(&type_le_number));
        let type_string = CoreType::TypeOf(Box::new(CoreType::Primitive(CorePrimitive::String)));
        assert!(!type_string.is_subtype_of(&type_le_number));

        // The tighter covariant bound is strictly more specific:
        // `Type{<:Integer} <: Type{<:Number}` but not the reverse.
        let type_le_integer = CoreType::TypeOf(Box::new(CoreType::TypeVar(CoreTypeVar {
            name: "_".to_string(),
            lower_bound: None,
            upper_bound: Some(Box::new(CoreType::Abstract(CoreAbstract::Integer))),
        })));
        assert!(type_le_integer.is_subtype_of(&type_le_number));
        assert!(!type_le_number.is_subtype_of(&type_le_integer));
    }

    #[test]
    fn dispatch_primitive_classifiers_are_core_owned() {
        for ty in [
            JuliaType::Bool,
            JuliaType::Int8,
            JuliaType::Int64,
            JuliaType::UInt128,
            JuliaType::Float16,
            JuliaType::BigInt,
            JuliaType::BigFloat,
            JuliaType::String,
            JuliaType::Char,
        ] {
            assert!(
                CoreType::from(&ty).is_builtin_dispatch_primitive(),
                "{ty:?} should be a dispatch primitive"
            );
        }

        for ty in [
            JuliaType::Number,
            JuliaType::Real,
            JuliaType::Integer,
            JuliaType::Signed,
            JuliaType::Unsigned,
            JuliaType::AbstractFloat,
        ] {
            let core = CoreType::from(&ty);
            assert!(core.is_builtin_abstract_numeric(), "{ty:?}");
            assert!(
                core.is_builtin_dispatch_primitive_or_abstract_numeric(),
                "{ty:?}"
            );
        }

        for ty in [
            JuliaType::AbstractString,
            JuliaType::Struct("Rational{Int64}".to_string()),
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::Nothing,
            JuliaType::Missing,
            JuliaType::Symbol,
        ] {
            assert!(
                !CoreType::from(&ty).is_builtin_dispatch_primitive_or_abstract_numeric(),
                "{ty:?} should not use primitive dispatch classification"
            );
        }
    }

    #[test]
    fn parse_parametric_name_preserves_tuple_value_param() {
        let (base, params) = parse_parametric_name("Val{(1,2)}");
        assert_eq!(base, "Val");
        assert_eq!(params, vec!["(1,2)"]);

        let (base, params) = parse_parametric_name("Tuple{Val{(1,2)}, Int64}");
        assert_eq!(base, "Tuple");
        assert_eq!(params, vec!["Val{(1,2)}", "Int64"]);
    }

    #[test]
    fn typed_integer_value_parameters_keep_carrier_issue_5616() {
        assert_eq!(
            CoreType::from_julia_name("Val{0x01}"),
            CoreType::Struct {
                name: "Val".to_string(),
                params: vec![CoreType::Value(CoreValueParam::UnsignedInt {
                    bits: 8,
                    value: 1,
                })],
            }
        );
        assert_eq!(
            CoreType::from_julia_name("Val{UInt8(1)}"),
            CoreType::from_julia_name("Val{0x01}")
        );
        assert_eq!(
            CoreType::from_julia_name("Val{Int32(2)}").to_julia_name(),
            "Val{Int32(2)}"
        );
    }

    #[test]
    fn abstract_integer_accepting_classifier_is_core_owned() {
        for ty in [
            JuliaType::Number,
            JuliaType::Real,
            JuliaType::Integer,
            JuliaType::Signed,
            JuliaType::Unsigned,
        ] {
            assert!(
                CoreType::from(&ty).is_builtin_abstract_integer_accepting(),
                "{ty:?}"
            );
        }

        for ty in [
            JuliaType::AbstractFloat,
            JuliaType::Int64,
            JuliaType::Bool,
            JuliaType::Any,
            JuliaType::AbstractString,
        ] {
            assert!(
                !CoreType::from(&ty).is_builtin_abstract_integer_accepting(),
                "{ty:?}"
            );
        }
    }

    #[test]
    fn builtin_bits_type_matches_julia_primitive_layout() {
        for name in [
            "Bool", "Int8", "Int16", "Int32", "Int64", "Int128", "Int", "UInt8", "UInt16",
            "UInt32", "UInt64", "UInt128", "UInt", "Float16", "Float32", "Float64", "Char",
            "Nothing", "Missing",
        ] {
            assert!(
                CoreType::is_builtin_bits_type_for_julia_name(name),
                "{name} should be a bits type"
            );
        }

        for name in [
            "String", "Symbol", "BigInt", "BigFloat", "Array", "DataType",
        ] {
            assert!(
                !CoreType::is_builtin_bits_type_for_julia_name(name),
                "{name} should not be a bits type"
            );
        }
    }

    #[test]
    fn builtin_sizeof_bytes_matches_primitive_layout() {
        let native_word_bytes = usize::BITS as usize / 8;
        for (name, bytes) in [
            ("Bool", 1),
            ("Int8", 1),
            ("UInt8", 1),
            ("Int16", 2),
            ("UInt16", 2),
            ("Float16", 2),
            ("Int32", 4),
            ("UInt32", 4),
            ("Float32", 4),
            ("Char", 4),
            ("Int64", 8),
            ("UInt64", 8),
            ("Float64", 8),
            ("Int", native_word_bytes),
            ("UInt", native_word_bytes),
            ("Int128", 16),
            ("UInt128", 16),
            ("Nothing", 0),
            ("Missing", 0),
        ] {
            assert_eq!(
                CoreType::builtin_sizeof_bytes_for_julia_name(name),
                Some(bytes),
                "{name} should have a {bytes}-byte layout"
            );
        }

        for name in [
            "String", "Symbol", "BigInt", "BigFloat", "Array", "DataType",
        ] {
            assert_eq!(
                CoreType::builtin_sizeof_bytes_for_julia_name(name),
                None,
                "{name} does not have a fixed primitive sizeof layout"
            );
        }
    }

    #[test]
    fn builtin_primitive_datatype_matches_julia_flags() {
        for name in [
            "Bool", "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32",
            "UInt64", "UInt128", "Float16", "Float32", "Float64", "Char",
        ] {
            assert!(
                CoreType::is_builtin_primitive_datatype_for_julia_name(name),
                "{name} should be a primitive DataType"
            );
        }

        for name in [
            "Int", "UInt", "String", "Symbol", "BigInt", "BigFloat", "Nothing", "Missing",
            "Number", "Any", "Array",
        ] {
            assert!(
                !CoreType::is_builtin_primitive_datatype_for_julia_name(name),
                "{name} should not be a primitive DataType"
            );
        }
    }

    #[test]
    fn builtin_abstract_datatype_matches_julia_flags() {
        for name in [
            "Any",
            "Number",
            "Real",
            "Integer",
            "Signed",
            "Unsigned",
            "AbstractFloat",
            "AbstractString",
            "AbstractChar",
            "AbstractArray",
            "AbstractVector",
            "AbstractMatrix",
            "DenseArray",
            "AbstractDict",
            "AbstractSet",
            "AbstractRange",
            "AbstractUnitRange",
            "Function",
            "IO",
            "Type",
            "Type{Int64}",
            "AbstractVector{Int64}",
        ] {
            assert!(
                CoreType::is_builtin_abstract_datatype_for_julia_name(name),
                "{name} should be an abstract DataType"
            );
        }

        for name in [
            "DataType",
            "UnionAll",
            "Module",
            "Tuple",
            "NamedTuple",
            "Array",
            "Vector",
            "Matrix",
            "Dict",
            "Set",
            "UnitRange",
            "IOBuffer",
            "Int64",
            "String",
            "Nothing",
            "Union{Int64, Float64}",
        ] {
            assert!(
                !CoreType::is_builtin_abstract_datatype_for_julia_name(name),
                "{name} should not be an abstract DataType"
            );
        }
    }

    #[test]
    fn builtin_concrete_datatype_matches_julia_flags() {
        for name in [
            "Bool", "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32",
            "UInt64", "UInt128", "Float16", "Float32", "Float64", "BigInt", "BigFloat", "String",
            "Char", "Symbol", "Nothing", "Missing", "DataType",
        ] {
            assert!(
                CoreType::is_builtin_concrete_datatype_for_julia_name(name),
                "{name} should be a concrete DataType"
            );
        }

        for name in [
            "Complex{Float64}",
            "Rational{Int64}",
            "Array{Int64, 1}",
            "Vector{Real}",
            "Matrix{Float64}",
            "Tuple{Int64, String}",
            "Dict{String, Int64}",
            "Set{Int64}",
            "UnitRange{Int64}",
            "StepRange{Int64, Int64}",
            "LinRange{Float64}",
            "IOBuffer",
            "Pair{Int64, String}",
            "VersionNumber",
        ] {
            assert!(
                CoreType::is_builtin_concrete_datatype_for_julia_name(name),
                "{name} should be a concrete built-in struct DataType"
            );
        }

        for name in [
            "Any",
            "Number",
            "Real",
            "Integer",
            "AbstractFloat",
            "Type",
            "Type{Int64}",
            "Tuple",
            "NamedTuple",
            "Array",
            "Vector",
            "Matrix",
            "Dict",
            "Set",
            "Complex",
            "Rational",
            "Array{Int64}",
            "Vector{T}",
            "Tuple{Vararg{Int64}}",
            "Pair",
            "Union{Int64, Float64}",
        ] {
            assert!(
                !CoreType::is_builtin_concrete_datatype_for_julia_name(name),
                "{name} should not be a concrete DataType"
            );
        }
    }

    #[test]
    fn builtin_mutable_datatype_matches_julia_flags() {
        for name in [
            "String",
            "Symbol",
            "BigInt",
            "DataType",
            "Array",
            "Array{Int64, 1}",
            "Vector",
            "Vector{Int64}",
            "Matrix",
            "Matrix{Float64}",
            "Dict",
            "Dict{String, Int64}",
            "IOBuffer",
        ] {
            assert!(
                CoreType::is_builtin_mutable_datatype_for_julia_name(name),
                "{name} should be a mutable DataType"
            );
        }

        for name in [
            "Int64",
            "Float64",
            "Bool",
            "Char",
            "BigFloat",
            "Nothing",
            "Missing",
            "Set",
            "Set{Int64}",
            "Tuple",
            "Tuple{Int64, String}",
            "NamedTuple",
            "Complex{Float64}",
            "Rational{Int64}",
            "Union{Int64, Float64}",
        ] {
            assert!(
                !CoreType::is_builtin_mutable_datatype_for_julia_name(name),
                "{name} should not be a mutable DataType"
            );
        }
    }

    #[test]
    fn builtin_struct_datatype_matches_julia_flags() {
        for name in [
            "String",
            "Symbol",
            "BigInt",
            "BigFloat",
            "Nothing",
            "Missing",
            "DataType",
            "Tuple",
            "Tuple{Int64, String}",
            "NamedTuple",
            "Array",
            "Vector",
            "Vector{Int64}",
            "Dict",
            "Dict{String, Int64}",
            "Set",
            "Set{Int64}",
            "Complex",
            "Complex{Float64}",
            "Rational",
            "Rational{Int64}",
            "Pair",
            "IOBuffer",
            "VersionNumber",
            "UnionAll",
            "Module",
        ] {
            assert!(
                CoreType::is_builtin_struct_datatype_for_julia_name(name),
                "{name} should be a built-in struct DataType"
            );
        }

        for name in [
            "Int64",
            "Float64",
            "Bool",
            "Char",
            "Any",
            "Number",
            "Function",
            "IO",
            "Type",
            "Type{Int64}",
            "Union{Int64, Float64}",
        ] {
            assert!(
                !CoreType::is_builtin_struct_datatype_for_julia_name(name),
                "{name} should not be a struct DataType"
            );
        }
    }

    #[test]
    fn julia_type_struct_name_is_structured() {
        let core = CoreType::from(&JuliaType::Struct("Complex{Float64}".to_string()));
        assert_eq!(
            core,
            CoreType::Struct {
                name: "Complex".to_string(),
                params: vec![CoreType::Primitive(CorePrimitive::Float64)],
            }
        );
    }

    #[test]
    fn nested_parametric_name_parser_preserves_nested_params() {
        let core = CoreType::from_julia_name("Dict{String, Vector{Int64}}");
        assert_eq!(
            core,
            CoreType::Struct {
                name: "Dict".to_string(),
                params: vec![
                    CoreType::Primitive(CorePrimitive::String),
                    CoreType::Struct {
                        name: "Vector".to_string(),
                        params: vec![CoreType::Primitive(CorePrimitive::Int64)],
                    },
                ],
            }
        );
    }

    #[test]
    fn parametric_name_parser_structures_value_parameters_issue_3885() {
        assert_eq!(
            CoreType::from_julia_name("Val{1}"),
            CoreType::Struct {
                name: "Val".to_string(),
                params: vec![CoreType::Value(CoreValueParam::Int(1))],
            }
        );

        assert_eq!(
            CoreType::from_julia_name("Array{Int64, 2}"),
            CoreType::Struct {
                name: "Array".to_string(),
                params: vec![
                    CoreType::Primitive(CorePrimitive::Int64),
                    CoreType::Value(CoreValueParam::Int(2)),
                ],
            }
        );

        assert_eq!(
            CoreType::from_julia_name("Tuple{Vararg{Int64, 3}}"),
            CoreType::Tuple(vec![CoreType::VarargLen {
                element: Box::new(CoreType::Primitive(CorePrimitive::Int64)),
                len: Box::new(CoreType::Value(CoreValueParam::Int(3))),
            }])
        );
        assert_eq!(
            CoreType::from_julia_name("NTuple{3}"),
            CoreType::Tuple(vec![CoreType::VarargLen {
                element: Box::new(CoreType::Any),
                len: Box::new(CoreType::Value(CoreValueParam::Int(3))),
            }])
        );
    }

    #[test]
    fn vararg_len_value_parameter_subtype_intersect_issue_5062() {
        // NTuple{3, Int} <: Tuple{Int, Int, Int} (reverse direction of the
        // already-supported Tuple{...} <: NTuple{N, T}). The fixed-length
        // vararg on the *actual* side must expand into a flat tuple shape.
        assert!(CoreType::from_julia_name("NTuple{3, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")));
        // NTuple{3, Int} == Tuple{Int, Int, Int} (both directions hold).
        assert!(CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("NTuple{3, Int64}")));
        // Length mismatch must fail.
        assert!(!CoreType::from_julia_name("NTuple{2, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")));
        // Element covariance still holds across the alias expansion.
        assert!(CoreType::from_julia_name("NTuple{3, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Tuple{Real, Real, Real}")));
        // Vararg{T, N} with concrete N requires an exact length match.
        assert!(CoreType::from_julia_name("Tuple{Vararg{Int64, 3}}")
            .is_subtype_of(&CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")));
        assert!(!CoreType::from_julia_name("Tuple{Vararg{Int64, 3}}")
            .is_subtype_of(&CoreType::from_julia_name("Tuple{Int64, Int64}")));
        // Two fixed-length varargs: equal length + element subtype holds.
        assert!(CoreType::from_julia_name("NTuple{3, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("NTuple{3, Integer}")));
        assert!(!CoreType::from_julia_name("NTuple{3, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("NTuple{2, Int64}")));

        // type_intersect over the alias mirrors upstream `typeintersect`.
        assert_eq!(
            CoreType::from_julia_name("NTuple{3, Int64}")
                .type_intersect(&CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")),
            CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")
        );
        assert_eq!(
            CoreType::from_julia_name("NTuple{2, Int64}")
                .type_intersect(&CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")),
            CoreType::Bottom
        );
    }

    #[test]
    fn value_parameter_subtyping_handles_array_aliases_and_ntuple_issue_3885() {
        assert!(CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Int64, 1}")));
        assert!(CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Int64}")));
        assert!(CoreType::from_julia_name("Matrix{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Float64}")));
        assert!(CoreType::from_julia_name("Array{Int64, 1}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Int64}")));
        assert!(CoreType::from_julia_name("Array{Int64, 3}")
            .is_subtype_of(&CoreType::from_julia_name("Array{T}")));
        assert!(CoreType::from_julia_name("Array{Bool, 3}")
            .is_subtype_of(&CoreType::from_julia_name("Array{T} where T")));
        assert!(CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{T} where T")));
        assert!(CoreType::from_julia_name("Matrix{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{T} where T")));
        assert!(!CoreType::from_julia_name("Array{Float64, 1}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Real}")));
        assert!(!CoreType::from_julia_name("Vector{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Real}")));
        assert!(CoreType::from_julia_name("Array{Int64, 1}")
            .is_subtype_of(&CoreType::from_julia_name("Vector{Int64}")));
        assert!(CoreType::from_julia_name("Array{Float64, 2}")
            .is_subtype_of(&CoreType::from_julia_name("Matrix{Float64}")));
        assert!(!CoreType::from_julia_name("Array{Float64, 3}")
            .is_subtype_of(&CoreType::from_julia_name("Matrix{Float64}")));
        assert!(CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("DenseVector{Int64}")));
        assert!(CoreType::from_julia_name("Matrix{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("DenseMatrix{Float64}")));
        assert!(!CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("DenseVector{Float64}")));

        assert!(CoreType::from_julia_name("Tuple{Int64, Int64, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("NTuple{3, Int64}")));
        assert!(!CoreType::from_julia_name("Tuple{Int64, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("NTuple{3, Int64}")));
        assert!(CoreType::from_julia_name("Tuple{Int64, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("NTuple{N, Int64}")));
    }

    #[test]
    fn dense_array_alias_subtyping_preserves_invariant_params_issue_3909() {
        assert!(CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("DenseVector{Int64}")));
        assert!(CoreType::from_julia_name("Matrix{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("DenseMatrix{Float64}")));
        assert!(!CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("DenseVector{Float64}")));
    }

    #[test]
    fn array_rank_structs_subtype_bare_abstract_aliases_issue_5615() {
        assert!(CoreType::from_julia_name("Array{Float64, 1}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(CoreType::from_julia_name("Array{Float64, 2}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix")));
        assert!(CoreType::from_julia_name("DenseArray{Int64, 1}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(CoreType::from_julia_name("DenseArray{Int64, 2}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix")));
        assert!(!CoreType::from_julia_name("Array{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(!CoreType::from_julia_name("Array{Float64, 3}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix")));
        assert!(!CoreType::from_julia_name("Matrix{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(!CoreType::from_julia_name("Vector{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix")));
    }

    #[test]
    fn wrapper_array_structs_skip_dense_layer_issue_5615() {
        let subarray = "SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}";
        let reshaped =
            "ReshapedArray{Int64, 2, SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}, Tuple}";

        assert!(CoreType::from_julia_name(subarray)
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector{Int64}")));
        assert!(CoreType::from_julia_name(subarray)
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray{Int64, 1}")));
        assert!(!CoreType::from_julia_name(subarray)
            .is_subtype_of(&CoreType::from_julia_name("DenseVector{Int64}")));
        assert!(!CoreType::from_julia_name(subarray)
            .is_subtype_of(&CoreType::from_julia_name("DenseArray{Int64, 1}")));

        assert!(CoreType::from_julia_name(reshaped)
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix{Int64}")));
        assert!(CoreType::from_julia_name(reshaped)
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray{Int64, 2}")));
        assert!(!CoreType::from_julia_name(reshaped)
            .is_subtype_of(&CoreType::from_julia_name("DenseMatrix{Int64}")));
        assert!(!CoreType::from_julia_name(reshaped)
            .is_subtype_of(&CoreType::from_julia_name("DenseArray{Int64, 2}")));
    }

    #[test]
    fn bitarray_family_subtyping_preserves_bool_rank_issue_5615() {
        assert!(CoreType::from_julia_name("BitVector")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector{Bool}")));
        assert!(CoreType::from_julia_name("BitMatrix")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix{Bool}")));
        assert!(CoreType::from_julia_name("BitArray{3}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray{Bool, 3}")));
        assert!(CoreType::from_julia_name("BitVector")
            .is_subtype_of(&CoreType::from_julia_name("BitArray")));
        assert!(CoreType::from_julia_name("BitVector")
            .is_subtype_of(&CoreType::from_julia_name("BitArray{1}")));

        assert!(!CoreType::from_julia_name("BitVector")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector{Any}")));
        assert!(!CoreType::from_julia_name("BitArray{3}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray{Bool, 2}")));
        assert!(!CoreType::from_julia_name("BitVector")
            .is_subtype_of(&CoreType::from_julia_name("DenseArray")));
        assert!(!CoreType::from_julia_name("Vector{Bool}")
            .is_subtype_of(&CoreType::from_julia_name("BitVector")));
    }

    #[test]
    fn abstract_lattice_subtyping_is_transitive_issue_5615() {
        assert!(CoreType::from_julia_name("Signed")
            .is_subtype_of(&CoreType::from_julia_name("Integer")));
        assert!(
            CoreType::from_julia_name("Signed").is_subtype_of(&CoreType::from_julia_name("Real"))
        );
        assert!(
            CoreType::from_julia_name("Signed").is_subtype_of(&CoreType::from_julia_name("Number"))
        );
        assert!(CoreType::from_julia_name("Core.Builtin")
            .is_subtype_of(&CoreType::from_julia_name("Function")));
        assert!(
            CoreType::from_julia_name("DataType").is_subtype_of(&CoreType::from_julia_name("Type"))
        );
        assert!(!CoreType::from_julia_name("Signed")
            .is_subtype_of(&CoreType::from_julia_name("AbstractFloat")));
        assert!(!CoreType::from_julia_name("AbstractVector")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix")));
        assert!(CoreType::from_julia_name("AbstractRange")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(CoreType::from_julia_name("AbstractRange")
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray")));
        assert!(CoreType::from_julia_name("AbstractUnitRange")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(CoreType::from_julia_name("UnitRange{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector{Int64}")));
        assert!(CoreType::from_julia_name("UnitRange{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray{Int64,1}")));
        assert!(!CoreType::from_julia_name("UnitRange{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector{Integer}")));
        assert!(!CoreType::from_julia_name("UnitRange{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("Array{Int64,1}")));
        assert!(CoreType::from_julia_name("LogRange{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector{Float64}")));
        assert!(CoreType::from_julia_name("LogRange{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractArray{Float64,1}")));
        assert!(!CoreType::from_julia_name("LogRange{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractRange{Float64}")));
    }

    #[test]
    fn parametric_name_parser_recognizes_builtin_type_forms() {
        assert_eq!(
            CoreType::from_julia_name("Union{Int64, String}"),
            CoreType::Union(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::String),
            ])
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{Int64, Real}"),
            CoreType::Tuple(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Abstract(CoreAbstract::Real),
            ])
        );
        assert_eq!(
            CoreType::from_julia_name("Type{Int64}"),
            CoreType::TypeOf(Box::new(CoreType::Primitive(CorePrimitive::Int64)))
        );
    }

    #[test]
    fn parametric_abstract_names_retain_invariant_params() {
        // A *parametric* abstract container spelling retains its invariant
        // element parameter as a `Struct` so subtyping can enforce it (Issues
        // #5047 / #5563 / #5564); only the bare, parameter-free spelling keeps
        // the covariant `Abstract` representation.
        assert_eq!(
            CoreType::from_julia_name("AbstractDict{String, Int64}"),
            CoreType::Struct {
                name: "AbstractDict".to_string(),
                params: vec![
                    CoreType::Primitive(CorePrimitive::String),
                    CoreType::Primitive(CorePrimitive::Int64),
                ],
            }
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractVector{Float64}"),
            CoreType::Struct {
                name: "AbstractVector".to_string(),
                params: vec![CoreType::Primitive(CorePrimitive::Float64)],
            }
        );
        // Bare spellings stay abstract.
        assert_eq!(
            CoreType::from_julia_name("AbstractDict"),
            CoreType::Abstract(CoreAbstract::AbstractDict)
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractVector"),
            CoreType::Abstract(CoreAbstract::AbstractVector)
        );
    }

    #[test]
    fn parametric_name_parser_recognizes_type_variables() {
        let core = CoreType::from_julia_name("Complex{T}");
        assert!(matches!(
            core,
            CoreType::Struct {
                ref params,
                ..
            } if matches!(params.as_slice(), [CoreType::TypeVar(var)] if var.name == "T")
        ));
        assert_eq!(core.specificity(), 4);
    }

    #[test]
    fn empty_tuple_type_matches_bare_tuple_parameter_issue_4739() {
        // `typeof(()) === Tuple{}` must stay in the Tuple family so a bare
        // `::Tuple` parameter wins over a generic untyped fallback. Regression
        // guard for the `show(io, ())` mis-dispatch (Issue #4739 / #4737).
        let empty = CoreType::from_julia_name("Tuple{}");
        assert_eq!(empty, CoreType::Tuple(vec![]));

        let bare_tuple = CoreType::from_julia_name("Tuple");
        // Empty tuple scores against `::Tuple` exactly like a non-empty tuple
        // does (the bare-family tier), and strictly better than the generic
        // untyped/`Any` fallback (which never reaches this arm).
        assert!(bare_tuple.dispatch_pattern_score(&empty) > 0);
        assert_eq!(
            bare_tuple.dispatch_pattern_score(&empty),
            bare_tuple.dispatch_pattern_score(&CoreType::from_julia_name("Tuple{Int64}")),
        );
    }

    #[test]
    fn dispatch_pattern_score_preserves_runtime_ordering() {
        assert_eq!(
            CoreType::from_julia_name("Rational{Int64}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Rational{Int64}")),
            4
        );
        assert_eq!(
            CoreType::from_julia_name("Rational{T}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Rational{Int64}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("TwoParamMatrixIssue{T}").dispatch_pattern_score(
                &CoreType::from_julia_name("TwoParamMatrixIssue{Float64, Vector{Float64}}")
            ),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("CovariantParamIssue{_<:Real}").dispatch_pattern_score(
                &CoreType::from_julia_name("CovariantParamIssue{Float64, Vector{Float64}}")
            ),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("CovariantParamIssue{_<:Real}").dispatch_pattern_score(
                &CoreType::from_julia_name("CovariantParamIssue{String, Vector{String}}")
            ),
            0
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{Any}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Tuple{Int64}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Rational")
                .dispatch_pattern_score(&CoreType::from_julia_name("Rational{Int64}")),
            2
        );
        assert_eq!(
            CoreType::from_julia_name("Array")
                .dispatch_pattern_score(&CoreType::from_julia_name("Vector{Int64}")),
            2
        );
        assert_eq!(
            CoreType::from_julia_name("Array{Int64}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Array{Int64, 1}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Array{Int64}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Vector{Int64}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Array{T}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Array{Int64, 1}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Array{T}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Vector{Int64}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Array{Real}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Array{Float64, 1}")),
            0
        );
        assert_eq!(
            CoreType::from_julia_name("Array{Real}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Vector{Float64}")),
            0
        );
        assert_eq!(
            CoreType::from_julia_name("Real")
                .dispatch_pattern_score(&CoreType::from_julia_name("Int64")),
            0
        );
        assert_eq!(
            CoreType::from_julia_name("Rational{Int64}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Rational{BigInt}")),
            0
        );
        assert_eq!(
            CoreType::from_julia_name("Matrix{_<:Integer}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Matrix{Int64}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Matrix{_<:Integer}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Matrix{Float64}")),
            0
        );
        assert_eq!(
            CoreType::from_julia_name("Type{Pair{K,V}}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Type{Pair{Int64,Int8}}")),
            3
        );
        assert_eq!(
            CoreType::from_julia_name("Type{Pair}")
                .dispatch_pattern_score(&CoreType::from_julia_name("Type{Pair{Int64,Int8}}")),
            2
        );
    }

    #[test]
    fn unionall_bridge_keeps_typevar_bound() {
        let ty = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: Some(Box::new("Real".to_string())),
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "T".to_string(),
                Some("Real".to_string()),
            )))),
        };
        let core = CoreType::from(&ty);
        assert!(matches!(core, CoreType::UnionAll { .. }));
    }

    #[test]
    fn primitive_subtyping_matches_julia_numeric_spine() {
        assert!(
            CoreType::from(&JuliaType::Bool).is_subtype_of(&CoreType::from(&JuliaType::Integer))
        );
        assert!(
            CoreType::from(&JuliaType::Int64).is_subtype_of(&CoreType::from(&JuliaType::Signed))
        );
        assert!(
            CoreType::from(&JuliaType::UInt64).is_subtype_of(&CoreType::from(&JuliaType::Unsigned))
        );
        assert!(
            CoreType::from(&JuliaType::Float64).is_subtype_of(&CoreType::from(&JuliaType::Real))
        );
        assert!(
            CoreType::from(&JuliaType::BigFloat).is_subtype_of(&CoreType::from(&JuliaType::Number))
        );
        assert!(!CoreType::from(&JuliaType::Float64)
            .is_subtype_of(&CoreType::from(&JuliaType::Integer)));
    }

    #[test]
    fn tuple_subtyping_is_covariant() {
        let tuple_int = CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]);
        let tuple_real = CoreType::Tuple(vec![CoreType::Abstract(CoreAbstract::Real)]);
        let tuple_string = CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::String)]);

        assert!(tuple_int.is_subtype_of(&tuple_real));
        assert!(!tuple_string.is_subtype_of(&tuple_real));
    }

    #[test]
    fn partial_parametric_struct_subtyping_matches_prefix_issue_8407() {
        assert!(CoreType::from_julia_name(
            "QuadGK.BatchIntegrand{Float64, Nothing, Vector{Float64}, Vector{Nothing}, typeof(f!)}"
        )
        .is_subtype_of(&CoreType::from_julia_name(
            "QuadGK.BatchIntegrand{Float64, Nothing}"
        )));
        assert!(!CoreType::from_julia_name(
            "QuadGK.BatchIntegrand{Float64, Float64, Vector{Float64}, Vector{Float64}, typeof(f!)}"
        )
        .is_subtype_of(&CoreType::from_julia_name(
            "QuadGK.BatchIntegrand{Float64, Nothing}"
        )));
    }

    #[test]
    fn named_tuple_subtyping_is_field_invariant_issue_5615() {
        let exact = CoreType::from_julia_name("@NamedTuple{a::Int64, b::String}");
        let same = CoreType::from_julia_name("@NamedTuple{a::Int, b::String}");
        let typelevel = CoreType::from_julia_name("NamedTuple{(:a, :b), Tuple{Int64, String}}");
        let wider = CoreType::from_julia_name("@NamedTuple{a::Integer, b::String}");
        let typelevel_wider =
            CoreType::from_julia_name("NamedTuple{(:a, :b), Tuple{Integer, String}}");
        let renamed = CoreType::from_julia_name("@NamedTuple{x::Int64, b::String}");
        let bare = CoreType::from_julia_name("NamedTuple");
        let names_only = CoreType::from_julia_name("NamedTuple{(:a, :b)}");
        let other_names = CoreType::from_julia_name("NamedTuple{(:x, :y)}");

        assert!(exact.is_subtype_of(&same));
        assert!(same.is_subtype_of(&exact));
        assert_eq!(exact, typelevel);
        assert!(!exact.is_subtype_of(&wider));
        assert!(!wider.is_subtype_of(&exact));
        assert!(!typelevel.is_subtype_of(&typelevel_wider));
        assert!(!exact.is_subtype_of(&renamed));
        assert!(exact.is_subtype_of(&bare));
        assert!(exact.is_subtype_of(&names_only));
        assert!(!exact.is_subtype_of(&other_names));
        assert!(!renamed.is_subtype_of(&names_only));
    }

    #[test]
    fn tuple_subtyping_supports_trailing_vararg() {
        let two_ints = CoreType::from_julia_name("Tuple{Int64, Int64}");
        let int_vararg = CoreType::from_julia_name("Tuple{Vararg{Integer}}");
        let fixed_then_vararg = CoreType::from_julia_name("Tuple{Int64, Vararg{AbstractString}}");

        assert!(two_ints.is_subtype_of(&int_vararg));
        assert!(CoreType::from_julia_name("Tuple{Int64, String, String}")
            .is_subtype_of(&fixed_then_vararg));
        assert!(!CoreType::from_julia_name("Tuple{Int64, String, Int64}")
            .is_subtype_of(&fixed_then_vararg));
    }

    #[test]
    fn unionall_rhs_binds_typevars_with_bounds() {
        let vector_of_t_integer = CoreType::UnionAll {
            var: CoreTypeVar {
                name: "T".to_string(),
                lower_bound: None,
                upper_bound: Some(Box::new(CoreType::from_julia_name("Integer"))),
            },
            body: Box::new(CoreType::Struct {
                name: "Vector".to_string(),
                params: vec![CoreType::TypeVar(CoreTypeVar {
                    name: "T".to_string(),
                    lower_bound: None,
                    upper_bound: None,
                })],
            }),
        };

        assert!(CoreType::from_julia_name("Vector{Int64}").is_subtype_of(&vector_of_t_integer));
        assert!(CoreType::from_julia_name("Vector{BigInt}").is_subtype_of(&vector_of_t_integer));
        assert!(!CoreType::from_julia_name("Vector{Real}").is_subtype_of(&vector_of_t_integer));
    }

    #[test]
    fn diagonal_typevar_tuple_patterns_require_concrete_reuse() {
        let tuple_t_t = CoreType::UnionAll {
            var: CoreTypeVar {
                name: "T".to_string(),
                lower_bound: None,
                upper_bound: None,
            },
            body: Box::new(CoreType::Tuple(vec![
                CoreType::TypeVar(CoreTypeVar {
                    name: "T".to_string(),
                    lower_bound: None,
                    upper_bound: None,
                }),
                CoreType::TypeVar(CoreTypeVar {
                    name: "T".to_string(),
                    lower_bound: None,
                    upper_bound: None,
                }),
            ])),
        };

        assert!(CoreType::from_julia_name("Tuple{Int64, Int64}").is_subtype_of(&tuple_t_t));
        assert!(!CoreType::from_julia_name("Tuple{Int64, Float64}").is_subtype_of(&tuple_t_t));
        assert!(!CoreType::from_julia_name("Tuple{Real, Real}").is_subtype_of(&tuple_t_t));
    }

    // Issue #5047: `from_julia_name` re-parses the rendered `Body where V`
    // surface form (the `JuliaType::name()` of a value-position UnionAll, #5569)
    // into a `CoreType::UnionAll`, so the runtime `<:` exists-right solver fires.
    #[test]
    fn from_julia_name_parses_where_into_unionall() {
        let unbounded = CoreType::from_julia_name("Tuple{T, T} where T");
        assert!(matches!(unbounded, CoreType::UnionAll { .. }));

        let bounded = CoreType::from_julia_name("Vector{T} where T<:Real");
        let CoreType::UnionAll { var, .. } = &bounded else {
            panic!("expected UnionAll, got {bounded:?}");
        };
        assert_eq!(var.name, "T");
        assert_eq!(
            var.upper_bound.as_deref(),
            Some(&CoreType::Abstract(CoreAbstract::Real))
        );

        // Several variables render as a right-nested chain `Body where S where T`
        // (outermost variable last); both layers must peel into nested UnionAlls.
        let multi = CoreType::from_julia_name("Tuple{T, S} where S where T");
        let CoreType::UnionAll { var: outer, body } = &multi else {
            panic!("expected outer UnionAll, got {multi:?}");
        };
        assert_eq!(outer.name, "T");
        assert!(matches!(body.as_ref(), CoreType::UnionAll { .. }));
    }

    #[test]
    fn where_surface_subtyping_round_trips_through_from_julia_name() {
        let st = |l: &str, r: &str| {
            CoreType::from_julia_name(l).is_subtype_of(&CoreType::from_julia_name(r))
        };
        // diagonal rule
        assert!(st("Tuple{Int64, Int64}", "Tuple{T, T} where T"));
        assert!(!st("Tuple{Int64, String}", "Tuple{T, T} where T"));
        // bounds
        assert!(st("Vector{Int64}", "Vector{T} where T<:Real"));
        assert!(st("Vector{Int64}", "Vector{T} where T<:S where S<:Real"));
        assert!(!st("Vector{String}", "Vector{T} where T<:S where S<:Real"));
        assert!(st("Vector{Int64}", "Vector{T} where {S<:Real, T<:S<:Real}"));
        assert!(!st(
            "Vector{String}",
            "Vector{T} where {S<:Real, T<:S<:Real}"
        ));
        assert!(!st("Vector{String}", "Vector{T} where T<:Real"));
        assert!(st("Tuple{Int64, Int64}", "Tuple{T, T} where T<:Integer"));
        assert!(!st(
            "Tuple{Int64, Int64}",
            "Tuple{T, T} where T<:AbstractString"
        ));
        // multi-var + container shapes
        assert!(st("Tuple{Int64, Float64}", "Tuple{T, S} where S where T"));
        assert!(st("Dict{String, Int64}", "Dict{K, V} where V where K"));
        assert!(st("Dict{String, Int64}", "AbstractDict{String, T} where T"));
        assert!(!st(
            "Dict{String, Int64}",
            "AbstractDict{Symbol, T} where T"
        ));
        assert!(st("Set{Int64}", "AbstractSet{T} where T"));
        assert!(!st("Set{String}", "AbstractSet{T} where T<:Real"));
        assert!(st("RefValue{Int64}", "Ref{T} where T"));
        assert!(!st("RefValue{String}", "Ref{T} where T<:Real"));
        assert!(st("Tuple{Int64, Int64, Int64}", "Tuple{Vararg{T}} where T"));
    }

    #[test]
    fn forall_left_introduces_rigid_var_within_bounds_issue_5047() {
        let st = |l: &str, r: &str| {
            CoreType::from_julia_name(l).is_subtype_of(&CoreType::from_julia_name(r))
        };
        // Bare bounded forall-left vs a plain (non-where) supertype.
        assert!(st("Vector{T} where T", "AbstractVector"));
        assert!(!st("Vector{T} where T", "Vector{Int64}"));
        assert!(st("Vector{T} where T<:Real", "AbstractVector"));
        assert!(st("Vector{T} where T<:Integer", "AbstractVector"));
        assert!(st("Tuple{T, T} where T", "Tuple"));
        assert!(!st("Tuple{T} where T", "Tuple{Int64}"));

        // Forall-LEFT + exists-RIGHT alternation: the rigid LHS var flows into
        // the RHS UnionAll pattern, so the declared bound is what is checked.
        assert!(st("Tuple{T} where T<:Integer", "Tuple{S} where S<:Real"));
        assert!(!st("Tuple{T} where T<:Real", "Tuple{S} where S<:Integer"));
        // Invariant element under alternation: S:=T (T<:Integer<:Real) works.
        assert!(st("Vector{T} where T<:Integer", "Vector{S} where S<:Real"));
        // Diagonal both sides: a single rigid var is a valid diagonal value, so
        // S:=T succeeds; two DIFFERENT rigid vars must NOT collapse onto one S.
        assert!(st(
            "Tuple{T, T} where T<:Integer",
            "Tuple{S, S} where S<:Real"
        ));
        assert!(!st(
            "Tuple{T, U} where U<:Integer where T<:Integer",
            "Tuple{S, S} where S<:Real"
        ));

        // A bare ABSTRACT diagonal actual is NOT concrete-diagonal, so the
        // exists-right diagonal rule still rejects it (must-stay-correct).
        assert!(!st("Tuple{Real, Real}", "Tuple{T, T} where T"));
        assert!(!st("Tuple{Int64, Real}", "Tuple{T, T} where T"));
    }

    #[test]
    fn unionall_tuple_matches_reshapedarray_method_signature_issue_5915() {
        let actual = CoreType::from_julia_name(
            "Tuple{ReshapedArray{Int64, 1, SubArray{Int64, 2, Matrix{Int64}, Tuple{UnitRange{Int64}, UnitRange{Int64}}, false}, Tuple{}}}",
        );
        let pattern =
            CoreType::from_julia_name("Tuple{ReshapedArray{T, 1, P, MI}} where MI where P where T");
        assert!(
            actual.is_subtype_of(&pattern),
            "1-D ReshapedArray actual must match collect(::ReshapedArray{{T,1,P,MI}})"
        );
    }

    #[test]
    fn split_trailing_where_ignores_nested_keyword() {
        // Top-level ` where ` splits at the rightmost occurrence.
        assert_eq!(
            split_trailing_where("Tuple{T, T} where T"),
            Some(("Tuple{T, T}", "T"))
        );
        assert_eq!(
            split_trailing_where("Tuple{T, S} where S where T"),
            Some(("Tuple{T, S} where S", "T"))
        );
        // A ` where ` nested inside braces is NOT a top-level split point.
        assert_eq!(split_trailing_where("Tuple{Foo where Bar}"), None);
        assert_eq!(split_trailing_where("Vector{Int64}"), None);
    }

    #[test]
    fn split_top_level_subtype_bound_ignores_nested_operators_issue_8383() {
        assert_eq!(
            split_top_level_subtype_bound("<:Tuple{Number, Number}"),
            Some(("", "Tuple{Number, Number}"))
        );
        assert_eq!(
            split_top_level_subtype_bound("T<:AbstractVector{Float64}"),
            Some(("T", "AbstractVector{Float64}"))
        );
        assert_eq!(
            split_top_level_subtype_bound("Tuple{<:Number, <:Number}"),
            None
        );
        assert_eq!(split_top_level_subtype_bound("typeof(<:)"), None);
    }

    #[test]
    fn abstract_vector_covariant_tuple_bound_issue_8383() {
        let st = |l: &str, r: &str| {
            CoreType::from_julia_name(l).is_subtype_of(&CoreType::from_julia_name(r))
        };
        assert!(st(
            "Vector{Tuple{Float64, Float64}}",
            "AbstractVector{<:Tuple{Number, Number}}"
        ));
        assert!(st(
            "Vector{Tuple{Float64, Float64}}",
            "AbstractVector{<:Tuple{<:Number, <:Number}}"
        ));
    }

    #[test]
    fn parse_where_var_handles_bound_spellings() {
        let bare = parse_where_var("T");
        assert_eq!(bare.name, "T");
        assert!(bare.upper_bound.is_none() && bare.lower_bound.is_none());

        let upper = parse_where_var("T<:Real");
        assert_eq!(upper.name, "T");
        assert_eq!(
            upper.upper_bound.as_deref(),
            Some(&CoreType::Abstract(CoreAbstract::Real))
        );

        // brace-wrapped single-var clause
        let braced = parse_where_var("{S<:Integer}");
        assert_eq!(braced.name, "S");
        assert_eq!(
            braced.upper_bound.as_deref(),
            Some(&CoreType::Abstract(CoreAbstract::Integer))
        );
    }

    #[test]
    fn vararg_typevar_patterns_share_tuple_semantics() {
        let tuple_vararg_t = CoreType::UnionAll {
            var: CoreTypeVar {
                name: "T".to_string(),
                lower_bound: None,
                upper_bound: None,
            },
            body: Box::new(CoreType::from_julia_name("Tuple{Vararg{T}}")),
        };

        assert!(
            CoreType::from_julia_name("Tuple{Int64, Int64, Int64}").is_subtype_of(&tuple_vararg_t)
        );
        assert!(!CoreType::from_julia_name("Tuple{Int64, Float64}").is_subtype_of(&tuple_vararg_t));
        assert!(CoreType::from_julia_name("Tuple{Int64, Vararg{Int64}}")
            .is_subtype_of(&CoreType::from_julia_name("Tuple{Integer, Vararg{Real}}")));
    }

    #[test]
    fn parametric_structs_are_invariant_but_match_bare_base() {
        let vec_i64 = CoreType::from_julia_name("Vector{Int64}");
        let vec_real = CoreType::from_julia_name("Vector{Real}");
        let bare_vec = CoreType::Struct {
            name: "Vector".to_string(),
            params: vec![],
        };
        let foo_i64 = CoreType::from_julia_name("Foo{Int64}");
        let bare_foo = CoreType::from_julia_name("Foo");
        let tuple_foo_i64 = CoreType::from_julia_name("Tuple{Foo{Int64}}");
        let tuple_bare_foo = CoreType::from_julia_name("Tuple{Foo}");
        let irrational_pi = CoreType::from_julia_name("Irrational{:π}");
        let bare_irrational = CoreType::from_julia_name("Irrational");

        assert!(!vec_i64.is_subtype_of(&vec_real));
        assert!(vec_i64.is_subtype_of(&bare_vec));
        assert!(foo_i64.is_subtype_of(&bare_foo));
        assert!(tuple_foo_i64.is_subtype_of(&tuple_bare_foo));
        assert!(vec_i64.is_subtype_of(&CoreType::Abstract(CoreAbstract::AbstractArray)));
        assert!(irrational_pi.is_subtype_of(&bare_irrational));
        assert!(irrational_pi.is_builtin_concrete_datatype());
        assert!(!bare_irrational.is_builtin_concrete_datatype());
    }

    #[test]
    fn fix_partial_application_types_match_bare_family_issue_5127() {
        let fix1 = CoreType::from_julia_name("Fix1{typeof(-), Int64}");
        let fix2 = CoreType::from_julia_name("Fix2{typeof(^), Int64}");

        assert!(fix1.is_subtype_of(&CoreType::from_julia_name("Fix1")));
        assert!(fix2.is_subtype_of(&CoreType::from_julia_name("Fix2")));
        assert!(fix1.is_builtin_concrete_datatype());
        assert!(fix2.is_builtin_concrete_datatype());
    }

    #[test]
    fn collection_and_io_hierarchy_matches_boot_abstracts() {
        assert!(CoreType::from_julia_name("Vector{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractVector")));
        assert!(CoreType::from_julia_name("Matrix{Float64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractMatrix")));
        assert!(CoreType::from_julia_name("Dict{String, Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractDict")));
        assert!(CoreType::from_julia_name("Set{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractSet")));
        assert!(CoreType::from_julia_name("UnitRange{Int64}")
            .is_subtype_of(&CoreType::from_julia_name("AbstractUnitRange")));
        assert!(
            CoreType::from_julia_name("IOBuffer").is_subtype_of(&CoreType::from_julia_name("IO"))
        );
    }

    // Issue #5129: Core.Builtin vs generic function type distinction.
    #[test]
    fn core_builtin_singleton_type_distinction_issue_5129() {
        let builtin = CoreType::from_julia_name("Core.Builtin");
        let func = CoreType::from_julia_name("Function");

        // Genuine built-in function singletons are <: Core.Builtin and <: Function.
        for nm in [
            "typeof(===)",
            "typeof(isa)",
            "typeof(typeof)",
            "typeof(<:)",
            "typeof(tuple)",
            "typeof(getfield)",
        ] {
            let t = CoreType::from_julia_name(nm);
            assert!(t.is_subtype_of(&builtin), "{nm} <: Core.Builtin");
            assert!(t.is_subtype_of(&func), "{nm} <: Function");
        }

        // Generic / user function singletons are <: Function but NOT <: Core.Builtin.
        for nm in [
            "typeof(+)",
            "typeof(sin)",
            "typeof(map)",
            "typeof(myuserfn)",
        ] {
            let t = CoreType::from_julia_name(nm);
            assert!(t.is_subtype_of(&func), "{nm} <: Function");
            assert!(!t.is_subtype_of(&builtin), "{nm} NOT <: Core.Builtin");
        }

        // Core.Builtin <: Function, but Function is not <: Core.Builtin; they differ.
        assert!(builtin.is_subtype_of(&func));
        assert!(!func.is_subtype_of(&builtin));
        assert_ne!(builtin, func);
        // The bare `Builtin` alias resolves to the same abstract type.
        assert_eq!(CoreType::from_julia_name("Builtin"), builtin);

        // Name registry sanity.
        assert!(is_core_builtin_function_name("==="));
        assert!(is_core_builtin_function_name("getfield"));
        assert!(!is_core_builtin_function_name("+"));
        assert!(!is_core_builtin_function_name("sin"));
        assert!(is_core_builtin_singleton_type_name("typeof(===)"));
        assert!(!is_core_builtin_singleton_type_name("typeof(+)"));
        assert!(!is_core_builtin_singleton_type_name("Function"));
    }

    #[test]
    fn user_abstract_bound_resolves_through_explicit_hierarchy() {
        // Issue #5383: a user struct (`Dog`) and a user abstract type (`Animal`)
        // both lower to `CoreType::Named`, so `Named <: Named` must consult the
        // supplied hierarchy instead of being hardcoded `false`.
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Mammal", Some("Animal".to_string()), Vec::new());
        hierarchy.insert("Dog", Some("Mammal".to_string()), Vec::new());
        hierarchy.insert("Cat", Some("Animal".to_string()), Vec::new());
        hierarchy.insert("Box", Some("Animal".to_string()), Vec::new());
        hierarchy.insert("Rock", None, Vec::new());

        let animal = CoreType::Named("Animal".to_string());
        let mammal = CoreType::Named("Mammal".to_string());
        let named = |n: &str| CoreType::Named(n.to_string());

        // Direct subtype.
        assert!(named("Cat").is_subtype_of_with_hierarchy(&animal, &hierarchy));
        // Transitive subtype through an intermediate user abstract type.
        assert!(named("Dog").is_subtype_of_with_hierarchy(&animal, &hierarchy));
        assert!(named("Dog").is_subtype_of_with_hierarchy(&mammal, &hierarchy));
        // Parametric user structs lower to `Struct`; their declared parent is
        // still keyed by base name in the hierarchy.
        assert!(CoreType::from_julia_name("Box{Int64}")
            .is_subtype_of_with_hierarchy(&animal, &hierarchy));
        // Reflexivity (handled by the early `self == other` check).
        assert!(animal.is_subtype_of(&animal));

        // Non-subtypes: unrelated type, the reverse relation, and a sibling.
        assert!(!named("Rock").is_subtype_of_with_hierarchy(&animal, &hierarchy));
        assert!(!animal.is_subtype_of_with_hierarchy(&named("Dog"), &hierarchy));
        assert!(!named("Cat").is_subtype_of_with_hierarchy(&mammal, &hierarchy));
        assert!(!CoreType::from_julia_name("Box{Int64}")
            .is_subtype_of_with_hierarchy(&mammal, &hierarchy));
    }

    #[test]
    fn parametric_parent_hierarchy_substitutes_declared_typevars() {
        // Issues #5615/#5882: `Pairs{K,V,I,A}` declares
        // `AbstractDict{K,V}` as its parent. The hierarchy must preserve the
        // parent template and substitute the concrete child parameters instead
        // of reducing the parent to bare `AbstractDict`.
        let mut parents = std::collections::HashMap::new();
        parents.insert(
            "Pairs".to_string(),
            (
                Some("AbstractDict{K,V}".to_string()),
                vec![
                    "K".to_string(),
                    "V".to_string(),
                    "I".to_string(),
                    "A".to_string(),
                ],
            ),
        );
        let hierarchy = StructHierarchy::from_parent_map(&parents);

        let pairs = CoreType::from_julia_name("Pairs{Symbol,Int64,Any,Any}");

        assert!(pairs
            .is_subtype_of_with_hierarchy(&CoreType::from_julia_name("AbstractDict"), &hierarchy));
        assert!(pairs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractDict{Symbol,Int64}"),
            &hierarchy
        ));
        assert!(!pairs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractDict{Symbol,Any}"),
            &hierarchy
        ));
        assert!(!pairs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractDict{Any,Int64}"),
            &hierarchy
        ));
    }

    #[test]
    fn subtype_with_explicit_hierarchy_uses_supplied_parents() {
        let mut parents = std::collections::HashMap::new();
        parents.insert(
            "Complex".to_string(),
            (Some("Number".to_string()), vec!["T".to_string()]),
        );
        parents.insert("Mammal".to_string(), (Some("Animal".to_string()), vec![]));
        parents.insert("Dog".to_string(), (Some("Mammal".to_string()), vec![]));
        parents.insert("Box".to_string(), (Some("Animal".to_string()), vec![]));
        parents.insert(
            "Pairs".to_string(),
            (
                Some("AbstractDict{K,V}".to_string()),
                vec![
                    "K".to_string(),
                    "V".to_string(),
                    "I".to_string(),
                    "A".to_string(),
                ],
            ),
        );
        let hierarchy = StructHierarchy::from_parent_map(&parents);

        let named = |n: &str| CoreType::Named(n.to_string());
        let animal = named("Animal");
        assert!(!named("Dog").is_subtype_of(&animal));
        assert!(named("Dog").is_subtype_of_with_hierarchy(&animal, &hierarchy));
        assert!(CoreType::from_julia_name("Box{Int64}")
            .is_subtype_of_with_hierarchy(&animal, &hierarchy));

        let complex = CoreType::from_julia_name("Complex");
        assert_eq!(complex.direct_builtin_supertype_name(), None);
        assert_eq!(
            complex.direct_builtin_supertype_name_with_hierarchy(&hierarchy),
            Some("Number")
        );

        let pairs = CoreType::from_julia_name("Pairs{Symbol,Int64,Any,Any}");
        assert!(pairs
            .is_subtype_of_with_hierarchy(&CoreType::from_julia_name("AbstractDict"), &hierarchy));
        assert!(pairs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractDict{Symbol,Int64}"),
            &hierarchy
        ));
        assert!(!pairs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractDict{Symbol,Any}"),
            &hierarchy
        ));
    }

    #[test]
    fn unionall_pattern_with_explicit_hierarchy_uses_supplied_parents() {
        let mut parents = std::collections::HashMap::new();
        parents.insert("Mammal".to_string(), (Some("Animal".to_string()), vec![]));
        parents.insert("Dog".to_string(), (Some("Mammal".to_string()), vec![]));
        let hierarchy = StructHierarchy::from_parent_map(&parents);

        let dog = CoreType::Named("Dog".to_string());
        let typevar = || {
            CoreType::TypeVar(CoreTypeVar {
                name: "T".to_string(),
                lower_bound: None,
                upper_bound: None,
            })
        };
        let bounded_var = CoreTypeVar {
            name: "T".to_string(),
            lower_bound: None,
            upper_bound: Some(Box::new(CoreType::Named("Animal".to_string()))),
        };

        let tuple_actual = CoreType::Tuple(vec![dog.clone()]);
        let tuple_pattern = CoreType::UnionAll {
            var: bounded_var.clone(),
            body: Box::new(CoreType::Tuple(vec![typevar()])),
        };
        assert!(!tuple_actual.is_subtype_of(&tuple_pattern));
        assert!(tuple_actual.is_subtype_of_with_hierarchy(&tuple_pattern, &hierarchy));

        let array_actual = CoreType::Struct {
            name: "Vector".to_string(),
            params: vec![dog],
        };
        let array_pattern = CoreType::UnionAll {
            var: bounded_var,
            body: Box::new(CoreType::Struct {
                name: "AbstractVector".to_string(),
                params: vec![typevar()],
            }),
        };
        assert!(!array_actual.is_subtype_of(&array_pattern));
        assert!(array_actual.is_subtype_of_with_hierarchy(&array_pattern, &hierarchy));
    }

    #[test]
    fn registered_parent_family_decision_tracks_known_and_unknown_families() {
        let mut parents = std::collections::HashMap::new();
        parents.insert(
            "Pairs".to_string(),
            (
                Some("AbstractDict{K,V}".to_string()),
                vec![
                    "K".to_string(),
                    "V".to_string(),
                    "I".to_string(),
                    "A".to_string(),
                ],
            ),
        );
        let hierarchy = StructHierarchy::from_parent_map(&parents);

        assert_eq!(
            registered_struct_parent_family_decision_in(&hierarchy, "Pairs", "AbstractDict"),
            Some(true)
        );
        assert_eq!(
            registered_struct_parent_family_decision_in(&hierarchy, "Pairs", "AbstractSet"),
            Some(false)
        );
        assert_eq!(
            registered_struct_parent_family_decision_in(&hierarchy, "Unregistered", "AbstractDict"),
            None
        );
    }

    #[test]
    fn subtype_resolves_int_uint_word_aliases() {
        // Issue #5047: a bare `Int`/`UInt` parses to an opaque `Named` (it has no
        // arm in `from_julia_name`), but the SUBTYPE relation must treat it as
        // its 64-bit word primitive (`Int === Int64`, `UInt === UInt64`). This is
        // what lets a bound check against an alias-spelled (e.g. nested) parameter
        // succeed; `from_julia_name` keeps the `Named` spelling so unrelated
        // representations are unchanged.
        let int_named = CoreType::Named("Int".to_string());
        let uint_named = CoreType::Named("UInt".to_string());
        assert!(int_named.is_subtype_of(&CoreType::from_julia_name("Integer")));
        assert!(int_named.is_subtype_of(&CoreType::from_julia_name("Real")));
        assert!(int_named.is_subtype_of(&CoreType::from_julia_name("Int64")));
        assert!(CoreType::from_julia_name("Int64").is_subtype_of(&int_named));
        assert!(uint_named.is_subtype_of(&CoreType::from_julia_name("Unsigned")));
        assert!(uint_named.is_subtype_of(&CoreType::from_julia_name("Integer")));
        // Negative: the alias is NOT a subtype of an unrelated abstract.
        assert!(!int_named.is_subtype_of(&CoreType::from_julia_name("AbstractFloat")));
        assert!(!uint_named.is_subtype_of(&CoreType::from_julia_name("Signed")));
    }

    #[test]
    fn module_qualified_parametric_struct_subtypes_bare_family_issue_6117() {
        let actual = CoreType::from_julia_name("LinearAlgebra.Diagonal{Float64}");

        assert!(actual.is_subtype_of(&CoreType::from_julia_name("Diagonal")));
        assert!(actual.is_subtype_of(&CoreType::from_julia_name("Diagonal{Float64}")));
        assert!(actual.is_subtype_of(&CoreType::Named("Diagonal".to_string())));
        assert!(!actual.is_subtype_of(&CoreType::from_julia_name("Diagonal{Int64}")));
    }

    #[test]
    fn nested_alias_param_satisfies_bounded_unionall_pattern() {
        // Issue #5047: `Box{Box{Int}} <: (Box{Box{T}} where T<:Integer)` is true
        // upstream (T=Int64, Int64<:Integer). The nested parameter renders with
        // the `Int` alias, so the exists-right matcher must resolve `Int` to
        // `Int64` to run the bound check. Box has no supplied parent, so this
        // exercises only the structured matcher + alias resolution.
        let actual = CoreType::from_julia_name("Box{Box{Int}}");
        let bounded = CoreType::from_julia_name("Box{Box{T}} where T<:Integer");
        assert!(actual.is_subtype_of(&bounded));

        // A non-Integer nested element must still be rejected.
        let str_actual = CoreType::from_julia_name("Box{Box{String}}");
        assert!(!str_actual.is_subtype_of(&bounded));

        // Unbounded `where T` accepts any nested element.
        let unbounded = CoreType::from_julia_name("Box{Box{T}} where T");
        assert!(actual.is_subtype_of(&unbounded));
        assert!(str_actual.is_subtype_of(&unbounded));
    }

    #[test]
    fn canonicalize_signature_for_dedup_collapses_single_use_covariant_typevar() {
        let tv = |name: &str, ub: Option<CoreType>| {
            CoreType::TypeVar(CoreTypeVar {
                name: name.to_string(),
                lower_bound: None,
                upper_bound: ub.map(Box::new),
            })
        };
        let number = CoreType::Abstract(CoreAbstract::Number);
        let unionall = |var: CoreTypeVar, body: CoreType| CoreType::UnionAll {
            var,
            body: Box::new(body),
        };
        let var = |name: &str, ub: Option<CoreType>| CoreTypeVar {
            name: name.to_string(),
            lower_bound: None,
            upper_bound: ub.map(Box::new),
        };

        // `Tuple{T} where T<:Number` collapses to `Tuple{Number}` — the bound is
        // taken from the peeled `where` var even when the body element carries no
        // inline bound (the bare `Struct("T")` parameter spelling).
        let bounded = unionall(
            var("T", Some(number.clone())),
            CoreType::Tuple(vec![tv("T", None)]),
        );
        assert_eq!(
            bounded.canonicalize_signature_for_dedup(),
            CoreType::Tuple(vec![number.clone()])
        );
        // An unbounded single-use var collapses to `Any` (`f(x::T) where T` == `f(x)`).
        let unbounded = unionall(var("T", None), CoreType::Tuple(vec![tv("T", None)]));
        assert_eq!(
            unbounded.canonicalize_signature_for_dedup(),
            CoreType::Tuple(vec![CoreType::Any])
        );

        // Diagonal use (`f(x::T, y::T)`) is preserved — it is NOT `Tuple{Number, Number}`.
        let diagonal = unionall(
            var("T", Some(number.clone())),
            CoreType::Tuple(vec![tv("T", None), tv("T", None)]),
        );
        assert_eq!(diagonal.canonicalize_signature_for_dedup(), diagonal);

        // Invariant-nested use (`Vector{T}`) is preserved — `Vector{T} where
        // T<:Number != Vector{Number}`, so the type variable stays.
        let nested = unionall(
            var("T", Some(number)),
            CoreType::Tuple(vec![CoreType::Struct {
                name: "Vector".to_string(),
                params: vec![tv("T", None)],
            }]),
        );
        assert_eq!(nested.canonicalize_signature_for_dedup(), nested);
    }

    #[test]
    fn direct_builtin_supertype_names_cover_reflection_families() {
        assert_eq!(
            CoreType::from_julia_name("Int64").direct_builtin_supertype_name(),
            Some("Signed")
        );
        assert_eq!(
            CoreType::from_julia_name("Vector").direct_builtin_supertype_name(),
            Some("DenseArray")
        );
        assert_eq!(
            CoreType::from_julia_name("BitVector").direct_builtin_supertype_name(),
            Some("AbstractVector")
        );
        assert_eq!(
            CoreType::from_julia_name("BitArray{3}").direct_builtin_supertype_name(),
            Some("BitArray")
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractVector").direct_builtin_supertype_name(),
            Some("AbstractArray")
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractRange").direct_builtin_supertype_name(),
            Some("AbstractVector")
        );
        assert_eq!(
            CoreType::from_julia_name("Vector{Int64}").direct_builtin_supertype_name(),
            Some("Vector")
        );
        assert_eq!(
            CoreType::from_julia_name("UnitRange").direct_builtin_supertype_name(),
            Some("AbstractUnitRange")
        );
        assert_eq!(
            CoreType::from_julia_name("StepRangeLen{Float64, Int64}")
                .direct_builtin_supertype_name(),
            Some("AbstractRange")
        );
        assert_eq!(
            CoreType::from_julia_name("LogRange{Float64}").direct_builtin_supertype_name(),
            Some("AbstractVector")
        );
        assert_eq!(
            CoreType::from_julia_name("IOBuffer").direct_builtin_supertype_name(),
            Some("IO")
        );
        assert_eq!(
            CoreType::from_julia_name("Foo").direct_builtin_supertype_name(),
            None
        );
    }

    #[test]
    fn builtin_supertype_chain_names_walk_direct_parents() {
        assert_eq!(
            CoreType::from_julia_name("Int64").builtin_supertype_chain_names(),
            Some(vec!["Int64", "Signed", "Integer", "Real", "Number", "Any"])
        );
        assert_eq!(
            CoreType::from_julia_name("Vector{Int64}").builtin_supertype_chain_names(),
            Some(vec!["Vector", "DenseArray", "AbstractArray", "Any"])
        );
        assert_eq!(
            CoreType::from_julia_name("UnitRange{Int64}").builtin_supertype_chain_names(),
            Some(vec![
                "UnitRange",
                "AbstractRange",
                "AbstractVector",
                "AbstractArray",
                "Any"
            ])
        );
        assert_eq!(
            CoreType::from_julia_name("Foo").builtin_supertype_chain_names(),
            None
        );
    }

    #[test]
    fn direct_builtin_subtype_names_follow_shared_parents() {
        assert_eq!(
            CoreType::from_julia_name("Signed").direct_builtin_subtype_names(),
            Some(vec!["Int8", "Int16", "Int32", "Int64", "Int128", "BigInt"])
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractFloat").direct_builtin_subtype_names(),
            Some(vec!["Float16", "Float32", "Float64", "BigFloat"])
        );
        assert_eq!(
            CoreType::from_julia_name("DenseArray").direct_builtin_subtype_names(),
            Some(vec!["Array", "Vector", "Matrix"])
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractVector").direct_builtin_subtype_names(),
            Some(vec!["AbstractRange", "LogRange", "BitVector"])
        );
        assert_eq!(
            CoreType::from_julia_name("AbstractMatrix").direct_builtin_subtype_names(),
            Some(vec!["BitMatrix"])
        );
        assert_eq!(
            CoreType::from_julia_name("Type").direct_builtin_subtype_names(),
            Some(vec!["DataType"])
        );
        assert_eq!(
            CoreType::from_julia_name("Foo").direct_builtin_subtype_names(),
            None
        );
    }

    #[test]
    fn type_intersection_distributes_over_union() {
        let numeric_union = CoreType::Union(vec![
            CoreType::Primitive(CorePrimitive::Int64),
            CoreType::Primitive(CorePrimitive::String),
        ]);
        let real = CoreType::Abstract(CoreAbstract::Real);

        assert_eq!(
            numeric_union.type_intersect(&real),
            CoreType::Primitive(CorePrimitive::Int64)
        );
    }

    #[test]
    fn type_intersection_handles_tuple_elements_and_typeof() {
        assert_eq!(
            CoreType::from_julia_name("Tuple{Union{Int64, String}, Float64}")
                .type_intersect(&CoreType::from_julia_name("Tuple{Integer, Real}")),
            CoreType::from_julia_name("Tuple{Int64, Float64}")
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{String}")
                .type_intersect(&CoreType::from_julia_name("Tuple{Real}")),
            CoreType::Bottom
        );
        assert_eq!(
            CoreType::from_julia_name("Type{Union{Int64, String}}")
                .type_intersect(&CoreType::from_julia_name("Type{Integer}")),
            CoreType::from_julia_name("Type{Int64}")
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{Int64, Vararg{Int64}}")
                .type_intersect(&CoreType::from_julia_name("Tuple{Integer, Vararg{Real}}")),
            CoreType::from_julia_name("Tuple{Int64, Vararg{Int64}}")
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{T, T} where T")
                .type_intersect(&CoreType::from_julia_name("Tuple{Int64, Float64}")),
            CoreType::Bottom
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{T, T} where T")
                .type_intersect(&CoreType::from_julia_name("Tuple{Int64, Real}")),
            CoreType::from_julia_name("Tuple{Int64, Int64}")
        );
        assert_eq!(
            CoreType::from_julia_name("Tuple{T, T} where T")
                .type_intersect(&CoreType::from_julia_name("Tuple{Real, Number}")),
            CoreType::from_julia_name("Tuple{T, T} where T<:Real")
        );
    }

    #[test]
    fn to_julia_name_preserves_structured_type_results() {
        for name in [
            "Int64",
            "Tuple{Int64, Float64}",
            "Union{Int64, String}",
            "Type{Int64}",
            "Dict{String, Vector{Int64}}",
            "Vararg{Integer}",
            "@NamedTuple{a::Int64, b}",
        ] {
            assert_eq!(CoreType::from_julia_name(name).to_julia_name(), name);
        }
        assert_eq!(
            CoreType::from_julia_name("NamedTuple{(:a, :b), Tuple{Int64, Any}}").to_julia_name(),
            "@NamedTuple{a::Int64, b}"
        );
    }

    #[test]
    fn type_parameters_preserve_structured_params() {
        assert_eq!(
            CoreType::from_julia_name("Dict{String, Vector{Int64}}")
                .type_parameters()
                .into_iter()
                .map(|param| param.to_julia_name())
                .collect::<Vec<_>>(),
            vec!["String", "Vector{Int64}"]
        );

        assert_eq!(
            CoreType::from_julia_name("Array{Int64, 1}")
                .type_parameters()
                .into_iter()
                .map(|param| param.to_julia_name())
                .collect::<Vec<_>>(),
            vec!["Int64", "1"]
        );

        assert_eq!(
            CoreType::from_julia_name("Tuple{Int64, Float64}")
                .type_parameters()
                .into_iter()
                .map(|param| param.to_julia_name())
                .collect::<Vec<_>>(),
            vec!["Int64", "Float64"]
        );
    }

    #[test]
    fn builtin_field_metadata_matches_ast_types() {
        let metadata = CoreType::from_julia_name("LineNumberNode")
            .builtin_field_metadata()
            .expect("LineNumberNode metadata");
        assert_eq!(
            metadata
                .iter()
                .map(|(name, ty)| (*name, ty.to_julia_name()))
                .collect::<Vec<_>>(),
            vec![
                ("line", "Int64".to_string()),
                ("file", "Union{Nothing, Symbol}".to_string())
            ]
        );

        let metadata = CoreType::from_julia_name("GlobalRef")
            .builtin_field_metadata()
            .expect("GlobalRef metadata");
        assert_eq!(
            metadata
                .iter()
                .map(|(name, ty)| (*name, ty.to_julia_name()))
                .collect::<Vec<_>>(),
            vec![
                ("mod", "Module".to_string()),
                ("name", "Symbol".to_string()),
                ("binding", "Core.Binding".to_string())
            ]
        );

        assert!(CoreType::from_julia_name("Int64")
            .builtin_field_metadata()
            .is_none());
    }

    #[test]
    fn ast_like_builtin_types_are_struct_datatypes() {
        for name in ["Expr", "QuoteNode", "LineNumberNode", "GlobalRef", "Module"] {
            let core = CoreType::from_julia_name(name);
            assert!(core.is_builtin_struct_datatype(), "{name}");
            assert!(core.is_builtin_concrete_datatype(), "{name}");
        }

        assert!(CoreType::from_julia_name("Expr").is_builtin_mutable_datatype());
        for name in ["QuoteNode", "LineNumberNode", "GlobalRef"] {
            assert!(
                !CoreType::from_julia_name(name).is_builtin_mutable_datatype(),
                "{name}"
            );
        }
    }

    #[test]
    fn typejoin_uses_existing_supertype_when_available() {
        let int64 = CoreType::Primitive(CorePrimitive::Int64);
        let real = CoreType::Abstract(CoreAbstract::Real);
        assert_eq!(int64.typejoin(&real), real);

        let joined = CoreType::Primitive(CorePrimitive::String)
            .typejoin(&CoreType::Primitive(CorePrimitive::Int64));
        assert!(matches!(joined, CoreType::Union(types) if types.len() == 2));
    }

    #[test]
    fn typejoin_handles_tuple_elements() {
        assert_eq!(
            CoreType::from_julia_name("Tuple{Int64, Float64}")
                .typejoin(&CoreType::from_julia_name("Tuple{Float64, Int64}")),
            CoreType::Tuple(vec![
                CoreType::Union(vec![
                    CoreType::Primitive(CorePrimitive::Int64),
                    CoreType::Primitive(CorePrimitive::Float64),
                ]),
                CoreType::Union(vec![
                    CoreType::Primitive(CorePrimitive::Float64),
                    CoreType::Primitive(CorePrimitive::Int64),
                ]),
            ])
        );
    }

    #[test]
    fn specificity_preserves_existing_ordering_shape() {
        assert!(
            CoreType::Primitive(CorePrimitive::Int64).specificity()
                > CoreType::Abstract(CoreAbstract::Integer).specificity()
        );
        assert!(
            CoreType::Abstract(CoreAbstract::Integer).specificity()
                > CoreType::Abstract(CoreAbstract::Real).specificity()
        );
        assert_eq!(
            CoreType::Tuple(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::Float64),
            ])
            .specificity(),
            10
        );
    }

    #[cfg(feature = "aot")]
    #[test]
    fn aot_static_type_bridge_reaches_same_numeric_core() {
        let core = CoreType::from(&crate::aot::types::StaticType::U128);
        assert_eq!(core.primitive_numeric(), Some(PrimitiveNumeric::UInt128));
    }
}
